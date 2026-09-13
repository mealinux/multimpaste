use crate::clipboard::Clip;
use crate::config::{self, Config};
use crate::services;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Key, Margin, Modifiers, RichText, Sense, Shadow,
    Stroke, Vec2, ViewportCommand,
};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc::{self, Receiver};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Gap between hiding the picker and sending the paste keystroke, so the OS has
/// time to hand focus back to the app the user was typing in.
const FOCUS_RETURN_DELAY: std::time::Duration = std::time::Duration::from_millis(180);

const PICKER_SIZE: Vec2 = Vec2::new(460.0, 400.0);
const SETTINGS_SIZE: Vec2 = Vec2::new(460.0, 470.0);
/// Room around the rounded panel for its drop shadow.
const SHADOW_MARGIN: i8 = 14;
const ROW_HEIGHT: f32 = 36.0;
/// Where the window waits between uses: a single transparent pixel in the corner.
const PARKED: egui::Pos2 = egui::Pos2::new(0.0, 0.0);
const PARKED_SIZE: Vec2 = Vec2::new(1.0, 1.0);
/// Used only if the monitor size is unknown, so the window never opens off screen.
const FALLBACK_POSITION: egui::Pos2 = egui::Pos2::new(120.0, 120.0);
/// A safety net only. The tray, the shortcut and the Service wake the window through
/// their own callbacks, and egui repaints on its own for every mouse and key event.
const HEARTBEAT: std::time::Duration = std::time::Duration::from_millis(400);

/// Something happened outside the egui event loop and the app should look at it.
///
/// The tray, the shortcut and the macOS Service all fire on system threads. They
/// post a signal here and wake the window, rather than the app polling for them:
/// while the window is hidden, eframe's timed repaints stop arriving.
enum Signal {
    ToggleWindow,
    OpenPicker,
    Menu(MenuId),
}

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

pub struct MultimPaste {
    clip: Clip,
    config: Config,
    /// Edited copy shown in the settings pane; only copied back on Save.
    draft: Config,
    view: View,
    visible: bool,
    entries: Vec<String>,
    selected: usize,
    /// Set once the shown window has actually held keyboard focus. Until then a
    /// "not focused" reading means focus has not arrived yet, not that the user
    /// clicked away -- dismissing on it would close the picker the frame it opens.
    saw_focus: bool,
    /// When the paste keystroke is due, if one was asked for.
    paste_at: Option<std::time::Instant>,
    /// Size and position the window was last told to take.
    placement: Option<(Vec2, egui::Pos2)>,
    notice: Option<String>,
    hotkeys: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
    // Both must stay alive for the whole run or the tray entry disappears.
    _tray: Option<TrayIcon>,
    menu: Option<TrayMenu>,
    signals: Receiver<Signal>,
}

impl MultimPaste {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        apply_style(&cc.egui_ctx);

