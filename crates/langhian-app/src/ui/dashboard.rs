use crate::ui::theme::{
    AntigravityColors, antigravity_row, antigravity_toggler_style, badge_style, card_style,
    danger_button_style, elevated_box_style, font_for_text, font_for_ui, ghost_button_style,
    hairline_divider, keycap, nav_button_style, palette_for, pill_button_style,
    primary_button_style, render_highlighted_text, selectable_card_style, sidebar_style,
    sleek_editor_style, sleek_input_style, sleek_menu_style, sleek_pick_list_style,
};
use iced::font::Weight;
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_editor, text_input,
    toggler,
};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Padding};
use langhian_core::{
    AppConfig, CapabilitySource, HistoryEntry, ModelProfile, PromptPreset, ProviderKind,
    TranslationMode, Translator, UiThemeMode, UpdateCheckResult, VocabEntry, VocabMatcher,
    supported_languages,
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
    pub fetched_models: Vec<String>,
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
            fetched_models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptEditorState {
    pub editing_id: Option<String>,
    pub name: String,
    pub system_prompt: String,
    pub user_template: String,
    pub vision_instruction: String,
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
            system_prompt: preset.system_prompt.clone(),
            user_template: preset.user_template.clone(),
            vision_instruction: preset.vision_instruction.clone(),
            shortcut_slot: preset.shortcut_slot,
        }
    }
}

pub struct DashboardState {
    pub active_tab: DashboardTab,
    // Translate tab
    pub source_editor: text_editor::Content,
    pub manual_output: String,
    pub is_translating: bool,
    pub translate_error: Option<String>,
    pub copied_manual: bool,
    pub show_save_vocab_box: bool,
    pub save_term: String,
    pub save_translation: String,
    pub save_notes: String,
    pub selected_vocab_preview: Option<VocabEntry>,
    // Models tab
    pub model_editor: Option<ModelEditorState>,
    // Prompts tab
    pub prompt_editor: Option<PromptEditorState>,
    // Vocabulary tab
    pub vocab_search: String,
    pub show_add_vocab_form: bool,
    pub new_vocab_term: String,
    pub new_vocab_translation: String,
    pub new_vocab_notes: String,
    pub export_status: Option<String>,
    // Settings tab drafts
    pub double_tap_ms_draft: String,
    pub screenshot_hotkey_draft: String,
    pub next_prompt_hotkey_draft: String,
    pub prev_prompt_hotkey_draft: String,
    pub dashboard_hotkey_draft: String,
    pub settings_toast: Option<String>,
    // Updates
    pub update_checking: bool,
    pub update_result: Option<UpdateCheckResult>,
    pub update_error: Option<String>,
    pub update_downloading: bool,
}

impl DashboardState {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            active_tab: DashboardTab::Translate,
            source_editor: text_editor::Content::new(),
            manual_output: String::new(),
            is_translating: false,
            translate_error: None,
            copied_manual: false,
            show_save_vocab_box: false,
            save_term: String::new(),
            save_translation: String::new(),
            save_notes: String::new(),
            selected_vocab_preview: None,
            model_editor: None,
            prompt_editor: None,
            vocab_search: String::new(),
            show_add_vocab_form: false,
            new_vocab_term: String::new(),
            new_vocab_translation: String::new(),
            new_vocab_notes: String::new(),
            export_status: None,
            double_tap_ms_draft: config.shortcuts.double_tap_ms.to_string(),
            screenshot_hotkey_draft: config.shortcuts.screenshot.format_display(),
            next_prompt_hotkey_draft: config.shortcuts.next_prompt.format_display(),
            prev_prompt_hotkey_draft: config.shortcuts.prev_prompt.format_display(),
            dashboard_hotkey_draft: config.shortcuts.open_dashboard.format_display(),
            settings_toast: None,
            update_checking: false,
            update_result: None,
            update_error: None,
            update_downloading: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DashboardMsg {
    SwitchTab(DashboardTab),
    // Translate tab
    SourceEditorAction(text_editor::Action),
    SelectSourceLang(String),
    SelectTargetLang(String),
    SwapLangs,
    SelectModel(String),
    SelectPrompt(String),
    RunManualTranslate,
    TriggerScreenshot,
    ClearManual,
    CopyManualResult,
    ToggleSaveManualBox,
    SaveManualTermChanged(String),
    SaveManualTranslationChanged(String),
    SaveManualNotesChanged(String),
    ConfirmSaveManualVocab,
    ClickVocabHighlight(VocabEntry),
    DismissVocabPreview,

    // Models tab
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
    PickFetchedModel(String),
    TestModelConnection,
    FetchProviderModels,
    SaveModelProfile,

    // Prompts tab
    StartNewPrompt,
    StartEditPrompt(String),
    DeletePrompt(String),
    CancelEditPrompt,
    PromptNameChanged(String),
    PromptSlotChanged(SlotOption),
    PromptSystemChanged(String),
    PromptUserTemplateChanged(String),
    PromptVisionChanged(String),
    SavePromptPreset,

    // Vocabulary tab
    VocabSearchChanged(String),
    ToggleAddVocabForm,
    NewVocabTermChanged(String),
    NewVocabTranslationChanged(String),
    NewVocabNotesChanged(String),
    ConfirmAddVocab,
    DeleteVocab(String),
    ExportVocabJson,
    ExportVocabCsv,

    // History tab
    CopyText(String),
    SaveHistoryToVocab(HistoryEntry),
    ClearHistory,

    // Settings tab
    UiLanguageChanged(String),
    ThemeChanged(UiThemeMode),
    ToggleDoubleAlt(bool),
    ToggleAutoSelectFallback(bool),
    ToggleDoubleCtrl(bool),
    DoubleTapMsChanged(String),
    ScreenshotHotkeyChanged(String),
    NextPromptHotkeyChanged(String),
    PrevPromptHotkeyChanged(String),
    DashboardHotkeyChanged(String),
    ToggleRestoreClipboard(bool),
    ToggleLaunchStartup(bool),
    ToggleShowHud(bool),
    ToggleAutoCheckUpdates(bool),
    SaveSettings,
    CheckForUpdates,
    DownloadAndInstallUpdate,
    OpenReleaseUrl(String),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PickerItem {
    id: String,
    label: String,
}

impl fmt::Display for PickerItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotOption(pub Option<u8>);

impl fmt::Display for SlotOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(s) => write!(f, "Ctrl+Alt+{s}"),
            None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderChoice(ProviderKind);

impl fmt::Display for ProviderChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ProviderKind::GoogleTranslateFree => write!(f, "Google Translate (Free)"),
            ProviderKind::OpenAiCompatible => write!(f, "OpenAI-Compatible"),
            ProviderKind::Anthropic => write!(f, "Anthropic Claude"),
            ProviderKind::Gemini => write!(f, "Google Gemini"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UiLangChoice {
    code: &'static str,
    label: &'static str,
}

impl fmt::Display for UiLangChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
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
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Semibold);
    let ui_font_bold = font_for_ui(i18n.direction(), Weight::Bold);

    let sidebar = view_sidebar(
        state,
        config,
        i18n,
        mode,
        p,
        ui_font,
        ui_font_semi,
        ui_font_bold,
    );

    let main_view: Element<'a, DashboardMsg> = match state.active_tab {
        DashboardTab::Translate => view_translate_tab(
            state,
            config,
            i18n,
            highlighter,
            mode,
            p,
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
            ui_font,
            ui_font_semi,
            ui_font_bold,
        ),
        DashboardTab::History => {
            view_history_tab(i18n, history, mode, p, ui_font, ui_font_semi, ui_font_bold)
        }
        DashboardTab::Settings => view_settings_tab(
            state,
            config,
            i18n,
            mode,
            p,
            ui_font,
            ui_font_semi,
            ui_font_bold,
        ),
    };

