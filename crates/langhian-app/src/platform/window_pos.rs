use iced::Point;

#[derive(Debug, Clone, Copy)]
pub struct MonitorWorkArea {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub raw_x: i32,
    pub raw_y: i32,
}

impl MonitorWorkArea {
    pub fn phys_to_logical(&self, px: i32, py: i32) -> (f32, f32) {
        let s = self.scale.max(0.5);
        (
            self.raw_x as f32 / s + ((px - self.raw_x) as f32) / s,
            self.raw_y as f32 / s + ((py - self.raw_y) as f32) / s,
        )
    }
}

pub fn active_monitor_work_area(cursor_phys: Option<(i32, i32)>) -> MonitorWorkArea {
    let monitors = xcap::Monitor::all().unwrap_or_default();
    let chosen = if let Some((cx, cy)) = cursor_phys {
        monitors
            .iter()
            .find(|m| {
                let mx = m.x().unwrap_or(0);
                let my = m.y().unwrap_or(0);
                let mw = m.width().unwrap_or(0) as i32;
                let mh = m.height().unwrap_or(0) as i32;
                cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
            })
            .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
            .or_else(|| monitors.first())
    } else {
        monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| monitors.first())
    };

    let (raw_x, raw_y, raw_w, raw_h, scale) = if let Some(m) = chosen {
        (
            m.x().unwrap_or(0),
            m.y().unwrap_or(0),
            m.width().unwrap_or(1920) as i32,
            m.height().unwrap_or(1080) as i32,
            m.scale_factor().unwrap_or(1.0).max(0.5),
        )
    } else {
        (0, 0, 1920, 1080, 1.0)
    };

    #[cfg(target_os = "windows")]
    if let Some((wl, wt, wr, wb)) = win_work_rect(cursor_phys) {
        let s = scale;
        return MonitorWorkArea {
            x: wl as f32 / s,
            y: wt as f32 / s,
            width: ((wr - wl).max(200)) as f32 / s,
            height: ((wb - wt).max(200)) as f32 / s,
            scale: s,
            raw_x,
            raw_y,
        };
    }

    let taskbar_pad = 48.0;
    MonitorWorkArea {
        x: raw_x as f32 / scale,
        y: raw_y as f32 / scale,
        width: (raw_w as f32 / scale).max(320.0),
        height: ((raw_h as f32 / scale) - taskbar_pad).max(240.0),
        scale,
        raw_x,
        raw_y,
    }
}

#[cfg(target_os = "windows")]
fn win_work_rect(cursor_phys: Option<(i32, i32)>) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };

    let (px, py) = cursor_phys.unwrap_or((100, 100));
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: px, y: py }, MONITOR_DEFAULTTONEAREST);
        if hmon.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(hmon, &mut info) == 0 {
            return None;
        }
        let w = info.rcWork;
        Some((w.left, w.top, w.right, w.bottom))
    }
}

/// Captures a best-effort physical anchor `(x, top_y, bottom_y)` from the active text caret or cursor.
pub fn active_selection_anchor_phys(
    foreground_hwnd: isize,
    cursor_phys: Option<(i32, i32)>,
) -> Option<(i32, i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        if let Some(caret) = win_caret_rect_screen(foreground_hwnd) {
            return Some(caret);
        }
    }
    let _ = foreground_hwnd;
    cursor_phys.map(|(x, y)| (x, y, y))
}

#[cfg(target_os = "windows")]
fn win_caret_rect_screen(foreground_hwnd: isize) -> Option<(i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = if foreground_hwnd != 0 {
            foreground_hwnd as HWND
        } else {
            GetForegroundWindow()
        };
        if hwnd.is_null() {
            return None;
        }
        let thread_id = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        if thread_id == 0 {
            return None;
        }
        let mut gui: GUITHREADINFO = std::mem::zeroed();
        gui.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(thread_id, &mut gui) == 0 || gui.hwndCaret.is_null() {
            return None;
        }
        let rc = gui.rcCaret;
        if rc.right <= rc.left && rc.bottom <= rc.top {
            return None;
        }
        let mut top_pt = POINT {
            x: rc.left,
            y: rc.top,
        };
        let mut bot_pt = POINT {
            x: rc.right.max(rc.left),
            y: rc.bottom.max(rc.top + 16),
        };
        if ClientToScreen(gui.hwndCaret, &mut top_pt) == 0
            || ClientToScreen(gui.hwndCaret, &mut bot_pt) == 0
        {
            return None;
        }
        Some((top_pt.x, top_pt.y, bot_pt.y))
    }
}

pub fn compute_smart_modal_position(
    saved_pos: Option<[f32; 2]>,
    anchor_phys: Option<(i32, i32, i32)>,
    cursor_phys: Option<(i32, i32)>,
    modal_w: f32,
    modal_h: f32,
) -> Point {
    if let Some([sx, sy]) = saved_pos
        && is_point_visible_on_desktop(sx, sy, modal_w, modal_h)
    {
        return Point::new(sx, sy);
    }

    let area = active_monitor_work_area(anchor_phys.map(|a| (a.0, a.2)).or(cursor_phys));
    let margin = 12.0;

    if let Some((ax, _ay_top, ay_bottom)) = anchor_phys {
        let (lx, ly_bottom) = area.phys_to_logical(ax, ay_bottom);
        let below_y = ly_bottom + 14.0;
        if below_y >= area.y + margin && below_y + modal_h <= area.y + area.height - margin {
            let clamped_x = (lx - modal_w * 0.35).clamp(
                area.x + margin,
                (area.x + area.width - modal_w - margin).max(area.x + margin),
            );
            return Point::new(clamped_x, below_y);
        }
    }

    // Center gracefully on the active monitor if placing below would clip off-screen
    Point::new(
        area.x + ((area.width - modal_w) / 2.0).max(margin),
        area.y + ((area.height - modal_h) / 2.0).max(margin),
    )
}

pub fn compute_hud_subtitle_position(
    cursor_phys: Option<(i32, i32)>,
    hud_w: f32,
    hud_h: f32,
) -> Point {
    let area = active_monitor_work_area(cursor_phys);
    let x = area.x + ((area.width - hud_w) / 2.0).max(16.0);
    let y = (area.y + area.height - hud_h - 32.0).max(area.y + 16.0);
    Point::new(x, y)
}

fn is_point_visible_on_desktop(x: f32, y: f32, w: f32, h: f32) -> bool {
    let Ok(monitors) = xcap::Monitor::all() else {
        return x >= -40.0 && y >= -40.0 && x < 4000.0 && y < 3000.0;
    };
    if monitors.is_empty() {
        return true;
    }
    monitors.iter().any(|m| {
        let s = m.scale_factor().unwrap_or(1.0).max(0.5);
        let mx = m.x().unwrap_or(0) as f32 / s;
        let my = m.y().unwrap_or(0) as f32 / s;
        let mw = m.width().unwrap_or(1920) as f32 / s;
        let mh = m.height().unwrap_or(1080) as f32 / s;
        x + w >= mx + 80.0 && x <= mx + mw - 80.0 && y + h >= my + 60.0 && y <= my + mh - 60.0
    })
}
