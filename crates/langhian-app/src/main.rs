#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;
mod ui;

use iced::futures::SinkExt;
use iced::keyboard::key::Named;
use iced::widget::container;
use iced::{Element, Point, Size, Subscription, Task, Theme, keyboard, stream, window};
use langhian_core::hotkey_logic::InputAction;
use langhian_core::vocab::{export_vocab_csv, export_vocab_json};
use langhian_core::{
    AppConfig, AppStorage, HistoryEntry, ModelCatalogEntry, ModelProfile, PromptPreset,
    ProviderError, ProviderKind, SecretStore, ShortcutCombo, ShortcutConfig, SpeechAudio,
    TranslateInput, TranslateRequest, TranslationMode, Translator, TranslatorClient,
    UpdateCheckResult, UpdaterClient, VocabEntry, VocabMatcher, find_vocab_match_index,
    history::push_bounded_history, infer_audio_capability, infer_vision_capability,
    is_dedicated_tts_model, resolve_bidirectional_direction, resolve_voice_for_profile,
};
use platform::audio::AudioPlayer;
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
use ui::rtl_editor;
use ui::snip_overlay::{SnipEvent, SnipState, view_snip_overlay};
use ui::theme::transparent_iced_theme_for;
use ui::voice_player::{VoicePlayerMsg, VoicePlayerState, view_voice_player};

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
    modal_id: Option<window::Id>,
    modal_opened_at: Option<Instant>,
    modal_has_focused: bool,
    modal_user_dragged: bool,
    snip_id: Option<window::Id>,
    hud_id: Option<window::Id>,
    voice_player_id: Option<window::Id>,
    voice_player_opened_at: Option<Instant>,
    voice_player_user_dragged: bool,

    // Window states
    dashboard: DashboardState,
    modal: Option<ModalState>,
    snip: Option<SnipState>,
    hud: Option<HudState>,
    hud_expires_at: Option<Instant>,
    voice_player: Option<VoicePlayerState>,
    pending_replace_prev_clip: Option<String>,
    pending_target_hwnd: isize,
    translation_seq: u64,
    voice_gen_seq: u64,
    audio_player: AudioPlayer,
    pending_voice_launch: Option<PendingModalLaunch>,
}

#[derive(Debug, Clone)]
enum Message {
    Bridge(BridgeEvent),
    WindowOpened(WindowKind, window::Id),
    DashboardMonitorSize(Option<Size>),
    WindowEventOccurred(window::Id, window::Event),
    GlobalEscPressed,
    HudTick,
    AudioTick,
    CancelVoiceGeneration,

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
    DoubleShiftCaptured {
        target_hwnd: isize,
        anchor_phys: Option<(i32, i32, i32)>,
        cursor_phys: Option<(i32, i32)>,
        result: Result<platform::clipboard::CapturedSelection, String>,
    },
    ModalTranslationFinished {
        seq: u64,
        effective_target: String,
        was_auto_reversed: bool,
        result: Result<ModalTranslationOutcome, TranslationFail>,
        mode: TranslationMode,
    },
    ModalVoiceGenerated {
        seq: u64,
        result: Result<(SpeechAudio, String), String>,
    },
    ManualVoiceGenerated {
        seq: u64,
        result: Result<(SpeechAudio, String), String>,
    },
    InPlaceFlowFinished(Result<(String, String, Option<String>), String>),
    ManualTranslateFinished {
        seq: u64,
        effective_target: String,
        was_auto_reversed: bool,
        result: Result<ModalTranslationOutcome, TranslationFail>,
    },
    TestVoiceFinished(Result<(SpeechAudio, String), String>),
    ProbeModelFinished(Result<(Option<bool>, Option<bool>, String), String>),
    FetchModelsFinished(Result<Vec<ModelCatalogEntry>, String>),
    UpdateCheckFinished(Result<UpdateCheckResult, String>),
    AutoUpdateCheckFinished(Option<Result<UpdateCheckResult, String>>),
    PeriodicUpdateCheck,
    UpdateDownloadProgress(Option<u8>),
    UpdateDownloadFinished(Result<std::path::PathBuf, String>),

    // Sub-views
    Dashboard(DashboardMsg),
    Modal(ModalMsg),
    Snip(SnipEvent),
    VoicePlayer(VoicePlayerMsg),
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
    VoicePlayer,
}

