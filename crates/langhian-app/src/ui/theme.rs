use iced::font::{Family, Weight};
use iced::widget::{button, container, rich_text, span, text, text_input};
use iced::{Background, Border, Color, Element, Font, Shadow, Theme, Vector, alignment};
use langhian_core::{TextDirection, TextSegment, UiThemeMode, VocabEntry, detect_direction};
use std::borrow::Cow;

pub const FONT_INTER_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
pub const FONT_INTER_MEDIUM_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Inter-Medium.ttf");
pub const FONT_INTER_SEMIBOLD_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");
pub const FONT_INTER_BOLD_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Bold.ttf");

pub const FONT_VAZIR_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Vazirmatn-Regular.ttf");
pub const FONT_VAZIR_MEDIUM_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Vazirmatn-Medium.ttf");
pub const FONT_VAZIR_BOLD_BYTES: &[u8] =
    include_bytes!("../../../../assets/fonts/Vazirmatn-Bold.ttf");

pub const APP_ICON_PNG: &[u8] = include_bytes!("../../../../assets/icons/langhian.png");

pub fn font_inter(weight: Weight) -> Font {
    Font {
        family: Family::Name("Inter"),
        weight,
        ..Font::DEFAULT
    }
}

pub fn font_vazirmatn(weight: Weight) -> Font {
    Font {
        family: Family::Name("Vazirmatn"),
        weight,
        ..Font::DEFAULT
    }
}

pub fn font_for_text(text_val: &str, lang_hint: Option<&str>, weight: Weight) -> Font {
    if detect_direction(text_val, lang_hint).is_rtl() {
        font_vazirmatn(weight)
    } else {
        font_inter(weight)
    }
}

pub fn font_for_ui(dir: TextDirection, weight: Weight) -> Font {
    if dir.is_rtl() {
        font_vazirmatn(weight)
    } else {
        font_inter(weight)
    }
}

pub fn bundled_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(FONT_INTER_BYTES),
        Cow::Borrowed(FONT_INTER_MEDIUM_BYTES),
        Cow::Borrowed(FONT_INTER_SEMIBOLD_BYTES),
        Cow::Borrowed(FONT_INTER_BOLD_BYTES),
        Cow::Borrowed(FONT_VAZIR_BYTES),
        Cow::Borrowed(FONT_VAZIR_MEDIUM_BYTES),
        Cow::Borrowed(FONT_VAZIR_BOLD_BYTES),
    ]
}

#[derive(Debug, Clone, Copy)]
pub struct AntigravityColors {
    pub bg_canvas: Color,
    pub bg_sidebar: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub border_subtle: Color,
    pub border_strong: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_text: Color,
    pub success: Color,
    pub warning: Color,
    pub warning_soft: Color,
    pub danger: Color,
    pub danger_soft: Color,
}

pub fn palette_for(mode: UiThemeMode) -> AntigravityColors {
    match mode {
        UiThemeMode::Dark => AntigravityColors {
            bg_canvas: Color::from_rgb8(11, 13, 18),
            bg_sidebar: Color::from_rgb8(14, 17, 24),
            bg_surface: Color::from_rgb8(19, 23, 34),
            bg_elevated: Color::from_rgb8(26, 32, 47),
            bg_hover: Color::from_rgb8(34, 42, 62),
            border_subtle: Color::from_rgba8(255, 255, 255, 0.08),
            border_strong: Color::from_rgba8(108, 160, 255, 0.42),
            text_primary: Color::from_rgb8(242, 245, 250),
            text_secondary: Color::from_rgb8(156, 166, 188),
            text_muted: Color::from_rgb8(105, 115, 138),
            accent: Color::from_rgb8(82, 139, 255),
            accent_soft: Color::from_rgba8(82, 139, 255, 0.16),
            accent_text: Color::from_rgb8(138, 180, 255),
            success: Color::from_rgb8(52, 211, 153),
            warning: Color::from_rgb8(246, 193, 92),
            warning_soft: Color::from_rgba8(246, 193, 92, 0.20),
            danger: Color::from_rgb8(248, 113, 113),
            danger_soft: Color::from_rgba8(248, 113, 113, 0.14),
        },
        UiThemeMode::Light => AntigravityColors {
            bg_canvas: Color::from_rgb8(246, 248, 252),
            bg_sidebar: Color::from_rgb8(239, 242, 248),
            bg_surface: Color::from_rgb8(255, 255, 255),
            bg_elevated: Color::from_rgb8(244, 247, 253),
            bg_hover: Color::from_rgb8(231, 237, 249),
            border_subtle: Color::from_rgba8(15, 23, 42, 0.09),
            border_strong: Color::from_rgba8(59, 118, 246, 0.45),
            text_primary: Color::from_rgb8(17, 24, 39),
            text_secondary: Color::from_rgb8(75, 85, 105),
            text_muted: Color::from_rgb8(122, 134, 154),
            accent: Color::from_rgb8(49, 108, 244),
            accent_soft: Color::from_rgba8(49, 108, 244, 0.11),
            accent_text: Color::from_rgb8(37, 92, 224),
            success: Color::from_rgb8(16, 163, 111),
            warning: Color::from_rgb8(180, 112, 14),
            warning_soft: Color::from_rgba8(245, 158, 11, 0.18),
            danger: Color::from_rgb8(220, 38, 38),
            danger_soft: Color::from_rgba8(220, 38, 38, 0.10),
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
        iced::theme::Palette {
            background: p.bg_canvas,
            text: p.text_primary,
            primary: p.accent,
            success: p.success,
            warning: p.warning,
            danger: p.danger,
        },
    )
}

pub fn card_style(mode: UiThemeMode, radius: f32) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_surface)),
        border: Border {
            color: p.border_subtle,
            width: 1.0,
            radius: radius.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.28),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        text_color: Some(p.text_primary),
        ..Default::default()
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

