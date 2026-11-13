use crate::models::{ShortcutCombo, ShortcutConfig};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKind {
    Ctrl,
    Alt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    DoubleTapAlt,
    DoubleTapCtrl,
    ScreenshotTranslate,
    NextPrompt,
    PrevPrompt,
    OpenDashboard,
    SwitchPromptSlot(u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Debug)]
pub struct InputStateMachine {
    double_tap_window: Duration,
    enable_double_alt: bool,
    enable_double_ctrl: bool,
    alt_down_clean: bool,
    ctrl_down_clean: bool,
    last_alt_tap: Option<Instant>,
    last_ctrl_tap: Option<Instant>,
}

impl InputStateMachine {
    pub fn new(config: &ShortcutConfig) -> Self {
        Self {
            double_tap_window: Duration::from_millis(config.double_tap_ms),
            enable_double_alt: config.enable_double_alt,
            enable_double_ctrl: config.enable_double_ctrl,
            alt_down_clean: false,
            ctrl_down_clean: false,
            last_alt_tap: None,
            last_ctrl_tap: None,
        }
    }

    pub fn update_config(&mut self, config: &ShortcutConfig) {
        self.double_tap_window = Duration::from_millis(config.double_tap_ms);
        self.enable_double_alt = config.enable_double_alt;
        self.enable_double_ctrl = config.enable_double_ctrl;
    }

    pub fn on_modifier_down(&mut self, kind: ModifierKind, others: ModifierState) {
        match kind {
            ModifierKind::Alt => {
                if !others.ctrl && !others.shift && !others.meta {
                    self.alt_down_clean = true;
                } else {
                    self.alt_down_clean = false;
                    self.last_alt_tap = None;
                }
            }
            ModifierKind::Ctrl => {
                if !others.alt && !others.shift && !others.meta {
                    self.ctrl_down_clean = true;
                } else {
                    self.ctrl_down_clean = false;
                    self.last_ctrl_tap = None;
                }
            }
        }
    }

    pub fn on_modifier_up(&mut self, kind: ModifierKind, now: Instant) -> Option<InputAction> {
        match kind {
            ModifierKind::Alt => {
                let was_clean = self.alt_down_clean;
                self.alt_down_clean = false;
                if !was_clean || !self.enable_double_alt {
                    return None;
                }
                if let Some(prev) = self.last_alt_tap
                    && now.duration_since(prev) <= self.double_tap_window
                {
                    self.last_alt_tap = None;
                    return Some(InputAction::DoubleTapAlt);
                }
                self.last_alt_tap = Some(now);
                None
            }
            ModifierKind::Ctrl => {
                let was_clean = self.ctrl_down_clean;
                self.ctrl_down_clean = false;
                if !was_clean || !self.enable_double_ctrl {
                    return None;
                }
                if let Some(prev) = self.last_ctrl_tap
                    && now.duration_since(prev) <= self.double_tap_window
                {
                    self.last_ctrl_tap = None;
                    return Some(InputAction::DoubleTapCtrl);
                }
                self.last_ctrl_tap = Some(now);
                None
            }
        }
    }

    pub fn on_non_modifier_key_down(
        &mut self,
        key_name: &str,
        mods: ModifierState,
        config: &ShortcutConfig,
    ) -> Option<InputAction> {
        // Any non-modifier key cancels pending standalone modifier tap detection.
        self.alt_down_clean = false;
        self.ctrl_down_clean = false;
        self.last_alt_tap = None;
        self.last_ctrl_tap = None;

        let upper = key_name.to_ascii_uppercase();

        if matches_combo(&config.screenshot, mods, &upper) {
            return Some(InputAction::ScreenshotTranslate);
        }
        if matches_combo(&config.next_prompt, mods, &upper) {
            return Some(InputAction::NextPrompt);
        }
        if matches_combo(&config.prev_prompt, mods, &upper) {
            return Some(InputAction::PrevPrompt);
        }
        if matches_combo(&config.open_dashboard, mods, &upper) {
            return Some(InputAction::OpenDashboard);
        }

        // Ctrl+Alt+1..9 direct prompt slot switching
        if mods.ctrl
            && mods.alt
            && !mods.shift
            && !mods.meta
            && upper.len() == 1
            && let Some(ch) = upper.chars().next()
            && ('1'..='9').contains(&ch)
        {
            return Some(InputAction::SwitchPromptSlot((ch as u8) - b'0'));
        }

        None
    }
}

pub fn matches_combo(combo: &ShortcutCombo, mods: ModifierState, key_upper: &str) -> bool {
    combo.ctrl == mods.ctrl
        && combo.alt == mods.alt
        && combo.shift == mods.shift
        && combo.meta == mods.meta
        && combo.key.eq_ignore_ascii_case(key_upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_double_tap_alt_triggers_and_resets() {
        let cfg = ShortcutConfig::default();
        let mut sm = InputStateMachine::new(&cfg);
        let t0 = Instant::now();

        sm.on_modifier_down(ModifierKind::Alt, ModifierState::default());
        assert_eq!(sm.on_modifier_up(ModifierKind::Alt, t0), None);

        sm.on_modifier_down(ModifierKind::Alt, ModifierState::default());
        assert_eq!(
            sm.on_modifier_up(ModifierKind::Alt, t0 + Duration::from_millis(180)),
            Some(InputAction::DoubleTapAlt)
        );

        // Triple tap must not fire immediately again
        sm.on_modifier_down(ModifierKind::Alt, ModifierState::default());
        assert_eq!(
            sm.on_modifier_up(ModifierKind::Alt, t0 + Duration::from_millis(260)),
            None
        );
    }

    #[test]
    fn alt_tab_cancels_double_tap() {
        let cfg = ShortcutConfig::default();
        let mut sm = InputStateMachine::new(&cfg);
        let t0 = Instant::now();

        sm.on_modifier_down(ModifierKind::Alt, ModifierState::default());
        let _ = sm.on_non_modifier_key_down(
            "TAB",
            ModifierState {
                alt: true,
                ..Default::default()
            },
            &cfg,
        );
        assert_eq!(sm.on_modifier_up(ModifierKind::Alt, t0), None);

        sm.on_modifier_down(ModifierKind::Alt, ModifierState::default());
        assert_eq!(
            sm.on_modifier_up(ModifierKind::Alt, t0 + Duration::from_millis(120)),
            None
        );
    }

    #[test]
    fn slow_taps_do_not_trigger() {
        let cfg = ShortcutConfig::default();
        let mut sm = InputStateMachine::new(&cfg);
        let t0 = Instant::now();

        sm.on_modifier_down(ModifierKind::Ctrl, ModifierState::default());
        assert_eq!(sm.on_modifier_up(ModifierKind::Ctrl, t0), None);

        sm.on_modifier_down(ModifierKind::Ctrl, ModifierState::default());
        assert_eq!(
            sm.on_modifier_up(ModifierKind::Ctrl, t0 + Duration::from_millis(500)),
            None
        );
    }

    #[test]
    fn matches_prompt_slots_and_screenshot() {
        let cfg = ShortcutConfig::default();
        let mut sm = InputStateMachine::new(&cfg);

        assert_eq!(
            sm.on_non_modifier_key_down(
                "X",
                ModifierState {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
                &cfg
            ),
            Some(InputAction::ScreenshotTranslate)
        );
        assert_eq!(
            sm.on_non_modifier_key_down(
                "3",
                ModifierState {
                    ctrl: true,
                    alt: true,
                    ..Default::default()
                },
                &cfg
            ),
            Some(InputAction::SwitchPromptSlot(3))
        );
    }
}
