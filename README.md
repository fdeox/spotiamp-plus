<p align="center">
  <img src="docs/brand/banner.png" alt="Spotiamp+ — boost your music" width="100%">
</p>

[![Discord](https://img.shields.io/badge/Discord-join%20the%20community-5865F2?logo=discord&logoColor=white)](https://discord.gg/8Rq5Xycny4)

A Winamp-style desktop player for **Spotify** — the classic skinned windows, real
skins, a Milkdrop-style visualizer, and full keyboard control, playing your
Spotify Premium account natively (no browser) — and **your own local files**
right in the same playlist.

On top of that it adds a playlist and library browser, full catalogue search, and
docking windows that snap together like classic Winamp.

> **Premium** streams directly (playback via
> [librespot](https://github.com/librespot-org/librespot)); **Free** accounts get
> **Free Mode**, where Spotiamp+ becomes the Winamp face of the official Spotify
> app and drives it. Login always uses Spotify's own OAuth page.

![Spotiamp+ in action: skins, the Skin Museum, lyrics and the visualizer](docs/demo.gif)

---
>## Project Status

>Spotiamp+ is actively maintained, but development is currently moving at a slower and more gradual pace due to limited development time.
>Bug fixes, stability improvements, security updates, and important compatibility fixes will continue. New features will primarily be considered based on community requests, usefulness, and available time.
>Feature suggestions can be submitted through the Spotiamp+ Discord community.

## Features

- 🎵 **Native Spotify playback** — Premium account via librespot (Ogg 320 kbps),
  seek, volume, gapless.
- 🦙 **Lala** — a little llama who lives on your player. She dances to the beat,
  naps when the music stops, wakes up when you pat her, says something now and
  then (a fact about the song, your listening streak, a tip) and wanders over to
  your other windows when she's bored. Drag her along the edge, double-click her
  for a stroll, right-click her for her size, to quiet her or to hide her.
- 💿 **Local file playback** — play your own MP3, FLAC, M4A/AAC, WAV or OGG
  files right alongside Spotify. Add files or a whole folder from the playlist
  menu (or press **O** / **Shift+O**); they land in the playlist as normal rows,
  mixed with Spotify tracks, with next/previous walking the lot. Name, artist and
  length come from the file's own tags, and the EQ and visualizer work on them too.
  In Free Mode too.
- 📂 **Playlist browser & Library window** — browse your Spotify playlists, open
  a two-pane Library (playlists + tracks), load or queue anything. In the
  **Library window**, just start typing a song or artist name and the selection
  jumps to it (what you typed shows in the header). **Pin** your favourite
  playlists to the top (right-click one), and **drag** songs or whole playlists
  from the Library onto the playlist. The playlist's **List** button (bottom
  right) opens it, with the list commands.
- 🕘 **Recently played & Most played** — two lists in the Library, kept on your
  own computer by Spotiamp+ (nothing is sent anywhere).
- 📊 **Listening stats** — time listened, top songs and artists, your streak and
  when you listen, for the last 7 days, 30 days, year or all time, plus
  **48 badges** to unlock (night owl, marathon, 30 days in a row, Winamp's
  birthday…). **Copy** them as a picture to share (Library → *Listening stats*).
  Every December, **Rewind** looks back on your year.
- ♡ **Loved songs** — press **F** to love the playing song (or the selected ones
  in the playlist); they're in the Library under *Loved songs*, with a ♡ next to
  them everywhere.
- ⏹️ **Stop after current** — **Ctrl+V** stops when the song ends, like Winamp.
- 🖱️ **Song menus** — right-click a song, in the playlist or the Library, to play
  it next, love it, add it to a list, copy its Spotify link or show a local file
  in its folder.
- 📄 **M3U playlists** — open and save `.m3u` / `.m3u8` files (right-click →
  *Playlist*): local files and Spotify songs, in order.
- 💬 **On-screen display** (optional) — the new song, with its cover, in the
  corner of the screen for a few seconds (right-click → *Windows*).
- 🔢 **Jump to track & queue** — press **J** and type part of a song's name to
  find it in the playlist (arrows pick, **Enter** plays); press **Q** to play
  the selected song next. The number on a row is its place in the queue.
  **Ctrl+J** jumps to a time in the song (type `1:23`, **Enter**).
- 🪄 **Instant mix** — right-click → *Instant mix*: 20 songs like the selected
  one, added to the end of the playlist or queued to play next.
- ⏯️ **Picks up where you left off** — your last track is ready at the same
  spot when you open the app again.
- 🚫 **No duplicates** — a song is never listed twice; adding one that's
  already there says so instead.
- 🔎 **Spotify catalogue search** — search the whole catalogue right in the
  Library. Double-clicking a search result **adds it to the end of the current
  playlist** (it doesn't replace what's playing), so you can build a playlist by
  searching for songs one after another.
- 🧲 **Docking windows** — the Playlist, Equalizer, Library and Visualizer snap
  to the main window and move together, just like classic Winamp.
- 🎚️ **10-band Equalizer** — a pixel-perfect Winamp EQ window with a **real DSP**
  behind it (biquad peaking filters on the decoded audio), preamp, presets and
  the animated response curve. **Load and save `.EQF` presets** (yours, or real
  Winamp ones). **AUTO** works like Winamp's: save a curve for a song or an
  artist (*Presets*) and it comes back whenever they play. Your EQ is remembered
  between runs. Plus a **balance** slider next to the volume.
- 🎨 **Skins** — right-click → *Skins* to switch skins live: **Classic**,
  **Cherry**, **Amber**, **Emerald**, plus **six bundled classic Winamp skins**
  right in the menu — the *Classified* series by
  [Victhor](https://victhor.deviantart.com/) and the Sony/Nucleo hardware-style
  skins (all rights remain with their original authors). Open the
  **Skin Museum** right inside Spotiamp+ (*Skins → Skin Museum…*) to browse
  thousands of classic skins from the
  [Winamp Skin Museum](https://skins.webamp.org/) and put one on with a click
  (star the ones you like to find them under *Favorites*),
  or **load any Winamp 2.x skin (`.wsz`)** from disk. Every window — player,
  equalizer, playlist **and the media library** — reskins live and persists
  across restarts.
- 🌀 **WebGL visualizer** — a window with **100 audio-reactive patterns**:
  spectrum analysers, VU meters, a scrolling spectrogram, Milkdrop-style
  feedback modes, synthwave, a spinning record and more, cycling on click, on
  a timer, and on every track change. **Double-click it for fullscreen**
  (**Esc** to come back).
- 🎤 **Lyrics window** — synced, scrolling lyrics that highlight the current
  line in time with playback; **click a line to jump the song there**
  (right-click → *Windows → Lyrics*).
- 🖼️ **Album art window** — the current cover in its own resizable window that
  docks like the others (right-click → *Windows → Album art*).
- 🔍 **Scale** — every window at 1×, 1.5×, 2× or 3× (right-click →
  *Windows → Scale*), or **Ctrl+D** for 2×.
- 🔊 **Normalize volume** — evens out loudness between songs (right-click →
  *Audio*), like Spotify's own setting. Pick the **output device** there too.
- 📌 **Taskbar extras** (optional) — the song title on the taskbar button,
  progress across it and ⏮ ⏯ ⏭ buttons under its thumbnail (right-click →
  *Windows*).
- 🖼️ **Now Playing card** — right-click → *Copy Now Playing card*: an image of
  the player and the song, ready to paste into Discord or anywhere.
- 😴 **Sleep timer** — pause playback after 15, 30, 45 or 60 minutes (right-click
  → *Sleep timer*).
- 🎛️ **Media keys** — the play/pause/next keys on your keyboard and the buttons
  on your headset control playback even when Spotiamp+ is in the background
  (registered through the Windows media session).
- 🎮 **Discord Rich Presence** — shows the track, artist, **album art** and a
  live progress bar on your Discord profile while it plays, and clears the moment
  playback stops.
- 🪟 **Windowshade mode** — roll any window up to a slim title strip, classic
  Winamp style.
- ↕️ **Sortable library** — click a column header (Artist / Album / Title / Date
  added / Time) to sort your tracks.
- 🪟 **Remembers your layout** — the library, visualizer, lyrics and album art
  windows reopen where and how you left them on the next launch.
- 🔀 **Shuffle & 3-state repeat** — off → repeat-all → repeat-one.
- ⏱️ **Playlist time readouts** — current elapsed + total playlist time.
- 🔌 **Auto-reconnect** — recovers automatically if Spotify drops the session,
  and the song carries on where it was. A moment offline won't sign you out.
- 🔄 **Built-in updater** — Spotiamp+ flags a new version on launch, and
  right-click → *Help → Check for updates* downloads and installs it (signed).
  After an update, a *What's new* window lists the changes once.
- 🩺 **Diagnostic info** — something wrong? Right-click → *Help → Copy
  diagnostic info* (or the button in an error box) copies what's needed for a
  bug report.
- 🖱️ **Winamp-style right-click menu** — on the playlist and the main window,
  with each key shown next to what it does.
- ⌨️ **Keyboard shortcuts** — classic Winamp keys that work from every window
  (see below), plus the **mouse wheel** over the main window for volume. Press
  **F1** anywhere for the full list.

## Screenshots

| Classic | Cherry | Amber | Emerald |
| :-----: | :----: | :---: | :-----: |
| ![](docs/screenshots/classic2.jpg) | ![](docs/screenshots/cherry.jpg) | ![](docs/screenshots/amber.jpg) | ![](docs/screenshots/emerald.jpg) |

| Media Library | Visualizer |
| :-----------: | :--------: |
| ![](docs/screenshots/library.jpg) | ![](docs/screenshots/visualizer.jpg) |

| Equalizer | Lyrics |
| :-------: | :----: |
| ![](docs/screenshots/eq.jpg) | ![](docs/screenshots/lyrics.jpg) |

| Fullscreen visualizer |
| :-------------------: |
| ![](docs/fullscreen-visualizer.gif) |

### Load any classic Winamp skin

Right-click → *Skins → Skin Museum…* to browse and put one on with a click, or
*Skins → Load .wsz from disk…* (or pick one of the bundled skins). Every window —
player, equalizer, playlist and library — reskins live.

| Bento Classified | Winamp3 Classified | Winamp5 Classified |
| :--------------: | :----------------: | :----------------: |
| ![](docs/screenshots/winampbento.jpg) | ![](docs/screenshots/winamp3.jpg) | ![](docs/screenshots/winamp5.jpg) |

| Nucleo NLog | Sony CDX-MP3 | Sony Esprit V2 |
| :---------: | :----------: | :------------: |
| ![](docs/screenshots/Nucleo%20Nlog.jpg) | ![](docs/screenshots/Sony%20CDX-MP3.jpg) | ![](docs/screenshots/Sony%20Esprit%20V2.jpg) |

## Install

1. Download the latest installer from the [**Releases**](../../releases) page.
2. Run it. Windows SmartScreen may warn because the build is unsigned — choose
   **More info → Run anyway**.
3. Launch Spotiamp+ and log in with your Spotify account — **Premium** streams
   directly, **Free** falls back to Free Mode.

From then on, right-click → *Help → Check for updates* keeps you current — no
need to revisit this page. Windows 10/11 (x64) only for now.

## Keyboard shortcuts

These work in the main window **and every other window** (equalizer,
playlist, visualizer, lyrics, album art, stats). In the Library, typing searches
instead, so there only `Ctrl+D`, `Ctrl+V`, `Ctrl+J` and `F1` work. The right-click menu
shows each key next to what it does, and **`F1`** opens the whole list.

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `Z` `X` `C` `V` `B` | prev / play / pause / stop / next | `Space` | play–pause |
| `↑` `↓` / mouse wheel | volume | `←` `→` | seek ∓5s |
| `S` | shuffle | `R` | repeat (off / all / one) |
| `J` | jump to a track | `Q` | play the selected track next |
| `F` | love the song ♡ | `Ctrl+V` | stop after the current song |
| `L` | open Library | `O` / `Shift+O` | add local file(s) / folder |
| `Ctrl+D` | everything 2× bigger | `F1` | all the keyboard shortcuts |
| `Ctrl+J` | jump to a time in the song | | |

The mouse wheel works over the main window. In the playlist window the arrow
keys move the selection instead (see below).

**Playlist window**

| Key | Action |
| --- | --- |
| `Ctrl+A` | select all |
| `Delete` | remove selected |
| `Enter` | play selected |
| `↑` `↓` | move selection (`Shift` extends it, `Alt+↑/↓` reorders) |

Typing letters in the playlist doesn't search, because letters are shortcuts:
press **`J`** and type part of a song's name, pick with `↑` `↓`, `Enter` plays
it, `Esc` closes.

**Library window**

| Key | Action |
| --- | --- |
| start typing | jump to the first track whose title (or artist) matches |

**Visualizer:** double-click the main window's spectrum to open it; click it to
cycle patterns, double-click it for fullscreen, `Esc` to come back.

**In the Library, double-click…**

- a **playlist** → loads it into the playlist window and plays it
- a **playlist track** → plays from that track and keeps the rest queued
- a **search result** → appends it to the end of the current playlist (does not
  replace what's playing) — search again and again to build a list

<details>
<summary><b>Build from source</b></summary>

Requires Rust (stable), Node.js, and the platform toolchain for
[Tauri 2](https://v2.tauri.app/start/prerequisites/) (on Windows: VS C++ Build
Tools + WebView2).

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # produce a release installer (src-tauri/target/release/bundle)
```

</details>

---

<sub>[MIT License](LICENSE) (Lala's art and animations: © fdeox, all rights reserved, see [their license](src/lib/mascot/LICENSE.md)) · [Privacy](PRIVACY.md) · based on [tedsteen/Spotiamp](https://github.com/tedsteen/Spotiamp) · not affiliated with Winamp or Spotify</sub>
