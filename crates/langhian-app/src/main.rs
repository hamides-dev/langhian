#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;
mod ui;

use iced::futures::SinkExt;
use iced::keyboard::key::Named;
use iced::widget::{container, text_editor};
use iced::{Element, Point, Size, Subscription, Task, Theme, keyboard, stream, window};
use langhian_core::hotkey_logic::InputAction;
use langhian_core::vocab::{export_vocab_csv, export_vocab_json};
use langhian_core::{
    AppConfig, AppStorage, HistoryEntry, ModelCatalogEntry, ModelProfile, PromptPreset,
    ProviderError, ProviderKind, SecretStore, ShortcutCombo, ShortcutConfig, TranslateInput,
    TranslateRequest, TranslationMode, Translator, TranslatorClient, UpdateCheckResult,
    UpdaterClient, VocabEntry, VocabMatcher, find_vocab_match_index, history::push_bounded_history,
    infer_vision_capability, resolve_bidirectional_direction,
};
use platform::capture::{capture_active_monitor, crop_screen_to_png};
use platform::clipboard::{
    capture_foreground_handle, capture_selection, replace_with_text, restore_foreground_handle,
    set_clipboard_text,
};
use platform::tray::{TrayAction, TrayHandle, build_tray, load_app_icon_rgba};
use platform::updater_exec::launch_installer;
use platform::window_pos::{
    active_selection_anchor_phys, compute_hud_subtitle_position, compute_smart_modal_position,
};
use platform::{current_cursor_pos, refresh_shortcut_config, start_keyboard_listener};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use ui::dashboard::{
    DashboardMsg, DashboardState, DashboardTab, ModelEditorState, ModelPresetKind,
    PromptEditorState, view_dashboard,
};
use ui::hud::{HudState, view_hud};
use ui::modal::{ModalMsg, ModalState, view_modal};
use ui::snip_overlay::{SnipEvent, SnipState, view_snip_overlay};
use ui::theme::transparent_iced_theme_for;

#[derive(Debug, Clone)]
enum BridgeEvent {
    Input(InputAction),
    Tray(TrayAction),
}

static BRIDGE_RX: OnceLock<Mutex<Option<UnboundedReceiver<BridgeEvent>>>> = OnceLock::new();