        let clip = Clip::start(config.history_size);
        let hotkeys = GlobalHotKeyManager::new().expect("global hotkey manager");
        let signals = listen(&cc.egui_ctx);
        let (tray, menu) = match build_tray() {
            Ok((tray, menu)) => (Some(tray), Some(menu)),
            Err(e) => {
                eprintln!("multimpaste: tray unavailable: {e}");
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
            saw_focus: false,
            paste_at: None,
            placement: None,
            notice: None,
            hotkeys,
            hotkey: None,
            _tray: tray,
            menu,
            signals,
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

    fn show(&mut self, view: View) {
        self.view = view;
        self.notice = None;
        self.visible = true;
        self.saw_focus = false;
        match view {
            View::Picker => {
                self.entries = self.clip.entries();
                self.selected = 0;
            }
            View::Settings => self.draft = self.config.clone(),
        }
        services::focus_app();
    }

    /// Put the window where the current state wants it, and only when that changes.
    ///
    /// Parked as a 1x1 transparent window in the corner rather than hidden: eframe
    /// never paints a window again once it has been shown and then hidden, which
    /// would leave the tray, the shortcut and the Service dead after the first use.
    /// Parking off screen instead loses the monitor size, and with it the ability to
    /// centre the window when it opens.
    fn place(&mut self, ctx: &egui::Context) {
        let monitor = ctx.input(|i| i.viewport().monitor_size);
        let (size, position) = if self.visible {
            let content = match self.view {
                View::Picker => PICKER_SIZE,
                View::Settings => SETTINGS_SIZE,
            };
            let outer = content + Vec2::splat(f32::from(SHADOW_MARGIN) * 2.0);
            let centred = monitor.map_or(FALLBACK_POSITION, |monitor| {
                ((monitor - outer) * 0.5).to_pos2()
            });
            (outer, centred)
        } else {
            (PARKED_SIZE, PARKED)
        };

        if self.placement == Some((size, position)) {
            return;
        }
        self.placement = Some((size, position));
        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(!self.visible));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
        if self.visible {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    fn hide(&mut self) {
        self.visible = false;
        self.saw_focus = false;
        self.notice = None;
        services::release_focus();
    }

    fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show(View::Picker);
        }
    }

    /// Put the entry back on the clipboard, close the picker, then paste it.
    fn pick(&mut self, index: usize) {
        let Some(text) = self.entries.get(index).cloned() else {
            return;
        };
        self.clip.copy(text);
        self.hide();

        if self.config.paste_on_select {
            self.paste_at = Some(std::time::Instant::now() + FOCUS_RETURN_DELAY);
        }
    }

    /// Send the paste keystroke once the delay is up.
    ///
    /// It has to go out from here rather than from a timer thread: enigo's macOS
    /// backend reads the keyboard layout through Text Input Services, and those trap
    /// the whole process when they are called off the main thread.
    fn pending_paste(&mut self, ctx: &egui::Context) {
        let Some(due) = self.paste_at else {
            return;
        };
        let now = std::time::Instant::now();
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        self.paste_at = None;
        send_paste();
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(signal) = self.signals.try_recv() {
            match signal {
                Signal::ToggleWindow => self.toggle(),
                Signal::OpenPicker => self.show(View::Picker),
                Signal::Menu(id) => {
                    let Some(menu) = &self.menu else { continue };
                    if id == menu.quit {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    } else if id == menu.settings {
                        self.show(View::Settings);
                    } else if id == menu.open {
                        self.show(View::Picker);
                    }
                }
            }
        }
    }

    fn picker_keys(&mut self, ctx: &egui::Context) {
        // ctx here is the picker window's own context.
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
            self.hide();
        } else if let Some(index) = direct {
            self.pick(index);
        } else if confirm {
            self.pick(self.selected);
        } else if delta != 0 && !self.entries.is_empty() {
            let last = self.entries.len() - 1;
            self.selected = match delta {
                d if d > 0 => (self.selected + 1).min(last),
                _ => self.selected.saturating_sub(1),
            };
        }
    }

