use crate::ui::icons::{UiIcon, app_logo, ui_icon};
use crate::ui::theme::{
    FramelessShellKind, aligned_text, badge_style, card_style, elevated_box_style, font_for_text,
    font_for_ui, ghost_button_style, has_rich_markdown, palette_for, pill_button_style,
    primary_button_style, render_markdown_with_highlights, rtl_row, seamless_editor_style,
    sleek_scrollable, square_icon_button, window_close_button_style, wrap_frameless_resize_frame,
};
use iced::font::Weight;
use iced::widget::{Space, button, column, container, image, mouse_area, row, text, text_editor};
use iced::{Alignment, Color, ContentFit, Element, Length, Padding, window};
use langhian_core::{
    AppConfig, TextSegment, Translator, VocabEntry, VocabMatcher, find_vocab_match_index,
    language_display_name,
};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ModalState {
    pub source_editor: text_editor::Content,
    pub output_editor: text_editor::Content,
    pub output_force_selectable: bool,
    pub source_text: String,
    pub screenshot_png: Option<Vec<u8>>,
    pub screenshot_handle: Option<image::Handle>,
    pub translated_text: String,
    pub effective_target_lang: String,
    pub detected_source_lang: Option<String>,
    pub was_auto_reversed: bool,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub is_vision_error: bool,
    pub pinned: bool,
    pub close_hovered: bool,
    pub selected_vocab_preview: Option<VocabEntry>,
    pub copied_source: bool,
    pub copied_source_seq: u64,
    pub copied_output: bool,
    pub copied_output_seq: u64,
}

fn decode_screenshot_handle(png_bytes: &[u8]) -> image::Handle {
    match ::image::load_from_memory(png_bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            image::Handle::from_rgba(width, height, rgba.into_raw())
        }
        Err(_) => image::Handle::from_bytes(png_bytes.to_vec()),
    }
}

impl ModalState {
    pub fn for_text(source_text: String, pinned: bool, default_target_lang: &str) -> Self {
        let source_editor = text_editor::Content::with_text(&source_text);
        Self {
            source_editor,
            output_editor: text_editor::Content::new(),
            output_force_selectable: false,
            source_text,
            screenshot_png: None,
            screenshot_handle: None,
            translated_text: String::new(),
            effective_target_lang: default_target_lang.to_string(),
            detected_source_lang: None,
            was_auto_reversed: false,
            is_loading: true,
            error_message: None,
            is_vision_error: false,
            pinned,
            close_hovered: false,
            selected_vocab_preview: None,
            copied_source: false,
            copied_source_seq: 0,
            copied_output: false,
            copied_output_seq: 0,
        }
    }

    pub fn for_screenshot(png_bytes: Vec<u8>, pinned: bool, default_target_lang: &str) -> Self {
        let screenshot_handle = Some(decode_screenshot_handle(&png_bytes));
        Self {
            source_editor: text_editor::Content::new(),
            output_editor: text_editor::Content::new(),
            output_force_selectable: false,
            source_text: String::new(),
            screenshot_png: Some(png_bytes),
            screenshot_handle,
            translated_text: String::new(),
            effective_target_lang: default_target_lang.to_string(),
            detected_source_lang: None,
            was_auto_reversed: false,
            is_loading: true,
            error_message: None,
            is_vision_error: false,
            pinned,
            close_hovered: false,
            selected_vocab_preview: None,
            copied_source: false,
            copied_source_seq: 0,
            copied_output: false,
            copied_output_seq: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModalMsg {
    EditSourceAction(text_editor::Action),
    EditOutputAction(text_editor::Action),
    ToggleOutputSelectable,
    TranslateOrRegenerate,
    CopySource,
    CopyOutput,
    ToggleStarVocab,
    TogglePin,
    CloseHovered(bool),
    Close,
    DragWindow,
    ResizeWindow(window::Direction),
    ClickVocabHighlight(VocabEntry),
    DismissVocabPreview,
    SwitchToModel(String),
    OpenModelsSettings,
}

pub fn view_modal<'a>(
    state: &'a ModalState,
    config: &'a AppConfig,
    i18n: &'a Translator,
    vocab: &'a [VocabEntry],
    highlighter: &'a VocabMatcher,
) -> Element<'a, ModalMsg> {
    let mode = config.theme;
    let p = palette_for(mode);
    let is_rtl = i18n.direction().is_rtl();
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Medium);
    let ui_font_bold = font_for_ui(i18n.direction(), Weight::Bold);

