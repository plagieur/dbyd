//! A custom mouse cursor. DBYB hides the system cursor inside its window and draws the chosen picture itself.
//! Built-in styles: "classic", "dot", "ring". Your own: a folder in `%APPDATA%\dbyb\cursors\<style name>` holding
//! `arrow`, `pointer` (over buttons and links) and `text` (over text boxes), as .svg or .png. The point of a cursor
//! is its top-left corner (the centre for `text`).

use eframe::egui::{self, CursorIcon, vec2};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::RwLock;

pub const SYSTEM: &str = "System";

struct Config {
    style: String,
    size: f32,
}

static CONFIG: RwLock<Config> = RwLock::new(Config { style: String::new(), size: 28.0 });

pub fn cursors_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("dbyb").join("cursors");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

const BUILT_IN: [&str; 3] = ["classic", "dot", "ring"];

/// "System" (the normal Windows cursor), the built-in styles, and the folders of the cursors folder.
pub fn styles() -> Vec<String> {
    let mut list = vec![SYSTEM.to_string()];
    list.extend(BUILT_IN.iter().map(|s| s.to_string()));
    if let Some(dir) = cursors_dir() {
        let mut names: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| !BUILT_IN.contains(&n.as_str())).collect();
        names.sort();
        list.extend(names);
    }
    list
}

pub fn configure(style: &str, size: f32) {
    if let Ok(mut config) = CONFIG.write() {
        *config = Config { style: style.to_string(), size: size.clamp(16.0, 64.0) };
    }
}

fn built_in(style: &str, kind: &str) -> Option<&'static str> {
    Some(match (style, kind) {
        ("classic", "arrow") => include_str!("../assets/cursors/classic/arrow.svg"),
        ("classic", "pointer") => include_str!("../assets/cursors/classic/pointer.svg"),
        ("classic", "text") => include_str!("../assets/cursors/classic/text.svg"),
        ("dot", "arrow") => include_str!("../assets/cursors/dot/arrow.svg"),
        ("dot", "pointer") => include_str!("../assets/cursors/dot/pointer.svg"),
        ("dot", "text") => include_str!("../assets/cursors/dot/text.svg"),
        ("ring", "arrow") => include_str!("../assets/cursors/ring/arrow.svg"),
        ("ring", "pointer") => include_str!("../assets/cursors/ring/pointer.svg"),
        ("ring", "text") => include_str!("../assets/cursors/ring/text.svg"),
        _ => return None,
    })
}

fn source(style: &str, kind: &str) -> Option<egui::ImageSource<'static>> {
    if let Some(svg) = built_in(style, kind) {
        return Some(egui::ImageSource::Bytes { uri: Cow::Owned(format!("bytes://dbyb-cursor-{style}-{kind}.svg")), bytes: egui::load::Bytes::Static(svg.as_bytes()) });
    }
    let dir = cursors_dir()?.join(style);
    ["svg", "png", "webp", "gif"].iter().map(|ext| dir.join(format!("{kind}.{ext}"))).find(|p| p.is_file()).map(|path| egui::ImageSource::Uri(Cow::Owned(format!("file://{}", path.display()).replace('\\', "/"))))
}

/// Where the active point of the picture is, as a fraction of its size.
fn hotspot(style: &str, kind: &str) -> egui::Vec2 {
    match (style, kind) {
        ("classic", "arrow") => vec2(0.16, 0.09),
        ("classic", "pointer") => vec2(0.41, 0.08),
        (_, "text") | ("dot", _) | ("ring", _) => vec2(0.5, 0.5),
        _ => vec2(0.0, 0.0),
    }
}

/// Call once per frame, at the end. Does nothing with the "System" cursor, or if the picture isn't ready yet
/// (the user must never be left without a cursor).
pub fn draw(ctx: &egui::Context) {
    let (style, size) = match CONFIG.read() {
        Ok(c) => (c.style.clone(), c.size),
        Err(_) => return,
    };
    if style.is_empty() || style == SYSTEM {
        return;
    }
    let Some(position) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    // what the widgets under the pointer asked for this frame
    let kind = match ctx.output(|o| o.cursor_icon) {
        CursorIcon::PointingHand | CursorIcon::Grab | CursorIcon::Grabbing => "pointer",
        CursorIcon::Text | CursorIcon::VerticalText => "text",
        _ => "arrow",
    };
    let Some(source) = source(&style, kind) else { return };
    let image = egui::Image::new(source).fit_to_exact_size(vec2(size, size));
    let ready = image.load_for_size(ctx, vec2(size, size)).map(|poll| poll.size().is_some()).unwrap_or(false);
    if !ready {
        ctx.request_repaint();
        return;
    }
    ctx.set_cursor_icon(CursorIcon::None);
    #[allow(deprecated)]
    let id = egui::Id::new("dbyb-cursor");
    egui::Area::new(id).order(egui::Order::Tooltip).interactable(false).fixed_pos(position - hotspot(&style, kind) * size).show(ctx, |ui| {
        ui.add(image);
    });
}
