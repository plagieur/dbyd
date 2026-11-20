mod backend;
mod cursor;
mod format;
mod icons;
mod messages;
mod settings;
mod theme;
mod voice;

use backend::{Channel, ChannelKind, Command, Emoji, Event, GifItem, Guild, Message, Profile, Upload};
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui::{self, Margin, RichText, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use format::Format;
use icons::Icon;
use settings::Settings;
use theme::{bad, bg, dim, hover, ink, input, line, muted, rail, sidebar};
use tokio::sync::mpsc::UnboundedSender;

const KEYRING_SERVICE: &str = "dbyb";
const KEYRING_USER: &str = "bot-token";
const KEYRING_GIF_USER: &str = "klipy-key";
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
    /// Profile picture address (empty for conversations saved by an older version).
    #[serde(default)]
    avatar: String,
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
    replying: Option<(u64, String)>,
    /// The message being edited (id, original text): the message box then edits it instead of sending.
    editing: Option<(u64, String)>,
    emoji_open: bool,
    gif_open: bool,
    gif_query: String,
    gif_items: Vec<GifItem>,
    gif_key: Option<String>,
    gif_key_input: String,

    settings: Settings,
    themes: Vec<theme::Theme>,
    settings_open: bool,
    draft: theme::Theme,
    new_theme_name: String,
    theme_note: Option<String>,
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

fn saved_gif_key() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_GIF_USER).ok()?.get_password().ok().filter(|k| !k.is_empty())
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
fn server_icon(ui: &mut egui::Ui, selected: bool, icon: Option<&str>, label: &str, glyph: Option<Icon>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
    let active = selected || response.hovered();
    let radius = if active { 16.0 } else { 24.0 };
    match icon {
        Some(url) => {
            egui::Image::new(url.to_string()).corner_radius(radius).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, radius, if active { hover() } else { input() });
            let initials: String = label.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
            match glyph {
                Some(glyph) => icons::paint(ui, glyph, egui::Rect::from_center_size(rect.center(), vec2(26.0, 26.0)), ink()),
                None => {
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initials, egui::FontId::proportional(16.0), ink());
                }
            }
        }
    }
    if selected {
        let pill = egui::Rect::from_center_size(egui::pos2(rect.left() - 10.0, rect.center().y), vec2(4.0, 36.0));
        ui.painter().rect_filled(pill, 2.0, ink());
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label)
}

