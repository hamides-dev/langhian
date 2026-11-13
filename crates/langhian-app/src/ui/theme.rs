use iced::font::{Family, Weight};
use iced::theme::{Palette, Theme};
use iced::widget::{
    button, container, pick_list, rich_text, span, text, text_editor, text_input, toggler,
};
use iced::{
    Background, Border, Color, Element, Font, Length, Padding, Shadow, Vector, overlay::menu,
};
use langhian_core::{
    TextDirection, TextSegment, UiThemeMode, VocabEntry, VocabMatcher, contains_rtl_script,
};
use std::borrow::Cow;

pub const APP_ICON_PNG: &[u8] = include_bytes!("../../../../assets/icons/langhian.png");

// IMPORTANT: Intentionally avoid loading *-Medium.ttf or requesting Weight::Medium.
// We restrict to Normal (400), Semibold (600), and Bold (700) loaded synchronously at daemon startup
// so all Latin/ASCII/Unicode glyphs render crisply with zero atlas corruption on Windows/macOS/Linux.
pub const FONT_INTER_REGULAR_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
pub const FONT_INTER_SEMIBOLD_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");
pub const FONT_INTER_BOLD_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Bold.ttf");
pub const FONT_VAZIR_REGULAR_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Vazirmatn-Regular.ttf");
pub const FONT_VAZIR_BOLD_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Vazirmatn-Bold.ttf");

pub fn font_inter(weight: Weight) -> Font {
    let safe_weight = match weight {
        Weight::Bold | Weight::ExtraBold | Weight::Black => Weight::Bold,
        Weight::Semibold | Weight::Medium => Weight::Semibold,
        _ => Weight::Normal,
    };
    Font {
        family: Family::Name("Inter"),
        weight: safe_weight,
        ..Font::DEFAULT
    }
}

pub fn font_vazirmatn(weight: Weight) -> Font {
    let safe_weight = match weight {
        Weight::Bold | Weight::ExtraBold | Weight::Black | Weight::Semibold | Weight::Medium => {
            Weight::Bold
        }
        _ => Weight::Normal,
    };
    Font {
        family: Family::Name("Vazirmatn"),
        weight: safe_weight,
        ..Font::DEFAULT
    }
}

pub fn font_for_text(text_val: &str, lang_hint: Option<&str>, weight: Weight) -> Font {
    let is_rtl_hint = matches!(lang_hint, Some("fa" | "ar" | "he" | "ur" | "ps"));
    if is_rtl_hint || contains_rtl_script(text_val) {
        font_vazirmatn(weight)
    } else {
        font_inter(weight)
    }
}

pub fn font_for_ui(dir: TextDirection, weight: Weight) -> Font {
    match dir {
        TextDirection::Rtl => font_vazirmatn(weight),
        TextDirection::Ltr => font_inter(weight),
    }
}

pub fn font_ui_default() -> Font {
    font_inter(Weight::Normal)
}

pub fn bundled_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(FONT_INTER_REGULAR_BYTES),
        Cow::Borrowed(FONT_INTER_SEMIBOLD_BYTES),
        Cow::Borrowed(FONT_INTER_BOLD_BYTES),
        Cow::Borrowed(FONT_VAZIR_REGULAR_BYTES),
        Cow::Borrowed(FONT_VAZIR_BOLD_BYTES),
    ]
}

#[derive(Debug, Clone, Copy)]
pub struct AntigravityColors {
    pub bg_canvas: Color,
    pub bg_sidebar: Color,
    pub bg_card: Color,
    pub bg_elevated: Color,
    pub bg_input: Color,
    pub bg_active_nav: Color,
    pub bg_selected_card: Color,
    pub border_subtle: Color,
    pub border_control: Color,
    pub border_selected: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_soft: Color,
    pub accent_text: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub highlight_bg: Color,
    pub highlight_text: Color,
}

