#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod commands;
mod engine;
mod journal;
mod net;
mod popup;
mod prefs;
mod snapshot;
mod tray;
mod update;
mod usage;
mod win;

fn main() {
    app::run();
}
