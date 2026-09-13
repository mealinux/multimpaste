# MultimPaste

Paste something you copied *earlier*.

MultimPaste keeps your recent clipboard entries and lets you pick one from a small list,
then pastes it for you. It lives in the menu bar / system tray, weighs a few megabytes,
and stores nothing outside your machine.

Works on **macOS**, **Windows** and **Linux**. Written in Rust, MIT licensed.

---

## Install

### macOS

Download `MultimPaste-macos.zip` from [Releases](../../releases), unzip it, and move
`MultimPaste.app` into `/Applications`. Open it once.

macOS will ask for **Accessibility** permission the first time MultimPaste pastes for you
(System Settings → Privacy & Security → Accessibility). Without it MultimPaste still copies
your pick to the clipboard, you just press <kbd>Cmd</kbd>+<kbd>V</kbd> yourself.

### Windows

Download `MultimPaste-windows.zip` from [Releases](../../releases), unzip it, and run
`multimpaste.exe`. No installer, no admin rights.

### Linux

Download `MultimPaste-linux.tar.gz` from [Releases](../../releases):

```sh
tar -xzf MultimPaste-linux.tar.gz
install -D multimpaste ~/.local/bin/multimpaste
install -D multimpaste.desktop ~/.local/share/applications/multimpaste.desktop
```

A tray icon needs a StatusNotifier host — GNOME users want the *AppIndicator* extension.

---

## Using it

Two ways to open the picker, both showing the same list:

| | |
|---|---|
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
| Start when I log in | off | On macOS this is a LaunchAgent, so it shows under System Settings → General → Login Items → *Allow in the Background*, not in the login items list above it. |
| Paste immediately after picking | on | Turn it off to only copy, and paste yourself. |

Settings live in a small JSON file (the path is shown at the bottom of the Settings pane):

- macOS `~/Library/Application Support/multimpaste/config.json`
- Windows `%APPDATA%\multimpaste\config.json`
- Linux `~/.config/multimpaste/config.json`

---

## Why there is no right-click entry

Short version: no application can put an item in another application's right-click
menu on macOS, Windows or Linux. Use the global shortcut — it works in every app and
every text field.

The longer version, because it is worth knowing what was actually tried.

The menu you get when you right-click in a text field is built by the application you
are in, not by the operating system, and none of the three desktops offers a supported
way to add to it. On macOS the whole menu comes from AppKit, and its contents are
fixed:

```
Look Up · Translate · Search With Google
Cut · Copy · Paste · Paste and Match Style
Share…
Font ▸ Spelling and Grammar ▸ Substitutions ▸ Transformations ▸ Speech ▸ Layout Orientation ▸
```

There is no extension point in that list, and — as of macOS 15 — not even a *Services*
submenu to hang one off. A **Service** is still the only way one app can reach another
app's menus at all, so MultimPaste registers one, and you will find **Multim Paste**
wherever an app does offer a Services menu: the menu bar, under the application's own
menu. It is worth opening System Settings → Keyboard → Keyboard Shortcuts → Services
once, where you can give it a key combination of its own.

On **Windows and Linux** the same is true for the same reason. The only ways in would be
injecting code into other processes — which breaks constantly and looks exactly like
malware to an antivirus.

## Accessibility permission on macOS

Pressing <kbd>Cmd</kbd>+<kbd>V</kbd> for you counts as controlling your computer, so macOS
asks for **Accessibility** permission the first time MultimPaste pastes. Without it nothing
breaks: your pick still goes to the clipboard and the picker says so — you just press
<kbd>Cmd</kbd>+<kbd>V</kbd> yourself.

Because these builds are ad-hoc signed rather than signed with a Developer ID, macOS sees
every new build as a different app, and the permission has to be granted again after an
update. To clear a stale entry:

```sh
tccutil reset Accessibility com.multimpaste.app
```

Then open MultimPaste, paste once, and allow it when asked.

## Build from source

Needs a [Rust toolchain](https://rustup.rs). On Debian/Ubuntu also:

```sh
sudo apt install libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libxkbcommon-dev \
                 libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

Then:

```sh
cargo test
cargo build --release          # -> target/release/multimpaste
./packaging/macos/bundle.sh    # macOS: -> target/MultimPaste.app
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
leaves your machine — quitting MultimPaste forgets all of it.

## License

[MIT](LICENSE).
