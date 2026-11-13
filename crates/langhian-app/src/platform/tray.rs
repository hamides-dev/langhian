use crate::ui::theme::APP_ICON_PNG;
use langhian_core::{AppConfig, Translator};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone)]
pub enum TrayAction {
    OpenDashboard,
    QuickTranslate,
    ScreenshotTranslate,
    SelectModel(String),
    SelectPrompt(String),
    Quit,
}

pub struct TrayHandle {
    _tray: TrayIcon,
    actions: Arc<Mutex<HashMap<MenuId, TrayAction>>>,
}

pub fn load_app_icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

pub fn build_tray(
    config: &AppConfig,
    i18n: &Translator,
    tx: UnboundedSender<TrayAction>,
) -> Result<TrayHandle, String> {
    let (menu, map) = build_menu(config, i18n)?;
    let actions = Arc::new(Mutex::new(map));

    let (rgba, w, h) = load_app_icon_rgba()
        .map(|(bytes, w, h)| {
            // Downscale nicely to 32x32 for crisp system tray rendering
            if let Some(buf) = image::RgbaImage::from_raw(w, h, bytes.clone()) {
                let small =
                    image::imageops::resize(&buf, 32, 32, image::imageops::FilterType::Lanczos3);
                (small.into_raw(), 32, 32)
            } else {
                (bytes, w, h)
            }
        })
        .unwrap_or_else(|| ([82, 139, 255, 255].repeat(32 * 32), 32, 32));

    let icon = Icon::from_rgba(rgba, w, h).map_err(|e| format!("Tray icon error: {e}"))?;

    let actions_ref = Arc::clone(&actions);
    let tx_menu = tx.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if let Ok(guard) = actions_ref.lock()
            && let Some(act) = guard.get(&ev.id)
        {
            let _ = tx_menu.send(act.clone());
        }
    }));

    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        #[cfg(not(target_os = "macos"))]
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = ev
        {
            let _ = tx.send(TrayAction::OpenDashboard);
        }
        #[cfg(target_os = "macos")]
        let _ = ev;
    }));

    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Langhian Translation")
        .with_icon(icon);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_menu_on_left_click(false);
    let tray = builder
        .build()
        .map_err(|e| format!("Tray builder failed: {e}"))?;

    Ok(TrayHandle {
        _tray: tray,
        actions,
    })
}

impl TrayHandle {
    pub fn rebuild_menu(&mut self, config: &AppConfig, i18n: &Translator) {
        if let Ok((menu, map)) = build_menu(config, i18n) {
            if let Ok(mut guard) = self.actions.lock() {
                *guard = map;
            }
            self._tray.set_menu(Some(Box::new(menu)));
        }
    }
}

fn build_menu(
    config: &AppConfig,
    i18n: &Translator,
) -> Result<(Menu, HashMap<MenuId, TrayAction>), String> {
    let menu = Menu::new();
    let mut actions = HashMap::new();

    let open_item = MenuItem::new(i18n.t("tray.open"), true, None);
    let quick_item = MenuItem::new(i18n.t("tray.quick_translate"), true, None);
    let snip_item = MenuItem::new(i18n.t("tray.screenshot_translate"), true, None);
    let quit_item = MenuItem::new(i18n.t("tray.quit"), true, None);

    actions.insert(open_item.id().clone(), TrayAction::OpenDashboard);
    actions.insert(quick_item.id().clone(), TrayAction::QuickTranslate);
    actions.insert(snip_item.id().clone(), TrayAction::ScreenshotTranslate);
    actions.insert(quit_item.id().clone(), TrayAction::Quit);

    let model_sub = Submenu::new(i18n.t("tray.active_model"), true);
    for m in &config.models {
        let label = if m.supports_vision {
            format!("{} [Vision]", m.name)
        } else {
            m.name.clone()
        };
        let checked = m.id == config.active_model_id;
        let item = CheckMenuItem::new(label, true, checked, None);
        actions.insert(item.id().clone(), TrayAction::SelectModel(m.id.clone()));
        let _ = model_sub.append(&item);
    }

    let prompt_sub = Submenu::new(i18n.t("tray.active_prompt"), true);
    for p in &config.prompts {
        let label = match p.shortcut_slot {
            Some(slot) => format!("[{}] {}", slot, p.name),
            None => p.name.clone(),
        };
        let checked = p.id == config.active_prompt_id;
        let item = CheckMenuItem::new(label, true, checked, None);
        actions.insert(item.id().clone(), TrayAction::SelectPrompt(p.id.clone()));
        let _ = prompt_sub.append(&item);
    }

    menu.append(&open_item).map_err(|e| e.to_string())?;
    menu.append(&quick_item).map_err(|e| e.to_string())?;
    menu.append(&snip_item).map_err(|e| e.to_string())?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| e.to_string())?;
    menu.append(&model_sub).map_err(|e| e.to_string())?;
    menu.append(&prompt_sub).map_err(|e| e.to_string())?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| e.to_string())?;
    menu.append(&quit_item).map_err(|e| e.to_string())?;

    Ok((menu, actions))
}
