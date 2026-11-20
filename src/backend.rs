//! Talks to Discord on a background thread: the UI sends `Command`s and receives `Event`s.

use crate::voice;
use async_trait::async_trait;
use songbird::driver::{DecodeConfig, DecodeMode};
use songbird::input::{Input, RawAdapter};
use songbird::shards::TwilightMap;
use songbird::{Config as SongbirdConfig, CoreEvent, Event as VoiceEvent, EventContext, EventHandler as VoiceEventHandler, Songbird};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use twilight_gateway::{Event as GatewayEvent, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client;
use twilight_model::channel::ChannelType;
use twilight_model::http::attachment::Attachment as UploadAttachment;
use twilight_model::id::{Id, marker::{ChannelMarker, GuildMarker, UserMarker}};
use twilight_model::util::ImageHash;

#[derive(Clone)]
pub struct Guild {
    pub id: u64,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    Voice,
    Category,
}

#[derive(Clone)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    pub kind: ChannelKind,
    pub parent: Option<u64>,
    pub position: i32,
}

#[derive(Clone)]
pub struct Attachment {
    pub url: String,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub width: Option<u64>,
    pub height: Option<u64>,
}

#[derive(Clone)]
pub struct Embed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<u32>,
    pub image: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Clone)]
pub struct Reply {
    pub author: String,
    pub content: String,
}

#[derive(Clone)]
pub struct Message {
    pub id: u64,
    pub author_id: u64,
    pub author: String,
    pub avatar: String,
    pub bot: bool,
    pub content: String,
    pub timestamp: i64,
    pub reply: Option<Reply>,
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
}

#[derive(Clone)]
pub struct Emoji {
    pub id: u64,
    pub name: String,
    pub animated: bool,
}