    fn picker_ui(&mut self, ui: &mut egui::Ui) {
        if header_row(
            ui,
            "Multim Paste",
            &entry_count(self.entries.len()),
            Some("Settings"),
        ) {
            self.show(View::Settings);
            return;
        }
        ui.add_space(10.0);

        if self.entries.is_empty() {
            ui.add_space(56.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Nothing copied yet")
                        .size(15.0)
                        .color(ui.visuals().text_color()),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Copy something, then open this window again.")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );
            });
        } else {
            let mut chosen = None;
            let mut hovered = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for (index, entry) in self.entries.iter().enumerate() {
                        let response =
                            entry_row(ui, index, &preview(entry), index == self.selected);
                        if response.clicked() {
                            chosen = Some(index);
                        }
                        if response.hovered() {
                            hovered = Some(index);
                        }
                        if index == self.selected && response.rect.height() > 0.0 {
                            response.scroll_to_me(None);
                        }
                    }
                });
            if let Some(index) = hovered {
                self.selected = index;
            }
            if let Some(index) = chosen {
                self.pick(index);
            }
        }

        footer(ui, "↑↓ move · ⏎ paste · 1-9 quick pick · esc close");
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        header_row(ui, "Settings", "", None);
        ui.add_space(10.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Entries to keep");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.draft.history_size).range(1..=200),
                            );
                        });
                    });
                    hint(ui, "How many past copies stay in the list.");
                });

                card(ui, |ui| {
                    ui.label("Global shortcut");
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.hotkey)
                            .desired_width(f32::INFINITY)
                            .margin(Margin::symmetric(8, 6)),
                    );
                    hint(ui, "Modifiers: CmdOrCtrl, Ctrl, Alt, Shift, Cmd.");
                });

                card(ui, |ui| {
                    ui.checkbox(&mut self.draft.start_at_login, "Start when I log in");
                    ui.add_space(2.0);
                    ui.checkbox(&mut self.draft.paste_on_select, "Paste right after picking");
                    hint(
                        ui,
                        "With this off, picking only copies and you paste yourself.",
                    );
                });
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Clear history").clicked() {
                self.clip.clear();
                self.entries.clear();
                self.notice = Some("History cleared.".to_owned());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    self.save_settings();
                }
                if ui.button("Back").clicked() {
                    self.draft = self.config.clone();
                    self.show(View::Picker);
                }
            });
        });
    }

    fn save_settings(&mut self) {
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
    }
}

impl eframe::App for MultimPaste {
    /// Transparent, so the panel keeps its rounded corners and shadow.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(HEARTBEAT);
        self.pending_paste(&ctx);
        self.place(&ctx);
        self.drain_events(&ctx);

        if !self.visible {
            return;
        }

        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(false);
        self.saw_focus |= focused;

        if self.view == View::Picker {
            self.picker_keys(&ctx);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.show(View::Picker);
        }

        panel(ui).show(ui, |ui| {
            match self.view {
                View::Picker => self.picker_ui(ui),
                View::Settings => self.settings_ui(ui),
            }
            if let Some(notice) = &self.notice {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(notice)
                        .size(12.0)
                        .color(ui.visuals().hyperlink_color),
                );
            }
        });

        // Clicking another window should dismiss the picker, the way a menu does --
        // but only once the window has had focus to lose.
        if self.view == View::Picker && self.saw_focus && !focused {
            self.hide();
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

/// Route every system event source into one channel, waking the window each time.
///
/// Only the tray *menu* is listened to. Raw tray clicks are deliberately ignored: a
/// left click already opens the menu, and on macOS that click arrives after the menu
/// selection, which would undo whatever the user just chose.
fn listen(ctx: &egui::Context) -> Receiver<Signal> {
    let (sender, receiver) = mpsc::channel();

    let (hotkey_sender, hotkey_ctx) = (sender.clone(), ctx.clone());
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed && hotkey_sender.send(Signal::ToggleWindow).is_ok() {
            hotkey_ctx.request_repaint();
        }
    }));

    let (menu_sender, menu_ctx) = (sender.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if menu_sender.send(Signal::Menu(event.id)).is_ok() {
            menu_ctx.request_repaint();
        }
    }));

    let service_ctx = ctx.clone();
    services::install(move || {
        if sender.send(Signal::OpenPicker).is_ok() {
            service_ctx.request_repaint();
        }
    });

    receiver
}

fn entry_count(count: usize) -> String {
    match count {
        1 => "1 entry".to_owned(),
        n => format!("{n} entries"),
    }
}

/// Rounded widgets and roomier spacing than the egui defaults.
fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 26.0;

        let widgets = &mut style.visuals.widgets;
        for visual in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            visual.corner_radius = CornerRadius::same(8);
        }
        style.visuals.window_corner_radius = CornerRadius::same(14);
        style.visuals.menu_corner_radius = CornerRadius::same(10);
    });
}

