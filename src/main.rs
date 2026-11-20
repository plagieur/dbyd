mod backend;
mod messages;
mod theme;
mod voice;

use backend::{Channel, ChannelKind, Command, Emoji, Event, Guild, Message, Profile, Upload};
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui::{self, Margin, RichText, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use theme::{BAD, BG, DIM, HOVER, INK, INPUT, LINE, MUTED, RAIL, SIDEBAR};
use tokio::sync::mpsc::UnboundedSender;

const KEYRING_SERVICE: &str = "dbyb";
const KEYRING_USER: &str = "bot-token";
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;

const UNICODE_EMOJIS: &str = "😀😁😂🤣😊😍😘😎🤔😅😭😡🥺😴🙄😏👍👎👏🙏💪🤝👀🔥✨⭐💯🎉🎂❤️💔💙💚💛🧡💜🖤🤍✅❌⚠️❓❗💀👻🤖🎮🎵🎧📷🎬🍕🍔🍟🍺☕🐱🐶🦊🐸🌈🌙☀️⚡🚀";

enum Screen {
    Login,
    Connecting,
    Chat,
}

/// A direct-message conversation the bot has had (Discord can't list them for bots, so we remember them).
#[derive(Clone, Serialize, Deserialize)]
struct Dm {
    channel: u64,
    user_id: u64,
    name: String,
}

/// An image picked for the profile, as the data URI Discord expects (also used for the preview).
struct Picked {
    data_uri: String,
}

struct App {
    screen: Screen,
    token: String,
    remember: bool,
    error: Option<String>,
    notice: Option<String>,

    commands: Option<UnboundedSender<Command>>,
    events: Option<mpsc::Receiver<Event>>,

    bot_id: u64,
    bot_name: String,
    bot_avatar: String,
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    emojis: HashMap<u64, Vec<Emoji>>,
    dms: Vec<Dm>,
    messages: Vec<Message>,
    dm_mode: bool,
    selected_guild: Option<u64>,
    selected_channel: Option<u64>,
    input: String,
    dm_input: String,
    pending: Vec<Upload>,
    emoji_open: bool,
    voice: Option<(u64, u64)>,
    mic_muted: bool,
    deafened: bool,

    profile: Profile,
    profile_open: bool,
    profile_name: String,
    profile_bio: String,
    avatar: Option<Picked>,
    banner: Option<Picked>,
}

fn saved_token() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?.get_password().ok()
}

fn store_token(token: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.set_password(token);
    }
}

fn forget_token() {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.delete_credential();
    }
}

fn dms_path(bot_id: u64) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("dbyb");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("dms-{bot_id}.json")))
}

fn load_dms(bot_id: u64) -> Vec<Dm> {
    dms_path(bot_id).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Reads an image file and turns it into the `data:` URI Discord expects.
fn pick_image(title: &str) -> Result<Option<Picked>, String> {
    let Some(path) = rfd::FileDialog::new().set_title(title).add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() else {
        return Ok(None);
    };
    let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size > MAX_IMAGE_BYTES {
        return Err("This image is too big (8 MiB maximum).".into());
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err("Unsupported image type.".into()),
    };
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(Some(Picked { data_uri: format!("data:{mime};base64,{}", STANDARD.encode(bytes)) }))
}

/// A round server icon in the left rail, with the white "selected" pill like Discord.
fn server_icon(ui: &mut egui::Ui, selected: bool, icon: Option<&str>, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
    let active = selected || response.hovered();
    let radius = if active { 16.0 } else { 24.0 };
    match icon {
        Some(url) => {
            egui::Image::new(url.to_string()).corner_radius(radius).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, radius, if active { HOVER } else { INPUT });
            let initials: String = label.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initials, egui::FontId::proportional(16.0), INK);
        }
    }
    if selected {
        let pill = egui::Rect::from_center_size(egui::pos2(rect.left() - 10.0, rect.center().y), vec2(4.0, 36.0));
        ui.painter().rect_filled(pill, 2.0, INK);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label)
}

