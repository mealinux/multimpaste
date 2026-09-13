# MultiPaste

Paste something you copied *earlier*.

MultiPaste keeps your recent clipboard entries and lets you pick one from a small list,
then pastes it for you. It lives in the menu bar / system tray, weighs a few megabytes,
and stores nothing outside your machine.

Works on **macOS**, **Windows** and **Linux**. Written in Rust, MIT licensed.

---

## Install

### macOS

Download `MultiPaste-macos.zip` from [Releases](../../releases), unzip it, and move
`MultiPaste.app` into `/Applications`. Open it once.

macOS will ask for **Accessibility** permission the first time MultiPaste pastes for you
(System Settings → Privacy & Security → Accessibility). Without it MultiPaste still copies
your pick to the clipboard, you just press <kbd>Cmd</kbd>+<kbd>V</kbd> yourself.

### Windows

Download `MultiPaste-windows.zip` from [Releases](../../releases), unzip it, and run
`multipaste.exe`. No installer, no admin rights.

### Linux

Download `MultiPaste-linux.tar.gz` from [Releases](../../releases):

```sh
tar -xzf MultiPaste-linux.tar.gz
install -D multipaste ~/.local/bin/multipaste
install -D multipaste.desktop ~/.local/share/applications/multipaste.desktop
```

A tray icon needs a StatusNotifier host — GNOME users want the *AppIndicator* extension.

---

## Using it

Three ways to open the picker, all showing the same list:

| | |
|---|---|
| **Right-click → Services → Multi Paste** | In any editable text field. macOS only, see the note below. |
| **<kbd>Cmd/Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>** | Anywhere. Change it in Settings. |
| **Menu bar / tray icon** | Click it, or use its menu. |

In the picker:

- <kbd>↑</kbd> <kbd>↓</kbd> to move, <kbd>Enter</kbd> to paste
- <kbd>1</kbd>–<kbd>9</kbd> to paste that entry straight away
- Click an entry to paste it
- <kbd>Esc</kbd>, or clicking elsewhere, closes it

## Settings

Open Settings from the tray menu or the button in the picker.

| Setting | Default | |
|---|---|---|
| Entries to keep | `10` | Anything from 1 to 200. |
| Global shortcut | `CmdOrCtrl+Shift+V` | Modifiers: `CmdOrCtrl`, `Ctrl`, `Alt`, `Shift`, `Cmd`. |
| Start when I log in | off | Registers MultiPaste as a login item. |
| Paste immediately after picking | on | Turn it off to only copy, and paste yourself. |

Settings live in a small JSON file (the path is shown at the bottom of the Settings pane):

- macOS `~/Library/Application Support/multipaste/config.json`
- Windows `%APPDATA%\multipaste\config.json`
- Linux `~/.config/multipaste/config.json`

---

## About the right-click entry

On macOS, MultiPaste registers a system **Service**, so it shows up when you right-click
in a text field — inside the **Services** submenu, a little below *Paste*. That submenu is
the only supported way for one app to add an item to every other app's context menu.
If it does not appear, open System Settings → Keyboard → Keyboard Shortcuts → Services
and tick **Multi Paste** under *Text*. You can give it its own shortcut there too.

On **Windows and Linux there is no right-click entry**, and this is not a missing feature:
every application draws that menu itself, and neither OS offers a supported hook into it.
The only ways in would be injecting code into other processes — which breaks constantly and
looks exactly like malware to an antivirus. The global shortcut does the same job everywhere.

---

## Build from source

Needs a [Rust toolchain](https://rustup.rs). On Debian/Ubuntu also:

```sh
sudo apt install libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libxkbcommon-dev \
                 libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

Then:

```sh
cargo test
cargo build --release          # -> target/release/multipaste
./packaging/macos/bundle.sh    # macOS: -> target/MultiPaste.app
```

The macOS `.app` wrapper is what registers the right-click Service; a bare binary cannot.

## How it works

| File | |
|---|---|
| [`src/clipboard.rs`](src/clipboard.rs) | One background thread owns the clipboard and records every change. |
| [`src/app.rs`](src/app.rs) | The picker and Settings window, the tray icon, the global shortcut. |
| [`src/services.rs`](src/services.rs) | The macOS Service behind the right-click entry. |
| [`src/config.rs`](src/config.rs) | Reading, writing and applying settings. |

Your clipboard history is kept in memory only. It is never written to disk and never
leaves your machine — quitting MultiPaste forgets all of it.

## License

[MIT](LICENSE).
