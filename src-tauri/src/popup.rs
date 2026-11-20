use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

pub const LABEL: &str = "popup";
const WIDTH: f64 = 360.0;
const MARGIN: f64 = 12.0;
/// A tray click right after the popup lost focus is the click that hid it.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

pub struct PopupState {
    height: Mutex<f64>,
    hidden_at: Mutex<Option<Instant>>,
}

impl Default for PopupState {
    fn default() -> Self {
        PopupState {
            height: Mutex::new(560.0),
            hidden_at: Mutex::new(None),
        }
    }
}

/// Above the tray: the bottom-right corner of the work area.
fn place(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let height = *app.state::<PopupState>().height.lock().unwrap();
    let (width, height) = ((WIDTH * scale).round(), (height * scale).round());
    let x = f64::from(area.position.x) + f64::from(area.size.width) - width - MARGIN * scale;
    let y = f64::from(area.position.y) + f64::from(area.size.height) - height - MARGIN * scale;
    let _ = window.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

pub fn visible(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    place(app);
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
        *app.state::<PopupState>().hidden_at.lock().unwrap() = Some(Instant::now());
    }
}

pub fn toggle(app: &AppHandle) {
    if visible(app) {
        hide(app);
        return;
    }
    let just_hidden = app
        .state::<PopupState>()
        .hidden_at
        .lock()
        .unwrap()
        .is_some_and(|at| at.elapsed() < REOPEN_GUARD);
    if !just_hidden {
        show(app);
    }
}

#[tauri::command]
pub fn popup_fit(app: AppHandle, height: f64) {
    *app.state::<PopupState>().height.lock().unwrap() = height.clamp(200.0, 900.0);
    place(&app);
}

#[tauri::command]
pub fn popup_hide(app: AppHandle) {
    hide(&app);
}
