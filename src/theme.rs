//! Colours and themes. The colours can change while the app runs; themes are small JSON files.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// A colour as `#rrggbb` in theme files.
mod hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(color: &[u8; 3], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 3], D::Error> {
        let text = String::deserialize(d)?;
        let digits = text.trim().trim_start_matches('#');
        if digits.len() != 6 {
            return Err(serde::de::Error::custom(format!("`{text}` is not a #rrggbb colour")));
        }
        let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).map_err(serde::de::Error::custom);
        Ok([channel(0)?, channel(2)?, channel(4)?])
    }
}

/// The ten colours of the interface.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Colors {
    /// Server icons column.
    #[serde(with = "hex")]
    pub rail: [u8; 3],
    /// Channel list.
    #[serde(with = "hex")]
    pub sidebar: [u8; 3],
    /// Chat background.
    #[serde(with = "hex")]
    pub bg: [u8; 3],
    /// Text boxes and cards.
    #[serde(with = "hex")]
    pub input: [u8; 3],
    /// Hovered or selected rows.
    #[serde(with = "hex")]
    pub hover: [u8; 3],
    #[serde(with = "hex")]
    pub line: [u8; 3],
    /// Main text.
    #[serde(with = "hex")]
    pub ink: [u8; 3],
    /// Secondary text.
    #[serde(with = "hex")]
    pub muted: [u8; 3],
    /// Faint text and badges.
    #[serde(with = "hex")]
    pub dim: [u8; 3],
    /// Errors.
    #[serde(with = "hex")]
    pub bad: [u8; 3],
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(flatten)]
    pub colors: Colors,
}

const fn colors(rail: [u8; 3], sidebar: [u8; 3], bg: [u8; 3], input: [u8; 3], hover: [u8; 3], line: [u8; 3], ink: [u8; 3], muted: [u8; 3], dim: [u8; 3], bad: [u8; 3]) -> Colors {
    Colors { rail, sidebar, bg, input, hover, line, ink, muted, dim, bad }
}

const DARK: Colors = colors([30, 31, 34], [43, 45, 49], [49, 51, 56], [56, 58, 64], [64, 66, 72], [63, 65, 70], [219, 222, 225], [148, 155, 164], [110, 115, 122], [240, 110, 110]);
const BLACK: Colors = colors([0, 0, 0], [10, 10, 10], [0, 0, 0], [24, 24, 24], [36, 36, 36], [40, 40, 40], [235, 235, 235], [140, 140, 140], [90, 90, 90], [230, 110, 110]);
const LIGHT: Colors = colors([227, 229, 232], [242, 243, 245], [255, 255, 255], [235, 237, 239], [222, 224, 228], [210, 212, 216], [46, 51, 56], [94, 103, 114], [128, 137, 148], [215, 55, 60]);
const MIDNIGHT: Colors = colors([14, 17, 28], [20, 25, 42], [26, 32, 52], [34, 41, 66], [45, 54, 84], [45, 54, 84], [225, 230, 245], [140, 152, 185], [100, 112, 150], [240, 110, 120]);

pub const DEFAULT_THEME: &str = "Dark grey";

pub fn built_in() -> Vec<Theme> {
    vec![
        Theme { name: DEFAULT_THEME.into(), colors: DARK },
        Theme { name: "Black".into(), colors: BLACK },
        Theme { name: "Light".into(), colors: LIGHT },
        Theme { name: "Midnight".into(), colors: MIDNIGHT },
    ]
}

static CURRENT: RwLock<Colors> = RwLock::new(DARK);

fn current() -> Colors {
    CURRENT.read().map(|c| *c).unwrap_or(DARK)
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn rail() -> Color32 {
    rgb(current().rail)
}
pub fn sidebar() -> Color32 {
    rgb(current().sidebar)
}
pub fn bg() -> Color32 {
    rgb(current().bg)
}
pub fn input() -> Color32 {
    rgb(current().input)
}
pub fn hover() -> Color32 {
    rgb(current().hover)
}
pub fn line() -> Color32 {
    rgb(current().line)
}
pub fn ink() -> Color32 {
    rgb(current().ink)
}
pub fn muted() -> Color32 {
    rgb(current().muted)
}
pub fn dim() -> Color32 {
    rgb(current().dim)
}
pub fn bad() -> Color32 {
    rgb(current().bad)
}

/// Where user themes live: `%APPDATA%\dbyb\themes`.
pub fn themes_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("dbyb").join("themes");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// The built-in themes, then the ones found in the themes folder. A broken file is skipped, not fatal.
pub fn load_all() -> Vec<Theme> {
    let mut themes = built_in();
    if let Some(dir) = themes_dir() {
        let mut files: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort();
        for path in files {
            if let Some(theme) = std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<Theme>(&text).ok()) {
                themes.retain(|t| t.name != theme.name);
                themes.push(theme);
            }
        }
    }
    themes
}

/// Writes a theme to the themes folder and returns the file name.
pub fn save(theme: &Theme) -> Result<String, String> {
    let dir = themes_dir().ok_or("Cannot find the themes folder.")?;
    let stem: String = theme.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let file = format!("{}.json", stem.trim_matches('-').to_lowercase());
    let json = serde_json::to_string_pretty(theme).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&file), json).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Makes `colors` the colours in use. Call `apply_visuals` afterwards so egui follows.
pub fn set(colors: Colors) {
    if let Ok(mut current) = CURRENT.write() {
        *current = colors;
    }
}

pub fn set_fonts(ctx: &egui::Context) {
    // Segoe UI ships with every Windows: use it ahead of the built-in font, and Segoe UI Emoji as a fallback
    let mut fonts = FontDefinitions::default();
    if let Ok(data) = std::fs::read("C:/Windows/Fonts/segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(data)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    if let Ok(data) = std::fs::read("C:/Windows/Fonts/seguiemj.ttf") {
        fonts.font_data.insert("segoe-emoji".into(), Arc::new(FontData::from_owned(data)));
        fonts.families.entry(FontFamily::Proportional).or_default().push("segoe-emoji".into());
    }
    // a real bold: Segoe UI Bold under the family name "bold" (the plain font if it is missing)
    let mut bold = vec![];
    if let Ok(data) = std::fs::read("C:/Windows/Fonts/segoeuib.ttf") {
        fonts.font_data.insert("segoe-bold".into(), Arc::new(FontData::from_owned(data)));
        bold.push("segoe-bold".to_string());
    }
    bold.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
}

pub fn apply_visuals(ctx: &egui::Context) {
    let c = current();
    let luminance = |p: [u8; 3]| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u32;
    let mut v = if luminance(c.bg) > 140 { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = bg();
    v.window_fill = sidebar();
    v.extreme_bg_color = input();
    v.faint_bg_color = sidebar();
    v.override_text_color = Some(ink());
    v.selection.bg_fill = hover();
    v.selection.stroke = Stroke::new(1.0, ink());
    let radius = CornerRadius::same(4);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = radius;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line());
    v.widgets.inactive.bg_fill = input();
    v.widgets.inactive.weak_bg_fill = input();
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = hover();
    v.widgets.hovered.weak_bg_fill = hover();
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_fill = hover();
    v.widgets.active.weak_bg_fill = hover();
    v.widgets.active.bg_stroke = Stroke::NONE;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 5.0);
    });
}