    let v_divider =
        container(Space::new().width(1).height(Length::Fill)).style(move |_| container::Style {
            background: Some(Background::Color(p.border_subtle)),
            ..Default::default()
        });

    row![
        sidebar,
        v_divider,
        container(main_view)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::from([26, 32])),
    ]
    .height(Length::Fill)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn view_sidebar<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let brand = container(
        row![
            container(Space::new().width(10).height(10)).style(move |_| container::Style {
                background: Some(Background::Color(p.accent)),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            column![
                text(i18n.t("app.title"))
                    .size(15)
                    .font(ui_font_bold)
                    .color(p.text_primary),
                text(i18n.t("app.subtitle"))
                    .size(11)
                    .font(ui_font)
                    .color(p.text_muted),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([4, 8]));

    let nav_items = [
        (DashboardTab::Translate, i18n.t("nav.translate")),
        (DashboardTab::Models, i18n.t("nav.models")),
        (DashboardTab::Prompts, i18n.t("nav.prompts")),
        (DashboardTab::Vocabulary, i18n.t("nav.vocabulary")),
        (DashboardTab::History, i18n.t("nav.history")),
        (DashboardTab::Settings, i18n.t("nav.settings")),
    ];

    let mut nav_col = column![].spacing(4);
    for (tab, label) in nav_items {
        let is_active = state.active_tab == tab;
        let lbl_font = if is_active { ui_font_semi } else { ui_font };
        let btn = button(
            row![text(label).size(13).font(lbl_font)]
                .align_y(Alignment::Center)
                .width(Length::Fill),
        )
        .on_press(DashboardMsg::SwitchTab(tab))
        .padding([8, 12])
        .width(Length::Fill)
        .style(nav_button_style(mode, is_active));
        nav_col = nav_col.push(btn);
    }

    let active_model = config.active_model();
    let active_prompt = config.active_prompt();

    let status_dot = container(Space::new().width(7).height(7)).style(move |_| container::Style {
        background: Some(Background::Color(p.success)),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let footer = container(
        column![
            text(&active_model.name)
                .size(12)
                .font(font_for_text(&active_model.name, None, Weight::Semibold))
                .color(p.text_primary),
            text(&active_prompt.name)
                .size(11)
                .font(font_for_text(&active_prompt.name, None, Weight::Normal))
                .color(p.text_secondary),
            Space::new().height(4),
            row![
                status_dot,
                text(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(11)
                    .font(ui_font)
                    .color(p.text_muted),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(3),
    )
    .padding(12)
    .width(Length::Fill)
    .style(card_style(mode, 9.0));

    container(
        column![
            brand,
            Space::new().height(16),
            nav_col,
            Space::new().height(Length::Fill),
            footer,
        ]
        .height(Length::Fill),
    )
    .width(Length::Fixed(218.0))
    .height(Length::Fill)
    .padding(Padding::from([18, 14]))
    .style(sidebar_style(mode))
    .into()
}

fn page_header<'a, M: 'a>(
    title: impl Into<String>,
    subtitle: Option<String>,
    right_action: Option<Element<'a, M>>,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_bold: Font,
) -> Element<'a, M> {
    let mut left = column![
        text(title.into())
            .size(21)
            .font(ui_font_bold)
            .color(p.text_primary)
    ]
    .spacing(4)
    .width(Length::Fill);

    if let Some(sub) = subtitle {
        left = left.push(text(sub).size(13).font(ui_font).color(p.text_secondary));
    }

    let mut hdr = row![left].spacing(16).align_y(Alignment::Center);
    if let Some(act) = right_action {
        hdr = hdr.push(act);
    }
    hdr.into()
}

#[allow(clippy::too_many_arguments)]
fn view_translate_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    highlighter: &'a VocabMatcher,
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
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

    // Two clean rows in the control bar so pick_lists NEVER overflow off the right edge!
    let lang_row = row![
        pick_list(src_langs, sel_src, |x| DashboardMsg::SelectSourceLang(x.id))
            .font(ui_font)
            .text_size(12)
            .padding([7, 11])
            .width(Length::FillPortion(3))
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode)),
        button(
            text(i18n.t("translate.swap_langs"))
                .size(12)
                .font(ui_font_semi)
        )
        .on_press(DashboardMsg::SwapLangs)
        .padding([7, 12])
        .style(pill_button_style(mode)),
        pick_list(dst_langs, sel_dst, |x| DashboardMsg::SelectTargetLang(x.id))
            .font(ui_font)
            .text_size(12)
            .padding([7, 11])
            .width(Length::FillPortion(3))
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let pipeline_row = row![
        row![
            text(i18n.t("tray.active_model"))
                .size(11)
                .font(ui_font_semi)
                .color(p.text_muted),
            pick_list(model_opts, sel_model, |x| DashboardMsg::SelectModel(x.id))
                .font(ui_font)
                .text_size(12)
                .padding([6, 10])
                .width(Length::Fill)
                .style(sleek_pick_list_style(mode))
                .menu_style(sleek_menu_style(mode)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(3)),
        row![
            text(i18n.t("tray.active_prompt"))
                .size(11)
                .font(ui_font_semi)
                .color(p.text_muted),
            pick_list(prompt_opts, sel_prompt, |x| DashboardMsg::SelectPrompt(
                x.id
            ))
            .font(ui_font)
            .text_size(12)
            .padding([6, 10])
            .width(Length::Fill)
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(2)),
    ]
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
                text(i18n.t("translate.screenshot_btn"))
                    .size(12)
                    .font(ui_font_semi),
            )
            .on_press(DashboardMsg::TriggerScreenshot)
            .padding([7, 14])
            .style(pill_button_style(mode))
            .into(),
        ),
        p,
        ui_font,
        ui_font_bold,
    );

    // Side-by-side Source & Output cards
    let source_pane = container(
        column![
            text(i18n.t("translate.source_label"))
                .size(12)
                .font(ui_font_semi)
                .color(p.text_secondary),
            text_editor(&state.source_editor)
                .placeholder(i18n.t("translate.source_placeholder"))
                .on_action(DashboardMsg::SourceEditorAction)
                .font(ui_font)
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

    let output_body: Element<'a, DashboardMsg> = if state.is_translating {
        text(i18n.t("app.status_translating"))
            .size(14)
            .font(ui_font_semi)
            .color(p.text_secondary)
            .into()
    } else if let Some(err) = &state.translate_error {
        text(err.clone())
            .size(13)
            .font(ui_font)
            .color(p.danger)
            .into()
    } else if state.manual_output.is_empty() {
        text(i18n.t("translate.result_placeholder"))
            .size(14)
            .font(ui_font)
            .color(p.text_muted)
            .into()
    } else {
        scrollable(render_highlighted_text(
            &state.manual_output,
            Some(&config.target_language),
            highlighter,
            DashboardMsg::ClickVocabHighlight,
            mode,
            15.0,
        ))
        .height(Length::Fill)
        .into()
    };

    let output_pane = container(
        column![
            row![
                text(i18n.t("translate.target_label"))
                    .size(12)
                    .font(ui_font_semi)
                    .color(p.text_secondary)
                    .width(Length::Fill),
                text(config.target_language.to_uppercase())
                    .size(11)
                    .font(ui_font_semi)
                    .color(p.accent_text),
            ]
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

    let mut content = column![
        header,
        control_card,
        row![source_pane, output_pane]
            .spacing(14)
            .height(Length::Fill),
    ]
    .spacing(14)
    .height(Length::Fill);

    if let Some(entry) = &state.selected_vocab_preview {
        content = content.push(
            container(
                row![
                    text(format!(
                        "{} -> {}   {}",
                        entry.term, entry.translation, entry.notes
                    ))
                    .size(12)
                    .font(font_for_text(&entry.term, None, Weight::Semibold))
                    .color(p.highlight_text)
                    .width(Length::Fill),
                    button(text("✕").size(11))
                        .on_press(DashboardMsg::DismissVocabPreview)
                        .padding([4, 8])
                        .style(ghost_button_style(mode, false)),
                ]
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .width(Length::Fill)
            .style(elevated_box_style(mode, 8.0)),
        );
    }

    if state.show_save_vocab_box {
        content = content.push(
            container(
                row![
                    text_input(&i18n.t("translate.word_label"), &state.save_term)
                        .on_input(DashboardMsg::SaveManualTermChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([7, 10])
                        .style(sleek_input_style(mode)),
                    text_input(&i18n.t("translate.meaning_label"), &state.save_translation)
                        .on_input(DashboardMsg::SaveManualTranslationChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([7, 10])
                        .style(sleek_input_style(mode)),
                    text_input(&i18n.t("translate.notes_label"), &state.save_notes)
                        .on_input(DashboardMsg::SaveManualNotesChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([7, 10])
                        .style(sleek_input_style(mode)),
                    button(text(i18n.t("vocab.save")).size(12).font(ui_font_semi))
                        .on_press(DashboardMsg::ConfirmSaveManualVocab)
                        .padding([7, 14])
                        .style(primary_button_style(mode)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(10)
            .style(elevated_box_style(mode, 8.0)),
        );
    }

    let copy_lbl = if state.copied_manual {
        i18n.t("translate.copied")
    } else {
        i18n.t("translate.copy_btn")
    };

    let actions = row![
        button(
            text(i18n.t("translate.translate_btn"))
                .size(13)
                .font(ui_font_semi)
        )
        .on_press(DashboardMsg::RunManualTranslate)
        .padding([8, 18])
        .style(primary_button_style(mode)),
        button(text(copy_lbl).size(12).font(ui_font_semi))
            .on_press(DashboardMsg::CopyManualResult)
            .padding([8, 14])
            .style(pill_button_style(mode)),
        button(
            text(i18n.t("translate.save_btn"))
                .size(12)
                .font(ui_font_semi)
        )
        .on_press(DashboardMsg::ToggleSaveManualBox)
        .padding([8, 14])
        .style(pill_button_style(mode)),
        Space::new().width(Length::Fill),
        button(text(i18n.t("translate.clear_btn")).size(12).font(ui_font))
            .on_press(DashboardMsg::ClearManual)
            .padding([8, 14])
            .style(ghost_button_style(mode, false)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    content.push(actions).into()
}

#[allow(clippy::too_many_arguments)]
fn view_models_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let header = page_header(
        i18n.t("models.title"),
        Some(i18n.t("hints.models_sub")),
        None,
        p,
        ui_font,
        ui_font_bold,
    );

    let preset_card =
        |label: String, sub: &'static str, kind: ModelPresetKind| -> Element<'a, DashboardMsg> {
            button(
                column![
                    text(label)
                        .size(13)
                        .font(ui_font_semi)
                        .color(p.text_primary),
                    text(sub).size(11).font(ui_font).color(p.text_muted),
                ]
                .spacing(2),
            )
            .on_press(DashboardMsg::StartAddPreset(kind))
            .padding([10, 12])
            .width(Length::FillPortion(1))
            .style(selectable_card_style(mode, false, 8.0))
            .into()
        };

    // 2 balanced rows of 4 cards — zero clipping, everything fits comfortably at 960px!
    let quick_add_grid = column![
        row![
            preset_card(
                i18n.t("models.preset_openrouter"),
                "Multi-provider",
                ModelPresetKind::OpenRouter
            ),
            preset_card(
                i18n.t("models.preset_openai"),
                "GPT models",
                ModelPresetKind::OpenAi
            ),
            preset_card(
                "Claude".to_string(),
                "Anthropic API",
                ModelPresetKind::Anthropic
            ),
            preset_card("Gemini".to_string(), "Google AI", ModelPresetKind::Gemini),
        ]
        .spacing(10),
        row![
            preset_card(
                i18n.t("models.preset_ollama"),
                "Localhost :11434",
                ModelPresetKind::Ollama
            ),
            preset_card(
                i18n.t("models.preset_lmstudio"),
                "Localhost :1234",
                ModelPresetKind::LmStudio
            ),
            preset_card(
                "Google Free".to_string(),
                "No API key",
                ModelPresetKind::GoogleFree
            ),
            preset_card(
                i18n.t("models.preset_custom"),
                "Custom Base URL",
                ModelPresetKind::Custom
            ),
        ]
        .spacing(10),
    ]
    .spacing(10);

    let mut content = column![header, quick_add_grid].spacing(18);

    if let Some(ed) = &state.model_editor {
        content = content.push(view_model_editor(
            ed,
            i18n,
            mode,
            p,
            ui_font,
            ui_font_semi,
            ui_font_bold,
        ));
    }

    // Grouped Antigravity container for saved models
    let mut rows_col = column![];
    for (idx, m) in config.models.iter().enumerate() {
        if idx > 0 {
            rows_col = rows_col.push(hairline_divider(mode));
        }
        let is_active = m.id == config.active_model_id;
        let cap_label = if m.supports_vision {
            i18n.t("models.vision_badge")
        } else {
            i18n.t("models.text_only_badge")
        };

        let mut badges =
            row![
                text(&m.name)
                    .size(13)
                    .font(font_for_text(&m.name, None, Weight::Semibold))
                    .color(p.text_primary),
                container(text(cap_label).size(10).font(ui_font_semi).color(
                    if m.supports_vision {
                        p.accent_text
                    } else {
                        p.text_muted
                    }
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
                )),
            ]
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
                    Color::from_rgba(0.20, 0.83, 0.60, 0.14),
                    p.success,
                    Color::from_rgba(0.20, 0.83, 0.60, 0.32),
                )),
            );
        }

        let sub = format!("{}  ·  {}", m.model_id, m.base_url);
        let left_col = column![
            badges,
            text(sub).size(12).font(ui_font).color(p.text_secondary),
        ]
        .spacing(3)
        .width(Length::Fill);

        let mut actions = row![].spacing(6).align_y(Alignment::Center);
        if !is_active {
            actions = actions.push(
                button(
                    text(i18n.t("models.set_active"))
                        .size(11)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::SelectModel(m.id.clone()))
                .padding([5, 11])
                .style(primary_button_style(mode)),
            );
        }
        actions = actions
            .push(
                button(
                    text(i18n.t("models.edit_profile"))
                        .size(11)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::StartEditModel(m.id.clone()))
                .padding([5, 11])
                .style(pill_button_style(mode)),
            )
            .push(
                button(
                    text(i18n.t("models.clone_profile"))
                        .size(11)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::DuplicateModel(m.id.clone()))
                .padding([5, 10])
                .style(pill_button_style(mode)),
            );
        if config.models.len() > 1 {
            actions = actions.push(
                button(
                    text(i18n.t("models.delete_profile"))
                        .size(11)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::DeleteModel(m.id.clone()))
                .padding([5, 10])
                .style(danger_button_style(mode)),
            );
        }

        rows_col = rows_col.push(
            container(
                row![left_col, actions]
                    .spacing(14)
                    .align_y(Alignment::Center),
            )
            .padding([13, 16])
            .width(Length::Fill),
        );
    }

    content = content.push(
        container(rows_col)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
    );

    scrollable(container(content).padding(Padding {
        top: 0.0,
        right: 4.0,
        bottom: 24.0,
        left: 0.0,
    }))
    .height(Length::Fill)
    .into()
}

fn view_model_editor<'a>(
    ed: &'a ModelEditorState,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let provider_choices = vec![
        ProviderChoice(ProviderKind::OpenAiCompatible),
        ProviderChoice(ProviderKind::GoogleTranslateFree),
        ProviderChoice(ProviderKind::Anthropic),
        ProviderChoice(ProviderKind::Gemini),
    ];
    let selected_provider = Some(ProviderChoice(ed.provider));

    let heading = if ed.editing_id.is_some() {
        i18n.t("models.edit_profile")
    } else {
        i18n.t("models.add_profile")
    };

    let labeled = |lbl: String, widget: Element<'a, DashboardMsg>| -> Element<'a, DashboardMsg> {
        column![
            text(lbl)
                .size(12)
                .font(ui_font_semi)
                .color(p.text_secondary),
            widget,
        ]
        .spacing(5)
        .width(Length::FillPortion(1))
        .into()
    };

    let row_name_provider = row![
        labeled(
            i18n.t("models.name_label"),
            text_input("OpenRouter / Ollama", &ed.name)
                .on_input(DashboardMsg::ModelNameChanged)
                .font(ui_font)
                .size(13)
                .padding([8, 12])
                .style(sleek_input_style(mode))
                .into(),
        ),
        labeled(
            i18n.t("models.provider_label"),
            pick_list(provider_choices, selected_provider, |c| {
                DashboardMsg::ModelProviderChanged(c.0)
            })
            .font(ui_font)
            .text_size(13)
            .padding([8, 12])
            .width(Length::Fill)
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode))
            .into(),
        ),
    ]
    .spacing(12);

    let mut form_rows = column![
        text(heading)
            .size(15)
            .font(ui_font_bold)
            .color(p.text_primary),
        row_name_provider,
    ]
    .spacing(12);

    if ed.provider != ProviderKind::GoogleTranslateFree {
        let model_id_input = row![
            text_input("openai/gpt-4o-mini / qwen2.5:7b", &ed.model_id)
                .on_input(DashboardMsg::ModelIdChanged)
                .font(ui_font)
                .size(13)
                .padding([8, 12])
                .width(Length::Fill)
                .style(sleek_input_style(mode)),
            button(
                text(i18n.t("models.fetch_models_btn"))
                    .size(12)
                    .font(ui_font_semi)
            )
            .on_press(DashboardMsg::FetchProviderModels)
            .padding([8, 12])
            .style(pill_button_style(mode)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        form_rows = form_rows.push(
            row![
                labeled(
                    i18n.t("models.base_url_label"),
                    text_input("https://openrouter.ai/api/v1", &ed.base_url)
                        .on_input(DashboardMsg::ModelBaseUrlChanged)
                        .font(ui_font)
                        .size(13)
                        .padding([8, 12])
                        .style(sleek_input_style(mode))
                        .into(),
                ),
                labeled(i18n.t("models.model_id_label"), model_id_input.into()),
            ]
            .spacing(12),
        );

        if !ed.fetched_models.is_empty() {
            let fetched_picker = pick_list(
                ed.fetched_models.clone(),
                None::<String>,
                DashboardMsg::PickFetchedModel,
            )
            .placeholder("Choose a model returned by this endpoint...")
            .font(ui_font)
            .text_size(12)
            .padding([7, 12])
            .width(Length::Fill)
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode));
            form_rows = form_rows.push(fetched_picker);
        }

        form_rows = form_rows.push(
            column![
                text(i18n.t("models.api_key_label"))
                    .size(12)
                    .font(ui_font_semi)
                    .color(p.text_secondary),
                text_input(&i18n.t("models.api_key_placeholder"), &ed.api_key)
                    .secure(true)
                    .on_input(DashboardMsg::ModelApiKeyChanged)
                    .font(ui_font)
                    .size(13)
                    .padding([8, 12])
                    .style(sleek_input_style(mode)),
            ]
            .spacing(5),
        );

        // Proper multiline editor for custom headers (no clipping!)
        form_rows = form_rows.push(
            column![
                text(i18n.t("models.custom_headers_label"))
                    .size(12)
                    .font(ui_font_semi)
                    .color(p.text_secondary),
                text_editor(&ed.headers_editor)
                    .placeholder("HTTP-Referer: https://github.com/hamides-dev/langhian\nX-Title: Langhian Translation")
                    .on_action(DashboardMsg::ModelHeadersAction)
                    .font(ui_font)
                    .size(12)
                    .padding(10)
                    .height(Length::Fixed(74.0))
                    .style(sleek_editor_style(mode)),
            ]
            .spacing(5),
        );

        form_rows = form_rows.push(
            row![
                labeled(
                    i18n.t("models.temperature_label"),
                    text_input("0.2", &ed.temperature)
                        .on_input(DashboardMsg::ModelTempChanged)
                        .font(ui_font)
                        .size(13)
                        .padding([8, 12])
                        .style(sleek_input_style(mode))
                        .into(),
                ),
                labeled(
                    i18n.t("models.max_tokens_label"),
                    text_input("Optional", &ed.max_tokens)
                        .on_input(DashboardMsg::ModelMaxTokensChanged)
                        .font(ui_font)
                        .size(13)
                        .padding([8, 12])
                        .style(sleek_input_style(mode))
                        .into(),
                ),
            ]
            .spacing(12),
        );

        form_rows = form_rows.push(
            container(
                row![
                    text(i18n.t("models.supports_vision_label"))
                        .size(13)
                        .font(ui_font_semi)
                        .color(p.text_primary)
                        .width(Length::Fill),
                    toggler(ed.supports_vision)
                        .on_toggle(DashboardMsg::ModelVisionToggled)
                        .size(18)
                        .style(antigravity_toggler_style(mode)),
                ]
                .align_y(Alignment::Center),
            )
            .padding([10, 14])
            .width(Length::Fill)
            .style(elevated_box_style(mode, 8.0)),
        );
    }

    let test_label = if ed.is_probing {
        i18n.t("models.testing")
    } else {
        i18n.t("models.test_btn")
    };

    let mut footer_row = row![
        button(text(test_label).size(12).font(ui_font_semi))
            .on_press(DashboardMsg::TestModelConnection)
            .padding([8, 14])
            .style(pill_button_style(mode)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    if let Some((ok, detail)) = &ed.probe_status {
        footer_row = footer_row.push(
            text(detail.clone())
                .size(12)
                .font(ui_font)
                .color(if *ok { p.success } else { p.danger })
                .width(Length::Fill),
        );
    } else {
        footer_row = footer_row.push(Space::new().width(Length::Fill));
    }

    footer_row = footer_row
        .push(
            button(text(i18n.t("models.cancel")).size(12).font(ui_font_semi))
                .on_press(DashboardMsg::CancelEditModel)
                .padding([8, 14])
                .style(ghost_button_style(mode, false)),
        )
        .push(
            button(
                text(i18n.t("models.save_profile"))
                    .size(12)
                    .font(ui_font_semi),
            )
            .on_press(DashboardMsg::SaveModelProfile)
            .padding([8, 16])
            .style(primary_button_style(mode)),
        );

    form_rows = form_rows.push(footer_row);

    container(form_rows)
        .padding(18)
        .width(Length::Fill)
        .style(card_style(mode, 10.0))
        .into()
}

#[allow(clippy::too_many_arguments)]
fn view_prompts_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let header = page_header(
        i18n.t("prompts.title"),
        Some(i18n.t("hints.prompts_sub")),
        Some(
            button(
                text(i18n.t("prompts.add_prompt"))
                    .size(12)
                    .font(ui_font_semi),
            )
            .on_press(DashboardMsg::StartNewPrompt)
            .padding([8, 14])
            .style(primary_button_style(mode))
            .into(),
        ),
        p,
        ui_font,
        ui_font_bold,
    );

    let mut content = column![header].spacing(18);

    if let Some(ed) = &state.prompt_editor {
        let mut slots = vec![SlotOption(None)];
        for n in 1..=9 {
            slots.push(SlotOption(Some(n)));
        }
        let heading = if ed.editing_id.is_some() {
            i18n.t("prompts.edit_prompt")
        } else {
            i18n.t("prompts.add_prompt")
        };

        let editor_card = container(
            column![
                text(heading)
                    .size(15)
                    .font(ui_font_bold)
                    .color(p.text_primary),
                row![
                    column![
                        text(i18n.t("prompts.name_label"))
                            .size(12)
                            .font(ui_font_semi)
                            .color(p.text_secondary),
                        text_input("Academic / Concise", &ed.name)
                            .on_input(DashboardMsg::PromptNameChanged)
                            .font(ui_font)
                            .size(13)
                            .padding([8, 12])
                            .style(sleek_input_style(mode)),
                    ]
                    .spacing(5)
                    .width(Length::FillPortion(2)),
                    column![
                        text(i18n.t("prompts.slot_label"))
                            .size(12)
                            .font(ui_font_semi)
                            .color(p.text_secondary),
                        pick_list(
                            slots,
                            Some(SlotOption(ed.shortcut_slot)),
                            DashboardMsg::PromptSlotChanged
                        )
                        .font(ui_font)
                        .text_size(13)
                        .padding([8, 12])
                        .width(Length::Fill)
                        .style(sleek_pick_list_style(mode))
                        .menu_style(sleek_menu_style(mode)),
                    ]
                    .spacing(5)
                    .width(Length::FillPortion(1)),
                ]
                .spacing(12),
                column![
                    text(i18n.t("prompts.system_prompt_label"))
                        .size(12)
                        .font(ui_font_semi)
                        .color(p.text_secondary),
                    text_input(
                        "System instructions for the AI translator...",
                        &ed.system_prompt
                    )
                    .on_input(DashboardMsg::PromptSystemChanged)
                    .font(ui_font)
                    .size(13)
                    .padding([8, 12])
                    .style(sleek_input_style(mode)),
                ]
                .spacing(5),
                column![
                    text(i18n.t("prompts.user_template_label"))
                        .size(12)
                        .font(ui_font_semi)
                        .color(p.text_secondary),
                    text_input(
                        "Translate from {source_lang} to {target_lang}:\n\n{text}",
                        &ed.user_template
                    )
                    .on_input(DashboardMsg::PromptUserTemplateChanged)
                    .font(ui_font)
                    .size(13)
                    .padding([8, 12])
                    .style(sleek_input_style(mode)),
                ]
                .spacing(5),
                column![
                    text(i18n.t("prompts.vision_instruction_label"))
                        .size(12)
                        .font(ui_font_semi)
                        .color(p.text_secondary),
                    text_input(
                        "Extract and translate visible text into {target_lang}.",
                        &ed.vision_instruction
                    )
                    .on_input(DashboardMsg::PromptVisionChanged)
                    .font(ui_font)
                    .size(13)
                    .padding([8, 12])
                    .style(sleek_input_style(mode)),
                ]
                .spacing(5),
                row![
                    text(i18n.t("prompts.template_vars_hint"))
                        .size(11)
                        .font(ui_font)
                        .color(p.text_muted),
                    keycap(mode, "{text}", ui_font_semi),
                    keycap(mode, "{source_lang}", ui_font_semi),
                    keycap(mode, "{target_lang}", ui_font_semi),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                row![
                    Space::new().width(Length::Fill),
                    button(text(i18n.t("prompts.cancel")).size(12).font(ui_font_semi))
                        .on_press(DashboardMsg::CancelEditPrompt)
                        .padding([8, 14])
                        .style(ghost_button_style(mode, false)),
                    button(
                        text(i18n.t("prompts.save_prompt"))
                            .size(12)
                            .font(ui_font_semi)
                    )
                    .on_press(DashboardMsg::SavePromptPreset)
                    .padding([8, 16])
                    .style(primary_button_style(mode)),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .style(card_style(mode, 10.0));

        content = content.push(editor_card);
    }

    let mut list_col = column![];
    for (idx, pr) in config.prompts.iter().enumerate() {
        if idx > 0 {
            list_col = list_col.push(hairline_divider(mode));
        }
        let is_active = pr.id == config.active_prompt_id;

        let mut title_row = row![
            text(&pr.name)
                .size(13)
                .font(font_for_text(&pr.name, None, Weight::Semibold))
                .color(p.text_primary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if let Some(slot) = pr.shortcut_slot {
            title_row =
                title_row.push(keycap_owned(mode, format!("Ctrl+Alt+{slot}"), ui_font_semi));
        }
        if is_active {
            title_row = title_row.push(
                container(
                    text(i18n.t("models.active_badge"))
                        .size(10)
                        .font(ui_font_semi)
                        .color(p.success),
                )
                .padding([2, 7])
                .style(badge_style(
                    Color::from_rgba(0.20, 0.83, 0.60, 0.14),
                    p.success,
                    Color::from_rgba(0.20, 0.83, 0.60, 0.32),
                )),
            );
        }

        let left = column![
            title_row,
            text(&pr.system_prompt)
                .size(12)
                .font(font_for_text(&pr.system_prompt, None, Weight::Normal))
                .color(p.text_secondary),
        ]
        .spacing(3)
        .width(Length::Fill);

        let mut actions = row![].spacing(6).align_y(Alignment::Center);
        if !is_active {
            actions = actions.push(
                button(
                    text(i18n.t("prompts.set_active"))
                        .size(11)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::SelectPrompt(pr.id.clone()))
                .padding([5, 11])
                .style(primary_button_style(mode)),
            );
        }
        actions = actions.push(
            button(text(i18n.t("vocab.edit")).size(11).font(ui_font_semi))
                .on_press(DashboardMsg::StartEditPrompt(pr.id.clone()))
                .padding([5, 11])
                .style(pill_button_style(mode)),
        );
        if config.prompts.len() > 1 {
            actions = actions.push(
                button(
                    text(i18n.t("prompts.delete_prompt"))
                        .size(11)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::DeletePrompt(pr.id.clone()))
                .padding([5, 10])
                .style(danger_button_style(mode)),
            );
        }

        list_col = list_col.push(
            container(row![left, actions].spacing(14).align_y(Alignment::Center))
                .padding([13, 16])
                .width(Length::Fill),
        );
    }

    content = content.push(
        container(list_col)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
    );

    scrollable(container(content).padding(Padding {
        top: 0.0,
        right: 4.0,
        bottom: 24.0,
        left: 0.0,
    }))
    .height(Length::Fill)
    .into()
}

fn keycap_owned<'a, M: 'a>(mode: UiThemeMode, label: String, font: Font) -> Element<'a, M> {
    let p = palette_for(mode);
    container(text(label).size(11).font(font).color(p.text_primary))
        .padding([2, 7])
        .style(crate::ui::theme::keycap_style(mode))
        .into()
}

#[allow(clippy::too_many_arguments)]
fn view_vocab_tab<'a>(
    state: &'a DashboardState,
    i18n: &'a Translator,
    vocab: &'a [VocabEntry],
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let header_actions = row![
        button(text(i18n.t("vocab.add_manual")).size(12).font(ui_font_semi))
            .on_press(DashboardMsg::ToggleAddVocabForm)
            .padding([7, 13])
            .style(primary_button_style(mode)),
        button(
            text(i18n.t("vocab.export_json"))
                .size(12)
                .font(ui_font_semi)
        )
        .on_press(DashboardMsg::ExportVocabJson)
        .padding([7, 12])
        .style(pill_button_style(mode)),
        button(text(i18n.t("vocab.export_csv")).size(12).font(ui_font_semi))
            .on_press(DashboardMsg::ExportVocabCsv)
            .padding([7, 12])
            .style(pill_button_style(mode)),
    ]
    .spacing(8);

    let header = page_header(
        format!("{} ({})", i18n.t("vocab.title"), vocab.len()),
        Some(i18n.t("hints.vocab_sub")),
        Some(header_actions.into()),
        p,
        ui_font,
        ui_font_bold,
    );

    let search = text_input(&i18n.t("vocab.search_placeholder"), &state.vocab_search)
        .on_input(DashboardMsg::VocabSearchChanged)
        .font(ui_font)
        .size(13)
        .padding([9, 14])
        .style(sleek_input_style(mode));

    let mut content = column![header, search].spacing(14);

    if let Some(st) = &state.export_status {
        content = content.push(text(st.clone()).size(12).font(ui_font).color(p.success));
    }

    if state.show_add_vocab_form {
        content = content.push(
            container(
                row![
                    text_input(&i18n.t("vocab.term_label"), &state.new_vocab_term)
                        .on_input(DashboardMsg::NewVocabTermChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([8, 10])
                        .style(sleek_input_style(mode)),
                    text_input(
                        &i18n.t("vocab.translation_label"),
                        &state.new_vocab_translation
                    )
                    .on_input(DashboardMsg::NewVocabTranslationChanged)
                    .font(ui_font)
                    .size(12)
                    .padding([8, 10])
                    .style(sleek_input_style(mode)),
                    text_input(&i18n.t("vocab.notes_label"), &state.new_vocab_notes)
                        .on_input(DashboardMsg::NewVocabNotesChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([8, 10])
                        .style(sleek_input_style(mode)),
                    button(text(i18n.t("vocab.save")).size(12).font(ui_font_semi))
                        .on_press(DashboardMsg::ConfirmAddVocab)
                        .padding([8, 14])
                        .style(primary_button_style(mode)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(12)
            .style(card_style(mode, 10.0)),
        );
    }

    let q = state.vocab_search.trim().to_lowercase();
    let filtered: Vec<&VocabEntry> = vocab
        .iter()
        .filter(|e| {
            q.is_empty()
                || e.term.to_lowercase().contains(&q)
                || e.translation.to_lowercase().contains(&q)
                || e.notes.to_lowercase().contains(&q)
        })
        .collect();

    if filtered.is_empty() {
        content = content.push(
            container(
                text(i18n.t("vocab.empty"))
                    .size(13)
                    .font(ui_font)
                    .color(p.text_muted),
            )
            .padding(24)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
        );
    } else {
        let mut rows_col = column![];
        for (idx, entry) in filtered.into_iter().enumerate() {
            if idx > 0 {
                rows_col = rows_col.push(hairline_divider(mode));
            }
            let term_font = font_for_text(&entry.term, None, Weight::Bold);
            let tr_font = font_for_text(&entry.translation, None, Weight::Normal);
            let mut left = column![
                row![
                    text(&entry.term)
                        .size(14)
                        .font(term_font)
                        .color(p.highlight_text),
                    text("→").size(12).color(p.text_muted),
                    text(&entry.translation)
                        .size(13)
                        .font(tr_font)
                        .color(p.text_primary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(3)
            .width(Length::Fill);

            if !entry.notes.trim().is_empty() {
                left = left.push(
                    text(&entry.notes)
                        .size(12)
                        .font(font_for_text(&entry.notes, None, Weight::Normal))
                        .color(p.text_secondary),
                );
            }

            rows_col = rows_col.push(
                container(
                    row![
                        left,
                        button(text(i18n.t("vocab.delete")).size(11).font(ui_font_semi))
                            .on_press(DashboardMsg::DeleteVocab(entry.id.clone()))
                            .padding([5, 11])
                            .style(danger_button_style(mode)),
                    ]
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
    }

    scrollable(container(content).padding(Padding {
        top: 0.0,
        right: 4.0,
        bottom: 24.0,
        left: 0.0,
    }))
    .height(Length::Fill)
    .into()
}

fn view_history_tab<'a>(
    i18n: &'a Translator,
    history: &'a [HistoryEntry],
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let clear_btn: Option<Element<'a, DashboardMsg>> = if history.is_empty() {
        None
    } else {
        Some(
            button(
                text(i18n.t("history.clear_all"))
                    .size(12)
                    .font(ui_font_semi),
            )
            .on_press(DashboardMsg::ClearHistory)
            .padding([7, 13])
            .style(danger_button_style(mode))
            .into(),
        )
    };

    let header = page_header(
        i18n.t("history.title"),
        Some(i18n.t("hints.history_sub")),
        clear_btn,
        p,
        ui_font,
        ui_font_bold,
    );

    let mut content = column![header].spacing(18);

    if history.is_empty() {
        content = content.push(
            container(
                text(i18n.t("history.empty"))
                    .size(13)
                    .font(ui_font)
                    .color(p.text_muted),
            )
            .padding(24)
            .width(Length::Fill)
            .style(card_style(mode, 10.0)),
        );
    } else {
        let mut rows_col = column![];
        for (idx, h) in history.iter().take(60).enumerate() {
            if idx > 0 {
                rows_col = rows_col.push(hairline_divider(mode));
            }
            let mode_lbl = match h.mode {
                TranslationMode::Modal => i18n.t("history.mode_modal"),
                TranslationMode::InPlace => i18n.t("history.mode_inplace"),
                TranslationMode::Screenshot => i18n.t("history.mode_screenshot"),
                TranslationMode::Manual => i18n.t("history.mode_manual"),
            };
            let ts = h.created_at.format("%Y-%m-%d %H:%M").to_string();
            let item_clone = h.clone();
            let tr_copy = h.translated_text.clone();

            let top_meta = row![
                container(
                    text(mode_lbl)
                        .size(10)
                        .font(ui_font_semi)
                        .color(p.accent_text)
                )
                .padding([2, 7])
                .style(badge_style(
                    p.accent_soft,
                    p.accent_text,
                    Color::TRANSPARENT
                )),
                text(&h.model_name)
                    .size(11)
                    .font(font_for_text(&h.model_name, None, Weight::Semibold))
                    .color(p.text_secondary),
                Space::new().width(Length::Fill),
                text(ts).size(11).font(ui_font).color(p.text_muted),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            let body = row![
                column![
                    top_meta,
                    text(&h.source_text)
                        .size(12)
                        .font(font_for_text(&h.source_text, None, Weight::Normal))
                        .color(p.text_secondary),
                    text(&h.translated_text)
                        .size(13)
                        .font(font_for_text(&h.translated_text, None, Weight::Semibold))
                        .color(p.text_primary),
                ]
                .spacing(5)
                .width(Length::Fill),
                row![
                    button(text(i18n.t("history.copy")).size(11).font(ui_font_semi))
                        .on_press(DashboardMsg::CopyText(tr_copy))
                        .padding([5, 11])
                        .style(pill_button_style(mode)),
                    button(
                        text(i18n.t("history.save_vocab"))
                            .size(11)
                            .font(ui_font_semi)
                    )
                    .on_press(DashboardMsg::SaveHistoryToVocab(item_clone))
                    .padding([5, 11])
                    .style(pill_button_style(mode)),
                ]
                .spacing(6),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            rows_col = rows_col.push(container(body).padding([13, 16]).width(Length::Fill));
        }

        content = content.push(
            container(rows_col)
                .width(Length::Fill)
                .style(card_style(mode, 10.0)),
        );
    }

    scrollable(container(content).padding(Padding {
        top: 0.0,
        right: 4.0,
        bottom: 24.0,
        left: 0.0,
    }))
    .height(Length::Fill)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn view_settings_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    mode: UiThemeMode,
    p: AntigravityColors,
    ui_font: Font,
    ui_font_semi: Font,
    ui_font_bold: Font,
) -> Element<'a, DashboardMsg> {
    let header = page_header(
        i18n.t("settings.title"),
        Some(i18n.t("hints.settings_sub")),
        None,
        p,
        ui_font,
        ui_font_bold,
    );

    // Theme selectable cards (Antigravity Settings look!)
    let dark_selected = config.theme == UiThemeMode::Dark;
    let light_selected = config.theme == UiThemeMode::Light;

    let theme_cards = row![
        button(
            column![
                text(i18n.t("settings.theme_dark"))
                    .size(14)
                    .font(ui_font_bold)
                    .color(p.text_primary),
                text(i18n.t("hints.theme_dark_desc"))
                    .size(12)
                    .font(ui_font)
                    .color(p.text_secondary),
            ]
            .spacing(5),
        )
        .on_press(DashboardMsg::ThemeChanged(UiThemeMode::Dark))
        .padding([16, 18])
        .width(Length::FillPortion(1))
        .style(selectable_card_style(mode, dark_selected, 10.0)),
        button(
            column![
                text(i18n.t("settings.theme_light"))
                    .size(14)
                    .font(ui_font_bold)
                    .color(p.text_primary),
                text(i18n.t("hints.theme_light_desc"))
                    .size(12)
                    .font(ui_font)
                    .color(p.text_secondary),
            ]
            .spacing(5),
        )
        .on_press(DashboardMsg::ThemeChanged(UiThemeMode::Light))
        .padding([16, 18])
        .width(Length::FillPortion(1))
        .style(selectable_card_style(mode, light_selected, 10.0)),
    ]
    .spacing(12);

    // Language row
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
    let selected_ui_lang = ui_langs
        .iter()
        .find(|u| u.code == config.ui_language)
        .cloned();

    let appearance_group = container(antigravity_row(
        mode,
        i18n.t("settings.language_label"),
        Some(i18n.t("hints.language_desc")),
        pick_list(ui_langs, selected_ui_lang, |c| {
            DashboardMsg::UiLanguageChanged(c.code.to_string())
        })
        .font(ui_font)
        .text_size(12)
        .padding([7, 12])
        .width(Length::Fixed(185.0))
        .style(sleek_pick_list_style(mode))
        .menu_style(sleek_menu_style(mode))
        .into(),
        ui_font,
        ui_font_semi,
    ))
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    // Shortcuts grouped card
    let double_alt_row = container(
        row![
            column![
                row![
                    text(i18n.t("settings.enable_double_alt"))
                        .size(13)
                        .font(ui_font_semi)
                        .color(p.text_primary),
                    keycap(mode, "Alt", ui_font_semi),
                    text("+").size(11).color(p.text_muted),
                    keycap(mode, "Alt", ui_font_semi),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                text(i18n.t("settings.double_alt_desc"))
                    .size(12)
                    .font(ui_font)
                    .color(p.text_secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
            toggler(config.shortcuts.enable_double_alt)
                .on_toggle(DashboardMsg::ToggleDoubleAlt)
                .size(19)
                .style(antigravity_toggler_style(mode)),
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([14, 16]));

    let double_ctrl_row = container(
        row![
            column![
                row![
                    text(i18n.t("settings.enable_double_ctrl"))
                        .size(13)
                        .font(ui_font_semi)
                        .color(p.text_primary),
                    keycap(mode, "Ctrl", ui_font_semi),
                    text("+").size(11).color(p.text_muted),
                    keycap(mode, "Ctrl", ui_font_semi),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                text(i18n.t("settings.double_ctrl_desc"))
                    .size(12)
                    .font(ui_font)
                    .color(p.text_secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
            toggler(config.shortcuts.enable_double_ctrl)
                .on_toggle(DashboardMsg::ToggleDoubleCtrl)
                .size(19)
                .style(antigravity_toggler_style(mode)),
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([14, 16]));

    let shortcuts_group = container(column![
        double_alt_row,
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.auto_select_all_fallback"),
            Some(i18n.t("settings.auto_select_all_desc")),
            toggler(config.shortcuts.auto_select_all_fallback)
                .on_toggle(DashboardMsg::ToggleAutoSelectFallback)
                .size(19)
                .style(antigravity_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        double_ctrl_row,
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.double_tap_ms"),
            Some(i18n.t("hints.double_tap_ms_desc")),
            text_input("450", &state.double_tap_ms_draft)
                .on_input(DashboardMsg::DoubleTapMsChanged)
                .font(ui_font)
                .size(12)
                .padding([7, 10])
                .width(Length::Fixed(95.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.screenshot_hotkey"),
            None,
            text_input("Ctrl+Shift+S", &state.screenshot_hotkey_draft)
                .on_input(DashboardMsg::ScreenshotHotkeyChanged)
                .font(ui_font)
                .size(12)
                .padding([7, 10])
                .width(Length::Fixed(165.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.next_prompt_hotkey"),
            None,
            text_input("Ctrl+Alt+Right", &state.next_prompt_hotkey_draft)
                .on_input(DashboardMsg::NextPromptHotkeyChanged)
                .font(ui_font)
                .size(12)
                .padding([7, 10])
                .width(Length::Fixed(165.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.prev_prompt_hotkey"),
            None,
            text_input("Ctrl+Alt+Left", &state.prev_prompt_hotkey_draft)
                .on_input(DashboardMsg::PrevPromptHotkeyChanged)
                .font(ui_font)
                .size(12)
                .padding([7, 10])
                .width(Length::Fixed(165.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.dashboard_hotkey"),
            None,
            text_input("Ctrl+Alt+L", &state.dashboard_hotkey_draft)
                .on_input(DashboardMsg::DashboardHotkeyChanged)
                .font(ui_font)
                .size(12)
                .padding([7, 10])
                .width(Length::Fixed(165.0))
                .style(sleek_input_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    // Behavior + Updates grouped card
    let mut update_actions = row![
        button(
            text(if state.update_checking {
                i18n.t("updates.checking")
            } else {
                i18n.t("updates.check_btn")
            })
            .size(12)
            .font(ui_font_semi)
        )
        .on_press(DashboardMsg::CheckForUpdates)
        .padding([7, 14])
        .style(pill_button_style(mode)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut update_desc = format!(
        "{}: v{}",
        i18n.t("updates.current_version"),
        env!("CARGO_PKG_VERSION")
    );
    if let Some(res) = &state.update_result {
        if res.has_update {
            update_desc = format!(
                "{}  ·  v{} {}",
                update_desc,
                res.latest_version,
                i18n.t("updates.available")
            );
            update_actions = update_actions.push(
                button(
                    text(if state.update_downloading {
                        i18n.t("updates.downloading")
                    } else {
                        i18n.t("updates.download_install")
                    })
                    .size(12)
                    .font(ui_font_semi),
                )
                .on_press(DashboardMsg::DownloadAndInstallUpdate)
                .padding([7, 14])
                .style(primary_button_style(mode)),
            );
            update_actions = update_actions.push(
                button(
                    text(i18n.t("updates.open_release"))
                        .size(12)
                        .font(ui_font_semi),
                )
                .on_press(DashboardMsg::OpenReleaseUrl(res.release_url.clone()))
                .padding([7, 12])
                .style(pill_button_style(mode)),
            );
        } else {
            update_desc = format!("{}  ·  {}", update_desc, i18n.t("updates.up_to_date"));
        }
    } else if let Some(err) = &state.update_error {
        update_desc = format!("{update_desc}  ·  {err}");
    }

    let behavior_group = container(column![
        antigravity_row(
            mode,
            i18n.t("settings.restore_clipboard"),
            Some(i18n.t("hints.restore_clipboard_desc")),
            toggler(config.restore_clipboard)
                .on_toggle(DashboardMsg::ToggleRestoreClipboard)
                .size(19)
                .style(antigravity_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.launch_at_startup"),
            Some(i18n.t("hints.launch_startup_desc")),
            toggler(config.launch_at_startup)
                .on_toggle(DashboardMsg::ToggleLaunchStartup)
                .size(19)
                .style(antigravity_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("settings.show_hud"),
            Some(i18n.t("hints.show_hud_desc")),
            toggler(config.show_hud)
                .on_toggle(DashboardMsg::ToggleShowHud)
                .size(19)
                .style(antigravity_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("updates.auto_check"),
            Some(i18n.t("hints.auto_update_desc")),
            toggler(config.auto_check_updates)
                .on_toggle(DashboardMsg::ToggleAutoCheckUpdates)
                .size(19)
                .style(antigravity_toggler_style(mode))
                .into(),
            ui_font,
            ui_font_semi,
        ),
        hairline_divider(mode),
        antigravity_row(
            mode,
            i18n.t("updates.section_title"),
            Some(update_desc),
            update_actions.into(),
            ui_font,
            ui_font_semi,
        ),
    ])
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    let save_footer = row![
        if let Some(msg) = &state.settings_toast {
            text(msg.clone())
                .size(12)
                .font(ui_font_semi)
                .color(p.success)
                .width(Length::Fill)
        } else {
            text(i18n.t("settings.platform_notes_desc"))
                .size(12)
                .font(ui_font)
                .color(p.text_muted)
                .width(Length::Fill)
        },
        button(
            text(i18n.t("settings.save_settings"))
                .size(13)
                .font(ui_font_semi)
        )
        .on_press(DashboardMsg::SaveSettings)
        .padding([8, 18])
        .style(primary_button_style(mode)),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    let body = column![
        header,
        column![
            text(i18n.t("settings.theme_label"))
                .size(14)
                .font(ui_font_bold)
                .color(p.text_primary),
            theme_cards,
            appearance_group,
        ]
        .spacing(10),
        column![
            text(i18n.t("settings.shortcuts_section"))
                .size(14)
                .font(ui_font_bold)
                .color(p.text_primary),
            shortcuts_group,
        ]
        .spacing(10),
        column![
            text(i18n.t("settings.behavior_section"))
                .size(14)
                .font(ui_font_bold)
                .color(p.text_primary),
            behavior_group,
        ]
        .spacing(10),
        save_footer,
    ]
    .spacing(22);

    scrollable(container(body).padding(Padding {
        top: 0.0,
        right: 4.0,
        bottom: 24.0,
        left: 0.0,
    }))
    .height(Length::Fill)
    .into()
}