    // Header: Only App Brand ("Langhian") + Pin + Close (drag handle across header title area)
    let brand_group = mouse_area(
        rtl_row(
            is_rtl,
            vec![
                app_logo(20.0).into(),
                text(i18n.t("app.title"))
                    .size(13)
                    .font(ui_font_bold)
                    .color(p.text_primary)
                    .shaping(iced::widget::text::Shaping::Advanced)
                    .into(),
                container(
                    text(language_display_name(&state.effective_target_lang))
                        .size(10)
                        .font(ui_font_semi)
                        .color(p.accent_text)
                        .shaping(iced::widget::text::Shaping::Advanced),
                )
                .padding([2, 8])
                .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle))
                .into(),
                Space::new().width(Length::Fill).into(),
            ],
        )
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .on_press(ModalMsg::DragWindow);

    let pin_btn = square_icon_button(
        ui_icon(
            if state.pinned {
                UiIcon::KeepFill
            } else {
                UiIcon::Keep
            },
            15.0,
            if state.pinned {
                p.accent_text
            } else {
                p.text_secondary
            },
        ),
        30.0,
        ModalMsg::TogglePin,
        ghost_button_style(mode, state.pinned),
    );

    let close_btn = mouse_area(square_icon_button(
        ui_icon(
            UiIcon::Close,
            15.0,
            if state.close_hovered {
                Color::WHITE
            } else {
                p.text_secondary
            },
        ),
        30.0,
        ModalMsg::Close,
        window_close_button_style(mode),
    ))
    .on_enter(ModalMsg::CloseHovered(true))
    .on_exit(ModalMsg::CloseHovered(false));

    let header = rtl_row(
        is_rtl,
        vec![
            brand_group.into(),
            rtl_row(is_rtl, vec![pin_btn.into(), close_btn.into()])
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
        ],
    )
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    // 1) Editable Source Box + 1:1 square Copy icon button
    let source_section: Element<'a, ModalMsg> = if let Some(handle) = &state.screenshot_handle {
        container(
            image(handle.clone())
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .padding(10)
        .width(Length::Fill)
        .height(Length::FillPortion(2))
        .style(elevated_box_style(mode, 10.0))
        .into()
    } else {
        let copy_src_icon = square_icon_button(
            ui_icon(
                if state.copied_source {
                    UiIcon::Check
                } else {
                    UiIcon::Copy
                },
                14.0,
                if state.copied_source {
                    p.success
                } else {
                    p.text_secondary
                },
            ),
            30.0,
            ModalMsg::CopySource,
            ghost_button_style(mode, state.copied_source),
        );

        let src_top_bar = rtl_row(
            is_rtl,
            vec![
                Space::new().width(Length::Fill).into(),
                copy_src_icon.into(),
            ],
        )
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let source_text_preview = state.source_editor.text();
        let src_font = font_for_text(&source_text_preview, None, Weight::Normal);

        container(
            column![
                src_top_bar,
                text_editor(&state.source_editor)
                    .placeholder(i18n.t("translate.source_placeholder"))
                    .on_action(ModalMsg::EditSourceAction)
                    .font(src_font)
                    .size(13.5)
                    .padding([8, 12])
                    .height(Length::Fill)
                    .style(seamless_editor_style(mode)),
            ]
            .spacing(6)
            .height(Length::Fill),
        )
        .padding([10, 14])
        .width(Length::Fill)
        .height(Length::FillPortion(2))
        .style(card_style(mode, 10.0))
        .into()
    };

    // Single crisp Translate / Re-translate button bar
    let translate_btn = button(
        rtl_row(
            is_rtl,
            vec![
                ui_icon(UiIcon::Translate, 15.0, Color::WHITE).into(),
                text(i18n.t("translate.translate_retranslate"))
                    .size(12.5)
                    .font(ui_font_semi)
                    .color(Color::WHITE)
                    .shaping(iced::widget::text::Shaping::Advanced)
                    .into(),
            ],
        )
        .spacing(7)
        .align_y(Alignment::Center),
    )
    .on_press(ModalMsg::TranslateOrRegenerate)
    .padding([8, 18])
    .style(primary_button_style(mode));

    let action_strip = rtl_row(
        is_rtl,
        vec![
            Space::new().width(Length::Fill).into(),
            translate_btn.into(),
            Space::new().width(Length::Fill).into(),
        ],
    )
    .align_y(Alignment::Center)
    .width(Length::Fill);

    // 2) Output Box + Star, Optional Markdown/Selectable toggle & 1:1 square Copy icon button
    let has_md = has_rich_markdown(&state.translated_text, config.preserve_markdown);
    let show_selectable_editor = !has_md || state.output_force_selectable;
    let output_ready = !state.is_loading
        && state.error_message.is_none()
        && !state.translated_text.trim().is_empty();

    let is_starred = !state.source_text.trim().is_empty()
        && !state.translated_text.trim().is_empty()
        && find_vocab_match_index(vocab, &state.source_text, &state.translated_text).is_some();

    let star_btn = square_icon_button(
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
        30.0,
        ModalMsg::ToggleStarVocab,
        ghost_button_style(mode, is_starred),
    );

    let copy_out_btn = square_icon_button(
        ui_icon(
            if state.copied_output {
                UiIcon::Check
            } else {
                UiIcon::Copy
            },
            14.0,
            if state.copied_output {
                p.success
            } else {
                p.text_secondary
            },
        ),
        30.0,
        ModalMsg::CopyOutput,
        ghost_button_style(mode, state.copied_output),
    );

    let mut toolbar_items: Vec<Element<'a, ModalMsg>> =
        vec![Space::new().width(Length::Fill).into()];
    if has_md && output_ready {
        toolbar_items.push(
            square_icon_button(
                ui_icon(
                    if show_selectable_editor {
                        UiIcon::MenuBook
                    } else {
                        UiIcon::Edit
                    },
                    14.0,
                    if state.output_force_selectable {
                        p.accent_text
                    } else {
                        p.text_secondary
                    },
                ),
                30.0,
                ModalMsg::ToggleOutputSelectable,
                ghost_button_style(mode, state.output_force_selectable),
            )
            .into(),
        );
    }
    toolbar_items.push(star_btn.into());
    toolbar_items.push(copy_out_btn.into());

    let out_toolbar = rtl_row(is_rtl, toolbar_items)
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let output_inner: Element<'a, ModalMsg> = if state.is_loading {
        container(aligned_text(
            i18n.t("app.status_translating"),
            13.0,
            ui_font_semi,
            Some(p.text_secondary),
            is_rtl,
        ))
        .padding([12, 10])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if let Some(err) = &state.error_message {
        let mut err_col = column![aligned_text(
            err.clone(),
            13.0,
            ui_font,
            Some(p.danger),
            is_rtl
        )]
        .spacing(8);

        if state.is_vision_error {
            let vision_models: Vec<_> =
                config.models.iter().filter(|m| m.supports_vision).collect();
            if !vision_models.is_empty() {
                let chip_elems: Vec<Element<'a, ModalMsg>> = vision_models
                    .into_iter()
                    .map(|vm| {
                        button(text(&vm.name).size(11).font(ui_font_semi))
                            .on_press(ModalMsg::SwitchToModel(vm.id.clone()))
                            .padding([4, 10])
                            .style(pill_button_style(mode))
                            .into()
                    })
                    .collect();
                err_col = err_col.push(row(chip_elems).spacing(6).wrap());
            } else {
                err_col = err_col.push(
                    button(
                        text(i18n.t("models.title"))
                            .size(11)
                            .font(ui_font_semi)
                            .shaping(iced::widget::text::Shaping::Advanced),
                    )
                    .on_press(ModalMsg::OpenModelsSettings)
                    .padding([4, 10])
                    .style(pill_button_style(mode)),
                );
            }
        }
        sleek_scrollable(
            container(err_col).padding([8, 10]).width(Length::Fill),
            mode,
        )
        .height(Length::Fill)
        .into()
    } else if state.translated_text.trim().is_empty() {
        container(aligned_text(
            i18n.t("translate.result_placeholder"),
            13.0,
            ui_font,
            Some(p.text_muted),
            is_rtl,
        ))
        .padding([10, 10])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if show_selectable_editor {
        let out_font = font_for_text(
            &state.translated_text,
            Some(&state.effective_target_lang),
            Weight::Normal,
        );
        let editor = text_editor(&state.output_editor)
            .on_action(ModalMsg::EditOutputAction)
            .font(out_font)
            .size(14.5)
            .padding([8, 12])
            .height(Length::Fill)
            .style(seamless_editor_style(mode));

        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut matched_chips: Vec<Element<'a, ModalMsg>> = Vec::new();
        for seg in highlighter.highlight_segments(&state.translated_text) {
            if let TextSegment::Saved { entry, .. } = seg
                && seen_ids.insert(entry.id.clone())
            {
                let chip_label = entry.term.clone();
                let chip_font = font_for_text(&chip_label, None, Weight::Medium);
                matched_chips.push(
                    button(
                        text(chip_label)
                            .size(11)
                            .font(chip_font)
                            .color(p.vocab_highlight_text)
                            .shaping(iced::widget::text::Shaping::Advanced),
                    )
                    .on_press(ModalMsg::ClickVocabHighlight(entry))
                    .padding([3, 9])
                    .style(ghost_button_style(mode, false))
                    .into(),
                );
            }
        }

        if matched_chips.is_empty() {
            editor.into()
        } else {
            column![
                editor,
                container(row(matched_chips).spacing(6).wrap())
                    .padding([2, 2])
                    .width(Length::Fill),
            ]
            .spacing(6)
            .height(Length::Fill)
            .into()
        }
    } else {
        sleek_scrollable(
            container(render_markdown_with_highlights(
                &state.translated_text,
                config.preserve_markdown,
                Some(&state.effective_target_lang),
                highlighter,
                ModalMsg::ClickVocabHighlight,
                mode,
                14.5,
            ))
            .padding(Padding::from([8, 12]))
            .width(Length::Fill),
            mode,
        )
        .height(Length::Fill)
        .into()
    };

    let output_section = container(
        column![out_toolbar, output_inner]
            .spacing(6)
            .height(Length::Fill),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .height(Length::FillPortion(3))
    .style(card_style(mode, 10.0));

    let mut body = column![header, source_section, action_strip, output_section]
        .spacing(10)
        .height(Length::Fill);

    if let Some(v) = &state.selected_vocab_preview {
        let preview = container(
            rtl_row(
                is_rtl,
                vec![
                    {
                        let mut preview_col = column![aligned_text(
                            format!("{} -> {}", v.term, v.translation),
                            12.0,
                            font_for_text(&v.term, None, Weight::Medium),
                            Some(p.vocab_highlight_text),
                            is_rtl,
                        )]
                        .spacing(2)
                        .width(Length::Fill);
                        if !v.notes.is_empty() {
                            preview_col = preview_col.push(aligned_text(
                                v.notes.as_str(),
                                11.0,
                                ui_font,
                                Some(p.text_secondary),
                                is_rtl,
                            ));
                        }
                        Element::from(preview_col)
                    },
                    square_icon_button(
                        ui_icon(UiIcon::Close, 13.0, p.text_secondary),
                        26.0,
                        ModalMsg::DismissVocabPreview,
                        ghost_button_style(mode, false),
                    )
                    .into(),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .style(elevated_box_style(mode, 8.0));

        body = body.push(preview);
    }

    let modal_inner = container(body)
        .padding(14)
        .width(Length::Fill)
        .height(Length::Fill);

    wrap_frameless_resize_frame(
        modal_inner.into(),
        mode,
        FramelessShellKind::Modal,
        14.0,
        false,
        ModalMsg::ResizeWindow,
    )
}
