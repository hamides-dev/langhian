use crate::ui::theme::{
    badge_style, card_style, danger_button_style, elevated_box_style, font_for_text, font_for_ui,
    ghost_button_style, palette_for, primary_button_style, render_highlighted_text, sidebar_style,
    sleek_input_style,
};
use iced::font::Weight;
use iced::widget::{
    Space, button, checkbox, column, container, pick_list, row, scrollable, text, text_editor,
    text_input, toggler,
};
use iced::{Alignment, Element, Length, alignment};
use langhian_core::{
    AppConfig, CapabilitySource, HistoryEntry, LocaleMeta, ModelProfile, PromptPreset,
    ProviderKind, TextDirection, TranslationMode, Translator, UpdateCheckResult, VocabEntry,
    VocabMatcher, supported_languages,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DashboardTab {
    #[default]
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
    pub temperature: String,
    pub max_tokens: String,
    pub supports_vision: bool,
    pub vision_source: CapabilitySource,
    pub fetched_models: Vec<String>,
    pub probe_status: Option<(bool, String)>,
    pub is_probing: bool,
}

impl ModelEditorState {
    pub fn from_profile(profile: &ModelProfile, api_key: Option<String>, is_new: bool) -> Self {
        let headers_raw = profile
            .extra_headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
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
            temperature: format!("{:.1}", profile.temperature),
            max_tokens: profile
                .max_tokens
                .map(|v| v.to_string())
                .unwrap_or_default(),
            supports_vision: profile.supports_vision,
            vision_source: profile.vision_source,
            fetched_models: Vec::new(),
            probe_status: None,
            is_probing: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptEditorState {
    pub editing_id: Option<String>,
    pub name: String,
    pub shortcut_slot: Option<u8>,
    pub system_prompt: String,
    pub user_template: String,
    pub vision_instruction: String,
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
            shortcut_slot: preset.shortcut_slot,
            system_prompt: preset.system_prompt.clone(),
            user_template: preset.user_template.clone(),
            vision_instruction: preset.vision_instruction.clone(),
        }
    }
}

#[derive(Debug)]
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
    pub new_vocab_term: String,
    pub new_vocab_translation: String,
    pub new_vocab_notes: String,
    pub show_add_vocab_form: bool,
    pub export_status: Option<String>,
    // Settings shortcuts draft
    pub double_tap_ms_draft: String,
    pub screenshot_hotkey_draft: String,
    pub next_prompt_hotkey_draft: String,
    pub prev_prompt_hotkey_draft: String,
    pub dashboard_hotkey_draft: String,
    pub settings_toast: Option<String>,
    // Updates
    pub update_checking: bool,
    pub update_downloading: bool,
    pub update_result: Option<UpdateCheckResult>,
    pub update_error: Option<String>,
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
            new_vocab_term: String::new(),
            new_vocab_translation: String::new(),
            new_vocab_notes: String::new(),
            show_add_vocab_form: false,
            export_status: None,
            double_tap_ms_draft: config.shortcuts.double_tap_ms.to_string(),
            screenshot_hotkey_draft: config.shortcuts.screenshot.format_display(),
            next_prompt_hotkey_draft: config.shortcuts.next_prompt.format_display(),
            prev_prompt_hotkey_draft: config.shortcuts.prev_prompt.format_display(),
            dashboard_hotkey_draft: config.shortcuts.open_dashboard.format_display(),
            settings_toast: None,
            update_checking: false,
            update_downloading: false,
            update_result: None,
            update_error: None,
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
    ModelHeadersChanged(String),
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
    // Settings & Updates
    UiLanguageChanged(String),
    ThemeChanged(langhian_core::UiThemeMode),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotOption(pub Option<u8>);

impl std::fmt::Display for SlotOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(n) => write!(f, "Ctrl+Alt+{n}"),
            None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PickItem {
    id: String,
    label: String,
}

impl std::fmt::Display for PickItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderChoice(ProviderKind);

impl std::fmt::Display for ProviderChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.0 {
            ProviderKind::GoogleTranslateFree => "Google Translate (Free)",
            ProviderKind::OpenAiCompatible => {
                "OpenAI-Compatible (OpenRouter/Ollama/LM Studio/Custom)"
            }
            ProviderKind::Anthropic => "Anthropic Claude",
            ProviderKind::Gemini => "Google Gemini",
        };
        write!(f, "{name}")
    }
}

