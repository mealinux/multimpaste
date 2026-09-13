use crate::clipboard::Clip;
use crate::config::{self, Config};
use crate::services;
use eframe::egui::{self, Color32, Key, Modifiers, RichText, ViewportCommand};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Gap between hiding the picker and sending the paste keystroke, so the OS has
/// time to hand focus back to the app the user was typing in.
const FOCUS_RETURN_DELAY: std::time::Duration = std::time::Duration::from_millis(180);

#[derive(PartialEq, Clone, Copy)]
enum View {
    Picker,
    Settings,
}

struct TrayMenu {
    open: MenuId,
    settings: MenuId,
    quit: MenuId,
}

pub struct MultiPaste {
    clip: Clip,
    config: Config,
    /// Edited copy shown in the settings pane; only copied back on Save.
    draft: Config,
    view: View,
    visible: bool,
    entries: Vec<String>,
    selected: usize,
    notice: Option<String>,
    hotkeys: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
    // Both must stay alive for the whole run or the tray entry disappears.
    _tray: Option<TrayIcon>,
    menu: Option<TrayMenu>,
    /// Fires when the user chooses MultiPaste from an app's context menu (macOS).
    context_menu: services::Trigger,
}

impl MultiPaste {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let clip = Clip::start(config.history_size);
        let hotkeys = GlobalHotKeyManager::new().expect("global hotkey manager");
        let (tray, menu) = match build_tray() {
            Ok((tray, menu)) => (Some(tray), Some(menu)),
            Err(e) => {
                eprintln!("multipaste: tray unavailable: {e}");
                (None, None)
            }
        };

