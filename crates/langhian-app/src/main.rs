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
    AppConfig, AppStorage, HistoryEntry, ModelProfile, PromptPreset, ProviderError, ProviderKind,
    SecretStore, ShortcutCombo, ShortcutConfig, TranslateInput, TranslateRequest, TranslationMode,
    Translator, TranslatorClient, UpdateCheckResult, UpdaterClient, VocabEntry, VocabMatcher,
    history::push_bounded_history, infer_vision_capability,
};
use platform::capture::{capture_active_monitor, crop_screen_to_png};
use platform::clipboard::{capture_selection, replace_with_text, set_clipboard_text};
use platform::tray::{TrayAction, TrayHandle, build_tray, load_app_icon_rgba};
use platform::updater_exec::launch_installer;
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
use ui::theme::{bundled_fonts, iced_theme_for};

#[derive(Debug, Clone)]
enum BridgeEvent {
    Input(InputAction),
    Tray(TrayAction),
}

static BRIDGE_RX: OnceLock<Mutex<Option<UnboundedReceiver<BridgeEvent>>>> = OnceLock::new();

#[cfg(target_os = "windows")]
fn acquire_windows_mutex() {
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let wide: Vec<u16> = "LanghianTranslationSingleInstanceMutex\0"
        .encode_utf16()
        .collect();
    unsafe {
        let _ = CreateMutexW(std::ptr::null(), 0, wide.as_ptr());
    }
}

fn main() -> iced::Result {
    #[cfg(target_os = "windows")]
    acquire_windows_mutex();

    iced::daemon(LanghianApp::new, LanghianApp::update, LanghianApp::view)
        .title(LanghianApp::title)
        .theme(LanghianApp::theme)
        .subscription(LanghianApp::subscription)
        .run()
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
    modal_id: Option<window::Id>,
    snip_id: Option<window::Id>,
    hud_id: Option<window::Id>,

    // Window states
    dashboard: DashboardState,
    modal: Option<ModalState>,
    snip: Option<SnipState>,
    hud: Option<HudState>,
    hud_expires_at: Option<Instant>,
    pending_replace_prev_clip: Option<String>,
    translation_seq: u64,
}

#[derive(Debug, Clone)]
enum Message {
    FontLoaded,
    Bridge(BridgeEvent),
    WindowOpened(WindowKind, window::Id),
    WindowCloseRequested(window::Id),
    GlobalEscPressed,
    HudTick,

    // Async workflow results
    ModalTranslationFinished {
        seq: u64,
        result: Result<(String, Option<String>), TranslationFail>,
        mode: TranslationMode,
    },
    InPlaceFlowFinished(Result<(String, String, Option<String>), String>),
    ManualTranslateFinished {
        seq: u64,
        result: Result<(String, Option<String>), TranslationFail>,
    },
    ProbeModelFinished(Result<(bool, String), String>),
    FetchModelsFinished(Result<Vec<String>, String>),
    UpdateCheckFinished(Result<UpdateCheckResult, String>),
    UpdateDownloadFinished(Result<std::path::PathBuf, String>),

    // Sub-views
    Dashboard(DashboardMsg),
    Modal(ModalMsg),
    Snip(SnipEvent),
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
        let dashboard = DashboardState::new(&config);

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
        let bridge_tx_tray = bridge_tx;
        std::thread::spawn(move || {
            while let Some(act) = tray_rx.blocking_recv() {
                if bridge_tx_tray.send(BridgeEvent::Tray(act)).is_err() {
                    break;
                }
            }
        });
        let tray = build_tray(&config, &i18n, tray_tx).ok();

        let app_icon =
            load_app_icon_rgba().and_then(|(rgba, w, h)| window::icon::from_rgba(rgba, w, h).ok());

        let start_minimized = std::env::args().any(|a| a == "--minimized");

        let mut tasks: Vec<Task<Message>> = bundled_fonts()
            .into_iter()
            .map(|bytes| iced::font::load(bytes).map(|_| Message::FontLoaded))
            .collect();