pub fn sidebar_style(mode: UiThemeMode) -> impl Fn(&Theme) -> container::Style {
    let p = palette_for(mode);
    move |_| container::Style {
        background: Some(Background::Color(p.bg_sidebar)),
        border: Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 0.0.into(),
        },
        text_color: Some(p.text_primary),
        ..Default::default()
    }
}

pub fn badge_style(bg: Color, fg: Color, border_col: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border_col,
            width: 1.0,
            radius: 999.0.into(),
        },
        text_color: Some(fg),
        ..Default::default()
    }
}

pub fn primary_button_style(mode: UiThemeMode) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = match status {
            button::Status::Hovered => Color::from_rgba(p.accent.r, p.accent.g, p.accent.b, 0.88),
            button::Status::Pressed => Color::from_rgba(p.accent.r, p.accent.g, p.accent.b, 0.76),
            button::Status::Disabled => p.bg_elevated,
            _ => p.accent,
        };
        let text_color = if matches!(status, button::Status::Disabled) {
            p.text_muted
        } else {
            Color::WHITE
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
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
        let bg = if active {
            p.accent_soft
        } else {
            match status {
                button::Status::Hovered => p.bg_hover,
                button::Status::Pressed => p.bg_elevated,
                _ => p.bg_elevated,
            }
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if active {
                p.accent_text
            } else {
                p.text_primary
            },
            border: Border {
                color: if active {
                    p.border_strong
                } else {
                    p.border_subtle
                },
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn danger_button_style(mode: UiThemeMode) -> impl Fn(&Theme, button::Status) -> button::Style {
    let p = palette_for(mode);
    move |_, status| {
        let bg = match status {
            button::Status::Hovered => Color::from_rgba(p.danger.r, p.danger.g, p.danger.b, 0.24),
            _ => p.danger_soft,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: p.danger,
            border: Border {
                color: Color::from_rgba(p.danger.r, p.danger.g, p.danger.b, 0.35),
                width: 1.0,
                radius: 8.0.into(),
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
        let border_color = match status {
            text_input::Status::Focused { .. } => p.accent,
            text_input::Status::Hovered => p.border_strong,
            _ => p.border_subtle,
        };
        text_input::Style {
            background: Background::Color(p.bg_elevated),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 8.0.into(),
            },
            icon: p.text_muted,
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: p.accent_soft,
        }
    }
}

pub fn render_highlighted_text<'a, M: Clone + 'a>(
    segments: Vec<TextSegment>,
    full_text: &str,
    lang_hint: Option<&str>,
    mode: UiThemeMode,
    size: f32,
    on_click_vocab: impl Fn(VocabEntry) -> M + 'a,
) -> Element<'a, M> {
    let p = palette_for(mode);
    let dir = detect_direction(full_text, lang_hint);
    let font = font_for_text(full_text, lang_hint, Weight::Normal);
    let bold_font = font_for_text(full_text, lang_hint, Weight::Semibold);
    let align_x = if dir.is_rtl() {
        alignment::Horizontal::Right
    } else {
        alignment::Horizontal::Left
    };

    if segments.is_empty() {
        return text("")
            .size(size)
            .font(font)
            .shaping(text::Shaping::Advanced)
            .align_x(align_x)
            .into();
    }

    let spans: Vec<iced::widget::text::Span<'a, VocabEntry, Font>> = segments
        .into_iter()
        .map(|seg| match seg {
            TextSegment::Plain(s) => span(s).color(p.text_primary).font(font),
            TextSegment::Saved {
                text: matched,
                entry,
            } => span(matched)
                .color(p.warning)
                .background(p.warning_soft)
                .underline(true)
                .font(bold_font)
                .link(entry),
        })
        .collect();

    rich_text(spans)
        .size(size)
        .align_x(align_x)
        .on_link_click(on_click_vocab)
        .into()
}
