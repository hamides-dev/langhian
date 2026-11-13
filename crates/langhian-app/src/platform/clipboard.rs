use arboard::Clipboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

#[cfg(not(target_os = "windows"))]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// Set to `true` briefly while Langhian synthesizes Ctrl+C / Ctrl+A / Ctrl+V so keyboard hooks ignore synthetic events.
pub static IN_SYNTHETIC_INPUT: AtomicBool = AtomicBool::new(false);

struct SyntheticGuard;
impl SyntheticGuard {
    fn enter() -> Self {
        IN_SYNTHETIC_INPUT.store(true, Ordering::SeqCst);
        Self
    }
}
impl Drop for SyntheticGuard {
    fn drop(&mut self) {
        thread::sleep(Duration::from_millis(20));
        IN_SYNTHETIC_INPUT.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
pub struct CapturedSelection {
    pub text: String,
    pub previous_clipboard: Option<String>,
}

#[cfg(target_os = "windows")]
mod win_input {
    use super::{Duration, thread};
    use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VK_A, VK_C, VK_CONTROL,
        VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
        VK_RWIN, VK_V,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

    pub fn clipboard_sequence() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }

    pub fn foreground_window() -> isize {
        unsafe { GetForegroundWindow() as isize }
    }

    pub fn restore_foreground_window(hwnd: isize) {
        if hwnd != 0 {
            unsafe {
                let _ = SetForegroundWindow(hwnd as _);
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    fn key_input(vk: u16, flags: u32) -> INPUT {
        let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send_inputs(inputs: &[INPUT]) {
        if inputs.is_empty() {
            return;
        }
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }

    /// Release any modifier keys that are not physically held down anymore (or lingering after double-tap).
    pub fn release_stray_modifiers() {
        let mods = [
            (VK_LMENU, 0u32),
            (VK_RMENU, KEYEVENTF_EXTENDEDKEY),
            (VK_MENU, 0),
            (VK_LCONTROL, 0),
            (VK_RCONTROL, KEYEVENTF_EXTENDEDKEY),
            (VK_CONTROL, 0),
            (VK_LSHIFT, 0),
            (VK_RSHIFT, 0),
            (VK_LWIN, 0),
            (VK_RWIN, KEYEVENTF_EXTENDEDKEY),
        ];
        let mut release = Vec::new();
        for (vk, ext) in mods {
            let down_async = unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 } != 0;
            if !down_async {
                // Only send keyup if async state says released? Wait: if physically released recently,
                // some target windows might still see latched key state; releasing cleanly is harmless when vk=Alt/Ctrl.
                // However avoid releasing LWIN blindly to prevent popping the Start menu.
                if vk != VK_LWIN && vk != VK_RWIN {
                    release.push(key_input(vk, ext | KEYEVENTF_KEYUP));
                }
            }
        }
        send_inputs(&release);
    }

    pub fn send_ctrl_vk(vk: u16) {
        let ctrl_down = key_input(VK_CONTROL, 0);
        let key_down = key_input(vk, 0);
        let key_up = key_input(vk, KEYEVENTF_KEYUP);
        let ctrl_up = key_input(VK_CONTROL, KEYEVENTF_KEYUP);

        send_inputs(&[ctrl_down]);
        thread::sleep(Duration::from_millis(14));
        send_inputs(&[key_down]);
        thread::sleep(Duration::from_millis(14));
        send_inputs(&[key_up]);
        thread::sleep(Duration::from_millis(10));
        send_inputs(&[ctrl_up]);
    }

    pub fn simulate_copy() {
        release_stray_modifiers();
        thread::sleep(Duration::from_millis(12));
        send_ctrl_vk(VK_C);
    }

    pub fn simulate_select_all() {
        release_stray_modifiers();
        thread::sleep(Duration::from_millis(12));
        send_ctrl_vk(VK_A);
    }

    pub fn simulate_paste() {
        release_stray_modifiers();
        thread::sleep(Duration::from_millis(12));
        send_ctrl_vk(VK_V);
    }
}

#[cfg(not(target_os = "windows"))]
fn simulate_shortcut(ch: char) -> Result<(), String> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("Input init failed: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("Modifier press error: {e}"))?;
    thread::sleep(Duration::from_millis(15));
    let _ = enigo.key(Key::Unicode(ch), Direction::Click);
    thread::sleep(Duration::from_millis(15));
    let _ = enigo.key(modifier, Direction::Release);
    Ok(())
}

pub fn capture_foreground_handle() -> isize {
    #[cfg(target_os = "windows")]
    {
        win_input::foreground_window()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

pub fn restore_foreground_handle(hwnd: isize) {
    #[cfg(target_os = "windows")]
    {
        win_input::restore_foreground_window(hwnd);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
    }
}

#[cfg(target_os = "windows")]
fn read_clipboard_with_retry(clipboard: &mut Clipboard) -> Option<String> {
    for _ in 0..4 {
        if let Ok(val) = clipboard.get_text()
            && !val.trim().is_empty()
        {
            return Some(val);
        }
        thread::sleep(Duration::from_millis(18));
    }
    None
}

#[cfg(target_os = "windows")]
fn wait_for_sequence_change(
    clipboard: &mut Clipboard,
    seq_before: u32,
    attempts: usize,
) -> Option<String> {
    for _ in 0..attempts {
        thread::sleep(Duration::from_millis(16));
        if win_input::clipboard_sequence() != seq_before {
            return read_clipboard_with_retry(clipboard);
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn poll_clipboard_non_empty(clipboard: &mut Clipboard, attempts: usize) -> Option<String> {
    for _ in 0..attempts {
        if let Ok(val) = clipboard.get_text()
            && !val.trim().is_empty()
        {
            return Some(val);
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Captures the currently selected text from the active application via OS clipboard + Ctrl/Cmd+C.
/// If nothing is selected and `allow_select_all_fallback` is true, tries Ctrl/Cmd+A followed by Ctrl/Cmd+C.
pub fn capture_selection(
    restore_clipboard_immediately: bool,
    allow_select_all_fallback: bool,
    prefer_markdown: bool,
) -> Result<CapturedSelection, String> {
    let _guard = SyntheticGuard::enter();
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    let previous = clipboard.get_text().ok();

    // Give the active app time to finalize double-tap Alt/Ctrl key-up before injecting Ctrl+C.
    thread::sleep(Duration::from_millis(45));

    #[cfg(target_os = "windows")]
    let mut captured = {
        let seq_before = win_input::clipboard_sequence();
        win_input::simulate_copy();
        let first = wait_for_sequence_change(&mut clipboard, seq_before, 20);
        if first.is_none() && allow_select_all_fallback {
            win_input::simulate_select_all();
            thread::sleep(Duration::from_millis(45));
            let seq_before_all = win_input::clipboard_sequence();
            win_input::simulate_copy();
            wait_for_sequence_change(&mut clipboard, seq_before_all, 20)
        } else {
            first
        }
    };

    #[cfg(not(target_os = "windows"))]
    let mut captured = {
        if clipboard.clear().is_err() {
            let _ = clipboard.set_text(String::new());
        }
        thread::sleep(Duration::from_millis(15));
        simulate_shortcut('c')?;
        let first = poll_clipboard_non_empty(&mut clipboard, 10);
        if first.is_none() && allow_select_all_fallback {
            simulate_shortcut('a')?;
            thread::sleep(Duration::from_millis(40));
            simulate_shortcut('c')?;
            poll_clipboard_non_empty(&mut clipboard, 10)
        } else {
            first
        }
    };

    if prefer_markdown
        && let Some(ref plain) = captured
        && !langhian_core::contains_markdown_structure(plain)
        && let Ok(html) = clipboard.get().html()
        && let Some(md) = langhian_core::html_fragment_to_markdown(&html)
    {
        captured = Some(md);
    }

    if restore_clipboard_immediately || captured.is_none() {
        restore_prev(&mut clipboard, &previous);
    }

    match captured.take() {
        Some(text) => Ok(CapturedSelection {
            text,
            previous_clipboard: previous,
        }),
        None => Err("No text selected".to_string()),
    }
}

/// Replaces the currently selected text in the active input box by putting `text` into clipboard and pressing Ctrl/Cmd+V.
pub fn replace_with_text(
    text: &str,
    restore_clipboard: bool,
    previous_clipboard: Option<String>,
) -> Result<(), String> {
    let _guard = SyntheticGuard::enter();
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Clipboard set failed: {e}"))?;
    thread::sleep(Duration::from_millis(40));

    #[cfg(target_os = "windows")]
    win_input::simulate_paste();

    #[cfg(not(target_os = "windows"))]
    simulate_shortcut('v')?;

    if restore_clipboard {
        thread::sleep(Duration::from_millis(180));
        restore_prev(&mut clipboard, &previous_clipboard);
    }
    Ok(())
}

pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Clipboard set failed: {e}"))
}

fn restore_prev(clipboard: &mut Clipboard, previous: &Option<String>) {
    if let Some(prev) = previous {
        let _ = clipboard.set_text(prev.clone());
    } else {
        let _ = clipboard.clear();
    }
}
