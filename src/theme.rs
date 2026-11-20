//! Dark greys in the spirit of the Discord client. No accent colour.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};
use std::sync::Arc;

pub const RAIL: Color32 = Color32::from_rgb(30, 31, 34);
pub const SIDEBAR: Color32 = Color32::from_rgb(43, 45, 49);
pub const BG: Color32 = Color32::from_rgb(49, 51, 56);
pub const INPUT: Color32 = Color32::from_rgb(56, 58, 64);
pub const HOVER: Color32 = Color32::from_rgb(64, 66, 72);
pub const LINE: Color32 = Color32::from_rgb(63, 65, 70);
pub const INK: Color32 = Color32::from_rgb(219, 222, 225);
pub const MUTED: Color32 = Color32::from_rgb(148, 155, 164);
pub const DIM: Color32 = Color32::from_rgb(110, 115, 122);
pub const BAD: Color32 = Color32::from_rgb(240, 110, 110);

pub fn apply(ctx: &egui::Context) {
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
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SIDEBAR;
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = SIDEBAR;
    v.override_text_color = Some(INK);
    v.selection.bg_fill = Color32::from_rgb(90, 93, 100);
    v.selection.stroke = Stroke::new(1.0, INK);
    let radius = CornerRadius::same(4);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = radius;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.bg_fill = INPUT;
    v.widgets.inactive.weak_bg_fill = INPUT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_fill = HOVER;
    v.widgets.active.weak_bg_fill = HOVER;
    v.widgets.active.bg_stroke = Stroke::NONE;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 5.0);
    });
}
