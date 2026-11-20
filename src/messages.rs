//! Drawing of messages: avatars, text with custom emojis, attachments and embeds.

use crate::backend::{Attachment, Embed, Message};
use crate::theme::{dim, hover, input, line as line_color, muted};
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

/// Reads `<:name:id>` or `<a:name:id>` at the start of `tail`: the emoji id and how many bytes it takes.
fn parse_emoji(tail: &str) -> Option<(u64, usize)> {
    let inner = tail.strip_prefix("<a:").or_else(|| tail.strip_prefix("<:"))?;
    let (_name, after) = inner.split_once(':')?;
    let (digits, _) = after.split_once('>')?;
    let id = digits.parse().ok()?;
    Some((id, tail.len() - after.len() + digits.len() + 1))
}

/// Reads `<@id>` or `<@!id>`: the user id and how many bytes it takes.
fn parse_mention(tail: &str) -> Option<(u64, usize)> {
    let inner = tail.strip_prefix("<@!").or_else(|| tail.strip_prefix("<@"))?;
    let (digits, _) = inner.split_once('>')?;
    let id = digits.parse().ok()?;
    Some((id, tail.len() - inner.len() + digits.len() + 1))
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    spoiler: bool,
}

enum Token {
    Text(String, Style),
    Code(String),
    Emoji(u64),
    Mention(String),
    Link(String),
}

/// Splits one line into styled text, inline code, links, emojis and mentions.
/// A marker (`**`, `*`, `~~`...) only counts when it is closed later in the line, so a lone `*` stays a `*`.
fn tokenize(line: &str, mentions: &[(u64, String)]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut buffer = String::new();
    let mut style = Style::default();
    let mut previous: Option<char> = None;
    let mut i = 0;
    'scan: while i < line.len() {
        let rest = &line[i..];
        let flush = |buffer: &mut String, out: &mut Vec<Token>, style: Style| {
            if !buffer.is_empty() {
                out.push(Token::Text(std::mem::take(buffer), style));
            }
        };
        if let Some(inner) = rest.strip_prefix('`') {
            if let Some(end) = inner.find('`').filter(|&e| e > 0) {
                flush(&mut buffer, &mut out, style);
                out.push(Token::Code(inner[..end].to_string()));
                i += end + 2;
                previous = Some('`');
                continue;
            }
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',', ';', ')', '!', '?']);
            flush(&mut buffer, &mut out, style);
            out.push(Token::Link(url.to_string()));
            i += url.len();
            previous = Some('/');
            continue;
        }
        if let Some((id, len)) = parse_emoji(rest) {
            flush(&mut buffer, &mut out, style);
            out.push(Token::Emoji(id));
            i += len;
            previous = Some('>');
            continue;
        }
        if let Some((id, len)) = parse_mention(rest) {
            flush(&mut buffer, &mut out, style);
            let name = mentions.iter().find(|(m, _)| *m == id).map(|(_, n)| n.clone()).unwrap_or_else(|| "unknown user".into());
            out.push(Token::Mention(name));
            i += len;
            previous = Some('>');
            continue;
        }
        for marker in ["**", "__", "~~", "||"] {
            let active = match marker {
                "**" => style.bold,
                "__" => style.underline,
                "~~" => style.strike,
                _ => style.spoiler,
            };
            if rest.starts_with(marker) && !active && !rest[marker.len()..].contains(marker) {
                // never closed: the characters are just text (not two single markers)
                buffer.push_str(marker);
                i += marker.len();
                previous = marker.chars().last();
                continue 'scan;
            }
            if rest.starts_with(marker) {
                flush(&mut buffer, &mut out, style);
                match marker {
                    "**" => style.bold = !style.bold,
                    "__" => style.underline = !style.underline,
                    "~~" => style.strike = !style.strike,
                    _ => style.spoiler = !style.spoiler,
                }
                i += marker.len();
                previous = marker.chars().last();
                continue 'scan;
            }
        }
        for marker in ['*', '_'] {
            // an opening `_` inside a word (snake_case) is just a character
            if !rest.starts_with(marker) {
                continue;
            }
            // (the marker is checked first: slicing at byte 1 would panic in the middle of a multi-byte character)
            let opens = !style.italic && rest[1..].contains(marker) && !(marker == '_' && previous.is_some_and(char::is_alphanumeric));
            if style.italic || opens {
                flush(&mut buffer, &mut out, style);
                style.italic = !style.italic;
                i += 1;
                previous = Some(marker);
                continue 'scan;
            }
        }
        let c = rest.chars().next().unwrap_or(' ');
        buffer.push(c);
        i += c.len_utf8();
        previous = Some(c);
    }
    if !buffer.is_empty() {
        out.push(Token::Text(buffer, style));
    }
    out
}

fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("bold".into()))
}

fn styled(text: &str, style: Style, size: f32) -> RichText {
    let mut rich = RichText::new(text).size(size);
    if style.bold {
        rich = rich.font(bold_font(size));
    }
    if style.italic {
        rich = rich.italics();
    }
    if style.underline {
        rich = rich.underline();
    }
    if style.strike {
        rich = rich.strikethrough();
    }
    if style.spoiler {
        rich = rich.color(dim()).background_color(dim());
    }
    rich
}

/// One line of a message: markdown, links, emojis and mentions, wrapped like normal text.
fn line(ui: &mut egui::Ui, text: &str, mentions: &[(u64, String)], emoji_size: f32) {
    let tokens = tokenize(text, mentions);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for token in tokens {
            match token {
                Token::Text(t, style) => {
                    let response = ui.label(styled(&t, style, 14.0));
                    if style.spoiler {
                        response.on_hover_text(t);
                    }
                }
                Token::Code(code) => {
                    ui.label(RichText::new(code).monospace().background_color(input()));
                }
                Token::Emoji(id) => {
                    ui.add(egui::Image::new(emoji_url(id)).fit_to_exact_size(vec2(emoji_size, emoji_size)));
                }
                Token::Mention(name) => {
                    ui.label(RichText::new(format!("@{name}")).background_color(hover()));
                }
                Token::Link(url) => {
                    if ui.link(&url).on_hover_text(&url).clicked() {
                        open(ui, &url);
                    }
                }
            }
        }
    });
}

/// A message's text with Discord's markdown: bold, italic, underline, strikethrough, spoilers, inline code,
/// code blocks, quotes, headings, clickable links, custom emojis and mentions.
pub fn content_with_mentions(ui: &mut egui::Ui, text: &str, mentions: &[(u64, String)]) {
    if text.trim().is_empty() {
        return;
    }
    // an emoji-only message shows big emojis
    let only_emoji = {
        let tokens = tokenize(text, mentions);
        !tokens.is_empty() && tokens.iter().all(|t| match t {
            Token::Emoji(_) => true,
            Token::Text(s, _) => s.trim().is_empty(),
            _ => false,
        }) && tokens.iter().any(|t| matches!(t, Token::Emoji(_)))
    };
    let emoji_size = if only_emoji { BIG_EMOJI } else { EMOJI };

    // fenced code blocks first: everything between two ``` is shown as is
    let mut rest = text;
    while !rest.is_empty() {
        let (before, code, after) = match rest.find("```") {
            Some(start) => match rest[start + 3..].find("```") {
                Some(end) => (&rest[..start], Some(&rest[start + 3..start + 3 + end]), &rest[start + 3 + end + 3..]),
                None => (rest, None, ""),
            },
            None => (rest, None, ""),
        };
        for text_line in before.lines() {
            let trimmed = text_line.trim_start();
            if let Some(quote) = trimmed.strip_prefix("> ") {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("▎").color(dim()));
                    ui.vertical(|ui| line(ui, quote, mentions, emoji_size));
                });
            } else if let Some((level, heading)) = [("### ", 16.0), ("## ", 19.0), ("# ", 22.0)].iter().find_map(|(mark, size)| trimmed.strip_prefix(mark).map(|h| (*size, h))) {
                ui.label(RichText::new(heading).font(bold_font(level)));
            } else if !text_line.trim().is_empty() {
                line(ui, text_line, mentions, emoji_size);
            }
        }
        if let Some(code) = code {
            // a first line made of one word is the language name (```rust): leave it out
            let body = match code.split_once('\n') {
                Some((first, tail)) if !first.trim().is_empty() && !first.contains(char::is_whitespace) => tail,
                _ => code,
            };
            egui::Frame::new().fill(input()).stroke(egui::Stroke::new(1.0, line_color())).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_max_width(MAX_MEDIA.x * 1.5);
                ui.label(RichText::new(body.trim_matches('\n')).monospace());
            });
        }
        rest = after;
    }
}