pub fn palette_for(mode: UiThemeMode) -> AntigravityColors {
    match mode {
        UiThemeMode::Dark => AntigravityColors {
            bg_canvas: Color::from_rgb8(0x11, 0x11, 0x13),
            bg_sidebar: Color::from_rgb8(0x18, 0x18, 0x1A),
            bg_card: Color::from_rgb8(0x16, 0x16, 0x18),
            bg_elevated: Color::from_rgb8(0x22, 0x22, 0x26),
            bg_input: Color::from_rgb8(0x12, 0x12, 0x14),
            bg_active_nav: Color::from_rgb8(0x27, 0x27, 0x2B),
            bg_selected_card: Color::from_rgb8(0x15, 0x1A, 0x26),
            border_subtle: Color::from_rgb8(0x23, 0x23, 0x27),
            border_control: Color::from_rgb8(0x32, 0x32, 0x38),
            border_selected: Color::from_rgb8(0x2D, 0x4A, 0x7C),
            text_primary: Color::from_rgb8(0xF4, 0xF4, 0xF5),
            text_secondary: Color::from_rgb8(0x9E, 0x9E, 0xA7),
            text_muted: Color::from_rgb8(0x6E, 0x6E, 0x78),
            accent: Color::from_rgb8(0x3B, 0x82, 0xF6),
            accent_hover: Color::from_rgb8(0x25, 0x63, 0xEB),
            accent_soft: Color::from_rgba8(0x3B, 0x82, 0xF6, 0.14),
            accent_text: Color::from_rgb8(0x93, 0xC5, 0xFD),
            success: Color::from_rgb8(0x34, 0xD3, 0x99),
            warning: Color::from_rgb8(0xFB, 0xBF, 0x24),
            danger: Color::from_rgb8(0xF8, 0x71, 0x71),
            highlight_bg: Color::from_rgba8(0xF5, 0x9E, 0x0B, 0.20),
            highlight_text: Color::from_rgb8(0xFD, 0xE6, 0x8A),
        },
        UiThemeMode::Light => AntigravityColors {
            bg_canvas: Color::from_rgb8(0xFA, 0xFA, 0xFB),
            bg_sidebar: Color::from_rgb8(0xF3, 0xF4, 0xF6),
            bg_card: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            bg_elevated: Color::from_rgb8(0xF4, 0xF4, 0xF6),
            bg_input: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            bg_active_nav: Color::from_rgb8(0xE4, 0xE4, 0xE7),
            bg_selected_card: Color::from_rgb8(0xEF, 0xF6, 0xFF),
            border_subtle: Color::from_rgb8(0xE4, 0xE4, 0xE7),
            border_control: Color::from_rgb8(0xD4, 0xD4, 0xD8),
            border_selected: Color::from_rgb8(0x3B, 0x82, 0xF6),
            text_primary: Color::from_rgb8(0x11, 0x18, 0x27),
            text_secondary: Color::from_rgb8(0x52, 0x52, 0x5B),
            text_muted: Color::from_rgb8(0x71, 0x71, 0x7A),
            accent: Color::from_rgb8(0x25, 0x63, 0xEB),
            accent_hover: Color::from_rgb8(0x1D, 0x4E, 0xD8),
            accent_soft: Color::from_rgba8(0x25, 0x63, 0xEB, 0.10),
            accent_text: Color::from_rgb8(0x1D, 0x4E, 0xD8),
            success: Color::from_rgb8(0x05, 0x96, 0x69),
            warning: Color::from_rgb8(0xD9, 0x77, 0x06),
            danger: Color::from_rgb8(0xDC, 0x26, 0x26),
            highlight_bg: Color::from_rgba8(0xFA, 0xCC, 0x15, 0.30),
            highlight_text: Color::from_rgb8(0x78, 0x35, 0x0F),
        },
    }
}

pub fn iced_theme_for(mode: UiThemeMode) -> Theme {
    let p = palette_for(mode);
    Theme::custom(
        match mode {
            UiThemeMode::Dark => "Antigravity Dark".to_string(),
            UiThemeMode::Light => "Antigravity Light".to_string(),
        },
        Palette {
            background: p.bg_canvas,
            text: p.text_primary,
            primary: p.accent,
            success: p.success,
            warning: p.warning,
            danger: p.danger,
        },
    )
}

pub fn transparent_iced_theme_for(mode: UiThemeMode) -> Theme {
    let p = palette_for(mode);
    Theme::custom_with_fn(
        match mode {
            UiThemeMode::Dark => "Antigravity Transparent Dark".to_string(),
            UiThemeMode::Light => "Antigravity Transparent Light".to_string(),
        },
        Palette {
            background: p.bg_canvas,
            text: p.text_primary,
            primary: p.accent,
            success: p.success,
            warning: p.warning,
            danger: p.danger,
        },
        |palette| {
            let mut ext = iced::theme::palette::Extended::generate(palette);
            ext.background.base.color = Color::TRANSPARENT;
            ext
        },
    )
}

pub fn card_style(mode: UiThemeMode, radius: f32) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_card)),
        border: Border {
            color: p.border_subtle,
            width: 1.0,
            radius: radius.into(),
        },
        text_color: Some(p.text_primary),
        ..Default::default()
    }
}

pub fn selectable_card_style(
    mode: UiThemeMode,
    selected: bool,
    radius: f32,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = if selected {
            p.bg_selected_card
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            p.bg_elevated
        } else {
            p.bg_card
        };
        let border_col = if selected {
            p.border_selected
        } else if matches!(status, button::Status::Hovered) {
            p.border_control
        } else {
            p.border_subtle
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: p.text_primary,
            border: Border {
                color: border_col,
                width: 1.0,
                radius: radius.into(),
            },
            ..Default::default()
        }
    }
}