        let mut app = Self {
            clip,
            draft: config.clone(),
            config,
            view: View::Picker,
            visible: false,
            entries: Vec::new(),
            selected: 0,
            notice: None,
            hotkeys,
            hotkey: None,
            _tray: tray,
            menu,
            context_menu: services::install(),
        };
        if let Err(e) = app.register_hotkey(&app.config.hotkey.clone()) {
            app.notice = Some(e);
        }
        app
    }

    fn register_hotkey(&mut self, spec: &str) -> Result<(), String> {
        let hotkey: HotKey = spec
            .parse()
            .map_err(|e| format!("Could not parse shortcut \"{spec}\": {e}"))?;
        if let Some(old) = self.hotkey.take() {
            let _ = self.hotkeys.unregister(old);
        }
        self.hotkeys
            .register(hotkey)
            .map_err(|e| format!("Could not register shortcut \"{spec}\": {e}"))?;
        self.hotkey = Some(hotkey);
        Ok(())
    }

    fn show(&mut self, ctx: &egui::Context, view: View) {
        self.view = view;
        self.entries = self.clip.entries();
        self.selected = 0;
        self.visible = true;
        if view == View::Settings {
            self.draft = self.config.clone();
        }

        let size = match view {
            View::Picker => egui::vec2(460.0, 380.0),
            View::Settings => egui::vec2(460.0, 420.0),
        };
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        if let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) {
            let pos = ((monitor - size) * 0.5).to_pos2();
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.notice = None;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    fn toggle(&mut self, ctx: &egui::Context) {
        if self.visible {
            self.hide(ctx);
        } else {
            self.show(ctx, View::Picker);
        }
    }

    /// Put the entry back on the clipboard, close the picker, then paste it.
    fn pick(&mut self, ctx: &egui::Context, index: usize) {
        let Some(text) = self.entries.get(index).cloned() else {
            return;
        };
        self.clip.copy(text);
        self.hide(ctx);

        if self.config.paste_on_select {
            std::thread::spawn(|| {
                std::thread::sleep(FOCUS_RETURN_DELAY);
                send_paste();
            });
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                self.toggle(ctx);
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let Some(menu) = &self.menu else { continue };
            if event.id == menu.quit {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            } else if event.id == menu.settings {
                self.show(ctx, View::Settings);
            } else if event.id == menu.open {
                self.show(ctx, View::Picker);
            }
        }

        while self.context_menu.try_recv().is_ok() {
            self.show(ctx, View::Picker);
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button, .. } = event {
                if button == tray_icon::MouseButton::Left {
                    self.show(ctx, View::Picker);
                }
            }
        }
    }

    fn picker_keys(&mut self, ctx: &egui::Context) {
        let (mut close, mut confirm, mut delta, mut direct) = (false, false, 0i32, None);
        ctx.input_mut(|i| {
            close = i.consume_key(Modifiers::NONE, Key::Escape);
            confirm = i.consume_key(Modifiers::NONE, Key::Enter);
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                delta += 1;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                delta -= 1;
            }
            for (offset, key) in NUMBER_KEYS.iter().enumerate() {
                if i.consume_key(Modifiers::NONE, *key) {
                    direct = Some(offset);
                }
            }
        });

        if close {
            self.hide(ctx);
        } else if let Some(index) = direct {
            self.pick(ctx, index);
        } else if confirm {
            self.pick(ctx, self.selected);
        } else if delta != 0 && !self.entries.is_empty() {
            let last = self.entries.len() - 1;
            self.selected = match delta {
                d if d > 0 => (self.selected + 1).min(last),
                _ => self.selected.saturating_sub(1),
            };
        }
    }

    fn picker_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Multi Paste");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    self.show(ctx, View::Settings);
                }
            });
        });
        ui.label(
            RichText::new("Enter pastes · 1-9 picks directly · Esc closes")
                .small()
                .color(Color32::GRAY),
        );
        ui.separator();

        if self.entries.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label("Nothing copied yet. Copy something and open this window again.");
            });
            return;
        }

        let mut chosen = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (index, entry) in self.entries.iter().enumerate() {
                let shortcut = if index < 9 {
                    format!("{}.", index + 1)
                } else {
                    "  ".to_owned()
                };
                let label = format!("{shortcut} {}", preview(entry));
                let row = ui.selectable_label(index == self.selected, label);
                if row.clicked() {
                    chosen = Some(index);
                }
                if index == self.selected {
                    row.scroll_to_me(None);
                }
            }
        });
        if let Some(index) = chosen {
            self.pick(ctx, index);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Settings");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Entries to keep");
            ui.add(egui::DragValue::new(&mut self.draft.history_size).range(1..=200));
        });

        ui.horizontal(|ui| {
            ui.label("Global shortcut");
            ui.text_edit_singleline(&mut self.draft.hotkey);
        });
        ui.label(
            RichText::new(
                "Modifiers: CmdOrCtrl, Ctrl, Alt, Shift, Cmd. Example: CmdOrCtrl+Shift+V",
            )
            .small()
            .color(Color32::GRAY),
        );

        ui.add_space(8.0);
        ui.checkbox(&mut self.draft.start_at_login, "Start when I log in");
        ui.checkbox(
            &mut self.draft.paste_on_select,
            "Paste immediately after picking (otherwise only copy)",
        );

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.save_settings(ctx);
            }
            if ui.button("Cancel").clicked() {
                self.draft = self.config.clone();
                self.show(ctx, View::Picker);
            }
            if ui.button("Clear history").clicked() {
                self.clip.clear();
                self.entries.clear();
                self.notice = Some("History cleared.".to_owned());
            }
        });

        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Config file: {}", Config::path().display()))
                .small()
                .color(Color32::GRAY),
        );
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        if let Err(e) = self.register_hotkey(&self.draft.hotkey.clone()) {
            self.notice = Some(e);
            return;
        }
        if self.draft.start_at_login != self.config.start_at_login {
            if let Err(e) = config::apply_start_at_login(self.draft.start_at_login) {
                self.notice = Some(format!("Could not change the login item: {e}"));
                self.draft.start_at_login = self.config.start_at_login;
                return;
            }
        }

        self.clip.set_limit(self.draft.history_size);
        self.config = self.draft.clone();
        self.notice = match self.config.save() {
            Ok(()) => Some("Saved.".to_owned()),
            Err(e) => Some(format!("Saved in memory, but writing the file failed: {e}")),
        };
        let _ = ctx;
    }
}

