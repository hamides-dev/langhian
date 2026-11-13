use image::{DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;

#[derive(Debug, Clone)]
pub struct CapturedScreen {
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub logical_width: f32,
    pub logical_height: f32,
    pub image_width: u32,
    pub image_height: u32,
    pub rgba_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct NormalizedRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub fn capture_active_monitor(cursor_pos: Option<(i32, i32)>) -> Result<CapturedScreen, String> {
    let monitors =
        xcap::Monitor::all().map_err(|e| format!("Failed to enumerate monitors: {e}"))?;
    if monitors.is_empty() {
        return Err("No displays found".to_string());
    }

    let chosen = if let Some((cx, cy)) = cursor_pos {
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
            .unwrap_or(&monitors[0])
    } else {
        monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .unwrap_or(&monitors[0])
    };

    let monitor_x = chosen.x().unwrap_or(0);
    let monitor_y = chosen.y().unwrap_or(0);
    let scale = chosen.scale_factor().unwrap_or(1.0).max(0.5);
    let raw_w = chosen.width().unwrap_or(1920);
    let raw_h = chosen.height().unwrap_or(1080);

    let captured = chosen
        .capture_image()
        .map_err(|e| format!("Screen capture failed: {e}"))?;
    let image_width = captured.width();
    let image_height = captured.height();
    let rgba_bytes = captured.into_raw();

    // Logical dimensions fallback safely if scale factor differs on OS
    let logical_width = (raw_w as f32) / scale;
    let logical_height = (raw_h as f32) / scale;

    Ok(CapturedScreen {
        monitor_x,
        monitor_y,
        logical_width,
        logical_height,
        image_width,
        image_height,
        rgba_bytes,
    })
}

pub fn crop_screen_to_png(
    screen: &CapturedScreen,
    canvas_width: f32,
    canvas_height: f32,
    sel: NormalizedRect,
) -> Result<Vec<u8>, String> {
    if canvas_width <= 1.0 || canvas_height <= 1.0 {
        return Err("Invalid overlay dimensions".to_string());
    }

    let scale_x = screen.image_width as f32 / canvas_width;
    let scale_y = screen.image_height as f32 / canvas_height;

    let x = (sel.x.max(0.0) * scale_x).round() as u32;
    let y = (sel.y.max(0.0) * scale_y).round() as u32;
    let w = (sel.width * scale_x)
        .round()
        .max(4.0)
        .min((screen.image_width.saturating_sub(x)) as f32) as u32;
    let h = (sel.height * scale_y)
        .round()
        .max(4.0)
        .min((screen.image_height.saturating_sub(y)) as f32) as u32;

    if w < 4 || h < 4 {
        return Err("Selected region is too small".to_string());
    }

    let img_buf = RgbaImage::from_raw(
        screen.image_width,
        screen.image_height,
        screen.rgba_bytes.clone(),
    )
    .ok_or_else(|| "Invalid RGBA buffer".to_string())?;

    let cropped = DynamicImage::ImageRgba8(img_buf).crop_imm(x, y, w, h);
    let mut out = Cursor::new(Vec::new());
    cropped
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode cropped PNG: {e}"))?;
    Ok(out.into_inner())
}
