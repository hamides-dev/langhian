use crate::ui::dashboard::{DashboardMsg, PickerItem, UiLangChoice};
use crate::ui::icons::{UiIcon, app_logo, ui_icon};
use crate::ui::theme::{
    ThemePalette, aligned_centered_text, aligned_text, badge_style, card_style, elevated_box_style,
    font_for_ui, ghost_button_style, hairline_divider, horizontal_for_dir, keycap,
    pill_button_style, primary_button_style, rtl_row, sleek_menu_style, sleek_pick_list_style,
    sleek_scrollable,
};
use iced::alignment::Horizontal;
use iced::font::Weight;
use iced::widget::text::Shaping;
use iced::widget::{Space, button, column, container, pick_list, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};
use langhian_core::{AppConfig, Translator, language_display_name, supported_languages};

#[derive(Debug, Clone, Default)]
pub struct OnboardingState {
    pub step: u8,
}

pub fn view_onboarding<'a>(
    state: &'a OnboardingState,
    config: &'a AppConfig,
    i18n: &'a Translator,
) -> Element<'a, DashboardMsg> {
    let mode = config.theme;
    let p = crate::ui::theme::palette_for(mode);
    let is_rtl = i18n.direction().is_rtl();
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Medium);
    let ui_font_bold = font_for_ui(i18n.direction(), Weight::Bold);
    let h_align = horizontal_for_dir(is_rtl);

    let ui_langs = vec![
        UiLangChoice {
            code: "en",
            label: "English",
        },
        UiLangChoice {
            code: "fa",
            label: "فارسی (Persian)",
        },
        UiLangChoice {
            code: "tr",
            label: "Türkçe (Turkish)",
        },
    ];
    let selected_ui = ui_langs
        .iter()
        .find(|u| u.code == config.ui_language)
        .cloned();

    let mut step_dots = Vec::new();
    for idx in 0..3u8 {
        let active = state.step == idx;
        let done = state.step > idx;
        let bg = if active {
            p.accent
        } else if done {
            p.success
        } else {
            p.border_subtle
        };
        let pill = container(Space::new().width(if active { 26 } else { 10 }).height(6)).style(
            move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        step_dots.push(pill.into());
    }

    let top_bar = rtl_row(
        is_rtl,
        vec![
            rtl_row(is_rtl, step_dots)
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            Space::new().width(Length::Fill).into(),
            pick_list(ui_langs, selected_ui, |c| {
                DashboardMsg::UiLanguageChanged(c.code.to_string())
            })
            .font(ui_font)
            .text_size(12)
            .padding([6, 10])
            .width(Length::Fixed(165.0))
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode))
            .into(),
            button(
                text(i18n.t("onboarding.skip_btn"))
                    .size(12)
                    .font(ui_font_semi),
            )
            .on_press(DashboardMsg::OnboardingSkip)
            .padding([6, 12])
            .style(ghost_button_style(mode, false))
            .into(),
        ],
    )
    .spacing(10)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let hero = column![
        rtl_row(
            is_rtl,
            vec![
                app_logo(30.0).into(),
                text(i18n.t("onboarding.title"))
                    .size(22)
                    .font(ui_font_bold)
                    .color(p.text_primary)
                    .shaping(Shaping::Advanced)
                    .into(),
            ],
        )
        .spacing(10)
        .align_y(Alignment::Center),
        aligned_text(
            i18n.t("onboarding.subtitle"),
            13.0,
            ui_font,
            Some(p.text_secondary),
            is_rtl,
        ),
    ]
    .spacing(6)
    .align_x(h_align)
    .width(Length::Fill);

    let step_content: Element<'a, DashboardMsg> = match state.step {
        0 => view_step_capabilities(mode, p, i18n, is_rtl, ui_font, ui_font_semi, ui_font_bold),
        1 => view_step_languages(
            config,
            mode,
            p,
            i18n,
            is_rtl,
            ui_font,
            ui_font_semi,
            ui_font_bold,
        ),
        _ => view_step_ready(mode, p, i18n, is_rtl, ui_font, ui_font_bold),
    };

    let mut footer_items: Vec<Element<'a, DashboardMsg>> = Vec::new();
    if state.step > 0 {
        footer_items.push(
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(
                            if is_rtl {
                                UiIcon::ChevronRight
                            } else {
                                UiIcon::ChevronLeft
                            },
                            16.0,
                            p.text_primary,
                        )
                        .into(),
                        text(i18n.t("onboarding.back_btn"))
                            .size(13)
                            .font(ui_font_semi)
                            .into(),
                    ],
                )
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::OnboardingStep(state.step - 1))
            .padding([9, 16])
            .style(pill_button_style(mode))
            .into(),
        );
    }

    footer_items.push(Space::new().width(Length::Fill).into());

    if state.step < 2 {
        footer_items.push(
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        text(i18n.t("onboarding.next_btn"))
                            .size(13)
                            .font(ui_font_semi)
                            .color(Color::WHITE)
                            .into(),
                        ui_icon(
                            if is_rtl {
                                UiIcon::ChevronLeft
                            } else {
                                UiIcon::ChevronRight
                            },
                            16.0,
                            Color::WHITE,
                        )
                        .into(),
                    ],
                )
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::OnboardingStep(state.step + 1))
            .padding([9, 20])
            .style(primary_button_style(mode))
            .into(),
        );
    } else {
        footer_items.push(
            button(
                text(i18n.t("onboarding.open_models_btn"))
                    .size(13)
                    .font(ui_font_semi),
            )
            .on_press(DashboardMsg::OnboardingFinish(true))
            .padding([9, 16])
            .style(pill_button_style(mode))
            .into(),
        );
        footer_items.push(
            button(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::RocketLaunch, 15.0, Color::WHITE).into(),
                        text(i18n.t("onboarding.finish_btn"))
                            .size(13)
                            .font(ui_font_semi)
                            .color(Color::WHITE)
                            .into(),
                    ],
                )
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(DashboardMsg::OnboardingFinish(false))
            .padding([9, 20])
            .style(primary_button_style(mode))
            .into(),
        );
    }

    let footer = rtl_row(is_rtl, footer_items)
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let card = container(
        column![
            top_bar,
            Space::new().height(4),
            hero,
            hairline_divider(mode),
            sleek_scrollable(
                container(step_content)
                    .padding(Padding::from([4, 4]))
                    .width(Length::Fill),
                mode,
            )
            .height(Length::Fill),
            hairline_divider(mode),
            footer,
        ]
        .spacing(14)
        .height(Length::Fill),
    )
    .padding(28)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(card_style(mode, 14.0));

    container(card)
        .padding(Padding::from([20, 44]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view_step_capabilities<'a>(
    mode: langhian_core::UiThemeMode,
    p: ThemePalette,
    i18n: &'a Translator,
    is_rtl: bool,
    ui_font: iced::Font,
    ui_font_semi: iced::Font,
    ui_font_bold: iced::Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    let cap_card = |icon: UiIcon,
                    shortcut_a: &'static str,
                    shortcut_b: &'static str,
                    title: String,
                    desc: String|
     -> Element<'a, DashboardMsg> {
        let header = rtl_row(
            is_rtl,
            vec![
                container(ui_icon(icon, 18.0, p.accent_text))
                    .padding(8)
                    .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle))
                    .into(),
                aligned_text(title, 14.0, ui_font_bold, Some(p.text_primary), is_rtl),
                row![
                    keycap(shortcut_a, mode, ui_font_semi),
                    text("+").size(11).color(p.text_muted),
                    keycap(shortcut_b, mode, ui_font_semi),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
            ],
        )
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        container(
            column![
                header,
                aligned_text(desc, 12.5, ui_font, Some(p.text_secondary), is_rtl),
            ]
            .spacing(8)
            .align_x(h_align),
        )
        .padding(16)
        .width(Length::Fill)
        .style(elevated_box_style(mode, 10.0))
        .into()
    };

    column![
        aligned_text(
            i18n.t("onboarding.step1_title"),
            15.0,
            ui_font_bold,
            Some(p.text_primary),
            is_rtl,
        ),
        cap_card(
            UiIcon::Bolt,
            "Alt",
            "Alt",
            i18n.t("onboarding.cap_alt_title"),
            i18n.t("onboarding.cap_alt_desc"),
        ),
        cap_card(
            UiIcon::StarShine,
            "Ctrl",
            "Ctrl",
            i18n.t("onboarding.cap_ctrl_title"),
            i18n.t("onboarding.cap_ctrl_desc"),
        ),
        cap_card(
            UiIcon::CropFree,
            "Ctrl+Shift",
            "X",
            i18n.t("onboarding.cap_snip_title"),
            i18n.t("onboarding.cap_snip_desc"),
        ),
    ]
    .spacing(12)
    .align_x(h_align)
    .width(Length::Fill)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn view_step_languages<'a>(
    config: &'a AppConfig,
    mode: langhian_core::UiThemeMode,
    p: ThemePalette,
    i18n: &'a Translator,
    is_rtl: bool,
    ui_font: iced::Font,
    ui_font_semi: iced::Font,
    ui_font_bold: iced::Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

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

    let native_card = container(
        column![
            rtl_row(
                is_rtl,
                vec![
                    container(ui_icon(UiIcon::Language, 16.0, p.success))
                        .padding(6)
                        .style(badge_style(p.success_soft, p.success, p.border_subtle))
                        .into(),
                    aligned_text(
                        i18n.t("settings.native_language"),
                        14.0,
                        ui_font_bold,
                        Some(p.text_primary),
                        is_rtl,
                    ),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill),
            aligned_text(
                i18n.t("onboarding.native_explain"),
                12.5,
                ui_font,
                Some(p.text_secondary),
                is_rtl,
            ),
            Space::new().height(2),
            pick_list(langs.clone(), sel_native, |x| {
                DashboardMsg::SelectNativeLang(x.id)
            })
            .font(ui_font)
            .text_size(13)
            .padding([8, 12])
            .width(Length::Fill)
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode)),
        ]
        .spacing(10)
        .align_x(h_align),
    )
    .padding(16)
    .width(Length::FillPortion(1))
    .style(elevated_box_style(mode, 10.0));

    let target_card = container(
        column![
            rtl_row(
                is_rtl,
                vec![
                    container(ui_icon(UiIcon::SwapHoriz, 16.0, p.accent_text))
                        .padding(6)
                        .style(badge_style(p.accent_soft, p.accent_text, p.border_subtle))
                        .into(),
                    aligned_text(
                        i18n.t("settings.target_language"),
                        14.0,
                        ui_font_bold,
                        Some(p.text_primary),
                        is_rtl,
                    ),
                ],
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill),
            aligned_text(
                i18n.t("onboarding.target_explain"),
                12.5,
                ui_font,
                Some(p.text_secondary),
                is_rtl,
            ),
            Space::new().height(2),
            pick_list(langs, sel_target, |x| {
                DashboardMsg::SelectTargetLang(x.id)
            })
            .font(ui_font)
            .text_size(13)
            .padding([8, 12])
            .width(Length::Fill)
            .style(sleek_pick_list_style(mode))
            .menu_style(sleek_menu_style(mode)),
        ]
        .spacing(10)
        .align_x(h_align),
    )
    .padding(16)
    .width(Length::FillPortion(1))
    .style(elevated_box_style(mode, 10.0));

    let native_name = language_display_name(&config.native_language);
    let target_name = language_display_name(&config.target_language);

    let preview_box = container(
        column![
            aligned_text(
                i18n.t("onboarding.preview_title"),
                12.0,
                ui_font_semi,
                Some(p.accent_text),
                is_rtl,
            ),
            aligned_text(
                format!("• Alt+Alt (Input):  Any -> {target_name}"),
                12.5,
                ui_font,
                Some(p.text_primary),
                is_rtl,
            ),
            aligned_text(
                format!("• Ctrl+Ctrl (Modal):  {target_name} -> {native_name}"),
                12.5,
                ui_font,
                Some(p.text_primary),
                is_rtl,
            ),
            aligned_text(
                format!(
                    "• Ctrl+Ctrl (Modal, >=70% {native_name}):  {native_name} -> {target_name}"
                ),
                12.5,
                ui_font,
                Some(p.text_primary),
                is_rtl,
            ),
        ]
        .spacing(6)
        .align_x(h_align),
    )
    .padding(14)
    .width(Length::Fill)
    .style(card_style(mode, 10.0));

    column![
        aligned_text(
            i18n.t("onboarding.step2_title"),
            15.0,
            ui_font_bold,
            Some(p.text_primary),
            is_rtl,
        ),
        rtl_row(is_rtl, vec![native_card.into(), target_card.into()]).spacing(12),
        preview_box,
    ]
    .spacing(14)
    .align_x(h_align)
    .width(Length::Fill)
    .into()
}