pub fn content(ui: &mut egui::Ui, text: &str) {
    content_with_mentions(ui, text, &[]);
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
            egui::Frame::new().fill(input()).stroke(egui::Stroke::new(1.0, line_color())).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_max_width(MAX_MEDIA.x);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        if ui.link(&file.filename).clicked() {
                            open(ui, &file.url);
                        }
                        ui.label(RichText::new(format!("{kind} · {}", format_size(file.size))).size(12.0).color(muted()));
                    });
                });
            });
        }
    }
}

fn is_image_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|ext| path.ends_with(ext))
}

fn is_single_link(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !text.contains(char::is_whitespace) && (text.starts_with("http://") || text.starts_with("https://"))
}

/// The animated image behind a "gifv" embed (Tenor, Giphy): Discord only gives a video, but both services
/// serve the same clip as a GIF at a predictable address.
fn gifv_image(embed: &Embed) -> Option<String> {
    let video = embed.video.as_deref()?;
    if video.contains("media.tenor.com/") {
        // Tenor: the folder ends with a code for the format (AAAPo = mp4, AAAPu = webm, AAAAC = gif)
        let (folder, file) = video.rsplit_once('/')?;
        let name = file.rsplit_once('.')?.0;
        let folder = folder.strip_suffix("AAAPo").or_else(|| folder.strip_suffix("AAAPu"))?;
        Some(format!("{folder}AAAAC/{name}.gif"))
    } else if video.contains("giphy.com/") {
        Some(format!("{}.gif", video.rsplit_once('.')?.0))
    } else {
        None
    }
}

/// The text of a message, without the link when that link is just an image or a GIF shown below (like Discord).
pub fn body(ui: &mut egui::Ui, message: &Message) {
    let link = message.content.trim();
    // a bare link to an image or GIF is drawn right away: Discord only adds its preview a moment later
    if is_single_link(link) && is_image_url(link) {
        media(ui, link.to_string(), Some(link));
        return;
    }
    let shows_media = message.embeds.iter().any(|e| e.kind == "gifv" || e.kind == "image");
    if !(is_single_link(link) && shows_media) {
        content_with_mentions(ui, &message.content, &message.mentions);
    }
}

fn media(ui: &mut egui::Ui, source: String, link: Option<&str>) {
    let image = egui::Image::new(source.clone()).corner_radius(6.0).sense(Sense::click());
    let size = match image.load_for_size(ui.ctx(), MAX_MEDIA) {
        Ok(poll) => poll.size(),
        Err(error) => {
            // say why instead of showing nothing
            let target = link.unwrap_or(&source).to_string();
            if ui.link(RichText::new(format!("Image not shown ({error}). Click to open it.")).size(12.0)).clicked() {
                open(ui, &target);
            }
            return;
        }
    };
    let Some(natural) = size.filter(|s| s.x > 0.0 && s.y > 0.0) else {
        // still loading: keep a visible place for it, so the message doesn't look empty or collapse
        let (rect, _) = ui.allocate_exact_size(vec2(220.0, 140.0), Sense::hover());
        ui.painter().rect_filled(rect, 6.0, input());
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "loading...", egui::FontId::proportional(12.0), muted());
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        return;
    };
    // small images are enlarged (a 100 px GIF is unreadable), big ones are shrunk to fit
    let wanted = if natural.x < 260.0 { (260.0 / natural.x).min(4.0) } else { 1.0 };
    let scale = wanted.min(MAX_MEDIA.x / natural.x).min(MAX_MEDIA.y / natural.y);
    let response = ui.add(image.fit_to_exact_size(natural * scale)).on_hover_text(format!("{source}\n{}x{} px", natural.x as u32, natural.y as u32));
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        if let Some(link) = link {
            open(ui, link);
        }
    }
}

