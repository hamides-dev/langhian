use crate::ui::icons::{UiIcon, ui_icon};
use crate::ui::theme::{font_for_ui, hud_pill_style, palette_for, rtl_row};
use iced::font::Weight;
use iced::widget::{container, text};
use iced::{Alignment, Element, Length};
use langhian_core::{Translator, UiThemeMode};

#[derive(Debug, Clone)]
pub struct HudState {
    pub title: String,
    pub detail: Option<String>,
    pub is_error: bool,
}

/// Compact movie-subtitle-style toast pill anchored at the bottom-center of the active screen.
pub fn view_hud<'a, M: 'a>(
    state: &'a HudState,
    mode: UiThemeMode,
    i18n: &'a Translator,
) -> Element<'a, M> {
    let p = palette_for(mode);
    let is_rtl = i18n.direction().is_rtl();
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Medium);

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

    let mut items: Vec<Element<'a, M>> = vec![
        ui_icon(icon_kind, 16.0, icon_color).into(),
        text(&state.title)
            .size(13)
            .font(ui_font_semi)
            .color(p.text_primary)
            .shaping(iced::widget::text::Shaping::Advanced)
            .into(),
    ];

    if let Some(detail) = &state.detail
        && !detail.trim().is_empty()
    {
        items.push(text("·").size(13).font(ui_font).color(p.text_muted).into());
        items.push(
            text(detail)
                .size(12.5)
                .font(ui_font)
                .color(p.text_secondary)
                .shaping(iced::widget::text::Shaping::Advanced)
                .into(),
        );
    }

    container(
        container(rtl_row(is_rtl, items).spacing(9).align_y(Alignment::Center))
            .padding([10, 18])
            .style(hud_pill_style(mode, state.is_error)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
