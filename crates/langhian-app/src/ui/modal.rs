use crate::ui::theme::{
    badge_style, card_style, elevated_box_style, font_for_text, font_for_ui, ghost_button_style,
    palette_for, primary_button_style, render_highlighted_text, sleek_input_style,
};
use iced::font::Weight;
use iced::widget::{
    Space, button, column, container, image, mouse_area, pick_list, row, scrollable, text,
    text_input,
};
use iced::{Alignment, Element, Length};
use langhian_core::{
    AppConfig, LanguageOption, ModelProfile, Translator, VocabEntry, VocabMatcher,
    supported_languages,
};

#[derive(Debug, Clone)]
pub struct ModalState {
    pub source_text: String,
    pub screenshot_png: Option<Vec<u8>>,
    pub screenshot_handle: Option<image::Handle>,
    pub translated_text: String,
    pub detected_source_lang: Option<String>,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub is_vision_error: bool,
    pub pinned: bool,
    pub copied_feedback: bool,
    pub selected_vocab_preview: Option<VocabEntry>,
    pub show_save_drawer: bool,
    pub save_term: String,
    pub save_translation: String,
    pub save_notes: String,
    pub saved_feedback: bool,
}

impl ModalState {
    pub fn for_text(text: String) -> Self {
        Self {
            source_text: text.clone(),
            screenshot_png: None,
            screenshot_handle: None,
            translated_text: String::new(),
            detected_source_lang: None,
            is_loading: true,
            error_message: None,
            is_vision_error: false,
            pinned: false,
            copied_feedback: false,
            selected_vocab_preview: None,
            show_save_drawer: false,
            save_term: text,
            save_translation: String::new(),
            save_notes: String::new(),
            saved_feedback: false,
        }
    }