pub fn elevated_box_style(mode: UiThemeMode, radius: f32) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_elevated)),
        border: Border {
            color: p.border_subtle,
            width: 1.0,
            radius: radius.into(),
        },
        text_color: Some(p.text_primary),
        ..Default::default()
    }
}

pub fn modal_surface_style(mode: UiThemeMode, radius: f32) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_sidebar)),
        border: Border {
            color: p.border_control,
            width: 1.0,
            radius: radius.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 20.0,
        },
        text_color: Some(p.text_primary),
        ..Default::default()
    }
}

pub fn sidebar_style(mode: UiThemeMode) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_sidebar)),
        border: Border {
            color: p.border_subtle,
            width: 0.0,
            radius: 0.0.into(),
        },
        text_color: Some(p.text_primary),
        ..Default::default()
    }
}

pub fn hairline_divider<'a, M: 'a>(mode: UiThemeMode) -> Element<'a, M> {
    let p = palette_for(mode);
    container(iced::widget::Space::new().width(Length::Fill).height(1))
        .style(move |_| container::Style {
            background: Some(Background::Color(p.border_subtle)),
            ..Default::default()
        })
        .into()
}

pub fn badge_style(bg: Color, fg: Color, border_col: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        border: Border {
            color: border_col,
            width: 1.0,
            radius: 999.0.into(),
        },
        ..Default::default()
    }
}

pub fn keycap_style(mode: UiThemeMode) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_elevated)),
        text_color: Some(p.text_primary),
        border: Border {
            color: p.border_control,
            width: 1.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

pub fn keycap<'a, M: 'a>(mode: UiThemeMode, label: &'a str, font: Font) -> Element<'a, M> {
    let p = palette_for(mode);
    container(text(label).size(11).font(font).color(p.text_primary))
        .padding([2, 7])
        .style(keycap_style(mode))
        .into()
}

pub fn nav_button_style(
    mode: UiThemeMode,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = if active {
            Some(Background::Color(p.bg_active_nav))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.04)))
        } else {
            None
        };
        let text_color = if active || matches!(status, button::Status::Hovered) {
            p.text_primary
        } else {
            p.text_secondary
        };
        button::Style {
            background: bg,
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn primary_button_style(mode: UiThemeMode) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => p.accent_hover,
            button::Status::Disabled => Color::from_rgba(0.35, 0.45, 0.65, 0.35),
            _ => p.accent,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: Color::WHITE,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn pill_button_style(mode: UiThemeMode) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => p.bg_active_nav,
            _ => p.bg_elevated,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: p.text_primary,
            border: Border {
                color: p.border_control,
                width: 1.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn ghost_button_style(
    mode: UiThemeMode,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if active {
            Some(Background::Color(p.accent_soft))
        } else if hovered {
            Some(Background::Color(p.bg_elevated))
        } else {
            Some(Background::Color(Color::TRANSPARENT))
        };
        let border_col = if active {
            p.border_selected
        } else if hovered {
            p.border_control
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: bg,
            text_color: if active {
                p.accent_text
            } else {
                p.text_primary
            },
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn danger_button_style(mode: UiThemeMode) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => {
                Color::from_rgba(0.97, 0.44, 0.44, 0.16)
            }
            _ => Color::from_rgba(0.97, 0.44, 0.44, 0.08),
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: p.danger,
            border: Border {
                color: Color::from_rgba(0.97, 0.44, 0.44, 0.28),
                width: 1.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn sleek_input_style(
    mode: UiThemeMode,
) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    let p = palette_for(mode);
    move |_, status| {
        let border_col = match status {
            text_input::Status::Focused { .. } => p.accent,
            text_input::Status::Hovered => p.border_control,
            _ => p.border_subtle,
        };
        text_input::Style {
            background: Background::Color(p.bg_input),
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 7.0.into(),
            },
            icon: p.text_secondary,
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: p.accent_soft,
        }
    }
}

pub fn sleek_editor_style(
    mode: UiThemeMode,
) -> impl Fn(&Theme, text_editor::Status) -> text_editor::Style {
    let p = palette_for(mode);
    move |_, status| {
        let border_col = match status {
            text_editor::Status::Focused { .. } => p.accent,
            text_editor::Status::Hovered => p.border_control,
            _ => p.border_subtle,
        };
        text_editor::Style {
            background: Background::Color(p.bg_input),
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 8.0.into(),
            },
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: p.accent_soft,
        }
    }
}

pub fn sleek_pick_list_style(
    mode: UiThemeMode,
) -> impl Fn(&Theme, pick_list::Status) -> pick_list::Style {
    let p = palette_for(mode);
    move |_, status| {
        let border_col = match status {
            pick_list::Status::Opened { .. } => p.accent,
            pick_list::Status::Hovered => Color::from_rgba(
                p.border_control.r,
                p.border_control.g,
                p.border_control.b,
                1.0,
            ),
            pick_list::Status::Active => p.border_control,
        };
        pick_list::Style {
            text_color: p.text_primary,
            placeholder_color: p.text_muted,
            handle_color: p.text_secondary,
            background: Background::Color(p.bg_elevated),
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 7.0.into(),
            },
        }
    }
}

pub fn sleek_menu_style(mode: UiThemeMode) -> impl Fn(&Theme) -> menu::Style {
    let p = palette_for(mode);
    move |_| menu::Style {
        background: Background::Color(p.bg_sidebar),
        border: Border {
            color: p.border_control,
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: p.text_primary,
        selected_text_color: Color::WHITE,
        selected_background: Background::Color(p.accent),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.40),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        },
    }
}