fn classify_window_role(
    target: window::Id,
    dashboard_id: Option<window::Id>,
    modal_id: Option<window::Id>,
    snip_id: Option<window::Id>,
    hud_id: Option<window::Id>,
    voice_player_id: Option<window::Id>,
) -> Option<WindowKind> {
    if Some(target) == modal_id {
        Some(WindowKind::Modal)
    } else if Some(target) == snip_id {
        Some(WindowKind::Snip)
    } else if Some(target) == hud_id {
        Some(WindowKind::Hud)
    } else if Some(target) == voice_player_id {
        Some(WindowKind::VoicePlayer)
    } else if Some(target) == dashboard_id {
        Some(WindowKind::Dashboard)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct TranslationFail {
    is_vision: bool,
    message: String,
}

#[derive(Debug, Clone)]
struct ModalTranslationOutcome {
    translated_text: String,
    detected_source_lang: Option<String>,
    voice_audio: Option<SpeechAudio>,
    voice_used: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct PendingModalLaunch {
    seq: u64,
    anchor_phys: Option<(i32, i32, i32)>,
    cursor_phys: Option<(i32, i32)>,
}

struct ResolvedTranslationPipeline {
    text_profile: ModelProfile,
    text_key: Option<String>,
    voice: Option<(ModelProfile, Option<String>, String)>,
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
        let mut dashboard_id = None;

        if !start_minimized {
            let (id, task) = open_dashboard_task(app_icon.clone());
            dashboard_id = Some(id);
            tasks.push(task);
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
            dashboard_id,
            modal_id: None,
            modal_opened_at: None,
            modal_has_focused: false,
            modal_user_dragged: false,
            snip_id: None,
            hud_id: None,
            voice_player_id: None,
            voice_player_opened_at: None,
            voice_player_user_dragged: false,
            dashboard,
            modal: None,
            snip: None,
            hud: None,
            hud_expires_at: None,
            voice_player: None,
            pending_replace_prev_clip: None,
            pending_target_hwnd: 0,
            translation_seq: 0,
            voice_gen_seq: 0,
            audio_player: AudioPlayer::new(),
            pending_voice_launch: None,
        };

        if app.config.auto_check_updates {
            tasks.push(app.spawn_auto_update_check());
        }

        (app, Task::batch(tasks))
    }

    fn title(&self, window: window::Id) -> String {
        match self.window_role(window) {
            Some(WindowKind::Modal) => "Langhian Quick Translate".to_string(),
            Some(WindowKind::Snip) => "Langhian Screenshot Capture".to_string(),
            Some(WindowKind::Hud) => "Langhian HUD".to_string(),
            Some(WindowKind::VoicePlayer) => self.i18n.t("voice_player.title"),
            Some(WindowKind::Dashboard) | None => format!(
                "{} — {}",
                self.i18n.t("app.title"),
                self.i18n.t("app.subtitle")
            ),
        }
    }

    fn window_role(&self, target: window::Id) -> Option<WindowKind> {
        classify_window_role(
            target,
            self.dashboard_id,
            self.modal_id,
            self.snip_id,
            self.hud_id,
            self.voice_player_id,
        )
    }

    fn theme(&self, _window: window::Id) -> Theme {
        transparent_iced_theme_for(self.config.theme)
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            Subscription::run(bridge_stream).map(Message::Bridge),
            window::events().map(|(id, ev)| Message::WindowEventOccurred(id, ev)),
        ];

        if self.hud_expires_at.is_some() || self.hud.as_ref().is_some_and(|h| h.is_loading) {
            subs.push(iced::time::every(Duration::from_millis(250)).map(|_| Message::HudTick));
        }

        if self.modal.as_ref().is_some_and(|m| m.voice_is_playing)
            || self.dashboard.manual_voice_playing
            || self.voice_player.as_ref().is_some_and(|v| v.is_playing)
        {
            subs.push(iced::time::every(Duration::from_millis(250)).map(|_| Message::AudioTick));
        }

        if self.modal_id.is_some()
            || self.snip_id.is_some()
            || self.pending_voice_launch.is_some()
            || self.hud.as_ref().is_some_and(|h| h.is_loading)
        {
            subs.push(keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(Named::Escape),
                    ..
                } => Some(Message::GlobalEscPressed),
                _ => None,
            }));
        }

        if self.config.auto_check_updates {
            subs.push(
                iced::time::every(Duration::from_secs(5 * 60))
                    .map(|_| Message::PeriodicUpdateCheck),
            );
        }

        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Bridge(BridgeEvent::Input(act)) => self.handle_input_action(act),
            Message::Bridge(BridgeEvent::Tray(act)) => self.handle_tray_action(act),

            Message::WindowOpened(kind, id) => {
                // Verify synchronous ownership: if a floating/transient window was superseded or
                // dismissed before WindowOpened arrived, close this orphan id immediately.
                let still_expected = match kind {
                    WindowKind::Dashboard => self.dashboard_id == Some(id),
                    WindowKind::Modal => self.modal_id == Some(id),
                    WindowKind::Snip => self.snip_id == Some(id),
                    WindowKind::Hud => self.hud_id == Some(id),
                    WindowKind::VoicePlayer => self.voice_player_id == Some(id),
                };
                if !still_expected {
                    return window::close(id);
                }
                if kind == WindowKind::Dashboard {
                    return window::monitor_size(id).map(Message::DashboardMonitorSize);
                }
                if kind == WindowKind::Modal {
                    self.modal_opened_at = Some(Instant::now());
                    self.modal_has_focused = false;
                    self.modal_user_dragged = false;
                    return window::gain_focus(id);
                }
                if kind == WindowKind::VoicePlayer {
                    self.voice_player_opened_at = Some(Instant::now());
                    self.voice_player_user_dragged = false;
                }
                Task::none()
            }

            Message::DashboardMonitorSize(maybe_size) => {
                if !self.dashboard.sidebar_user_override
                    && let Some(size) = maybe_size
                    && size.width <= 1280.0
                {
                    self.dashboard.sidebar_collapsed = true;
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
                    } else if Some(id) == self.voice_player_id {
                        let settled = self.voice_player_user_dragged
                            || self
                                .voice_player_opened_at
                                .map(|t| t.elapsed() >= Duration::from_millis(350))
                                .unwrap_or(true);
                        if settled {
                            self.config.voice_player_position = Some([pos.x, pos.y]);
                        }
                    }
                    Task::none()
                }
                window::Event::Resized(size) => {
                    if Some(id) == self.dashboard_id && !self.dashboard.sidebar_user_override {
                        self.dashboard.sidebar_collapsed = size.width < 960.0;
                    }
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
                let (source_text, prev_clip) = match result {
                    Ok(c) => (c.text, c.previous_clipboard),
                    Err(_) => (String::new(), None),
                };
                self.pending_target_hwnd = target_hwnd;
                self.pending_replace_prev_clip = prev_clip;
                let modal = ModalState::for_text(
                    source_text,
                    self.config.modal_pinned,
                    &self.config.native_language,
                );
                self.open_or_update_modal(
                    modal,
                    TranslationMode::Modal,
                    false,
                    anchor_phys,
                    cursor_phys,
                )
            }

            Message::DoubleShiftCaptured {
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
                self.open_or_update_modal(
                    modal,
                    TranslationMode::Modal,
                    true,
                    anchor_phys,
                    cursor_phys,
                )
            }

            Message::ModalVoiceGenerated { seq, result } => {
                if seq != self.voice_gen_seq {
                    return Task::none();
                }
                let Some(modal) = self.modal.as_mut() else {
                    return Task::none();
                };
                modal.voice_generating = false;
                match result {
                    Ok((audio, voice_used)) => {
                        self.audio_player.stop();
                        self.mark_all_audio_stopped();
                        match self.audio_player.play(&audio) {
                            Ok(()) => {
                                if let Some(m) = self.modal.as_mut() {
                                    m.voice_is_playing = true;
                                    m.voice_used = Some(voice_used);
                                    m.voice_audio = Some(audio);
                                }
                                Task::none()
                            }
                            Err(err) => self.show_hud_toast(
                                self.i18n.t("app.status_error"),
                                Some(err),
                                true,
                            ),
                        }
                    }
                    Err(err) => {
                        self.show_hud_toast(self.i18n.t("app.status_error"), Some(err), true)
                    }
                }
            }

            Message::ManualVoiceGenerated { seq, result } => {
                if seq != self.voice_gen_seq {
                    return Task::none();
                }
                self.dashboard.manual_voice_generating = false;
                match result {
                    Ok((audio, voice_used)) => {
                        self.audio_player.stop();
                        self.mark_all_audio_stopped();
                        match self.audio_player.play(&audio) {
                            Ok(()) => {
                                self.dashboard.manual_voice_playing = true;
                                self.dashboard.manual_voice_used = Some(voice_used);
                                self.dashboard.manual_voice_audio = Some(audio);
                                self.dashboard.translate_error = None;
                            }
                            Err(err) => {
                                self.dashboard.manual_voice_playing = false;
                                self.dashboard.translate_error = Some(err);
                            }
                        }
                    }
                    Err(err) => {
                        self.dashboard.manual_voice_playing = false;
                        self.dashboard.translate_error = Some(err);
                    }
                }
                Task::none()
            }

            Message::GlobalEscPressed => {
                if let Some(id) = self.snip_id.take() {
                    self.snip = None;
                    return window::close(id);
                }
                if self.pending_voice_launch.take().is_some()
                    || self.hud.as_ref().is_some_and(|h| h.is_loading)
                {
                    self.audio_player.stop();
                    self.mark_all_audio_stopped();
                    self.translation_seq = self.translation_seq.wrapping_add(1);
                    self.voice_gen_seq = self.voice_gen_seq.wrapping_add(1);
                    if self.modal_id.is_none() {
                        self.modal = None;
                    }
                    return self.dismiss_loading_hud();
                }
                if self.modal_id.is_some() {
                    return self.close_modal();
                }
                Task::none()
            }

            Message::CancelVoiceGeneration => {
                self.pending_voice_launch = None;
                self.audio_player.stop();
                self.mark_all_audio_stopped();
                self.translation_seq = self.translation_seq.wrapping_add(1);
                self.voice_gen_seq = self.voice_gen_seq.wrapping_add(1);
                if self.modal_id.is_none() {
                    self.modal = None;
                }
                self.dismiss_loading_hud()
            }

            Message::HudTick => {
                if let Some(h) = self.hud.as_mut()
                    && h.is_loading
                {
                    h.anim_phase = h.anim_phase.wrapping_add(1);
                }
                if let Some(exp) = self.hud_expires_at
                    && Instant::now() >= exp
                {
                    return self.dismiss_hud();
                }
                Task::none()
            }

            Message::AudioTick => {
                let playing = self.audio_player.is_playing();
                if playing {
                    if let Some(vp) = self.voice_player.as_mut()
                        && vp.is_playing
                    {
                        vp.anim_phase = vp.anim_phase.wrapping_add(1);
                    }
                } else {
                    self.mark_all_audio_stopped();
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
                let pending_launch = self
                    .pending_voice_launch
                    .take()
                    .filter(|pending| pending.seq == seq);

                if let Some(launch) = pending_launch
                    && self.modal_id.is_none()
                {
                    let src_record = self
                        .modal
                        .take()
                        .map(|m| {
                            if m.screenshot_png.is_some() {
                                "[Screenshot]".to_string()
                            } else {
                                m.source_text
                            }
                        })
                        .unwrap_or_default();

                    match result {
                        Ok(outcome) => {
                            let translated = outcome.translated_text;
                            self.record_history(mode, src_record, translated, effective_target);

                            if let Some(audio) = outcome.voice_audio {
                                self.audio_player.stop();
                                self.mark_all_audio_stopped();
                                match self.audio_player.play(&audio) {
                                    Ok(()) => {
                                        let model_name = self
                                            .config
                                            .resolved_voice_model()
                                            .map(|m| m.name.clone())
                                            .unwrap_or_default();
                                        let voice_name = outcome
                                            .voice_used
                                            .unwrap_or_else(|| self.config.voice_name.clone());
                                        self.voice_player = Some(VoicePlayerState::new(
                                            audio, model_name, voice_name,
                                        ));
                                        let dismiss_task = self.dismiss_loading_hud();
                                        let vp_task = self.open_voice_player_at(
                                            launch.anchor_phys,
                                            launch.cursor_phys,
                                        );
                                        return Task::batch([dismiss_task, vp_task]);
                                    }
                                    Err(err) => {
                                        return self.show_hud_toast(
                                            self.i18n.t("app.status_error"),
                                            Some(err),
                                            true,
                                        );
                                    }
                                }
                            }
                            return self.dismiss_loading_hud();
                        }
                        Err(fail) => {
                            return self.show_hud_toast(
                                self.i18n.t("app.status_error"),
                                Some(fail.message),
                                true,
                            );
                        }
                    }
                }

                let dismiss_task = self.dismiss_loading_hud();
                let Some(modal) = self.modal.as_mut() else {
                    return dismiss_task;
                };
                modal.is_loading = false;
                modal.effective_target_lang = effective_target.clone();
                modal.was_auto_reversed = was_auto_reversed;
                match result {
                    Ok(outcome) => {
                        let translated = outcome.translated_text;
                        modal.translated_text = translated.clone();
                        modal.output_editor = rtl_editor::Content::with_text(&translated);
                        modal.output_force_selectable = false;
                        modal.detected_source_lang = outcome.detected_source_lang;
                        modal.error_message = None;
                        modal.is_vision_error = false;
                        modal.copied_output = false;
                        modal.voice_used = outcome.voice_used;

                        if let Some(audio) = outcome.voice_audio {
                            self.audio_player.stop();
                            match self.audio_player.play(&audio) {
                                Ok(()) => {
                                    modal.voice_is_playing = true;
                                    self.dashboard.manual_voice_playing = false;
                                    if let Some(vp) = self.voice_player.as_mut() {
                                        vp.is_playing = false;
                                    }
                                }
                                Err(err) => {
                                    modal.error_message = Some(err);
                                    modal.voice_is_playing = false;
                                }
                            }
                            modal.voice_audio = Some(audio);
                        } else {
                            modal.voice_audio = None;
                            modal.voice_is_playing = false;
                        }

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
                        modal.voice_audio = None;
                        modal.voice_is_playing = false;
                    }
                }
                if self.modal_id.is_none()
                    && let Some(launch) = pending_launch
                {
                    let modal_task =
                        self.open_modal_window_at(launch.anchor_phys, launch.cursor_phys);
                    return Task::batch([dismiss_task, modal_task]);
                }
                dismiss_task
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
                    Ok(outcome) => {
                        let translated = outcome.translated_text;
                        self.dashboard.manual_output = translated.clone();
                        self.dashboard.translate_error = None;
                        self.dashboard.save_term = self.dashboard.source_editor.text();
                        self.dashboard.save_translation = translated.clone();
                        self.dashboard.manual_voice_used = outcome.voice_used;

                        if let Some(audio) = outcome.voice_audio {
                            self.audio_player.stop();
                            match self.audio_player.play(&audio) {
                                Ok(()) => {
                                    self.dashboard.manual_voice_playing = true;
                                    if let Some(m) = self.modal.as_mut() {
                                        m.voice_is_playing = false;
                                    }
                                    if let Some(vp) = self.voice_player.as_mut() {
                                        vp.is_playing = false;
                                    }
                                }
                                Err(err) => {
                                    self.dashboard.translate_error = Some(err);
                                    self.dashboard.manual_voice_playing = false;
                                }
                            }
                            self.dashboard.manual_voice_audio = Some(audio);
                        } else {
                            self.dashboard.manual_voice_audio = None;
                            self.dashboard.manual_voice_playing = false;
                        }

                        self.record_history(
                            TranslationMode::Manual,
                            self.dashboard.source_editor.text(),
                            translated,
                            effective_target,
                        );
                    }
                    Err(fail) => {
                        self.dashboard.translate_error = Some(fail.message);
                        self.dashboard.manual_voice_audio = None;
                        self.dashboard.manual_voice_playing = false;
                        self.dashboard.manual_voice_used = None;
                    }
                }
                self.dismiss_loading_hud()
            }

            Message::TestVoiceFinished(res) => {
                self.dashboard.voice_testing = false;
                match res {
                    Ok((audio, voice_used)) => {
                        self.audio_player.stop();
                        self.dashboard.manual_voice_playing = false;
                        if let Some(m) = self.modal.as_mut() {
                            m.voice_is_playing = false;
                        }
                        if let Err(err) = self.audio_player.play(&audio) {
                            self.dashboard.settings_error = Some(err);
                        } else {
                            self.dashboard.settings_error = None;
                            self.dashboard.settings_toast = Some(format!(
                                "{} ({voice_used})",
                                self.i18n.t("translate.voice_playing")
                            ));
                        }
                    }
                    Err(e) => {
                        self.dashboard.settings_error = Some(e);
                    }
                }
                Task::none()
            }

            Message::ProbeModelFinished(res) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.is_probing = false;
                    match res {
                        Ok((vision, audio, msg)) => {
                            if let Some(vis) = vision {
                                ed.supports_vision = vis;
                                ed.vision_source = langhian_core::CapabilitySource::AutoDetected;
                            }
                            if let Some(aud) = audio {
                                ed.supports_audio = aud;
                                ed.audio_source = langhian_core::CapabilitySource::AutoDetected;
                            }
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

            Message::AutoUpdateCheckFinished(opt) => {
                self.dashboard.update_checking = false;
                if let Some(res) = opt {
                    match res {
                        Ok(r) => {
                            self.dashboard.update_error = None;
                            self.dashboard.update_status = Some(r);
                        }
                        Err(e) => {
                            self.dashboard.update_error = Some(e);
                        }
                    }
                }
                Task::none()
            }

            Message::PeriodicUpdateCheck => {
                if self.config.auto_check_updates {
                    self.spawn_auto_update_check()
                } else {
                    Task::none()
                }
            }

            Message::UpdateDownloadProgress(pct) => {
                self.dashboard.update_download_progress = pct;
                Task::none()
            }

            Message::UpdateDownloadFinished(res) => {
                self.dashboard.update_downloading = false;
                self.dashboard.update_download_progress = None;
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
            Message::VoicePlayer(msg) => self.update_voice_player(msg),
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
        match self.window_role(window) {
            Some(WindowKind::Modal) => {
                if let Some(modal) = &self.modal {
                    return view_modal(
                        modal,
                        &self.config,
                        &self.i18n,
                        &self.vocab,
                        &self.vocab_matcher,
                    )
                    .map(Message::Modal);
                }
            }
            Some(WindowKind::Snip) => {
                if let Some(snip) = &self.snip {
                    return view_snip_overlay(
                        snip,
                        format!("{} · Esc", self.i18n.t("translate.screenshot_btn")),
                        Message::Snip,
                    );
                }
            }
            Some(WindowKind::Hud) => {
                if let Some(hud) = &self.hud {
                    let on_cancel = hud.is_loading.then_some(Message::CancelVoiceGeneration);
                    return view_hud(hud, self.config.theme, &self.i18n, on_cancel);
                }
            }
            Some(WindowKind::VoicePlayer) => {
                if let Some(vp) = &self.voice_player {
                    return view_voice_player(vp, self.config.theme, &self.i18n)
                        .map(Message::VoicePlayer);
                }
            }
            Some(WindowKind::Dashboard) => {
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
                return content.map(Message::Dashboard);
            }
            None => {}
        }

        // Unowned or closing/initializing transient window IDs must NEVER render the dashboard chrome.
        container(iced::widget::Space::new())
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
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
        if Some(id) == self.voice_player_id {
            return self.close_voice_player();
        }
        if Some(id) == self.dashboard_id {
            self.dashboard_id = None;
            self.dashboard.is_maximized = false;
            return window::close(id);
        }
        window::close(id)
    }

    fn close_modal(&mut self) -> Task<Message> {
        self.audio_player.stop();
        self.mark_all_audio_stopped();
        self.voice_gen_seq = self.voice_gen_seq.wrapping_add(1);
        self.pending_voice_launch = None;
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

    fn close_voice_player(&mut self) -> Task<Message> {
        self.audio_player.stop();
        self.mark_all_audio_stopped();
        self.voice_player = None;
        self.voice_player_opened_at = None;
        self.voice_player_user_dragged = false;
        let _ = self.storage.save_config(&self.config);
        if let Some(id) = self.voice_player_id.take() {
            window::close(id)
        } else {
            Task::none()
        }
    }

    fn mark_all_audio_stopped(&mut self) {
        if let Some(m) = self.modal.as_mut() {
            m.voice_is_playing = false;
        }
        self.dashboard.manual_voice_playing = false;
        if let Some(vp) = self.voice_player.as_mut() {
            vp.is_playing = false;
        }
    }

    fn update_voice_player(&mut self, msg: VoicePlayerMsg) -> Task<Message> {
        match msg {
            VoicePlayerMsg::DragWindow => {
                self.voice_player_user_dragged = true;
                if let Some(id) = self.voice_player_id {
                    window::drag(id)
                } else {
                    Task::none()
                }
            }
            VoicePlayerMsg::TogglePlay => {
                let Some(vp) = self.voice_player.as_mut() else {
                    return Task::none();
                };
                if vp.is_playing {
                    self.audio_player.stop();
                    vp.is_playing = false;
                    return Task::none();
                }
                let audio = vp.audio.clone();
                self.audio_player.stop();
                match self.audio_player.play(&audio) {
                    Ok(()) => {
                        if let Some(m) = self.modal.as_mut() {
                            m.voice_is_playing = false;
                        }
                        self.dashboard.manual_voice_playing = false;
                        if let Some(vp) = self.voice_player.as_mut() {
                            vp.is_playing = true;
                        }
                        Task::none()
                    }
                    Err(err) => {
                        self.show_hud_toast(self.i18n.t("app.status_error"), Some(err), true)
                    }
                }
            }
            VoicePlayerMsg::Minimize => {
                if let Some(id) = self.voice_player_id {
                    window::minimize(id, true)
                } else {
                    Task::none()
                }
            }
            VoicePlayerMsg::Cancel => self.close_voice_player(),
            VoicePlayerMsg::CloseHovered(hovered) => {
                if let Some(vp) = self.voice_player.as_mut() {
                    vp.close_hovered = hovered;
                }
                Task::none()
            }
        }
    }

    fn handle_input_action(&mut self, action: InputAction) -> Task<Message> {
        match action {
            InputAction::DoubleTapAlt => self.run_double_alt_inplace(),
            InputAction::DoubleTapCtrl => self.run_double_ctrl_modal(),
            InputAction::DoubleTapShift => self.run_double_shift_voice(),
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
            TrayAction::SelectVoiceModel(id) => {
                self.config.active_voice_model_id = Some(id);
                if let Some(vp) = self.config.resolved_voice_model().cloned() {
                    self.config.voice_name =
                        resolve_voice_for_profile(&vp, &self.config.voice_name);
                }
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            TrayAction::SelectVisionModel(id) => {
                self.config.active_vision_model_id = Some(id);
                self.config.ensure_valid();
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
        } else {
            let (id, task) = open_dashboard_task(self.app_icon.clone());
            self.dashboard_id = Some(id);
            task
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

    fn run_double_shift_voice(&mut self) -> Task<Message> {
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
            move |result| Message::DoubleShiftCaptured {
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

        let (id, open) = window::open(window::Settings {
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
        self.snip_id = Some(id);
        open.map(|id| Message::WindowOpened(WindowKind::Snip, id))
    }

    fn open_or_update_modal(
        &mut self,
        modal: ModalState,
        mode: TranslationMode,
        with_voice: bool,
        anchor_phys: Option<(i32, i32, i32)>,
        cursor_phys: Option<(i32, i32)>,
    ) -> Task<Message> {
        self.audio_player.stop();
        self.mark_all_audio_stopped();
        let has_content = modal.screenshot_png.is_some() || !modal.source_text.trim().is_empty();
        self.modal = Some(modal);
        let translate_task = if has_content {
            self.dispatch_modal_translation(mode, with_voice)
        } else {
            Task::none()
        };
        let immediate_error = self.modal.as_ref().and_then(|m| m.error_message.clone());

        if with_voice && self.modal_id.is_none() {
            if let Some(err) = immediate_error {
                self.modal = None;
                self.pending_voice_launch = None;
                return self.show_hud_toast(self.i18n.t("app.status_error"), Some(err), true);
            }
            self.pending_voice_launch = Some(PendingModalLaunch {
                seq: self.translation_seq,
                anchor_phys,
                cursor_phys,
            });
            let loader_task = self.show_loading_hud(self.i18n.t("hud.generating_voice"), None);
            return Task::batch([loader_task, translate_task]);
        }

        self.pending_voice_launch = None;
        let window_task = self.open_modal_window_at(anchor_phys, cursor_phys);
        Task::batch([window_task, translate_task])
    }

    fn open_voice_player_at(
        &mut self,
        anchor_phys: Option<(i32, i32, i32)>,
        cursor_phys: Option<(i32, i32)>,
    ) -> Task<Message> {
        const PLAYER_W: f32 = 320.0;
        const PLAYER_H: f32 = 66.0;
        let pos_pt = compute_smart_modal_position(
            self.config.voice_player_position,
            anchor_phys,
            cursor_phys,
            PLAYER_W,
            PLAYER_H,
        );

        if let Some(id) = self.voice_player_id {
            return Task::batch([window::move_to(id, pos_pt), window::minimize(id, false)]);
        }

        let (id, open) = window::open(window::Settings {
            size: Size::new(PLAYER_W, PLAYER_H),
            position: window::Position::Specific(pos_pt),
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            resizable: false,
            icon: self.app_icon.clone(),
            exit_on_close_request: false,
            platform_specific: dashboard_platform_specific(),
            ..Default::default()
        });
        self.voice_player_id = Some(id);
        self.voice_player_opened_at = Some(Instant::now());
        self.voice_player_user_dragged = false;

        open.map(|id| Message::WindowOpened(WindowKind::VoicePlayer, id))
    }

    fn open_modal_window_at(
        &mut self,
        anchor_phys: Option<(i32, i32, i32)>,
        cursor_phys: Option<(i32, i32)>,
    ) -> Task<Message> {
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
            return Task::batch([window::move_to(id, pos_pt), window::gain_focus(id)]);
        }

        let (id, open) = window::open(window::Settings {
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
        self.modal_id = Some(id);
        self.modal_opened_at = Some(Instant::now());
        self.modal_has_focused = false;
        self.modal_user_dragged = false;

        open.map(|id| Message::WindowOpened(WindowKind::Modal, id))
    }

    fn resolve_text_profile_for_voice(
        &self,
        mut text_profile: ModelProfile,
        requires_vision: bool,
    ) -> (ModelProfile, Option<String>) {
        if !is_dedicated_tts_model(text_profile.provider, &text_profile.model_id)
            && (!requires_vision || text_profile.supports_vision)
        {
            let key = self.secrets.get_api_key(&text_profile.id);
            return (text_profile, key);
        }

        if let Some(m) = self
            .config
            .models
            .iter()
            .filter(|m| {
                !is_dedicated_tts_model(m.provider, &m.model_id)
                    && (!requires_vision || m.supports_vision)
                    && (!m.requires_api_key
                        || self
                            .secrets
                            .get_api_key(&m.id)
                            .is_some_and(|k| !k.trim().is_empty()))
            })
            .min_by_key(|m| {
                if m.provider == ProviderKind::GoogleTranslateFree {
                    1
                } else {
                    0
                }
            })
            .cloned()
        {
            let key = self.secrets.get_api_key(&m.id);
            return (m, key);
        }

        if !requires_vision && text_profile.provider == ProviderKind::Gemini {
            let key = self.secrets.get_api_key(&text_profile.id);
            if key.as_deref().is_some_and(|k| !k.trim().is_empty()) {
                let mut fallback = ModelProfile::preset_gemini();
                fallback.model_id = "gemini-2.5-flash".to_string();
                return (fallback, key);
            }
        }

        if !requires_vision {
            return (ModelProfile::google_translate_free(), None);
        }

        text_profile.supports_vision = false;
        (text_profile, None)
    }

    fn resolve_translation_pipeline(
        &self,
        requires_vision: bool,
        with_voice: bool,
    ) -> Result<ResolvedTranslationPipeline, TranslationFail> {
        let vision_msg = self.i18n.t("errors.vision_unsupported");
        let audio_msg = self.i18n.t("errors.audio_unsupported");

        if requires_vision {
            let Some(vis_profile) = self.config.resolved_vision_model().cloned() else {
                return Err(TranslationFail {
                    is_vision: true,
                    message: vision_msg,
                });
            };
            let key = self.secrets.get_api_key(&vis_profile.id);
            let voice = if with_voice {
                let Some(vp) = self.config.resolved_voice_model().cloned() else {
                    return Err(TranslationFail {
                        is_vision: false,
                        message: audio_msg,
                    });
                };
                let vk = self.secrets.get_api_key(&vp.id);
                let vn = resolve_voice_for_profile(&vp, &self.config.voice_name);
                Some((vp, vk, vn))
            } else {
                None
            };
            return Ok(ResolvedTranslationPipeline {
                text_profile: vis_profile,
                text_key: key,
                voice,
            });
        }

        let base_profile = self.config.active_model().clone();

        if !with_voice {
            let key = self.secrets.get_api_key(&base_profile.id);
            return Ok(ResolvedTranslationPipeline {
                text_profile: base_profile,
                text_key: key,
                voice: None,
            });
        }

        let Some(voice_profile) = self.config.resolved_voice_model().cloned() else {
            return Err(TranslationFail {
                is_vision: false,
                message: audio_msg,
            });
        };
        if !voice_profile.supports_audio_output() {
            return Err(TranslationFail {
                is_vision: false,
                message: audio_msg,
            });
        }

        let (text_profile, text_key) = self.resolve_text_profile_for_voice(base_profile, false);

        let voice_key = self.secrets.get_api_key(&voice_profile.id);
        let voice_name = resolve_voice_for_profile(&voice_profile, &self.config.voice_name);
        Ok(ResolvedTranslationPipeline {
            text_profile,
            text_key,
            voice: Some((voice_profile, voice_key, voice_name)),
        })
    }

    fn dispatch_modal_translation(
        &mut self,
        mode: TranslationMode,
        with_voice: bool,
    ) -> Task<Message> {
        let vision_msg = self.i18n.t("errors.vision_unsupported");
        let key_msg = self.i18n.t("errors.missing_api_key");
        let rate_msg = self.i18n.t("errors.rate_limited");
        let audio_msg = self.i18n.t("errors.audio_unsupported");

        let Some((src_text, screenshot_png)) = self
            .modal
            .as_ref()
            .map(|m| (m.source_text.clone(), m.screenshot_png.clone()))
        else {
            return Task::none();
        };

        let requires_vision = screenshot_png.is_some();
        let (input, effective_source, effective_target, was_auto_reversed) =
            if let Some(png) = screenshot_png {
                (
                    TranslateInput::ImagePng {
                        bytes: png,
                        context_hint: None,
                    },
                    "auto".to_string(),
                    self.config.native_language.clone(),
                    false,
                )
            } else {
                let res = resolve_bidirectional_direction(
                    &src_text,
                    &self.config.native_language,
                    &self.config.target_language,
                    true,
                    None,
                );
                (
                    TranslateInput::Text(src_text),
                    res.effective_source,
                    res.effective_target,
                    res.was_reversed,
                )
            };

        let pipeline = self.resolve_translation_pipeline(requires_vision, with_voice);
        let Some(modal) = self.modal.as_mut() else {
            return Task::none();
        };
        modal.effective_target_lang = effective_target.clone();
        modal.was_auto_reversed = was_auto_reversed;
        modal.voice_mode = with_voice;
        modal.voice_audio = None;
        modal.voice_used = None;
        modal.voice_is_playing = false;
        modal.voice_generating = false;

        let pipeline = match pipeline {
            Ok(parts) => parts,
            Err(fail) => {
                modal.is_loading = false;
                modal.is_vision_error = fail.is_vision;
                modal.error_message = Some(fail.message);
                return Task::none();
            }
        };

        self.translation_seq = self.translation_seq.wrapping_add(1);
        let seq = self.translation_seq;

        modal.is_loading = true;
        modal.error_message = None;
        modal.is_vision_error = false;
        modal.copied_output = false;

        let req = TranslateRequest {
            input,
            source_lang: effective_source,
            target_lang: effective_target.clone(),
            prompt: self.config.active_prompt().clone(),
            preserve_markdown: self.config.preserve_markdown,
        };
        let client = self.client.clone();

        Task::perform(
            run_translation_request(
                client, pipeline, req, vision_msg, key_msg, rate_msg, audio_msg,
            ),
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

    fn open_or_move_hud(&mut self, hud: HudState, expires_at: Option<Instant>) -> Task<Message> {
        self.hud = Some(hud);
        self.hud_expires_at = expires_at;

        let pos_pt = compute_hud_subtitle_position(current_cursor_pos(), 460.0, 56.0);
        if let Some(id) = self.hud_id {
            // Do NOT steal focus from the user's active app when updating HUD text.
            return window::move_to(id, pos_pt);
        }

        let (id, open) = window::open(window::Settings {
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
        self.hud_id = Some(id);
        open.map(|id| Message::WindowOpened(WindowKind::Hud, id))
    }

    fn show_hud_toast(
        &mut self,
        title: String,
        detail: Option<String>,
        is_error: bool,
    ) -> Task<Message> {
        self.open_or_move_hud(
            HudState::toast(title, detail, is_error),
            Some(Instant::now() + Duration::from_millis(1900)),
        )
    }

    fn show_loading_hud(&mut self, title: String, detail: Option<String>) -> Task<Message> {
        self.open_or_move_hud(HudState::loading(title, detail), None)
    }

    fn dismiss_hud(&mut self) -> Task<Message> {
        self.hud_expires_at = None;
        self.hud = None;
        if let Some(id) = self.hud_id.take() {
            window::close(id)
        } else {
            Task::none()
        }
    }

    fn dismiss_loading_hud(&mut self) -> Task<Message> {
        if self.hud.as_ref().is_some_and(|h| h.is_loading) {
            self.dismiss_hud()
        } else {
            Task::none()
        }
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
                            false,
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
            ModalMsg::TranslateOrRegenerate => self.dispatch_modal_translation(current_mode, false),
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
                if current_mode == TranslationMode::Screenshot {
                    self.config.active_vision_model_id = Some(id);
                } else {
                    self.config.active_model_id = id;
                }
                self.config.ensure_valid();
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                self.dispatch_modal_translation(current_mode, false)
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
            ModalMsg::PlayAudio => {
                let cached = self.modal.as_ref().and_then(|m| m.voice_audio.clone());
                if let Some(audio) = cached {
                    self.audio_player.stop();
                    self.mark_all_audio_stopped();
                    if let Some(m) = self.modal.as_mut() {
                        match self.audio_player.play(&audio) {
                            Ok(()) => m.voice_is_playing = true,
                            Err(e) => {
                                m.voice_is_playing = false;
                                m.error_message = Some(e);
                            }
                        }
                    }
                    return Task::none();
                }

                let Some((translated_text, already_generating)) = self
                    .modal
                    .as_ref()
                    .map(|m| (m.translated_text.clone(), m.voice_generating))
                else {
                    return Task::none();
                };
                if translated_text.trim().is_empty() || already_generating {
                    return Task::none();
                }

                let Some(voice_profile) = self.config.resolved_voice_model().cloned() else {
                    return self.show_hud_toast(
                        self.i18n.t("app.status_error"),
                        Some(self.i18n.t("errors.audio_unsupported")),
                        true,
                    );
                };
                let voice_key = self.secrets.get_api_key(&voice_profile.id);
                let voice_name = resolve_voice_for_profile(&voice_profile, &self.config.voice_name);

                self.voice_gen_seq = self.voice_gen_seq.wrapping_add(1);
                let seq = self.voice_gen_seq;
                if let Some(m) = self.modal.as_mut() {
                    m.voice_generating = true;
                }

                let client = self.client.clone();
                let key_msg = self.i18n.t("errors.missing_api_key");
                let audio_msg = self.i18n.t("errors.audio_unsupported");
                let rate_msg = self.i18n.t("errors.rate_limited");
                Task::perform(
                    async move {
                        client
                            .synthesize_speech(
                                &voice_profile,
                                voice_key.as_deref(),
                                &translated_text,
                                &voice_name,
                            )
                            .await
                            .map_err(|e| match e {
                                ProviderError::MissingApiKey { .. } => key_msg,
                                ProviderError::AudioUnsupported { .. } => audio_msg,
                                ProviderError::RateLimited => rate_msg,
                                other => other.to_string(),
                            })
                    },
                    move |result| Message::ModalVoiceGenerated { seq, result },
                )
            }
            ModalMsg::StopAudio => {
                self.audio_player.stop();
                self.dashboard.manual_voice_playing = false;
                if let Some(m) = self.modal.as_mut() {
                    m.voice_is_playing = false;
                }
                Task::none()
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
                self.dashboard.translate_open_picker = None;
                Task::none()
            }
            DashboardMsg::ToggleSidebar => {
                self.dashboard.sidebar_collapsed = !self.dashboard.sidebar_collapsed;
                self.dashboard.sidebar_user_override = true;
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
                    self.dashboard.source_editor = rtl_editor::Content::with_text(&prev_out);
                    self.dashboard.manual_output.clear();
                }
                self.audio_player.stop();
                self.dashboard.manual_voice_audio = None;
                self.dashboard.manual_voice_playing = false;
                self.dashboard.manual_voice_generating = false;
                self.dashboard.manual_voice_used = None;
                Task::none()
            }
            DashboardMsg::ToggleTranslatePicker(kind) => {
                self.dashboard.translate_open_picker =
                    if self.dashboard.translate_open_picker == Some(kind) {
                        None
                    } else {
                        Some(kind)
                    };
                Task::none()
            }
            DashboardMsg::SelectModel(id) => {
                self.config.active_model_id = id;
                self.dashboard.translate_open_picker = None;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::SelectPrompt(id) => {
                self.config.active_prompt_id = id;
                self.dashboard.translate_open_picker = None;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::RunManualTranslate => {
                let text_val = self.dashboard.source_editor.text();
                if text_val.trim().is_empty() {
                    return Task::none();
                }
                self.audio_player.stop();
                self.voice_gen_seq = self.voice_gen_seq.wrapping_add(1);
                self.dashboard.manual_voice_playing = false;
                self.dashboard.manual_voice_generating = false;
                self.dashboard.manual_voice_audio = None;
                self.dashboard.manual_voice_used = None;
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

                let pipeline = match self.resolve_translation_pipeline(false, false) {
                    Ok(parts) => parts,
                    Err(fail) => {
                        self.dashboard.is_translating = false;
                        self.dashboard.translate_error = Some(fail.message);
                        return Task::none();
                    }
                };
                let req = TranslateRequest {
                    input: TranslateInput::Text(text_val),
                    source_lang: effective_source,
                    target_lang: effective_target.clone(),
                    prompt: self.config.active_prompt().clone(),
                    preserve_markdown: self.config.preserve_markdown,
                };
                let has_voice = pipeline.voice.is_some();
                let client = self.client.clone();
                let vision_msg = self.i18n.t("errors.vision_unsupported");
                let key_msg = self.i18n.t("errors.missing_api_key");
                let rate_msg = self.i18n.t("errors.rate_limited");
                let audio_msg = self.i18n.t("errors.audio_unsupported");

                let hud_task = if has_voice {
                    self.show_loading_hud(self.i18n.t("hud.generating_voice"), None)
                } else {
                    Task::none()
                };

                let work_task = Task::perform(
                    run_translation_request(
                        client, pipeline, req, vision_msg, key_msg, rate_msg, audio_msg,
                    ),
                    move |result| Message::ManualTranslateFinished {
                        seq,
                        effective_target,
                        was_auto_reversed,
                        result,
                    },
                );
                Task::batch([hud_task, work_task])
            }
            DashboardMsg::PlayManualVoice => {
                if let Some(audio) = self.dashboard.manual_voice_audio.clone() {
                    self.audio_player.stop();
                    self.mark_all_audio_stopped();
                    match self.audio_player.play(&audio) {
                        Ok(()) => {
                            self.dashboard.manual_voice_playing = true;
                            self.dashboard.translate_error = None;
                        }
                        Err(e) => {
                            self.dashboard.manual_voice_playing = false;
                            self.dashboard.translate_error = Some(e);
                        }
                    }
                    return Task::none();
                }

                let translated = self.dashboard.manual_output.clone();
                if translated.trim().is_empty() || self.dashboard.manual_voice_generating {
                    return Task::none();
                }

                let Some(voice_profile) = self.config.resolved_voice_model().cloned() else {
                    self.dashboard.translate_error = Some(self.i18n.t("errors.audio_unsupported"));
                    return Task::none();
                };
                let voice_key = self.secrets.get_api_key(&voice_profile.id);
                let voice_name = resolve_voice_for_profile(&voice_profile, &self.config.voice_name);
                self.voice_gen_seq = self.voice_gen_seq.wrapping_add(1);
                let seq = self.voice_gen_seq;
                self.dashboard.manual_voice_generating = true;
                self.dashboard.translate_error = None;

                let client = self.client.clone();
                let key_msg = self.i18n.t("errors.missing_api_key");
                let audio_msg = self.i18n.t("errors.audio_unsupported");
                let rate_msg = self.i18n.t("errors.rate_limited");
                Task::perform(
                    async move {
                        client
                            .synthesize_speech(
                                &voice_profile,
                                voice_key.as_deref(),
                                &translated,
                                &voice_name,
                            )
                            .await
                            .map_err(|e| match e {
                                ProviderError::MissingApiKey { .. } => key_msg,
                                ProviderError::AudioUnsupported { .. } => audio_msg,
                                ProviderError::RateLimited => rate_msg,
                                other => other.to_string(),
                            })
                    },
                    move |result| Message::ManualVoiceGenerated { seq, result },
                )
            }
            DashboardMsg::StopManualVoice => {
                self.audio_player.stop();
                self.dashboard.manual_voice_playing = false;
                Task::none()
            }
            DashboardMsg::TriggerScreenshot => self.start_screenshot_capture(),
            DashboardMsg::ClearManual => {
                self.audio_player.stop();
                self.dashboard.source_editor = rtl_editor::Content::new();
                self.dashboard.manual_output.clear();
                self.dashboard.manual_voice_audio = None;
                self.dashboard.manual_voice_playing = false;
                self.dashboard.manual_voice_used = None;
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
                    self.dashboard.save_notes_editor = rtl_editor::Content::new();
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
                    if let Some(key) = self.secrets.get_api_key(&id)
                        && !key.trim().is_empty()
                    {
                        let _ = self.secrets.set_api_key(&copy.id, &key);
                    }
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
                            ed.supports_audio = false;
                        }
                        ProviderKind::OpenAiCompatible => {
                            if ed.base_url.is_empty()
                                || ed.base_url.contains("anthropic")
                                || ed.base_url.contains("generativelanguage")
                            {
                                ed.base_url = "https://openrouter.ai/api/v1".to_string();
                            }
                            ed.supports_vision = infer_vision_capability(kind, &ed.model_id);
                            ed.supports_audio = infer_audio_capability(kind, &ed.model_id);
                        }
                        ProviderKind::Anthropic => {
                            ed.base_url = "https://api.anthropic.com".to_string();
                            if ed.model_id.is_empty() || ed.model_id == "gtx-free" {
                                ed.model_id = "claude-sonnet-4-20250514".to_string();
                            }
                            ed.supports_vision = infer_vision_capability(kind, &ed.model_id);
                            ed.supports_audio = false;
                        }
                        ProviderKind::Gemini => {
                            ed.base_url = "https://generativelanguage.googleapis.com".to_string();
                            if ed.model_id.is_empty() || ed.model_id == "gtx-free" {
                                ed.model_id = "gemini-2.5-flash".to_string();
                            }
                            ed.supports_vision = infer_vision_capability(kind, &ed.model_id);
                            ed.supports_audio = infer_audio_capability(kind, &ed.model_id);
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
                    if ed.audio_source != langhian_core::CapabilitySource::ManualOverride {
                        ed.supports_audio = infer_audio_capability(ed.provider, &v);
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
            DashboardMsg::ToggleModelAudio(v) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.supports_audio = v;
                    ed.audio_source = langhian_core::CapabilitySource::ManualOverride;
                }
                Task::none()
            }
            DashboardMsg::ModelCatalogFilterChanged(q) => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.catalog_filter = q;
                }
                Task::none()
            }
            DashboardMsg::ToggleCatalogAudioOnly => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.catalog_audio_only = !ed.catalog_audio_only;
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
                    if let Some(aud) = entry.supports_audio {
                        ed.supports_audio = aud;
                        ed.audio_source = langhian_core::CapabilitySource::AutoDetected;
                    } else if ed.audio_source != langhian_core::CapabilitySource::ManualOverride {
                        ed.supports_audio = infer_audio_capability(ed.provider, &entry.id);
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
                            .map(|p| (p.detected_vision, p.detected_audio, p.detail))
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
            DashboardMsg::ToggleModelAdvanced => {
                if let Some(ed) = self.dashboard.model_editor.as_mut() {
                    ed.show_advanced = !ed.show_advanced;
                }
                Task::none()
            }
            DashboardMsg::SaveModelProfile => {
                let Some(ed) = self.dashboard.model_editor.take() else {
                    return Task::none();
                };
                self.dashboard.selected_preset_kind = None;
                let profile = build_profile_from_editor(&ed);
                let pid = profile.id.clone();
                let is_tts_only = is_dedicated_tts_model(profile.provider, &profile.model_id);
                let _ = self.secrets.set_api_key(&pid, &ed.api_key);

                if let Some(existing) = self.config.models.iter_mut().find(|m| m.id == pid) {
                    *existing = profile;
                } else {
                    self.config.models.push(profile);
                    if is_tts_only {
                        self.config.active_voice_model_id = Some(pid);
                    } else {
                        self.config.active_model_id = pid;
                    }
                }
                self.config.ensure_valid();
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
                    self.dashboard.new_vocab_notes_editor = rtl_editor::Content::new();
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
            DashboardMsg::ToggleDoubleShift(v) => {
                self.config.shortcuts.enable_double_shift = v;
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
            DashboardMsg::SelectVoiceModel(id) => {
                self.config.active_voice_model_id = id;
                if let Some(vp) = self.config.resolved_voice_model().cloned() {
                    self.config.voice_name =
                        resolve_voice_for_profile(&vp, &self.config.voice_name);
                }
                self.config.ensure_valid();
                self.dashboard.settings_error = None;
                self.dashboard.translate_open_picker = None;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::SelectVisionModel(id) => {
                self.config.active_vision_model_id = id;
                self.config.ensure_valid();
                self.dashboard.settings_error = None;
                self.dashboard.translate_open_picker = None;
                let _ = self.storage.save_config(&self.config);
                self.rebuild_tray();
                Task::none()
            }
            DashboardMsg::SelectVoiceName(name) => {
                self.config.voice_name = name;
                self.dashboard.settings_error = None;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::TestVoicePreview => {
                let Some(voice_profile) = self.config.resolved_voice_model().cloned() else {
                    self.dashboard.settings_error = Some(self.i18n.t("errors.audio_unsupported"));
                    return Task::none();
                };
                if !voice_profile.supports_audio_output() {
                    self.dashboard.settings_error = Some(self.i18n.t("errors.audio_unsupported"));
                    return Task::none();
                }
                let voice_key = self.secrets.get_api_key(&voice_profile.id);
                let voice_name = resolve_voice_for_profile(&voice_profile, &self.config.voice_name);
                let sample_text = match self.config.target_language.as_str() {
                    "fa" => "سلام! صدای ترجمه آماده پخش است.",
                    "de" => "Hallo! Die Sprachausgabe der Übersetzung funktioniert.",
                    "fr" => "Bonjour ! La traduction vocale est prête.",
                    "es" => "¡Hola! La traducción por voz está lista.",
                    "tr" => "Merhaba! Sesli çeviri kullanıma hazır.",
                    "ar" => "مرحباً! الترجمة الصوتية جاهزة الآن.",
                    _ => "Hello! Your voice translation audio is working cleanly.",
                };
                self.dashboard.voice_testing = true;
                self.dashboard.settings_error = None;
                let client = self.client.clone();
                let key_msg = self.i18n.t("errors.missing_api_key");
                let audio_msg = self.i18n.t("errors.audio_unsupported");
                Task::perform(
                    async move {
                        client
                            .synthesize_speech(
                                &voice_profile,
                                voice_key.as_deref(),
                                sample_text,
                                &voice_name,
                            )
                            .await
                            .map_err(|e| match e {
                                ProviderError::MissingApiKey { .. } => key_msg,
                                ProviderError::AudioUnsupported { .. } => audio_msg,
                                other => other.to_string(),
                            })
                    },
                    Message::TestVoiceFinished,
                )
            }
            DashboardMsg::ToggleAutoCheckUpdates(v) => {
                self.config.auto_check_updates = v;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ToggleIncludePrereleases(v) => {
                self.config.include_prereleases = v;
                self.dashboard.update_status = None;
                self.dashboard.update_error = None;
                let _ = self.storage.save_config(&self.config);
                Task::none()
            }
            DashboardMsg::ResetModalPosition => {
                self.config.modal_position = None;
                self.config.modal_size = None;
                self.config.voice_player_position = None;
                let _ = self.storage.save_config(&self.config);
                self.dashboard.settings_toast = Some(self.i18n.t("settings.saved_toast"));
                Task::none()
            }
            DashboardMsg::CheckUpdatesNow => {
                self.dashboard.update_checking = true;
                self.dashboard.update_error = None;
                let updater = self.updater.clone();
                let include_prereleases = self.config.include_prereleases;
                Task::perform(
                    async move {
                        updater
                            .check_for_updates_with_channel(
                                env!("CARGO_PKG_VERSION"),
                                include_prereleases,
                            )
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::UpdateCheckFinished,
                )
            }
            DashboardMsg::DownloadAndInstallUpdate => {
                if self.dashboard.update_downloading {
                    return Task::none();
                }
                let Some(res) = &self.dashboard.update_status else {
                    return Task::none();
                };
                let Some(asset) = res.installer_asset.clone() else {
                    let _ = open::that(&res.release_url);
                    return Task::none();
                };
                self.dashboard.update_downloading = true;
                self.dashboard.update_download_progress = Some(0);
                let updater = self.updater.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
                tokio::spawn(async move {
                    let mut last_pct: Option<Option<u8>> = None;
                    let result = updater
                        .download_installer_with_progress(&asset, |progress| {
                            let pct = progress.percent();
                            if last_pct != Some(pct) {
                                last_pct = Some(pct);
                                let _ = tx.send(Message::UpdateDownloadProgress(pct));
                            }
                        })
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Message::UpdateDownloadFinished(result));
                });
                Task::stream(iced::futures::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|msg| (msg, rx))
                }))
            }
            DashboardMsg::OpenReleaseUrl(url) => {
                let _ = open::that(url);
                Task::none()
            }
        }
    }

    fn spawn_auto_update_check(&mut self) -> Task<Message> {
        if self.dashboard.update_checking || self.dashboard.update_downloading {
            return Task::none();
        }
        let updater = self.updater.clone();
        let include_prereleases = self.config.include_prereleases;
        self.dashboard.update_checking = true;
        Task::perform(
            async move {
                if !langhian_core::updater::has_internet_connectivity().await {
                    return None;
                }
                Some(
                    updater
                        .check_for_updates_with_channel(
                            env!("CARGO_PKG_VERSION"),
                            include_prereleases,
                        )
                        .await
                        .map_err(|e| e.to_string()),
                )
            },
            Message::AutoUpdateCheckFinished,
        )
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

fn open_dashboard_task(icon: Option<window::Icon>) -> (window::Id, Task<Message>) {
    let (id, open) = window::open(window::Settings {
        size: Size::new(1060.0, 720.0),
        min_size: Some(Size::new(720.0, 560.0)),
        position: window::Position::Centered,
        decorations: false,
        transparent: true,
        icon,
        exit_on_close_request: false,
        platform_specific: dashboard_platform_specific(),
        ..Default::default()
    });
    (
        id,
        open.map(|id| Message::WindowOpened(WindowKind::Dashboard, id)),
    )
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

fn map_voice_provider_error(
    err: ProviderError,
    vision_msg: &str,
    key_msg: &str,
    rate_msg: &str,
    audio_msg: &str,
) -> TranslationFail {
    match err {
        ProviderError::AudioUnsupported { .. } => TranslationFail {
            is_vision: false,
            message: audio_msg.to_string(),
        },
        other => map_provider_error(other, vision_msg, key_msg, rate_msg),
    }
}

async fn run_translation_request(
    client: TranslatorClient,
    pipeline: ResolvedTranslationPipeline,
    req: TranslateRequest,
    vision_msg: String,
    key_msg: String,
    rate_msg: String,
    audio_msg: String,
) -> Result<ModalTranslationOutcome, TranslationFail> {
    if let Some((voice_profile, voice_key, voice_name)) = pipeline.voice {
        match client
            .translate_with_voice(
                &pipeline.text_profile,
                pipeline.text_key.as_deref(),
                &voice_profile,
                voice_key.as_deref(),
                &req,
                &voice_name,
            )
            .await
        {
            Ok(v) => Ok(ModalTranslationOutcome {
                translated_text: v.translated_text,
                detected_source_lang: v.detected_source_lang,
                voice_audio: Some(v.audio),
                voice_used: Some(v.voice_used),
            }),
            Err(e) => Err(map_voice_provider_error(
                e,
                &vision_msg,
                &key_msg,
                &rate_msg,
                &audio_msg,
            )),
        }
    } else {
        match client
            .translate(&pipeline.text_profile, pipeline.text_key.as_deref(), &req)
            .await
        {
            Ok(r) => Ok(ModalTranslationOutcome {
                translated_text: r.translated_text,
                detected_source_lang: r.detected_source_lang,
                voice_audio: None,
                voice_used: None,
            }),
            Err(e) => Err(map_provider_error(e, &vision_msg, &key_msg, &rate_msg)),
        }
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
        supports_audio: ed.supports_audio,
        audio_source: ed.audio_source,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_and_orphan_windows_never_classify_as_dashboard() {
        let hud = window::Id::unique();
        let orphan = window::Id::unique();
        let dash = window::Id::unique();

        assert_eq!(
            classify_window_role(hud, None, None, None, Some(hud), None),
            Some(WindowKind::Hud)
        );
        // When dashboard_id is None (--minimized startup), unknown/initializing IDs must be None.
        assert_eq!(
            classify_window_role(orphan, None, None, None, Some(hud), None),
            None
        );
        // When dashboard_id is closed or dismissed HUD clears hud_id, old hud ID does not fall through to Dashboard.
        assert_eq!(
            classify_window_role(hud, Some(dash), None, None, None, None),
            None
        );
        assert_eq!(
            classify_window_role(dash, Some(dash), None, None, Some(hud), None),
            Some(WindowKind::Dashboard)
        );
    }
}
