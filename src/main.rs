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
            .with_title("MultiPaste")
            .with_inner_size([460.0, 380.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
            .with_visible(false)
            .with_taskbar(false),
        ..Default::default()
    };

    eframe::run_native(
        "MultiPaste",
        options,
        Box::new(move |cc| Ok(Box::new(app::MultiPaste::new(cc, config)))),
    )
}