pub fn view_dashboard<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    vocab: &'a [VocabEntry],
    history: &'a [HistoryEntry],
    i18n: &'a Translator,
    matcher: &VocabMatcher,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_dir = i18n.direction();
    let ui_font = font_for_ui(ui_dir, Weight::Normal);
    let ui_bold = font_for_ui(ui_dir, Weight::Semibold);
    let align_x = if ui_dir.is_rtl() {
        alignment::Horizontal::Right
    } else {
        alignment::Horizontal::Left
    };

    let nav_items = [
        (DashboardTab::Translate, i18n.t("nav.translate")),
        (DashboardTab::Models, i18n.t("nav.models")),
        (DashboardTab::Prompts, i18n.t("nav.prompts")),
        (DashboardTab::Vocabulary, i18n.t("nav.vocabulary")),
        (DashboardTab::History, i18n.t("nav.history")),
        (DashboardTab::Settings, i18n.t("nav.settings")),
    ];

    let mut nav_col = column![
        row![
            container(text(""))
                .width(12)
                .height(12)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(p.accent)),
                    border: iced::Border {
                        radius: 99.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            column![
                text("Langhian")
                    .size(16)
                    .font(font_for_ui(TextDirection::Ltr, Weight::Bold))
                    .color(p.text_primary),
                text(i18n.t("app.subtitle"))
                    .size(11)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .color(p.text_muted),
            ]
            .spacing(1)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        Space::new().height(12),
    ]
    .spacing(6);

    for (tab, label) in nav_items {
        let active = state.active_tab == tab;
        nav_col = nav_col.push(
            button(
                text(label)
                    .size(13)
                    .font(if active { ui_bold } else { ui_font })
                    .shaping(text::Shaping::Advanced)
                    .width(Length::Fill)
                    .align_x(align_x),
            )
            .width(Length::Fill)
            .padding([10, 12])
            .style(ghost_button_style(mode, active))
            .on_press(DashboardMsg::SwitchTab(tab)),
        );
    }

    let active_model = config.active_model();
    let active_prompt = config.active_prompt();

    let status_footer = container(
        column![
            text(active_model.name.clone())
                .size(11)
                .font(font_for_text(&active_model.name, None, Weight::Medium))
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary),
            text(active_prompt.name.clone())
                .size(11)
                .font(font_for_text(&active_prompt.name, None, Weight::Normal))
                .shaping(text::Shaping::Advanced)
                .color(p.text_secondary),
            text(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(10)
                .color(p.text_muted),
        ]
        .spacing(4),
    )
    .width(Length::Fill)
    .padding(10)
    .style(elevated_box_style(mode, 10.0));

    nav_col = nav_col
        .push(Space::new().height(Length::Fill))
        .push(status_footer);

    let sidebar = container(nav_col)
        .width(Length::Fixed(225.0))
        .height(Length::Fill)
        .padding(16)
        .style(sidebar_style(mode));

    let body: Element<'a, DashboardMsg> = match state.active_tab {
        DashboardTab::Translate => view_translate_tab(state, config, i18n, matcher),
        DashboardTab::Models => view_models_tab(state, config, i18n),
        DashboardTab::Prompts => view_prompts_tab(state, config, i18n),
        DashboardTab::Vocabulary => view_vocab_tab(state, config, vocab, i18n),
        DashboardTab::History => view_history_tab(config, history, i18n),
        DashboardTab::Settings => view_settings_tab(state, config, i18n),
    };

    let main_area = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(22);

    if ui_dir.is_rtl() {
        row![main_area, sidebar].into()
    } else {
        row![sidebar, main_area].into()
    }
}