        if !start_minimized {
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
            modal_id: None,
            snip_id: None,
            hud_id: None,
            dashboard,
            modal: None,
            snip: None,
            hud: None,
            hud_expires_at: None,
            pending_replace_prev_clip: None,
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
        iced_theme_for(self.config.theme)
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            Subscription::run(bridge_stream).map(Message::Bridge),
            window::close_requests().map(Message::WindowCloseRequested),
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
            Message::FontLoaded => Task::none(),

            Message::Bridge(BridgeEvent::Input(act)) => self.handle_input_action(act),
            Message::Bridge(BridgeEvent::Tray(act)) => self.handle_tray_action(act),

            Message::WindowOpened(kind, id) => {
                match kind {
                    WindowKind::Dashboard => self.dashboard_id = Some(id),
                    WindowKind::Modal => self.modal_id = Some(id),
                    WindowKind::Snip => self.snip_id = Some(id),
                    WindowKind::Hud => self.hud_id = Some(id),
                }
                Task::none()
            }

            Message::WindowCloseRequested(id) => {
                if Some(id) == self.dashboard_id {
                    self.dashboard_id = None;
                } else if Some(id) == self.modal_id {
                    self.modal_id = None;
                    self.modal = None;
                } else if Some(id) == self.snip_id {
                    self.snip_id = None;
                    self.snip = None;
                } else if Some(id) == self.hud_id {
                    self.hud_id = None;
                    self.hud = None;
                    self.hud_expires_at = None;
                }
                window::close(id)
            }

            Message::GlobalEscPressed => {
                if let Some(id) = self.snip_id.take() {
                    self.snip = None;
                    return window::close(id);
                }
                if let Some(id) = self.modal_id.take() {
                    self.modal = None;
                    return window::close(id);
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

            Message::ModalTranslationFinished { seq, result, mode } => {
                if seq != self.translation_seq {
                    return Task::none();
                }
                let Some(modal) = self.modal.as_mut() else {
                    return Task::none();
                };
                modal.is_loading = false;
                match result {
                    Ok((translated, detected)) => {
                        modal.translated_text = translated.clone();
                        modal.detected_source_lang = detected;
                        modal.error_message = None;
                        modal.is_vision_error = false;
                        modal.save_translation = translated.clone();
                        let src_record = if modal.screenshot_png.is_some() {
                            "[Screenshot]".to_string()
                        } else {
                            modal.source_text.clone()
                        };
                        self.record_history(mode, src_record, translated);
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
                    self.record_history(TranslationMode::InPlace, source, translated);
                    if self.config.show_hud {
                        self.show_hud_toast(
                            self.i18n.t("app.title"),
                            self.i18n.t("hud.replaced_success"),
                            false,
                        )
                    } else {
                        Task::none()
                    }
                }
                Err(err) => {
                    if self.config.show_hud {
                        self.show_hud_toast(self.i18n.t("app.status_error"), err, true)
                    } else {
                        Task::none()
                    }
                }
            },

            Message::ManualTranslateFinished { seq, result } => {
                if seq != self.translation_seq {
                    return Task::none();
                }
                self.dashboard.is_translating = false;
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
                            ed.fetched_models = list;
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
                        self.dashboard.update_result = Some(r);
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
        }
    }

    fn view(&self, window: window::Id) -> Element<'_, Message> {
        if Some(window) == self.modal_id
            && let Some(modal) = &self.modal
        {
            return view_modal(modal, &self.config, &self.i18n, &self.vocab_matcher)
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
            return view_hud(hud, self.config.theme);
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
                    self.show_hud_toast(self.i18n.t("hud.prompt_switched"), pname, false)
                } else {
                    Task::none()
                }
            }
            TrayAction::Quit => iced::exit(),
        }
    }

    fn ensure_dashboard_open(&mut self) -> Task<Message> {
        if let Some(id) = self.dashboard_id {
            window::gain_focus(id)
        } else {
            open_dashboard_task(self.app_icon.clone())
        }
    }