/// The rounded card the whole window is drawn inside.
fn panel(ui: &egui::Ui) -> egui::Frame {
    let visuals = ui.visuals();
    egui::Frame::NONE
        .fill(visuals.window_fill)
        .stroke(visuals.window_stroke)
        .corner_radius(CornerRadius::same(14))
        .shadow(Shadow {
            offset: [0, 6],
            blur: 20,
            spread: 0,
            color: Color32::from_black_alpha(70),
        })
        .outer_margin(Margin::same(SHADOW_MARGIN))
        .inner_margin(Margin::symmetric(16, 14))
}

/// Title, a muted subtitle, and an optional button pinned to the right edge.
fn header_row(ui: &mut egui::Ui, title: &str, subtitle: &str, button: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(16.0).strong());
        if !subtitle.is_empty() {
            ui.label(
                RichText::new(subtitle)
                    .size(12.0)
                    .color(ui.visuals().weak_text_color()),
            );
        }
        if let Some(label) = button {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clicked = ui
                    .add(egui::Button::new(RichText::new(label).size(12.0)))
                    .clicked();
            });
        }
    });
    clicked
}

fn footer(ui: &mut egui::Ui, hints: &str) {
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(
        RichText::new(hints)
            .size(11.0)
            .color(ui.visuals().weak_text_color()),
    );
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text)
            .size(11.0)
            .color(ui.visuals().weak_text_color()),
    );
}

/// A settings section, boxed so related controls read as one group.
fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let visuals = ui.visuals();
    egui::Frame::NONE
        .fill(visuals.faint_bg_color)
        .stroke(Stroke::new(
            1.0,
            visuals.widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, add_contents);
    ui.add_space(8.0);
}

/// One history row: index badge on the left, single-line preview beside it.
fn entry_row(ui: &mut egui::Ui, index: usize, text: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let visuals = ui.visuals();
    let background = if selected {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    if background != Color32::TRANSPARENT {
        ui.painter().rect_filled(
            rect.shrink2(Vec2::new(0.0, 1.0)),
            CornerRadius::same(9),
            background,
        );
    }

    let text_color = if selected {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let badge_color = if selected {
        text_color.gamma_multiply(0.75)
    } else {
        visuals.weak_text_color()
    };

    let painter = ui.painter().with_clip_rect(rect);
    if index < 9 {
        painter.text(
            rect.left_center() + Vec2::new(12.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{}", index + 1),
            FontId::monospace(11.0),
            badge_color,
        );
    }
    painter.text(
        rect.left_center() + Vec2::new(32.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.5),
        text_color,
    );
    response
}

/// One tidy line per entry: newlines and runs of spaces collapse, long text is cut.
fn preview(text: &str) -> String {
    const MAX: usize = 54;
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
    let open = MenuItem::new("Multim Paste", true, None);
    let settings = MenuItem::new("Settings…", true, None);
    let quit = MenuItem::new("Quit MultimPaste", true, None);
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
        .with_tooltip("MultimPaste")
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
    use super::{entry_count, preview};

    #[test]
    fn preview_collapses_whitespace_into_one_line() {
        assert_eq!(preview("hello\n\n  world\t!"), "hello world !");
    }

    #[test]
    fn preview_truncates_long_text() {
        let long = "x".repeat(200);
        let shown = preview(&long);
        assert_eq!(shown.chars().count(), 54);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn preview_counts_characters_not_bytes() {
        // Multi-byte input must not panic or cut a character in half.
        let turkish = "şğüöçı".repeat(30);
        assert_eq!(preview(&turkish).chars().count(), 54);
    }

    #[test]
    fn entry_count_is_singular_for_one() {
        assert_eq!(entry_count(1), "1 entry");
        assert_eq!(entry_count(0), "0 entries");
        assert_eq!(entry_count(12), "12 entries");
    }
}