fn view_translate_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    matcher: &VocabMatcher,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_dir = i18n.direction();
    let ui_font = font_for_ui(ui_dir, Weight::Normal);
    let ui_bold = font_for_ui(ui_dir, Weight::Semibold);

    let mut source_langs = vec![PickItem {
        id: "auto".to_string(),
        label: i18n.t("translate.auto_detect"),
    }];
    for l in supported_languages() {
        source_langs.push(PickItem {
            id: l.code.to_string(),
            label: l.to_string(),
        });
    }
    let target_langs: Vec<PickItem> = supported_languages()
        .iter()
        .map(|l| PickItem {
            id: l.code.to_string(),
            label: l.to_string(),
        })
        .collect();

    let sel_source = source_langs
        .iter()
        .find(|x| x.id.eq_ignore_ascii_case(&config.source_language))
        .cloned();
    let sel_target = target_langs
        .iter()
        .find(|x| x.id.eq_ignore_ascii_case(&config.target_language))
        .cloned();

    let model_items: Vec<PickItem> = config
        .models
        .iter()
        .map(|m| PickItem {
            id: m.id.clone(),
            label: if m.supports_vision {
                format!("{} [Vision]", m.name)
            } else {
                m.name.clone()
            },
        })
        .collect();
    let sel_model = model_items
        .iter()
        .find(|x| x.id == config.active_model_id)
        .cloned();

    let prompt_items: Vec<PickItem> = config
        .prompts
        .iter()
        .map(|pr| PickItem {
            id: pr.id.clone(),
            label: pr.name.clone(),
        })
        .collect();
    let sel_prompt = prompt_items
        .iter()
        .find(|x| x.id == config.active_prompt_id)
        .cloned();

    let toolbar = row![
        pick_list(
            source_langs,
            sel_source,
            |x| DashboardMsg::SelectSourceLang(x.id)
        )
        .text_size(12)
        .padding([6, 10]),
        button(
            text(i18n.t("translate.swap_langs"))
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([6, 10])
        .style(ghost_button_style(mode, false))
        .on_press(DashboardMsg::SwapLangs),
        pick_list(
            target_langs,
            sel_target,
            |x| DashboardMsg::SelectTargetLang(x.id)
        )
        .text_size(12)
        .padding([6, 10]),
        Space::new().width(Length::Fill),
        pick_list(model_items, sel_model, |x| DashboardMsg::SelectModel(x.id))
            .text_size(12)
            .padding([6, 10]),
        pick_list(prompt_items, sel_prompt, |x| DashboardMsg::SelectPrompt(
            x.id
        ))
        .text_size(12)
        .padding([6, 10]),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let source_raw = state.source_editor.text();
    let source_segs = matcher.highlight_segments(&source_raw);
    let has_source_matches = source_segs
        .iter()
        .any(|s| matches!(s, langhian_core::TextSegment::Saved { .. }));

    let mut left_col = column![
        text(i18n.t("translate.source_label"))
            .size(12)
            .font(ui_bold)
            .shaping(text::Shaping::Advanced)
            .color(p.text_secondary),
        text_editor(&state.source_editor)
            .placeholder(i18n.t("translate.source_placeholder"))
            .on_action(DashboardMsg::SourceEditorAction)
            .font(font_for_text(&source_raw, None, Weight::Normal))
            .height(Length::Fill)
            .padding(12),
    ]
    .spacing(8)
    .width(Length::Fill)
    .height(Length::Fill);

    if has_source_matches {
        left_col = left_col.push(
            container(render_highlighted_text(
                source_segs,
                &source_raw,
                None,
                mode,
                12.5,
                DashboardMsg::ClickVocabHighlight,
            ))
            .width(Length::Fill)
            .padding(10)
            .style(elevated_box_style(mode, 8.0)),
        );
    }

    let left_card = container(left_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(14)
        .style(card_style(mode, 12.0));

    let out_elem: Element<'a, DashboardMsg> = if state.is_translating {
        text(i18n.t("app.status_translating"))
            .size(14)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .color(p.accent_text)
            .into()
    } else if let Some(err) = &state.translate_error {
        text(err.clone())
            .size(13)
            .font(font_for_text(err, None, Weight::Normal))
            .shaping(text::Shaping::Advanced)
            .color(p.danger)
            .into()
    } else if state.manual_output.is_empty() {
        text(i18n.t("translate.result_placeholder"))
            .size(14)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .color(p.text_muted)
            .into()
    } else {
        let out_segs = matcher.highlight_segments(&state.manual_output);
        render_highlighted_text(
            out_segs,
            &state.manual_output,
            Some(&config.target_language),
            mode,
            15.0,
            DashboardMsg::ClickVocabHighlight,
        )
    };

    let right_card = container(
        column![
            text(i18n.t("translate.target_label"))
                .size(12)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
                .color(p.text_secondary),
            container(scrollable(out_elem).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(12)
                .style(elevated_box_style(mode, 10.0)),
        ]
        .spacing(8)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(14)
    .style(card_style(mode, 12.0));

    let panes = row![left_card, right_card].spacing(14).height(Length::Fill);

    let mut col = column![toolbar, panes].spacing(14).height(Length::Fill);

    if let Some(entry) = &state.selected_vocab_preview {
        col = col.push(
            container(
                row![
                    text(format!(
                        "{}  →  {}  {}",
                        entry.term, entry.translation, entry.notes
                    ))
                    .size(13)
                    .font(font_for_text(&entry.translation, None, Weight::Medium))
                    .shaping(text::Shaping::Advanced)
                    .color(p.warning)
                    .width(Length::Fill),
                    button(text("×").size(13))
                        .padding([2, 8])
                        .style(ghost_button_style(mode, false))
                        .on_press(DashboardMsg::DismissVocabPreview),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(badge_style(p.warning_soft, p.text_primary, p.warning)),
        );
    }

    if state.show_save_vocab_box {
        col = col.push(
            container(
                row![
                    text_input(&i18n.t("translate.word_label"), &state.save_term)
                        .on_input(DashboardMsg::SaveManualTermChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                    text_input(&i18n.t("translate.meaning_label"), &state.save_translation)
                        .on_input(DashboardMsg::SaveManualTranslationChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                    text_input(&i18n.t("translate.notes_label"), &state.save_notes)
                        .on_input(DashboardMsg::SaveManualNotesChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                    button(
                        text(i18n.t("vocab.save"))
                            .size(12)
                            .font(ui_bold)
                            .shaping(text::Shaping::Advanced)
                    )
                    .padding([8, 14])
                    .style(primary_button_style(mode))
                    .on_press(DashboardMsg::ConfirmSaveManualVocab),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(10)
            .style(card_style(mode, 10.0)),
        );
    }

    let action_bar = row![
        button(
            text(i18n.t("translate.translate_btn"))
                .size(13)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
        )
        .padding([9, 18])
        .style(primary_button_style(mode))
        .on_press(DashboardMsg::RunManualTranslate),
        button(
            text(i18n.t("translate.screenshot_btn"))
                .size(13)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([9, 14])
        .style(ghost_button_style(mode, false))
        .on_press(DashboardMsg::TriggerScreenshot),
        button(
            text(if state.copied_manual {
                i18n.t("translate.copied")
            } else {
                i18n.t("translate.copy_btn")
            })
            .size(13)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
        )
        .padding([9, 14])
        .style(ghost_button_style(mode, false))
        .on_press(DashboardMsg::CopyManualResult),
        button(
            text(i18n.t("translate.save_btn"))
                .size(13)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([9, 14])
        .style(ghost_button_style(mode, state.show_save_vocab_box))
        .on_press(DashboardMsg::ToggleSaveManualBox),
        Space::new().width(Length::Fill),
        button(
            text(i18n.t("translate.clear_btn"))
                .size(13)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([9, 14])
        .style(ghost_button_style(mode, false))
        .on_press(DashboardMsg::ClearManual),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    col.push(action_bar).into()
}

fn view_models_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_bold = font_for_ui(i18n.direction(), Weight::Semibold);

    let header = text(i18n.t("models.title"))
        .size(20)
        .font(ui_bold)
        .shaping(text::Shaping::Advanced)
        .color(p.text_primary);

    let preset_row = row![
        preset_btn(
            i18n.t("models.preset_openrouter"),
            ModelPresetKind::OpenRouter,
            mode
        ),
        preset_btn(
            i18n.t("models.preset_openai"),
            ModelPresetKind::OpenAi,
            mode
        ),
        preset_btn("Claude", ModelPresetKind::Anthropic, mode),
        preset_btn("Gemini", ModelPresetKind::Gemini, mode),
        preset_btn(
            i18n.t("models.preset_ollama"),
            ModelPresetKind::Ollama,
            mode
        ),
        preset_btn(
            i18n.t("models.preset_lmstudio"),
            ModelPresetKind::LmStudio,
            mode
        ),
        preset_btn(
            i18n.t("models.preset_custom"),
            ModelPresetKind::Custom,
            mode
        ),
        preset_btn("Google Free", ModelPresetKind::GoogleFree, mode),
    ]
    .spacing(6);

    let mut list_col = column![].spacing(10);
    for m in &config.models {
        let is_active = m.id == config.active_model_id;
        let mid_set = m.id.clone();
        let mid_edit = m.id.clone();
        let mid_dup = m.id.clone();
        let mid_del = m.id.clone();

        let vision_badge = container(
            text(if m.supports_vision {
                i18n.t("models.vision_badge")
            } else {
                i18n.t("models.text_only_badge")
            })
            .size(11)
            .font(ui_font)
            .shaping(text::Shaping::Advanced),
        )
        .padding([3, 8])
        .style(if m.supports_vision {
            badge_style(p.accent_soft, p.accent_text, p.border_strong)
        } else {
            badge_style(p.bg_elevated, p.text_muted, p.border_subtle)
        });

        let mut card_row = row![
            column![
                row![
                    text(m.name.clone())
                        .size(14)
                        .font(font_for_text(&m.name, None, Weight::Semibold))
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_primary),
                    vision_badge,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(format!("{}  ·  {}", m.model_id, m.base_url))
                    .size(12)
                    .color(p.text_secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if is_active {
            card_row = card_row.push(
                container(
                    text(i18n.t("models.active_badge"))
                        .size(11)
                        .font(ui_bold)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([5, 10])
                .style(badge_style(p.accent_soft, p.accent_text, p.border_strong)),
            );
        } else {
            card_row = card_row.push(
                button(
                    text(i18n.t("models.set_active"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([6, 12])
                .style(primary_button_style(mode))
                .on_press(DashboardMsg::SelectModel(mid_set)),
            );
        }

        card_row = card_row
            .push(
                button(
                    text(i18n.t("models.edit_profile"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([6, 10])
                .style(ghost_button_style(mode, false))
                .on_press(DashboardMsg::StartEditModel(mid_edit)),
            )
            .push(
                button(
                    text(i18n.t("models.clone_profile"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([6, 10])
                .style(ghost_button_style(mode, false))
                .on_press(DashboardMsg::DuplicateModel(mid_dup)),
            );

        if config.models.len() > 1 {
            card_row = card_row.push(
                button(
                    text(i18n.t("models.delete_profile"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([6, 10])
                .style(danger_button_style(mode))
                .on_press(DashboardMsg::DeleteModel(mid_del)),
            );
        }

        list_col = list_col.push(
            container(card_row)
                .width(Length::Fill)
                .padding(14)
                .style(card_style(mode, 12.0)),
        );
    }

    let mut content = column![
        header,
        scrollable(preset_row).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(3).scroller_width(3),
        )),
        list_col,
    ]
    .spacing(14);

    if let Some(ed) = &state.model_editor {
        let providers: Vec<ProviderChoice> = ProviderKind::all()
            .iter()
            .copied()
            .map(ProviderChoice)
            .collect();
        let selected_prov = Some(ProviderChoice(ed.provider));

        let mut editor_col = column![
            text(if ed.editing_id.is_some() {
                i18n.t("models.edit_profile")
            } else {
                i18n.t("models.add_profile")
            })
            .size(16)
            .font(ui_bold)
            .shaping(text::Shaping::Advanced)
            .color(p.text_primary),
            row![
                column![
                    text(i18n.t("models.name_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    text_input("My Model", &ed.name)
                        .on_input(DashboardMsg::ModelNameChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                ]
                .spacing(4)
                .width(Length::Fill),
                column![
                    text(i18n.t("models.provider_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    pick_list(providers, selected_prov, |c| {
                        DashboardMsg::ModelProviderChanged(c.0)
                    })
                    .text_size(12)
                    .padding([8, 10])
                    .width(Length::Fill),
                ]
                .spacing(4)
                .width(Length::Fill),
            ]
            .spacing(12),
        ]
        .spacing(12);

        if ed.provider != ProviderKind::GoogleTranslateFree {
            editor_col = editor_col.push(
                row![
                    column![
                        text(i18n.t("models.base_url_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text_input("https://openrouter.ai/api/v1", &ed.base_url)
                            .on_input(DashboardMsg::ModelBaseUrlChanged)
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    column![
                        text(i18n.t("models.model_id_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text_input("gpt-4o-mini / qwen2.5:7b", &ed.model_id)
                            .on_input(DashboardMsg::ModelIdChanged)
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                ]
                .spacing(12),
            );

            if !ed.fetched_models.is_empty() {
                let selected_fetched = ed
                    .fetched_models
                    .iter()
                    .find(|x| *x == &ed.model_id)
                    .cloned();
                editor_col = editor_col.push(
                    pick_list(
                        ed.fetched_models.as_slice(),
                        selected_fetched,
                        DashboardMsg::PickFetchedModel,
                    )
                    .placeholder("Select from fetched models...")
                    .text_size(12)
                    .padding([6, 10])
                    .width(Length::Fill),
                );
            }

            editor_col = editor_col.push(
                column![
                    text(i18n.t("models.api_key_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    text_input(&i18n.t("models.api_key_placeholder"), &ed.api_key)
                        .secure(true)
                        .on_input(DashboardMsg::ModelApiKeyChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                ]
                .spacing(4),
            );

            editor_col = editor_col.push(
                row![
                    column![
                        text(i18n.t("models.temperature_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text_input("0.2", &ed.temperature)
                            .on_input(DashboardMsg::ModelTempChanged)
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(4)
                    .width(Length::Fixed(140.0)),
                    column![
                        text(i18n.t("models.max_tokens_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text_input("2048", &ed.max_tokens)
                            .on_input(DashboardMsg::ModelMaxTokensChanged)
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(4)
                    .width(Length::Fixed(180.0)),
                    column![
                        text(i18n.t("models.custom_headers_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text_input("X-Custom: value", &ed.headers_raw)
                            .on_input(DashboardMsg::ModelHeadersChanged)
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                ]
                .spacing(12),
            );

            editor_col = editor_col.push(
                row![
                    toggler(ed.supports_vision)
                        .label(i18n.t("models.supports_vision_label"))
                        .on_toggle(DashboardMsg::ModelVisionToggled)
                        .text_size(13),
                ]
                .align_y(Alignment::Center),
            );
        }

        if let Some((ok, msg)) = &ed.probe_status {
            editor_col = editor_col.push(
                container(
                    text(msg.clone())
                        .size(12)
                        .color(if *ok { p.success } else { p.danger }),
                )
                .width(Length::Fill)
                .padding([8, 12])
                .style(if *ok {
                    badge_style(p.accent_soft, p.success, p.success)
                } else {
                    badge_style(p.danger_soft, p.danger, p.danger)
                }),
            );
        }

        editor_col = editor_col.push(
            row![
                button(
                    text(if ed.is_probing {
                        i18n.t("models.testing")
                    } else {
                        i18n.t("models.test_btn")
                    })
                    .size(12)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                )
                .padding([8, 12])
                .style(ghost_button_style(mode, false))
                .on_press(DashboardMsg::TestModelConnection),
                button(
                    text(i18n.t("models.fetch_models_btn"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                )
                .padding([8, 12])
                .style(ghost_button_style(mode, false))
                .on_press(DashboardMsg::FetchProviderModels),
                Space::new().width(Length::Fill),
                button(
                    text(i18n.t("models.cancel"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                )
                .padding([8, 14])
                .style(ghost_button_style(mode, false))
                .on_press(DashboardMsg::CancelEditModel),
                button(
                    text(i18n.t("models.save_profile"))
                        .size(12)
                        .font(ui_bold)
                        .shaping(text::Shaping::Advanced)
                )
                .padding([8, 16])
                .style(primary_button_style(mode))
                .on_press(DashboardMsg::SaveModelProfile),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        content = content.push(
            container(editor_col)
                .width(Length::Fill)
                .padding(16)
                .style(card_style(mode, 12.0)),
        );
    }

    scrollable(content).height(Length::Fill).into()
}

fn preset_btn<'a>(
    label: impl Into<String>,
    kind: ModelPresetKind,
    mode: langhian_core::UiThemeMode,
) -> Element<'a, DashboardMsg> {
    let s = label.into();
    button(
        text(format!("+ {s}"))
            .size(12)
            .font(font_for_text(&s, None, Weight::Medium))
            .shaping(text::Shaping::Advanced),
    )
    .padding([6, 12])
    .style(ghost_button_style(mode, false))
    .on_press(DashboardMsg::StartAddPreset(kind))
    .into()
}

fn view_prompts_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_bold = font_for_ui(i18n.direction(), Weight::Semibold);

    let header = row![
        text(i18n.t("prompts.title"))
            .size(20)
            .font(ui_bold)
            .shaping(text::Shaping::Advanced)
            .color(p.text_primary)
            .width(Length::Fill),
        button(
            text(format!("+ {}", i18n.t("prompts.add_prompt")))
                .size(12)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
        )
        .padding([8, 14])
        .style(primary_button_style(mode))
        .on_press(DashboardMsg::StartNewPrompt),
    ]
    .align_y(Alignment::Center);

    let mut list_col = column![].spacing(10);
    for pr in &config.prompts {
        let is_active = pr.id == config.active_prompt_id;
        let pid_act = pr.id.clone();
        let pid_edit = pr.id.clone();
        let pid_del = pr.id.clone();
        let slot_txt = pr
            .shortcut_slot
            .map(|n| format!("Ctrl+Alt+{n}"))
            .unwrap_or_else(|| i18n.t("prompts.slot_none"));

        let mut card_row = row![
            column![
                row![
                    text(pr.name.clone())
                        .size(14)
                        .font(font_for_text(&pr.name, None, Weight::Semibold))
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_primary),
                    container(text(slot_txt).size(11).color(p.accent_text))
                        .padding([2, 8])
                        .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(pr.system_prompt.clone())
                    .size(12)
                    .font(font_for_text(&pr.system_prompt, None, Weight::Normal))
                    .shaping(text::Shaping::Advanced)
                    .color(p.text_secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if is_active {
            card_row = card_row.push(
                container(
                    text(i18n.t("models.active_badge"))
                        .size(11)
                        .font(ui_bold)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([5, 10])
                .style(badge_style(p.accent_soft, p.accent_text, p.border_strong)),
            );
        } else {
            card_row = card_row.push(
                button(
                    text(i18n.t("prompts.set_active"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([6, 12])
                .style(primary_button_style(mode))
                .on_press(DashboardMsg::SelectPrompt(pid_act)),
            );
        }

        card_row = card_row.push(
            button(
                text(i18n.t("prompts.edit_prompt"))
                    .size(12)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced),
            )
            .padding([6, 10])
            .style(ghost_button_style(mode, false))
            .on_press(DashboardMsg::StartEditPrompt(pid_edit)),
        );

        if config.prompts.len() > 1 {
            card_row = card_row.push(
                button(
                    text(i18n.t("prompts.delete_prompt"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                )
                .padding([6, 10])
                .style(danger_button_style(mode))
                .on_press(DashboardMsg::DeletePrompt(pid_del)),
            );
        }

        list_col = list_col.push(
            container(card_row)
                .width(Length::Fill)
                .padding(14)
                .style(card_style(mode, 12.0)),
        );
    }

    let mut content = column![header, list_col].spacing(14);

    if let Some(ed) = &state.prompt_editor {
        let mut slots = vec![SlotOption(None)];
        for n in 1..=9 {
            slots.push(SlotOption(Some(n)));
        }
        let sel_slot = Some(SlotOption(ed.shortcut_slot));

        let ed_card = container(
            column![
                row![
                    column![
                        text(i18n.t("prompts.name_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text_input("Custom Prompt", &ed.name)
                            .on_input(DashboardMsg::PromptNameChanged)
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    column![
                        text(i18n.t("prompts.slot_label"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        pick_list(slots, sel_slot, DashboardMsg::PromptSlotChanged)
                            .text_size(12)
                            .padding([8, 10]),
                    ]
                    .spacing(4),
                ]
                .spacing(12),
                column![
                    text(i18n.t("prompts.system_prompt_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    text_input("System prompt...", &ed.system_prompt)
                        .on_input(DashboardMsg::PromptSystemChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                ]
                .spacing(4),
                column![
                    text(i18n.t("prompts.user_template_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    text_input(
                        "Translate from {source_lang} to {target_lang}: {text}",
                        &ed.user_template
                    )
                    .on_input(DashboardMsg::PromptUserTemplateChanged)
                    .style(sleek_input_style(mode))
                    .padding(8)
                    .size(13),
                    text(i18n.t("prompts.template_vars_hint"))
                        .size(11)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_muted),
                ]
                .spacing(4),
                column![
                    text(i18n.t("prompts.vision_instruction_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    text_input(
                        "Read and translate screenshot into {target_lang}",
                        &ed.vision_instruction
                    )
                    .on_input(DashboardMsg::PromptVisionChanged)
                    .style(sleek_input_style(mode))
                    .padding(8)
                    .size(13),
                ]
                .spacing(4),
                row![
                    Space::new().width(Length::Fill),
                    button(
                        text(i18n.t("prompts.cancel"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                    )
                    .padding([8, 14])
                    .style(ghost_button_style(mode, false))
                    .on_press(DashboardMsg::CancelEditPrompt),
                    button(
                        text(i18n.t("prompts.save_prompt"))
                            .size(12)
                            .font(ui_bold)
                            .shaping(text::Shaping::Advanced)
                    )
                    .padding([8, 16])
                    .style(primary_button_style(mode))
                    .on_press(DashboardMsg::SavePromptPreset),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .width(Length::Fill)
        .padding(16)
        .style(card_style(mode, 12.0));

        content = content.push(ed_card);
    }

    scrollable(content).height(Length::Fill).into()
}

fn view_vocab_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    vocab: &'a [VocabEntry],
    i18n: &'a Translator,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_bold = font_for_ui(i18n.direction(), Weight::Semibold);

    let header = row![
        column![
            text(i18n.t("vocab.title"))
                .size(20)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary),
            text(format!("{}: {}", i18n.t("vocab.count_label"), vocab.len()))
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .color(p.text_secondary),
        ]
        .spacing(2)
        .width(Length::Fill),
        button(
            text(i18n.t("vocab.export_json"))
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([7, 12])
        .style(ghost_button_style(mode, false))
        .on_press(DashboardMsg::ExportVocabJson),
        button(
            text(i18n.t("vocab.export_csv"))
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([7, 12])
        .style(ghost_button_style(mode, false))
        .on_press(DashboardMsg::ExportVocabCsv),
        button(
            text(format!("+ {}", i18n.t("vocab.add_manual")))
                .size(12)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
        )
        .padding([7, 14])
        .style(primary_button_style(mode))
        .on_press(DashboardMsg::ToggleAddVocabForm),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let search = text_input(&i18n.t("vocab.search_placeholder"), &state.vocab_search)
        .on_input(DashboardMsg::VocabSearchChanged)
        .style(sleek_input_style(mode))
        .padding(10)
        .size(13);

    let mut content = column![header, search].spacing(12);

    if let Some(status) = &state.export_status {
        content = content.push(text(status.clone()).size(12).color(p.success));
    }

    if state.show_add_vocab_form {
        content = content.push(
            container(
                row![
                    text_input(&i18n.t("vocab.term_label"), &state.new_vocab_term)
                        .on_input(DashboardMsg::NewVocabTermChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                    text_input(
                        &i18n.t("vocab.translation_label"),
                        &state.new_vocab_translation
                    )
                    .on_input(DashboardMsg::NewVocabTranslationChanged)
                    .style(sleek_input_style(mode))
                    .padding(8)
                    .size(13),
                    text_input(&i18n.t("vocab.notes_label"), &state.new_vocab_notes)
                        .on_input(DashboardMsg::NewVocabNotesChanged)
                        .style(sleek_input_style(mode))
                        .padding(8)
                        .size(13),
                    button(
                        text(i18n.t("vocab.save"))
                            .size(12)
                            .font(ui_bold)
                            .shaping(text::Shaping::Advanced)
                    )
                    .padding([8, 14])
                    .style(primary_button_style(mode))
                    .on_press(DashboardMsg::ConfirmAddVocab),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(12)
            .style(card_style(mode, 10.0)),
        );
    }

    let q = state.vocab_search.trim().to_ascii_lowercase();
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
            container(
                text(i18n.t("vocab.empty"))
                    .size(13)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .color(p.text_secondary),
            )
            .width(Length::Fill)
            .padding(24)
            .style(card_style(mode, 12.0)),
        );
    } else {
        let mut list = column![].spacing(8);
        for v in filtered {
            let vid = v.id.clone();
            let tr_copy = v.translation.clone();
            list = list.push(
                container(
                    row![
                        column![
                            text(v.term.clone())
                                .size(14)
                                .font(font_for_text(&v.term, None, Weight::Semibold))
                                .shaping(text::Shaping::Advanced)
                                .color(p.warning),
                            text(v.translation.clone())
                                .size(13)
                                .font(font_for_text(&v.translation, None, Weight::Normal))
                                .shaping(text::Shaping::Advanced)
                                .color(p.text_primary),
                            if v.notes.is_empty() {
                                Element::from(Space::new().height(0))
                            } else {
                                Element::from(
                                    text(v.notes.clone())
                                        .size(11)
                                        .font(font_for_text(&v.notes, None, Weight::Normal))
                                        .shaping(text::Shaping::Advanced)
                                        .color(p.text_muted),
                                )
                            }
                        ]
                        .spacing(3)
                        .width(Length::Fill),
                        button(
                            text(i18n.t("history.copy"))
                                .size(12)
                                .font(ui_font)
                                .shaping(text::Shaping::Advanced)
                        )
                        .padding([6, 10])
                        .style(ghost_button_style(mode, false))
                        .on_press(DashboardMsg::CopyText(tr_copy)),
                        button(
                            text(i18n.t("vocab.delete"))
                                .size(12)
                                .font(ui_font)
                                .shaping(text::Shaping::Advanced)
                        )
                        .padding([6, 10])
                        .style(danger_button_style(mode))
                        .on_press(DashboardMsg::DeleteVocab(vid)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding(12)
                .style(card_style(mode, 10.0)),
            );
        }
        content = content.push(list);
    }

    scrollable(content).height(Length::Fill).into()
}

fn view_history_tab<'a>(
    config: &'a AppConfig,
    history: &'a [HistoryEntry],
    i18n: &'a Translator,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_bold = font_for_ui(i18n.direction(), Weight::Semibold);

    let header = row![
        text(i18n.t("history.title"))
            .size(20)
            .font(ui_bold)
            .shaping(text::Shaping::Advanced)
            .color(p.text_primary)
            .width(Length::Fill),
        button(
            text(i18n.t("history.clear_all"))
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([7, 12])
        .style(danger_button_style(mode))
        .on_press(DashboardMsg::ClearHistory),
    ]
    .align_y(Alignment::Center);

    let mut content = column![header].spacing(12);
    if history.is_empty() {
        content = content.push(
            container(
                text(i18n.t("history.empty"))
                    .size(13)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .color(p.text_secondary),
            )
            .width(Length::Fill)
            .padding(24)
            .style(card_style(mode, 12.0)),
        );
    } else {
        let mut list = column![].spacing(10);
        for item in history.iter().take(80) {
            let mode_label = match item.mode {
                TranslationMode::Modal => i18n.t("history.mode_modal"),
                TranslationMode::InPlace => i18n.t("history.mode_inplace"),
                TranslationMode::Screenshot => i18n.t("history.mode_screenshot"),
                TranslationMode::Manual => i18n.t("history.mode_manual"),
            };
            let tr_copy = item.translated_text.clone();
            let entry_clone = item.clone();

            list = list.push(
                container(
                    column![
                        row![
                            container(
                                text(mode_label)
                                    .size(10)
                                    .font(ui_bold)
                                    .shaping(text::Shaping::Advanced)
                            )
                            .padding([2, 8])
                            .style(badge_style(
                                p.accent_soft,
                                p.accent_text,
                                p.border_subtle
                            )),
                            text(format!(
                                "{} · {} · {}",
                                item.model_name, item.prompt_name, item.target_lang
                            ))
                            .size(11)
                            .color(p.text_muted)
                            .width(Length::Fill),
                            button(
                                text(i18n.t("history.copy"))
                                    .size(11)
                                    .font(ui_font)
                                    .shaping(text::Shaping::Advanced)
                            )
                            .padding([4, 10])
                            .style(ghost_button_style(mode, false))
                            .on_press(DashboardMsg::CopyText(tr_copy)),
                            button(
                                text(i18n.t("history.save_vocab"))
                                    .size(11)
                                    .font(ui_font)
                                    .shaping(text::Shaping::Advanced)
                            )
                            .padding([4, 10])
                            .style(ghost_button_style(mode, false))
                            .on_press(DashboardMsg::SaveHistoryToVocab(entry_clone)),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        text(item.source_text.clone())
                            .size(12)
                            .font(font_for_text(&item.source_text, None, Weight::Normal))
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_secondary),
                        text(item.translated_text.clone())
                            .size(14)
                            .font(font_for_text(
                                &item.translated_text,
                                Some(&item.target_lang),
                                Weight::Medium
                            ))
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_primary),
                    ]
                    .spacing(6),
                )
                .width(Length::Fill)
                .padding(12)
                .style(card_style(mode, 10.0)),
            );
        }
        content = content.push(list);
    }

    scrollable(content).height(Length::Fill).into()
}

fn view_settings_tab<'a>(
    state: &'a DashboardState,
    config: &'a AppConfig,
    i18n: &'a Translator,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_bold = font_for_ui(i18n.direction(), Weight::Semibold);

    let locales: Vec<LocaleMeta> = i18n.available_locales();
    let sel_locale = locales
        .iter()
        .find(|l| l.code == config.ui_language)
        .cloned();

    let appearance_card = container(
        column![
            text(i18n.t("settings.title"))
                .size(18)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary),
            row![
                column![
                    text(i18n.t("settings.language_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    pick_list(locales, sel_locale, |loc| {
                        DashboardMsg::UiLanguageChanged(loc.code)
                    })
                    .text_size(13)
                    .padding([8, 12]),
                ]
                .spacing(6)
                .width(Length::Fill),
                column![
                    text(i18n.t("settings.theme_label"))
                        .size(12)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_secondary),
                    row![
                        button(
                            text(i18n.t("settings.theme_dark"))
                                .size(12)
                                .font(ui_font)
                                .shaping(text::Shaping::Advanced)
                        )
                        .padding([8, 14])
                        .style(ghost_button_style(
                            mode,
                            config.theme == langhian_core::UiThemeMode::Dark
                        ))
                        .on_press(DashboardMsg::ThemeChanged(langhian_core::UiThemeMode::Dark)),
                        button(
                            text(i18n.t("settings.theme_light"))
                                .size(12)
                                .font(ui_font)
                                .shaping(text::Shaping::Advanced)
                        )
                        .padding([8, 14])
                        .style(ghost_button_style(
                            mode,
                            config.theme == langhian_core::UiThemeMode::Light
                        ))
                        .on_press(DashboardMsg::ThemeChanged(
                            langhian_core::UiThemeMode::Light
                        )),
                    ]
                    .spacing(8),
                ]
                .spacing(6)
                .width(Length::Fill),
            ]
            .spacing(16),
        ]
        .spacing(14),
    )
    .width(Length::Fill)
    .padding(16)
    .style(card_style(mode, 12.0));

    let shortcuts_card = container(
        column![
            text(i18n.t("settings.shortcuts_section"))
                .size(15)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary),
            checkbox(config.shortcuts.enable_double_alt)
                .label(i18n.t("settings.enable_double_alt"))
                .on_toggle(DashboardMsg::ToggleDoubleAlt)
                .text_size(13),
            text(i18n.t("settings.double_alt_desc"))
                .size(11)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .color(p.text_muted),
            checkbox(config.shortcuts.auto_select_all_fallback)
                .label(i18n.t("settings.auto_select_all_fallback"))
                .on_toggle(DashboardMsg::ToggleAutoSelectFallback)
                .text_size(13),
            text(i18n.t("settings.auto_select_all_desc"))
                .size(11)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .color(p.text_muted),
            checkbox(config.shortcuts.enable_double_ctrl)
                .label(i18n.t("settings.enable_double_ctrl"))
                .on_toggle(DashboardMsg::ToggleDoubleCtrl)
                .text_size(13),
            text(i18n.t("settings.double_ctrl_desc"))
                .size(11)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .color(p.text_muted),
            row![
                hotkey_field(
                    i18n.t("settings.double_tap_ms"),
                    &state.double_tap_ms_draft,
                    DashboardMsg::DoubleTapMsChanged,
                    mode,
                    ui_font,
                    p.text_secondary
                ),
                hotkey_field(
                    i18n.t("settings.screenshot_hotkey"),
                    &state.screenshot_hotkey_draft,
                    DashboardMsg::ScreenshotHotkeyChanged,
                    mode,
                    ui_font,
                    p.text_secondary
                ),
                hotkey_field(
                    i18n.t("settings.dashboard_hotkey"),
                    &state.dashboard_hotkey_draft,
                    DashboardMsg::DashboardHotkeyChanged,
                    mode,
                    ui_font,
                    p.text_secondary
                ),
            ]
            .spacing(12),
            row![
                hotkey_field(
                    i18n.t("settings.next_prompt_hotkey"),
                    &state.next_prompt_hotkey_draft,
                    DashboardMsg::NextPromptHotkeyChanged,
                    mode,
                    ui_font,
                    p.text_secondary
                ),
                hotkey_field(
                    i18n.t("settings.prev_prompt_hotkey"),
                    &state.prev_prompt_hotkey_draft,
                    DashboardMsg::PrevPromptHotkeyChanged,
                    mode,
                    ui_font,
                    p.text_secondary
                ),
            ]
            .spacing(12),
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .padding(16)
    .style(card_style(mode, 12.0));

    let behavior_card = container(
        column![
            text(i18n.t("settings.behavior_section"))
                .size(15)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary),
            checkbox(config.restore_clipboard)
                .label(i18n.t("settings.restore_clipboard"))
                .on_toggle(DashboardMsg::ToggleRestoreClipboard)
                .text_size(13),
            checkbox(config.launch_at_startup)
                .label(i18n.t("settings.launch_at_startup"))
                .on_toggle(DashboardMsg::ToggleLaunchStartup)
                .text_size(13),
            checkbox(config.show_hud)
                .label(i18n.t("settings.show_hud"))
                .on_toggle(DashboardMsg::ToggleShowHud)
                .text_size(13),
            text(i18n.t("settings.platform_notes_desc"))
                .size(11)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .color(p.text_muted),
            row![
                button(
                    text(i18n.t("settings.save_settings"))
                        .size(13)
                        .font(ui_bold)
                        .shaping(text::Shaping::Advanced)
                )
                .padding([8, 16])
                .style(primary_button_style(mode))
                .on_press(DashboardMsg::SaveSettings),
                if let Some(toast) = &state.settings_toast {
                    Element::from(
                        text(toast.clone())
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.success),
                    )
                } else {
                    Element::from(Space::new().width(0))
                }
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .padding(16)
    .style(card_style(mode, 12.0));

    // Updates & About card
    let mut updates_col = column![
        row![
            text(i18n.t("updates.section_title"))
                .size(15)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary)
                .width(Length::Fill),
            text(format!(
                "{}: v{}",
                i18n.t("updates.current_version"),
                env!("CARGO_PKG_VERSION")
            ))
            .size(12)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .color(p.accent_text),
        ]
        .align_y(Alignment::Center),
        checkbox(config.auto_check_updates)
            .label(i18n.t("updates.auto_check"))
            .on_toggle(DashboardMsg::ToggleAutoCheckUpdates)
            .text_size(13),
        row![
            button(
                text(if state.update_checking {
                    i18n.t("updates.checking")
                } else {
                    i18n.t("updates.check_btn")
                })
                .size(12)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
            )
            .padding([8, 14])
            .style(primary_button_style(mode))
            .on_press(DashboardMsg::CheckForUpdates),
            button(
                text(i18n.t("updates.open_release"))
                    .size(12)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
            )
            .padding([8, 12])
            .style(ghost_button_style(mode, false))
            .on_press(DashboardMsg::OpenReleaseUrl(
                "https://github.com/hamides-dev/langhian/releases".to_string()
            )),
        ]
        .spacing(10),
    ]
    .spacing(10);

    if let Some(err) = &state.update_error {
        updates_col = updates_col.push(
            text(format!("{}: {err}", i18n.t("updates.check_failed")))
                .size(12)
                .color(p.danger),
        );
    }

    if let Some(res) = &state.update_result {
        if res.has_update {
            updates_col = updates_col.push(
                container(
                    column![
                        text(format!(
                            "{}: v{}",
                            i18n.t("updates.available"),
                            res.latest_version
                        ))
                        .size(13)
                        .font(ui_bold)
                        .shaping(text::Shaping::Advanced)
                        .color(p.success),
                        text(res.notes.clone()).size(12).color(p.text_secondary),
                        button(
                            text(if state.update_downloading {
                                i18n.t("updates.downloading")
                            } else {
                                i18n.t("updates.download_install")
                            })
                            .size(12)
                            .font(ui_bold)
                            .shaping(text::Shaping::Advanced)
                        )
                        .padding([8, 14])
                        .style(primary_button_style(mode))
                        .on_press(DashboardMsg::DownloadAndInstallUpdate),
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
                .padding(12)
                .style(badge_style(p.accent_soft, p.text_primary, p.border_strong)),
            );
        } else {
            updates_col = updates_col.push(
                text(i18n.t("updates.up_to_date"))
                    .size(12)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .color(p.success),
            );
        }
    }

    let updates_card = container(updates_col)
        .width(Length::Fill)
        .padding(16)
        .style(card_style(mode, 12.0));

    scrollable(column![appearance_card, shortcuts_card, behavior_card, updates_card].spacing(14))
        .height(Length::Fill)
        .into()
}

fn hotkey_field<'a>(
    label: String,
    value: &'a str,
    on_change: impl Fn(String) -> DashboardMsg + 'a,
    mode: langhian_core::UiThemeMode,
    ui_font: iced::Font,
    label_color: iced::Color,
) -> Element<'a, DashboardMsg> {
    column![
        text(label)
            .size(12)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .color(label_color),
        text_input("", value)
            .on_input(on_change)
            .style(sleek_input_style(mode))
            .padding(8)
            .size(12),
    ]
    .spacing(4)
    .width(Length::Fill)
    .into()
}
