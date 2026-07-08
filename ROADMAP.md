# spotamp — roadmap

Fork of [tedsteen/Spotiamp](https://github.com/tedsteen/Spotiamp) (MIT). Goal: our
own customizable Winamp-style Spotify player. Build works: `npm run tauri dev`
(needs `export PATH="$HOME/.cargo/bin:$PATH"`). Toolchain (Rust 1.96 + VS Build
Tools 2022 + node) already installed on this machine.

## Works today
- Login via Spotify OAuth (Premium), playback, seek, volume, spectrum data.
- **Load a whole playlist by dragging it from Spotify onto the PLAYLIST window**
  (backend `get_track_ids` expands playlist/album URIs → tracks).

## Known gaps (tedsteen's app is a minimal WIP)
- No "browse my playlists" library UI — you must drag playlists in.
- Bottom-bar buttons (ADD / REM / SEL / MISC) and the playlist mini-transport
  are decorative sprites with **no click handlers** wired.
- kbps / kHz / mono-stereo displays are empty (not populated).
- Right-click menu is crude.

## Roadmap (priority order)

### 1. Browse my playlists  ← ✅ WORKING (2026-07-08)
DONE & verified: rootlist-based browser works end to end. `get_user_playlists`
uses `spclient().get_rootlist(0, Some(500))`, scans the bytes for
`spotify:playlist:` URIs, resolves name + length via `Playlist::get`. Frontend
"♪ my playlists" button opens the overlay; clicking a playlist converts
`spotify:playlist:ID` → `https://open.spotify.com/playlist/ID` and calls
`playlist.addUrls([url])` (addUrls expects a URL, not a URI — that was the bug).
Results cached per session (only first open is slow).
Future polish: parallelize the per-playlist `Playlist::get` name resolution (add
`futures-util` dep, chunked `join_all`) — first open is currently sequential and
can take several seconds for many playlists. Also: playlist images (image: None
now), and moving the button out of the track area (done: moved into title bar).

--- original notes below (history) ---
### 1b. (history) Web API path — BLOCKED
DONE: UI is built — a green "♪ my playlists" button in the playlist window opens
an overlay list (`src/routes/playlist/+page.svelte`), click → `playlist.clear()`
+ `playlist.addUrls([uri])`. Backend command `get_user_playlists` exists
(`spotify.rs` + `player_window.rs` + registered in `lib.rs`).

BLOCKED: fetching the playlist list. Tried `session.token_provider()
.get_token[_with_client_id]("playlist-read-private,playlist-read-collaborative", ...)`
→ Spotify keymaster returns **403 "Invalid request"** for BOTH the android client
id (65b708073fc0480ea92a077233ca87bd) AND the web-player id
(d8a5ed958d274c2e8ee717e6a4b0971d). So the Web API `/me/playlists` path is dead —
these session-minted tokens can't get playlist-read scope.

RELIABLE FIX (do next, needs protobuf work): use librespot's internal rootlist,
which uses the same session auth that already works for playback + get_track_ids:
- `self.session.inner.spclient().get_rootlist(0, Some(500)).await` → `Bytes`
  (returns protobuf, endpoint `/playlist/v2/user/{user}/rootlist?decorate=...attributes...`).
- Parse: add deps `librespot-protocol` + `protobuf`. Use
  `protocol::playlist4_external::SelectedListContent::parse_from_bytes(&bytes)`
  then librespot-metadata's `SelectedListContent::try_from(&msg)` (it has
  `.attributes.name`, `.length`, `.contents.items` with playlist URIs). The
  `decorate=attributes` should include names, so no N extra `Playlist::get` calls.
- Map to our `UserPlaylist { name, uri, track_count, image }` and return.
- Fallback if decoration lacks names: `Playlist::get(&session, &uri).await` per
  playlist gives `.attributes.name` + `.length` (works, just slower).

Alternative (heavier, uncertain): change `oauth.rs` to request playlist-read
scopes, RETAIN the access token (currently consumed into Credentials), handle the
cached-credentials-skip-OAuth path (would force re-login) + token expiry/refresh.
Prefer the rootlist approach above.

### 2. Wire the dead buttons
- Playlist mini-transport (bottom of PLAYLIST window): prev/play/pause/stop/next
  → reuse the player-window handlers (play/pause/stop/load_track + row nav).
- ADD → open a "paste Spotify URL" dialog → addUrls. REM → remove selected rows.
  SEL → selection menu. MISC → misc menu.
- Sprite-overlay buttons need pixel-positioned clickable divs over PLEDIT.BMP
  regions. Iterate WITH screenshots (can't see the native window remotely).

### 3. kbps / kHz / mono-stereo displays
- Spotify OGG ≈ 320 kbps / 44 kHz / stereo — can show sensible static values,
  or pull real format from librespot if exposed.

### 4. Skins (our customization story)
- Skins are BMP sets under `src/static/assets/skins/base-2.91/`, referenced by
  CSS `--sprite-url`. Add a skin SWITCHER that swaps the base path; bundle a few
  classic `.wsz` skins (just zips of these BMPs). Later: design our own
  spotamp/termspot skin.

### 5. Right-click menu polish
### 6. Milkdrop-style visualizer
- The app already has an audioviz spectrum. A full Milkdrop port is heavy; a
  richer canvas visualizer (projectM-style) is the realistic target.

## Workflow notes
- Frontend (Svelte/CSS) hot-reloads in dev — fast. Rust changes need recompile
  (incremental ~fast after first build).
- I (the assistant) CANNOT screenshot the native window — every visual change
  needs Ahmet to send a screenshot. Plan iterations around that.