/// A conversation in the DM list: the person's picture, then their name.
fn dm_row(ui: &mut egui::Ui, selected: bool, dm: &Dm) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 42.0), Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 4.0, hover());
    }
    let picture = if dm.avatar.is_empty() { format!("https://cdn.discordapp.com/embed/avatars/{}.png", (dm.user_id >> 22) % 6) } else { dm.avatar.clone() };
    let picture_rect = egui::Rect::from_min_size(rect.min + vec2(6.0, 5.0), vec2(32.0, 32.0));
    egui::Image::new(picture).corner_radius(16.0).paint_at(ui, picture_rect);
    let color = if selected { ink() } else { muted() };
    ui.painter().text(egui::pos2(picture_rect.right() + 10.0, rect.center().y), egui::Align2::LEFT_CENTER, &dm.name, egui::FontId::proportional(14.0), color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// A line of a channel list: an icon, then the name.
fn row(ui: &mut egui::Ui, selected: bool, icon: Icon, text: &str) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 4.0, hover());
    }
    let color = if selected { ink() } else { muted() };
    icons::paint(ui, icon, egui::Rect::from_center_size(egui::pos2(rect.left() + 16.0, rect.center().y), vec2(16.0, 16.0)), color);
    ui.painter().text(egui::pos2(rect.left() + 32.0, rect.center().y), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(14.0), color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::set_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        let themes = theme::load_all();
        let draft_theme = themes.iter().find(|t| t.name == settings.theme).or(themes.first()).cloned().unwrap_or_else(|| theme::built_in().remove(0));
        icons::configure(&settings.icon_pack, settings.icon_size, settings.icon_color);
        cursor::configure(&settings.cursor_style, settings.cursor_size);
        theme::set(draft_theme.colors);
        theme::apply_visuals(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(settings.scale.clamp(0.8, 1.5));
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
            replying: None,
            editing: None,
            emoji_open: false,
            gif_open: false,
            gif_query: String::new(),
            gif_items: Vec::new(),
            gif_key: saved_gif_key(),
            gif_key_input: String::new(),
            settings: settings,
            themes: themes,
            settings_open: false,
            draft: draft_theme,
            new_theme_name: String::new(),
            theme_note: None,
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
        self.replying = None;
        self.editing = None;
        self.voice = None;
        self.mic_muted = false;
        self.deafened = false;
        self.dm_mode = false;
        self.emoji_open = false;
        self.gif_open = false;
        self.gif_items.clear();
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

    fn remember_dm(&mut self, channel: u64, user_id: u64, name: &str, avatar: &str) {
        if !self.dms.iter().any(|d| d.channel == channel) {
            self.dms.push(Dm { channel, user_id, name: name.to_string(), avatar: avatar.to_string() });
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
                Event::Gifs { query, items } => {
                    if query == self.gif_query.trim() {
                        self.gif_items = items;
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
                        self.remember_dm(channel, message.author_id, &message.author, &message.avatar);
                    }
                    if self.selected_channel == Some(channel) && !self.messages.iter().any(|m| m.id == message.id) {
                        self.messages.push(message);
                    }
                }
                Event::MessageUpdated { channel, message } => {
                    if self.selected_channel == Some(channel) {
                        if let Some(old) = self.messages.iter_mut().find(|m| m.id == message.id) {
                            *old = message;
                        }
                    }
                }
                Event::MessageDeleted { channel, id } => {
                    if self.selected_channel == Some(channel) {
                        self.messages.retain(|m| m.id != id);
                    }
                }
                Event::DmOpened { channel, user_id, name, avatar } => {
                    self.remember_dm(channel, user_id, &name, &avatar);
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
        egui::CentralPanel::no_frame().frame(egui::Frame::new().fill(bg())).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.28).max(30.0));
                ui.label(RichText::new("DBYB").size(40.0).strong());
                ui.label(RichText::new("Discord, But You're A Bot").size(15.0).color(muted()));
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
                    ui.label(RichText::new("Connecting...").color(muted()));
                }
                if let Some(error) = &self.error {
                    ui.add_space(10.0);
                    ui.label(RichText::new(error).color(bad()));
                }
                ui.add_space(30.0);
                ui.label(RichText::new("Use the token of a bot you own. It is only sent to discord.com\nand kept in the Windows credential store if you tick the box.").size(12.0).color(muted()));
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
        let card = egui::Frame::new().fill(sidebar()).corner_radius(12.0).stroke(Stroke::new(1.0, line()));
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
                    let color = self.profile.accent.map(|c| egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).unwrap_or(hover());
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
            // `place` leaves the layout cursor alone (`put` would move it back up to the banner's top)
            if ui.place(egui::Rect::from_min_size(banner_rect.right_top() + vec2(-36.0, 8.0), vec2(28.0, 28.0)), egui::Button::new("✕").corner_radius(14.0)).clicked() {
                close = true;
            }

            // avatar overlapping the banner: click to change
            let avatar_rect = egui::Rect::from_min_size(banner_rect.left_bottom() + vec2(18.0, -48.0), vec2(96.0, 96.0));
            ui.painter().circle_filled(avatar_rect.center(), 54.0, sidebar());
            let avatar_src = self.avatar.as_ref().map(|a| a.data_uri.clone()).unwrap_or_else(|| self.profile.avatar.clone());
            egui::Image::new(avatar_src).corner_radius(48.0).paint_at(ui, avatar_rect);
            let avatar = ui.interact(avatar_rect, ui.make_persistent_id("profile-avatar"), Sense::click());
            if avatar.hovered() {
                ui.painter().circle_filled(avatar_rect.center(), 48.0, egui::Color32::from_black_alpha(150));
                ui.painter().text(avatar_rect.center(), egui::Align2::CENTER_CENTER, "CHANGE", egui::FontId::proportional(12.0), ink());
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
                ui.label(RichText::new(format!("{}  •  Bot", self.profile.name)).size(13.0).color(muted()));
                ui.add_space(4.0);
                egui::Frame::new().fill(rail()).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Bio").size(12.0).strong());
                    ui.add(egui::TextEdit::multiline(&mut self.profile_bio).hint_text("Tell people about this bot").desired_rows(3).desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Member since").size(12.0).strong());
                    ui.label(messages::format_date(self.profile.created));
                });
                ui.label(RichText::new("Discord limits how often a bot can change its username and images.").size(11.0).color(muted()));
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(bad()));
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(dirty, egui::Button::new("Save changes").fill(hover())).clicked() {
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
                    ui.label(RichText::new(&guild.name).size(12.0).color(muted()));
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
                ui.label(RichText::new("Standard").size(12.0).color(muted()));
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

    fn gif_window(&mut self, ctx: &egui::Context) {
        let mut open = self.gif_open;
        let mut search = false;
        let mut save_key = false;
        let mut forget_key = false;
        let mut chosen: Option<GifItem> = None;
        egui::Window::new("GIFs").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_BOTTOM, [-24.0, -84.0]).show(ctx, |ui| {
            ui.set_width(420.0);
            if self.gif_key.is_none() {
                ui.label(RichText::new("Klipy API key").size(12.0).color(muted()));
                let field = ui.add(egui::TextEdit::singleline(&mut self.gif_key_input).password(true).hint_text("Paste your key").desired_width(f32::INFINITY));
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    save_key = true;
                }
                if ui.button("Save").clicked() {
                    save_key = true;
                }
                ui.label(RichText::new("Get a free key at partner.klipy.com. It is kept in your system's credential store, never in a file.").size(11.0).color(muted()));
                return;
            }
            let field = ui.add(egui::TextEdit::singleline(&mut self.gif_query).hint_text("Search GIFs").desired_width(f32::INFINITY));
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                search = true;
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for gif in &self.gif_items {
                        let height = 100.0;
                        let width = (height * gif.width as f32 / gif.height.max(1) as f32).clamp(60.0, 200.0);
                        let image = egui::Image::new(gif.preview.clone()).fit_to_exact_size(vec2(width, height)).corner_radius(4.0);
                        if ui.add(egui::Button::image(image).frame(false)).on_hover_text(&gif.title).clicked() {
                            chosen = Some(gif.clone());
                        }
                    }
                });
                if self.gif_items.is_empty() {
                    ui.label(RichText::new("Nothing yet.").size(12.0).color(muted()));
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Powered by KLIPY").size(11.0).color(muted()));
                if ui.add(egui::Button::new(RichText::new("Change key").size(11.0)).frame(false)).clicked() {
                    forget_key = true;
                }
            });
        });
        self.gif_open = open;
        if save_key {
            let key = self.gif_key_input.trim().to_string();
            if !key.is_empty() {
                if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_GIF_USER) {
                    let _ = entry.set_password(&key);
                }
                self.gif_key_input.clear();
                self.gif_key = Some(key.clone());
                self.send(Command::Gifs { key, query: String::new() });
            }
        }
        if forget_key {
            if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_GIF_USER) {
                let _ = entry.delete_credential();
            }
            self.gif_key = None;
            self.gif_items.clear();
        }
        if search {
            if let Some(key) = self.gif_key.clone() {
                self.send(Command::Gifs { key, query: self.gif_query.trim().to_string() });
            }
        }
        if let (Some(gif), Some(channel)) = (chosen, self.selected_channel) {
            // a GIF is sent as its link: Discord shows it as an image
            let reply_to = self.replying.take().map(|(id, _)| id);
            self.send(Command::Send { channel, text: gif.send_url.clone(), files: Vec::new(), reply_to });
            if let Some(key) = self.gif_key.clone() {
                self.send(Command::GifShared { key, slug: gif.slug });
            }
            self.gif_open = false;
        }
    }
    /// Applies a formatting choice to the text selected in the message box (or inserts the markers at the cursor).
    fn apply_format(&mut self, ctx: &egui::Context, id: egui::Id, format: Format) {
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let length = self.input.chars().count();
        let (start, end) = match state.cursor.char_range() {
            Some(range) => (range.primary.index.0.min(range.secondary.index.0), range.primary.index.0.max(range.secondary.index.0)),
            None => (length, length),
        };
        let (text, new_start, new_end) = format::apply(&self.input, start, end, format);
        self.input = text;
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(new_start), egui::text::CCursor::new(new_end))));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn select_theme(&mut self, ctx: &egui::Context, name: &str) {
        if let Some(theme) = self.themes.iter().find(|t| t.name == name).cloned() {
            theme::set(theme.colors);
            theme::apply_visuals(ctx);
            self.settings.theme = theme.name.clone();
            self.settings.save();
            self.draft = theme;
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        let mut pick: Option<String> = None;
        let mut colors_changed = false;
        let mut save_theme = false;
        let mut open_folder = false;
        let mut scale_done = false;
        let mut icons_changed = false;
        let mut cursor_changed = false;
        let mut open_icons_dir = false;
        let mut open_cursors_dir = false;
        egui::Window::new("Settings").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Appearance").strong());
            egui::ComboBox::from_label("Theme").selected_text(self.settings.theme.clone()).show_ui(ui, |ui| {
                for theme in &self.themes {
                    if ui.selectable_label(self.settings.theme == theme.name, &theme.name).clicked() {
                        pick = Some(theme.name.clone());
                    }
                }
            });
            let slider = ui.add(egui::Slider::new(&mut self.settings.scale, 0.8..=1.5).text("Interface size"));
            if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                scale_done = true;
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Icons").strong());
            egui::ComboBox::from_label("Icon pack").selected_text(self.settings.icon_pack.clone()).show_ui(ui, |ui| {
                for pack in icons::packs() {
                    if ui.selectable_label(self.settings.icon_pack == pack, &pack).clicked() {
                        self.settings.icon_pack = pack;
                        icons_changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                let mut own_color = self.settings.icon_color.is_some();
                if ui.checkbox(&mut own_color, "Own icon colour").changed() {
                    self.settings.icon_color = own_color.then_some([219, 222, 225]);
                    icons_changed = true;
                }
                if let Some(color) = &mut self.settings.icon_color {
                    if ui.color_edit_button_srgb(color).changed() {
                        icons_changed = true;
                    }
                }
            });
            let icon_slider = ui.add(egui::Slider::new(&mut self.settings.icon_size, 14.0..=32.0).text("Icon size"));
            if icon_slider.drag_stopped() || (icon_slider.changed() && !icon_slider.dragged()) {
                icons_changed = true;
            }
            if ui.button("Open the icon packs folder").on_hover_text("A pack is a folder holding plus.png, gif.svg, ... which replace the built-in icons").clicked() {
                open_icons_dir = true;
            }
            if self.settings.icon_pack != icons::DEFAULT_PACK {
                ui.label(RichText::new(format!("{} icon(s) replaced by this pack.", icons::replaced_count())).size(12.0).color(muted()));
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Cursor (experimental)").strong());
            egui::ComboBox::from_label("Cursor style").selected_text(self.settings.cursor_style.clone()).show_ui(ui, |ui| {
                for style in cursor::styles() {
                    if ui.selectable_label(self.settings.cursor_style == style, &style).clicked() {
                        self.settings.cursor_style = style;
                        cursor_changed = true;
                    }
                }
            });
            let cursor_slider = ui.add(egui::Slider::new(&mut self.settings.cursor_size, 16.0..=64.0).text("Cursor size"));
            if cursor_slider.drag_stopped() || (cursor_slider.changed() && !cursor_slider.dragged()) {
                cursor_changed = true;
            }
            if ui.button("Open the cursors folder").on_hover_text("A style is a folder holding arrow, pointer and text images (.png or .svg)").clicked() {
                open_cursors_dir = true;
            }
            ui.label(RichText::new("DBYB hides the Windows cursor in its window and draws this one itself.").size(11.0).color(muted()));

            ui.add_space(6.0);
            egui::CollapsingHeader::new("Customise the colours").show(ui, |ui| {
                let c = &mut self.draft.colors;
                for (label, value) in [
                    ("Server list", &mut c.rail),
                    ("Channel list", &mut c.sidebar),
                    ("Chat background", &mut c.bg),
                    ("Boxes and cards", &mut c.input),
                    ("Hover and selection", &mut c.hover),
                    ("Lines", &mut c.line),
                    ("Text", &mut c.ink),
                    ("Secondary text", &mut c.muted),
                    ("Faint text", &mut c.dim),
                    ("Errors", &mut c.bad),
                ] {
                    ui.horizontal(|ui| {
                        if ui.color_edit_button_srgb(value).changed() {
                            colors_changed = true;
                        }
                        ui.label(label);
                    });
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Save your colours as a theme to keep them:").size(12.0).color(muted()));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_theme_name).hint_text("Theme name").desired_width(180.0));
                    if ui.button("Save as a theme").clicked() {
                        save_theme = true;
                    }
                });
            });
            ui.add_space(4.0);
            if ui.button("Open the themes folder").on_hover_text("Themes are small .json files: copy, edit or share them").clicked() {
                open_folder = true;
            }
            if let Some(note) = &self.theme_note {
                ui.label(RichText::new(note).size(12.0).color(muted()));
            }
        });
        self.settings_open = open;
        if let Some(name) = pick {
            self.select_theme(ctx, &name);
        }
        if colors_changed {
            theme::set(self.draft.colors);
            theme::apply_visuals(ctx);
        }
        if scale_done {
            self.settings.scale = self.settings.scale.clamp(0.8, 1.5);
            ctx.set_zoom_factor(self.settings.scale);
            self.settings.save();
        }
        if icons_changed {
            icons::configure(&self.settings.icon_pack, self.settings.icon_size, self.settings.icon_color);
            self.settings.save();
        }
        if cursor_changed {
            cursor::configure(&self.settings.cursor_style, self.settings.cursor_size);
            self.settings.save();
        }
        for (wanted, dir) in [(open_icons_dir, icons::packs_dir()), (open_cursors_dir, cursor::cursors_dir())] {
            if let (true, Some(dir)) = (wanted, dir) {
                let _ = std::process::Command::new("explorer").arg(dir).spawn();
            }
        }
        if open_folder {
            if let Some(dir) = theme::themes_dir() {
                let _ = std::process::Command::new("explorer").arg(dir).spawn();
            }
        }
        if save_theme {
            let name = self.new_theme_name.trim().to_string();
            if name.is_empty() {
                self.theme_note = Some("Give the theme a name first.".into());
            } else if theme::built_in().iter().any(|t| t.name == name) {
                self.theme_note = Some("That name is used by a built-in theme: pick another.".into());
            } else {
                let saved = theme::Theme { name: name.clone(), colors: self.draft.colors };
                match theme::save(&saved) {
                    Ok(file) => {
                        self.themes = theme::load_all();
                        self.new_theme_name.clear();
                        self.theme_note = Some(format!("Saved as {file} in the themes folder."));
                        self.select_theme(ctx, &name);
                    }
                    Err(e) => self.theme_note = Some(format!("Could not save: {e}")),
                }
            }
        }
    }
    fn channel_list(&self, ui: &mut egui::Ui, pick: &mut Option<u64>, pick_voice: &mut Option<u64>) {
        let sort = |a: &&Channel, b: &&Channel| (a.kind == ChannelKind::Voice, a.position).cmp(&(b.kind == ChannelKind::Voice, b.position));
        let mut show = |ui: &mut egui::Ui, channel: &Channel, pick: &mut Option<u64>| match channel.kind {
            ChannelKind::Text => {
                if row(ui, self.selected_channel == Some(channel.id), Icon::Hash, &channel.name) {
                    *pick = Some(channel.id);
                }
            }
            ChannelKind::Voice => {
                let connected = self.voice.is_some_and(|(_, c)| c == channel.id);
                if row(ui, connected, Icon::Speaker, &channel.name) {
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
            egui::CollapsingHeader::new(RichText::new(category.name.to_uppercase()).size(11.0).color(muted())).id_salt(category.id).default_open(true).show(ui, |ui| {
                for channel in children {
                    show(ui, channel, pick);
                }
            });
        }
    }

    fn message_list(&self, ui: &mut egui::Ui, open_dm: &mut Option<u64>, reply_to: &mut Option<(u64, String)>, delete: &mut Option<u64>, edit: &mut Option<(u64, String)>) {
        for (i, message) in self.messages.iter().enumerate() {
            let previous = i.checked_sub(1).and_then(|p| self.messages.get(p));
            let grouped = messages::continues(previous, message);
            if let Some(reply) = &message.reply {
                let mut text: String = reply.content.chars().take(80).collect();
                text = text.replace('\n', " ");
                ui.label(RichText::new(format!("   ↳ {}  {}", reply.author, text)).size(12.0).color(muted()));
            }
            ui.add_space(if grouped { 0.0 } else { 10.0 });
            let row = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
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
                                    ui.label(RichText::new(" BOT ").size(10.0).color(ink()).background_color(dim()));
                                }
                                ui.label(RichText::new(messages::format_time(message.timestamp)).size(11.0).color(muted()));
                            });
                        }
                        messages::body(ui, message);
                        if message.edited {
                            ui.label(RichText::new("(edited)").size(11.0).color(muted()));
                        }
                        messages::attachments(ui, &message.attachments);
                        messages::embeds(ui, message);
                    });
                });
            });
            // the whole line, to the right edge: the pointer stays "on the message" while it moves to the button
            let line = egui::Rect::from_min_max(row.response.rect.min, egui::pos2(ui.max_rect().right(), row.response.rect.max.y));
            // a "Reply" button appears at the right of the message under the pointer
            if ui.rect_contains_pointer(line) {
                let button = egui::Rect::from_min_size(egui::pos2(line.right() - 40.0, line.top()), vec2(34.0, 34.0));
                let mut corner = ui.new_child(egui::UiBuilder::new().max_rect(button));
                if icons::button(&mut corner, Icon::Reply, "Reply").clicked() {
                    *reply_to = Some((message.id, message.author.clone()));
                }
            }
            row.response.context_menu(|ui| {
                if icons::menu_item(ui, Icon::Reply, "Reply").clicked() {
                    *reply_to = Some((message.id, message.author.clone()));
                    ui.close();
                }
                if icons::menu_item(ui, Icon::Copy, "Copy text").clicked() {
                    ui.ctx().copy_text(message.content.clone());
                    ui.close();
                }
                if icons::menu_item(ui, Icon::Copy, "Copy message link").clicked() {
                    let scope = self.selected_guild.map(|g| g.to_string()).unwrap_or_else(|| "@me".into());
                    ui.ctx().copy_text(format!("https://discord.com/channels/{scope}/{}/{}", self.selected_channel.unwrap_or(0), message.id));
                    ui.close();
                }
                if icons::menu_item(ui, Icon::Copy, "Copy message ID").clicked() {
                    ui.ctx().copy_text(message.id.to_string());
                    ui.close();
                }
                if message.author_id == self.bot_id && icons::menu_item(ui, Icon::Edit, "Edit message").clicked() {
                    *edit = Some((message.id, message.content.clone()));
                    ui.close();
                }
                if message.author_id == self.bot_id && icons::menu_item(ui, Icon::Trash, "Delete message").clicked() {
                    *delete = Some(message.id);
                    ui.close();
                }
                if message.author_id != self.bot_id && icons::menu_item(ui, Icon::Message, "Send a DM").clicked() {
                    *open_dm = Some(message.author_id);
                    ui.close();
                }
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

        egui::Panel::left("rail").exact_size(72.0).resizable(false).frame(panel(rail(), Margin::symmetric(12, 10))).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if server_icon(ui, self.dm_mode, None, "Direct messages", Some(Icon::Message)).clicked() {
                pick_dms = true;
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for guild in &self.guilds {
                    if server_icon(ui, !self.dm_mode && self.selected_guild == Some(guild.id), guild.icon.as_deref(), &guild.name, None).clicked() {
                        pick_guild = Some(guild.id);
                    }
                }
            });
        });

        egui::Panel::left("channels").exact_size(240.0).resizable(false).frame(panel(sidebar(), Margin::symmetric(10, 12))).show(ui, |ui| {
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
                    ui.label(RichText::new("The user must share a server with the bot.").size(11.0).color(muted()));
                    ui.add_space(6.0);
                    for dm in &self.dms {
                        if dm_row(ui, self.selected_channel == Some(dm.channel), dm) {
                            pick_channel = Some(dm.channel);
                        }
                    }
                    if self.dms.is_empty() {
                        ui.label(RichText::new("No conversation yet.").size(12.0).color(muted()));
                    }
                } else {
                    self.channel_list(ui, &mut pick_channel, &mut pick_voice);
                }
            });
            ui.separator();
            if let Some((_, channel)) = self.voice {
                let name = self.channels.iter().find(|c| c.id == channel).map(|c| c.name.clone()).unwrap_or_default();
                ui.label(RichText::new("Voice connected").size(12.0).strong());
                ui.label(RichText::new(format!("🔊 {name}")).size(12.0).color(muted()));
                ui.horizontal(|ui| {
                    if icons::button(ui, if self.mic_muted { Icon::MicOff } else { Icon::Mic }, if self.mic_muted { "Unmute the microphone" } else { "Mute the microphone" }).clicked() {
                        voice_action = Some(Command::SetMicMuted(!self.mic_muted));
                        self.mic_muted = !self.mic_muted;
                    }
                    if icons::button(ui, if self.deafened { Icon::HeadphonesOff } else { Icon::Headphones }, if self.deafened { "Hear the others again" } else { "Stop hearing the others" }).clicked() {
                        voice_action = Some(Command::SetDeafened(!self.deafened));
                        self.deafened = !self.deafened;
                    }
                    if icons::button(ui, Icon::Leave, "Leave the voice channel").clicked() {
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
                    ui.label(RichText::new("Bot").size(11.0).color(muted()));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Logout, "Log out").clicked() {
                        logout = true;
                    }
                    if icons::button(ui, Icon::User, "Edit the bot's profile").clicked() {
                        open_profile = true;
                    }
                    if icons::button(ui, Icon::Sliders, "Settings and themes").clicked() {
                        self.settings_open = !self.settings_open;
                    }
                });
            });
        });

        egui::Panel::top("chat-top").frame(panel(bg(), Margin::symmetric(16, 12)).stroke(Stroke::new(1.0, rail()))).show(ui, |ui| {
            ui.label(RichText::new(self.channel_title()).strong().size(15.0));
        });

        let mut to_send: Option<(String, Vec<Upload>)> = None;
        let mut new_reply: Option<(u64, String)> = None;
        let mut delete_id: Option<u64> = None;
        let mut new_edit: Option<(u64, String)> = None;
        let mut edit_done: Option<String> = None;
        let mut format_pick: Option<Format> = None;
        let input_id = ui.make_persistent_id("composer-input");
        egui::Panel::bottom("composer").frame(panel(bg(), Margin::symmetric(16, 14))).show(ui, |ui| {
            if let Some(error) = &self.error {
                ui.label(RichText::new(error).color(bad()));
            }
            if let Some(notice) = &self.notice {
                ui.label(RichText::new(notice).color(muted()));
            }
            if let Some(author) = self.replying.as_ref().map(|(_, author)| author.clone()) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Replying to {author}")).size(12.0).color(muted()));
                    if icons::button(ui, Icon::Close, "Cancel the reply").clicked() {
                        self.replying = None;
                    }
                });
            }
            if self.editing.is_some() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Editing a message: Enter saves, Esc cancels").size(12.0).color(muted()));
                    if icons::button(ui, Icon::Close, "Cancel the edit").clicked() {
                        self.editing = None;
                        self.input.clear();
                    }
                });
            }
            egui::Frame::new().fill(input()).corner_radius(8.0).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                let enabled = self.selected_channel.is_some();
                ui.add_enabled_ui(enabled, |ui| {
                    if !self.pending.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            let mut remove = None;
                            for (i, file) in self.pending.iter().enumerate() {
                                if ui.add(egui::Button::image_and_text(icons::image(Icon::Close).tint(muted()).fit_to_exact_size(vec2(12.0, 12.0)), &file.name).fill(hover())).on_hover_text("Remove").clicked() {
                                    remove = Some(i);
                                }
                            }
                            if let Some(i) = remove {
                                self.pending.remove(i);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if icons::button(ui, Icon::Plus, "Add files (or drop them on the window)").clicked() {
                            add_files = true;
                        }
                        let hint = format!("Message {}", self.channel_title());
                        let field_width = ui.available_width() - 44.0;
                        let response = ui.add(egui::TextEdit::singleline(&mut self.input).id(input_id).hint_text(hint).frame(egui::Frame::NONE).desired_width(field_width));
                        // keyboard shortcuts, like in Discord
                        if response.has_focus() {
                            for (key, format) in [(egui::Key::B, Format::Bold), (egui::Key::I, Format::Italic), (egui::Key::U, Format::Underline)] {
                                if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key)) {
                                    format_pick = Some(format);
                                }
                            }
                        }
                        // right-click: formatting boxes (they apply to the selected text)
                        response.context_menu(|ui| {
                            for format in Format::ALL {
                                if ui.button(format.label()).clicked() {
                                    format_pick = Some(format);
                                    ui.close();
                                }
                            }
                        });
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.editing.is_some() {
                            self.editing = None;
                            self.input.clear();
                        } else if response.lost_focus() && enter && self.editing.is_some() {
                            if !self.input.trim().is_empty() {
                                edit_done = Some(std::mem::take(&mut self.input).trim().to_string());
                            }
                            response.request_focus();
                        } else if response.lost_focus() && enter && (!self.input.trim().is_empty() || !self.pending.is_empty()) {
                            to_send = Some((std::mem::take(&mut self.input).trim().to_string(), std::mem::take(&mut self.pending)));
                            response.request_focus();
                        }
                        if icons::button(ui, Icon::Smile, "Emojis").clicked() {
                            self.emoji_open = !self.emoji_open;
                            self.gif_open = false;
                        }
                        if icons::button(ui, Icon::Gif, "GIFs").clicked() {
                            self.gif_open = !self.gif_open;
                            self.emoji_open = false;
                            if self.gif_open && self.gif_items.is_empty() {
                                if let Some(key) = self.gif_key.clone() {
                                    self.send(Command::Gifs { key, query: String::new() });
                                }
                            }
                        }
                    });
                });
            });
        });

        egui::CentralPanel::no_frame().frame(panel(bg(), Margin::symmetric(16, 8))).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                self.message_list(ui, &mut open_dm, &mut new_reply, &mut delete_id, &mut new_edit);
            });
        });

        let ctx = ui.ctx().clone();
        if self.profile_open {
            self.profile_window(&ctx);
        }
        if self.settings_open {
            self.settings_window(&ctx);
        }
        if self.gif_open {
            self.gif_window(&ctx);
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
        if let (Some(text), Some((id, _)), Some(channel)) = (edit_done, self.editing.clone(), self.selected_channel) {
            self.send(Command::EditMessage { channel, id, text });
            self.editing = None;
        }
        if let Some((id, text)) = new_edit {
            self.editing = Some((id, text.clone()));
            self.input = text;
            self.replying = None;
            ctx.memory_mut(|m| m.request_focus(input_id));
        }
        if let Some(format) = format_pick {
            self.apply_format(&ctx, input_id, format);
        }
        if let (Some(id), Some(channel)) = (delete_id, self.selected_channel) {
            self.send(Command::DeleteMessage { channel, id });
        }
        if new_reply.is_some() {
            self.replying = new_reply;
        }
        if let Some(id) = pick_channel {
            self.replying = None;
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
            let reply_to = self.replying.take().map(|(id, _)| id);
            self.send(Command::Send { channel, text, files, reply_to });
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
        // last, so it sees which cursor the widgets asked for
        cursor::draw(ui.ctx());
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

/// Writes every crash to `%APPDATA%\dbyb\crash.log` (message, place in the code, backtrace) and tells the user,
/// so a crash always leaves something to look at.
fn install_crash_handler() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let report = format!("[{}] DBYB {} crashed on thread {:?}\n{info}\n\n{backtrace}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), env!("CARGO_PKG_VERSION"), std::thread::current().name().unwrap_or("?"));
        let dir = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("dbyb"));
        if let Some(dir) = &dir {
            let _ = std::fs::create_dir_all(dir);
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
                use std::io::Write;
                let _ = file.write_all(report.as_bytes());
            }
        }
        default_hook(info);
        // a crash of the window itself ends the program: say so (other threads report through the app)
        if std::thread::current().name() == Some("main") {
            let place = dir.map(|d| d.join("crash.log").display().to_string()).unwrap_or_default();
            rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("DBYB crashed").set_description(format!("DBYB hit an unexpected error and has to close.\n\nThe details are in:\n{place}")).show();
        }
    }));
}
fn main() -> eframe::Result {
    install_crash_handler();
    init_logging();
    // TLS needs one crypto provider chosen explicitly
    let _ = rustls::crypto::ring::default_provider().install_default();
    let viewport = egui::ViewportBuilder::default().with_title("DBYB").with_inner_size([1200.0, 740.0]).with_min_inner_size([860.0, 520.0]);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("DBYB", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