pub fn antigravity_toggler_style(
    mode: UiThemeMode,
) -> impl Fn(&Theme, toggler::Status) -> toggler::Style {
    let p = palette_for(mode);
    move |_, status| {
        let is_on = match status {
            toggler::Status::Active { is_toggled }
            | toggler::Status::Hovered { is_toggled }
            | toggler::Status::Disabled { is_toggled } => is_toggled,
        };
        let bg = if is_on { p.accent } else { p.bg_active_nav };
        let fg = if is_on {
            Color::WHITE
        } else {
            p.text_secondary
        };
        toggler::Style {
            background: Background::Color(bg),
            background_border_width: 1.0,
            background_border_color: if is_on { p.accent } else { p.border_control },
            foreground: Background::Color(fg),
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
            text_color: Some(p.text_primary),
            border_radius: None,
            padding_ratio: 0.16,
        }
    }
}

pub fn antigravity_row<'a, M: 'a>(
    mode: UiThemeMode,
    title: impl Into<String>,
    description: Option<String>,
    control: Element<'a, M>,
    ui_font: Font,
    ui_font_semi: Font,
) -> Element<'a, M> {
    let p = palette_for(mode);
    let title_str = title.into();
    let mut left_col = iced::widget::column![
        text(title_str)
            .size(13)
            .font(ui_font_semi)
            .color(p.text_primary)
    ]
    .spacing(3);
    if let Some(desc) = description
        && !desc.is_empty()
    {
        left_col = left_col.push(text(desc).size(12).font(ui_font).color(p.text_secondary));
    }
    container(
        iced::widget::row![
            left_col.width(Length::Fill),
            container(control).align_y(iced::Alignment::Center),
        ]
        .spacing(16)
        .align_y(iced::Alignment::Center),
    )
    .padding(Padding::from([14, 16]))
    .width(Length::Fill)
    .into()
}

pub fn render_highlighted_text<'a, M: Clone + 'a>(
    full_text: &str,
    lang_hint: Option<&str>,
    highlighter: &VocabMatcher,
    on_click_vocab: impl Fn(VocabEntry) -> M + 'a,
    mode: UiThemeMode,
    size: f32,
) -> Element<'a, M> {
    let p = palette_for(mode);
    let spans = highlighter.highlight_segments(full_text);
    let has_highlights = spans.iter().any(|s| matches!(s, TextSegment::Saved { .. }));
    let text_font = font_for_text(full_text, lang_hint, Weight::Normal);
    let bold_font = font_for_text(full_text, lang_hint, Weight::Semibold);
    let is_rtl = matches!(lang_hint, Some("fa" | "ar" | "he" | "ur" | "ps"))
        || contains_rtl_script(full_text);

    if !has_highlights {
        let mut t = text(full_text.to_string())
            .size(size)
            .font(text_font)
            .color(p.text_primary);
        if is_rtl {
            t = t.align_x(iced::alignment::Horizontal::Right);
        }
        return container(t).width(Length::Fill).into();
    }

    let mut rt_spans: Vec<iced::widget::text::Span<'a, VocabEntry, Font>> =
        Vec::with_capacity(spans.len());
    for seg in spans {
        match seg {
            TextSegment::Saved {
                text: seg_text,
                entry,
            } => {
                rt_spans.push(
                    span(seg_text)
                        .font(bold_font)
                        .size(size)
                        .color(p.highlight_text)
                        .background(p.highlight_bg)
                        .padding([1, 3])
                        .underline(true)
                        .link(entry),
                );
            }
            TextSegment::Plain(seg_text) => {
                rt_spans.push(
                    span(seg_text)
                        .font(text_font)
                        .size(size)
                        .color(p.text_primary),
                );
            }
        }
    }

    let mut widget = rich_text(rt_spans)
        .on_link_click(on_click_vocab)
        .width(Length::Fill);
    if is_rtl {
        widget = widget.align_x(iced::alignment::Horizontal::Right);
    }
    widget.into()
}
