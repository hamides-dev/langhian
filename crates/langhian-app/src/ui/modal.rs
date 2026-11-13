use crate::ui::theme::{
    badge_style, card_style, elevated_box_style, font_for_ui, ghost_button_style, hairline_divider,
    modal_surface_style, palette_for, pill_button_style, primary_button_style,
    render_highlighted_text, sleek_input_style, sleek_menu_style, sleek_pick_list_style,
};
use iced::font::Weight;
use iced::widget::{
    Space, button, column, container, mouse_area, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Color, Element, Length, Padding};
use langhian_core::{
    AppConfig, Translator, UiThemeMode, VocabEntry, VocabMatcher, supported_languages,
};
use std::fmt;

#[derive(Debug, Clone)]
pub struct ModalState {
    pub source_text: String,
    pub screenshot_png: Option<Vec<u8>>,
    pub translated_text: String,
    pub detected_source_lang: Option<String>,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub is_vision_error: bool,
    pub pinned: bool,
    pub show_save_drawer: bool,
    pub save_term: String,
    pub save_translation: String,
    pub save_notes: String,
    pub selected_vocab_preview: Option<VocabEntry>,
    pub copied_feedback: bool,
    pub saved_feedback: bool,
}

impl ModalState {
    pub fn for_text(source: String) -> Self {
        Self {
            save_term: source.trim().to_string(),
            source_text: source,
            screenshot_png: None,
            translated_text: String::new(),
            detected_source_lang: None,
            is_loading: true,
            error_message: None,
            is_vision_error: false,
            pinned: false,
            show_save_drawer: false,
            save_translation: String::new(),
            save_notes: String::new(),
            selected_vocab_preview: None,
            copied_feedback: false,
            saved_feedback: false,
        }
    }

