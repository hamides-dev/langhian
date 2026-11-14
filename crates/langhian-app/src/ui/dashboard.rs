use crate::ui::icons::{ProviderLogoKind, UiIcon, app_logo, provider_logo, ui_icon};
use crate::ui::onboarding::{OnboardingState, view_onboarding};
use crate::ui::theme::{
    FramelessShellKind, ThemePalette, aligned_text, badge_style, card_style, danger_button_style,
    elevated_box_style, font_for_text, font_for_ui, ghost_button_style, hairline_divider,
    horizontal_for_dir, keycap, nav_button_style, palette_for, pill_button_style,
    primary_button_style, render_markdown_with_highlights, rounded_sidebar_style,
    rounded_titlebar_style, rtl_row, selectable_card_style, sleek_editor_style, sleek_input_style,
    sleek_menu_style, sleek_pick_list_style, sleek_row, sleek_scrollable, sleek_toggler_style,
    square_icon_button, theme_preview_card_content, window_close_button_style,
    wrap_frameless_resize_frame,
};
use iced::font::Weight;
use iced::widget::text::Shaping;
use iced::widget::{
    Space, button, column, container, mouse_area, pick_list, text, text_editor, text_input, toggler,
};
use iced::window;
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Padding};
use langhian_core::{
    AppConfig, CapabilitySource, HistoryEntry, ModelCatalogEntry, ModelProfile, PromptPreset,
    ProviderKind, TranslationMode, Translator, UiThemeMode, UpdateCheckResult, VocabEntry,
    VocabMatcher, find_vocab_match_index, language_display_name, supported_languages,
};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardTab {
    Translate,
    Models,
    Prompts,
    Vocabulary,
    History,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSubTab {
    General,
    Shortcuts,
    Translation,
    Appearance,
    Updates,
}

#[derive(Debug, Clone)]
pub struct ModelEditorState {
    pub editing_id: Option<String>,
    pub name: String,
    pub provider: ProviderKind,
    pub base_url: String,
    pub model_id: String,
    pub api_key: String,
    pub headers_raw: String,
    pub headers_editor: text_editor::Content,
    pub temperature: String,
    pub max_tokens: String,
    pub supports_vision: bool,
    pub vision_source: CapabilitySource,
    pub is_probing: bool,
    pub probe_status: Option<(bool, String)>,
    pub fetched_catalog: Vec<ModelCatalogEntry>,
    pub catalog_filter: String,
    pub selected_catalog_idx: Option<usize>,
}

impl ModelEditorState {
    pub fn from_profile(profile: &ModelProfile, api_key: Option<String>, is_new: bool) -> Self {
        let headers_raw = profile
            .extra_headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        let headers_editor = text_editor::Content::with_text(&headers_raw);
        Self {
            editing_id: if is_new {
                None
            } else {
                Some(profile.id.clone())
            },
            name: profile.name.clone(),
            provider: profile.provider,
            base_url: profile.base_url.clone(),
            model_id: profile.model_id.clone(),
            api_key: api_key.unwrap_or_default(),
            headers_raw,
            headers_editor,
            temperature: format!("{:.1}", profile.temperature),
            max_tokens: profile
                .max_tokens
                .map(|m| m.to_string())
                .unwrap_or_default(),
            supports_vision: profile.supports_vision,
            vision_source: profile.vision_source,
            is_probing: false,
            probe_status: None,
            fetched_catalog: Vec::new(),
            catalog_filter: String::new(),
            selected_catalog_idx: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptEditorState {
    pub editing_id: Option<String>,
    pub name: String,
    pub system_editor: text_editor::Content,
    pub user_template_editor: text_editor::Content,
    pub vision_editor: text_editor::Content,
    pub shortcut_slot: Option<u8>,
}

impl PromptEditorState {
    pub fn from_preset(preset: &PromptPreset, is_new: bool) -> Self {
        Self {
            editing_id: if is_new {
                None
            } else {
                Some(preset.id.clone())
            },
            name: preset.name.clone(),
            system_editor: text_editor::Content::with_text(&preset.system_prompt),
            user_template_editor: text_editor::Content::with_text(&preset.user_template),
            vision_editor: text_editor::Content::with_text(&preset.vision_instruction),
            shortcut_slot: preset.shortcut_slot,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DashboardState {
    pub active_tab: DashboardTab,
    pub settings_subtab: SettingsSubTab,
    pub show_onboarding: bool,
    pub onboarding: OnboardingState,
    pub is_maximized: bool,
    pub titlebar_close_hovered: bool,

    // Manual Translate tab
    pub source_editor: text_editor::Content,
    pub manual_output: String,
    pub is_translating: bool,
    pub translate_error: Option<String>,
    pub manual_auto_reversed: bool,
    pub manual_effective_target: Option<String>,
    pub copied_manual: bool,
    pub copied_manual_seq: u64,
    pub show_save_vocab_box: bool,
    pub save_term: String,
    pub save_translation: String,
    pub save_notes_editor: text_editor::Content,
    pub selected_vocab_preview: Option<VocabEntry>,

    // Models & Prompts
    pub selected_preset_kind: Option<ModelPresetKind>,
    pub model_editor: Option<ModelEditorState>,
    pub prompt_editor: Option<PromptEditorState>,

    // Vocab
    pub vocab_search: String,
    pub show_add_vocab_form: bool,
    pub new_vocab_term: String,
    pub new_vocab_translation: String,
    pub new_vocab_notes_editor: text_editor::Content,
    pub export_status: Option<String>,

    // Settings buffers (saved automatically on valid change)
    pub hotkey_screenshot: String,
    pub hotkey_next_prompt: String,
    pub hotkey_prev_prompt: String,
    pub hotkey_dashboard: String,
    pub double_tap_ms_input: String,
    pub settings_error: Option<String>,
    pub settings_toast: Option<String>,

    // Updates
    pub update_checking: bool,
    pub update_downloading: bool,
    pub update_status: Option<UpdateCheckResult>,
    pub update_error: Option<String>,
}

impl DashboardState {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            active_tab: DashboardTab::Translate,
            settings_subtab: SettingsSubTab::General,
            show_onboarding: !config.onboarding_completed,
            onboarding: OnboardingState::default(),
            is_maximized: false,
            titlebar_close_hovered: false,
            source_editor: text_editor::Content::new(),
            manual_output: String::new(),
            is_translating: false,
            translate_error: None,
            manual_auto_reversed: false,
            manual_effective_target: None,
            copied_manual: false,
            copied_manual_seq: 0,
            show_save_vocab_box: false,
            save_term: String::new(),
            save_translation: String::new(),
            save_notes_editor: text_editor::Content::new(),
            selected_vocab_preview: None,
            selected_preset_kind: None,
            model_editor: None,
            prompt_editor: None,
            vocab_search: String::new(),
            show_add_vocab_form: false,
            new_vocab_term: String::new(),
            new_vocab_translation: String::new(),
            new_vocab_notes_editor: text_editor::Content::new(),
            export_status: None,
            hotkey_screenshot: config.shortcuts.screenshot.format_display(),
            hotkey_next_prompt: config.shortcuts.next_prompt.format_display(),
            hotkey_prev_prompt: config.shortcuts.prev_prompt.format_display(),
            hotkey_dashboard: config.shortcuts.open_dashboard.format_display(),
            double_tap_ms_input: config.shortcuts.double_tap_ms.to_string(),
            settings_error: None,
            settings_toast: None,
            update_checking: false,
            update_downloading: false,
            update_status: None,
            update_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub id: String,
    pub label: String,
}

impl fmt::Display for PickerItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiLangChoice {
    pub code: &'static str,
    pub label: &'static str,
}

impl fmt::Display for UiLangChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderChoice {
    pub kind: ProviderKind,
    pub label: &'static str,
}

impl fmt::Display for ProviderChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotChoice {
    pub slot: Option<u8>,
}

impl fmt::Display for SlotChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.slot {
            Some(s) => write!(f, "Ctrl+Alt+{s}"),
            None => f.write_str("None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPresetKind {
    GoogleFree,
    OpenRouter,
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
    LmStudio,
    Custom,
}

#[derive(Debug, Clone)]
pub enum DashboardMsg {
    // Window chrome
    DragTitlebar,
    MinimizeWindow,
    ToggleMaximize,
    CloseWindow,
    TitlebarCloseHovered(bool),
    ResizeWindow(window::Direction),

    // Onboarding
    OnboardingStep(u8),
    OnboardingFinish(bool),
    OnboardingSkip,
    OpenOnboarding,

    SwitchTab(DashboardTab),
    SwitchSettingsSubTab(SettingsSubTab),

    // Manual Translate tab
    SourceEditorAction(text_editor::Action),
    SelectSourceLang(String),
    SelectNativeLang(String),
    SelectTargetLang(String),
    SwapLangs,
    SelectModel(String),
    SelectPrompt(String),
    RunManualTranslate,
    TriggerScreenshot,
    ClearManual,
    CopyManualResult,
    ToggleStarManualVocab,
    ToggleSaveManualBox,
    SaveManualTermChanged(String),
    SaveManualTranslationChanged(String),
    SaveManualNotesAction(text_editor::Action),
    ConfirmSaveManualVocab,
    ClickVocabHighlight(VocabEntry),
    DismissVocabPreview,

    // Models tab (Provider-First Setup)
    StartAddPreset(ModelPresetKind),
    StartEditModel(String),
    DuplicateModel(String),
    DeleteModel(String),
    CancelEditModel,
    ModelNameChanged(String),
    ModelProviderChanged(ProviderKind),
    ModelBaseUrlChanged(String),
    ModelIdChanged(String),
    ModelApiKeyChanged(String),
    ModelHeadersAction(text_editor::Action),
    ModelTempChanged(String),
    ModelMaxTokensChanged(String),
    ModelVisionToggled(bool),
    ModelCatalogFilterChanged(String),
    PickCatalogModel(usize),
    TestModelConnection,
    FetchProviderModels,
    SaveModelProfile,

    // Prompts tab
    StartAddPrompt,
    StartEditPrompt(String),
    DeletePrompt(String),
    SelectPromptActive(String),
    CancelEditPrompt,
    PromptNameChanged(String),
    PromptSlotChanged(Option<u8>),
    PromptSystemAction(text_editor::Action),
    PromptUserTemplateAction(text_editor::Action),
    PromptVisionAction(text_editor::Action),
    SavePromptPreset,

    // Vocabulary tab
    VocabSearchChanged(String),
    ToggleAddVocabForm,
    NewVocabTermChanged(String),
    NewVocabTranslationChanged(String),
    NewVocabNotesAction(text_editor::Action),
    ConfirmAddVocab,
    DeleteVocab(String),
    ExportVocabJson,
    ExportVocabCsv,

    // History tab
    CopyText(String),
    ToggleHistoryStarVocab(HistoryEntry),
    ClearHistory,

    // Settings tab (all auto-save)
    UiLanguageChanged(String),
    ThemeChanged(UiThemeMode),
    ToggleDoubleAlt(bool),
    ToggleAutoSelectFallback(bool),
    ToggleDoubleCtrl(bool),
    DoubleTapMsChanged(String),
    HotkeyScreenshotChanged(String),
    HotkeyNextPromptChanged(String),
    HotkeyPrevPromptChanged(String),
    HotkeyDashboardChanged(String),
    ToggleRestoreClipboard(bool),
    ToggleLaunchStartup(bool),
    ToggleShowHud(bool),
    TogglePreserveMarkdown(bool),
    ToggleAutoCheckUpdates(bool),
    ResetModalPosition,
    CheckUpdatesNow,
    DownloadAndInstallUpdate,
    OpenReleaseUrl(String),
}

pub fn view_dashboard<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    vocab: &'a [VocabEntry],
    history: &'a [HistoryEntry],
    i18n: &'a Translator,
    highlighter: &'a VocabMatcher,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let is_rtl = i18n.direction().is_rtl();
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Medium);
    let ui_font_bold = font_for_ui(i18n.direction(), Weight::Bold);

    let titlebar = view_custom_titlebar(state, mode, p, i18n, is_rtl, ui_font, ui_font_bold);

    let body: Element<'a, DashboardMsg> = if state.show_onboarding {
        view_onboarding(&state.onboarding, config, i18n)
    } else {
        let sidebar = view_sidebar(
            state,
            i18n,
            mode,
            p,
            is_rtl,
            ui_font,
            ui_font_semi,
            ui_font_bold,
        );

        let main_view: Element<'a, DashboardMsg> = match state.active_tab {
            DashboardTab::Translate => view_translate_tab(
                state,
                config,
                vocab,
                i18n,
                highlighter,
                mode,
                p,
                is_rtl,
                ui_font,
                ui_font_semi,
                ui_font_bold,
            ),
            DashboardTab::Models => view_models_tab(
                state,
                config,
                i18n,
                mode,
                p,
                is_rtl,
                ui_font,
                ui_font_semi,
                ui_font_bold,
            ),
            DashboardTab::Prompts => view_prompts_tab(
                state,
                config,
                i18n,
                mode,
                p,
                is_rtl,
                ui_font,
                ui_font_semi,
                ui_font_bold,
            ),
            DashboardTab::Vocabulary => view_vocab_tab(
                state,
                i18n,
                vocab,
                mode,
                p,
                is_rtl,
                ui_font,
                ui_font_semi,
                ui_font_bold,
            ),
            DashboardTab::History => view_history_tab(
                i18n,
                history,
                vocab,
                mode,
                p,
                is_rtl,
                ui_font,
                ui_font_semi,
                ui_font_bold,
            ),
            DashboardTab::Settings => view_settings_tab(
                state,
                config,
                i18n,
                mode,
                p,
                is_rtl,
                ui_font,
                ui_font_semi,
                ui_font_bold,
            ),
        };

        let v_divider = container(Space::new().width(1).height(Length::Fill)).style(move |_| {
            container::Style {
                background: Some(Background::Color(p.border_subtle)),
                ..Default::default()
            }
        });

        let workspace = rtl_row(
            is_rtl,
            vec![
                sidebar,
                v_divider.into(),
                container(main_view)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(Padding::from([22, 26]))
                    .into(),
            ],
        )
        .height(Length::Fill);

        workspace.into()
    };

    let shell = column![titlebar, hairline_divider(mode), body]
        .width(Length::Fill)
        .height(Length::Fill);

    wrap_frameless_resize_frame(
        shell.into(),
        mode,
        FramelessShellKind::Dashboard,
        12.0,
        state.is_maximized,
        DashboardMsg::ResizeWindow,
    )
}

fn view_custom_titlebar<'a>(
    state: &'a DashboardState,
    mode: UiThemeMode,
    p: ThemePalette,
    i18n: &'a Translator,
    is_rtl: bool,
    ui_font: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let brand_strip = mouse_area(
        rtl_row(
            is_rtl,
            vec![
                app_logo(22.0).into(),
                text(i18n.t("app.title"))
                    .size(13)
                    .font(ui_font_bold)
                    .color(p.text_primary)
                    .shaping(Shaping::Advanced)
                    .into(),
                text("·").size(12).color(p.text_muted).into(),
                text(i18n.t("app.subtitle"))
                    .size(12)
                    .font(ui_font)
                    .color(p.text_secondary)
                    .shaping(Shaping::Advanced)
                    .into(),
                Space::new().width(Length::Fill).into(),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fixed(36.0)),
    )
    .on_press(DashboardMsg::DragTitlebar)
    .on_double_click(DashboardMsg::ToggleMaximize);

    let min_btn = square_icon_button(
        ui_icon(UiIcon::Remove, 15.0, p.text_secondary),
        28.0,
        DashboardMsg::MinimizeWindow,
        ghost_button_style(mode, false),
    );

    let max_icon = if state.is_maximized {
        UiIcon::FilterNone
    } else {
        UiIcon::CropSquare
    };
    let max_btn = square_icon_button(
        ui_icon(max_icon, 14.0, p.text_secondary),
        28.0,
        DashboardMsg::ToggleMaximize,
        ghost_button_style(mode, false),
    );

    let close_btn = mouse_area(square_icon_button(
        ui_icon(
            UiIcon::Close,
            15.0,
            if state.titlebar_close_hovered {
                Color::WHITE
            } else {
                p.text_secondary
            },
        ),
        28.0,
        DashboardMsg::CloseWindow,
        window_close_button_style(mode),
    ))
    .on_enter(DashboardMsg::TitlebarCloseHovered(true))
    .on_exit(DashboardMsg::TitlebarCloseHovered(false));

    let controls = rtl_row(
        is_rtl,
        vec![min_btn.into(), max_btn.into(), close_btn.into()],
    )
    .spacing(4)
    .align_y(Alignment::Center);

    let top_radius = if state.is_maximized { 0.0 } else { 11.0 };

    container(
        rtl_row(is_rtl, vec![brand_strip.into(), controls.into()])
            .spacing(10)
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(Padding::from([2, 12]))
    .width(Length::Fill)
    .height(Length::Fixed(38.0))
    .style(rounded_titlebar_style(mode, top_radius))
    .into()
}

#[allow(clippy::too_many_arguments)]
fn view_sidebar<'a>(
    state: &'a DashboardState,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    _ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let nav_items = [
        (
            DashboardTab::Translate,
            UiIcon::Translate,
            i18n.t("nav.translate"),
        ),
        (DashboardTab::Models, UiIcon::SmartToy, i18n.t("nav.models")),
        (
            DashboardTab::Prompts,
            UiIcon::Psychology,
            i18n.t("nav.prompts"),
        ),
        (
            DashboardTab::Vocabulary,
            UiIcon::MenuBook,
            i18n.t("nav.vocabulary"),
        ),
        (
            DashboardTab::History,
            UiIcon::History,
            i18n.t("nav.history"),
        ),
        (
            DashboardTab::Settings,
            UiIcon::Settings,
            i18n.t("nav.settings"),
        ),
    ];

    let mut nav_col = column![].spacing(5);
    for (tab, icon, label) in nav_items {
        let is_active = state.active_tab == tab;
        let lbl_font = if is_active { ui_font_semi } else { ui_font };
        let icon_color = if is_active {
            p.accent_text
        } else {
            p.text_secondary
        };

        let btn = button(
            rtl_row(
                is_rtl,
                vec![
                    ui_icon(icon, 17.0, icon_color).into(),
                    aligned_text(label, 13.0, lbl_font, None, is_rtl),
                ],
            )
            .spacing(10)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .on_press(DashboardMsg::SwitchTab(tab))
        .padding([9, 12])
        .width(Length::Fill)
        .style(nav_button_style(mode, is_active));
        nav_col = nav_col.push(btn);
    }

    let bottom_radius = if state.is_maximized { 0.0 } else { 11.0 };

    container(column![nav_col, Space::new().height(Length::Fill)].height(Length::Fill))
        .width(Length::Fixed(220.0))
        .height(Length::Fill)
        .padding(Padding::from([16, 14]))
        .style(rounded_sidebar_style(mode, bottom_radius, is_rtl))
        .into()
}

fn page_header<'a, M: 'a>(
    title: impl Into<String>,
    subtitle: Option<String>,
    right_action: Option<Element<'a, M>>,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_bold: Font,
) -> Element<'a, M> {
    let h_align = horizontal_for_dir(is_rtl);

    let mut title_col = column![aligned_text(
        title.into(),
        20.0,
        ui_font_bold,
        Some(p.text_primary),
        is_rtl,
    )]
    .spacing(4)
    .align_x(h_align)
    .width(Length::Fill);

    if let Some(sub) = subtitle {
        title_col = title_col.push(aligned_text(
            sub,
            12.5,
            ui_font,
            Some(p.text_secondary),
            is_rtl,
        ));
    }

    let mut items: Vec<Element<'a, M>> = vec![title_col.into()];
    if let Some(act) = right_action {
        items.push(act);
    }

    rtl_row(is_rtl, items)
        .spacing(16)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

// ============================================================================
// 1. TRANSLATE TAB
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn view_translate_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    vocab: &'a [VocabEntry],
    i18n: &'a Translator,
    highlighter: &'a VocabMatcher,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let mut src_langs = vec![PickerItem {
        id: "auto".to_string(),
        label: i18n.t("translate.auto_detect"),
    }];
    src_langs.extend(supported_languages().iter().map(|l| PickerItem {
        id: l.code.to_string(),
        label: format!("{} — {}", l.name, l.native_name),
    }));
    let dst_langs: Vec<PickerItem> = supported_languages()
        .iter()
        .filter(|l| l.code != "auto")
        .map(|l| PickerItem {
            id: l.code.to_string(),
            label: format!("{} — {}", l.name, l.native_name),
        })
        .collect();

    let sel_src = src_langs
        .iter()
        .find(|l| l.id == config.source_language)
        .cloned()
        .or_else(|| src_langs.first().cloned());
    let sel_dst = dst_langs
        .iter()
        .find(|l| l.id == config.target_language)
        .cloned();

    let model_opts: Vec<PickerItem> = config
        .models
        .iter()
        .map(|m| PickerItem {
            id: m.id.clone(),
            label: if m.supports_vision {
                format!("{} · Vision", m.name)
            } else {
                m.name.clone()
            },
        })
        .collect();
    let sel_model = model_opts
        .iter()
        .find(|m| m.id == config.active_model_id)
        .cloned();

    let prompt_opts: Vec<PickerItem> = config
        .prompts
        .iter()
        .map(|pr| PickerItem {
            id: pr.id.clone(),
            label: pr.name.clone(),
        })
        .collect();
    let sel_prompt = prompt_opts
        .iter()
        .find(|pr| pr.id == config.active_prompt_id)
        .cloned();

    let lang_row = rtl_row(
        is_rtl,
        vec![
            pick_list(src_langs, sel_src, |x| DashboardMsg::SelectSourceLang(x.id))
                .font(ui_font)
                .text_size(12)
                .padding([7, 11])
                .width(Length::FillPortion(3))
                .style(sleek_pick_list_style(mode))
                .menu_style(sleek_menu_style(mode))
                .into(),
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::SwapHoriz, 15.0, p.text_primary).into(),
                        text(i18n.t("translate.swap_langs"))
                            .size(12)
                            .font(ui_font_semi)
                            .into(),
                    ],
                )
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::SwapLangs)
            .padding([7, 12])
            .style(pill_button_style(mode))
            .into(),
            pick_list(dst_langs, sel_dst, |x| DashboardMsg::SelectTargetLang(x.id))
                .font(ui_font)
                .text_size(12)
                .padding([7, 11])
                .width(Length::FillPortion(3))
                .style(sleek_pick_list_style(mode))
                .menu_style(sleek_menu_style(mode))
                .into(),
        ],
    )
    .spacing(8)
    .align_y(Alignment::Center);

    let pipeline_row = rtl_row(
        is_rtl,
        vec![
            rtl_row(
                is_rtl,
                vec![
                    text(i18n.t("tray.active_model"))
                        .size(11.5)
                        .font(ui_font_semi)
                        .color(p.text_muted)
                        .into(),
                    pick_list(model_opts, sel_model, |x| DashboardMsg::SelectModel(x.id))
                        .font(ui_font)
                        .text_size(12)
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(sleek_pick_list_style(mode))
                        .menu_style(sleek_menu_style(mode))
                        .into(),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::FillPortion(3))
            .into(),
            rtl_row(
                is_rtl,
                vec![
                    text(i18n.t("tray.active_prompt"))
                        .size(11.5)
                        .font(ui_font_semi)
                        .color(p.text_muted)
                        .into(),
                    pick_list(prompt_opts, sel_prompt, |x| {
                        DashboardMsg::SelectPrompt(x.id)
                    })
                    .font(ui_font)
                    .text_size(12)
                    .padding([6, 10])
                    .width(Length::Fill)
                    .style(sleek_pick_list_style(mode))
                    .menu_style(sleek_menu_style(mode))
                    .into(),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::FillPortion(2))
            .into(),
        ],
    )
    .spacing(12)
    .align_y(Alignment::Center);

    let control_card = container(column![
        container(lang_row).padding([10, 14]),
        hairline_divider(mode),
        container(pipeline_row).padding([10, 14]),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    let header = page_header(
        i18n.t("nav.translate"),
        None,
        Some(
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::CropFree, 15.0, p.text_primary).into(),
                        text(i18n.t("translate.screenshot_btn"))
                            .size(12)
                            .font(ui_font_semi)
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::TriggerScreenshot)
            .padding([7, 14])
            .style(pill_button_style(mode))
            .into(),
        ),
        p,
        is_rtl,
        ui_font,
        ui_font_bold,
    );

    let source_pane = container(
        column![
            aligned_text(
                i18n.t("translate.source_label"),
                12.0,
                ui_font_semi,
                Some(p.text_secondary),
                is_rtl,
            ),
            text_editor(&state.source_editor)
                .placeholder(i18n.t("translate.source_placeholder"))
                .on_action(DashboardMsg::SourceEditorAction)
                .font(font_for_text(
                    &state.source_editor.text(),
                    None,
                    Weight::Normal
                ))
                .size(14)
                .padding(12)
                .height(Length::Fill)
                .style(sleek_editor_style(mode)),
        ]
        .spacing(8)
        .height(Length::Fill),
    )
    .padding(14)
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .style(card_style(mode, 10.0));

    let effective_lang = state
        .manual_effective_target
        .as_deref()
        .unwrap_or(&config.native_language);

    let output_body: Element<'a, DashboardMsg> = if state.is_translating {
        aligned_text(
            i18n.t("app.status_translating"),
            14.0,
            ui_font_semi,
            Some(p.text_secondary),
            is_rtl,
        )
    } else if let Some(err) = &state.translate_error {
        aligned_text(err.as_str(), 13.0, ui_font, Some(p.danger), is_rtl)
    } else if state.manual_output.is_empty() {
        aligned_text(
            i18n.t("translate.result_placeholder"),
            14.0,
            ui_font,
            Some(p.text_muted),
            is_rtl,
        )
    } else {
        sleek_scrollable(
            container(render_markdown_with_highlights(
                &state.manual_output,
                config.preserve_markdown,
                Some(effective_lang),
                highlighter,
                DashboardMsg::ClickVocabHighlight,
                mode,
                15.0,
            ))
            .padding(Padding::from([0, 4]))
            .width(Length::Fill),
            mode,
        )
        .height(Length::Fill)
        .into()
    };

    let dir_badge_label = if state.manual_auto_reversed {
        format!(
            "{} {}",
            i18n.t("translate.direction_auto_swapped"),
            language_display_name(effective_lang)
        )
    } else {
        format!(
            "{} <-> {}",
            language_display_name(&config.native_language),
            language_display_name(&config.target_language)
        )
    };

    let output_pane = container(
        column![
            rtl_row(
                is_rtl,
                vec![
                    aligned_text(
                        i18n.t("translate.target_label"),
                        12.0,
                        ui_font_semi,
                        Some(p.text_secondary),
                        is_rtl,
                    ),
                    container(text(dir_badge_label).size(11).font(ui_font_semi).color(
                        if state.manual_auto_reversed {
                            p.success
                        } else {
                            p.accent_text
                        }
                    ),)
                    .padding([2, 8])
                    .style(badge_style(
                        if state.manual_auto_reversed {
                            p.success_soft
                        } else {
                            p.accent_soft
                        },
                        if state.manual_auto_reversed {
                            p.success
                        } else {
                            p.accent_text
                        },
                        p.border_subtle,
                    ))
                    .into(),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center),
            container(output_body)
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.bg_input)),
                    border: Border {
                        color: p.border_subtle,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    text_color: Some(p.text_primary),
                    ..Default::default()
                }),
        ]
        .spacing(8)
        .height(Length::Fill),
    )
    .padding(14)
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .style(card_style(mode, 10.0));

    let panes = rtl_row(is_rtl, vec![source_pane.into(), output_pane.into()])
        .spacing(12)
        .height(Length::Fill);

    let mut content = column![header, control_card, panes]
        .spacing(12)
        .height(Length::Fill);

    if let Some(v) = &state.selected_vocab_preview {
        content = content.push(
            container(
                rtl_row(
                    is_rtl,
                    vec![
                        column![
                            aligned_text(
                                format!("{}  ->  {}", v.term, v.translation),
                                13.0,
                                font_for_text(&v.term, None, Weight::Medium),
                                Some(p.vocab_highlight_text),
                                is_rtl,
                            ),
                            aligned_text(
                                v.notes.as_str(),
                                12.0,
                                ui_font,
                                Some(p.text_secondary),
                                is_rtl,
                            ),
                        ]
                        .spacing(2)
                        .width(Length::Fill)
                        .into(),
                        square_icon_button(
                            ui_icon(UiIcon::Close, 14.0, p.text_secondary),
                            28.0,
                            DashboardMsg::DismissVocabPreview,
                            ghost_button_style(mode, false),
                        )
                        .into(),
                    ],
                )
                .align_y(Alignment::Center),
            )
            .padding(12)
            .width(Length::Fill)
            .style(elevated_box_style(mode, 9.0)),
        );
    }

    if state.show_save_vocab_box {
        content = content.push(
            container(
                column![
                    aligned_text(
                        i18n.t("translate.save_word_title"),
                        13.0,
                        ui_font_bold,
                        Some(p.text_primary),
                        is_rtl,
                    ),
                    rtl_row(
                        is_rtl,
                        vec![
                            text_input(&i18n.t("translate.word_label"), &state.save_term)
                                .on_input(DashboardMsg::SaveManualTermChanged)
                                .font(ui_font)
                                .size(13)
                                .padding(9)
                                .width(Length::FillPortion(1))
                                .style(sleek_input_style(mode))
                                .into(),
                            text_input(&i18n.t("translate.meaning_label"), &state.save_translation)
                                .on_input(DashboardMsg::SaveManualTranslationChanged)
                                .font(ui_font)
                                .size(13)
                                .padding(9)
                                .width(Length::FillPortion(1))
                                .style(sleek_input_style(mode))
                                .into(),
                        ],
                    )
                    .spacing(8),
                    text_editor(&state.save_notes_editor)
                        .placeholder(i18n.t("vocab.notes_placeholder"))
                        .on_action(DashboardMsg::SaveManualNotesAction)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .height(Length::Fixed(72.0))
                        .style(sleek_editor_style(mode)),
                    rtl_row(
                        is_rtl,
                        vec![
                            button(
                                text(i18n.t("vocab.save"))
                                    .size(12)
                                    .font(ui_font_semi)
                                    .color(Color::WHITE),
                            )
                            .on_press(DashboardMsg::ConfirmSaveManualVocab)
                            .padding([7, 14])
                            .style(primary_button_style(mode))
                            .into(),
                            button(text(i18n.t("models.cancel")).size(12).font(ui_font_semi),)
                                .on_press(DashboardMsg::ToggleSaveManualBox)
                                .padding([7, 12])
                                .style(pill_button_style(mode))
                                .into(),
                        ],
                    )
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8)
                .align_x(h_align),
            )
            .padding(14)
            .width(Length::Fill)
            .style(elevated_box_style(mode, 10.0)),
        );
    }

    let source_txt = state.source_editor.text();
    let is_starred = !source_txt.trim().is_empty()
        && !state.manual_output.trim().is_empty()
        && find_vocab_match_index(vocab, &source_txt, &state.manual_output).is_some();

    let actions = rtl_row(
        is_rtl,
        vec![
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::Translate, 15.0, Color::WHITE).into(),
                        text(i18n.t("translate.translate_btn"))
                            .size(13)
                            .font(ui_font_semi)
                            .color(Color::WHITE)
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::RunManualTranslate)
            .padding([9, 18])
            .style(primary_button_style(mode))
            .into(),
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(
                            if state.copied_manual {
                                UiIcon::Check
                            } else {
                                UiIcon::Copy
                            },
                            15.0,
                            if state.copied_manual {
                                p.success
                            } else {
                                p.text_primary
                            },
                        )
                        .into(),
                        text(if state.copied_manual {
                            i18n.t("translate.copied")
                        } else {
                            i18n.t("translate.copy_btn")
                        })
                        .size(12)
                        .font(ui_font_semi)
                        .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::CopyManualResult)
            .padding([8, 14])
            .style(pill_button_style(mode))
            .into(),
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(
                            if is_starred {
                                UiIcon::StarFill
                            } else {
                                UiIcon::Star
                            },
                            15.0,
                            if is_starred {
                                p.warning
                            } else {
                                p.text_primary
                            },
                        )
                        .into(),
                        text(if is_starred {
                            i18n.t("translate.starred")
                        } else {
                            i18n.t("translate.star_vocab")
                        })
                        .size(12)
                        .font(ui_font_semi)
                        .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::ToggleStarManualVocab)
            .padding([8, 14])
            .style(pill_button_style(mode))
            .into(),
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::Edit, 14.0, p.text_primary).into(),
                        text(i18n.t("translate.save_btn"))
                            .size(12)
                            .font(ui_font_semi)
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::ToggleSaveManualBox)
            .padding([8, 13])
            .style(pill_button_style(mode))
            .into(),
            Space::new().width(Length::Fill).into(),
            button(text(i18n.t("translate.clear_btn")).size(12).font(ui_font))
                .on_press(DashboardMsg::ClearManual)
                .padding([8, 14])
                .style(ghost_button_style(mode, false))
                .into(),
        ],
    )
    .spacing(8)
    .align_y(Alignment::Center);

    content.push(actions).into()
}

