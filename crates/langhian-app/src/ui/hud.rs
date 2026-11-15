use crate::ui::icons::{UiIcon, ui_icon};
use crate::ui::theme::{
    font_for_ui, ghost_button_style, hud_pill_style, palette_for, rtl_row, square_icon_button,
};
use iced::font::Weight;
use iced::widget::{Space, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};
use langhian_core::{Translator, UiThemeMode};

#[derive(Debug, Clone)]
pub struct HudState {
    pub title: String,
    pub detail: Option<String>,
    pub is_error: bool,
    pub is_loading: bool,
    pub anim_phase: u8,
}

impl HudState {
    pub fn toast(title: impl Into<String>, detail: Option<String>, is_error: bool) -> Self {
        Self {
            title: title.into(),
            detail,
            is_error,
            is_loading: false,
            anim_phase: 0,
        }
    }

    pub fn loading(title: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            title: title.into(),
            detail,
            is_error: false,
            is_loading: true,
            anim_phase: 0,
        }
    }
}

fn mini_loading_bars<'a, M: 'a>(phase: u8, color: Color) -> Element<'a, M> {
    let heights = [4.0_f32, 9.0, 12.0];
    let bars: Vec<Element<'a, M>> = (0..3)
        .map(|i| {
            let h = heights[(i + phase as usize) % heights.len()];
            container(
                Space::new()
                    .width(Length::Fixed(2.5))
                    .height(Length::Fixed(h)),
            )
            .style(move |_| container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        })
        .collect();

    row(bars)
        .spacing(2.0)
        .align_y(Alignment::Center)
        .height(Length::Fixed(13.0))
        .into()
}

/// Compact movie-subtitle-style toast pill anchored at the bottom-center of the active screen.
pub fn view_hud<'a, M: Clone + 'a>(
    state: &'a HudState,
    mode: UiThemeMode,
    i18n: &'a Translator,
    on_cancel: Option<M>,
) -> Element<'a, M> {
    let p = palette_for(mode);
    let is_rtl = i18n.direction().is_rtl();
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Medium);

    let mut items: Vec<Element<'a, M>> = if state.is_loading {
        vec![
            mini_loading_bars(state.anim_phase, p.accent_text),
            ui_icon(UiIcon::VolumeUp, 13.0, p.accent_text).into(),
            text(&state.title)
                .size(12.0)
                .font(ui_font_semi)
                .color(p.text_primary)
                .shaping(iced::widget::text::Shaping::Advanced)
                .into(),
        ]
    } else {
        let icon_kind = if state.is_error {
            UiIcon::Error
        } else if state.detail.is_some() {
            UiIcon::WandStars
        } else {
            UiIcon::CheckCircle
        };
        let icon_color = if state.is_error {
            p.danger
        } else {
            p.accent_text
        };

        vec![
            ui_icon(icon_kind, 16.0, icon_color).into(),
            text(&state.title)
                .size(13)
                .font(ui_font_semi)
                .color(p.text_primary)
                .shaping(iced::widget::text::Shaping::Advanced)
                .into(),
        ]
    };

    if let Some(detail) = &state.detail
        && !detail.trim().is_empty()
    {
        items.push(
            text("·")
                .size(if state.is_loading { 11.5 } else { 13.0 })
                .font(ui_font)
                .color(p.text_muted)
                .into(),
        );
        items.push(
            text(detail)
                .size(if state.is_loading { 11.5 } else { 12.5 })
                .font(ui_font)
                .color(p.text_secondary)
                .shaping(iced::widget::text::Shaping::Advanced)
                .into(),
        );
    }

    if state.is_loading
        && let Some(msg) = on_cancel
    {
        items.push(
            square_icon_button(
                ui_icon(UiIcon::Close, 11.0, p.text_muted),
                20.0,
                msg,
                ghost_button_style(mode, false),
            )
            .into(),
        );
    }

    let padding = if state.is_loading { [6, 13] } else { [10, 18] };
    let spacing = if state.is_loading { 7 } else { 9 };

    container(
        container(
            rtl_row(is_rtl, items)
                .spacing(spacing)
                .align_y(Alignment::Center),
        )
        .padding(padding)
        .style(hud_pill_style(mode, state.is_error)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_state_constructors_set_expected_flags() {
        let toast = HudState::toast("Done", Some("detail".to_string()), false);
        assert!(!toast.is_loading);
        assert!(!toast.is_error);
        assert_eq!(toast.anim_phase, 0);

        let loader = HudState::loading("Generating voice...", None);
        assert!(loader.is_loading);
        assert!(!loader.is_error);
        assert_eq!(loader.title, "Generating voice...");
    }
}