    pub fn for_screenshot(png_bytes: Vec<u8>) -> Self {
        Self {
            source_text: String::new(),
            screenshot_png: Some(png_bytes),
            translated_text: String::new(),
            detected_source_lang: None,
            is_loading: true,
            error_message: None,
            is_vision_error: false,
            pinned: false,
            show_save_drawer: false,
            save_term: String::new(),
            save_translation: String::new(),
            save_notes: String::new(),
            selected_vocab_preview: None,
            copied_feedback: false,
            saved_feedback: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModalMsg {
    DragWindow,
    Close,
    TogglePin,
    SelectModel(String),
    SelectPrompt(String),
    SelectTargetLang(String),
    Regenerate,
    CopyResult,
    ReplaceInSourceApp,
    ClickHighlightedVocab(VocabEntry),
    DismissVocabPreview,
    ToggleSaveDrawer,
    SaveTermChanged(String),
    SaveTranslationChanged(String),
    SaveNotesChanged(String),
    ConfirmSaveVocab,
    OpenModelsSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PickerOption {
    id: String,
    label: String,
}

impl fmt::Display for PickerOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

pub fn view_modal<'a>(
    state: &'a ModalState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    highlighter: &'a VocabMatcher,
) -> Element<'a, ModalMsg> {
    let mode: UiThemeMode = config.theme;
    let p = palette_for(mode);
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Semibold);
    let ui_font_bold = font_for_ui(i18n.direction(), Weight::Bold);

    // Header row (draggable)
    let mode_tag = if state.screenshot_png.is_some() {
        i18n.t("history.mode_screenshot")
    } else {
        i18n.t("history.mode_modal")
    };

    let drag_brand = mouse_area(
        row![
            container(Space::new().width(8).height(8)).style(move |_| container::Style {
                background: Some(iced::Background::Color(p.accent)),
                border: iced::Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            text("Langhian")
                .size(13)
                .font(ui_font_bold)
                .color(p.text_primary),
            container(
                text(mode_tag)
                    .size(11)
                    .font(ui_font_semi)
                    .color(p.accent_text)
            )
            .padding([2, 8])
            .style(badge_style(
                p.accent_soft,
                p.accent_text,
                Color::TRANSPARENT
            )),
            Space::new().width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(ModalMsg::DragWindow);

    let pin_label = if state.pinned {
        i18n.t("translate.unpin")
    } else {
        i18n.t("translate.pin")
    };
    let pin_btn = button(text(pin_label).size(11).font(ui_font_semi))
        .on_press(ModalMsg::TogglePin)
        .padding([4, 10])
        .style(ghost_button_style(mode, state.pinned));

    let close_btn = button(text("✕").size(12).font(ui_font_semi))
        .on_press(ModalMsg::Close)
        .padding([4, 10])
        .style(ghost_button_style(mode, false));

    let top_header = container(
        row![drag_brand, pin_btn, close_btn]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding(Padding::from([10, 14]));

    // Controls strip
    let model_options: Vec<PickerOption> = config
        .models
        .iter()
        .map(|m| PickerOption {
            id: m.id.clone(),
            label: if m.supports_vision {
                format!("{} · Vision", m.name)
            } else {
                m.name.clone()
            },
        })
        .collect();
    let selected_model = model_options
        .iter()
        .find(|o| o.id == config.active_model_id)
        .cloned();

    let prompt_options: Vec<PickerOption> = config
        .prompts
        .iter()
        .map(|pr| PickerOption {
            id: pr.id.clone(),
            label: pr.name.clone(),
        })
        .collect();
    let selected_prompt = prompt_options
        .iter()
        .find(|o| o.id == config.active_prompt_id)
        .cloned();

    let lang_options: Vec<PickerOption> = supported_languages()
        .iter()
        .filter(|l| l.code != "auto")
        .map(|l| PickerOption {
            id: l.code.to_string(),
            label: format!("{} ({})", l.name, l.native_name),
        })
        .collect();
    let selected_lang = lang_options
        .iter()
        .find(|o| o.id == config.target_language)
        .cloned();

    let model_picker = pick_list(model_options, selected_model, |o| {
        ModalMsg::SelectModel(o.id)
    })
    .font(ui_font)
    .text_size(12)
    .padding([5, 10])
    .width(Length::FillPortion(3))
    .style(sleek_pick_list_style(mode))
    .menu_style(sleek_menu_style(mode));

    let prompt_picker = pick_list(prompt_options, selected_prompt, |o| {
        ModalMsg::SelectPrompt(o.id)
    })
    .font(ui_font)
    .text_size(12)
    .padding([5, 10])
    .width(Length::FillPortion(2))
    .style(sleek_pick_list_style(mode))
    .menu_style(sleek_menu_style(mode));

    let target_picker = pick_list(lang_options, selected_lang, |o| {
        ModalMsg::SelectTargetLang(o.id)
    })
    .font(ui_font)
    .text_size(12)
    .padding([5, 10])
    .width(Length::FillPortion(2))
    .style(sleek_pick_list_style(mode))
    .menu_style(sleek_menu_style(mode));

    let regen_btn = button(
        text(i18n.t("translate.regenerate"))
            .size(12)
            .font(ui_font_semi),
    )
    .on_press(ModalMsg::Regenerate)
    .padding([6, 12])
    .style(pill_button_style(mode));

    let controls_bar = container(
        row![model_picker, prompt_picker, target_picker, regen_btn]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 14]));

    // Source excerpt section
    let mut source_section = column![].spacing(6);
    if !state.source_text.trim().is_empty() {
        let mut label_row = row![
            text(i18n.t("translate.source_label"))
                .size(11)
                .font(ui_font_semi)
                .color(p.text_muted)
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if let Some(det) = &state.detected_source_lang {
            label_row = label_row.push(
                container(
                    text(det.to_uppercase())
                        .size(10)
                        .font(ui_font_semi)
                        .color(p.accent_text),
                )
                .padding([1, 6])
                .style(badge_style(
                    p.accent_soft,
                    p.accent_text,
                    Color::TRANSPARENT,
                )),
            );
        }

        let src_preview = scrollable(render_highlighted_text(
            &state.source_text,
            None,
            highlighter,
            ModalMsg::ClickHighlightedVocab,
            mode,
            13.0,
        ))
        .height(Length::Fixed(74.0));

        source_section = source_section.push(
            container(column![label_row, src_preview].spacing(6))
                .padding(12)
                .width(Length::Fill)
                .style(card_style(mode, 8.0)),
        );
    }

    // Translation output / loading / vision error section
    let result_area: Element<'a, ModalMsg> = if state.is_loading {
        container(
            row![
                text(i18n.t("app.status_translating"))
                    .size(13)
                    .font(ui_font_semi)
                    .color(p.text_secondary)
            ]
            .spacing(8),
        )
        .padding(18)
        .width(Length::Fill)
        .style(card_style(mode, 8.0))
        .into()
    } else if state.is_vision_error {
        let mut vision_btns = row![].spacing(8);
        let vision_models: Vec<_> = config.models.iter().filter(|m| m.supports_vision).collect();
        if vision_models.is_empty() {
            vision_btns = vision_btns.push(
                button(
                    text(i18n.t("models.edit_profile"))
                        .size(12)
                        .font(ui_font_semi),
                )
                .on_press(ModalMsg::OpenModelsSettings)
                .padding([6, 12])
                .style(primary_button_style(mode)),
            );
        } else {
            for m in vision_models.into_iter().take(3) {
                let id = m.id.clone();
                vision_btns = vision_btns.push(
                    button(text(m.name.clone()).size(12).font(ui_font_semi))
                        .on_press(ModalMsg::SelectModel(id))
                        .padding([6, 12])
                        .style(pill_button_style(mode)),
                );
            }
        }
        container(
            column![
                text(i18n.t("models.vision_required_title"))
                    .size(13)
                    .font(ui_font_bold)
                    .color(p.warning),
                text(i18n.t("models.vision_required_desc"))
                    .size(12)
                    .font(ui_font)
                    .color(p.text_secondary),
                text(i18n.t("models.switch_to_vision_model"))
                    .size(11)
                    .font(ui_font_semi)
                    .color(p.text_muted),
                vision_btns,
            ]
            .spacing(8),
        )
        .padding(14)
        .width(Length::Fill)
        .style(card_style(mode, 8.0))
        .into()
    } else if let Some(err) = &state.error_message {
        container(text(err.clone()).size(13).font(ui_font).color(p.danger))
            .padding(14)
            .width(Length::Fill)
            .style(card_style(mode, 8.0))
            .into()
    } else {
        let rendered = render_highlighted_text(
            &state.translated_text,
            Some(&config.target_language),
            highlighter,
            ModalMsg::ClickHighlightedVocab,
            mode,
            15.0,
        );
        container(scrollable(rendered).height(Length::Fill))
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(card_style(mode, 8.0))
            .into()
    };

    let mut body = column![source_section, result_area]
        .spacing(10)
        .height(Length::Fill);

    if let Some(entry) = &state.selected_vocab_preview {
        let preview_box = container(
            row![
                column![
                    row![
                        text(&entry.term)
                            .size(13)
                            .font(ui_font_bold)
                            .color(p.highlight_text),
                        text("→").size(12).color(p.text_muted),
                        text(&entry.translation)
                            .size(13)
                            .font(ui_font_semi)
                            .color(p.text_primary),
                    ]
                    .spacing(6),
                    text(&entry.notes)
                        .size(12)
                        .font(ui_font)
                        .color(p.text_secondary),
                ]
                .spacing(2)
                .width(Length::Fill),
                button(text("✕").size(11))
                    .on_press(ModalMsg::DismissVocabPreview)
                    .padding([4, 8])
                    .style(ghost_button_style(mode, false)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(10)
        .width(Length::Fill)
        .style(elevated_box_style(mode, 8.0));
        body = body.push(preview_box);
    }

    if state.show_save_drawer {
        let save_box = container(
            column![
                text(i18n.t("translate.save_word_title"))
                    .size(12)
                    .font(ui_font_bold)
                    .color(p.text_primary),
                row![
                    text_input(&i18n.t("translate.word_label"), &state.save_term)
                        .on_input(ModalMsg::SaveTermChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([7, 10])
                        .style(sleek_input_style(mode)),
                    text_input(&i18n.t("translate.meaning_label"), &state.save_translation)
                        .on_input(ModalMsg::SaveTranslationChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([7, 10])
                        .style(sleek_input_style(mode)),
                ]
                .spacing(8),
                row![
                    text_input(&i18n.t("translate.notes_label"), &state.save_notes)
                        .on_input(ModalMsg::SaveNotesChanged)
                        .font(ui_font)
                        .size(12)
                        .padding([7, 10])
                        .width(Length::Fill)
                        .style(sleek_input_style(mode)),
                    button(text(i18n.t("vocab.save")).size(12).font(ui_font_semi))
                        .on_press(ModalMsg::ConfirmSaveVocab)
                        .padding([7, 14])
                        .style(primary_button_style(mode)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fill)
        .style(elevated_box_style(mode, 8.0));
        body = body.push(save_box);
    }

    // Footer action bar
    let copy_label = if state.copied_feedback {
        i18n.t("translate.copied")
    } else {
        i18n.t("translate.copy_btn")
    };
    let save_label = if state.saved_feedback {
        i18n.t("translate.saved")
    } else {
        i18n.t("translate.save_btn")
    };

    let mut footer_row = row![
        text(i18n.t("translate.highlight_hint"))
            .size(11)
            .font(ui_font)
            .color(p.text_muted)
            .width(Length::Fill),
        button(text(save_label).size(12).font(ui_font_semi))
            .on_press(ModalMsg::ToggleSaveDrawer)
            .padding([7, 12])
            .style(pill_button_style(mode)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    if state.screenshot_png.is_none() && !state.translated_text.is_empty() {
        footer_row = footer_row.push(
            button(
                text(i18n.t("translate.replace_btn"))
                    .size(12)
                    .font(ui_font_semi),
            )
            .on_press(ModalMsg::ReplaceInSourceApp)
            .padding([7, 12])
            .style(pill_button_style(mode)),
        );
    }

    footer_row = footer_row.push(
        button(text(copy_label).size(12).font(ui_font_semi))
            .on_press(ModalMsg::CopyResult)
            .padding([7, 14])
            .style(primary_button_style(mode)),
    );

    let card = container(
        column![
            top_header,
            hairline_divider(mode),
            controls_bar,
            hairline_divider(mode),
            container(body).padding([12, 14]).height(Length::Fill),
            hairline_divider(mode),
            container(footer_row).padding([10, 14]),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(modal_surface_style(mode, 14.0));

    // Outer transparent padding gives rounded corners and soft drop shadow room without black rectangle clipping
    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(6)
        .into()
}
