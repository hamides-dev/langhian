#![cfg(target_os = "windows")]

use langhian_core::ShortcutConfig;
use langhian_core::hotkey_logic::{InputAction, InputStateMachine, ModifierKind, ModifierState};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VK_CONTROL, VK_DOWN, VK_F1, VK_F12, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB,
    VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, SetWindowsHookExW, TranslateMessage, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

struct HookSharedState {
    config: Arc<Mutex<ShortcutConfig>>,
    sm: Mutex<InputStateMachine>,
    tx: UnboundedSender<InputAction>,
    alt_is_down: Mutex<bool>,
    ctrl_is_down: Mutex<bool>,
}

static SHARED: OnceLock<HookSharedState> = OnceLock::new();

pub fn start_keyboard_listener(
    config: Arc<Mutex<ShortcutConfig>>,
    tx: UnboundedSender<InputAction>,
) {
    let initial_cfg = config.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = SHARED.set(HookSharedState {
        config,
        sm: Mutex::new(InputStateMachine::new(&initial_cfg)),
        tx,
        alt_is_down: Mutex::new(false),
        ctrl_is_down: Mutex::new(false),
    });

    std::thread::Builder::new()
        .name("langhian-win-hook".to_string())
        .spawn(|| unsafe {
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0);
            if hook.is_null() {
                eprintln!("Langhian: SetWindowsHookExW failed");
                return;
            }
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
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
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) != 0 {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }
}

unsafe fn is_vk_down(vk: u16) -> bool {
    (unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000) != 0
}

unsafe fn current_modifiers(exclude_vk: u16) -> ModifierState {
    let ctrl = (exclude_vk != VK_LCONTROL && exclude_vk != VK_RCONTROL && exclude_vk != VK_CONTROL)
        && (unsafe { is_vk_down(VK_LCONTROL) } || unsafe { is_vk_down(VK_RCONTROL) });
    let alt = (exclude_vk != VK_LMENU && exclude_vk != VK_RMENU && exclude_vk != VK_MENU)
        && (unsafe { is_vk_down(VK_LMENU) } || unsafe { is_vk_down(VK_RMENU) });
    let shift = (exclude_vk != VK_LSHIFT && exclude_vk != VK_RSHIFT && exclude_vk != VK_SHIFT)
        && (unsafe { is_vk_down(VK_LSHIFT) } || unsafe { is_vk_down(VK_RSHIFT) });
    let meta = (exclude_vk != VK_LWIN && exclude_vk != VK_RWIN)
        && (unsafe { is_vk_down(VK_LWIN) } || unsafe { is_vk_down(VK_RWIN) });
    ModifierState {
        ctrl,
        alt,
        shift,
        meta,
    }
}

fn mask_alt_menu_activation() {
    // Inject an unassigned virtual key (0xE8) to dismiss Windows menu-bar activation cleanly
    unsafe {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0xE8,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0xE8,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];
        SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

fn vk_to_name(vk: u16) -> Option<String> {
    match vk {
        0x30..=0x39 => Some(((vk as u8) as char).to_string()),
        0x41..=0x5A => Some(((vk as u8) as char).to_string()),
        v if v == VK_LEFT => Some("LEFT".to_string()),
        v if v == VK_RIGHT => Some("RIGHT".to_string()),
        v if v == VK_UP => Some("UP".to_string()),
        v if v == VK_DOWN => Some("DOWN".to_string()),
        v if v == VK_SPACE => Some("SPACE".to_string()),
        v if v == VK_TAB => Some("TAB".to_string()),
        v if (VK_F1..=VK_F12).contains(&v) => Some(format!("F{}", (v - VK_F1) + 1)),
        _ => None,
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if (kbd.flags & LLKHF_INJECTED) == 0
            && let Some(shared) = SHARED.get()
        {
            let vk = kbd.vkCode as u16;
            let msg = wparam as u32;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

            let is_alt = vk == VK_LMENU || vk == VK_RMENU || vk == VK_MENU;
            let is_ctrl = vk == VK_LCONTROL || vk == VK_RCONTROL || vk == VK_CONTROL;
            let is_shift = vk == VK_LSHIFT || vk == VK_RSHIFT || vk == VK_SHIFT;
            let is_meta = vk == VK_LWIN || vk == VK_RWIN;

            if is_alt {
                if is_down {
                    if let Ok(mut down) = shared.alt_is_down.lock()
                        && !*down
                    {
                        *down = true;
                        let others = unsafe { current_modifiers(vk) };
                        if let Ok(mut sm) = shared.sm.lock() {
                            sm.on_modifier_down(ModifierKind::Alt, others);
                        }
                    }
                } else if is_up {
                    if let Ok(mut down) = shared.alt_is_down.lock() {
                        *down = false;
                    }
                    let action =
                        shared.sm.lock().ok().and_then(|mut sm| {
                            sm.on_modifier_up(ModifierKind::Alt, Instant::now())
                        });
                    if let Some(act) = action {
                        mask_alt_menu_activation();
                        let _ = shared.tx.send(act);
                    }
                }
            } else if is_ctrl {
                if is_down {
                    if let Ok(mut down) = shared.ctrl_is_down.lock()
                        && !*down
                    {
                        *down = true;
                        let others = unsafe { current_modifiers(vk) };
                        if let Ok(mut sm) = shared.sm.lock() {
                            sm.on_modifier_down(ModifierKind::Ctrl, others);
                        }
                    }
                } else if is_up {
                    if let Ok(mut down) = shared.ctrl_is_down.lock() {
                        *down = false;
                    }
                    let action =
                        shared.sm.lock().ok().and_then(|mut sm| {
                            sm.on_modifier_up(ModifierKind::Ctrl, Instant::now())
                        });
                    if let Some(act) = action {
                        let _ = shared.tx.send(act);
                    }
                }
            } else if !is_shift && !is_meta && is_down {
                if let Some(key_name) = vk_to_name(vk) {
                    let mods = unsafe { current_modifiers(vk) };
                    let cfg = shared.config.lock().map(|g| g.clone()).unwrap_or_default();
                    let action = shared
                        .sm
                        .lock()
                        .ok()
                        .and_then(|mut sm| sm.on_non_modifier_key_down(&key_name, mods, &cfg));
                    if let Some(act) = action {
                        let _ = shared.tx.send(act);
                        return 1; // Suppress matched global hotkey
                    }
                } else if let Ok(mut sm) = shared.sm.lock() {
                    let mods = unsafe { current_modifiers(vk) };
                    let cfg = shared.config.lock().map(|g| g.clone()).unwrap_or_default();
                    let _ = sm.on_non_modifier_key_down("OTHER", mods, &cfg);
                }
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}