    pub fn for_screenshot(png_bytes: Vec<u8>) -> Self {
        let handle = image::Handle::from_bytes(png_bytes.clone());
        Self {
            source_text: String::new(),
            screenshot_png: Some(png_bytes),
            screenshot_handle: Some(handle),
            translated_text: String::new(),
            detected_source_lang: None,
            is_loading: true,
            error_message: None,
            is_vision_error: false,
            pinned: false,
            copied_feedback: false,
            selected_vocab_preview: None,
            show_save_drawer: false,
            save_term: String::new(),
            save_translation: String::new(),
            save_notes: String::new(),
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

pub fn view_modal<'a>(
    state: &'a ModalState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    matcher: &VocabMatcher,
) -> Element<'a, ModalMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let ui_dir = i18n.direction();
    let ui_font = font_for_ui(ui_dir, Weight::Normal);
    let ui_bold = font_for_ui(ui_dir, Weight::Semibold);
    let active_model = config.active_model();

    let model_options: Vec<ModelLabel> = config
        .models
        .iter()
        .map(|m| ModelLabel {
            id: m.id.clone(),
            label: if m.supports_vision {
                format!("{} [Vision]", m.name)
            } else {
                m.name.clone()
            },
        })
        .collect();
    let selected_model = model_options
        .iter()
        .find(|m| m.id == active_model.id)
        .cloned();

    let lang_options: Vec<LanguageOption> = supported_languages().to_vec();
    let selected_lang = lang_options
        .iter()
        .find(|l| l.code.eq_ignore_ascii_case(&config.target_language))
        .cloned();

    // Header bar (draggable)
    let brand = row![
        container(text(""))
            .width(10)
            .height(10)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(p.accent)),
                border: iced::Border {
                    radius: 99.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        text("Langhian")
            .size(14)
            .font(font_for_ui(langhian_core::TextDirection::Ltr, Weight::Bold))
            .color(p.text_primary),
        container(
            text(if active_model.supports_vision {
                i18n.t("models.vision_badge")
            } else {
                i18n.t("models.text_only_badge")
            })
            .size(11)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
        )
        .padding([3, 8])
        .style(if active_model.supports_vision {
            badge_style(p.accent_soft, p.accent_text, p.border_strong)
        } else {
            badge_style(p.bg_elevated, p.text_muted, p.border_subtle)
        }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let model_picker = pick_list(model_options, selected_model, |m| {
        ModalMsg::SelectModel(m.id)
    })
    .text_size(12)
    .padding([5, 10]);

    let lang_picker = pick_list(lang_options, selected_lang, |l| {
        ModalMsg::SelectTargetLang(l.code.to_string())
    })
    .text_size(12)
    .padding([5, 10]);

    let pin_btn = button(
        text(if state.pinned {
            i18n.t("translate.unpin")
        } else {
            i18n.t("translate.pin")
        })
        .size(11)
        .font(ui_font)
        .shaping(text::Shaping::Advanced),
    )
    .padding([5, 10])
    .style(ghost_button_style(mode, state.pinned))
    .on_press(ModalMsg::TogglePin);

    let close_btn = button(text("×").size(15))
        .padding([3, 10])
        .style(ghost_button_style(mode, false))
        .on_press(ModalMsg::Close);

    let header = mouse_area(
        row![
            brand,
            Space::new().width(Length::Fill),
            model_picker,
            lang_picker,
            pin_btn,
            close_btn,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(ModalMsg::DragWindow);

    // Prompt quick-switch pills
    let mut prompt_pills = row![].spacing(6).align_y(Alignment::Center);
    for pr in &config.prompts {
        let is_active = pr.id == config.active_prompt_id;
        let label = match pr.shortcut_slot {
            Some(slot) => format!("{slot} · {}", pr.name),
            None => pr.name.clone(),
        };
        let pid = pr.id.clone();
        prompt_pills = prompt_pills.push(
            button(
                text(label)
                    .size(11)
                    .font(font_for_text(&pr.name, None, Weight::Medium))
                    .shaping(text::Shaping::Advanced),
            )
            .padding([5, 10])
            .style(ghost_button_style(mode, is_active))
            .on_press(ModalMsg::SelectPrompt(pid)),
        );
    }
    let prompt_bar = scrollable(prompt_pills)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(3).scroller_width(3),
        ))
        .width(Length::Fill);

    // Source card
    let source_body: Element<'a, ModalMsg> = if let Some(handle) = &state.screenshot_handle {
        row![
            image(handle.clone())
                .width(Length::Fixed(180.0))
                .height(Length::Fixed(100.0)),
            column![
                text(i18n.t("translate.screenshot_btn"))
                    .size(13)
                    .font(ui_bold)
                    .shaping(text::Shaping::Advanced)
                    .color(p.text_primary),
                text(active_model.name.clone())
                    .size(12)
                    .color(p.text_secondary),
            ]
            .spacing(4)
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    } else {
        let segs = matcher.highlight_segments(&state.source_text);
        let has_matches = segs
            .iter()
            .any(|s| matches!(s, langhian_core::TextSegment::Saved { .. }));
        let rich = render_highlighted_text(
            segs,
            &state.source_text,
            state.detected_source_lang.as_deref(),
            mode,
            13.5,
            ModalMsg::ClickHighlightedVocab,
        );
        if has_matches {
            column![
                scrollable(rich).height(Length::Shrink),
                text(i18n.t("translate.highlight_hint"))
                    .size(11)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .color(p.warning),
            ]
            .spacing(6)
            .into()
        } else {
            scrollable(rich).height(Length::Shrink).into()
        }
    };

    let source_box = container(source_body)
        .width(Length::Fill)
        .padding(12)
        .max_height(145.0)
        .style(elevated_box_style(mode, 10.0));

    // Inline highlighted vocab preview popover
    let vocab_preview: Option<Element<'a, ModalMsg>> =
        state.selected_vocab_preview.as_ref().map(|entry| {
            container(
                row![
                    column![
                        text(format!("{}  →  {}", entry.term, entry.translation))
                            .size(13)
                            .font(font_for_text(&entry.translation, None, Weight::Semibold))
                            .shaping(text::Shaping::Advanced)
                            .color(p.warning),
                        if entry.notes.is_empty() {
                            Element::from(Space::new().height(0))
                        } else {
                            Element::from(
                                text(entry.notes.clone())
                                    .size(12)
                                    .font(font_for_text(&entry.notes, None, Weight::Normal))
                                    .shaping(text::Shaping::Advanced)
                                    .color(p.text_secondary),
                            )
                        }
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    button(text("×").size(13))
                        .padding([2, 8])
                        .style(ghost_button_style(mode, false))
                        .on_press(ModalMsg::DismissVocabPreview),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(badge_style(p.warning_soft, p.text_primary, p.warning))
            .into()
        });

    // Translation / Error output panel
    let output_body: Element<'a, ModalMsg> = if state.is_loading {
        container(
            text(i18n.t("app.status_translating"))
                .size(14)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .color(p.accent_text),
        )
        .padding(16)
        .width(Length::Fill)
        .into()
    } else if state.is_vision_error {
        let vision_models: Vec<&ModelProfile> =
            config.models.iter().filter(|m| m.supports_vision).collect();
        let mut switch_row = row![].spacing(6).align_y(Alignment::Center);
        for vm in vision_models.iter().take(3) {
            let vid = vm.id.clone();
            switch_row = switch_row.push(
                button(text(vm.name.clone()).size(12))
                    .padding([6, 10])
                    .style(primary_button_style(mode))
                    .on_press(ModalMsg::SelectModel(vid)),
            );
        }

        container(
            column![
                text(i18n.t("models.vision_required_title"))
                    .size(14)
                    .font(ui_bold)
                    .shaping(text::Shaping::Advanced)
                    .color(p.danger),
                text(i18n.t("models.vision_required_desc"))
                    .size(12)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .color(p.text_secondary),
                if vision_models.is_empty() {
                    row![
                        text(i18n.t("models.no_vision_models_hint"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.warning),
                        button(
                            text(i18n.t("nav.models"))
                                .size(12)
                                .font(ui_font)
                                .shaping(text::Shaping::Advanced)
                        )
                        .padding([6, 12])
                        .style(primary_button_style(mode))
                        .on_press(ModalMsg::OpenModelsSettings),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                } else {
                    row![
                        text(i18n.t("models.switch_to_vision_model"))
                            .size(12)
                            .font(ui_font)
                            .shaping(text::Shaping::Advanced)
                            .color(p.text_primary),
                        switch_row,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                }
            ]
            .spacing(10),
        )
        .width(Length::Fill)
        .padding(14)
        .style(badge_style(p.danger_soft, p.text_primary, p.danger))
        .into()
    } else if let Some(err) = &state.error_message {
        container(
            text(err.clone())
                .size(13)
                .font(font_for_text(err, None, Weight::Normal))
                .shaping(text::Shaping::Advanced)
                .color(p.danger),
        )
        .width(Length::Fill)
        .padding(12)
        .style(badge_style(p.danger_soft, p.danger, p.danger))
        .into()
    } else {
        let out_segs = matcher.highlight_segments(&state.translated_text);
        let rich_out = render_highlighted_text(
            out_segs,
            &state.translated_text,
            Some(&config.target_language),
            mode,
            15.0,
            ModalMsg::ClickHighlightedVocab,
        );
        container(scrollable(rich_out).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(14)
            .style(elevated_box_style(mode, 10.0))
            .into()
    };

    // Optional inline Save-to-Vocabulary drawer
    let save_drawer: Option<Element<'a, ModalMsg>> = if state.show_save_drawer {
        Some(
            container(
                column![
                    text(i18n.t("translate.save_word_title"))
                        .size(13)
                        .font(ui_bold)
                        .shaping(text::Shaping::Advanced)
                        .color(p.text_primary),
                    row![
                        text_input(&i18n.t("translate.word_label"), &state.save_term)
                            .on_input(ModalMsg::SaveTermChanged)
                            .font(font_for_text(&state.save_term, None, Weight::Normal))
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                        text_input(&i18n.t("translate.meaning_label"), &state.save_translation)
                            .on_input(ModalMsg::SaveTranslationChanged)
                            .font(font_for_text(
                                &state.save_translation,
                                Some(&config.target_language),
                                Weight::Normal
                            ))
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(13),
                    ]
                    .spacing(8),
                    row![
                        text_input(&i18n.t("translate.notes_label"), &state.save_notes)
                            .on_input(ModalMsg::SaveNotesChanged)
                            .font(font_for_text(&state.save_notes, None, Weight::Normal))
                            .style(sleek_input_style(mode))
                            .padding(8)
                            .size(12)
                            .width(Length::Fill),
                        button(
                            text(i18n.t("vocab.save"))
                                .size(12)
                                .font(ui_bold)
                                .shaping(text::Shaping::Advanced)
                        )
                        .padding([8, 14])
                        .style(primary_button_style(mode))
                        .on_press(ModalMsg::ConfirmSaveVocab),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding(12)
            .width(Length::Fill)
            .style(elevated_box_style(mode, 10.0))
            .into(),
        )
    } else {
        None
    };

    // Footer buttons
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

    let mut footer = row![
        button(
            text(copy_label)
                .size(12)
                .font(ui_bold)
                .shaping(text::Shaping::Advanced)
        )
        .padding([7, 14])
        .style(primary_button_style(mode))
        .on_press(ModalMsg::CopyResult),
        button(
            text(save_label)
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
        )
        .padding([7, 12])
        .style(ghost_button_style(mode, state.show_save_drawer))
        .on_press(ModalMsg::ToggleSaveDrawer),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    if state.screenshot_png.is_none() {
        footer = footer.push(
            button(
                text(i18n.t("translate.replace_btn"))
                    .size(12)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced),
            )
            .padding([7, 12])
            .style(ghost_button_style(mode, false))
            .on_press(ModalMsg::ReplaceInSourceApp),
        );
    }

    footer = footer.push(Space::new().width(Length::Fill)).push(
        button(
            text(i18n.t("translate.regenerate"))
                .size(12)
                .font(ui_font)
                .shaping(text::Shaping::Advanced),
        )
        .padding([7, 12])
        .style(ghost_button_style(mode, false))
        .on_press(ModalMsg::Regenerate),
    );

    let mut col = column![header, prompt_bar, source_box].spacing(10);
    if let Some(vp) = vocab_preview {
        col = col.push(vp);
    }
    col = col.push(
        container(output_body)
            .width(Length::Fill)
            .height(Length::Fill),
    );
    if let Some(sd) = save_drawer {
        col = col.push(sd);
    }
    col = col.push(footer);

    let card = container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .style(card_style(mode, 14.0));

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8)
        .into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelLabel {
    id: String,
    label: String,
}

impl std::fmt::Display for ModelLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}