    fn run_double_alt_inplace(&mut self) -> Task<Message> {
        let profile = self.config.active_model().clone();
        let api_key = self.secrets.get_api_key(&profile.id);
        let prompt = self.config.active_prompt().clone();
        let source_lang = self.config.source_language.clone();
        let target_lang = self.config.target_language.clone();
        let restore_clip = self.config.restore_clipboard;
        let auto_select = self.config.shortcuts.auto_select_all_fallback;
        let client = self.client.clone();
        let empty_err = self.i18n.t("errors.empty_selection");

        let hud_task = if self.config.show_hud {
            self.show_hud_toast(
                self.i18n.t("app.title"),
                self.i18n.t("hud.translating_inplace"),
                false,
            )
        } else {
            Task::none()
        };

        let work_task = Task::perform(
            async move {
                let captured =
                    tokio::task::spawn_blocking(move || capture_selection(false, auto_select))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| if e == "EMPTY_SELECTION" { empty_err } else { e })?;

                let req = TranslateRequest {
                    input: TranslateInput::Text(captured.text.clone()),
                    source_lang,
                    target_lang,
                    prompt,
                };
                let resp = client
                    .translate(&profile, api_key.as_deref(), &req)
                    .await
                    .map_err(|e| e.to_string())?;

                let translated = resp.translated_text.clone();
                let prev = captured.previous_clipboard.clone();
                let tr_paste = translated.clone();
                tokio::task::spawn_blocking(move || {
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

    fn run_double_ctrl_modal(&mut self) -> Task<Message> {
        let restore_clip = self.config.restore_clipboard;
        let captured = match capture_selection(restore_clip, false) {
            Ok(c) => c,
            Err(_) => {
                if self.config.show_hud {
                    return self.show_hud_toast(
                        self.i18n.t("app.status_error"),
                        self.i18n.t("errors.empty_selection"),
                        true,
                    );
                }
                return Task::none();
            }
        };

        self.pending_replace_prev_clip = captured.previous_clipboard;
        let modal = ModalState::for_text(captured.text);
        self.open_or_update_modal(modal, TranslationMode::Modal)
    }

    fn start_screenshot_capture(&mut self) -> Task<Message> {
        let cursor = current_cursor_pos();
        let screen = match capture_active_monitor(cursor) {
            Ok(s) => s,
            Err(e) => {
                return self.show_hud_toast(self.i18n.t("app.status_error"), e, true);
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
            ..Default::default()
        });
        open.map(|id| Message::WindowOpened(WindowKind::Snip, id))
    }

    fn open_or_update_modal(&mut self, modal: ModalState, mode: TranslationMode) -> Task<Message> {
        self.modal = Some(modal);
        let translate_task = self.dispatch_modal_translation(mode);

        if let Some(id) = self.modal_id {
            return Task::batch([window::gain_focus(id), translate_task]);
        }

        let pos = match current_cursor_pos() {
            Some((cx, cy)) => window::Position::Specific(Point::new(
                (cx - 240).max(20) as f32,
                (cy + 16).max(20) as f32,
            )),
            None => window::Position::Centered,
        };

        let (_, open) = window::open(window::Settings {
            size: Size::new(540.0, 440.0),
            min_size: Some(Size::new(420.0, 320.0)),
            position: pos,
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            icon: self.app_icon.clone(),
            exit_on_close_request: false,
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

        let input = if let Some(png) = &modal.screenshot_png {
            TranslateInput::ImagePng {
                bytes: png.clone(),
                context_hint: None,
            }
        } else {
            TranslateInput::Text(modal.source_text.clone())
        };

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

        let api_key = self.secrets.get_api_key(&profile.id);
        let req = TranslateRequest {
            input,
            source_lang: self.config.source_language.clone(),
            target_lang: self.config.target_language.clone(),
            prompt: self.config.active_prompt().clone(),
        };
        let client = self.client.clone();

        Task::perform(
            async move {
                match client.translate(&profile, api_key.as_deref(), &req).await {
                    Ok(r) => Ok((r.translated_text, r.detected_source_lang)),
                    Err(e) => Err(map_provider_error(e, &vision_msg, &key_msg, &rate_msg)),
                }
            },
            move |result| Message::ModalTranslationFinished { seq, result, mode },
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
            self.show_hud_toast(self.i18n.t("hud.prompt_switched"), name, false)
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
                format!("[{slot}] {}", found.name),
                false,
            )
        } else {
            Task::none()
        }
    }

    fn show_hud_toast(&mut self, title: String, detail: String, is_error: bool) -> Task<Message> {
        self.hud = Some(HudState {
            title,
            detail,
            is_error,
        });
        self.hud_expires_at = Some(Instant::now() + Duration::from_millis(1600));

        if let Some(id) = self.hud_id {
            return window::gain_focus(id);
        }

        let (_, open) = window::open(window::Settings {
            size: Size::new(360.0, 56.0),
            position: window::Position::Specific(Point::new(32.0, 32.0)),
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            resizable: false,
            exit_on_close_request: false,
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
                        let modal = ModalState::for_screenshot(png);
                        let modal_task =
                            self.open_or_update_modal(modal, TranslationMode::Screenshot);
                        Task::batch([close_task, modal_task])
                    }
                    Err(err) => {
                        let hud_task =
                            self.show_hud_toast(self.i18n.t("app.status_error"), err, true);
                        Task::batch([close_task, hud_task])
                    }
                }
            }
        }
    }

    fn update_modal(&mut self, msg: ModalMsg) -> Task<Message> {
        match msg {
            ModalMsg::DragWindow => {
                if let Some(id) = self.modal_id {
                    window::drag(id)
                } else {
                    Task::none()
                }
            }
            ModalMsg::Close => {
                self.modal = None;
                if let Some(id) = self.modal_id.take() {
                    window::close(id)
                } else {
                    Task::none()
                }
            }
            ModalMsg::TogglePin => {
                if let Some(m) = self.modal.as_mut() {
                    m.pinned = !m.pinned;
                }
                Task::none()
            }
            ModalMsg::SelectModel(id) => {
                self.config.active_model_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                let mode = if self
                    .modal
                    .as_ref()
                    .is_some_and(|m| m.screenshot_png.is_some())
                {
                    TranslationMode::Screenshot
                } else {
                    TranslationMode::Modal
                };
                self.dispatch_modal_translation(mode)
            }
            ModalMsg::SelectPrompt(id) => {
                self.config.active_prompt_id = id;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                let mode = if self
                    .modal
                    .as_ref()
                    .is_some_and(|m| m.screenshot_png.is_some())
                {
                    TranslationMode::Screenshot
                } else {
                    TranslationMode::Modal
                };
                self.dispatch_modal_translation(mode)
            }
            ModalMsg::SelectTargetLang(code) => {
                self.config.target_language = code;
                let _ = self.storage.save_config(&self.config);
                let mode = if self
                    .modal
                    .as_ref()
                    .is_some_and(|m| m.screenshot_png.is_some())
                {
                    TranslationMode::Screenshot
                } else {
                    TranslationMode::Modal
                };
                self.dispatch_modal_translation(mode)
            }
            ModalMsg::Regenerate => {
                let mode = if self
                    .modal
                    .as_ref()
                    .is_some_and(|m| m.screenshot_png.is_some())
                {
                    TranslationMode::Screenshot
                } else {
                    TranslationMode::Modal
                };
                self.dispatch_modal_translation(mode)
            }
            ModalMsg::CopyResult => {
                if let Some(m) = self.modal.as_mut()
                    && !m.translated_text.is_empty()
                    && set_clipboard_text(&m.translated_text).is_ok()
                {
                    m.copied_feedback = true;
                }
                Task::none()
            }
            ModalMsg::ReplaceInSourceApp => {
                let Some(m) = self.modal.take() else {
                    return Task::none();
                };
                let close_task = if let Some(id) = self.modal_id.take() {
                    window::close(id)
                } else {
                    Task::none()
                };
                if m.translated_text.is_empty() {
                    return close_task;
                }
                let text = m.translated_text;
                let restore = self.config.restore_clipboard;
                let prev = self.pending_replace_prev_clip.take();
                let replace_task = Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        let _ = tokio::task::spawn_blocking(move || {
                            replace_with_text(&text, restore, prev)
                        })
                        .await;
                    },
                    |_| Message::FontLoaded,
                );
                Task::batch([close_task, replace_task])
            }
            ModalMsg::ClickHighlightedVocab(entry) => {
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
            ModalMsg::ToggleSaveDrawer => {
                if let Some(m) = self.modal.as_mut() {
                    m.show_save_drawer = !m.show_save_drawer;
                    if m.show_save_drawer && m.save_translation.is_empty() {
                        m.save_translation = m.translated_text.clone();
                    }
                }
                Task::none()
            }
            ModalMsg::SaveTermChanged(v) => {
                if let Some(m) = self.modal.as_mut() {
                    m.save_term = v;
                }
                Task::none()
            }
            ModalMsg::SaveTranslationChanged(v) => {
                if let Some(m) = self.modal.as_mut() {
                    m.save_translation = v;
                }
                Task::none()
            }
            ModalMsg::SaveNotesChanged(v) => {
                if let Some(m) = self.modal.as_mut() {
                    m.save_notes = v;
                }
                Task::none()
            }
            ModalMsg::ConfirmSaveVocab => {
                let (term, translation, notes) = if let Some(m) = self.modal.as_mut() {
                    let t = m.save_term.trim().to_string();
                    let tr = m.save_translation.trim().to_string();
                    let n = m.save_notes.trim().to_string();
                    if t.is_empty() || tr.is_empty() {
                        return Task::none();
                    }
                    m.show_save_drawer = false;
                    m.saved_feedback = true;
                    (t, tr, n)
                } else {
                    return Task::none();
                };
                self.add_vocab_entry(term, translation, notes);
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
            DashboardMsg::SwitchTab(tab) => {
                self.dashboard.active_tab = tab;
                Task::none()
            }
            DashboardMsg::SourceEditorAction(action) => {
                self.dashboard.source_editor.perform(action);
                Task::none()
            }
            DashboardMsg::SelectSourceLang(code) => {
                self.config.source_language = code;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::SelectTargetLang(code) => {
                self.config.target_language = code;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::SwapLangs => {
                if self.config.source_language != "auto" {
                    std::mem::swap(
                        &mut self.config.source_language,
                        &mut self.config.target_language,
                    );
                    let _ = self.storage.save_config(&self.config);
                }
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

                let profile = self.config.active_model().clone();
                let api_key = self.secrets.get_api_key(&profile.id);
                let req = TranslateRequest {
                    input: TranslateInput::Text(text_val),
                    source_lang: self.config.source_language.clone(),
                    target_lang: self.config.target_language.clone(),
                    prompt: self.config.active_prompt().clone(),
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
                    move |result| Message::ManualTranslateFinished { seq, result },
                )
            }
            DashboardMsg::TriggerScreenshot => self.start_screenshot_capture(),
            DashboardMsg::ClearManual => {
                self.dashboard.source_editor = text_editor::Content::new();
                self.dashboard.manual_output.clear();
                self.dashboard.translate_error = None;
                self.dashboard.copied_manual = false;
                Task::none()
            }
            DashboardMsg::CopyManualResult => {
                if !self.dashboard.manual_output.is_empty()
                    && set_clipboard_text(&self.dashboard.manual_output).is_ok()
                {
                    self.dashboard.copied_manual = true;
                }
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
            DashboardMsg::SaveManualNotesChanged(v) => {
                self.dashboard.save_notes = v;
                Task::none()
            }
            DashboardMsg::ConfirmSaveManualVocab => {
                let term = self.dashboard.save_term.trim().to_string();
                let tr = self.dashboard.save_translation.trim().to_string();
                let notes = self.dashboard.save_notes.trim().to_string();
                if !term.is_empty() && !tr.is_empty() {
                    self.add_vocab_entry(term, tr, notes);
                    self.dashboard.show_save_vocab_box = false;
                    self.dashboard.save_notes.clear();
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
                            if ed.base_url.is_empty() || ed.base_url.contains("anthropic") {
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
            DashboardMsg::ModelHeadersChanged(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.headers_raw = v;
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
            DashboardMsg::PickFetchedModel(id) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.model_id = id.clone();
                    ed.supports_vision = infer_vision_capability(ed.provider, &id);
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
                            .fetch_available_models(&profile, key.as_deref())
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
            DashboardMsg::StartNewPrompt => {
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
                    ed.shortcut_slot = slot.0;
                }
                Task::none()
            }
            DashboardMsg::PromptSystemChanged(v) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.system_prompt = v;
                }
                Task::none()
            }
            DashboardMsg::PromptUserTemplateChanged(v) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.user_template = v;
                }
                Task::none()
            }
            DashboardMsg::PromptVisionChanged(v) => {
                if let Some(ed) = self.dashboard.prompt_editor.as_mut() {
                    ed.vision_instruction = v;
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
                    system_prompt: ed.system_prompt,
                    user_template: ed.user_template,
                    vision_instruction: ed.vision_instruction,
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
            DashboardMsg::NewVocabNotesChanged(v) => {
                self.dashboard.new_vocab_notes = v;
                Task::none()
            }
            DashboardMsg::ConfirmAddVocab => {
                let t = self.dashboard.new_vocab_term.trim().to_string();
                let tr = self.dashboard.new_vocab_translation.trim().to_string();
                let n = self.dashboard.new_vocab_notes.trim().to_string();
                if !t.is_empty() && !tr.is_empty() {
                    self.add_vocab_entry(t, tr, n);
                    self.dashboard.new_vocab_term.clear();
                    self.dashboard.new_vocab_translation.clear();
                    self.dashboard.new_vocab_notes.clear();
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
            DashboardMsg::SaveHistoryToVocab(item) => {
                self.add_vocab_entry(item.source_text, item.translated_text, String::new());
                Task::none()
            }
            DashboardMsg::ClearHistory => {
                self.history.clear();
                let _ = self.storage.save_history(&self.history);
                Task::none()
            }

            // Settings & Updates
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
                Task::none()
            }
            DashboardMsg::ToggleAutoSelectFallback(v) => {
                self.config.shortcuts.auto_select_all_fallback = v;
                Task::none()
            }
            DashboardMsg::ToggleDoubleCtrl(v) => {
                self.config.shortcuts.enable_double_ctrl = v;
                Task::none()
            }
            DashboardMsg::DoubleTapMsChanged(v) => {
                self.dashboard.double_tap_ms_draft = v;
                Task::none()
            }
            DashboardMsg::ScreenshotHotkeyChanged(v) => {
                self.dashboard.screenshot_hotkey_draft = v;
                Task::none()
            }
            DashboardMsg::NextPromptHotkeyChanged(v) => {
                self.dashboard.next_prompt_hotkey_draft = v;
                Task::none()
            }
            DashboardMsg::PrevPromptHotkeyChanged(v) => {
                self.dashboard.prev_prompt_hotkey_draft = v;
                Task::none()
            }
            DashboardMsg::DashboardHotkeyChanged(v) => {
                self.dashboard.dashboard_hotkey_draft = v;
                Task::none()
            }
            DashboardMsg::ToggleRestoreClipboard(v) => {
                self.config.restore_clipboard = v;
                Task::none()
            }
            DashboardMsg::ToggleLaunchStartup(v) => {
                self.config.launch_at_startup = v;
                let _ = platform::autostart::set_launch_at_startup(v);
                Task::none()
            }
            DashboardMsg::ToggleShowHud(v) => {
                self.config.show_hud = v;
                Task::none()
            }
            DashboardMsg::ToggleAutoCheckUpdates(v) => {
                self.config.auto_check_updates = v;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::SaveSettings => {
                if let Ok(ms) = self.dashboard.double_tap_ms_draft.trim().parse::<u64>() {
                    self.config.shortcuts.double_tap_ms = ms.clamp(160, 800);
                }
                if let Some(c) =
                    ShortcutCombo::parse_display(&self.dashboard.screenshot_hotkey_draft)
                {
                    self.config.shortcuts.screenshot = c;
                }
                if let Some(c) =
                    ShortcutCombo::parse_display(&self.dashboard.next_prompt_hotkey_draft)
                {
                    self.config.shortcuts.next_prompt = c;
                }
                if let Some(c) =
                    ShortcutCombo::parse_display(&self.dashboard.prev_prompt_hotkey_draft)
                {
                    self.config.shortcuts.prev_prompt = c;
                }
                if let Some(c) =
                    ShortcutCombo::parse_display(&self.dashboard.dashboard_hotkey_draft)
                {
                    self.config.shortcuts.open_dashboard = c;
                }
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                if let Ok(mut g) = self.shared_shortcuts.lock() {
                    *g = self.config.shortcuts.clone();
                }
                refresh_shortcut_config(&self.config.shortcuts);
                self.dashboard.settings_toast = Some(self.i18n.t("settings.saved_toast"));
                Task::none()
            }
            DashboardMsg::CheckForUpdates => {
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
                let Some(res) = &self.dashboard.update_result else {
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

    fn add_vocab_entry(&mut self, term: String, translation: String, notes: String) {
        let entry = VocabEntry::new(
            term,
            translation,
            notes,
            self.config.source_language.clone(),
            self.config.target_language.clone(),
        );
        // Deduplicate by case-insensitive term
        let mut seen: HashSet<String> = HashSet::new();
        self.vocab.insert(0, entry);
        self.vocab
            .retain(|v| seen.insert(v.term.trim().to_lowercase()));
        self.vocab_matcher = VocabMatcher::build(self.vocab.clone());
        let _ = self.storage.save_vocab(&self.vocab);
    }

    fn record_history(&mut self, mode: TranslationMode, source: String, translated: String) {
        let entry = HistoryEntry::new(
            mode,
            source,
            translated,
            self.config.active_model().name.clone(),
            self.config.active_prompt().name.clone(),
            self.config.target_language.clone(),
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

fn open_dashboard_task(icon: Option<window::Icon>) -> Task<Message> {
    let (_, open) = window::open(window::Settings {
        size: Size::new(980.0, 660.0),
        min_size: Some(Size::new(760.0, 520.0)),
        position: window::Position::Centered,
        decorations: true,
        transparent: false,
        icon,
        exit_on_close_request: false,
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
