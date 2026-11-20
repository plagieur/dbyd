//! The icons of the interface. A built-in set (SVG, white, tinted with the theme's colours), and icon packs:
//! a folder in `%APPDATA%\dbyb\icon-packs\<pack name>` holding `plus.png`, `gif.svg`... replaces the matching
//! built-in icons. Every file name is the icon's name (see `Icon::name`).

use eframe::egui::{self, Color32, Sense, vec2};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;

macro_rules! icons {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Not every icon is used by the interface yet: they are all part of what a pack can replace.
        #[allow(dead_code)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Icon { $($variant),* }

        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$variant),*];

            /// The file name (without extension) a pack uses to replace this icon.
            pub fn name(self) -> &'static str {
                match self { $(Icon::$variant => $name),* }
            }

            fn svg(self) -> &'static str {
                match self { $(Icon::$variant => include_str!(concat!("../assets/icons/", $name, ".svg"))),* }
            }
        }
    };
}

icons! {
    Plus => "plus",
    Smile => "smile",
    Gif => "gif",
    Sliders => "sliders",
    User => "user",
    Logout => "logout",
    Reply => "reply",
    Copy => "copy",
    Trash => "trash",
    Edit => "edit",
    Close => "close",
    Search => "search",
    Hash => "hash",
    Speaker => "speaker",
    Mic => "mic",
    MicOff => "mic_off",
    Headphones => "headphones",
    HeadphonesOff => "headphones_off",
    Leave => "leave",
    Send => "send",
    Message => "message",
    Image => "image",
    PlusCircle => "plus_circle",
    Members => "members",
}

pub const DEFAULT_PACK: &str = "Default";
const EXTENSIONS: [&str; 5] = ["svg", "png", "webp", "gif", "jpg"];

struct Config {
    /// Icons replaced by the pack: icon name -> address of the file.
    overrides: BTreeMap<&'static str, String>,
    size: f32,
    /// A colour chosen by the user; otherwise icons follow the theme.
    color: Option<[u8; 3]>,
}

static CONFIG: RwLock<Config> = RwLock::new(Config { overrides: BTreeMap::new(), size: 20.0, color: None });

pub fn packs_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("dbyb").join("icon-packs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// "Default" and the folders found in the packs folder.
pub fn packs() -> Vec<String> {
    let mut list = vec![DEFAULT_PACK.to_string()];
    if let Some(dir) = packs_dir() {
        let mut names: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).collect();
        names.sort();
        list.extend(names);
    }
    list
}

/// Chooses the pack, the size and the colour. Call again whenever one of them changes.
pub fn configure(pack: &str, size: f32, color: Option<[u8; 3]>) {
    let mut overrides = BTreeMap::new();
    if pack != DEFAULT_PACK {
        if let Some(dir) = packs_dir().map(|d| d.join(pack)) {
            for icon in Icon::ALL {
                let found = EXTENSIONS.iter().map(|ext| dir.join(format!("{}.{ext}", icon.name()))).find(|path| path.is_file());
                if let Some(path) = found {
                    overrides.insert(icon.name(), format!("file://{}", path.display()).replace('\\', "/"));
                }
            }
        }
    }
    if let Ok(mut config) = CONFIG.write() {
        *config = Config { overrides, size: size.clamp(12.0, 40.0), color };
    }
}

/// How many icons of the current pack replace a built-in one (to tell the user it was understood).
pub fn replaced_count() -> usize {
    CONFIG.read().map(|c| c.overrides.len()).unwrap_or(0)
}

pub fn size() -> f32 {
    CONFIG.read().map(|c| c.size).unwrap_or(20.0)
}

fn tint(default: Color32) -> Color32 {
    match CONFIG.read().ok().and_then(|c| c.color) {
        Some(c) => Color32::from_rgb(c[0], c[1], c[2]),
        None => default,
    }
}

/// The picture of an icon, at the interface's icon size. It is white: tint it before drawing.
pub fn image(icon: Icon) -> egui::Image<'static> {
    let source = match CONFIG.read().ok().and_then(|c| c.overrides.get(icon.name()).cloned()) {
        Some(address) => egui::ImageSource::Uri(Cow::Owned(address)),
        None => egui::ImageSource::Bytes { uri: Cow::Owned(format!("bytes://dbyb-icon-{}.svg", icon.name())), bytes: egui::load::Bytes::Static(icon.svg().as_bytes()) },
    };
    egui::Image::new(source)
}

/// Draws an icon of `size` points centred in `rect`.
pub fn paint(ui: &egui::Ui, icon: Icon, rect: egui::Rect, color: Color32) {
    image(icon).tint(tint(color)).fit_to_exact_size(rect.size()).paint_at(ui, rect);
}

/// A clickable icon: it lights up when hovered and shows `tip` as a tooltip.
pub fn button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let size = size();
    let (rect, response) = ui.allocate_exact_size(vec2(size + 10.0, size + 10.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 5.0, crate::theme::hover());
    }
    let color = if response.hovered() { crate::theme::ink() } else { crate::theme::muted() };
    paint(ui, icon, egui::Rect::from_center_size(rect.center(), vec2(size, size)), color);
    response.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A line of a menu: a small icon, then the text.
pub fn menu_item(ui: &mut egui::Ui, icon: Icon, text: &str) -> egui::Response {
    let picture = image(icon).tint(tint(crate::theme::muted())).fit_to_exact_size(vec2(16.0, 16.0));
    ui.add(egui::Button::image_and_text(picture, text).frame(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_a_unique_name_and_a_real_svg() {
        let mut names: Vec<&str> = Icon::ALL.iter().map(|i| i.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Icon::ALL.len());
        for icon in Icon::ALL {
            assert!(icon.svg().trim_start().starts_with("<svg") && icon.svg().contains("</svg>"), "{} is not an svg", icon.name());
        }
    }
}
