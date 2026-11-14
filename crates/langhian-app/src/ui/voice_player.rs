use crate::ui::icons::{UiIcon, ui_icon};
use crate::ui::theme::{
    font_for_ui, ghost_button_style, modal_surface_style, palette_for, primary_button_style,
    rtl_row, square_icon_button, window_close_button_style,
};
use iced::font::Weight;
use iced::widget::{Space, column, container, mouse_area, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};
use langhian_core::{SpeechAudio, Translator, UiThemeMode};

#[derive(Debug, Clone)]
pub struct VoicePlayerState {
    pub audio: SpeechAudio,
    pub model_name: String,
    pub voice_name: String,
    pub is_playing: bool,
    pub anim_phase: u8,
    pub close_hovered: bool,
}

impl VoicePlayerState {
    pub fn new(audio: SpeechAudio, model_name: String, voice_name: String) -> Self {
        Self {
            audio,
            model_name,
            voice_name,
            is_playing: true,
            anim_phase: 0,
            close_hovered: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum VoicePlayerMsg {
    DragWindow,
    TogglePlay,
    Minimize,
    Cancel,
    CloseHovered(bool),
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else if max_chars > 1 {
        let head: String = chars[..max_chars - 1].iter().collect();
        format!("{head}…")
    } else {
        "…".to_string()
    }
}

fn mini_equalizer<'a, M: 'a>(
    is_playing: bool,
    phase: u8,
    active_col: Color,
    idle_col: Color,
) -> Element<'a, M> {
    let base_heights = [5.0_f32, 11.0, 14.0, 8.0];
    let bars: Vec<Element<'a, M>> = (0..4)
        .map(|i| {
            let h = if is_playing {
                base_heights[(i + phase as usize) % base_heights.len()]
            } else {
                4.0
            };
            let bar_col = if is_playing { active_col } else { idle_col };
            container(
                Space::new()
                    .width(Length::Fixed(3.0))
                    .height(Length::Fixed(h)),
            )
            .style(move |_| container::Style {
                background: Some(Background::Color(bar_col)),
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
        .spacing(2.5)
        .align_y(Alignment::Center)
        .height(Length::Fixed(16.0))
        .into()
}

/// Renders the compact, pinned, draggable floating mini speaker player.
pub fn view_voice_player<'a>(
    state: &'a VoicePlayerState,
    mode: UiThemeMode,
    i18n: &'a Translator,
) -> Element<'a, VoicePlayerMsg> {
    let p = palette_for(mode);
    let is_rtl = i18n.direction().is_rtl();
    let ui_font = font_for_ui(i18n.direction(), Weight::Normal);
    let ui_font_semi = font_for_ui(i18n.direction(), Weight::Medium);

    let speaker_bg = if state.is_playing {
        p.success_soft
    } else {
        p.accent_soft
    };
    let speaker_border = if state.is_playing {
        p.success
    } else {
        p.border_focus
    };
    let speaker_fg = if state.is_playing {
        p.success
    } else {
        p.accent_text
    };

    let speaker_badge = container(ui_icon(UiIcon::VolumeUp, 17.0, speaker_fg))
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(34.0))
        .center_x(Length::Fixed(34.0))
        .center_y(Length::Fixed(34.0))
        .style(move |_| container::Style {
            background: Some(Background::Color(speaker_bg)),
            border: Border {
                color: speaker_border,
                width: 1.0,
                radius: 17.0.into(),
            },
            ..Default::default()
        });

    let status_label = if state.is_playing {
        i18n.t("voice_player.speaking")
    } else {
        i18n.t("voice_player.paused")
    };
    let status_col = if state.is_playing {
        p.success
    } else {
        p.text_primary
    };

    let status_row = rtl_row(
        is_rtl,
        vec![
            mini_equalizer(state.is_playing, state.anim_phase, p.success, p.text_muted),
            text(status_label)
                .size(12.0)
                .font(ui_font_semi)
                .color(status_col)
                .into(),
        ],
    )
    .spacing(6)
    .align_y(Alignment::Center);

    let subtitle_raw = if state.voice_name.trim().is_empty() {
        state.model_name.clone()
    } else {
        format!("{} · {}", state.model_name, state.voice_name)
    };
    let subtitle_row = rtl_row(
        is_rtl,
        vec![
            ui_icon(UiIcon::KeepFill, 10.5, p.accent_text).into(),
            text(truncate_label(&subtitle_raw, 22))
                .size(10.5)
                .font(ui_font)
                .color(p.text_muted)
                .into(),
        ],
    )
    .spacing(4)
    .align_y(Alignment::Center);

    let drag_handle = mouse_area(
        rtl_row(
            is_rtl,
            vec![
                speaker_badge.into(),
                column![status_row, subtitle_row].spacing(2).into(),
            ],
        )
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .on_press(VoicePlayerMsg::DragWindow)
    .interaction(iced::mouse::Interaction::Grab);

    let play_stop_btn = square_icon_button(
        ui_icon(
            if state.is_playing {
                UiIcon::Stop
            } else {
                UiIcon::PlayArrow
            },
            15.0,
            Color::WHITE,
        ),
        28.0,
        VoicePlayerMsg::TogglePlay,
        primary_button_style(mode),
    );

    let minimize_btn = square_icon_button(
        ui_icon(UiIcon::Remove, 14.0, p.text_secondary),
        26.0,
        VoicePlayerMsg::Minimize,
        ghost_button_style(mode, false),
    );

    let close_btn = mouse_area(square_icon_button(
        ui_icon(
            UiIcon::Close,
            13.5,
            if state.close_hovered {
                Color::WHITE
            } else {
                p.text_secondary
            },
        ),
        26.0,
        VoicePlayerMsg::Cancel,
        window_close_button_style(mode),
    ))
    .on_enter(VoicePlayerMsg::CloseHovered(true))
    .on_exit(VoicePlayerMsg::CloseHovered(false));

    let controls = rtl_row(
        is_rtl,
        vec![play_stop_btn.into(), minimize_btn.into(), close_btn.into()],
    )
    .spacing(4)
    .align_y(Alignment::Center);

    let pill = container(
        rtl_row(is_rtl, vec![drag_handle.into(), controls.into()])
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(modal_surface_style(mode, 18.0));

    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(1.0)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_player_state_defaults_playing_and_truncates_labels() {
        let state = VoicePlayerState::new(
            SpeechAudio {
                bytes: vec![1, 2, 3],
                format: langhian_core::AudioFormat::Wav,
            },
            "Gemini 2.5 Flash TTS Very Long Name".to_string(),
            "Kore".to_string(),
        );
        assert!(state.is_playing);
        assert_eq!(state.anim_phase, 0);
        assert!(!state.close_hovered);
        assert_eq!(truncate_label("short", 10), "short");
        assert_eq!(truncate_label("abcdefghij", 5), "abcd…");
    }
}