// ============================================================================
// 2. MODELS TAB (Provider-First + Live/Reference Pricing Catalog)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn view_models_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let header = page_header(
        i18n.t("models.title"),
        Some(i18n.t("hints.models_sub")),
        None,
        p,
        is_rtl,
        ui_font,
        ui_font_bold,
    );

    let preset_card = |label: String,
                       sub: &'static str,
                       kind: ModelPresetKind,
                       logo: ProviderLogoKind|
     -> Element<'a, DashboardMsg> {
        let is_selected = state.selected_preset_kind == Some(kind) && state.model_editor.is_some();
        button(
            rtl_row(
                is_rtl,
                vec![
                    provider_logo(logo, 20.0, p.text_primary).into(),
                    column![
                        aligned_text(label, 13.0, ui_font_semi, Some(p.text_primary), is_rtl),
                        aligned_text(sub, 11.0, ui_font, Some(p.text_muted), is_rtl),
                    ]
                    .spacing(2)
                    .align_x(h_align)
                    .width(Length::Fill)
                    .into(),
                ],
            )
            .spacing(10)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .on_press(DashboardMsg::StartAddPreset(kind))
        .padding([10, 12])
        .width(Length::FillPortion(1))
        .style(selectable_card_style(mode, is_selected, 9.0))
        .into()
    };

    let quick_add_grid = column![
        aligned_text(
            i18n.t("models.step_provider"),
            12.5,
            ui_font_semi,
            Some(p.text_secondary),
            is_rtl,
        ),
        rtl_row(
            is_rtl,
            vec![
                preset_card(
                    i18n.t("models.preset_openrouter"),
                    "Multi-provider",
                    ModelPresetKind::OpenRouter,
                    ProviderLogoKind::OpenRouter,
                ),
                preset_card(
                    i18n.t("models.preset_openai"),
                    "GPT / o-series",
                    ModelPresetKind::OpenAi,
                    ProviderLogoKind::OpenAi,
                ),
                preset_card(
                    "Claude".to_string(),
                    "Anthropic API",
                    ModelPresetKind::Anthropic,
                    ProviderLogoKind::Anthropic,
                ),
                preset_card(
                    "Gemini".to_string(),
                    "Google AI",
                    ModelPresetKind::Gemini,
                    ProviderLogoKind::Gemini,
                ),
            ],
        )
        .spacing(10),
        rtl_row(
            is_rtl,
            vec![
                preset_card(
                    i18n.t("models.preset_ollama"),
                    "Localhost :11434",
                    ModelPresetKind::Ollama,
                    ProviderLogoKind::Ollama,
                ),
                preset_card(
                    i18n.t("models.preset_lmstudio"),
                    "Localhost :1234",
                    ModelPresetKind::LmStudio,
                    ProviderLogoKind::LmStudio,
                ),
                preset_card(
                    "Google Free".to_string(),
                    "No API key",
                    ModelPresetKind::GoogleFree,
                    ProviderLogoKind::GoogleTranslate,
                ),
                preset_card(
                    i18n.t("models.preset_custom"),
                    "Custom Base URL",
                    ModelPresetKind::Custom,
                    ProviderLogoKind::Custom,
                ),
            ],
        )
        .spacing(10),
    ]
    .spacing(8)
    .align_x(h_align);

    let mut content = column![header, quick_add_grid].spacing(16);

    if let Some(ed) = &state.model_editor {
        content = content.push(view_model_editor(
            ed,
            i18n,
            mode,
            p,
            is_rtl,
            ui_font,
            ui_font_semi,
            ui_font_bold,
        ));
    }

    let mut rows_col = column![];
    for (idx, m) in config.models.iter().enumerate() {
        if idx > 0 {
            rows_col = rows_col.push(hairline_divider(mode));
        }
        let is_active = m.id == config.active_model_id;
        let logo_kind = ProviderLogoKind::from_profile(m.provider, &m.base_url, &m.name);
        let cap_label = if m.supports_vision {
            i18n.t("models.vision_badge")
        } else {
            i18n.t("models.text_only_badge")
        };

        let mut badges = rtl_row(
            is_rtl,
            vec![
                provider_logo(logo_kind, 18.0, p.text_primary).into(),
                text(&m.name)
                    .size(13)
                    .font(font_for_text(&m.name, None, Weight::Medium))
                    .color(p.text_primary)
                    .into(),
                container(text(cap_label).size(10).font(ui_font_semi).color(
                    if m.supports_vision {
                        p.accent_text
                    } else {
                        p.text_muted
                    },
                ))
                .padding([2, 7])
                .style(badge_style(
                    if m.supports_vision {
                        p.accent_soft
                    } else {
                        p.bg_elevated
                    },
                    if m.supports_vision {
                        p.accent_text
                    } else {
                        p.text_muted
                    },
                    p.border_subtle,
                ))
                .into(),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center);

        if is_active {
            badges = badges.push(
                container(
                    text(i18n.t("models.active_badge"))
                        .size(10)
                        .font(ui_font_semi)
                        .color(p.success),
                )
                .padding([2, 7])
                .style(badge_style(
                    p.success_soft,
                    p.success,
                    Color::from_rgba(0.20, 0.83, 0.60, 0.32),
                )),
            );
        }

        let sub = format!("{}  ·  {}", m.model_id, m.base_url);
        let left_col = column![
            badges,
            aligned_text(sub, 12.0, ui_font, Some(p.text_secondary), is_rtl),
        ]
        .spacing(4)
        .align_x(h_align)
        .width(Length::Fill);

        let mut action_items: Vec<Element<'a, DashboardMsg>> = Vec::new();
        if !is_active {
            action_items.push(
                button(
                    text(i18n.t("models.set_active"))
                        .size(11.5)
                        .font(ui_font_semi)
                        .color(Color::WHITE),
                )
                .on_press(DashboardMsg::SelectModel(m.id.clone()))
                .padding([6, 12])
                .style(primary_button_style(mode))
                .into(),
            );
        }
        action_items.push(
            button(ui_icon(UiIcon::Edit, 14.0, p.text_primary))
                .on_press(DashboardMsg::StartEditModel(m.id.clone()))
                .padding([6, 9])
                .style(pill_button_style(mode))
                .into(),
        );
        action_items.push(
            button(ui_icon(UiIcon::FileCopy, 14.0, p.text_primary))
                .on_press(DashboardMsg::DuplicateModel(m.id.clone()))
                .padding([6, 9])
                .style(pill_button_style(mode))
                .into(),
        );
        if config.models.len() > 1 {
            action_items.push(
                button(ui_icon(UiIcon::Delete, 14.0, p.danger))
                    .on_press(DashboardMsg::DeleteModel(m.id.clone()))
                    .padding([6, 9])
                    .style(danger_button_style(mode))
                    .into(),
            );
        }

        rows_col = rows_col.push(
            container(
                rtl_row(
                    is_rtl,
                    vec![
                        left_col.into(),
                        rtl_row(is_rtl, action_items)
                            .spacing(6)
                            .align_y(Alignment::Center)
                            .into(),
                    ],
                )
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([12, 16])
            .width(Length::Fill),
        );
    }

    content = content.push(
        container(rows_col)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
    );

    sleek_scrollable(
        container(content)
            .padding(Padding::from([0, 6]))
            .width(Length::Fill),
        mode,
    )
    .height(Length::Fill)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn view_model_editor<'a>(
    ed: &'a ModelEditorState,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);
    let logo_kind = ProviderLogoKind::from_profile(ed.provider, &ed.base_url, &ed.name);

    let providers = vec![
        ProviderChoice {
            kind: ProviderKind::OpenAiCompatible,
            label: "OpenAI-Compatible",
        },
        ProviderChoice {
            kind: ProviderKind::Anthropic,
            label: "Anthropic Claude",
        },
        ProviderChoice {
            kind: ProviderKind::Gemini,
            label: "Google Gemini",
        },
        ProviderChoice {
            kind: ProviderKind::GoogleTranslateFree,
            label: "Google Translate (Free)",
        },
    ];
    let sel_prov = providers.iter().find(|pr| pr.kind == ed.provider).cloned();

    let field_lbl = |s: String| aligned_text(s, 11.5, ui_font_semi, Some(p.text_secondary), is_rtl);

    let step2_block = column![
        aligned_text(
            i18n.t("models.step_key"),
            12.5,
            ui_font_bold,
            Some(p.accent_text),
            is_rtl,
        ),
        rtl_row(
            is_rtl,
            vec![
                column![
                    field_lbl(i18n.t("models.name_label")),
                    text_input(&i18n.t("models.name_label"), &ed.name)
                        .on_input(DashboardMsg::ModelNameChanged)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .style(sleek_input_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(1))
                .into(),
                column![
                    field_lbl(i18n.t("models.provider_label")),
                    pick_list(providers, sel_prov, |x| {
                        DashboardMsg::ModelProviderChanged(x.kind)
                    })
                    .font(ui_font)
                    .text_size(13)
                    .padding(9)
                    .width(Length::Fill)
                    .style(sleek_pick_list_style(mode))
                    .menu_style(sleek_menu_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(1))
                .into(),
            ],
        )
        .spacing(12),
        rtl_row(
            is_rtl,
            vec![
                column![
                    field_lbl(i18n.t("models.base_url_label")),
                    text_input("https://...", &ed.base_url)
                        .on_input(DashboardMsg::ModelBaseUrlChanged)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .style(sleek_input_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(1))
                .into(),
                column![
                    field_lbl(i18n.t("models.api_key_label")),
                    text_input(&i18n.t("models.api_key_placeholder"), &ed.api_key)
                        .on_input(DashboardMsg::ModelApiKeyChanged)
                        .secure(true)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .style(sleek_input_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(1))
                .into(),
            ],
        )
        .spacing(12),
        rtl_row(
            is_rtl,
            vec![
                button(
                    rtl_row(
                        is_rtl,
                        vec![
                            ui_icon(UiIcon::Refresh, 15.0, Color::WHITE).into(),
                            text(i18n.t("models.fetch_models_btn"))
                                .size(12.5)
                                .font(ui_font_semi)
                                .color(Color::WHITE)
                                .into(),
                        ],
                    )
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(DashboardMsg::FetchProviderModels)
                .padding([8, 14])
                .style(primary_button_style(mode))
                .into(),
                button(
                    rtl_row(
                        is_rtl,
                        vec![
                            ui_icon(UiIcon::Visibility, 15.0, p.text_primary).into(),
                            text(if ed.is_probing {
                                i18n.t("models.testing")
                            } else {
                                i18n.t("models.test_btn")
                            })
                            .size(12.5)
                            .font(ui_font_semi)
                            .into(),
                        ],
                    )
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(DashboardMsg::TestModelConnection)
                .padding([8, 14])
                .style(pill_button_style(mode))
                .into(),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(10)
    .align_x(h_align);

    // Step 3: Searchable Catalog with Per-1M Pricing + Vision + Context
    let catalog_block: Element<'a, DashboardMsg> = if !ed.fetched_catalog.is_empty() {
        let q = ed.catalog_filter.trim().to_lowercase();
        let mut rows = column![].spacing(4);
        for (idx, item) in ed.fetched_catalog.iter().enumerate() {
            if !q.is_empty()
                && !item.id.to_lowercase().contains(&q)
                && !item.display_name.to_lowercase().contains(&q)
            {
                continue;
            }
            let selected = ed.model_id == item.id || ed.selected_catalog_idx == Some(idx);
            let price_badge = item.format_price_badge();
            let ctx_badge = item.format_context_badge();

            let mut row_items: Vec<Element<'a, DashboardMsg>> = vec![
                column![
                    text(&item.display_name)
                        .size(12.5)
                        .font(ui_font_semi)
                        .color(p.text_primary),
                    text(&item.id)
                        .size(11)
                        .font(Font::MONOSPACE)
                        .color(p.text_muted),
                ]
                .spacing(2)
                .width(Length::Fill)
                .into(),
            ];

            if let Some(ctx) = ctx_badge {
                row_items.push(
                    container(
                        text(ctx)
                            .size(10.5)
                            .font(ui_font_semi)
                            .color(p.text_secondary),
                    )
                    .padding([2, 7])
                    .style(badge_style(
                        p.bg_elevated,
                        p.text_secondary,
                        p.border_subtle,
                    ))
                    .into(),
                );
            }
            if item.supports_vision == Some(true) {
                row_items.push(
                    container(
                        text(i18n.t("models.vision_badge"))
                            .size(10.5)
                            .font(ui_font_semi)
                            .color(p.accent_text),
                    )
                    .padding([2, 7])
                    .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle))
                    .into(),
                );
            }
            row_items.push(
                container(
                    rtl_row(
                        is_rtl,
                        vec![
                            ui_icon(UiIcon::Payments, 12.0, p.success).into(),
                            text(price_badge)
                                .size(11)
                                .font(ui_font_semi)
                                .color(p.success)
                                .into(),
                        ],
                    )
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding([3, 8])
                .style(badge_style(p.success_soft, p.success, p.border_subtle))
                .into(),
            );

            rows = rows.push(
                button(
                    rtl_row(is_rtl, row_items)
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .width(Length::Fill),
                )
                .on_press(DashboardMsg::PickCatalogModel(idx))
                .padding([8, 12])
                .width(Length::Fill)
                .style(selectable_card_style(mode, selected, 8.0)),
            );
        }

        column![
            rtl_row(
                is_rtl,
                vec![
                    text(i18n.t("models.step_pick_model"))
                        .size(12.5)
                        .font(ui_font_bold)
                        .color(p.accent_text)
                        .width(Length::Fill)
                        .into(),
                    text(i18n.t("models.pricing_in_out"))
                        .size(11)
                        .font(ui_font)
                        .color(p.text_muted)
                        .into(),
                ],
            )
            .align_y(Alignment::Center),
            text_input(
                &i18n.t("models.search_models_placeholder"),
                &ed.catalog_filter
            )
            .on_input(DashboardMsg::ModelCatalogFilterChanged)
            .font(ui_font)
            .size(12.5)
            .padding(8)
            .style(sleek_input_style(mode)),
            container(
                sleek_scrollable(
                    container(rows)
                        .padding(Padding::from([2, 4]))
                        .width(Length::Fill),
                    mode,
                )
                .height(Length::Fixed(210.0)),
            )
            .padding(6)
            .width(Length::Fill)
            .style(elevated_box_style(mode, 9.0)),
        ]
        .spacing(8)
        .align_x(h_align)
        .into()
    } else {
        container(
            rtl_row(
                is_rtl,
                vec![
                    ui_icon(UiIcon::Info, 15.0, p.text_muted).into(),
                    aligned_text(
                        i18n.t("models.no_models_fetched"),
                        12.0,
                        ui_font,
                        Some(p.text_secondary),
                        is_rtl,
                    ),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([10, 12])
        .width(Length::Fill)
        .style(elevated_box_style(mode, 8.0))
        .into()
    };

    let manual_and_advanced = column![
        rtl_row(
            is_rtl,
            vec![
                column![
                    field_lbl(i18n.t("models.model_id_label")),
                    text_input("gpt-4o-mini / claude-sonnet-4...", &ed.model_id)
                        .on_input(DashboardMsg::ModelIdChanged)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .style(sleek_input_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(2))
                .into(),
                column![
                    field_lbl(i18n.t("models.temperature_label")),
                    text_input("0.2", &ed.temperature)
                        .on_input(DashboardMsg::ModelTempChanged)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .style(sleek_input_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(1))
                .into(),
                column![
                    field_lbl(i18n.t("models.max_tokens_label")),
                    text_input("Optional", &ed.max_tokens)
                        .on_input(DashboardMsg::ModelMaxTokensChanged)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .style(sleek_input_style(mode)),
                ]
                .spacing(5)
                .width(Length::FillPortion(1))
                .into(),
            ],
        )
        .spacing(10),
        column![
            field_lbl(i18n.t("models.custom_headers_label")),
            text_editor(&ed.headers_editor)
                .on_action(DashboardMsg::ModelHeadersAction)
                .font(Font::MONOSPACE)
                .size(12.5)
                .padding(10)
                .height(Length::Fixed(92.0))
                .style(sleek_editor_style(mode)),
        ]
        .spacing(5),
        rtl_row(
            is_rtl,
            vec![
                toggler(ed.supports_vision)
                    .on_toggle(DashboardMsg::ModelVisionToggled)
                    .size(19)
                    .style(sleek_toggler_style(mode))
                    .into(),
                aligned_text(
                    i18n.t("models.supports_vision_label"),
                    12.5,
                    ui_font,
                    Some(p.text_primary),
                    is_rtl,
                ),
            ],
        )
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(10)
    .align_x(h_align);

    let mut form = column![
        rtl_row(
            is_rtl,
            vec![
                provider_logo(logo_kind, 20.0, p.text_primary).into(),
                aligned_text(
                    if ed.editing_id.is_some() {
                        i18n.t("models.edit_profile")
                    } else {
                        i18n.t("models.add_profile")
                    },
                    15.0,
                    ui_font_bold,
                    Some(p.text_primary),
                    is_rtl,
                ),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center),
        hairline_divider(mode),
        step2_block,
        catalog_block,
        manual_and_advanced,
    ]
    .spacing(12)
    .align_x(h_align);

    if let Some((ok, status)) = &ed.probe_status {
        form = form.push(aligned_text(
            status.as_str(),
            12.0,
            ui_font_semi,
            Some(if *ok { p.success } else { p.danger }),
            is_rtl,
        ));
    }

    form = form.push(
        rtl_row(
            is_rtl,
            vec![
                button(
                    rtl_row(
                        is_rtl,
                        vec![
                            ui_icon(UiIcon::Check, 15.0, Color::WHITE).into(),
                            text(if !ed.fetched_catalog.is_empty() {
                                i18n.t("models.use_selected_model")
                            } else {
                                i18n.t("models.save_profile")
                            })
                            .size(13)
                            .font(ui_font_semi)
                            .color(Color::WHITE)
                            .into(),
                        ],
                    )
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(DashboardMsg::SaveModelProfile)
                .padding([8, 16])
                .style(primary_button_style(mode))
                .into(),
                button(text(i18n.t("models.cancel")).size(12.5).font(ui_font_semi))
                    .on_press(DashboardMsg::CancelEditModel)
                    .padding([8, 14])
                    .style(pill_button_style(mode))
                    .into(),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center),
    );

    container(form)
        .padding(18)
        .width(Length::Fill)
        .style(card_style(mode, 10.0))
        .into()
}

// ============================================================================
// 3. PROMPTS TAB (Tall Multiline Editors)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn view_prompts_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let header = page_header(
        i18n.t("prompts.title"),
        Some(i18n.t("hints.prompts_sub")),
        Some(
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::Add, 15.0, Color::WHITE).into(),
                        text(i18n.t("prompts.add_prompt"))
                            .size(12.5)
                            .font(ui_font_semi)
                            .color(Color::WHITE)
                            .into(),
                    ],
                )
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::StartAddPrompt)
            .padding([8, 14])
            .style(primary_button_style(mode))
            .into(),
        ),
        p,
        is_rtl,
        ui_font,
        ui_font_bold,
    );

    let mut content = column![header].spacing(16);

    if let Some(ed) = &state.prompt_editor {
        let slots: Vec<SlotChoice> = std::iter::once(SlotChoice { slot: None })
            .chain((1..=9).map(|s| SlotChoice { slot: Some(s) }))
            .collect();
        let sel_slot = slots
            .iter()
            .find(|s| s.slot == ed.shortcut_slot)
            .cloned()
            .unwrap_or(SlotChoice { slot: None });

        let lbl = |s: String| aligned_text(s, 11.5, ui_font_semi, Some(p.text_secondary), is_rtl);

        content = content.push(
            container(
                column![
                    aligned_text(
                        if ed.editing_id.is_some() {
                            i18n.t("prompts.edit_prompt")
                        } else {
                            i18n.t("prompts.add_prompt")
                        },
                        15.0,
                        ui_font_bold,
                        Some(p.text_primary),
                        is_rtl,
                    ),
                    rtl_row(
                        is_rtl,
                        vec![
                            column![
                                lbl(i18n.t("prompts.name_label")),
                                text_input(&i18n.t("prompts.name_label"), &ed.name)
                                    .on_input(DashboardMsg::PromptNameChanged)
                                    .font(ui_font)
                                    .size(13)
                                    .padding(9)
                                    .style(sleek_input_style(mode)),
                            ]
                            .spacing(5)
                            .width(Length::FillPortion(2))
                            .into(),
                            column![
                                lbl(i18n.t("prompts.slot_label")),
                                pick_list(slots, Some(sel_slot), |x| {
                                    DashboardMsg::PromptSlotChanged(x.slot)
                                })
                                .font(ui_font)
                                .text_size(13)
                                .padding(9)
                                .width(Length::Fill)
                                .style(sleek_pick_list_style(mode))
                                .menu_style(sleek_menu_style(mode)),
                            ]
                            .spacing(5)
                            .width(Length::FillPortion(1))
                            .into(),
                        ],
                    )
                    .spacing(12),
                    aligned_text(
                        i18n.t("prompts.template_vars_hint"),
                        11.5,
                        ui_font,
                        Some(p.accent_text),
                        is_rtl,
                    ),
                    column![
                        lbl(i18n.t("prompts.system_prompt_label")),
                        text_editor(&ed.system_editor)
                            .on_action(DashboardMsg::PromptSystemAction)
                            .font(ui_font)
                            .size(13)
                            .padding(10)
                            .height(Length::Fixed(112.0))
                            .style(sleek_editor_style(mode)),
                    ]
                    .spacing(5),
                    column![
                        lbl(i18n.t("prompts.user_template_label")),
                        text_editor(&ed.user_template_editor)
                            .on_action(DashboardMsg::PromptUserTemplateAction)
                            .font(ui_font)
                            .size(13)
                            .padding(10)
                            .height(Length::Fixed(96.0))
                            .style(sleek_editor_style(mode)),
                    ]
                    .spacing(5),
                    column![
                        lbl(i18n.t("prompts.vision_instruction_label")),
                        text_editor(&ed.vision_editor)
                            .on_action(DashboardMsg::PromptVisionAction)
                            .font(ui_font)
                            .size(13)
                            .padding(10)
                            .height(Length::Fixed(96.0))
                            .style(sleek_editor_style(mode)),
                    ]
                    .spacing(5),
                    rtl_row(
                        is_rtl,
                        vec![
                            button(
                                text(i18n.t("prompts.save_prompt"))
                                    .size(12.5)
                                    .font(ui_font_semi)
                                    .color(Color::WHITE),
                            )
                            .on_press(DashboardMsg::SavePromptPreset)
                            .padding([8, 16])
                            .style(primary_button_style(mode))
                            .into(),
                            button(text(i18n.t("prompts.cancel")).size(12.5).font(ui_font_semi),)
                                .on_press(DashboardMsg::CancelEditPrompt)
                                .padding([8, 14])
                                .style(pill_button_style(mode))
                                .into(),
                        ],
                    )
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(12)
                .align_x(h_align),
            )
            .padding(18)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
        );
    }

    let mut rows_col = column![];
    for (idx, pr) in config.prompts.iter().enumerate() {
        if idx > 0 {
            rows_col = rows_col.push(hairline_divider(mode));
        }
        let is_active = pr.id == config.active_prompt_id;
        let mut head_items: Vec<Element<'a, DashboardMsg>> = vec![
            text(&pr.name)
                .size(13.5)
                .font(font_for_text(&pr.name, None, Weight::Medium))
                .color(p.text_primary)
                .into(),
        ];
        if let Some(slot) = pr.shortcut_slot {
            head_items.push(
                container(
                    text(format!("Ctrl+Alt+{slot}"))
                        .size(10.5)
                        .font(ui_font_semi)
                        .color(p.accent_text),
                )
                .padding([2, 7])
                .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle))
                .into(),
            );
        }
        if is_active {
            head_items.push(
                container(
                    text(i18n.t("models.active_badge"))
                        .size(10.5)
                        .font(ui_font_semi)
                        .color(p.success),
                )
                .padding([2, 7])
                .style(badge_style(p.success_soft, p.success, p.border_subtle))
                .into(),
            );
        }

        let snippet: String = pr.system_prompt.chars().take(120).collect();
        let left = column![
            rtl_row(is_rtl, head_items)
                .spacing(8)
                .align_y(Alignment::Center),
            aligned_text(snippet, 12.0, ui_font, Some(p.text_secondary), is_rtl),
        ]
        .spacing(4)
        .align_x(h_align)
        .width(Length::Fill);

        let mut actions: Vec<Element<'a, DashboardMsg>> = Vec::new();
        if !is_active {
            actions.push(
                button(
                    text(i18n.t("prompts.set_active"))
                        .size(11.5)
                        .font(ui_font_semi)
                        .color(Color::WHITE)
                        .shaping(Shaping::Advanced),
                )
                .on_press(DashboardMsg::SelectPromptActive(pr.id.clone()))
                .padding([6, 12])
                .style(primary_button_style(mode))
                .into(),
            );
        }
        actions.push(
            square_icon_button(
                ui_icon(UiIcon::Edit, 14.0, p.text_primary),
                28.0,
                DashboardMsg::StartEditPrompt(pr.id.clone()),
                pill_button_style(mode),
            )
            .into(),
        );
        if config.prompts.len() > 1 {
            actions.push(
                square_icon_button(
                    ui_icon(UiIcon::Delete, 14.0, p.danger),
                    28.0,
                    DashboardMsg::DeletePrompt(pr.id.clone()),
                    danger_button_style(mode),
                )
                .into(),
            );
        }

        rows_col = rows_col.push(
            container(
                rtl_row(
                    is_rtl,
                    vec![
                        left.into(),
                        rtl_row(is_rtl, actions)
                            .spacing(6)
                            .align_y(Alignment::Center)
                            .into(),
                    ],
                )
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([12, 16])
            .width(Length::Fill),
        );
    }

    content = content.push(
        container(rows_col)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
    );

    sleek_scrollable(
        container(content)
            .padding(Padding::from([0, 6]))
            .width(Length::Fill),
        mode,
    )
    .height(Length::Fill)
    .into()
}

// ============================================================================
// 4. VOCABULARY TAB
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn view_vocab_tab<'a>(
    state: &'a DashboardState,
    i18n: &'a Translator,
    vocab: &'a [VocabEntry],
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let header = page_header(
        i18n.t("vocab.title"),
        Some(i18n.t("hints.vocab_sub")),
        Some(
            rtl_row(
                is_rtl,
                vec![
                    button(
                        rtl_row(
                            is_rtl,
                            vec![
                                ui_icon(UiIcon::Download, 14.0, p.text_primary).into(),
                                text(i18n.t("vocab.export_json"))
                                    .size(12)
                                    .font(ui_font_semi)
                                    .into(),
                            ],
                        )
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .on_press(DashboardMsg::ExportVocabJson)
                    .padding([7, 12])
                    .style(pill_button_style(mode))
                    .into(),
                    button(
                        rtl_row(
                            is_rtl,
                            vec![
                                ui_icon(UiIcon::Download, 14.0, p.text_primary).into(),
                                text(i18n.t("vocab.export_csv"))
                                    .size(12)
                                    .font(ui_font_semi)
                                    .into(),
                            ],
                        )
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .on_press(DashboardMsg::ExportVocabCsv)
                    .padding([7, 12])
                    .style(pill_button_style(mode))
                    .into(),
                    button(
                        rtl_row(
                            is_rtl,
                            vec![
                                ui_icon(UiIcon::Add, 15.0, Color::WHITE).into(),
                                text(i18n.t("vocab.add_manual"))
                                    .size(12)
                                    .font(ui_font_semi)
                                    .color(Color::WHITE)
                                    .into(),
                            ],
                        )
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .on_press(DashboardMsg::ToggleAddVocabForm)
                    .padding([7, 14])
                    .style(primary_button_style(mode))
                    .into(),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ),
        p,
        is_rtl,
        ui_font,
        ui_font_bold,
    );

    let search_bar = rtl_row(
        is_rtl,
        vec![
            ui_icon(UiIcon::Search, 16.0, p.text_muted).into(),
            text_input(&i18n.t("vocab.search_placeholder"), &state.vocab_search)
                .on_input(DashboardMsg::VocabSearchChanged)
                .font(ui_font)
                .size(13)
                .padding(9)
                .width(Length::Fill)
                .style(sleek_input_style(mode))
                .into(),
            container(
                text(format!("{} {}", vocab.len(), i18n.t("vocab.count_label")))
                    .size(12)
                    .font(ui_font_semi)
                    .color(p.text_secondary),
            )
            .padding([7, 12])
            .style(badge_style(p.bg_surface, p.text_secondary, p.border_subtle))
            .into(),
        ],
    )
    .spacing(10)
    .align_y(Alignment::Center);

    let mut content = column![header, search_bar].spacing(14);

    if let Some(status) = &state.export_status {
        content = content.push(aligned_text(
            status.as_str(),
            12.0,
            ui_font,
            Some(p.success),
            is_rtl,
        ));
    }

    if state.show_add_vocab_form {
        content = content.push(
            container(
                column![
                    rtl_row(
                        is_rtl,
                        vec![
                            text_input(&i18n.t("vocab.term_label"), &state.new_vocab_term)
                                .on_input(DashboardMsg::NewVocabTermChanged)
                                .font(ui_font)
                                .size(13)
                                .padding(9)
                                .width(Length::FillPortion(1))
                                .style(sleek_input_style(mode))
                                .into(),
                            text_input(
                                &i18n.t("vocab.translation_label"),
                                &state.new_vocab_translation,
                            )
                            .on_input(DashboardMsg::NewVocabTranslationChanged)
                            .font(ui_font)
                            .size(13)
                            .padding(9)
                            .width(Length::FillPortion(1))
                            .style(sleek_input_style(mode))
                            .into(),
                        ],
                    )
                    .spacing(10),
                    text_editor(&state.new_vocab_notes_editor)
                        .placeholder(i18n.t("vocab.notes_placeholder"))
                        .on_action(DashboardMsg::NewVocabNotesAction)
                        .font(ui_font)
                        .size(13)
                        .padding(9)
                        .height(Length::Fixed(82.0))
                        .style(sleek_editor_style(mode)),
                    rtl_row(
                        is_rtl,
                        vec![
                            button(
                                text(i18n.t("vocab.save"))
                                    .size(12.5)
                                    .font(ui_font_semi)
                                    .color(Color::WHITE),
                            )
                            .on_press(DashboardMsg::ConfirmAddVocab)
                            .padding([7, 15])
                            .style(primary_button_style(mode))
                            .into(),
                            button(text(i18n.t("models.cancel")).size(12.5).font(ui_font_semi),)
                                .on_press(DashboardMsg::ToggleAddVocabForm)
                                .padding([7, 13])
                                .style(pill_button_style(mode))
                                .into(),
                        ],
                    )
                    .spacing(8),
                ]
                .spacing(10)
                .align_x(h_align),
            )
            .padding(16)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
        );
    }

    let q = state.vocab_search.trim().to_lowercase();
    let filtered: Vec<&VocabEntry> = vocab
        .iter()
        .filter(|v| {
            q.is_empty()
                || v.term.to_lowercase().contains(&q)
                || v.translation.to_lowercase().contains(&q)
                || v.notes.to_lowercase().contains(&q)
        })
        .collect();

    if filtered.is_empty() {
        content = content.push(
            container(aligned_text(
                i18n.t("vocab.empty"),
                13.0,
                ui_font,
                Some(p.text_muted),
                is_rtl,
            ))
            .padding(24)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
        );
    } else {
        let mut rows_col = column![];
        for (idx, item) in filtered.iter().enumerate() {
            if idx > 0 {
                rows_col = rows_col.push(hairline_divider(mode));
            }
            let mut info = column![
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::StarFill, 14.0, p.warning).into(),
                        text(&item.term)
                            .size(13.5)
                            .font(font_for_text(&item.term, None, Weight::Medium))
                            .color(p.vocab_highlight_text)
                            .into(),
                        text("->").size(12).color(p.text_muted).into(),
                        text(&item.translation)
                            .size(13.5)
                            .font(font_for_text(&item.translation, None, Weight::Normal))
                            .color(p.text_primary)
                            .into(),
                    ],
                )
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(3)
            .align_x(h_align)
            .width(Length::Fill);

            if !item.notes.is_empty() {
                info = info.push(aligned_text(
                    item.notes.as_str(),
                    12.0,
                    ui_font,
                    Some(p.text_secondary),
                    is_rtl,
                ));
            }

            rows_col = rows_col.push(
                container(
                    rtl_row(
                        is_rtl,
                        vec![
                            info.into(),
                            square_icon_button(
                                ui_icon(UiIcon::Delete, 14.0, p.danger),
                                28.0,
                                DashboardMsg::DeleteVocab(item.id.clone()),
                                danger_button_style(mode),
                            )
                            .into(),
                        ],
                    )
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([12, 16])
                .width(Length::Fill),
            );
        }
        content = content.push(
            container(rows_col)
                .width(Length::Fill)
                .style(card_style(mode, 10.0)),
        );
    }

    sleek_scrollable(
        container(content)
            .padding(Padding::from([0, 6]))
            .width(Length::Fill),
        mode,
    )
    .height(Length::Fill)
    .into()
}

// ============================================================================
// 5. HISTORY TAB
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn view_history_tab<'a>(
    i18n: &'a Translator,
    history: &'a [HistoryEntry],
    vocab: &'a [VocabEntry],
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let header = page_header(
        i18n.t("history.title"),
        Some(i18n.t("hints.history_sub")),
        if history.is_empty() {
            None
        } else {
            Some(
                button(
                    rtl_row(
                        is_rtl,
                        vec![
                            ui_icon(UiIcon::Delete, 14.0, p.danger).into(),
                            text(i18n.t("history.clear_all"))
                                .size(12)
                                .font(ui_font_semi)
                                .into(),
                        ],
                    )
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(DashboardMsg::ClearHistory)
                .padding([7, 13])
                .style(danger_button_style(mode))
                .into(),
            )
        },
        p,
        is_rtl,
        ui_font,
        ui_font_bold,
    );

    let mut content = column![header].spacing(14);

    if history.is_empty() {
        content = content.push(
            container(aligned_text(
                i18n.t("history.empty"),
                13.0,
                ui_font,
                Some(p.text_muted),
                is_rtl,
            ))
            .padding(24)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
        );
    } else {
        let mut rows_col = column![];
        for (idx, item) in history.iter().enumerate() {
            if idx > 0 {
                rows_col = rows_col.push(hairline_divider(mode));
            }
            let mode_lbl = match item.mode {
                TranslationMode::Modal => i18n.t("history.mode_modal"),
                TranslationMode::InPlace => i18n.t("history.mode_inplace"),
                TranslationMode::Screenshot => i18n.t("history.mode_screenshot"),
                TranslationMode::Manual => i18n.t("history.mode_manual"),
            };
            let ts = item.created_at.format("%m-%d %H:%M").to_string();
            let is_starred =
                find_vocab_match_index(vocab, &item.source_text, &item.translated_text).is_some();

            let top_row = rtl_row(
                is_rtl,
                vec![
                    container(
                        text(mode_lbl)
                            .size(10.5)
                            .font(ui_font_semi)
                            .color(p.accent_text),
                    )
                    .padding([2, 8])
                    .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle))
                    .into(),
                    text(format!("{}  ·  {}", item.model_name, ts))
                        .size(11.5)
                        .font(ui_font)
                        .color(p.text_muted)
                        .width(Length::Fill)
                        .into(),
                    square_icon_button(
                        ui_icon(
                            if is_starred {
                                UiIcon::StarFill
                            } else {
                                UiIcon::Star
                            },
                            15.0,
                            if is_starred {
                                p.warning
                            } else {
                                p.text_secondary
                            },
                        ),
                        28.0,
                        DashboardMsg::ToggleHistoryStarVocab(item.clone()),
                        ghost_button_style(mode, is_starred),
                    )
                    .into(),
                    square_icon_button(
                        ui_icon(UiIcon::Copy, 14.0, p.text_secondary),
                        28.0,
                        DashboardMsg::CopyText(item.translated_text.clone()),
                        ghost_button_style(mode, false),
                    )
                    .into(),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill);

            let body = column![
                top_row,
                aligned_text(
                    item.source_text.as_str(),
                    12.5,
                    font_for_text(&item.source_text, None, Weight::Normal),
                    Some(p.text_secondary),
                    is_rtl,
                ),
                aligned_text(
                    item.translated_text.as_str(),
                    13.5,
                    font_for_text(
                        &item.translated_text,
                        Some(&item.target_lang),
                        Weight::Medium,
                    ),
                    Some(p.text_primary),
                    is_rtl,
                ),
            ]
            .spacing(5)
            .align_x(h_align);

            rows_col = rows_col.push(container(body).padding([12, 16]).width(Length::Fill));
        }

        content = content.push(
            container(rows_col)
                .width(Length::Fill)
                .style(card_style(mode, 10.0)),
        );
    }

    sleek_scrollable(
        container(content)
            .padding(Padding::from([0, 6]))
            .width(Length::Fill),
        mode,
    )
    .height(Length::Fill)
    .into()
}

// ============================================================================
// 6. SETTINGS TAB (Decluttered 5 Sub-Tabs + Instant Auto-Save)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn view_settings_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let auto_save_pill = container(
        rtl_row(
            is_rtl,
            vec![
                ui_icon(UiIcon::CheckCircle, 14.0, p.success).into(),
                text(i18n.t("settings.auto_save_hint"))
                    .size(11.5)
                    .font(ui_font_semi)
                    .color(p.success)
                    .into(),
            ],
        )
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([5, 11])
    .style(badge_style(p.success_soft, p.success, p.border_subtle));

    let header = page_header(
        i18n.t("settings.title"),
        Some(i18n.t("hints.settings_sub")),
        Some(auto_save_pill.into()),
        p,
        is_rtl,
        ui_font,
        ui_font_bold,
    );

    let sub_tabs = [
        (
            SettingsSubTab::General,
            UiIcon::Language,
            i18n.t("settings.tab_general"),
        ),
        (
            SettingsSubTab::Shortcuts,
            UiIcon::Keyboard,
            i18n.t("settings.tab_shortcuts"),
        ),
        (
            SettingsSubTab::Translation,
            UiIcon::Tune,
            i18n.t("settings.tab_translation"),
        ),
        (
            SettingsSubTab::Appearance,
            UiIcon::DarkMode,
            i18n.t("settings.tab_appearance"),
        ),
        (
            SettingsSubTab::Updates,
            UiIcon::Download,
            i18n.t("settings.tab_updates"),
        ),
    ];

    let mut sub_tab_items: Vec<Element<'a, DashboardMsg>> = Vec::new();
    for (sub, icon, label) in sub_tabs {
        let active = state.settings_subtab == sub;
        sub_tab_items.push(
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(
                            icon,
                            14.0,
                            if active {
                                p.accent_text
                            } else {
                                p.text_secondary
                            },
                        )
                        .into(),
                        text(label)
                            .size(12.5)
                            .font(if active { ui_font_semi } else { ui_font })
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::SwitchSettingsSubTab(sub))
            .padding([7, 13])
            .style(ghost_button_style(mode, active))
            .into(),
        );
    }

    let tab_bar = container(
        rtl_row(is_rtl, sub_tab_items)
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding([6, 8])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    let sub_body: Element<'a, DashboardMsg> = match state.settings_subtab {
        SettingsSubTab::General => {
            view_settings_general(config, i18n, mode, p, is_rtl, ui_font, ui_font_semi)
        }
        SettingsSubTab::Shortcuts => {
            view_settings_shortcuts(state, config, i18n, mode, p, is_rtl, ui_font, ui_font_semi)
        }
        SettingsSubTab::Translation => {
            view_settings_translation(config, i18n, mode, is_rtl, ui_font, ui_font_semi)
        }
        SettingsSubTab::Appearance => {
            view_settings_appearance(config, i18n, mode, is_rtl, ui_font, ui_font_bold)
        }
        SettingsSubTab::Updates => {
            view_settings_updates(state, config, i18n, mode, is_rtl, ui_font, ui_font_semi)
        }
    };

    let mut content = column![header, tab_bar, sub_body]
        .spacing(16)
        .align_x(h_align);

    if let Some(err) = &state.settings_error {
        content = content.push(aligned_text(
            err.as_str(),
            12.0,
            ui_font_semi,
            Some(p.danger),
            is_rtl,
        ));
    }
    if let Some(toast) = &state.settings_toast {
        content = content.push(aligned_text(
            toast.as_str(),
            12.0,
            ui_font_semi,
            Some(p.success),
            is_rtl,
        ));
    }

    sleek_scrollable(
        container(content)
            .padding(Padding::from([0, 8]))
            .width(Length::Fill),
        mode,
    )
    .height(Length::Fill)
    .into()
}

fn view_settings_general<'a>(
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let ui_langs = vec![
        UiLangChoice {
            code: "en",
            label: "English",
        },
        UiLangChoice {
            code: "fa",
            label: "Persian — فارسی",
        },
        UiLangChoice {
            code: "tr",
            label: "Turkish — Türkçe",
        },
    ];
    let sel_ui_lang = ui_langs
        .iter()
        .find(|u| u.code == config.ui_language)
        .cloned();

    let langs: Vec<PickerItem> = supported_languages()
        .iter()
        .filter(|l| l.code != "auto")
        .map(|l| PickerItem {
            id: l.code.to_string(),
            label: format!("{} — {}", l.name, l.native_name),
        })
        .collect();
    let sel_native = langs
        .iter()
        .find(|l| l.id == config.native_language)
        .cloned();
    let sel_target = langs
        .iter()
        .find(|l| l.id == config.target_language)
        .cloned();

    let lang_group = container(column![
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.language_label"),
            Some(i18n.t("hints.language_desc")),
            pick_list(ui_langs, sel_ui_lang, |c| {
                DashboardMsg::UiLanguageChanged(c.code.to_string())
            })
            .font(ui_font)
            .text_size(12.5)
            .padding([7, 12])
            .width(Length::Fixed(190.0))
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode))
            .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.native_language"),
            Some(i18n.t("settings.native_language_desc")),
            pick_list(langs.clone(), sel_native, |c| {
                DashboardMsg::SelectNativeLang(c.id)
            })
            .font(ui_font)
            .text_size(12.5)
            .padding([7, 12])
            .width(Length::Fixed(190.0))
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode))
            .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.target_language"),
            Some(i18n.t("settings.target_language_desc")),
            pick_list(langs, sel_target, |c| {
                DashboardMsg::SelectTargetLang(c.id)
            })
            .font(ui_font)
            .text_size(12.5)
            .padding([7, 12])
            .width(Length::Fixed(190.0))
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode))
            .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.rerun_onboarding"),
            Some(i18n.t("settings.rerun_onboarding_desc")),
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::RocketLaunch, 14.0, p.text_primary).into(),
                        text(i18n.t("settings.rerun_onboarding_btn"))
                            .size(12)
                            .font(ui_font_semi)
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::OpenOnboarding)
            .padding([7, 14])
            .style(pill_button_style(mode))
            .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    let system_group = container(column![
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.launch_at_startup"),
            Some(i18n.t("hints.launch_startup_desc")),
            toggler(config.launch_at_startup)
                .on_toggle(DashboardMsg::ToggleLaunchStartup)
                .size(19)
                .style(sleek_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.restore_clipboard"),
            Some(i18n.t("hints.restore_clipboard_desc")),
            toggler(config.restore_clipboard)
                .on_toggle(DashboardMsg::ToggleRestoreClipboard)
                .size(19)
                .style(sleek_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    let permissions = container(
        column![
            aligned_text(
                i18n.t("settings.platform_notes_title"),
                12.5,
                ui_font_semi,
                Some(p.accent_text),
                is_rtl,
            ),
            aligned_text(
                i18n.t("settings.platform_notes_desc"),
                12.0,
                ui_font,
                Some(p.text_secondary),
                is_rtl,
            ),
        ]
        .spacing(4)
        .align_x(h_align),
    )
    .padding([12, 16])
    .width(Length::Fill)
    .style(elevated_box_style(mode, 10.0));

    column![lang_group, system_group, permissions]
        .spacing(14)
        .width(Length::Fill)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn view_settings_shortcuts<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: ThemePalette,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let syntax_banner = container(
        rtl_row(
            is_rtl,
            vec![
                ui_icon(UiIcon::Keyboard, 16.0, p.accent_text).into(),
                aligned_text(
                    i18n.t("settings.hotkey_syntax_hint"),
                    12.0,
                    ui_font,
                    Some(p.text_primary),
                    is_rtl,
                ),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .style(elevated_box_style(mode, 9.0));

    let double_tap_group = container(column![
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.enable_double_alt"),
            Some(i18n.t("settings.double_alt_desc")),
            rtl_row(
                is_rtl,
                vec![
                    keycap("Alt", mode, ui_font_semi),
                    text("+").size(11).color(p.text_muted).into(),
                    keycap("Alt", mode, ui_font_semi),
                    Space::new().width(6).into(),
                    toggler(config.shortcuts.enable_double_alt)
                        .on_toggle(DashboardMsg::ToggleDoubleAlt)
                        .size(19)
                        .style(sleek_toggler_style(mode))
                        .into(),
                ],
            )
            .spacing(5)
            .align_y(Alignment::Center)
            .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.auto_select_all_fallback"),
            Some(i18n.t("settings.auto_select_all_desc")),
            toggler(config.shortcuts.auto_select_all_fallback)
                .on_toggle(DashboardMsg::ToggleAutoSelectFallback)
                .size(19)
                .style(sleek_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.enable_double_ctrl"),
            Some(i18n.t("settings.double_ctrl_desc")),
            rtl_row(
                is_rtl,
                vec![
                    keycap("Ctrl", mode, ui_font_semi),
                    text("+").size(11).color(p.text_muted).into(),
                    keycap("Ctrl", mode, ui_font_semi),
                    Space::new().width(6).into(),
                    toggler(config.shortcuts.enable_double_ctrl)
                        .on_toggle(DashboardMsg::ToggleDoubleCtrl)
                        .size(19)
                        .style(sleek_toggler_style(mode))
                        .into(),
                ],
            )
            .spacing(5)
            .align_y(Alignment::Center)
            .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.double_tap_ms"),
            Some(i18n.t("hints.double_tap_ms_desc")),
            text_input("340", &state.double_tap_ms_input)
                .on_input(DashboardMsg::DoubleTapMsChanged)
                .font(ui_font)
                .size(13)
                .padding([7, 10])
                .width(Length::Fixed(105.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    let hotkeys_group = container(column![
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.screenshot_hotkey"),
            None,
            text_input("Ctrl+Shift+X", &state.hotkey_screenshot)
                .on_input(DashboardMsg::HotkeyScreenshotChanged)
                .font(Font::MONOSPACE)
                .size(12.5)
                .padding([7, 10])
                .width(Length::Fixed(185.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.next_prompt_hotkey"),
            None,
            text_input("Ctrl+Alt+Right", &state.hotkey_next_prompt)
                .on_input(DashboardMsg::HotkeyNextPromptChanged)
                .font(Font::MONOSPACE)
                .size(12.5)
                .padding([7, 10])
                .width(Length::Fixed(185.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.prev_prompt_hotkey"),
            None,
            text_input("Ctrl+Alt+Left", &state.hotkey_prev_prompt)
                .on_input(DashboardMsg::HotkeyPrevPromptChanged)
                .font(Font::MONOSPACE)
                .size(12.5)
                .padding([7, 10])
                .width(Length::Fixed(185.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.dashboard_hotkey"),
            None,
            text_input("Ctrl+Shift+L", &state.hotkey_dashboard)
                .on_input(DashboardMsg::HotkeyDashboardChanged)
                .font(Font::MONOSPACE)
                .size(12.5)
                .padding([7, 10])
                .width(Length::Fixed(185.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    column![syntax_banner, double_tap_group, hotkeys_group]
        .spacing(14)
        .align_x(h_align)
        .width(Length::Fill)
        .into()
}

fn view_settings_translation<'a>(
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
) -> Element<'a, DashboardMsg> {
    let p = palette_for(mode);
    container(column![
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.preserve_markdown"),
            Some(i18n.t("settings.preserve_markdown_desc")),
            toggler(config.preserve_markdown)
                .on_toggle(DashboardMsg::TogglePreserveMarkdown)
                .size(19)
                .style(sleek_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.show_hud"),
            Some(i18n.t("hints.show_hud_desc")),
            toggler(config.show_hud)
                .on_toggle(DashboardMsg::ToggleShowHud)
                .size(19)
                .style(sleek_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("settings.reset_modal_pos"),
            Some(i18n.t("settings.reset_modal_pos_desc")),
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::RestartAlt, 14.0, p.text_primary).into(),
                        text(i18n.t("settings.reset_modal_pos_btn"))
                            .size(12)
                            .font(ui_font_semi)
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::ResetModalPosition)
            .padding([7, 14])
            .style(pill_button_style(mode))
            .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0))
    .into()
}

fn view_settings_appearance<'a>(
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    is_rtl: bool,
    ui_font: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let dark_selected = config.theme == UiThemeMode::Dark;
    let light_selected = config.theme == UiThemeMode::Light;

    let cards = rtl_row(
        is_rtl,
        vec![
            button(theme_preview_card_content(
                UiThemeMode::Dark,
                dark_selected,
                is_rtl,
                i18n.t("settings.theme_dark"),
                i18n.t("hints.theme_dark_desc"),
                mode,
                ui_font,
                ui_font_bold,
            ))
            .on_press(DashboardMsg::ThemeChanged(UiThemeMode::Dark))
            .padding(16)
            .width(Length::FillPortion(1))
            .style(selectable_card_style(mode, dark_selected, 12.0))
            .into(),
            button(theme_preview_card_content(
                UiThemeMode::Light,
                light_selected,
                is_rtl,
                i18n.t("settings.theme_light"),
                i18n.t("hints.theme_light_desc"),
                mode,
                ui_font,
                ui_font_bold,
            ))
            .on_press(DashboardMsg::ThemeChanged(UiThemeMode::Light))
            .padding(16)
            .width(Length::FillPortion(1))
            .style(selectable_card_style(mode, light_selected, 12.0))
            .into(),
        ],
    )
    .spacing(14)
    .width(Length::Fill);

    cards.into()
}

#[allow(clippy::too_many_arguments)]
fn view_settings_updates<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    is_rtl: bool,
    ui_font: Font,
    ui_font_semi: Font,
) -> Element<'a, DashboardMsg> {
    let p = palette_for(mode);
    let mut update_desc = format!(
        "{}: v{}",
        i18n.t("updates.current_version"),
        env!("CARGO_PKG_VERSION")
    );
    let mut action_items: Vec<Element<'a, DashboardMsg>> = vec![
        button(
            rtl_row(
                is_rtl,
                vec![
                    ui_icon(UiIcon::Refresh, 14.0, p.text_primary).into(),
                    text(if state.update_checking {
                        i18n.t("updates.checking")
                    } else {
                        i18n.t("updates.check_btn")
                    })
                    .size(12)
                    .font(ui_font_semi)
                    .into(),
                ],
            )
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .on_press(DashboardMsg::CheckUpdatesNow)
        .padding([7, 13])
        .style(pill_button_style(mode))
        .into(),
    ];

    if let Some(res) = &state.update_status {
        if res.has_update {
            update_desc = format!(
                "{}  ·  {}: v{}",
                update_desc,
                i18n.t("updates.available"),
                res.latest_version
            );
            action_items.push(
                button(
                    rtl_row(
                        is_rtl,
                        vec![
                            ui_icon(UiIcon::Download, 14.0, Color::WHITE).into(),
                            text(if state.update_downloading {
                                i18n.t("updates.downloading")
                            } else {
                                i18n.t("updates.download_install")
                            })
                            .size(12)
                            .font(ui_font_semi)
                            .color(Color::WHITE)
                            .into(),
                        ],
                    )
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(DashboardMsg::DownloadAndInstallUpdate)
                .padding([7, 14])
                .style(primary_button_style(mode))
                .into(),
            );
            action_items.push(
                button(
                    text(i18n.t("updates.open_release"))
                        .size(12)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::OpenReleaseUrl(res.release_url.clone()))
                .padding([7, 12])
                .style(pill_button_style(mode))
                .into(),
            );
        } else {
            update_desc = format!("{}  ·  {}", update_desc, i18n.t("updates.up_to_date"));
        }
    } else if let Some(err) = &state.update_error {
        update_desc = format!("{update_desc}  ·  {err}");
    }

    container(column![
        sleek_row(
            mode,
            is_rtl,
            i18n.t("updates.auto_check"),
            Some(i18n.t("hints.auto_update_desc")),
            toggler(config.auto_check_updates)
                .on_toggle(DashboardMsg::ToggleAutoCheckUpdates)
                .size(19)
                .style(sleek_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        sleek_row(
            mode,
            is_rtl,
            i18n.t("updates.section_title"),
            Some(update_desc),
            rtl_row(is_rtl, action_items)
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0))
    .into()
}
