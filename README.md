# DBYB: Discord, But You're A Bot

A desktop Discord client, written in Rust, where you log in **as a bot** instead of as a user. Browse the servers your bot is in, read and send messages, upload files, use emojis, open DMs, edit the bot's profile and even join voice channels, with an interface modelled on the Discord client.

> [!IMPORTANT]
> DBYB logs in with a **bot token**, through Discord's official API. Use it only with bots **you own** (or whose owner agrees). It is not a self-bot: it never touches a user account. It is an independent project, not affiliated with or endorsed by Discord.

## Features

- **Servers and channels**: server icons, channels grouped by category, voice channels listed.
- **Messages**: avatars, timestamps, BOT badges, grouped messages, reply previews, embeds, live updates.
- **Sending**: text, files, photos and videos (pick them with **+** or drop them on the window), custom and standard emojis.
- **Emojis**: custom emojis render as images, and the picker lists the emojis of **every server the bot is in**.
- **Direct messages**: open a DM from a user ID, or click an author's name. Conversations are remembered locally.
- **Profile**: a Discord-style card to change the bot's username, avatar, banner and bio.
- **Voice**: join a voice channel, talk with your microphone, hear the others (end-to-end encrypted calls are supported).
- **Private by design**: the token is stored in the Windows credential store (only if you tick the box) and is only ever sent to `discord.com`.

## Limits

What a bot can do is decided by Discord, not by this app:

- A bot can't join a server by itself. Someone with the *Manage Server* permission has to add it with the OAuth2 invite link.
- No video, camera or screen sharing, and no calls in DMs: bots can only use server voice channels.
- No member list (it needs a privileged intent) and no reactions yet.
- Changing a bot's username and images is rate-limited by Discord.
- Windows only for now (the token is stored with the Windows credential store).

## Setup

1. Create an application at the [Discord developer portal](https://discord.com/developers/applications), add a **Bot**, and copy its token.
2. In the **Bot** page, turn on **Message Content Intent** so the bot can read messages.
3. Invite the bot to your server with an OAuth2 URL (scope `bot`, with the permissions you want: view channels, send messages, attach files, connect and speak for voice).
4. Run DBYB and paste the token.

## Build

You need [Rust](https://rustup.rs), the MSVC build tools and [CMake](https://cmake.org/download/) (Opus, the voice codec, is built from source).

```powershell
git clone https://github.com/plagieur/dbyb.git
cd dbyb
cargo build --release
```

The executable is `target\release\dbyb.exe`.

If the Opus build fails with errors about file tracking, the path to the project is probably too long: build from a shorter folder.

## Data

Everything stays on your machine, in `%APPDATA%\dbyb`:

| File | What it is |
| --- | --- |
| `dms-<bot id>.json` | The DM conversations the bot has had (Discord can't list them for bots) |
| `voice.log` | Debug log of the voice connection, rewritten at every launch |

The token itself is in the Windows credential store, under the name `dbyb`. *Log out* removes it.

## Built with

[egui](https://github.com/emilk/egui) for the interface, [twilight](https://github.com/twilight-rs/twilight) for the Discord API and gateway, [songbird](https://github.com/serenity-rs/songbird) for voice, and [cpal](https://github.com/RustAudio/cpal) for audio.

## License

MIT, see [LICENSE](LICENSE).
