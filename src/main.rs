// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod clipboard;
mod config;
mod services;

use eframe::egui;

fn main() -> eframe::Result {
    let config = config::Config::load();

    // The window exists for the whole session and is only hidden, so the tray icon,
    // the global shortcut and the clipboard watcher all keep running behind it.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MultimPaste")
            .with_inner_size([1.0, 1.0])
            .with_decorations(false)
            // Transparent so the app can paint its own rounded panel and drop shadow.
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false)
            .with_taskbar(false)
            // Parked off screen rather than hidden; see app::MultimPaste::place.
            .with_position([0.0, 0.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MultimPaste",
        options,
        Box::new(move |cc| Ok(Box::new(app::MultimPaste::new(cc, config)))),
    )
}