#[cfg(target_os = "windows")]
static WAKE_EVENT_HANDLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "windows")]
fn acquire_windows_mutex_or_wake_existing() {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, SetEvent,
    };

    let mutex_name: Vec<u16> = "LanghianTranslationSingleInstanceMutex\0"
        .encode_utf16()
        .collect();
    let event_name: Vec<u16> = "Local\\LanghianWakeDashboardEvent\0"
        .encode_utf16()
        .collect();

    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        let err = GetLastError();
        if mutex.is_null() || err == ERROR_ALREADY_EXISTS {
            let start_minimized = std::env::args()
                .any(|a| a == "--minimized" || a == "--background" || a == "--background-only");
            if !start_minimized {
                let ev = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
                if !ev.is_null() {
                    let _ = SetEvent(ev);
                    let _ = CloseHandle(ev);
                }
            }
            if !mutex.is_null() {
                let _ = CloseHandle(mutex);
            }
            std::process::exit(0);
        }

        // Primary instance: create auto-reset wake event and leak mutex handle intentionally for process lifetime.
        let ev = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
        if !ev.is_null() {
            WAKE_EVENT_HANDLE.store(ev as usize, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

fn main() -> iced::Result {
    #[cfg(target_os = "windows")]
    acquire_windows_mutex_or_wake_existing();

    let mut daemon = iced::daemon(LanghianApp::new, LanghianApp::update, LanghianApp::view)
        .title(LanghianApp::title)
        .theme(LanghianApp::theme)
        .subscription(LanghianApp::subscription)
        .default_font(ui::theme::font_ui_default());

    for font_bytes in ui::theme::bundled_fonts() {
        daemon = daemon.font(font_bytes);
    }

    daemon.run()
}

struct LanghianApp {
    storage: AppStorage,
    secrets: SecretStore,
    config: AppConfig,
    vocab: Vec<VocabEntry>,
    vocab_matcher: VocabMatcher,
    history: Vec<HistoryEntry>,
    i18n: Translator,
    client: TranslatorClient,
    updater: UpdaterClient,
    shared_shortcuts: Arc<Mutex<ShortcutConfig>>,
    tray: Option<TrayHandle>,
    app_icon: Option<window::Icon>,

    // Window tracking
    dashboard_id: Option<window::Id>,
    dashboard_opening: bool,
    modal_id: Option<window::Id>,
    modal_opened_at: Option<Instant>,
    modal_has_focused: bool,
    modal_user_dragged: bool,
    snip_id: Option<window::Id>,
    hud_id: Option<window::Id>,

    // Window states
    dashboard: DashboardState,
    modal: Option<ModalState>,
    snip: Option<SnipState>,
    hud: Option<HudState>,
    hud_expires_at: Option<Instant>,
    pending_replace_prev_clip: Option<String>,
    pending_target_hwnd: isize,
    translation_seq: u64,
}

#[derive(Debug, Clone)]
enum Message {
    Bridge(BridgeEvent),
    WindowOpened(WindowKind, window::Id),
    WindowEventOccurred(window::Id, window::Event),
    GlobalEscPressed,
    HudTick,

    // Async workflow results
    InPlaceCaptured {
        target_hwnd: isize,
        result: Result<platform::clipboard::CapturedSelection, String>,
    },
    DoubleCtrlCaptured {
        target_hwnd: isize,
        anchor_phys: Option<(i32, i32, i32)>,
        cursor_phys: Option<(i32, i32)>,
        result: Result<platform::clipboard::CapturedSelection, String>,
    },
    ModalTranslationFinished {
        seq: u64,
        effective_target: String,
        was_auto_reversed: bool,
        result: Result<(String, Option<String>), TranslationFail>,
        mode: TranslationMode,
    },
    InPlaceFlowFinished(Result<(String, String, Option<String>), String>),
    ManualTranslateFinished {
        seq: u64,
        effective_target: String,
        was_auto_reversed: bool,
        result: Result<(String, Option<String>), TranslationFail>,
    },
    ProbeModelFinished(Result<(bool, String), String>),
    FetchModelsFinished(Result<Vec<ModelCatalogEntry>, String>),
    UpdateCheckFinished(Result<UpdateCheckResult, String>),
    UpdateDownloadFinished(Result<std::path::PathBuf, String>),

    // Sub-views
    Dashboard(DashboardMsg),
    Modal(ModalMsg),
    Snip(SnipEvent),
    ClearCopyBadge {
        target: CopyBadgeTarget,
        seq: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyBadgeTarget {
    ModalSource,
    ModalOutput,
    DashboardManual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowKind {
    Dashboard,
    Modal,
    Snip,
    Hud,
}

#[derive(Debug, Clone)]
struct TranslationFail {
    is_vision: bool,
    message: String,
}

impl LanghianApp {
    fn new() -> (Self, Task<Message>) {
        let storage = AppStorage::open_default();
        let secrets = SecretStore::new(storage.data_dir());
        let config = storage.load_config();
        let vocab = storage.load_vocab();
        let vocab_matcher = VocabMatcher::build(vocab.clone());
        let history = storage.load_history();
        let i18n = Translator::new(&config.ui_language, Some(storage.locales_dir()));
        let dashboard = DashboardState::from_config(&config);

        let shared_shortcuts = Arc::new(Mutex::new(config.shortcuts.clone()));
        let (bridge_tx, bridge_rx) = unbounded_channel::<BridgeEvent>();
        let _ = BRIDGE_RX.set(Mutex::new(Some(bridge_rx)));

        // Forward keyboard hook events into unified bridge
        let (in_tx, mut in_rx) = unbounded_channel::<InputAction>();
        let bridge_tx_in = bridge_tx.clone();
        std::thread::spawn(move || {
            while let Some(act) = in_rx.blocking_recv() {
                if bridge_tx_in.send(BridgeEvent::Input(act)).is_err() {
                    break;
                }
            }
        });
        start_keyboard_listener(Arc::clone(&shared_shortcuts), in_tx);

        // Forward tray events into unified bridge
        let (tray_tx, mut tray_rx) = unbounded_channel::<TrayAction>();
        let bridge_tx_tray = bridge_tx.clone();
        std::thread::spawn(move || {
            while let Some(act) = tray_rx.blocking_recv() {
                if bridge_tx_tray.send(BridgeEvent::Tray(act)).is_err() {
                    break;
                }
            }
        });
        let tray = build_tray(&config, &i18n, tray_tx).ok();

        #[cfg(target_os = "windows")]
        {
            let bridge_tx_wake = bridge_tx.clone();
            std::thread::spawn(move || {
                use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
                use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
                let raw = WAKE_EVENT_HANDLE.load(std::sync::atomic::Ordering::SeqCst);
                if raw != 0 {
                    let ev = raw as windows_sys::Win32::Foundation::HANDLE;
                    loop {
                        let res = unsafe { WaitForSingleObject(ev, INFINITE) };
                        if res == WAIT_OBJECT_0 {
                            if bridge_tx_wake
                                .send(BridgeEvent::Tray(TrayAction::OpenDashboard))
                                .is_err()
                            {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
            });
        }

        let app_icon =
            load_app_icon_rgba().and_then(|(rgba, w, h)| window::icon::from_rgba(rgba, w, h).ok());

        let start_minimized = std::env::args()
            .any(|a| a == "--minimized" || a == "--background" || a == "--background-only");

        let mut tasks: Vec<Task<Message>> = Vec::new();
        let mut dashboard_opening = false;

        if !start_minimized {
            dashboard_opening = true;
            tasks.push(open_dashboard_task(app_icon.clone()));
        }

        let mut app = Self {
            storage,
            secrets,
            config,
            vocab,
            vocab_matcher,
            history,
            i18n,
            client: TranslatorClient::new(),
            updater: UpdaterClient::new(),
            shared_shortcuts,
            tray,
            app_icon,
            dashboard_id: None,
            dashboard_opening,
            modal_id: None,
            modal_opened_at: None,
            modal_has_focused: false,
            modal_user_dragged: false,
            snip_id: None,
            hud_id: None,
            dashboard,
            modal: None,
            snip: None,
            hud: None,
            hud_expires_at: None,
            pending_replace_prev_clip: None,
            pending_target_hwnd: 0,
            translation_seq: 0,
        };

        if app.config.auto_check_updates {
            let updater = app.updater.clone();
            app.dashboard.update_checking = true;
            tasks.push(Task::perform(
                async move {
                    updater
                        .check_for_updates(env!("CARGO_PKG_VERSION"))
                        .await
                        .map_err(|e| e.to_string())
                },
                Message::UpdateCheckFinished,
            ));
        }

        (app, Task::batch(tasks))
    }

    fn title(&self, window: window::Id) -> String {
        if Some(window) == self.modal_id {
            "Langhian Quick Translate".to_string()
        } else if Some(window) == self.snip_id {
            "Langhian Screenshot Capture".to_string()
        } else if Some(window) == self.hud_id {
            "Langhian HUD".to_string()
        } else {
            format!(
                "{} — {}",
                self.i18n.t("app.title"),
                self.i18n.t("app.subtitle")
            )
        }
    }

    fn theme(&self, _window: window::Id) -> Theme {
        transparent_iced_theme_for(self.config.theme)
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            Subscription::run(bridge_stream).map(Message::Bridge),
            window::events().map(|(id, ev)| Message::WindowEventOccurred(id, ev)),
        ];

        if self.hud_expires_at.is_some() {
            subs.push(iced::time::every(Duration::from_millis(250)).map(|_| Message::HudTick));
        }

        if self.modal_id.is_some() || self.snip_id.is_some() {
            subs.push(keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(Named::Escape),
                    ..
                } => Some(Message::GlobalEscPressed),
                _ => None,
            }));
        }

        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Bridge(BridgeEvent::Input(act)) => self.handle_input_action(act),
            Message::Bridge(BridgeEvent::Tray(act)) => self.handle_tray_action(act),

            Message::WindowOpened(kind, id) => {
                match kind {
                    WindowKind::Dashboard => {
                        self.dashboard_opening = false;
                        if let Some(existing) = self.dashboard_id {
                            return Task::batch([window::close(id), window::gain_focus(existing)]);
                        }
                        self.dashboard_id = Some(id);
                    }
                    WindowKind::Modal => {
                        self.modal_id = Some(id);
                        self.modal_opened_at = Some(Instant::now());
                        self.modal_has_focused = false;
                        self.modal_user_dragged = false;
                        return window::gain_focus(id);
                    }
                    WindowKind::Snip => self.snip_id = Some(id),
                    WindowKind::Hud => self.hud_id = Some(id),
                }
                Task::none()
            }

            Message::WindowEventOccurred(id, event) => match event {
                window::Event::CloseRequested => self.handle_window_close(id),
                window::Event::Focused => {
                    if Some(id) == self.modal_id {
                        self.modal_has_focused = true;
                    }
                    Task::none()
                }
                window::Event::Unfocused => {
                    if Some(id) == self.modal_id {
                        let pinned = self.modal.as_ref().is_some_and(|m| m.pinned);
                        let past_startup_grace = self
                            .modal_opened_at
                            .map(|t| t.elapsed() >= Duration::from_millis(260))
                            .unwrap_or(true);
                        if !pinned && (self.modal_has_focused || past_startup_grace) {
                            return self.close_modal();
                        }
                    }
                    Task::none()
                }
                window::Event::Moved(pos) => {
                    if Some(id) == self.modal_id {
                        let settled = self.modal_user_dragged
                            || self
                                .modal_opened_at
                                .map(|t| t.elapsed() >= Duration::from_millis(350))
                                .unwrap_or(true);
                        if settled {
                            self.config.modal_position = Some([pos.x, pos.y]);
                        }
                    }
                    Task::none()
                }
                window::Event::Resized(size) => {
                    if Some(id) == self.modal_id {
                        let settled = self
                            .modal_opened_at
                            .map(|t| t.elapsed() >= Duration::from_millis(260))
                            .unwrap_or(true);
                        if settled {
                            self.config.modal_size = Some([
                                size.width.clamp(420.0, 1200.0),
                                size.height.clamp(320.0, 960.0),
                            ]);
                        }
                    }
                    Task::none()
                }
                _ => Task::none(),
            },

            Message::InPlaceCaptured {
                target_hwnd,
                result,
            } => {
                let captured = match result {
                    Ok(c) => c,
                    Err(_) => {
                        if self.config.show_hud {
                            return self.show_hud_toast(
                                self.i18n.t("app.status_error"),
                                Some(self.i18n.t("errors.empty_selection")),
                                true,
                            );
                        }
                        return Task::none();
                    }
                };

                // Double-Alt in-place mode always translates toward `target_language` (never reverses back).
                let dir = resolve_bidirectional_direction(
                    &captured.text,
                    &self.config.native_language,
                    &self.config.target_language,
                    false,
                    None,
                );
                let profile = self.config.active_model().clone();
                let api_key = self.secrets.get_api_key(&profile.id);
                let prompt = self.config.active_prompt().clone();
                let source_lang = dir.effective_source;
                let target_lang = dir.effective_target;
                let preserve_markdown = self.config.preserve_markdown;
                let restore_clip = self.config.restore_clipboard;
                let client = self.client.clone();
                let vision_msg = self.i18n.t("errors.vision_unsupported");
                let key_msg = self.i18n.t("errors.missing_api_key");
                let rate_msg = self.i18n.t("errors.rate_limited");

                let hud_task = if self.config.show_hud {
                    self.show_hud_toast(self.i18n.t("hud.translating_inplace"), None, false)
                } else {
                    Task::none()
                };

                let work_task = Task::perform(
                    async move {
                        let req = TranslateRequest {
                            input: TranslateInput::Text(captured.text.clone()),
                            source_lang,
                            target_lang,
                            prompt,
                            preserve_markdown,
                        };
                        let resp = client
                            .translate(&profile, api_key.as_deref(), &req)
                            .await
                            .map_err(|e| {
                                map_provider_error(e, &vision_msg, &key_msg, &rate_msg).message
                            })?;

                        let translated = resp.translated_text.clone();
                        let prev = captured.previous_clipboard.clone();
                        let tr_paste = translated.clone();
                        tokio::task::spawn_blocking(move || {
                            restore_foreground_handle(target_hwnd);
                            replace_with_text(&tr_paste, restore_clip, prev)
                        })
                        .await
                        .map_err(|e| e.to_string())??;

                        Ok((captured.text, translated, captured.previous_clipboard))
                    },
                    Message::InPlaceFlowFinished,
                );

                Task::batch([hud_task, work_task])
            }

            Message::DoubleCtrlCaptured {
                target_hwnd,
                anchor_phys,
                cursor_phys,
                result,
            } => {
                let captured = match result {
                    Ok(c) => c,
                    Err(_) => {
                        if self.config.show_hud {
                            return self.show_hud_toast(
                                self.i18n.t("app.status_error"),
                                Some(self.i18n.t("errors.empty_selection")),
                                true,
                            );
                        }
                        return Task::none();
                    }
                };
                self.pending_target_hwnd = target_hwnd;
                self.pending_replace_prev_clip = captured.previous_clipboard;
                let modal = ModalState::for_text(
                    captured.text,
                    self.config.modal_pinned,
                    &self.config.native_language,
                );
                self.open_or_update_modal(modal, TranslationMode::Modal, anchor_phys, cursor_phys)
            }

            Message::GlobalEscPressed => {
                if let Some(id) = self.snip_id.take() {
                    self.snip = None;
                    return window::close(id);
                }
                if self.modal_id.is_some() {
                    return self.close_modal();
                }
                Task::none()
            }

            Message::HudTick => {
                if let Some(exp) = self.hud_expires_at
                    && Instant::now() >= exp
                {
                    self.hud_expires_at = None;
                    self.hud = None;
                    if let Some(id) = self.hud_id.take() {
                        return window::close(id);
                    }
                }
                Task::none()
            }

            Message::ModalTranslationFinished {
                seq,
                effective_target,
                was_auto_reversed,
                result,
                mode,
            } => {
                if seq != self.translation_seq {
                    return Task::none();
                }
                let Some(modal) = self.modal.as_mut() else {
                    return Task::none();
                };
                modal.is_loading = false;
                modal.effective_target_lang = effective_target.clone();
                modal.was_auto_reversed = was_auto_reversed;
                match result {
                    Ok((translated, detected)) => {
                        modal.translated_text = translated.clone();
                        modal.output_editor = text_editor::Content::with_text(&translated);
                        modal.output_force_selectable = false;
                        modal.detected_source_lang = detected;
                        modal.error_message = None;
                        modal.is_vision_error = false;
                        modal.copied_output = false;
                        let src_record = if modal.screenshot_png.is_some() {
                            "[Screenshot]".to_string()
                        } else {
                            modal.source_text.clone()
                        };
                        self.record_history(mode, src_record, translated, effective_target);
                    }
                    Err(fail) => {
                        modal.error_message = Some(fail.message);
                        modal.is_vision_error = fail.is_vision;
                    }
                }
                Task::none()
            }

            Message::InPlaceFlowFinished(res) => match res {
                Ok((source, translated, _)) => {
                    let target = self.config.target_language.clone();
                    self.record_history(TranslationMode::InPlace, source, translated, target);
                    if self.config.show_hud {
                        self.show_hud_toast(self.i18n.t("hud.replaced_success"), None, false)
                    } else {
                        Task::none()
                    }
                }
                Err(err) => {
                    if self.config.show_hud {
                        self.show_hud_toast(self.i18n.t("app.status_error"), Some(err), true)
                    } else {
                        Task::none()
                    }
                }
            },

            Message::ManualTranslateFinished {
                seq,
                effective_target,
                was_auto_reversed,
                result,
            } => {
                if seq != self.translation_seq {
                    return Task::none();
                }
                self.dashboard.is_translating = false;
                self.dashboard.manual_auto_reversed = was_auto_reversed;
                self.dashboard.manual_effective_target = Some(effective_target.clone());
                match result {
                    Ok((translated, _)) => {
                        self.dashboard.manual_output = translated.clone();
                        self.dashboard.translate_error = None;
                        self.dashboard.save_term = self.dashboard.source_editor.text();
                        self.dashboard.save_translation = translated.clone();
                        self.record_history(
                            TranslationMode::Manual,
                            self.dashboard.source_editor.text(),
                            translated,
                            effective_target,
                        );
                    }
                    Err(fail) => {
                        self.dashboard.translate_error = Some(fail.message);
                    }
                }
                Task::none()
            }

            Message::ProbeModelFinished(res) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.is_probing = false;
                    match res {
                        Ok((vision, msg)) => {
                            ed.supports_vision = vision;
                            ed.vision_source = langhian_core::CapabilitySource::AutoDetected;
                            ed.probe_status = Some((true, msg));
                        }
                        Err(e) => {
                            ed.probe_status = Some((false, e));
                        }
                    }
                }
                Task::none()
            }

            Message::FetchModelsFinished(res) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.is_probing = false;
                    match res {
                        Ok(list) => {
                            ed.probe_status =
                                Some((true, format!("Fetched {} models", list.len())));
                            ed.fetched_catalog = list;
                            ed.selected_catalog_idx = None;
                        }
                        Err(e) => {
                            ed.probe_status = Some((false, e));
                        }
                    }
                }
                Task::none()
            }

            Message::UpdateCheckFinished(res) => {
                self.dashboard.update_checking = false;
                match res {
                    Ok(r) => {
                        self.dashboard.update_error = None;
                        self.dashboard.update_status = Some(r);
                    }
                    Err(e) => {
                        self.dashboard.update_error = Some(e);
                    }
                }
                Task::none()
            }

            Message::UpdateDownloadFinished(res) => {
                self.dashboard.update_downloading = false;
                match res {
                    Ok(path) => match launch_installer(&path) {
                        Ok(should_exit) => {
                            if should_exit {
                                return iced::exit();
                            }
                            self.dashboard.settings_toast = Some(self.i18n.t("updates.installing"));
                        }
                        Err(e) => self.dashboard.update_error = Some(e),
                    },
                    Err(e) => self.dashboard.update_error = Some(e),
                }
                Task::none()
            }

            Message::Dashboard(msg) => self.update_dashboard(msg),
            Message::Modal(msg) => self.update_modal(msg),
            Message::Snip(ev) => self.update_snip(ev),
            Message::ClearCopyBadge { target, seq } => {
                match target {
                    CopyBadgeTarget::ModalSource => {
                        if let Some(m) = self.modal.as_mut()
                            && m.copied_source_seq == seq
                        {
                            m.copied_source = false;
                        }
                    }
                    CopyBadgeTarget::ModalOutput => {
                        if let Some(m) = self.modal.as_mut()
                            && m.copied_output_seq == seq
                        {
                            m.copied_output = false;
                        }
                    }
                    CopyBadgeTarget::DashboardManual => {
                        if self.dashboard.copied_manual_seq == seq {
                            self.dashboard.copied_manual = false;
                        }
                    }
                }
                Task::none()
            }
        }
    }

    fn view(&self, window: window::Id) -> Element<'_, Message> {
        if Some(window) == self.modal_id
            && let Some(modal) = &self.modal
        {
            return view_modal(
                modal,
                &self.config,
                &self.i18n,
                &self.vocab,
                &self.vocab_matcher,
            )
            .map(Message::Modal);
        }
        if Some(window) == self.snip_id
            && let Some(snip) = &self.snip
        {
            return view_snip_overlay(
                snip,
                format!("{} · Esc", self.i18n.t("translate.screenshot_btn")),
                Message::Snip,
            );
        }
        if Some(window) == self.hud_id
            && let Some(hud) = &self.hud
        {
            return view_hud(hud, self.config.theme, &self.i18n);
        }

        let content: Element<'_, DashboardMsg> = container(view_dashboard(
            &self.dashboard,
            &self.config,
            &self.vocab,
            &self.history,
            &self.i18n,
            &self.vocab_matcher,
        ))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into();
        content.map(Message::Dashboard)
    }

    fn handle_window_close(&mut self, id: window::Id) -> Task<Message> {
        if Some(id) == self.modal_id {
            return self.close_modal();
        }
        if Some(id) == self.snip_id {
            self.snip_id = None;
            self.snip = None;
            return window::close(id);
        }
        if Some(id) == self.hud_id {
            self.hud_id = None;
            self.hud = None;
            self.hud_expires_at = None;
            return window::close(id);
        }
        if Some(id) == self.dashboard_id {
            self.dashboard_id = None;
            self.dashboard_opening = false;
            self.dashboard.is_maximized = false;
            return window::close(id);
        }
        window::close(id)
    }

    fn close_modal(&mut self) -> Task<Message> {
        self.modal = None;
        self.modal_opened_at = None;
        self.modal_has_focused = false;
        self.modal_user_dragged = false;
        let _ = self.storage.save_config(&self.config);
        if let Some(id) = self.modal_id.take() {
            window::close(id)
        } else {
            Task::none()
        }
    }

    fn handle_input_action(&mut self, action: InputAction) -> Task<Message> {
        match action {
            InputAction::DoubleTapAlt => self.run_double_alt_inplace(),
            InputAction::DoubleTapCtrl => self.run_double_ctrl_modal(),
            InputAction::ScreenshotTranslate => self.start_screenshot_capture(),
            InputAction::NextPrompt => self.cycle_prompt(1),
            InputAction::PrevPrompt => self.cycle_prompt(-1),
            InputAction::OpenDashboard => self.ensure_dashboard_open(),
            InputAction::SwitchPromptSlot(slot) => self.activate_prompt_slot(slot),
        }
    }

    fn handle_tray_action(&mut self, action: TrayAction) -> Task<Message> {
        match action {
            TrayAction::OpenDashboard => self.ensure_dashboard_open(),
            TrayAction::QuickTranslate => {
                self.dashboard.active_tab = DashboardTab::Translate;
                self.ensure_dashboard_open()
            }
            TrayAction::ScreenshotTranslate => self.start_screenshot_capture(),
            TrayAction::SelectModel(id) => {
                self.config.active_model_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            TrayAction::SelectPrompt(id) => {
                self.config.active_prompt_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                let pname = self.config.active_prompt().name.clone();
                if self.config.show_hud {
                    self.show_hud_toast(self.i18n.t("hud.prompt_switched"), Some(pname), false)
                } else {
                    Task::none()
                }
            }
            TrayAction::Quit => {
                let _ = self.storage.save_config(&self.config);
                iced::exit()
            }
        }
    }

    fn ensure_dashboard_open(&mut self) -> Task<Message> {
        if let Some(id) = self.dashboard_id {
            window::gain_focus(id)
        } else if self.dashboard_opening {
            Task::none()
        } else {
            self.dashboard_opening = true;
            open_dashboard_task(self.app_icon.clone())
        }
    }

    fn run_double_alt_inplace(&mut self) -> Task<Message> {
        let target_hwnd = capture_foreground_handle();
        let auto_select = self.config.shortcuts.auto_select_all_fallback;
        let prefer_markdown = self.config.preserve_markdown;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    capture_selection(false, auto_select, prefer_markdown)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |result| Message::InPlaceCaptured {
                target_hwnd,
                result,
            },
        )
    }

    fn run_double_ctrl_modal(&mut self) -> Task<Message> {
        let target_hwnd = capture_foreground_handle();
        let cursor_phys = current_cursor_pos();
        let anchor_phys = active_selection_anchor_phys(target_hwnd, cursor_phys);
        let restore_clip = self.config.restore_clipboard;
        let prefer_markdown = self.config.preserve_markdown;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    capture_selection(restore_clip, false, prefer_markdown)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |result| Message::DoubleCtrlCaptured {
                target_hwnd,
                anchor_phys,
                cursor_phys,
                result,
            },
        )
    }

    fn start_screenshot_capture(&mut self) -> Task<Message> {
        let cursor = current_cursor_pos();
        let screen = match capture_active_monitor(cursor) {
            Ok(s) => s,
            Err(e) => {
                return self.show_hud_toast(self.i18n.t("app.status_error"), Some(e), true);
            }
        };

        let pos = Point::new(screen.monitor_x as f32, screen.monitor_y as f32);
        let size = Size::new(screen.logical_width, screen.logical_height);
        self.snip = Some(SnipState::new(screen));

        if let Some(existing) = self.snip_id {
            return window::gain_focus(existing);
        }

        let (_, open) = window::open(window::Settings {
            size,
            position: window::Position::Specific(pos),
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            resizable: false,
            exit_on_close_request: false,
            platform_specific: floating_platform_specific(),
            ..Default::default()
        });
        open.map(|id| Message::WindowOpened(WindowKind::Snip, id))
    }

    fn open_or_update_modal(
        &mut self,
        modal: ModalState,
        mode: TranslationMode,
        anchor_phys: Option<(i32, i32, i32)>,
        cursor_phys: Option<(i32, i32)>,
    ) -> Task<Message> {
        self.modal = Some(modal);
        let translate_task = self.dispatch_modal_translation(mode);

        let [modal_w, modal_h] = self.config.modal_size.unwrap_or([540.0, 460.0]);
        let modal_w = modal_w.clamp(420.0, 1200.0);
        let modal_h = modal_h.clamp(320.0, 960.0);
        let pos_pt = compute_smart_modal_position(
            self.config.modal_position,
            anchor_phys,
            cursor_phys,
            modal_w,
            modal_h,
        );

        if let Some(id) = self.modal_id {
            return Task::batch([
                window::move_to(id, pos_pt),
                window::gain_focus(id),
                translate_task,
            ]);
        }

        let (_, open) = window::open(window::Settings {
            size: Size::new(modal_w, modal_h),
            min_size: Some(Size::new(420.0, 320.0)),
            position: window::Position::Specific(pos_pt),
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            icon: self.app_icon.clone(),
            exit_on_close_request: false,
            platform_specific: floating_platform_specific(),
            ..Default::default()
        });

        Task::batch([
            open.map(|id| Message::WindowOpened(WindowKind::Modal, id)),
            translate_task,
        ])
    }

    fn dispatch_modal_translation(&mut self, mode: TranslationMode) -> Task<Message> {
        let vision_msg = self.i18n.t("errors.vision_unsupported");
        let key_msg = self.i18n.t("errors.missing_api_key");
        let rate_msg = self.i18n.t("errors.rate_limited");
        let Some(modal) = self.modal.as_mut() else {
            return Task::none();
        };

        self.translation_seq = self.translation_seq.wrapping_add(1);
        let seq = self.translation_seq;

        let (input, effective_source, effective_target, was_auto_reversed) =
            if let Some(png) = &modal.screenshot_png {
                (
                    TranslateInput::ImagePng {
                        bytes: png.clone(),
                        context_hint: None,
                    },
                    "auto".to_string(),
                    self.config.native_language.clone(),
                    false,
                )
            } else {
                let res = resolve_bidirectional_direction(
                    &modal.source_text,
                    &self.config.native_language,
                    &self.config.target_language,
                    true,
                    None,
                );
                (
                    TranslateInput::Text(modal.source_text.clone()),
                    res.effective_source,
                    res.effective_target,
                    res.was_reversed,
                )
            };

        modal.effective_target_lang = effective_target.clone();
        modal.was_auto_reversed = was_auto_reversed;

        let profile = self.config.active_model().clone();
        if matches!(input, TranslateInput::ImagePng { .. }) && !profile.supports_vision {
            modal.is_loading = false;
            modal.is_vision_error = true;
            modal.error_message = Some(vision_msg);
            return Task::none();
        }

        modal.is_loading = true;
        modal.error_message = None;
        modal.is_vision_error = false;
        modal.copied_output = false;

        let api_key = self.secrets.get_api_key(&profile.id);
        let req = TranslateRequest {
            input,
            source_lang: effective_source,
            target_lang: effective_target.clone(),
            prompt: self.config.active_prompt().clone(),
            preserve_markdown: self.config.preserve_markdown,
        };
        let client = self.client.clone();

        Task::perform(
            async move {
                match client.translate(&profile, api_key.as_deref(), &req).await {
                    Ok(r) => Ok((r.translated_text, r.detected_source_lang)),
                    Err(e) => Err(map_provider_error(e, &vision_msg, &key_msg, &rate_msg)),
                }
            },
            move |result| Message::ModalTranslationFinished {
                seq,
                effective_target,
                was_auto_reversed,
                result,
                mode,
            },
        )
    }

    fn cycle_prompt(&mut self, delta: isize) -> Task<Message> {
        if self.config.prompts.is_empty() {
            return Task::none();
        }
        let len = self.config.prompts.len() as isize;
        let cur = self
            .config
            .prompts
            .iter()
            .position(|p| p.id == self.config.active_prompt_id)
            .unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.config.active_prompt_id = self.config.prompts[next].id.clone();
        let _ = self.storage.save_config(&self.config);
        self.rebuild_tray();

        let name = self.config.active_prompt().name.clone();
        if self.config.show_hud {
            self.show_hud_toast(self.i18n.t("hud.prompt_switched"), Some(name), false)
        } else {
            Task::none()
        }
    }

    fn activate_prompt_slot(&mut self, slot: u8) -> Task<Message> {
        let Some(found) = self
            .config
            .prompts
            .iter()
            .find(|p| p.shortcut_slot == Some(slot))
            .cloned()
        else {
            return Task::none();
        };

        self.config.active_prompt_id = found.id.clone();
        let _ = self.storage.save_config(&self.config);
        self.rebuild_tray();

        if self.config.show_hud {
            self.show_hud_toast(
                self.i18n.t("hud.prompt_switched"),
                Some(format!("[{slot}] {}", found.name)),
                false,
            )
        } else {
            Task::none()
        }
    }

    fn show_hud_toast(
        &mut self,
        title: String,
        detail: Option<String>,
        is_error: bool,
    ) -> Task<Message> {
        self.hud = Some(HudState {
            title,
            detail,
            is_error,
        });
        self.hud_expires_at = Some(Instant::now() + Duration::from_millis(1900));

        let pos_pt = compute_hud_subtitle_position(current_cursor_pos(), 460.0, 56.0);
        if let Some(id) = self.hud_id {
            // Do NOT steal focus from the user's active app when updating HUD text.
            return window::move_to(id, pos_pt);
        }

        let (_, open) = window::open(window::Settings {
            size: Size::new(460.0, 56.0),
            position: window::Position::Specific(pos_pt),
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            resizable: false,
            exit_on_close_request: false,
            platform_specific: floating_platform_specific(),
            ..Default::default()
        });
        open.map(|id| Message::WindowOpened(WindowKind::Hud, id))
    }

    fn update_snip(&mut self, ev: SnipEvent) -> Task<Message> {
        match ev {
            SnipEvent::DragStart(pt) => {
                if let Some(s) = self.snip.as_mut() {
                    s.drag_start = Some(pt);
                    s.drag_current = Some(pt);
                }
                Task::none()
            }
            SnipEvent::DragMove(pt) => {
                if let Some(s) = self.snip.as_mut()
                    && s.drag_start.is_some()
                {
                    s.drag_current = Some(pt);
                }
                Task::none()
            }
            SnipEvent::Cancel => {
                self.snip = None;
                if let Some(id) = self.snip_id.take() {
                    window::close(id)
                } else {
                    Task::none()
                }
            }
            SnipEvent::DragEnd(bounds_size) => {
                let Some(snip) = self.snip.take() else {
                    return Task::none();
                };
                let close_task = if let Some(id) = self.snip_id.take() {
                    window::close(id)
                } else {
                    Task::none()
                };

                let Some(rect) = snip.selection_rect() else {
                    return close_task;
                };

                match crop_screen_to_png(&snip.screen, bounds_size.width, bounds_size.height, rect)
                {
                    Ok(png) => {
                        let cursor_phys = current_cursor_pos();
                        let modal = ModalState::for_screenshot(
                            png,
                            self.config.modal_pinned,
                            &self.config.native_language,
                        );
                        let modal_task = self.open_or_update_modal(
                            modal,
                            TranslationMode::Screenshot,
                            None,
                            cursor_phys,
                        );
                        Task::batch([close_task, modal_task])
                    }
                    Err(err) => {
                        let hud_task =
                            self.show_hud_toast(self.i18n.t("app.status_error"), Some(err), true);
                        Task::batch([close_task, hud_task])
                    }
                }
            }
        }
    }

    fn update_modal(&mut self, msg: ModalMsg) -> Task<Message> {
        let current_mode = if self
            .modal
            .as_ref()
            .is_some_and(|m| m.screenshot_png.is_some())
        {
            TranslationMode::Screenshot
        } else {
            TranslationMode::Modal
        };

        match msg {
            ModalMsg::DragWindow => {
                self.modal_user_dragged = true;
                if let Some(id) = self.modal_id {
                    window::drag(id)
                } else {
                    Task::none()
                }
            }
            ModalMsg::ResizeWindow(dir) => {
                if let Some(id) = self.modal_id {
                    window::drag_resize(id, dir)
                } else {
                    Task::none()
                }
            }
            ModalMsg::CloseHovered(hovered) => {
                if let Some(m) = self.modal.as_mut() {
                    m.close_hovered = hovered;
                }
                Task::none()
            }
            ModalMsg::Close => self.close_modal(),
            ModalMsg::TogglePin => {
                if let Some(m) = self.modal.as_mut() {
                    m.pinned = !m.pinned;
                    self.config.modal_pinned = m.pinned;
                    let _ = self.storage.save_config(&self.config);
                }
                Task::none()
            }
            ModalMsg::EditSourceAction(action) => {
                if let Some(m) = self.modal.as_mut() {
                    m.source_editor.perform(action);
                    m.source_text = m.source_editor.text();
                    m.copied_source = false;
                }
                Task::none()
            }
            ModalMsg::EditOutputAction(action) => {
                if let Some(m) = self.modal.as_mut()
                    && !action.is_edit()
                {
                    m.output_editor.perform(action);
                }
                Task::none()
            }
            ModalMsg::ToggleOutputSelectable => {
                if let Some(m) = self.modal.as_mut() {
                    m.output_force_selectable = !m.output_force_selectable;
                }
                Task::none()
            }
            ModalMsg::TranslateOrRegenerate => self.dispatch_modal_translation(current_mode),
            ModalMsg::CopySource => {
                if let Some(m) = self.modal.as_mut()
                    && !m.source_text.trim().is_empty()
                    && set_clipboard_text(&m.source_text).is_ok()
                {
                    m.copied_source_seq = m.copied_source_seq.wrapping_add(1);
                    m.copied_source = true;
                    return schedule_copy_reset(CopyBadgeTarget::ModalSource, m.copied_source_seq);
                }
                Task::none()
            }
            ModalMsg::CopyOutput => {
                if let Some(m) = self.modal.as_mut()
                    && !m.translated_text.trim().is_empty()
                    && set_clipboard_text(&m.translated_text).is_ok()
                {
                    m.copied_output_seq = m.copied_output_seq.wrapping_add(1);
                    m.copied_output = true;
                    return schedule_copy_reset(CopyBadgeTarget::ModalOutput, m.copied_output_seq);
                }
                Task::none()
            }
            ModalMsg::ToggleStarVocab => {
                if let Some((term, translation, src_lang, target_lang)) =
                    self.modal.as_ref().map(|m| {
                        (
                            m.source_text.clone(),
                            m.translated_text.clone(),
                            m.detected_source_lang
                                .clone()
                                .unwrap_or_else(|| "auto".to_string()),
                            m.effective_target_lang.clone(),
                        )
                    })
                {
                    self.toggle_star_vocab(term, translation, src_lang, target_lang);
                }
                Task::none()
            }
            ModalMsg::SwitchToModel(id) => {
                self.config.active_model_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                self.dispatch_modal_translation(current_mode)
            }
            ModalMsg::ClickVocabHighlight(entry) => {
                if let Some(m) = self.modal.as_mut() {
                    m.selected_vocab_preview = Some(entry);
                }
                Task::none()
            }
            ModalMsg::DismissVocabPreview => {
                if let Some(m) = self.modal.as_mut() {
                    m.selected_vocab_preview = None;
                }
                Task::none()
            }
            ModalMsg::OpenModelsSettings => {
                self.dashboard.active_tab = DashboardTab::Models;
                self.ensure_dashboard_open()
            }
        }
    }

    fn update_dashboard(&mut self, msg: DashboardMsg) -> Task<Message> {
        match msg {
            // Window chrome (custom frameless bar & resize handles)
            DashboardMsg::DragTitlebar => {
                if let Some(id) = self.dashboard_id {
                    window::drag(id)
                } else {
                    Task::none()
                }
            }
            DashboardMsg::MinimizeWindow => {
                if let Some(id) = self.dashboard_id {
                    window::minimize(id, true)
                } else {
                    Task::none()
                }
            }
            DashboardMsg::ToggleMaximize => {
                if let Some(id) = self.dashboard_id {
                    self.dashboard.is_maximized = !self.dashboard.is_maximized;
                    window::maximize(id, self.dashboard.is_maximized)
                } else {
                    Task::none()
                }
            }
            DashboardMsg::TitlebarCloseHovered(hovered) => {
                self.dashboard.titlebar_close_hovered = hovered;
                Task::none()
            }
            DashboardMsg::CloseWindow => {
                if let Some(id) = self.dashboard_id {
                    self.handle_window_close(id)
                } else {
                    Task::none()
                }
            }
            DashboardMsg::ResizeWindow(dir) => {
                if let Some(id) = self.dashboard_id {
                    window::drag_resize(id, dir)
                } else {
                    Task::none()
                }
            }

            // First-run onboarding wizard
            DashboardMsg::OnboardingStep(step) => {
                self.dashboard.onboarding.step = step.min(2);
                Task::none()
            }
            DashboardMsg::OnboardingFinish(open_models) => {
                self.config.onboarding_completed = true;
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                self.dashboard.show_onboarding = false;
                if open_models {
                    self.dashboard.active_tab = DashboardTab::Models;
                }
                Task::none()
            }
            DashboardMsg::OnboardingSkip => {
                self.config.onboarding_completed = true;
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                self.dashboard.show_onboarding = false;
                Task::none()
            }
            DashboardMsg::OpenOnboarding => {
                self.dashboard.onboarding.step = 0;
                self.dashboard.show_onboarding = true;
                Task::none()
            }

            DashboardMsg::SwitchTab(tab) => {
                self.dashboard.active_tab = tab;
                Task::none()
            }
            DashboardMsg::SwitchSettingsSubTab(sub) => {
                self.dashboard.settings_subtab = sub;
                Task::none()
            }

            // Manual Translate tab
            DashboardMsg::SourceEditorAction(action) => {
                self.dashboard.source_editor.perform(action);
                Task::none()
            }
            DashboardMsg::SelectSourceLang(code) => {
                self.config.source_language = code;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::SelectNativeLang(code) => {
                self.config.native_language = code;
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::SelectTargetLang(code) => {
                self.config.target_language = code;
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::SwapLangs => {
                if self.config.source_language != "auto" {
                    std::mem::swap(
                        &mut self.config.source_language,
                        &mut self.config.target_language,
                    );
                } else if !self
                    .config
                    .native_language
                    .eq_ignore_ascii_case(&self.config.target_language)
                {
                    std::mem::swap(
                        &mut self.config.native_language,
                        &mut self.config.target_language,
                    );
                }
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                if !self.dashboard.manual_output.is_empty() {
                    let prev_out = self.dashboard.manual_output.clone();
                    self.dashboard.source_editor = text_editor::Content::with_text(&prev_out);
                    self.dashboard.manual_output.clear();
                }
                Task::none()
            }
            DashboardMsg::SelectModel(id) => {
                self.config.active_model_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::SelectPrompt(id) => {
                self.config.active_prompt_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::RunManualTranslate => {
                let text_val = self.dashboard.source_editor.text();
                if text_val.trim().is_empty() {
                    return Task::none();
                }
                self.translation_seq = self.translation_seq.wrapping_add(1);
                let seq = self.translation_seq;
                self.dashboard.is_translating = true;
                self.dashboard.translate_error = None;
                self.dashboard.copied_manual = false;

                let (effective_source, effective_target, was_auto_reversed) =
                    if self.config.source_language.eq_ignore_ascii_case("auto") {
                        let res = resolve_bidirectional_direction(
                            &text_val,
                            &self.config.native_language,
                            &self.config.target_language,
                            true,
                            None,
                        );
                        (res.effective_source, res.effective_target, res.was_reversed)
                    } else {
                        (
                            self.config.source_language.clone(),
                            self.config.target_language.clone(),
                            false,
                        )
                    };

                self.dashboard.manual_auto_reversed = was_auto_reversed;
                self.dashboard.manual_effective_target = Some(effective_target.clone());

                let profile = self.config.active_model().clone();
                let api_key = self.secrets.get_api_key(&profile.id);
                let req = TranslateRequest {
                    input: TranslateInput::Text(text_val),
                    source_lang: effective_source,
                    target_lang: effective_target.clone(),
                    prompt: self.config.active_prompt().clone(),
                    preserve_markdown: self.config.preserve_markdown,
                };
                let client = self.client.clone();
                let vision_msg = self.i18n.t("errors.vision_unsupported");
                let key_msg = self.i18n.t("errors.missing_api_key");
                let rate_msg = self.i18n.t("errors.rate_limited");

                Task::perform(
                    async move {
                        match client.translate(&profile, api_key.as_deref(), &req).await {
                            Ok(r) => Ok((r.translated_text, r.detected_source_lang)),
                            Err(e) => Err(map_provider_error(e, &vision_msg, &key_msg, &rate_msg)),
                        }
                    },
                    move |result| Message::ManualTranslateFinished {
                        seq,
                        effective_target,
                        was_auto_reversed,
                        result,
                    },
                )
            }
            DashboardMsg::TriggerScreenshot => self.start_screenshot_capture(),
            DashboardMsg::ClearManual => {
                self.dashboard.source_editor = text_editor::Content::new();
                self.dashboard.manual_output.clear();
                self.dashboard.translate_error = None;
                self.dashboard.manual_auto_reversed = false;
                self.dashboard.manual_effective_target = None;
                self.dashboard.copied_manual = false;
                Task::none()
            }
            DashboardMsg::CopyManualResult => {
                if !self.dashboard.manual_output.is_empty()
                    && set_clipboard_text(&self.dashboard.manual_output).is_ok()
                {
                    self.dashboard.copied_manual_seq =
                        self.dashboard.copied_manual_seq.wrapping_add(1);
                    self.dashboard.copied_manual = true;
                    return schedule_copy_reset(
                        CopyBadgeTarget::DashboardManual,
                        self.dashboard.copied_manual_seq,
                    );
                }
                Task::none()
            }
            DashboardMsg::ToggleStarManualVocab => {
                let term = self.dashboard.source_editor.text();
                let tr = self.dashboard.manual_output.clone();
                let tgt = self
                    .dashboard
                    .manual_effective_target
                    .clone()
                    .unwrap_or_else(|| self.config.target_language.clone());
                self.toggle_star_vocab(term, tr, self.config.source_language.clone(), tgt);
                Task::none()
            }
            DashboardMsg::ToggleSaveManualBox => {
                self.dashboard.show_save_vocab_box = !self.dashboard.show_save_vocab_box;
                if self.dashboard.show_save_vocab_box {
                    if self.dashboard.save_term.trim().is_empty() {
                        self.dashboard.save_term = self.dashboard.source_editor.text();
                    }
                    if self.dashboard.save_translation.trim().is_empty() {
                        self.dashboard.save_translation = self.dashboard.manual_output.clone();
                    }
                }
                Task::none()
            }
            DashboardMsg::SaveManualTermChanged(v) => {
                self.dashboard.save_term = v;
                Task::none()
            }
            DashboardMsg::SaveManualTranslationChanged(v) => {
                self.dashboard.save_translation = v;
                Task::none()
            }
            DashboardMsg::SaveManualNotesAction(action) => {
                self.dashboard.save_notes_editor.perform(action);
                Task::none()
            }
            DashboardMsg::ConfirmSaveManualVocab => {
                let term = self.dashboard.save_term.trim().to_string();
                let tr = self.dashboard.save_translation.trim().to_string();
                let notes = self.dashboard.save_notes_editor.text().trim().to_string();
                let tgt = self
                    .dashboard
                    .manual_effective_target
                    .clone()
                    .unwrap_or_else(|| self.config.target_language.clone());
                if !term.is_empty() && !tr.is_empty() {
                    self.add_vocab_entry_with_langs(
                        term,
                        tr,
                        notes,
                        self.config.source_language.clone(),
                        tgt,
                    );
                    self.dashboard.show_save_vocab_box = false;
                    self.dashboard.save_notes_editor = text_editor::Content::new();
                }
                Task::none()
            }
            DashboardMsg::ClickVocabHighlight(entry) => {
                self.dashboard.selected_vocab_preview = Some(entry);
                Task::none()
            }
            DashboardMsg::DismissVocabPreview => {
                self.dashboard.selected_vocab_preview = None;
                Task::none()
            }

            // Models Tab
            DashboardMsg::StartAddPreset(kind) => {
                self.dashboard.selected_preset_kind = Some(kind);
                let profile = match kind {
                    ModelPresetKind::GoogleFree => ModelProfile::google_translate_free(),
                    ModelPresetKind::OpenRouter => ModelProfile::preset_openrouter(),
                    ModelPresetKind::OpenAi => ModelProfile::preset_openai(),
                    ModelPresetKind::Anthropic => ModelProfile::preset_anthropic(),
                    ModelPresetKind::Gemini => ModelProfile::preset_gemini(),
                    ModelPresetKind::Ollama => ModelProfile::preset_ollama(),
                    ModelPresetKind::LmStudio => ModelProfile::preset_lmstudio(),
                    ModelPresetKind::Custom => {
                        let mut p = ModelProfile::preset_openai();
                        p.name = "Custom Endpoint".to_string();
                        p.base_url = "https://api.example.com/v1".to_string();
                        p.model_id = "model-name".to_string();
                        p
                    }
                };
                self.dashboard.model_editor =
                    Some(ModelEditorState::from_profile(&profile, None, true));
                Task::none()
            }
            DashboardMsg::StartEditModel(id) => {
                self.dashboard.selected_preset_kind = None;
                if let Some(m) = self.config.models.iter().find(|m| m.id == id) {
                    let key = self.secrets.get_api_key(&m.id);
                    self.dashboard.model_editor =
                        Some(ModelEditorState::from_profile(m, key, false));
                }
                Task::none()
            }
            DashboardMsg::DuplicateModel(id) => {
                if let Some(m) = self.config.models.iter().find(|m| m.id == id).cloned() {
                    let mut copy = m;
                    copy.id = uuid_v4();
                    copy.name = format!("{} (Copy)", copy.name);
                    self.config.models.push(copy);
                    let _ = self.storage.save_config(&self.config);
                    self.rebuild_tray();
                }
                Task::none()
            }
            DashboardMsg::DeleteModel(id) => {
                if self.config.models.len() > 1 {
                    self.config.models.retain(|m| m.id != id);
                    self.secrets.delete_api_key(&id);
                    self.config.ensure_valid();
                    let _ = self.storage.save_config(&self.config);
                    self.rebuild_tray();
                }
                Task::none()
            }
            DashboardMsg::CancelEditModel => {
                self.dashboard.model_editor = None;
                self.dashboard.selected_preset_kind = None;
                Task::none()
            }
            DashboardMsg::ModelNameChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.name = v;
                }
                Task::none()
            }
            DashboardMsg::ModelProviderChanged(kind) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.provider = kind;
                    match kind {
                        ProviderKind::GoogleTranslateFree => {
                            ed.base_url = "https://translate.googleapis.com".to_string();
                            ed.model_id = "gtx-free".to_string();
                            ed.supports_vision = false;
                        }
                        ProviderKind::OpenAiCompatible => {
                            if ed.base_url.is_empty()
                                || ed.base_url.contains("anthropic")
                                || ed.base_url.contains("generativelanguage")
                            {
                                ed.base_url = "https://openrouter.ai/api/v1".to_string();
                            }
                            ed.supports_vision = infer_vision_capability(kind, &ed.model_id);
                        }
                        ProviderKind::Anthropic => {
                            ed.base_url = "https://api.anthropic.com".to_string();
                            if ed.model_id.is_empty() || ed.model_id == "gtx-free" {
                                ed.model_id = "claude-sonnet-4-20250514".to_string();
                            }
                            ed.supports_vision = infer_vision_capability(kind, &ed.model_id);
                        }
                        ProviderKind::Gemini => {
                            ed.base_url = "https://generativelanguage.googleapis.com".to_string();
                            if ed.model_id.is_empty() || ed.model_id == "gtx-free" {
                                ed.model_id = "gemini-2.5-flash".to_string();
                            }
                            ed.supports_vision = infer_vision_capability(kind, &ed.model_id);
                        }
                    }
                }
                Task::none()
            }
            DashboardMsg::ModelBaseUrlChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.base_url = v;
                }
                Task::none()
            }
            DashboardMsg::ModelIdChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.model_id = v.clone();
                    if ed.vision_source != langhian_core::CapabilitySource::ManualOverride {
                        ed.supports_vision = infer_vision_capability(ed.provider, &v);
                    }
                }
                Task::none()
            }
            DashboardMsg::ModelApiKeyChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.api_key = v;
                }
                Task::none()
            }
            DashboardMsg::ModelHeadersAction(action) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.headers_editor.perform(action);
                    ed.headers_raw = ed.headers_editor.text();
                }
                Task::none()
            }
            DashboardMsg::ModelTempChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.temperature = v;
                }
                Task::none()
            }
            DashboardMsg::ModelMaxTokensChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.max_tokens = v;
                }
                Task::none()
            }
            DashboardMsg::ModelVisionToggled(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.supports_vision = v;
                    ed.vision_source = langhian_core::CapabilitySource::ManualOverride;
                }
                Task::none()
            }
            DashboardMsg::ModelCatalogFilterChanged(q) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.catalog_filter = q;
                }
                Task::none()
            }
            DashboardMsg::PickCatalogModel(idx) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut()
                    && let Some(entry) = ed.fetched_catalog.get(idx).cloned()
                {
                    ed.selected_catalog_idx = Some(idx);
                    ed.model_id = entry.id.clone();
                    if ed.editing_id.is_none() || ed.name.trim().is_empty() {
                        ed.name = if entry.display_name.trim().is_empty() {
                            entry.id.clone()
                        } else {
                            entry.display_name.clone()
                        };
                    }
                    if let Some(vis) = entry.supports_vision {
                        ed.supports_vision = vis;
                        ed.vision_source = langhian_core::CapabilitySource::AutoDetected;
                    } else if ed.vision_source != langhian_core::CapabilitySource::ManualOverride {
                        ed.supports_vision = infer_vision_capability(ed.provider, &entry.id);
                    }
                }
                Task::none()
            }
            DashboardMsg::TestModelConnection => {
                let Some(ed) = self.dashboard.model_editor.as_mut() else {
                    return Task::none();
                };
                ed.is_probing = true;
                ed.probe_status = None;
                let profile = build_profile_from_editor(ed);
                let key = if ed.api_key.trim().is_empty() {
                    None
                } else {
                    Some(ed.api_key.clone())
                };
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client
                            .probe_model(&profile, key.as_deref())
                            .await
                            .map(|p| (p.detected_vision.unwrap_or(false), p.detail))
                            .map_err(|e| e.to_string())
                    },
                    Message::ProbeModelFinished,
                )
            }
            DashboardMsg::FetchProviderModels => {
                let Some(ed) = self.dashboard.model_editor.as_mut() else {
                    return Task::none();
                };
                ed.is_probing = true;
                ed.probe_status = None;
                let profile = build_profile_from_editor(ed);
                let key = if ed.api_key.trim().is_empty() {
                    None
                } else {
                    Some(ed.api_key.clone())
                };
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client
                            .fetch_available_models_detailed(&profile, key.as_deref())
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::FetchModelsFinished,
                )
            }
            DashboardMsg::SaveModelProfile => {
                let Some(ed) = self.dashboard.model_editor.take() else {
                    return Task::none();
                };
                self.dashboard.selected_preset_kind = None;
                let profile = build_profile_from_editor(&ed);
                let pid = profile.id.clone();
                let _ = self.secrets.set_api_key(&pid, &ed.api_key);

                if let Some(existing) = self.config.models.iter_mut().find(|m| m.id == pid) {
                    *existing = profile;
                } else {
                    self.config.models.push(profile);
                    self.config.active_model_id = pid;
                }
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }

            // Prompts Tab
            DashboardMsg::StartAddPrompt => {
                let preset = PromptPreset {
                    id: uuid_v4(),
                    name: "Custom Prompt".to_string(),
                    shortcut_slot: None,
                    system_prompt: "You are a precise translator. Output only the translation."
                        .to_string(),
                    user_template: "Translate from {source_lang} to {target_lang}:\n\n{text}"
                        .to_string(),
                    vision_instruction: "Read and translate this screenshot into {target_lang}."
                        .to_string(),
                };
                self.dashboard.prompt_editor = Some(PromptEditorState::from_preset(&preset, true));
                Task::none()
            }
            DashboardMsg::StartEditPrompt(id) => {
                if let Some(p) = self.config.prompts.iter().find(|p| p.id == id) {
                    self.dashboard.prompt_editor = Some(PromptEditorState::from_preset(p, false));
                }
                Task::none()
            }
            DashboardMsg::DeletePrompt(id) => {
                if self.config.prompts.len() > 1 {
                    self.config.prompts.retain(|p| p.id != id);
                    self.config.ensure_valid();
                    let _ = self.storage.save_config(&self.config);
                    self.rebuild_tray();
                }
                Task::none()
            }
            DashboardMsg::SelectPromptActive(id) => {
                self.config.active_prompt_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::CancelEditPrompt => {
                self.dashboard.prompt_editor = None;
                Task::none()
            }
            DashboardMsg::PromptNameChanged(v) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.name = v;
                }
                Task::none()
            }
            DashboardMsg::PromptSlotChanged(slot) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.shortcut_slot = slot;
                }
                Task::none()
            }
            DashboardMsg::PromptSystemAction(action) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.system_editor.perform(action);
                }
                Task::none()
            }
            DashboardMsg::PromptUserTemplateAction(action) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.user_template_editor.perform(action);
                }
                Task::none()
            }
            DashboardMsg::PromptVisionAction(action) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.vision_editor.perform(action);
                }
                Task::none()
            }
            DashboardMsg::SavePromptPreset => {
                let Some(ed) = self.dashboard.prompt_editor.take() else {
                    return Task::none();
                };
                let pid = ed.editing_id.unwrap_or_else(uuid_v4);
                let preset = PromptPreset {
                    id: pid.clone(),
                    name: if ed.name.trim().is_empty() {
                        "Prompt".to_string()
                    } else {
                        ed.name.trim().to_string()
                    },
                    shortcut_slot: ed.shortcut_slot,
                    system_prompt: ed.system_editor.text(),
                    user_template: ed.user_template_editor.text(),
                    vision_instruction: ed.vision_editor.text(),
                };

                // Clear duplicate shortcut slot from other prompts
                if let Some(slot) = preset.shortcut_slot {
                    for p in &mut self.config.prompts {
                        if p.id != pid && p.shortcut_slot == Some(slot) {
                            p.shortcut_slot = None;
                        }
                    }
                }

                if let Some(existing) = self.config.prompts.iter_mut().find(|p| p.id == pid) {
                    *existing = preset;
                } else {
                    self.config.prompts.push(preset);
                }
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }

            // Vocabulary Tab
            DashboardMsg::VocabSearchChanged(v) => {
                self.dashboard.vocab_search = v;
                Task::none()
            }
            DashboardMsg::ToggleAddVocabForm => {
                self.dashboard.show_add_vocab_form = !self.dashboard.show_add_vocab_form;
                Task::none()
            }
            DashboardMsg::NewVocabTermChanged(v) => {
                self.dashboard.new_vocab_term = v;
                Task::none()
            }
            DashboardMsg::NewVocabTranslationChanged(v) => {
                self.dashboard.new_vocab_translation = v;
                Task::none()
            }
            DashboardMsg::NewVocabNotesAction(action) => {
                self.dashboard.new_vocab_notes_editor.perform(action);
                Task::none()
            }
            DashboardMsg::ConfirmAddVocab => {
                let t = self.dashboard.new_vocab_term.trim().to_string();
                let tr = self.dashboard.new_vocab_translation.trim().to_string();
                let n = self
                    .dashboard
                    .new_vocab_notes_editor
                    .text()
                    .trim()
                    .to_string();
                if !t.is_empty() && !tr.is_empty() {
                    self.add_vocab_entry(t, tr, n);
                    self.dashboard.new_vocab_term.clear();
                    self.dashboard.new_vocab_translation.clear();
                    self.dashboard.new_vocab_notes_editor = text_editor::Content::new();
                    self.dashboard.show_add_vocab_form = false;
                }
                Task::none()
            }
            DashboardMsg::DeleteVocab(id) => {
                self.vocab.retain(|v| v.id != id);
                self.vocab_matcher = VocabMatcher::build(self.vocab.clone());
                let _ = self.storage.save_vocab(&self.vocab);
                Task::none()
            }
            DashboardMsg::ExportVocabJson => {
                let path = self.storage.data_dir().join("vocab-export.json");
                if let Ok(json) = export_vocab_json(&self.vocab)
                    && std::fs::write(&path, json).is_ok()
                {
                    self.dashboard.export_status = Some(format!("Exported: {}", path.display()));
                }
                Task::none()
            }
            DashboardMsg::ExportVocabCsv => {
                let path = self.storage.data_dir().join("vocab-export.csv");
                let csv = export_vocab_csv(&self.vocab);
                if std::fs::write(&path, csv).is_ok() {
                    self.dashboard.export_status = Some(format!("Exported: {}", path.display()));
                }
                Task::none()
            }

            // History Tab
            DashboardMsg::CopyText(txt) => {
                let _ = set_clipboard_text(&txt);
                Task::none()
            }
            DashboardMsg::ToggleHistoryStarVocab(item) => {
                self.toggle_star_vocab(
                    item.source_text,
                    item.translated_text,
                    "auto".to_string(),
                    item.target_lang,
                );
                Task::none()
            }
            DashboardMsg::ClearHistory => {
                self.history.clear();
                let _ = self.storage.save_history(&self.history);
                Task::none()
            }

            // Settings & Updates (every change auto-saves immediately)
            DashboardMsg::UiLanguageChanged(code) => {
                self.config.ui_language = code.clone();
                self.i18n.set_language(&code);
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::ThemeChanged(t) => {
                self.config.theme = t;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ToggleDoubleAlt(v) => {
                self.config.shortcuts.enable_double_alt = v;
                self.sync_shortcuts_and_save();
                Task::none()
            }
            DashboardMsg::ToggleAutoSelectFallback(v) => {
                self.config.shortcuts.auto_select_all_fallback = v;
                self.sync_shortcuts_and_save();
                Task::none()
            }
            DashboardMsg::ToggleDoubleCtrl(v) => {
                self.config.shortcuts.enable_double_ctrl = v;
                self.sync_shortcuts_and_save();
                Task::none()
            }
            DashboardMsg::DoubleTapMsChanged(v) => {
                self.dashboard.double_tap_ms_input = v.clone();
                if let Ok(ms) = v.trim().parse::<u64>() {
                    self.config.shortcuts.double_tap_ms = ms.clamp(160, 800);
                    self.sync_shortcuts_and_save();
                }
                Task::none()
            }
            DashboardMsg::HotkeyScreenshotChanged(v) => {
                self.dashboard.hotkey_screenshot = v;
                self.parse_and_save_hotkeys();
                Task::none()
            }
            DashboardMsg::HotkeyNextPromptChanged(v) => {
                self.dashboard.hotkey_next_prompt = v;
                self.parse_and_save_hotkeys();
                Task::none()
            }
            DashboardMsg::HotkeyPrevPromptChanged(v) => {
                self.dashboard.hotkey_prev_prompt = v;
                self.parse_and_save_hotkeys();
                Task::none()
            }
            DashboardMsg::HotkeyDashboardChanged(v) => {
                self.dashboard.hotkey_dashboard = v;
                self.parse_and_save_hotkeys();
                Task::none()
            }
            DashboardMsg::ToggleRestoreClipboard(v) => {
                self.config.restore_clipboard = v;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ToggleLaunchStartup(v) => {
                self.config.launch_at_startup = v;
                let _ = platform::autostart::set_launch_at_startup(v);
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ToggleShowHud(v) => {
                self.config.show_hud = v;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::TogglePreserveMarkdown(v) => {
                self.config.preserve_markdown = v;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ToggleAutoCheckUpdates(v) => {
                self.config.auto_check_updates = v;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ResetModalPosition => {
                self.config.modal_position = None;
                self.config.modal_size = None;
                let _ = self.storage.save_config(&self.config);
                self.dashboard.settings_toast = Some(self.i18n.t("settings.saved_toast"));
                Task::none()
            }
            DashboardMsg::CheckUpdatesNow => {
                self.dashboard.update_checking = true;
                self.dashboard.update_error = None;
                let updater = self.updater.clone();
                Task::perform(
                    async move {
                        updater
                            .check_for_updates(env!("CARGO_PKG_VERSION"))
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::UpdateCheckFinished,
                )
            }
            DashboardMsg::DownloadAndInstallUpdate => {
                let Some(res) = &self.dashboard.update_status else {
                    return Task::none();
                };
                let Some(asset) = res.installer_asset.clone() else {
                    let _ = open::that(&res.release_url);
                    return Task::none();
                };
                self.dashboard.update_downloading = true;
                let updater = self.updater.clone();
                Task::perform(
                    async move {
                        updater
                            .download_installer(&asset)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::UpdateDownloadFinished,
                )
            }
            DashboardMsg::OpenReleaseUrl(url) => {
                let _ = open::that(url);
                Task::none()
            }
        }
    }

    fn sync_shortcuts_and_save(&mut self) {
        self.config.ensure_valid();
        let _ = self.storage.save_config(&self.config);
        if let Ok(mut g) = self.shared_shortcuts.lock() {
            *g = self.config.shortcuts.clone();
        }
        refresh_shortcut_config(&self.config.shortcuts);
        self.dashboard.settings_toast = Some(self.i18n.t("settings.saved_toast"));
    }

    fn parse_and_save_hotkeys(&mut self) {
        let mut all_valid = true;
        if let Some(c) = ShortcutCombo::parse_display(&self.dashboard.hotkey_screenshot) {
            self.config.shortcuts.screenshot = c;
        } else {
            all_valid = false;
        }
        if let Some(c) = ShortcutCombo::parse_display(&self.dashboard.hotkey_next_prompt) {
            self.config.shortcuts.next_prompt = c;
        } else {
            all_valid = false;
        }
        if let Some(c) = ShortcutCombo::parse_display(&self.dashboard.hotkey_prev_prompt) {
            self.config.shortcuts.prev_prompt = c;
        } else {
            all_valid = false;
        }
        if let Some(c) = ShortcutCombo::parse_display(&self.dashboard.hotkey_dashboard) {
            self.config.shortcuts.open_dashboard = c;
        } else {
            all_valid = false;
        }

        if all_valid {
            self.dashboard.settings_error = None;
            self.sync_shortcuts_and_save();
        } else {
            self.dashboard.settings_error = Some(self.i18n.t("settings.hotkey_syntax_hint"));
        }
    }

    fn toggle_star_vocab(
        &mut self,
        term: String,
        translation: String,
        source_lang: String,
        target_lang: String,
    ) {
        let trimmed = term.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Some(idx) = find_vocab_match_index(&self.vocab, trimmed, &translation) {
            self.vocab.remove(idx);
            self.vocab_matcher = VocabMatcher::build(self.vocab.clone());
            let _ = self.storage.save_vocab(&self.vocab);
        } else if !translation.trim().is_empty() {
            self.add_vocab_entry_with_langs(
                term,
                translation,
                String::new(),
                source_lang,
                target_lang,
            );
        }
    }

    fn add_vocab_entry(&mut self, term: String, translation: String, notes: String) {
        self.add_vocab_entry_with_langs(
            term,
            translation,
            notes,
            self.config.source_language.clone(),
            self.config.target_language.clone(),
        );
    }

    fn add_vocab_entry_with_langs(
        &mut self,
        term: String,
        translation: String,
        notes: String,
        source_lang: String,
        target_lang: String,
    ) {
        let entry = VocabEntry::new(term, translation, notes, source_lang, target_lang);
        // Deduplicate by case-insensitive term
        let mut seen: HashSet<String> = HashSet::new();
        self.vocab.insert(0, entry);
        self.vocab
            .retain(|v| seen.insert(v.term.trim().to_lowercase()));
        self.vocab_matcher = VocabMatcher::build(self.vocab.clone());
        let _ = self.storage.save_vocab(&self.vocab);
    }

    fn record_history(
        &mut self,
        mode: TranslationMode,
        source: String,
        translated: String,
        target_lang: String,
    ) {
        let entry = HistoryEntry::new(
            mode,
            source,
            translated,
            self.config.active_model().name.clone(),
            self.config.active_prompt().name.clone(),
            target_lang,
        );
        push_bounded_history(&mut self.history, entry);
        let _ = self.storage.save_history(&self.history);
    }

    fn rebuild_tray(&mut self) {
        if let Some(tray) = self.tray.as_mut() {
            tray.rebuild_menu(&self.config, &self.i18n);
        }
    }
}

fn schedule_copy_reset(target: CopyBadgeTarget, seq: u64) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
        },
        move |_| Message::ClearCopyBadge { target, seq },
    )
}

fn floating_platform_specific() -> window::settings::PlatformSpecific {
    #[cfg(target_os = "windows")]
    {
        window::settings::PlatformSpecific {
            skip_taskbar: true,
            undecorated_shadow: false,
            corner_preference: window::settings::platform::CornerPreference::DoNotRound,
            ..Default::default()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        window::settings::PlatformSpecific::default()
    }
}

fn dashboard_platform_specific() -> window::settings::PlatformSpecific {
    #[cfg(target_os = "windows")]
    {
        window::settings::PlatformSpecific {
            skip_taskbar: false,
            undecorated_shadow: false,
            corner_preference: window::settings::platform::CornerPreference::DoNotRound,
            ..Default::default()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        window::settings::PlatformSpecific::default()
    }
}

fn open_dashboard_task(icon: Option<window::Icon>) -> Task<Message> {
    let (_, open) = window::open(window::Settings {
        size: Size::new(1060.0, 720.0),
        min_size: Some(Size::new(900.0, 600.0)),
        position: window::Position::Centered,
        decorations: false,
        transparent: true,
        icon,
        exit_on_close_request: false,
        platform_specific: dashboard_platform_specific(),
        ..Default::default()
    });
    open.map(|id| Message::WindowOpened(WindowKind::Dashboard, id))
}

fn bridge_stream() -> impl iced::futures::Stream<Item = BridgeEvent> {
    stream::channel(64, async move |mut output| {
        let rx = BRIDGE_RX
            .get()
            .and_then(|m| m.lock().ok())
            .and_then(|mut g| g.take());
        if let Some(mut rx) = rx {
            while let Some(ev) = rx.recv().await {
                if output.send(ev).await.is_err() {
                    break;
                }
            }
        }
        // Keep stream parked forever if receiver was already taken
        std::future::pending::<()>().await;
    })
}

fn map_provider_error(
    err: ProviderError,
    vision_msg: &str,
    key_msg: &str,
    rate_msg: &str,
) -> TranslationFail {
    match err {
        ProviderError::VisionUnsupported { .. } => TranslationFail {
            is_vision: true,
            message: vision_msg.to_string(),
        },
        ProviderError::MissingApiKey { .. } => TranslationFail {
            is_vision: false,
            message: key_msg.to_string(),
        },
        ProviderError::RateLimited => TranslationFail {
            is_vision: false,
            message: rate_msg.to_string(),
        },
        other => TranslationFail {
            is_vision: false,
            message: other.to_string(),
        },
    }
}

fn build_profile_from_editor(ed: &ModelEditorState) -> ModelProfile {
    let mut headers = BTreeMap::new();
    for line in ed.headers_raw.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let kt = k.trim();
            let vt = v.trim();
            if !kt.is_empty() {
                headers.insert(kt.to_string(), vt.to_string());
            }
        }
    }
    let requires_api_key = match ed.provider {
        ProviderKind::GoogleTranslateFree => false,
        ProviderKind::Anthropic | ProviderKind::Gemini => true,
        ProviderKind::OpenAiCompatible => {
            let lower = ed.base_url.to_ascii_lowercase();
            !(lower.contains("localhost") || lower.contains("127.0.0.1"))
        }
    };

    ModelProfile {
        id: ed.editing_id.clone().unwrap_or_else(uuid_v4),
        name: if ed.name.trim().is_empty() {
            ed.model_id.clone()
        } else {
            ed.name.trim().to_string()
        },
        provider: ed.provider,
        base_url: ed.base_url.trim().to_string(),
        model_id: ed.model_id.trim().to_string(),
        requires_api_key,
        supports_vision: ed.supports_vision,
        vision_source: ed.vision_source,
        extra_headers: headers,
        temperature: ed.temperature.trim().parse::<f32>().unwrap_or(0.2),
        max_tokens: ed.max_tokens.trim().parse::<u32>().ok(),
    }
}

fn uuid_v4() -> String {
    format!(
        "{:016x}{:016x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        rand_mix()
    )
}

fn rand_mix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0x517cc1b727220a95);
    COUNTER.fetch_add(0x9e3779b97f4a7c15, Ordering::Relaxed)
}