fn view_step_ready<'a>(
    mode: langhian_core::UiThemeMode,
    p: ThemePalette,
    i18n: &'a Translator,
    is_rtl: bool,
    ui_font: iced::Font,
    ui_font_bold: iced::Font,
) -> Element<'a, DashboardMsg> {
    let h_align = horizontal_for_dir(is_rtl);

    container(
        column![
            Space::new().height(12),
            container(ui_icon(UiIcon::CheckCircle, 34.0, p.success))
                .padding(14)
                .style(badge_style(p.success_soft, p.success, p.border_subtle)),
            aligned_centered_text(
                i18n.t("onboarding.step3_title"),
                18.0,
                ui_font_bold,
                Some(p.text_primary),
            ),
            aligned_centered_text(
                i18n.t("onboarding.step3_desc"),
                13.0,
                ui_font,
                Some(p.text_secondary),
            ),
            Space::new().height(6),
            container(
                rtl_row(
                    is_rtl,
                    vec![
                        ui_icon(UiIcon::Info, 16.0, p.accent_text).into(),
                        aligned_text(
                            i18n.t("settings.auto_save_hint"),
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
            .style(elevated_box_style(mode, 8.0)),
            Space::new().height(12),
        ]
        .spacing(12)
        .align_x(Horizontal::Center)
        .width(Length::Fill),
    )
    .padding(18)
    .width(Length::Fill)
    .align_x(h_align)
    .into()
}