pub fn embeds(ui: &mut egui::Ui, message: &Message) {
    let link = message.content.trim();
    let drawn_by_body = is_single_link(link) && is_image_url(link);
    for embed in &message.embeds {
        match embed.kind.as_str() {
            // the body already drew this image
            "image" if drawn_by_body => continue,
            // a GIF from Tenor or Giphy: just the animation, no card
            "gifv" => {
                if let Some(source) = gifv_image(embed).or_else(|| embed.thumbnail.clone()) {
                    media(ui, source, embed.url.as_deref());
                }
                continue;
            }
            // a link to an image: show the image itself (the original, so a GIF stays animated)
            "image" => {
                let original = embed.url.clone().filter(|u| is_image_url(u));
                if let Some(source) = original.or_else(|| embed.thumbnail.clone()).or_else(|| embed.image.clone()) {
                    media(ui, source, embed.url.as_deref());
                }
                continue;
            }
            _ => {}
        }
        let color = embed.color.map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).unwrap_or(hover());
        let response = egui::Frame::new().fill(input()).corner_radius(4.0).inner_margin(egui::Margin { left: 14, right: 12, top: 10, bottom: 10 }).show(ui, |ui| {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A compact description of the tokens, to compare in tests: `b` bold, `i` italic, `u` underline, `s` strike, `|` spoiler.
    fn describe(line: &str) -> String {
        let mentions = vec![(42, "alice".to_string())];
        tokenize(line, &mentions)
            .iter()
            .map(|t| match t {
                Token::Text(text, s) => {
                    let flags: String = [(s.bold, 'b'), (s.italic, 'i'), (s.underline, 'u'), (s.strike, 's'), (s.spoiler, '|')].iter().filter(|(on, _)| *on).map(|(_, c)| *c).collect();
                    format!("[{flags}:{text}]")
                }
                Token::Code(c) => format!("[code:{c}]"),
                Token::Emoji(id) => format!("[emoji:{id}]"),
                Token::Mention(n) => format!("[@{n}]"),
                Token::Link(u) => format!("[link:{u}]"),
            })
            .collect()
    }

    #[test]
    fn plain_text_stays_plain() {
        assert_eq!(describe("hello world"), "[:hello world]");
    }

    #[test]
    fn bold_italic_and_strike() {
        assert_eq!(describe("a **b** c"), "[:a ][b:b][: c]");
        assert_eq!(describe("*it*"), "[i:it]");
        assert_eq!(describe("~~gone~~"), "[s:gone]");
        assert_eq!(describe("***both***"), "[bi:both]");
    }

    #[test]
    fn a_lone_marker_is_a_normal_character() {
        assert_eq!(describe("2 * 3"), "[:2 * 3]");
        assert_eq!(describe("**never closed"), "[:**never closed]");
    }

    #[test]
    fn underscores_inside_words_are_kept() {
        assert_eq!(describe("my_var_name"), "[:my_var_name]");
        assert_eq!(describe("_it_"), "[i:it]");
    }

    #[test]
    fn inline_code_is_not_formatted() {
        assert_eq!(describe("run `a **b**` now"), "[:run ][code:a **b**][: now]");
    }

    #[test]
    fn links_lose_their_trailing_punctuation() {
        assert_eq!(describe("see https://example.com/a?b=1."), "[:see ][link:https://example.com/a?b=1][:.]");
    }

    #[test]
    fn emojis_and_mentions() {
        assert_eq!(describe("<:wave:123> <@42>"), "[emoji:123][: ][@alice]");
        assert_eq!(describe("<@!99>"), "[@unknown user]");
    }

    #[test]
    fn text_with_accents_and_emojis_does_not_panic() {
        // every kind of multi-byte character, at the start, in the middle, and next to markers
        for text in ["é", "éa", "ça marche *bien*", "日本語 **太字** _斜体_", "🙂", "🙂 *x*", "*é*", "__é__", "a é _b", "é_", "> é"] {
            let _ = describe(text);
        }
        assert_eq!(describe("ça *va*"), "[:ça ][i:va]");
    }

    #[test]
    fn gifv_addresses_are_rebuilt() {
        let embed = |video: &str| Embed { kind: "gifv".into(), url: None, video: Some(video.into()), title: None, description: None, color: None, image: None, thumbnail: None };
        assert_eq!(gifv_image(&embed("https://media.tenor.com/abcdAAAPo/cat.mp4")).as_deref(), Some("https://media.tenor.com/abcdAAAAC/cat.gif"));
        assert_eq!(gifv_image(&embed("https://media.giphy.com/media/xyz/giphy.mp4")).as_deref(), Some("https://media.giphy.com/media/xyz/giphy.gif"));
        assert_eq!(gifv_image(&embed("https://example.com/a.mp4")), None);
    }
}