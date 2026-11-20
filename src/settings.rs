//! The app's own settings, kept in `%APPDATA%\dbyb\settings.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Name of the theme in use.
    pub theme: String,
    /// Size of the whole interface, 1.0 = normal.
    pub scale: f32,
    /// Name of the icon pack (a folder in the icon packs folder), or "Default".
    pub icon_pack: String,
    /// Icon size in points.
    pub icon_size: f32,
    /// A fixed colour for the icons; None follows the theme.
    pub icon_color: Option<[u8; 3]>,
    /// "System" (the Windows cursor), a built-in style or a folder of the cursors folder.
    pub cursor_style: String,
    pub cursor_size: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: crate::theme::DEFAULT_THEME.into(), scale: 1.0, icon_pack: crate::icons::DEFAULT_PACK.into(), icon_size: 20.0, icon_color: None, cursor_style: crate::cursor::SYSTEM.into(), cursor_size: 28.0 }
    }
}

fn path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("dbyb");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let (Some(path), Ok(json)) = (path(), serde_json::to_string_pretty(self)) {
            let _ = std::fs::write(path, json);
        }
    }
}
