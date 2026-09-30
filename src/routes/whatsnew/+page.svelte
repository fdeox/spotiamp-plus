<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";

  // Set before the first paint so the window opens at this size.
  REACTIVE_WINDOW_SIZE.setSize(320, 380);

  // What each version added, in plain words, newest at the top. Only what the
  // version really has; a version without an entry just shows the keys.
  /** @type {Record<string, string[]>} */
  const NOTES = {
    "0.7.4": [
      "<b>Listening stats</b>: time listened, top songs and artists, your streak and when you listen, for the last 7 days, 30 days, year or all time (Library, or right-click the playlist, <i>Windows</i>). <i>COPY</i> puts them on the clipboard as a picture. Kept on this computer only.",
      "<b>F</b> loves a song ♡: the playing one in the main window, the selected ones in the playlist. They're in the Library under <i>Loved songs</i>, with a ♡ next to them everywhere.",
      "<b>Ctrl+V</b>: stop after the current song, like Winamp.",
      "Right-click a song, in the playlist or the Library, for its own commands: play next, love, add to a list, copy its Spotify link, show a local file in its folder.",
      "Open and save <b>.m3u</b> playlist files (right-click the playlist, <i>Playlist</i>): local files and Spotify songs, in order.",
      "Optional on-screen display: the new song, with its cover, in the corner of the screen for a few seconds (right-click the playlist, <i>Windows</i>).",
      "Repeat one and autoplay now work for local files too.",
      "Lighter on the CPU while music plays.",
      "Fixed: opening the right-click menu right after picking something in it (like a skin) could freeze the whole app.",
      "When Spotify stops sending songs (it happens now and then, often right after the app opens), Spotiamp+ reconnects within seconds instead of skipping songs in silence for half a minute.",
    ],
    "0.7.3": [
      "<b>J</b> jumps to any track: type part of its name, pick with the arrows, <b>Enter</b> plays it.",
      "<b>Q</b> queues the selected track to play next. The number on the row is its place in the queue.",
      "Picks up where you left off: your last track is ready at the same spot when you open the app.",
      "Scale every window 1× to 3× (right-click the playlist, <i>Windows</i>), or <b>Ctrl+D</b> for 2×.",
      "<b>Skin Museum</b>: browse thousands of classic Winamp skins and put one on with a click (right-click the playlist, <i>Skins</i>).",
      "50 new visualizer patterns, 100 in all: synthwave, a moonlit sea, an ECG, a spinning record, the C64 maze and more.",
      "Fullscreen visualizer: double-click it, <b>Esc</b> to come back. It also opens instantly now.",
      "<i>Copy Now Playing card</i> (right-click the playlist): an image of the player and the song to paste into Discord.",
      "Mouse wheel over the main window changes the volume.",
      "The keyboard shortcuts work from every window now, not just the main one.",
      "The right-click menu is a proper Winamp-style menu, with each key shown next to what it does. It opens on the main window too.",
      "Click a line of synced lyrics to jump the song there.",
      "<i>Instant mix</i> (right-click menu): 20 songs like the selected one, added to the list or queued to play next.",
      "The Library has <i>Recently played</i> and <i>Most played</i>, kept on this computer by Spotiamp+.",
      "Pin playlists to the top of the Library (right-click one), and drag songs or playlists from the Library onto the playlist.",
      "<i>Normalize volume</i> evens out loudness between songs (right-click the playlist, <i>Audio</i>).",
      "Optional taskbar extras: song title, progress and ⏮ ⏯ ⏭ buttons (right-click the playlist, <i>Windows</i>).",
      "The playlist's total time now counts every track.",
      "Something wrong? The error box (and the <b>?</b> tab) can copy diagnostic info for a bug report.",
      "Long playlists fill in many times faster, in the playlist and in the Library.",
      "Fixed: in a long playlist some songs got stuck on \"Failed to load\" or \"loading…\" and wouldn't play.",
      "Fixed: lists you saved looked empty in the Library.",
      "Fixed: after an hour or so of music the windows could crash to a \"!\" page.",
    ],
  };

  const KEYS = [
    ["J", "Jump to a track"],
    ["Q", "Play the selected track next"],
    ["F", "Love a song ♡"],
    ["Z X C V B", "Previous, play, pause, stop, next"],
    ["Ctrl+V", "Stop after the current song"],
    ["← →", "Seek 5 seconds"],
    ["↑ ↓ / wheel", "Volume"],
    ["S / R", "Shuffle, repeat"],
    ["Ctrl+D", "Everything 2× bigger"],
    ["O / Shift+O", "Add music files / a folder"],
    ["L", "Open the Library"],
  ];

  let version = $state("");
  const notes = $derived(NOTES[version] ?? []);

  onMount(() => {
    getVersion()
      .then((v) => (version = v))
      .catch(() => {});
  });

  const close = () => invoke("close_whats_new").catch(() => {});
  const releaseNotes = () => invoke("open_external", { target: "releases" }).catch(() => {});

  /** @param {MouseEvent} e */
  function drag(e) {
    if (e.button !== 0) return;
    if (/** @type {HTMLElement} */ (e.target).closest("[data-no-drag]")) return;
    getCurrentWindow().startDragging().catch(() => {});
  }

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (e.key === "Escape" || e.key === "Enter") close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="wn">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="wn-titlebar" onmousedown={drag}>
    <div class="wn-tl"></div>
    <span class="wn-title">WHAT'S NEW</span>
    <button class="wn-close" data-no-drag onclick={close} aria-label="Close"></button>
  </div>

  <div class="wn-body">
    <h1>Spotiamp+ {version}</h1>
    {#if notes.length}
      <h2>New in this version</h2>
      <ul>
        {#each notes as note}
          <!-- the notes are the constants above, not user data -->
          <li>{@html note}</li>
        {/each}
      </ul>
    {/if}
    <h2>Keys worth knowing</h2>
    <table>
      <tbody>
        {#each KEYS as [key, what]}
          <tr>
            <td class="wn-key"><kbd>{key}</kbd></td>
            <td>{what}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="wn-hint">
      The keys work in every window, except the Library, where typing searches.
      Everything else is in the right-click menu, with each key shown next to
      what it does. You can open this again from its <b>Help</b> menu.
    </p>
  </div>

  <div class="wn-footer">
    <button class="wn-btn" onclick={releaseNotes}>Release notes</button>
    <button class="wn-btn wn-ok" onclick={close}>OK</button>
  </div>
</div>

<style>
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }

  :global(html),
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }

  .wn {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* The same skin-following frame as the other plugin windows. */
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    background: var(--frame);
    box-sizing: border-box;
    padding: 0 2px 2px;
    border: 1px solid var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    box-shadow:
      inset 1px 1px 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset 2px 0 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset -2px -2px 0 var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    user-select: none;
  }

  .wn-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill) repeat-x;
    cursor: default;
  }
  .wn-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl) no-repeat;
  }
  .wn-title {
    position: absolute;
    left: 50%;
    top: 0;
    transform: translateX(-50%);
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    line-height: 1;
    padding: 0 10px 5px;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    letter-spacing: 1px;
    white-space: nowrap;
    color: var(--skin-titletext, var(--skin-genexhdrtext, #cdd6ea));
    background: var(--skin-gentitle, transparent) repeat-x;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.55);
    z-index: 1;
  }
  .wn-close {
    position: absolute;
    right: 0;
    top: 0;
    width: 15px;
    height: 20px;
    background: var(--skin-gentr) no-repeat;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 2;
  }

  .wn-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0 4px;
    padding: 6px 9px 8px;
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 11px;
    -webkit-font-smoothing: none;
    line-height: 1.45;
    color: var(--skin-plnormal, #00ff41);
    user-select: text;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 40%, transparent) transparent;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 700;
    color: var(--skin-plcurrent, #fff);
  }
  h2 {
    margin: 10px 0 3px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--skin-plcurrent, #fff);
  }
  ul {
    margin: 0;
    padding-left: 13px;
  }
  li {
    margin: 0 0 4px;
  }
  li :global(b) {
    color: var(--skin-plcurrent, #fff);
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  td {
    padding: 2px 0;
    vertical-align: top;
  }
  .wn-key {
    width: 1%;
    white-space: nowrap;
    padding-right: 8px;
  }
  kbd {
    display: inline-block;
    padding: 0 4px;
    font-family: inherit;
    font-size: 10px;
    color: var(--skin-plcurrent, #fff);
    border: 1px solid color-mix(in srgb, var(--skin-plnormal, #00ff41) 55%, transparent);
    border-radius: 2px;
  }
  .wn-hint {
    margin: 10px 0 0;
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 70%, transparent);
  }

  .wn-footer {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding: 5px 4px 3px;
  }
  .wn-btn {
    font-family: "px sans nouveaux", sans-serif;
    font-size: 10px;
    -webkit-font-smoothing: none;
    padding: 2px 10px;
    color: var(--skin-titletext, var(--skin-genexbtntext, #fff));
    background: color-mix(in srgb, var(--frame) 88%, #fff);
    border: 1px solid;
    border-color: color-mix(in srgb, var(--frame) 60%, #fff)
      color-mix(in srgb, var(--frame) 50%, #000) color-mix(in srgb, var(--frame) 50%, #000)
      color-mix(in srgb, var(--frame) 60%, #fff);
    cursor: pointer;
  }
  .wn-btn:active {
    border-color: color-mix(in srgb, var(--frame) 50%, #000)
      color-mix(in srgb, var(--frame) 60%, #fff) color-mix(in srgb, var(--frame) 60%, #fff)
      color-mix(in srgb, var(--frame) 50%, #000);
  }
  .wn-ok {
    min-width: 54px;
  }
</style>