fn row(ui: &mut egui::Ui, selected: bool, text: String) -> bool {
    let width = ui.available_width();
    ui.add_sized([width, 28.0], egui::Button::selectable(selected, text).fill(if selected { HOVER } else { egui::Color32::TRANSPARENT })).clicked()
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let token = saved_token();
        let mut app = Self {
            screen: Screen::Login,
            remember: token.is_some(),
            token: token.clone().unwrap_or_default(),
            error: None,
            notice: None,
            commands: None,
            events: None,
            bot_id: 0,
            bot_name: String::new(),
            bot_avatar: String::new(),
            guilds: Vec::new(),
            channels: Vec::new(),
            emojis: HashMap::new(),
            dms: Vec::new(),
            messages: Vec::new(),
            dm_mode: false,
            selected_guild: None,
            selected_channel: None,
            input: String::new(),
            dm_input: String::new(),
            pending: Vec::new(),
            emoji_open: false,
            voice: None,
            mic_muted: false,
            deafened: false,
            profile: Profile::default(),
            profile_open: false,
            profile_name: String::new(),
            profile_bio: String::new(),
            avatar: None,
            banner: None,
        };
        if token.is_some() {
            app.connect(&cc.egui_ctx);
        }
        app
    }

    fn connect(&mut self, ctx: &egui::Context) {
        let token = self.token.trim().to_string();
        if token.is_empty() {
            self.error = Some("Paste the token of one of your bots.".into());
            return;
        }
        let (event_tx, event_rx) = mpsc::channel();
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let events = event_tx.clone();
            let repaint = ctx.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                backend::run(token, command_rx, event_tx, move || ctx.request_repaint())
            }));
            if result.is_err() {
                let _ = events.send(Event::Error("The connection thread crashed. See the console for details.".into()));
                repaint.request_repaint();
            }
        });
        self.events = Some(event_rx);
        self.commands = Some(command_tx);
        self.error = None;
        self.screen = Screen::Connecting;
    }

    fn disconnect(&mut self) {
        self.commands = None; // closing the channel stops the backend thread
        self.events = None;
        self.guilds.clear();
        self.channels.clear();
        self.emojis.clear();
        self.dms.clear();
        self.messages.clear();
        self.pending.clear();
        self.voice = None;
        self.mic_muted = false;
        self.deafened = false;
        self.dm_mode = false;
        self.emoji_open = false;
        self.selected_guild = None;
        self.selected_channel = None;
        self.profile_open = false;
        self.avatar = None;
        self.banner = None;
        self.screen = Screen::Login;
    }

    fn send(&self, command: Command) {
        if let Some(tx) = &self.commands {
            let _ = tx.send(command);
        }
    }

    fn save_dms(&self) {
        if let Some(path) = dms_path(self.bot_id) {
            if let Ok(json) = serde_json::to_string_pretty(&self.dms) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn remember_dm(&mut self, channel: u64, user_id: u64, name: &str) {
        if !self.dms.iter().any(|d| d.channel == channel) {
            self.dms.push(Dm { channel, user_id, name: name.to_string() });
            self.save_dms();
        }
    }

    fn open_channel(&mut self, channel: u64) {
        self.selected_channel = Some(channel);
        self.messages.clear();
        self.error = None;
        self.send(Command::LoadMessages(channel));
    }

    fn add_file(&mut self, path: &Path) {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if size > MAX_UPLOAD_BYTES {
            self.error = Some("This file is too big to send (25 MiB maximum here; Discord's own limit may be lower).".into());
            return;
        }
        match std::fs::read(path) {
            Ok(bytes) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
                self.pending.push(Upload { name, bytes });
            }
            Err(e) => self.error = Some(format!("Cannot read this file: {e}")),
        }
    }

    fn channel_title(&self) -> String {
        let Some(id) = self.selected_channel else { return String::new() };
        if self.dm_mode {
            self.dms.iter().find(|d| d.channel == id).map(|d| format!("@ {}", d.name)).unwrap_or_default()
        } else {
            self.channels.iter().find(|c| c.id == id).map(|c| format!("# {}", c.name)).unwrap_or_default()
        }
    }

    fn poll(&mut self) {
        let Some(rx) = &self.events else { return };
        let events: Vec<Event> = rx.try_iter().collect();
        for event in events {
            match event {
                Event::LoggedIn { profile, guilds } => {
                    if self.remember {
                        store_token(self.token.trim());
                    } else {
                        forget_token();
                    }
                    let bot_id = profile.id;
                    self.bot_id = bot_id;
                    self.profile_name = profile.name.clone();
                    self.profile_bio = profile.bio.clone();
                    self.bot_name = profile.name.clone();
                    self.bot_avatar = profile.avatar.clone();
                    self.profile = profile;
                    for guild in &guilds {
                        self.send(Command::LoadEmojis(guild.id));
                    }
                    self.guilds = guilds;
                    self.dms = load_dms(bot_id);
                    self.screen = Screen::Chat;
                }
                Event::Channels { guild, channels } => {
                    if self.selected_guild == Some(guild) {
                        self.channels = channels;
                    }
                }
                Event::Emojis { guild, emojis } => {
                    self.emojis.insert(guild, emojis);
                }
                Event::Messages { channel, messages } => {
                    if self.selected_channel == Some(channel) {
                        self.messages = messages;
                    }
                }
                Event::NewMessage { channel, message, is_dm } => {
                    if is_dm && message.author_id != self.bot_id {
                        self.remember_dm(channel, message.author_id, &message.author);
                    }
                    if self.selected_channel == Some(channel) && !self.messages.iter().any(|m| m.id == message.id) {
                        self.messages.push(message);
                    }
                }
                Event::DmOpened { channel, user_id, name } => {
                    self.remember_dm(channel, user_id, &name);
                    self.dm_mode = true;
                    self.selected_guild = None;
                    self.open_channel(channel);
                }
                Event::ProfileUpdated(profile) => {
                    self.bot_name = profile.name.clone();
                    self.profile_name = profile.name.clone();
                    self.profile_bio = profile.bio.clone();
                    self.bot_avatar = profile.avatar.clone();
                    self.profile = profile;
                    self.profile_open = false;
                    self.avatar = None;
                    self.banner = None;
                    self.notice = Some("Profile updated.".into());
                }
                Event::Voice(state) => self.voice = state,
                Event::Error(message) => {
                    if matches!(self.screen, Screen::Connecting) {
                        self.disconnect();
                    }
                    self.error = Some(message);
                }
            }
        }
    }

    fn login(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::no_frame().frame(egui::Frame::new().fill(BG)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.28).max(30.0));
                ui.label(RichText::new("DBYB").size(40.0).strong());
                ui.label(RichText::new("Discord, But You're A Bot").size(15.0).color(MUTED));
                ui.add_space(28.0);

                let connecting = matches!(self.screen, Screen::Connecting);
                ui.add_enabled_ui(!connecting, |ui| {
                    let field = ui.add(egui::TextEdit::singleline(&mut self.token).password(true).hint_text("Bot token").desired_width(360.0));
                    ui.checkbox(&mut self.remember, "Remember this bot on this computer");
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Log in").clicked() || enter {
                        let ctx = ui.ctx().clone();
                        self.connect(&ctx);
                    }
                });
                if connecting {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Connecting...").color(MUTED));
                }
                if let Some(error) = &self.error {
                    ui.add_space(10.0);
                    ui.label(RichText::new(error).color(BAD));
                }
                ui.add_space(30.0);
                ui.label(RichText::new("Use the token of a bot you own. It is only sent to discord.com\nand kept in the Windows credential store if you tick the box.").size(12.0).color(MUTED));
            });
        });
    }

    fn reset_profile_draft(&mut self) {
        self.profile_name = self.profile.name.clone();
        self.profile_bio = self.profile.bio.clone();
        self.avatar = None;
        self.banner = None;
    }

    fn profile_window(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut apply = false;
        let width = 420.0;
        let dirty = self.profile_name.trim() != self.profile.name || self.profile_bio != self.profile.bio || self.avatar.is_some() || self.banner.is_some();
        let card = egui::Frame::new().fill(SIDEBAR).corner_radius(12.0).stroke(Stroke::new(1.0, LINE));
        egui::Window::new("profile-card").title_bar(false).resizable(false).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).fixed_size([width, 0.0]).frame(card).show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);

            // banner: click to change
            let (banner_rect, banner) = ui.allocate_exact_size(vec2(width, 120.0), Sense::click());
            let top = egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 };
            let banner_src = self.banner.as_ref().map(|b| b.data_uri.clone()).or_else(|| self.profile.banner.clone());
            match banner_src {
                Some(src) => {
                    let crop = egui::Rect::from_min_max(egui::pos2(0.0, 0.143), egui::pos2(1.0, 0.857));
                    egui::Image::new(src).uv(crop).corner_radius(top).paint_at(ui, banner_rect);
                }
                None => {
                    let color = self.profile.accent.map(|c| egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).unwrap_or(HOVER);
                    ui.painter().rect_filled(banner_rect, top, color);
                }
            }
            if banner.on_hover_text("Change banner").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                match pick_image("Choose a banner") {
                    Ok(Some(image)) => self.banner = Some(image),
                    Ok(None) => {}
                    Err(e) => self.error = Some(e),
                }
            }
            if ui.put(egui::Rect::from_min_size(banner_rect.right_top() + vec2(-36.0, 8.0), vec2(28.0, 28.0)), egui::Button::new("✕").corner_radius(14.0)).clicked() {
                close = true;
            }

            // avatar overlapping the banner: click to change
            let avatar_rect = egui::Rect::from_min_size(banner_rect.left_bottom() + vec2(18.0, -48.0), vec2(96.0, 96.0));
            ui.painter().circle_filled(avatar_rect.center(), 54.0, SIDEBAR);
            let avatar_src = self.avatar.as_ref().map(|a| a.data_uri.clone()).unwrap_or_else(|| self.profile.avatar.clone());
            egui::Image::new(avatar_src).corner_radius(48.0).paint_at(ui, avatar_rect);
            let avatar = ui.interact(avatar_rect, ui.make_persistent_id("profile-avatar"), Sense::click());
            if avatar.hovered() {
                ui.painter().circle_filled(avatar_rect.center(), 48.0, egui::Color32::from_black_alpha(150));
                ui.painter().text(avatar_rect.center(), egui::Align2::CENTER_CENTER, "CHANGE", egui::FontId::proportional(12.0), INK);
            }
            if avatar.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                match pick_image("Choose an avatar") {
                    Ok(Some(image)) => self.avatar = Some(image),
                    Ok(None) => {}
                    Err(e) => self.error = Some(e),
                }
            }
            ui.add_space(52.0);

            egui::Frame::new().inner_margin(Margin::symmetric(18, 0)).show(ui, |ui| {
                ui.set_width(width - 36.0);
                ui.add(egui::TextEdit::singleline(&mut self.profile_name).font(egui::FontId::proportional(24.0)).desired_width(f32::INFINITY));
                ui.label(RichText::new(format!("{}  •  Bot", self.profile.name)).size(13.0).color(MUTED));
                ui.add_space(4.0);
                egui::Frame::new().fill(RAIL).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Bio").size(12.0).strong());
                    ui.add(egui::TextEdit::multiline(&mut self.profile_bio).hint_text("Tell people about this bot").desired_rows(3).desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Member since").size(12.0).strong());
                    ui.label(messages::format_date(self.profile.created));
                });
                ui.label(RichText::new("Discord limits how often a bot can change its username and images.").size(11.0).color(MUTED));
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(BAD));
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(dirty, egui::Button::new("Save changes").fill(HOVER)).clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(14.0);
        });
        if apply {
            let name = self.profile_name.trim().to_string();
            let username = (!name.is_empty() && name != self.profile.name).then_some(name);
            let bio = (self.profile_bio != self.profile.bio).then(|| self.profile_bio.clone());
            let avatar = self.avatar.as_ref().map(|i| i.data_uri.clone());
            let banner = self.banner.as_ref().map(|i| i.data_uri.clone());
            self.error = None;
            self.send(Command::UpdateProfile { username, avatar, banner, bio });
        }
        if close {
            self.profile_open = false;
            self.reset_profile_draft();
        }
    }
    fn emoji_window(&mut self, ctx: &egui::Context) {
        let mut open = self.emoji_open;
        let mut inserted = None;
        egui::Window::new("Emojis").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_BOTTOM, [-24.0, -84.0]).show(ctx, |ui| {
            ui.set_width(340.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for guild in &self.guilds {
                    let Some(list) = self.emojis.get(&guild.id).filter(|l| !l.is_empty()) else { continue };
                    ui.label(RichText::new(&guild.name).size(12.0).color(MUTED));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for emoji in list {
                            let image = egui::Image::new(messages::emoji_url(emoji.id)).fit_to_exact_size(vec2(28.0, 28.0));
                            let button = ui.add(egui::Button::image(image).frame(false)).on_hover_text(format!(":{}:", emoji.name));
                            if button.clicked() {
                                let tag = if emoji.animated { "a" } else { "" };
                                inserted = Some(format!("<{tag}:{}:{}> ", emoji.name, emoji.id));
                            }
                        }
                    });
                    ui.add_space(6.0);
                }
                ui.label(RichText::new("Standard").size(12.0).color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    let mut chars = UNICODE_EMOJIS.chars().peekable();
                    while let Some(c) = chars.next() {
                        let mut symbol = c.to_string();
                        // keep the variation selector with its emoji
                        while chars.peek() == Some(&'\u{fe0f}') {
                            symbol.push(chars.next().unwrap_or_default());
                        }
                        if ui.add(egui::Button::new(RichText::new(&symbol).size(20.0)).frame(false)).clicked() {
                            inserted = Some(symbol);
                        }
                    }
                });
            });
        });
        self.emoji_open = open;
        if let Some(text) = inserted {
            self.input.push_str(&text);
        }
    }

    fn channel_list(&self, ui: &mut egui::Ui, pick: &mut Option<u64>, pick_voice: &mut Option<u64>) {
        let sort = |a: &&Channel, b: &&Channel| (a.kind == ChannelKind::Voice, a.position).cmp(&(b.kind == ChannelKind::Voice, b.position));
        let mut show = |ui: &mut egui::Ui, channel: &Channel, pick: &mut Option<u64>| match channel.kind {
            ChannelKind::Text => {
                if row(ui, self.selected_channel == Some(channel.id), format!("#  {}", channel.name)) {
                    *pick = Some(channel.id);
                }
            }
            ChannelKind::Voice => {
                let connected = self.voice.is_some_and(|(_, c)| c == channel.id);
                if row(ui, connected, format!("🔊  {}", channel.name)) {
                    *pick_voice = Some(channel.id);
                }
            }
            ChannelKind::Category => {}
        };
        let mut roots: Vec<&Channel> = self.channels.iter().filter(|c| c.kind != ChannelKind::Category && c.parent.is_none()).collect();
        roots.sort_by(sort);
        for channel in roots {
            show(ui, channel, pick);
        }
        let mut categories: Vec<&Channel> = self.channels.iter().filter(|c| c.kind == ChannelKind::Category).collect();
        categories.sort_by_key(|c| c.position);
        for category in categories {
            let mut children: Vec<&Channel> = self.channels.iter().filter(|c| c.parent == Some(category.id)).collect();
            children.sort_by(sort);
            egui::CollapsingHeader::new(RichText::new(category.name.to_uppercase()).size(11.0).color(MUTED)).id_salt(category.id).default_open(true).show(ui, |ui| {
                for channel in children {
                    show(ui, channel, pick);
                }
            });
        }
    }

    fn message_list(&self, ui: &mut egui::Ui, open_dm: &mut Option<u64>) {
        for (i, message) in self.messages.iter().enumerate() {
            let previous = i.checked_sub(1).and_then(|p| self.messages.get(p));
            let grouped = messages::continues(previous, message);
            if let Some(reply) = &message.reply {
                let mut text: String = reply.content.chars().take(80).collect();
                text = text.replace('\n', " ");
                ui.label(RichText::new(format!("   ↳ {}  {}", reply.author, text)).size(12.0).color(MUTED));
            }
            ui.add_space(if grouped { 0.0 } else { 10.0 });
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if grouped {
                    ui.add_space(messages::AVATAR);
                } else {
                    messages::avatar(ui, &message.avatar, messages::AVATAR);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    if !grouped {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let author = ui.add(egui::Label::new(RichText::new(&message.author).strong()).sense(Sense::click()));
                            if message.author_id != self.bot_id {
                                let author = author.on_hover_text("Click to send a DM").on_hover_cursor(egui::CursorIcon::PointingHand);
                                if author.clicked() {
                                    *open_dm = Some(message.author_id);
                                }
                            }
                            if message.bot {
                                ui.label(RichText::new(" BOT ").size(10.0).color(INK).background_color(DIM));
                            }
                            ui.label(RichText::new(messages::format_time(message.timestamp)).size(11.0).color(MUTED));
                        });
                    }
                    messages::content(ui, &message.content);
                    messages::attachments(ui, &message.attachments);
                    messages::embeds(ui, &message.embeds);
                });
            });
        }
    }

    fn chat(&mut self, ui: &mut egui::Ui) {
        let panel = |fill, margin: Margin| egui::Frame::new().fill(fill).inner_margin(margin);
        let mut pick_guild = None;
        let mut pick_dms = false;
        let mut pick_channel = None;
        let mut pick_voice = None;
        let mut voice_action = None;
        let mut open_dm = None;
        let mut logout = false;
        let mut open_profile = false;
        let mut add_files = false;

        // files dropped on the window go to the attachment list
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if self.selected_channel.is_some() {
                self.add_file(file.path());
            }
        }

        egui::Panel::left("rail").exact_size(72.0).resizable(false).frame(panel(RAIL, Margin::symmetric(12, 10))).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if server_icon(ui, self.dm_mode, None, "Direct messages").clicked() {
                pick_dms = true;
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for guild in &self.guilds {
                    if server_icon(ui, !self.dm_mode && self.selected_guild == Some(guild.id), guild.icon.as_deref(), &guild.name).clicked() {
                        pick_guild = Some(guild.id);
                    }
                }
            });
        });

        egui::Panel::left("channels").exact_size(240.0).resizable(false).frame(panel(SIDEBAR, Margin::symmetric(10, 12))).show(ui, |ui| {
            let title = if self.dm_mode { "Direct messages".to_string() } else { self.guilds.iter().find(|g| Some(g.id) == self.selected_guild).map(|g| g.name.clone()).unwrap_or_else(|| "Pick a server".into()) };
            ui.label(RichText::new(title).strong().size(15.0));
            ui.separator();
            // keep room under the list for the user bar, and for the voice bar when connected
            let reserved = 72.0 + if self.voice.is_some() { 104.0 } else { 0.0 };
            let list_height = (ui.available_height() - reserved).max(80.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(list_height).show(ui, |ui| {
                if self.dm_mode {
                    let field = ui.add(egui::TextEdit::singleline(&mut self.dm_input).hint_text("Open a DM by user ID").desired_width(f32::INFINITY));
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        match self.dm_input.trim().parse::<u64>() {
                            Ok(user) => {
                                open_dm = Some(user);
                                self.dm_input.clear();
                            }
                            Err(_) => self.error = Some("A user ID is a long number (right-click a user, Copy User ID).".into()),
                        }
                    }
                    ui.label(RichText::new("The user must share a server with the bot.").size(11.0).color(MUTED));
                    ui.add_space(6.0);
                    for dm in &self.dms {
                        if row(ui, self.selected_channel == Some(dm.channel), format!("@  {}", dm.name)) {
                            pick_channel = Some(dm.channel);
                        }
                    }
                    if self.dms.is_empty() {
                        ui.label(RichText::new("No conversation yet.").size(12.0).color(MUTED));
                    }
                } else {
                    self.channel_list(ui, &mut pick_channel, &mut pick_voice);
                }
            });
            ui.separator();
            if let Some((_, channel)) = self.voice {
                let name = self.channels.iter().find(|c| c.id == channel).map(|c| c.name.clone()).unwrap_or_default();
                ui.label(RichText::new("Voice connected").size(12.0).strong());
                ui.label(RichText::new(format!("🔊 {name}")).size(12.0).color(MUTED));
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.mic_muted, if self.mic_muted { "Mic off" } else { "Mic on" }).clicked() {
                        voice_action = Some(Command::SetMicMuted(!self.mic_muted));
                        self.mic_muted = !self.mic_muted;
                    }
                    if ui.selectable_label(self.deafened, if self.deafened { "Deaf" } else { "Hear" }).clicked() {
                        voice_action = Some(Command::SetDeafened(!self.deafened));
                        self.deafened = !self.deafened;
                    }
                    if ui.button("Leave").clicked() {
                        voice_action = Some(Command::LeaveVoice);
                    }
                });
                ui.separator();
            }
            ui.horizontal(|ui| {
                messages::avatar(ui, &self.bot_avatar, 32.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(&self.bot_name).strong().size(13.0));
                    ui.label(RichText::new("Bot").size(11.0).color(MUTED));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("Out").frame(false)).on_hover_text("Log out").clicked() {
                        logout = true;
                    }
                    if ui.add(egui::Button::new("Profile").frame(false)).on_hover_text("Edit the bot's profile").clicked() {
                        open_profile = true;
                    }
                });
            });
        });

        egui::Panel::top("chat-top").frame(panel(BG, Margin::symmetric(16, 12)).stroke(Stroke::new(1.0, RAIL))).show(ui, |ui| {
            ui.label(RichText::new(self.channel_title()).strong().size(15.0));
        });

        let mut to_send: Option<(String, Vec<Upload>)> = None;
        egui::Panel::bottom("composer").frame(panel(BG, Margin::symmetric(16, 14))).show(ui, |ui| {
            if let Some(error) = &self.error {
                ui.label(RichText::new(error).color(BAD));
            }
            if let Some(notice) = &self.notice {
                ui.label(RichText::new(notice).color(MUTED));
            }
            egui::Frame::new().fill(INPUT).corner_radius(8.0).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                let enabled = self.selected_channel.is_some();
                ui.add_enabled_ui(enabled, |ui| {
                    if !self.pending.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            let mut remove = None;
                            for (i, file) in self.pending.iter().enumerate() {
                                if ui.add(egui::Button::new(format!("{}  ✕", file.name)).fill(HOVER)).on_hover_text("Remove").clicked() {
                                    remove = Some(i);
                                }
                            }
                            if let Some(i) = remove {
                                self.pending.remove(i);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("+").size(20.0)).frame(false)).on_hover_text("Add files (or drop them on the window)").clicked() {
                            add_files = true;
                        }
                        let hint = format!("Message {}", self.channel_title());
                        let field_width = ui.available_width() - 44.0;
                        let response = ui.add(egui::TextEdit::singleline(&mut self.input).hint_text(hint).frame(egui::Frame::NONE).desired_width(field_width));
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && (!self.input.trim().is_empty() || !self.pending.is_empty()) {
                            to_send = Some((std::mem::take(&mut self.input).trim().to_string(), std::mem::take(&mut self.pending)));
                            response.request_focus();
                        }
                        if ui.add(egui::Button::new(RichText::new("🙂").size(18.0)).frame(false)).on_hover_text("Emojis").clicked() {
                            self.emoji_open = !self.emoji_open;
                        }
                    });
                });
            });
        });

        egui::CentralPanel::no_frame().frame(panel(BG, Margin::symmetric(16, 8))).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                self.message_list(ui, &mut open_dm);
            });
        });

        let ctx = ui.ctx().clone();
        if self.profile_open {
            self.profile_window(&ctx);
        }
        if self.emoji_open {
            self.emoji_window(&ctx);
        }

        if add_files {
            if let Some(paths) = rfd::FileDialog::new().set_title("Add files").pick_files() {
                for path in paths {
                    self.add_file(&path);
                }
            }
        }
        if logout {
            forget_token();
            self.token.clear();
            self.remember = false;
            self.disconnect();
        }
        if open_profile {
            self.reset_profile_draft();
            self.profile_open = true;
            self.notice = None;
        }
        if pick_dms {
            self.dm_mode = true;
            self.selected_channel = None;
            self.messages.clear();
        }
        if let Some(id) = pick_guild {
            self.dm_mode = false;
            self.selected_guild = Some(id);
            self.selected_channel = None;
            self.channels.clear();
            self.messages.clear();
            self.send(Command::LoadChannels(id));
        }
        if let Some(id) = pick_channel {
            self.open_channel(id);
        }
        if let (Some(channel), Some(guild)) = (pick_voice, self.selected_guild) {
            self.mic_muted = false;
            self.deafened = false;
            self.send(Command::JoinVoice { guild, channel });
        }
        if let Some(command) = voice_action {
            self.send(command);
        }
        if let Some(user) = open_dm {
            self.notice = None;
            self.send(Command::OpenDm(user));
        }
        if let (Some((text, files)), Some(channel)) = (to_send, self.selected_channel) {
            self.error = None;
            self.notice = None;
            self.send(Command::Send { channel, text, files });
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        match self.screen {
            Screen::Login | Screen::Connecting => self.login(ui),
            Screen::Chat => self.chat(ui),
        }
    }
}

/// Writes the voice library's logs to `%APPDATA%\dbyb\voice.log`, to find out why a call fails.
fn init_logging() {
    let Some(dir) = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("dbyb")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(file) = std::fs::File::create(dir.join("voice.log")) {
        let _ = tracing_subscriber::fmt()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .with_env_filter(tracing_subscriber::EnvFilter::new("songbird=debug,twilight_gateway=info"))
            .try_init();
    }
}

fn main() -> eframe::Result {
    init_logging();
    // TLS needs one crypto provider chosen explicitly
    let _ = rustls::crypto::ring::default_provider().install_default();
    let viewport = egui::ViewportBuilder::default().with_title("DBYB").with_inner_size([1200.0, 740.0]).with_min_inner_size([860.0, 520.0]);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("DBYB", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
