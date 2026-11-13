use crate::ui::theme::{font_for_text, palette_for};
use iced::font::Weight;
use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Vector};
use langhian_core::UiThemeMode;

#[derive(Debug, Clone)]
pub struct HudState {
    pub title: String,
    pub detail: String,
    pub is_error: bool,
}

pub fn view_hud<'a, M: 'a>(hud: &'a HudState, mode: UiThemeMode) -> Element<'a, M> {
    let p = palette_for(mode);
    let dot_color = if hud.is_error { p.danger } else { p.accent };
    let font_t = font_for_text(&hud.title, None, Weight::Semibold);
    let font_d = font_for_text(&hud.detail, None, Weight::Normal);

    let pill = container(
        row![
            container(text(""))
                .width(8)
                .height(8)
                .style(move |_| container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: 99.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            text(hud.title.clone())
                .size(13)
                .font(font_t)
                .shaping(text::Shaping::Advanced)
                .color(p.text_primary),
            text("·").size(13).color(p.text_muted),
            text(hud.detail.clone())
                .size(12)
                .font(font_d)
                .shaping(text::Shaping::Advanced)
                .color(p.text_secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .style(move |_| container::Style {
        background: Some(Background::Color(p.bg_surface)),
        border: Border {
            color: if hud.is_error {
                p.danger
            } else {
                p.border_strong
            },
            width: 1.0,
            radius: 999.0.into(),
        },
        shadow: Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, 0.35),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    });

    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}
