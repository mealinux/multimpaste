use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_HOTKEY: &str = "CmdOrCtrl+Shift+V";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// How many clipboard entries to keep.
    pub history_size: usize,
    /// Global shortcut, e.g. "CmdOrCtrl+Shift+V".
    pub hotkey: String,
    /// Launch MultiPaste when the user logs in.
    pub start_at_login: bool,
    /// Send Ctrl/Cmd+V after picking an entry instead of only copying it.
    pub paste_on_select: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            history_size: 10,
            hotkey: DEFAULT_HOTKEY.to_owned(),
            start_at_login: false,
            paste_on_select: true,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("multipaste")
            .join("config.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}

/// Register or unregister the login item. Errors are surfaced to the settings pane.
pub fn apply_start_at_login(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // ponytail: on macOS this registers the raw binary unless MultiPaste.app is installed;
    // bundling the .app (see packaging/) makes the login item show a proper name and icon.
    let launcher = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("MultiPaste")
        .set_app_path(&exe.to_string_lossy())
        .build()
        .map_err(|e| e.to_string())?;

    if enabled {
        launcher.enable().map_err(|e| e.to_string())
    } else {
        launcher.disable().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_json() {
        let json = serde_json::to_string(&Config::default()).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.history_size, 10);
        assert_eq!(back.hotkey, DEFAULT_HOTKEY);
        assert!(back.paste_on_select);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let back: Config = serde_json::from_str(r#"{"history_size": 42}"#).unwrap();
        assert_eq!(back.history_size, 42);
        assert_eq!(back.hotkey, DEFAULT_HOTKEY);
    }

    #[test]
    fn default_hotkey_parses() {
        DEFAULT_HOTKEY
            .parse::<global_hotkey::hotkey::HotKey>()
            .unwrap();
    }
}
