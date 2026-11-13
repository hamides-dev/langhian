pub mod autostart;
pub mod capture;
pub mod clipboard;
#[cfg(not(target_os = "windows"))]
pub mod input_unix;
#[cfg(target_os = "windows")]
pub mod input_win;
pub mod tray;
pub mod updater_exec;

#[cfg(not(target_os = "windows"))]
pub use input_unix::{current_cursor_pos, refresh_shortcut_config, start_keyboard_listener};
#[cfg(target_os = "windows")]
pub use input_win::{current_cursor_pos, refresh_shortcut_config, start_keyboard_listener};
