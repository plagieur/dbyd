# DBYB: Discord, But You're A Bot

A desktop Discord client, written in Rust, where you log in **as a bot** instead of as a user. Browse the servers your bot is in, chat with Discord's markdown, reply, edit, send files and GIFs, use emojis, open DMs, edit the bot's profile and even join voice channels, with an interface modelled on the Discord client that you can theme completely.

> [!IMPORTANT]
> DBYB logs in with a **bot token**, through Discord's official API. Use it only with bots **you own** (or whose owner agrees). It is not a self-bot: it never touches a user account. It is an independent project, not affiliated with or endorsed by Discord.

## Features

**Chat**
- Servers with their icons, channels grouped by category, voice channels listed.
- Messages with avatars, timestamps, BOT badges, grouped messages, reply previews, embeds, `(edited)` marks and live updates (new, edited and deleted messages).
- **Markdown** like Discord: `**bold**`, `*italic*`, `__underline__`, `~~strikethrough~~`, `||spoilers||`, `` `code` ``, code blocks, `> quotes`, `# headings`, clickable links, custom emojis and `@mentions`.
- **Reply** to a message (button on hover, or right-click), with the quoted preview above the answer.
- **Right-click a message**: reply, copy the text, link or ID, send a DM to its author, and for the bot's own messages **edit** and **delete**.
- **Formatting from the message box**: right-click inside it to wrap the selected text in bold, italic, underline, strikethrough, spoiler, code, code block or quote (click again to remove it). `Ctrl+B`, `Ctrl+I` and `Ctrl+U` work too.

