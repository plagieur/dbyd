//! Drawing of messages: avatars, text with custom emojis, attachments and embeds.

use crate::backend::{Attachment, Embed, Message};
use crate::theme::{HOVER, INPUT, LINE, MUTED};
use chrono::{DateTime, Local};
use eframe::egui::{self, Color32, RichText, Sense, vec2};

pub const AVATAR: f32 = 40.0;
const EMOJI: f32 = 22.0;
const BIG_EMOJI: f32 = 40.0;
const MAX_MEDIA: egui::Vec2 = vec2(420.0, 320.0);

pub fn emoji_url(id: u64) -> String {
    format!("https://cdn.discordapp.com/emojis/{id}.png?size=64")
}

pub fn format_time(timestamp: i64) -> String {
    let Some(time) = DateTime::from_timestamp(timestamp, 0).map(|t| t.with_timezone(&Local)) else { return String::new() };
    let today = Local::now().date_naive();
    let day = time.date_naive();
    if day == today {
        format!("Today at {}", time.format("%H:%M"))
    } else if today.pred_opt() == Some(day) {
        format!("Yesterday at {}", time.format("%H:%M"))
    } else {
        time.format("%d/%m/%Y %H:%M").to_string()
    }
}

/// A message continues the previous one (no new header) when it is from the same author, close in time, and not a reply.
pub fn continues(previous: Option<&Message>, message: &Message) -> bool {
    previous.is_some_and(|p| p.author_id == message.author_id && message.reply.is_none() && (message.timestamp - p.timestamp).abs() < 420)
}

fn format_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KiB", b as f64 / (1 << 10) as f64),
        b => format!("{b} B"),
    }
}

pub fn avatar(ui: &mut egui::Ui, url: &str, size: f32) {
    ui.add(egui::Image::new(url.to_string()).fit_to_exact_size(vec2(size, size)).corner_radius(size / 2.0));
}

enum Piece<'a> {
    Text(&'a str),
    Emoji(u64),
}

/// Reads `<:name:id>` or `<a:name:id>` at the start of `tail`: the emoji id and how many bytes it takes.
fn parse_emoji(tail: &str) -> Option<(u64, usize)> {
    let inner = tail.strip_prefix("<a:").or_else(|| tail.strip_prefix("<:"))?;
    let (_name, after) = inner.split_once(':')?;
    let (digits, _) = after.split_once('>')?;
    let id = digits.parse().ok()?;
    Some((id, tail.len() - after.len() + digits.len() + 1))
}

/// Splits a message into text and custom emojis.
fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let (mut start, mut cursor) = (0, 0);
    while let Some(offset) = text[cursor..].find('<') {
        let at = cursor + offset;
        match parse_emoji(&text[at..]) {
            Some((id, len)) => {
                if start < at {
                    out.push(Piece::Text(&text[start..at]));
                }
                out.push(Piece::Emoji(id));
                start = at + len;
                cursor = start;
            }
            None => cursor = at + 1,
        }
    }
    if start < text.len() {
        out.push(Piece::Text(&text[start..]));
    }
    out
}
pub fn content(ui: &mut egui::Ui, text: &str) {
    if text.is_empty() {
        return;
    }
    let parts = pieces(text);
    let has_emoji = parts.iter().any(|p| matches!(p, Piece::Emoji(_)));
    if !has_emoji {
        ui.label(text);
        return;
    }
    let only_emoji = parts.iter().all(|p| match p {
        Piece::Emoji(_) => true,
        Piece::Text(t) => t.trim().is_empty(),
    });
    let size = if only_emoji { BIG_EMOJI } else { EMOJI };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for part in parts {
            match part {
                Piece::Text(t) => {
                    ui.label(t);
                }
                Piece::Emoji(id) => {
                    ui.add(egui::Image::new(emoji_url(id)).fit_to_exact_size(vec2(size, size)));
                }
            }
        }
    });
}

fn open(ui: &egui::Ui, url: &str) {
    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
}

pub fn attachments(ui: &mut egui::Ui, files: &[Attachment]) {
    for file in files {
        if file.content_type.starts_with("image/") {
            let mut size = match (file.width, file.height) {
                (Some(w), Some(h)) if w > 0 && h > 0 => vec2(w as f32, h as f32),
                _ => MAX_MEDIA,
            };
            let scale = (MAX_MEDIA.x / size.x).min(MAX_MEDIA.y / size.y).min(1.0);
            size *= scale;
            let response = ui.add(egui::Image::new(file.url.clone()).fit_to_exact_size(size).corner_radius(6.0).sense(Sense::click()));
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                open(ui, &file.url);
            }
        } else {
            let kind = if file.content_type.starts_with("video/") {
                "Video"
            } else if file.content_type.starts_with("audio/") {
                "Audio"
            } else {
                "File"
            };
            egui::Frame::new().fill(INPUT).stroke(egui::Stroke::new(1.0, LINE)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_max_width(MAX_MEDIA.x);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        if ui.link(&file.filename).clicked() {
                            open(ui, &file.url);
                        }
                        ui.label(RichText::new(format!("{kind} · {}", format_size(file.size))).size(12.0).color(MUTED));
                    });
                });
            });
        }
    }
}

pub fn embeds(ui: &mut egui::Ui, list: &[Embed]) {
    for embed in list {
        let color = embed.color.map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).unwrap_or(HOVER);
        let response = egui::Frame::new().fill(INPUT).corner_radius(4.0).inner_margin(egui::Margin { left: 14, right: 12, top: 10, bottom: 10 }).show(ui, |ui| {
            ui.set_max_width(MAX_MEDIA.x + 20.0);
            if let Some(title) = &embed.title {
                ui.label(RichText::new(title).strong());
            }
            if let Some(description) = &embed.description {
                content(ui, description);
            }
            if let Some(image) = embed.image.as_ref().or(embed.thumbnail.as_ref()) {
                ui.add(egui::Image::new(image.clone()).fit_to_exact_size(vec2(MAX_MEDIA.x, 200.0)).maintain_aspect_ratio(true).corner_radius(4.0));
            }
        });
        let rect = response.response.rect;
        ui.painter().rect_filled(egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 4.0, rect.bottom())), 2.0, color);
    }
}

pub fn format_date(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0).map(|t| t.with_timezone(&Local).format("%b %d, %Y").to_string()).unwrap_or_default()
}
