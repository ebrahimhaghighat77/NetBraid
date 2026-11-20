use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::app::{self, Core};
use crate::popup;

const ID: &str = "netbraid";
const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png");
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png");

pub struct TrayState {
    power: MenuItem<Wry>,
    shown: Mutex<Option<(bool, String)>>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open NetBraid", true, None::<&str>)?;
    let power = MenuItem::with_id(app, "power", "Turn on", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit NetBraid", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &power, &quit])?;
    app.manage(TrayState {
        power,
        shown: Mutex::new(None),
    });
    TrayIconBuilder::with_id(ID)
        .icon(Image::from_bytes(ICON_OFF)?)
        .tooltip("NetBraid is off")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => popup::show(app),
            "power" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let core = app.state::<Arc<Core>>().inner().clone();
                    let _ = core.set_power(!core.is_on()).await;
                    app::publish(&app, &core);
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                popup::toggle(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Shows on or off in the icon, its tooltip and the menu; only touches them
/// when something changed.
pub fn reflect(app: &AppHandle, on: bool, tooltip: String) {
    let state = app.state::<TrayState>();
    let mut shown = state.shown.lock().unwrap();
    if shown.as_ref() == Some(&(on, tooltip.clone())) {
        return;
    }
    if let Some(tray) = app.tray_by_id(ID) {
        let icon = if on { ICON_ON } else { ICON_OFF };
        if let Ok(image) = Image::from_bytes(icon) {
            let _ = tray.set_icon(Some(image));
        }
        let _ = tray.set_tooltip(Some(&tooltip));
    }
    let _ = state.power.set_text(if on { "Turn off" } else { "Turn on" });
    *shown = Some((on, tooltip));
}
