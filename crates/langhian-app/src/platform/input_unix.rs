#![cfg(not(target_os = "windows"))]

use super::clipboard::IN_SYNTHETIC_INPUT;
use langhian_core::ShortcutConfig;
use langhian_core::hotkey_logic::{InputAction, InputStateMachine, ModifierKind, ModifierState};
use rdev::{Event, EventType, Key, listen};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

struct UnixHookState {
    config: Arc<Mutex<ShortcutConfig>>,
    sm: Mutex<InputStateMachine>,
    tx: UnboundedSender<InputAction>,
    mods: Mutex<ModifierState>,
    cursor: Mutex<(i32, i32)>,
}

static SHARED: OnceLock<UnixHookState> = OnceLock::new();

pub fn start_keyboard_listener(
    config: Arc<Mutex<ShortcutConfig>>,
    tx: UnboundedSender<InputAction>,
) {
    let initial_cfg = config.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = SHARED.set(UnixHookState {
        config,
        sm: Mutex::new(InputStateMachine::new(&initial_cfg)),
        tx,
        mods: Mutex::new(ModifierState::default()),
        cursor: Mutex::new((0, 0)),
    });

    std::thread::Builder::new()
        .name("langhian-unix-hook".to_string())
        .spawn(|| {
            if let Err(err) = listen(handle_event) {
                eprintln!("Langhian global input listener exited: {:?}", err);
            }
        })
        .ok();
}

pub fn refresh_shortcut_config(cfg: &ShortcutConfig) {
    if let Some(shared) = SHARED.get() {
        if let Ok(mut g) = shared.config.lock() {
            *g = cfg.clone();
        }
        if let Ok(mut sm) = shared.sm.lock() {
            sm.update_config(cfg);
        }
    }
}

pub fn current_cursor_pos() -> Option<(i32, i32)> {
    SHARED.get().and_then(|s| s.cursor.lock().ok().map(|g| *g))
}

fn handle_event(event: Event) {
    if IN_SYNTHETIC_INPUT.load(Ordering::SeqCst) {
        return;
    }
    let Some(shared) = SHARED.get() else {
        return;
    };

    match event.event_type {
        EventType::MouseMove { x, y } => {
            if let Ok(mut cur) = shared.cursor.lock() {
                *cur = (x as i32, y as i32);
            }
        }
        EventType::KeyPress(key) => match key {
            Key::Alt | Key::AltGr => {
                if let Ok(mut m) = shared.mods.lock() {
                    if !m.alt {
                        m.alt = true;
                        let others = ModifierState { alt: false, ..*m };
                        if let Ok(mut sm) = shared.sm.lock() {
                            sm.on_modifier_down(ModifierKind::Alt, others);
                        }
                    }
                }
            }
            Key::ControlLeft | Key::ControlRight => {
                if let Ok(mut m) = shared.mods.lock() {
                    if !m.ctrl {
                        m.ctrl = true;
                        let others = ModifierState { ctrl: false, ..*m };
                        if let Ok(mut sm) = shared.sm.lock() {
                            sm.on_modifier_down(ModifierKind::Ctrl, others);
                        }
                    }
                }
            }
            Key::ShiftLeft | Key::ShiftRight => {
                if let Ok(mut m) = shared.mods.lock() {
                    m.shift = true;
                }
            }
            Key::MetaLeft | Key::MetaRight => {
                if let Ok(mut m) = shared.mods.lock() {
                    m.meta = true;
                }
            }
            other => {
                let mods = shared.mods.lock().map(|m| *m).unwrap_or_default();
                let cfg = shared.config.lock().map(|c| c.clone()).unwrap_or_default();
                let name = rdev_key_to_name(other);
                if let Ok(mut sm) = shared.sm.lock() {
                    if let Some(act) = sm.on_non_modifier_key_down(&name, mods, &cfg) {
                        let _ = shared.tx.send(act);
                    }
                }
            }
        },
        EventType::KeyRelease(key) => match key {
            Key::Alt | Key::AltGr => {
                if let Ok(mut m) = shared.mods.lock() {
                    m.alt = false;
                }
                if let Ok(mut sm) = shared.sm.lock() {
                    if let Some(act) = sm.on_modifier_up(ModifierKind::Alt, Instant::now()) {
                        let _ = shared.tx.send(act);
                    }
                }
            }
            Key::ControlLeft | Key::ControlRight => {
                if let Ok(mut m) = shared.mods.lock() {
                    m.ctrl = false;
                }
                if let Ok(mut sm) = shared.sm.lock() {
                    if let Some(act) = sm.on_modifier_up(ModifierKind::Ctrl, Instant::now()) {
                        let _ = shared.tx.send(act);
                    }
                }
            }
            Key::ShiftLeft | Key::ShiftRight => {
                if let Ok(mut m) = shared.mods.lock() {
                    m.shift = false;
                }
            }
            Key::MetaLeft | Key::MetaRight => {
                if let Ok(mut m) = shared.mods.lock() {
                    m.meta = false;
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn rdev_key_to_name(key: Key) -> String {
    match key {
        Key::KeyA => "A",
        Key::KeyB => "B",
        Key::KeyC => "C",
        Key::KeyD => "D",
        Key::KeyE => "E",
        Key::KeyF => "F",
        Key::KeyG => "G",
        Key::KeyH => "H",
        Key::KeyI => "I",
        Key::KeyJ => "J",
        Key::KeyK => "K",
        Key::KeyL => "L",
        Key::KeyM => "M",
        Key::KeyN => "N",
        Key::KeyO => "O",
        Key::KeyP => "P",
        Key::KeyQ => "Q",
        Key::KeyR => "R",
        Key::KeyS => "S",
        Key::KeyT => "T",
        Key::KeyU => "U",
        Key::KeyV => "V",
        Key::KeyW => "W",
        Key::KeyX => "X",
        Key::KeyY => "Y",
        Key::KeyZ => "Z",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::LeftArrow => "LEFT",
        Key::RightArrow => "RIGHT",
        Key::UpArrow => "UP",
        Key::DownArrow => "DOWN",
        Key::Space => "SPACE",
        Key::Tab => "TAB",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        _ => "OTHER",
    }
    .to_string()
}