**Media**
- Send text, files, photos and videos (pick them with **+** or drop them on the window).
- Links to images and GIFs are shown as the image itself, and animated GIFs play. Tenor and Giphy previews are turned into real animated GIFs.
- **GIF picker** powered by [Klipy](https://klipy.com): trending and search. It needs your own free API key, asked the first time you open the picker and kept in the Windows credential store (never in a file).
- Custom emojis render as images, and the emoji picker lists the emojis of **every server the bot is in**, plus standard emojis.

**Direct messages**
- Open a DM from a user ID, or from the name of an author or the right-click menu. The list shows each person's picture, and conversations are remembered locally.

**Members and profiles**
- **Member list** on the right of a server channel, like Discord: *Online* and *Offline* groups, each member with their picture, a **status dot** (online, idle, do not disturb, offline), a BOT tag, and what they are doing ("Playing…", "Listening to…", custom status). Statuses update live, and members who join or leave appear or disappear. Servers with thousands of members stay smooth.
- The button at the top right of a server channel shows or hides the list. It needs the **Server Members** and **Presence** intents turned on for your bot (see Setup); DBYB tells you if they are not.
- **Profile card**: click a member (in the list or by their name in the chat) to see their banner, picture and status, nickname and username, **badges**, role list with colours, when they joined the server, since when they are on Discord, and since when they boost the server. From the card you can send them a message or copy their ID.
- **Badges** are Discord's own pictures, loaded from Discord's servers when needed (only the first Bug Hunter level, which has no address we know, is shipped with DBYB): Staff, Partner, Moderator Programs Alumni, HypeSquad (Events, Bravery, Brilliance, Balance), Bug Hunter (gold), Early Supporter, Early Verified Bot Developer, Active Developer. The **server boost badge grows with time** (1, 2, 3, 6, 9, 12, 15, 18, 24 months). The Nitro badge is a guess, see Limits.

**Profile and voice**
- A Discord-style profile card to change the bot's own username, avatar, banner and bio.
- Join a voice channel, talk with your microphone and hear the others (end-to-end encrypted calls are supported). Mute and deafen buttons are in the bar that appears while connected.

**Make it yours** (the sliders icon at the bottom left)
- **Themes**: *Dark grey*, *Black*, *Light* and *Midnight* are built in. Edit any of the ten colours live and save the result as your own theme.
- **Interface size** from 80 % to 150 %.
- **Icons**: a built-in set of 23 vector icons, tinted with the theme (or a colour you choose), with a size slider, and **icon packs**.
- **Cursor** (experimental): three built-in styles, or your own.

**Private by design**: the token is stored in the Windows credential store (only if you tick the box) and is only ever sent to `discord.com`.

## Themes, icon packs and cursors

Everything lives in `%APPDATA%\dbyb`. The settings window has a button that opens each folder.

**Themes** are small JSON files in `themes\`, with colours as `#rrggbb`. Copy one, edit it, share it:

```json
{
  "name": "My theme",
  "rail": "#1e1f22", "sidebar": "#2b2d31", "bg": "#313338", "input": "#383a40", "hover": "#404248",
  "line": "#3f4146", "ink": "#dbdee1", "muted": "#949ba4", "dim": "#6e737a", "bad": "#f06e6e"
}
```

**Icon packs** are folders in `icon-packs\<pack name>`. A file replaces the built-in icon of the same name (`.svg`, `.png`, `.webp`, `.gif` or `.jpg`); the icons you don't provide stay as they are. The names are: `plus`, `smile`, `gif`, `sliders`, `user`, `logout`, `reply`, `copy`, `trash`, `edit`, `close`, `search`, `hash`, `speaker`, `mic`, `mic_off`, `headphones`, `headphones_off`, `leave`, `send`, `message`, `image`, `plus_circle`, `members`. Icons are tinted with the theme colour, so draw them in white.

**Cursors** are folders in `cursors\<style name>` holding `arrow`, `pointer` (over buttons and links) and `text` (over text boxes), as `.svg` or `.png`. The active point is the top-left corner (the centre for `text`). While a custom cursor is on, DBYB hides the Windows cursor in its window and draws the picture itself, so it only exists inside the window and can lag a little.

## Limits

What a bot can do is decided by Discord, not by this app:

- A bot can't join a server by itself. Someone with the *Manage Server* permission has to add it with the OAuth2 invite link.
- A bot can't be added to a group DM, and it can only edit and delete its own messages.
- No video, camera or screen sharing, and no calls in DMs: bots can only use server voice channels.
- The member list and the statuses need two privileged intents, which you turn on yourself for your own bot.
- A bot can't see another user's **"About me"** text, **Nitro subscription** (and so its level: Starter to Opal) or **boosts on other servers**. DBYB shows the Nitro badge when someone has an animated picture or a banner (only Nitro allows them), which is a hint and not proof. The boost badge only reflects the boost of the server you are looking at, so it is a minimum.
- No reactions yet.
- Changing a bot's username and images is rate-limited by Discord.
- Windows only for now (the token is stored with the Windows credential store).

## Setup

1. Create an application at the [Discord developer portal](https://discord.com/developers/applications), add a **Bot**, and copy its token.
2. In the **Bot** page, turn on **Message Content Intent** so the bot can read messages.
3. Invite the bot to your server with an OAuth2 URL (scope `bot`, with the permissions you want: view channels, send messages, attach files, connect and speak for voice).
4. Run DBYB and paste the token.
5. Optional: for GIFs, get a free key at [partner.klipy.com](https://partner.klipy.com) and paste it the first time you open the GIF picker.
6. Optional: for the member list and statuses, turn on **Server Members Intent** and **Presence Intent** on the **Bot** page of the developer portal, then use the members button at the top right of a server channel (or the settings). If you turn the list on without them, Discord refuses the connection: DBYB notices, turns the list off again and explains.

## Build

You need [Rust](https://rustup.rs), the MSVC build tools and [CMake](https://cmake.org/download/) (Opus, the voice codec, is built from source).

```powershell
git clone https://github.com/plagieur/dbyd.git
cd dbyd
cargo build --release
```

The executable is `target\release\dbyb.exe`. `cargo test` runs the tests (markdown, formatting, icons, badges).

If the Opus build fails with errors about file tracking, the path to the project is probably too long: build from a shorter folder.

## Data

Everything stays on your machine, in `%APPDATA%\dbyb`:

| File | What it is |
| --- | --- |
| `settings.json` | Theme, interface size, icon pack, icon colour and size, cursor, member list on or off |
| `themes\`, `icon-packs\`, `cursors\` | Your own themes, icon packs and cursors |
| `dms-<bot id>.json` | The DM conversations the bot has had (Discord can't list them for bots) |
| `voice.log` | Debug log of the voice connection, rewritten at every launch |
| `crash.log` | If DBYB crashes: the message, where it happened and a backtrace |

The token is in the Windows credential store under the name `dbyb` (`bot-token`), and the Klipy key under `klipy-key`. *Log out* removes the token; *Change key* in the GIF picker removes the key.

## Built with

[egui](https://github.com/emilk/egui) for the interface, [twilight](https://github.com/twilight-rs/twilight) for the Discord API and gateway, [songbird](https://github.com/serenity-rs/songbird) for voice, [cpal](https://github.com/RustAudio/cpal) for audio, [reqwest](https://github.com/seanmonstar/reqwest) for the GIF search and [Klipy](https://klipy.com) for the GIFs.

## License

MIT, see [LICENSE](LICENSE).
