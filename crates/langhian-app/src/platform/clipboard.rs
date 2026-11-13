use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

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
        IN_SYNTHETIC_INPUT.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
pub struct CapturedSelection {
    pub text: String,
    pub previous_clipboard: Option<String>,
}

fn modifier_key() -> Key {
    #[cfg(target_os = "macos")]
    {
        Key::Meta
    }
    #[cfg(not(target_os = "macos"))]
    {
        Key::Control
    }
}

fn send_combo_c(enigo: &mut Enigo) -> Result<(), String> {
    let m = modifier_key();
    enigo.key(m, Press).map_err(|e| format!("{e:?}"))?;
    thread::sleep(Duration::from_millis(10));
    enigo
        .key(Key::Unicode('c'), Click)
        .map_err(|e| format!("{e:?}"))?;
    thread::sleep(Duration::from_millis(10));
    enigo.key(m, Release).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

fn send_combo_a(enigo: &mut Enigo) -> Result<(), String> {
    let m = modifier_key();
    enigo.key(m, Press).map_err(|e| format!("{e:?}"))?;
    thread::sleep(Duration::from_millis(10));
    enigo
        .key(Key::Unicode('a'), Click)
        .map_err(|e| format!("{e:?}"))?;
    thread::sleep(Duration::from_millis(10));
    enigo.key(m, Release).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

fn send_combo_v(enigo: &mut Enigo) -> Result<(), String> {
    let m = modifier_key();
    enigo.key(m, Press).map_err(|e| format!("{e:?}"))?;
    thread::sleep(Duration::from_millis(10));
    enigo
        .key(Key::Unicode('v'), Click)
        .map_err(|e| format!("{e:?}"))?;
    thread::sleep(Duration::from_millis(10));
    enigo.key(m, Release).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

fn poll_clipboard_non_empty(clipboard: &mut Clipboard, attempts: usize) -> Option<String> {
    for _ in 0..attempts {
        thread::sleep(Duration::from_millis(20));
        if let Ok(val) = clipboard.get_text()
            && !val.trim().is_empty()
        {
            return Some(val);
        }
    }
    None
}

pub fn capture_selection(
    restore_clipboard_immediately: bool,
    auto_select_all_fallback: bool,
) -> Result<CapturedSelection, String> {
    let _guard = SyntheticGuard::enter();
    thread::sleep(Duration::from_millis(25));

    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    let previous = clipboard.get_text().ok();

    if clipboard.clear().is_err() {
        let _ = clipboard.set_text(String::new());
    }
    thread::sleep(Duration::from_millis(20));

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("Input simulator error: {e:?}"))?;
    send_combo_c(&mut enigo)?;

    let mut captured = poll_clipboard_non_empty(&mut clipboard, 7);

    if captured.is_none() && auto_select_all_fallback {
        send_combo_a(&mut enigo)?;
        thread::sleep(Duration::from_millis(35));
        send_combo_c(&mut enigo)?;
        captured = poll_clipboard_non_empty(&mut clipboard, 7);
    }

    if restore_clipboard_immediately || captured.is_none() {
        restore_prev(&mut clipboard, &previous);
    }

    match captured {
        Some(text) => Ok(CapturedSelection {
            text,
            previous_clipboard: previous,
        }),
        None => Err("EMPTY_SELECTION".to_string()),
    }
}

pub fn replace_with_text(
    translated: &str,
    restore_clipboard: bool,
    previous_clipboard: Option<String>,
) -> Result<(), String> {
    let _guard = SyntheticGuard::enter();
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    clipboard
        .set_text(translated.to_string())
        .map_err(|e| format!("Clipboard write error: {e}"))?;
    thread::sleep(Duration::from_millis(35));

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("Input simulator error: {e:?}"))?;
    send_combo_v(&mut enigo)?;

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
        .map_err(|e| format!("Clipboard write error: {e}"))
}

fn restore_prev(clipboard: &mut Clipboard, previous: &Option<String>) {
    if let Some(prev) = previous {
        let _ = clipboard.set_text(prev.clone());
    } else {
        let _ = clipboard.clear();
    }
}
