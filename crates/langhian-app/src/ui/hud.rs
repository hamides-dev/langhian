use crate::ui::theme::{font_for_text, palette_for};
use iced::font::Weight;
use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Vector};
use langhian_core::UiThemeMode;

#[derive(Debug, Clone)]
pub struct HudState {
    pub title: String,
    pub detail: String,
    pub is_error: bool,
}

pub fn view_hud<'a, M: 'a>(hud: &'a HudState, mode: UiThemeMode) -> Element<'a, M> {
    let p = palette_for(mode);
    let accent = if hud.is_error { p.danger } else { p.accent };

    let dot =
        container(iced::widget::Space::new().width(8).height(8)).style(move |_| container::Style {
            background: Some(Background::Color(accent)),
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let title_font = font_for_text(&hud.title, None, Weight::Semibold);
    let detail_font = font_for_text(&hud.detail, None, Weight::Normal);

    let mut content = row![
        dot,
        text(&hud.title)
            .size(13)
            .font(title_font)
            .color(p.text_primary),
    ]
    .spacing(9)
    .align_y(Alignment::Center);

    if !hud.detail.is_empty() {
        content = content.push(text("·").size(13).color(p.text_muted)).push(
            text(&hud.detail)
                .size(12)
                .font(detail_font)
                .color(p.text_secondary),
        );
    }

    let bg = match mode {
        UiThemeMode::Dark => Color::from_rgba8(0x18, 0x18, 0x1B, 0.94),
        UiThemeMode::Light => Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.96),
    };
    let border_col = if hud.is_error {
        Color::from_rgba(p.danger.r, p.danger.g, p.danger.b, 0.55)
    } else {
        p.border_control
    };

    let pill = container(content)
        .padding([10, 16])
        .width(Length::Shrink)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 999.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 12.0,
            },
            text_color: Some(p.text_primary),
            ..Default::default()
        });

    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding([4, 8])
        .into()
}
