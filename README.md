# Spotiamp+

A Winamp-style desktop player for **Spotify** — the classic skinned windows, real
skins, a Milkdrop-style visualizer, and full keyboard control, playing your
Spotify Premium account natively (no browser).

Spotiamp+ is a fork of [**tedsteen/Spotiamp**](https://github.com/tedsteen/Spotiamp)
(MIT), extended with a playlist/library browser, catalogue search, docking,
skins, a WebGL visualizer, and much more.

> Requires a **Spotify Premium** account. Playback is handled by
> [librespot](https://github.com/librespot-org/librespot); login uses Spotify's
> own OAuth page.

<!-- Replace the paths below with your own screenshots (put them in docs/). -->
![Spotiamp+](docs/screenshots/main.png)

---

## Features

- 🎵 **Native Spotify playback** — Premium account via librespot (Ogg 320 kbps),
  seek, volume, gapless.
- 📂 **Playlist browser & Library window** — browse your Spotify playlists, open
  a two-pane Library (playlists + tracks), load or queue anything.
- 🔎 **Spotify catalogue search** — search the whole catalogue right in the
  Library; add results to your playlist one by one.
- 🧲 **Docking windows** — the Playlist, Library and Visualizer snap to the main
  window and move together, just like classic Winamp.
- 🎨 **Skins** — right-click the playlist to switch skins live: **Classic**,
  **Cherry**, **Amber**, **Emerald**. Persisted across restarts.
- 🌀 **Milkdrop-style visualizer** — a WebGL window with **12 audio-reactive
  patterns** that cycle on click, on a timer, and on every track change.
- 🔀 **Shuffle & 3-state repeat** — off → repeat-all → repeat-one.
- ⏱️ **Playlist time readouts** — current elapsed + total playlist time.
- 🔌 **Auto-reconnect** — recovers automatically if Spotify drops the session.
- ⌨️ **Keyboard shortcuts** — classic Winamp keys (see below).

## Screenshots

| Classic | Cherry | Amber | Emerald |
| :-----: | :----: | :---: | :-----: |
| ![](docs/screenshots/skin-classic.png) | ![](docs/screenshots/skin-cherry.png) | ![](docs/screenshots/skin-amber.png) | ![](docs/screenshots/skin-emerald.png) |

| Library | Visualizer |
| :-----: | :--------: |
| ![](docs/screenshots/library.png) | ![](docs/screenshots/visualizer.png) |

## Install

1. Download the latest installer from the [**Releases**](../../releases) page.
2. Run it. Windows SmartScreen may warn because the build is unsigned — choose
   **More info → Run anyway**.
3. Launch Spotiamp+ and log in with your Spotify (Premium) account.

## Keyboard shortcuts

**Main window**

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `Z` `X` `C` `V` `B` | prev / play / pause / stop / next | `Space` | play–pause |
| `↑` `↓` | volume | `←` `→` | seek ∓5s |
| `S` | shuffle | `R` | repeat |
| `L` | open Library | | |

**Playlist window**

| Key | Action |
| --- | --- |
| `Ctrl+A` | select all |
| `Delete` | remove selected |
| `Enter` | play selected |
| `↑` `↓` | move selection (`Alt+↑/↓` reorder) |
| `Z X C V B` | transport (forwarded to the player) |

Double-click the main window's spectrum to open the visualizer; click the
visualizer to cycle patterns.

## Build from source

Requires Rust (stable), Node.js, and the platform toolchain for
[Tauri 2](https://v2.tauri.app/start/prerequisites/) (on Windows: VS C++ Build
Tools + WebView2).

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # produce a release installer (src-tauri/target/release/bundle)
```

## Credits

Built on [**tedsteen/Spotiamp**](https://github.com/tedsteen/Spotiamp) (MIT) —
the original Tauri + librespot Winamp-style player. Spotiamp+ adds the browser,
search, library, docking, skins, visualizer and the rest.

Winamp is a trademark of its respective owners; this is an independent
fan project and is not affiliated with or endorsed by Winamp or Spotify.

## License

[MIT](LICENSE) — original © Ted Steen, additions © fdeox.