pub struct Upload {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub enum Command {
    LoadChannels(u64),
    LoadEmojis(u64),
    LoadMessages(u64),
    Send { channel: u64, text: String, files: Vec<Upload> },
    OpenDm(u64),
    JoinVoice { guild: u64, channel: u64 },
    LeaveVoice,
    SetMicMuted(bool),
    SetDeafened(bool),
    UpdateProfile { username: Option<String>, avatar: Option<String>, banner: Option<String>, bio: Option<String> },
}

pub enum Event {
    LoggedIn { profile: Profile, guilds: Vec<Guild> },
    Channels { guild: u64, channels: Vec<Channel> },
    Emojis { guild: u64, emojis: Vec<Emoji> },
    Messages { channel: u64, messages: Vec<Message> },
    /// `is_dm` is true when the message was sent in a direct message channel.
    NewMessage { channel: u64, message: Message, is_dm: bool },
    DmOpened { channel: u64, user_id: u64, name: String },
    ProfileUpdated(Profile),
    /// The voice channel the bot is connected to (guild, channel), or `None` once it left.
    Voice(Option<(u64, u64)>),
    Error(String),
}

/// What is kept alive while the bot is in a voice channel: dropping it stops the microphone and the speakers.
struct VoiceSession {
    guild: Id<GuildMarker>,
    _mic: Option<voice::Mic>,
    _speaker: voice::SpeakerGuard,
    mic_muted: Arc<AtomicBool>,
    deafened: Arc<AtomicBool>,
}

/// Plays what the other people in the voice channel say.
struct Hear {
    speaker: voice::Speaker,
}

#[async_trait]
impl VoiceEventHandler for Hear {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<VoiceEvent> {
        if let EventContext::VoiceTick(tick) = ctx {
            let mut mix: Vec<i32> = Vec::new();
            for data in tick.speaking.values() {
                if let Some(pcm) = &data.decoded_voice {
                    if mix.len() < pcm.len() {
                        mix.resize(pcm.len(), 0);
                    }
                    for (m, s) in mix.iter_mut().zip(pcm) {
                        *m += *s as i32;
                    }
                }
            }
            if !mix.is_empty() {
                let out: Vec<i16> = mix.iter().map(|&v| v.clamp(-32768, 32767) as i16).collect();
                self.speaker.push(&out);
            }
        }
        None
    }
}

fn short(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Attachment names may only hold ASCII letters, digits, dots, dashes and underscores.
fn safe_filename(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    if cleaned.trim_matches(|c| c == '_' || c == '.').is_empty() { "file".to_string() } else { cleaned }
}

fn id<T>(raw: u64) -> Option<Id<T>> {
    std::num::NonZeroU64::new(raw).map(Id::from)
}

/// The bot's own profile, as shown in the profile card.
#[derive(Clone, Default)]
pub struct Profile {
    pub id: u64,
    pub name: String,
    pub avatar: String,
    pub banner: Option<String>,
    pub accent: Option<u32>,
    pub bio: String,
    /// Account creation time (unix seconds), read from the ID.
    pub created: i64,
}

async fn fetch_profile(http: &Client) -> Result<Profile, String> {
    let user = http.current_user().await.map_err(short)?.model().await.map_err(short)?;
    let bio = match http.current_user_application().await {
        Ok(r) => r.model().await.map(|a| a.description).unwrap_or_default(),
        Err(_) => String::new(),
    };
    Ok(Profile {
        id: user.id.get(),
        avatar: avatar_url(user.id, user.avatar.as_ref(), 256),
        banner: user.banner.as_ref().map(|hash| format!("https://cdn.discordapp.com/banners/{}/{hash}.png?size=480", user.id)),
        accent: user.accent_color,
        name: user.name,
        bio,
        created: (((user.id.get() >> 22) + 1_420_070_400_000) / 1000) as i64,
    })
}

fn avatar_url(user_id: Id<UserMarker>, avatar: Option<&ImageHash>, size: u32) -> String {
    match avatar {
        Some(hash) => format!("https://cdn.discordapp.com/avatars/{user_id}/{hash}.png?size={size}"),
        None => format!("https://cdn.discordapp.com/embed/avatars/{}.png", (user_id.get() >> 22) % 6),
    }
}

fn convert(m: &twilight_model::channel::Message) -> Message {
    Message {
        id: m.id.get(),
        author_id: m.author.id.get(),
        author: m.author.name.clone(),
        avatar: avatar_url(m.author.id, m.author.avatar.as_ref(), 64),
        bot: m.author.bot,
        content: m.content.clone(),
        timestamp: m.timestamp.as_secs(),
        reply: m.referenced_message.as_ref().map(|r| Reply { author: r.author.name.clone(), content: r.content.clone() }),
        attachments: m
            .attachments
            .iter()
            .map(|a| Attachment {
                url: a.url.clone(),
                filename: a.filename.clone(),
                content_type: a.content_type.clone().unwrap_or_default(),
                size: a.size,
                width: a.width,
                height: a.height,
            })
            .collect(),
        embeds: m
            .embeds
            .iter()
            .map(|e| Embed {
                title: e.title.clone(),
                description: e.description.clone(),
                color: e.color,
                image: e.image.as_ref().map(|i| i.url.clone()),
                thumbnail: e.thumbnail.as_ref().map(|i| i.url.clone()),
            })
            .collect(),
    }
}

/// Runs until the UI side is dropped. Blocking: call it from a dedicated thread.
pub fn run(token: String, commands: UnboundedReceiver<Command>, events: Sender<Event>, repaint: impl Fn() + Send + Sync + 'static) {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build() {
        Ok(r) => r,
        Err(e) => {
            let _ = events.send(Event::Error(short(e)));
            return;
        }
    };
    runtime.block_on(async move {
        let repaint = Arc::new(repaint);
        let send = {
            let (events, repaint) = (events.clone(), repaint.clone());
            move |event: Event| {
                let _ = events.send(event);
                repaint();
            }
        };
        let http = std::sync::Arc::new(Client::new(token.clone()));

        let profile = match fetch_profile(&http).await {
            Ok(p) => p,
            Err(_) => return send(Event::Error("Discord refused this token. Check that it is a bot token.".into())),
        };
        let guilds = match http.current_user_guilds().await {
            Ok(r) => r.models().await.unwrap_or_default(),
            Err(e) => return send(Event::Error(short(e))),
        };
        let profile_id = profile.id;
        send(Event::LoggedIn {
            profile,
            guilds: guilds
                .iter()
                .map(|g| Guild {
                    id: g.id.get(),
                    name: g.name.clone(),
                    icon: g.icon.as_ref().map(|hash| format!("https://cdn.discordapp.com/icons/{}/{hash}.png?size=64", g.id)),
                })
                .collect(),
        });

        // incoming messages (needs the "Message Content" intent enabled in the developer portal)
        let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::DIRECT_MESSAGES | Intents::MESSAGE_CONTENT | Intents::GUILD_VOICE_STATES;
        let mut shard = Shard::new(ShardId::ONE, token, intents);
        let shards = HashMap::from([(shard.id().number(), shard.sender())]);
        let user_id: Id<UserMarker> = id(profile_id).expect("the bot has an ID");
        let songbird = Arc::new(Songbird::twilight_from_config(Arc::new(TwilightMap::new(shards)), user_id, SongbirdConfig::default().decode_mode(DecodeMode::Decode(DecodeConfig::default()))));
        let live = {
            let (events, repaint, songbird) = (events.clone(), repaint.clone(), songbird.clone());
            tokio::spawn(async move {
                let wanted = EventTypeFlags::MESSAGE_CREATE | EventTypeFlags::VOICE_STATE_UPDATE | EventTypeFlags::VOICE_SERVER_UPDATE;
                while let Some(item) = shard.next_event(wanted).await {
                    let Ok(event) = item else { continue };
                    songbird.process(&event).await;
                    if let GatewayEvent::MessageCreate(m) = event {
                        let _ = events.send(Event::NewMessage { channel: m.channel_id.get(), message: convert(&m), is_dm: m.guild_id.is_none() });
                        repaint();
                    }
                }
            })
        };
        let mut voice: Option<VoiceSession> = None;

        let handle_commands = async {
            let mut commands = commands;
            while let Some(command) = commands.recv().await {
                match command {
                    Command::LoadChannels(guild) => {
                        let Some(gid) = id(guild) else { continue };
                        match http.guild_channels(gid).await {
                            Ok(r) => {
                                let channels = r
                                    .models()
                                    .await
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter_map(|c| {
                                        let kind = match c.kind {
                                            ChannelType::GuildText | ChannelType::GuildAnnouncement => ChannelKind::Text,
                                            ChannelType::GuildVoice | ChannelType::GuildStageVoice => ChannelKind::Voice,
                                            ChannelType::GuildCategory => ChannelKind::Category,
                                            _ => return None,
                                        };
                                        Some(Channel {
                                            id: c.id.get(),
                                            name: c.name.unwrap_or_default(),
                                            kind,
                                            parent: c.parent_id.map(|p| p.get()),
                                            position: c.position.unwrap_or(0),
                                        })
                                    })
                                    .collect();
                                send(Event::Channels { guild, channels });
                            }
                            Err(e) => send(Event::Error(short(e))),
                        }
                    }
                    Command::LoadEmojis(guild) => {
                        let Some(gid) = id(guild) else { continue };
                        if let Ok(r) = http.emojis(gid).await {
                            let emojis = r
                                .models()
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .map(|e| Emoji { id: e.id.get(), name: e.name, animated: e.animated })
                                .collect();
                            send(Event::Emojis { guild, emojis });
                        }
                    }
                    Command::LoadMessages(channel) => {
                        let Some(cid) = id(channel) else { continue };
                        match http.channel_messages(cid).limit(50).await {
                            Ok(r) => {
                                let mut messages: Vec<Message> = r.models().await.unwrap_or_default().iter().map(convert).collect();
                                messages.reverse(); // Discord returns the newest first
                                send(Event::Messages { channel, messages });
                            }
                            Err(e) => send(Event::Error(format!("Cannot read this channel: {e}"))),
                        }
                    }
                    Command::Send { channel, text, files } => {
                        let Some(cid) = id(channel) else { continue };
                        let uploads: Vec<UploadAttachment> =
                            files.into_iter().enumerate().map(|(i, f)| UploadAttachment::from_bytes(safe_filename(&f.name), f.bytes, i as u64)).collect();
                        let mut request = http.create_message(cid);
                        if !text.is_empty() {
                            request = request.content(&text);
                        }
                        if !uploads.is_empty() {
                            request = request.attachments(&uploads);
                        }
                        if let Err(e) = request.await {
                            send(Event::Error(format!("Message not sent: {e}")));
                        }
                    }
                    Command::JoinVoice { guild, channel } => {
                        let (Some(gid), Some(cid)) = (id::<GuildMarker>(guild), id::<ChannelMarker>(channel)) else { continue };
                        if let Some(old) = voice.take() {
                            let _ = songbird.leave(old.guild).await;
                        }
                        let mic_muted = Arc::new(AtomicBool::new(false));
                        let deafened = Arc::new(AtomicBool::new(false));
                        let (speaker_guard, speaker) = match voice::start_speaker(deafened.clone()) {
                            Ok(s) => s,
                            Err(e) => {
                                send(Event::Error(e));
                                continue;
                            }
                        };
                        let mic = voice::start_mic(mic_muted.clone());
                        if let Err(e) = &mic {
                            send(Event::Error(format!("{e} You will be able to listen but not to speak.")));
                        }
                        match songbird.join(gid, cid).await {
                            Ok(call) => {
                                let mut handler = call.lock().await;
                                handler.add_global_event(CoreEvent::VoiceTick.into(), Hear { speaker });
                                let mic_guard = mic.ok().map(|(guard, info)| {
                                    handler.play_input(Input::from(RawAdapter::new(info.source, info.sample_rate, info.channels)));
                                    guard
                                });
                                drop(handler);
                                voice = Some(VoiceSession { guild: gid, _mic: mic_guard, _speaker: speaker_guard, mic_muted, deafened });
                                send(Event::Voice(Some((guild, channel))));
                            }
                            Err(e) => {
                                // the gateway part may have worked (the bot shows in the channel): make it leave
                                let _ = songbird.leave(gid).await;
                                send(Event::Error(format!("Cannot join this voice channel: {e}")));
                            }
                        }
                    }
                    Command::LeaveVoice => {
                        if let Some(old) = voice.take() {
                            let _ = songbird.leave(old.guild).await;
                        }
                        send(Event::Voice(None));
                    }
                    Command::SetMicMuted(muted) => {
                        if let Some(session) = &voice {
                            session.mic_muted.store(muted, Ordering::Relaxed);
                            if let Some(call) = songbird.get(session.guild) {
                                let _ = call.lock().await.mute(muted).await;
                            }
                        }
                    }
                    Command::SetDeafened(deaf) => {
                        if let Some(session) = &voice {
                            session.deafened.store(deaf, Ordering::Relaxed);
                            if let Some(call) = songbird.get(session.guild) {
                                let _ = call.lock().await.deafen(deaf).await;
                            }
                        }
                    }
                    Command::OpenDm(user) => {
                        let Some(uid) = id(user) else {
                            send(Event::Error("That is not a valid user ID.".into()));
                            continue;
                        };
                        match http.create_private_channel(uid).await {
                            Ok(r) => match r.model().await {
                                Ok(channel) => {
                                    let name = channel.recipients.and_then(|r| r.into_iter().next()).map(|u| u.name).unwrap_or_else(|| user.to_string());
                                    send(Event::DmOpened { channel: channel.id.get(), user_id: user, name });
                                }
                                Err(e) => send(Event::Error(short(e))),
                            },
                            Err(e) => send(Event::Error(format!("Cannot open this DM (the user must share a server with the bot): {e}"))),
                        }
                    }
                    Command::UpdateProfile { username, avatar, banner, bio } => {
                        if username.is_some() || avatar.is_some() || banner.is_some() {
                            let mut request = http.update_current_user();
                            if let Some(name) = username.as_deref() {
                                request = request.username(name);
                            }
                            if let Some(data) = avatar.as_deref() {
                                request = request.avatar(Some(data));
                            }
                            if let Some(data) = banner.as_deref() {
                                request = request.banner(Some(data));
                            }
                            if let Err(e) = request.await {
                                send(Event::Error(format!("Profile not updated (Discord limits how often it can change): {e}")));
                                continue;
                            }
                        }
                        if let Some(text) = bio.as_deref() {
                            if let Err(e) = http.update_current_user_application().description(text).await {
                                send(Event::Error(format!("Bio not updated: {e}")));
                                continue;
                            }
                        }
                        match fetch_profile(&http).await {
                            Ok(profile) => send(Event::ProfileUpdated(profile)),
                            Err(e) => send(Event::Error(e)),
                        }
                    }
                }
            }
        };

        handle_commands.await;
        live.abort();
    });
}