impl eframe::App for MultiPaste {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // ponytail: tray and hotkey crates deliver events on channels, so the app polls
        // them on a timer instead of wiring a custom winit event loop.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        self.drain_events(&ctx);

        if !self.visible {
            return;
        }
        if self.view == View::Picker {
            self.picker_keys(&ctx);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.show(&ctx, View::Picker);
        }

        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            match self.view {
                View::Picker => self.picker_ui(ui, &ctx),
                View::Settings => self.settings_ui(ui, &ctx),
            }
            if let Some(notice) = &self.notice {
                ui.add_space(6.0);
                ui.label(RichText::new(notice).color(Color32::LIGHT_BLUE));
            }
        });

        // Clicking another window should dismiss the picker, the way a menu does.
        if self.view == View::Picker && ctx.input(|i| i.viewport().focused == Some(false)) {
            self.hide(&ctx);
        }
    }
}

const NUMBER_KEYS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

/// One tidy line per entry: newlines and runs of spaces collapse, long text is cut.
fn preview(text: &str) -> String {
    const MAX: usize = 72;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX {
        return flat;
    }
    flat.chars().take(MAX - 1).collect::<String>() + "…"
}

fn send_paste() {
    use enigo::{Direction, Enigo, Key as EKey, Keyboard, Settings};

    // ponytail: on macOS this needs Accessibility permission; without it the keystroke
    // is silently dropped and the entry is still on the clipboard for a manual paste.
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let modifier = if cfg!(target_os = "macos") {
        EKey::Meta
    } else {
        EKey::Control
    };
    let _ = enigo.key(modifier, Direction::Press);
    let _ = enigo.key(EKey::Unicode('v'), Direction::Click);
    let _ = enigo.key(modifier, Direction::Release);
}

fn build_tray() -> Result<(TrayIcon, TrayMenu), Box<dyn std::error::Error>> {
    let open = MenuItem::new("Multi Paste", true, None);
    let settings = MenuItem::new("Settings…", true, None);
    let quit = MenuItem::new("Quit MultiPaste", true, None);
    let ids = TrayMenu {
        open: open.id().clone(),
        settings: settings.id().clone(),
        quit: quit.id().clone(),
    };

    let menu = Menu::new();
    menu.append(&open)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&settings)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("MultiPaste")
        .with_menu(Box::new(menu))
        .with_icon(tray_icon_image())
        .with_icon_as_template(true)
        .build()?;

    Ok((tray, ids))
}

/// Two overlapping sheets, drawn in code so the binary needs no image decoder.
fn tray_icon_image() -> tray_icon::Icon {
    const SIZE: usize = 32;
    let mut pixels = vec![0u8; SIZE * SIZE * 4];

    let mut set = |x: usize, y: usize, opaque: bool| {
        let i = (y * SIZE + x) * 4;
        pixels[i..i + 4].copy_from_slice(&[0, 0, 0, if opaque { 255 } else { 0 }]);
    };
    let mut sheet = |x0: usize, y0: usize, x1: usize, y1: usize| {
        for y in y0..y1 {
            for x in x0..x1 {
                let edge = x < x0 + 2 || x + 2 >= x1 || y < y0 + 2 || y + 2 >= y1;
                set(x, y, edge);
            }
        }
    };

    sheet(3, 3, 21, 25);
    sheet(11, 7, 29, 29);

    tray_icon::Icon::from_rgba(pixels, SIZE as u32, SIZE as u32).expect("tray icon")
}

#[cfg(test)]
mod tests {
    use super::preview;

    #[test]
    fn preview_collapses_whitespace_into_one_line() {
        assert_eq!(preview("hello\n\n  world\t!"), "hello world !");
    }

    #[test]
    fn preview_truncates_long_text() {
        let long = "x".repeat(200);
        let shown = preview(&long);
        assert_eq!(shown.chars().count(), 72);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn preview_counts_characters_not_bytes() {
        // Multi-byte input must not panic or cut a character in half.
        let turkish = "şğüöçı".repeat(30);
        assert_eq!(preview(&turkish).chars().count(), 72);
    }
}
