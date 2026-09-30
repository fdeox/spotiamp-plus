<script>
  import {
    enterExitViewport,
    range,
    handleDrop,
    handleError,
    copyDiagnostics,
    REACTIVE_WINDOW_SIZE,
  } from "$lib/common.svelte.js";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { Menu } from "@tauri-apps/api/menu";
  import { emit } from "@tauri-apps/api/event";
  import { onMount, tick, untrack } from "svelte";
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { Playlist } from "$lib/playlist.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { ask, message } from "@tauri-apps/plugin-dialog";

  /** @type {{data: import('./$types').PageData}} */
  const { data: playlistSettings } = $props();

  function applyInitialWindowSize() {
    if (!playlistSettings.window_state.inner_size) {
      return;
    }

    const { width, height } = playlistSettings.window_state.inner_size;
    REACTIVE_WINDOW_SIZE.setSize(width, height);
  }

  // Controller ("free") mode: no librespot session exists, so loading the
  // saved tracks (each needs session metadata) would only produce errors —
  // the playlist starts empty and the saved URIs stay untouched in settings
  // for when the user returns to Premium mode.
  const controllerMode = playlistSettings.controller_mode === true;

  function createInitialPlaylist() {
    return new Playlist(controllerMode ? [] : playlistSettings.uris, !controllerMode);
  }

  applyInitialWindowSize();
  const playlist = createInitialPlaylist();

  // --- J: jump to file (Winamp) ---
  // A search box over the track list: type to filter the whole playlist, move
  // with the arrows, Enter plays, Esc closes. Opened by J here or in the main
  // window.
  let jumpQuery = $state("");
  let jumpActive = $state(0);
  /** @type {HTMLInputElement | undefined} */
  let jumpInput = $state();
  /** @type {HTMLElement | undefined} */
  let jumpListEl = $state();
  // Case- and accent-insensitive, like the library's type-to-find.
  /** @param {string} s */
  const fold = (s) =>
    (s || "")
      .toLowerCase()
      .normalize("NFD")
      .replace(/\p{M}/gu, "")
      .replace(/ı/g, "i");
  // Names are re-filtered every time one loads in the background, so fold each
  // distinct name once instead of on every pass over a long playlist.
  /** @type {Map<string, string>} */
  const foldCache = new Map();
  /** @param {string} s */
  const foldName = (s) => {
    let f = foldCache.get(s);
    if (f === undefined) {
      f = fold(s);
      foldCache.set(s, f);
    }
    return f;
  };
  const jumpMatches = $derived.by(() => {
    if (!playlist.jumpOpen) return [];
    const q = fold(jumpQuery.trim());
    const rows = playlist.rows;
    const out = [];
    for (let i = 0; i < rows.length && out.length < 200; i++) {
      if (!q || foldName(rows[i].displayName).includes(q)) out.push({ row: rows[i], n: i + 1 });
    }
    return out;
  });
  // Spotify rows fill their names in lazily; say so instead of "no match".
  const jumpLoadingNames = $derived(
    playlist.jumpOpen && playlist.rows.some((r) => !r.isLocal && !(/** @type {any} */ (r).track)),
  );
  $effect(() => {
    if (!playlist.jumpOpen) return;
    untrack(() => {
      jumpQuery = "";
      jumpActive = 0;
      const win = getCurrentWindow();
      // J from the main window while the playlist is hidden: nothing to show.
      win
        .isVisible()
        .then((visible) => {
          if (!visible) {
            playlist.jumpOpen = false;
            return;
          }
          win.setFocus().catch(() => {});
          tick().then(() => jumpInput?.focus());
        })
        .catch(() => {});
      playlist.loadAllNames();
    });
  });
  // A new query starts from the top match.
  $effect(() => {
    jumpQuery;
    jumpActive = 0;
  });
  function closeJump() {
    playlist.jumpOpen = false;
  }
  /** @param {any} row */
  function jumpPlay(row) {
    closeJump();
    playlist.select(row);
    row.play();
  }
  /** @param {KeyboardEvent} e */
  function onJumpKey(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeJump();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      jumpActive = Math.max(0, Math.min(jumpMatches.length - 1, jumpActive + step));
      tick().then(() =>
        jumpListEl?.querySelector(".jump-item.active")?.scrollIntoView({ block: "nearest" }),
      );
    } else if (e.key === "Enter") {
      e.preventDefault();
      const hit = jumpMatches[jumpActive];
      if (hit) jumpPlay(hit.row);
    }
  }

  // --- "my playlists" library browser (our addition) ---
  let showLibrary = $state(false);
  let libraryLoading = $state(false);
  let libraryError = $state("");
  let libraryPlaylists = $state([]);
  let librarySearch = $state("");
  const filteredPlaylists = $derived(
    librarySearch.trim()
      ? libraryPlaylists.filter((p) =>
          p.name.toLowerCase().includes(librarySearch.toLowerCase()),
        )
      : libraryPlaylists,
  );

  // Opens the standalone Library window (built on demand in Rust).
  const openLibraryWindow = () =>
    invoke("set_library_window_visible", { visible: true });

  // m:ss for the bottom-bar time readouts
  function fmtTime(ms) {
    const s = Math.floor((ms || 0) / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }

  // --- right-click menu ---
  let currentSkin = $state("classic");
  invoke("get_skin")
    .then((s) => (currentSkin = s || "classic"))
    .catch(() => {});
  // Which .wsz is on: the settings only say "custom", so the menu remembers
  // the pick itself ("bundled:<name>", "file" or "museum") to tick the right
  // entry. Only a convenience: without it, nothing custom is ticked.
  const WORN_KEY = "spotiamp.wornSkin";
  let wornCustom = $state(readWorn());
  function readWorn() {
    try {
      return localStorage.getItem(WORN_KEY) ?? "";
    } catch {
      return "";
    }
  }
  /** @param {string} value */
  function setWorn(value) {
    wornCustom = value;
    try {
      localStorage.setItem(WORN_KEY, value);
    } catch {
      /* storage off: the tick just won't survive a restart */
    }
  }

  // --- in-app updates ---
  // The manifest and the downloaded package are both signature-checked against
  // the public key baked into tauri.conf.json, so a tampered update is refused.
  let updateBusy = $state(false);
  // Set to the new version string once a silent launch check finds an update, so
  // a small "update available" pill can surface it (most users never open the
  // menu to check manually). Null while up to date / unknown.
  let updateAvailable = $state(/** @type {string | null} */ (null));

  // Quietly ask the updater on launch — no dialog, just light up the pill.
  async function checkForUpdatesSilently() {
    try {
      const update = await check();
      if (update) updateAvailable = update.version;
    } catch {
      /* offline or check failed — stay quiet, the menu still works */
    }
  }

  async function checkForUpdates() {
    if (updateBusy) return;
    updateBusy = true;
    try {
      const update = await check();
      if (!update) {
        updateAvailable = null;
        await message("You're running the latest version.", {
          title: "Spotiamp+",
          kind: "info",
        });
        return;
      }
      updateAvailable = update.version;
      const wanted = await ask(
        `Spotiamp+ ${update.version} is available (you have ${update.currentVersion}).\n\nDownload and install it now?`,
        { title: "Update available", kind: "info" },
      );
      if (!wanted) return;
      await update.downloadAndInstall();
      const restart = await ask(
        "Update installed. Restart Spotiamp+ now to finish?",
        { title: "Spotiamp+", kind: "info" },
      );
      if (restart) await relaunch();
    } catch (e) {
      await message(`Couldn't check for updates.\n\n${e}`, {
        title: "Spotiamp+",
        kind: "error",
      });
    } finally {
      updateBusy = false;
    }
  }

  // --- always on top ---
  let alwaysOnTop = $state(false);
  let normalizeVolume = $state(false);
  let taskbarExtras = $state(false);
  // Song title on the taskbar button, progress across it, and prev/play/next
  // under its thumbnail. Opt-in.
  async function toggleTaskbarExtras() {
    taskbarExtras = !taskbarExtras;
    await invoke("set_taskbar_extras", { enabled: taskbarExtras }).catch(() => {});
  }
  async function loadAlwaysOnTop() {
    try {
      const settings = await invoke("get_player_settings");
      alwaysOnTop = Boolean(settings?.always_on_top);
      normalizeVolume = Boolean(settings?.normalize);
      taskbarExtras = Boolean(settings?.taskbar_extras);
    } catch {
      alwaysOnTop = false;
    }
  }
  async function toggleAlwaysOnTop() {
    alwaysOnTop = !alwaysOnTop;
    await invoke("set_always_on_top", { active: alwaysOnTop }).catch(() => {});
  }

  // Windows' own popup menu, the way Winamp's was: submenus, each command's
  // key shown next to it, and never cut off by a small window. Built fresh on
  // every open so the ticks match the current state. The main window opens
  // the same menu (it asks over playerWindow: MenuRequested).
  /** @param {MouseEvent} e */
  function openMenu(e) {
    e.preventDefault();
    // Right-clicking a song selects it first (unless it's already in the
    // selection), so the song commands at the top act on what was clicked.
    const target = /** @type {Node | null} */ (e.target);
    const row = target ? playlist.rows.find((r) => r.element?.contains(target)) : undefined;
    if (row && !playlist.selectedRows.includes(row)) {
      playlist.selectedRows = [row];
      playlist.focusedRow = row;
      playlist.selectionAnchor = row;
    }
    showMenu(null);
  }
  /** @type {Menu | null} */
  let openedMenu = null;
  let menuBusy = false;
  /** @param {string | null} windowLabel where to show it (null = here) */
  async function showMenu(windowLabel) {
    if (menuBusy) return;
    menuBusy = true;
    try {
      await Promise.all([loadAudioDevices(), loadAlwaysOnTop(), loadMenuLists()]);
      const menu = await Menu.new({ items: menuItems() });
      openedMenu?.close().catch(() => {});
      openedMenu = menu;
      const target = windowLabel ? await Window.getByLabel(windowLabel) : null;
      await menu.popup(undefined, target ?? undefined);
    } catch (e) {
      invoke("log_frontend_error", { window: "playlist", message: `menu: ${e}` }).catch(() => {});
    } finally {
      menuBusy = false;
    }
  }

  // Instant mix: songs like the selected (or playing) one, from Spotify radio.
  /** @param {"append" | "queue"} how */
  async function runInstantMix(how) {
    showToast("Finding similar songs…");
    try {
      const { added, seed } = await playlist.instantMix(how);
      showToast(
        added
          ? `Added ${added} songs like ${seed || "that one"}${how === "queue" ? ", playing next" : ""}`
          : "Select a Spotify song for the mix first",
      );
    } catch {
      showToast("Couldn't get a mix right now");
    }
  }

  // The app's saved lists, for the song menu's "Add to list".
  /** @type {string[]} */
  let menuLists = [];
  async function loadMenuLists() {
    try {
      menuLists = (/** @type {{name: string}[]} */ (await invoke("get_saved_lists"))).map((l) => l.name);
    } catch {
      menuLists = [];
    }
  }

  /** @param {string[]} uris */
  async function copySpotifyLinks(uris) {
    const text = uris.map((u) => `https://open.spotify.com/track/${u.split(":").pop()}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      showToast(uris.length === 1 ? "Link copied" : `${uris.length} links copied`);
    } catch {
      showToast("Couldn't copy the link");
    }
  }

  /** @param {string} name @param {string[]} uris */
  async function addSelectionToList(name, uris) {
    for (const uri of uris) await invoke("add_to_list", { name, uri }).catch(() => {});
    showToast(`Added to "${name}"`);
  }

  /** The commands for the selected songs, shown at the top of the menu. */
  function songItems() {
    const sep = { item: /** @type {const} */ ("Separator") };
    const rows = playlist.selectedRows;
    if (!rows.length || controllerMode) return [];
    const uris = playlist.selectedSpotifyUris();
    const allLoved = uris.length > 0 && uris.every((u) => playlist.loved.has(u));
    const local = rows.length === 1 && rows[0].isLocal ? /** @type {{path: string}} */ (/** @type {any} */ (rows[0])) : null;
    /** @type {any[]} */
    const items = [
      { text: "Play", accelerator: "Enter", action: () => playlist.playSelected() },
      { text: "Play next", accelerator: "Q", action: () => playlist.toggleQueue() },
    ];
    if (uris.length) {
      items.push(
        { text: allLoved ? "Unlove" : "♡ Love", accelerator: "F", action: () => playlist.toggleLoveSelected() },
        {
          text: "Add to list",
          items: menuLists.length
            ? menuLists.map((name) => ({ text: menuText(name), action: () => addSelectionToList(name, uris) }))
            : [{ text: "No lists yet (Playlist ▸ Save as a list…)", enabled: false }],
        },
        { text: uris.length === 1 ? "Copy Spotify link" : `Copy ${uris.length} Spotify links`, action: () => copySpotifyLinks(uris) },
      );
    }
    if (local) {
      items.push({
        text: "Show in folder",
        action: () => invoke("local_reveal", { path: local.path }).catch(() => showToast("That file isn't there any more")),
      });
    }
    items.push({ text: rows.length === 1 ? "Remove from playlist" : `Remove ${rows.length} songs`, accelerator: "Del", action: () => playlist.removeSelected() }, sep);
    return items;
  }

  /** A literal "&" in a Windows menu needs doubling (a single one marks the Alt key). */
  const menuText = (/** @type {string} */ text) => text.replaceAll("&", "&&");

  function menuItems() {
    const sep = { item: /** @type {const} */ ("Separator") };
    const isWorn = (/** @type {string} */ custom) => currentSkin === "custom" && wornCustom === custom;
    /** @type {any[]} */
    const playItems = [
      { text: "Play", accelerator: "X", action: () => emitWindowEvent("playlistWindow", { PlayRequested: null }) },
      { text: "Pause", accelerator: "C", action: () => emitWindowEvent("playlistWindow", { PauseRequested: null }) },
      { text: "Stop", accelerator: "V", action: () => emitWindowEvent("playlistWindow", { StopRequested: null }) },
      { text: "Previous", accelerator: "Z", action: () => playlist.previous(true) },
      { text: "Next", accelerator: "B", action: () => playlist.next(true) },
    ];
    if (!controllerMode) {
      playItems.push({
        text: "Stop after current",
        accelerator: "Ctrl+V",
        checked: playlist.stopAfterCurrent,
        action: () => playlist.toggleStopAfterCurrent(),
      });
    }
    if (!controllerMode) {
      playItems.push(sep, {
        text: "Autoplay similar songs when the list ends",
        checked: playlist.autoplay,
        action: () => (playlist.autoplay = !playlist.autoplay),
      });
    }
    const listItems = controllerMode
      ? [{ text: "Clear playlist", action: () => playlist.clear() }]
      : [
          { text: "Add file(s)…", accelerator: "O", action: addLocalFiles },
          { text: "Add folder…", accelerator: "Shift+O", action: addLocalFolder },
          sep,
          { text: "Save as a list…", action: openSaveList },
          { text: "Clear playlist", action: () => playlist.clear() },
        ];
    const skinItems = [
      { text: "Classic", checked: currentSkin === "classic", action: () => chooseSkin("classic") },
      ...bundledSkins.map((name) => ({
        text: menuText(prettySkinName(name)),
        checked: isWorn(`bundled:${name}`),
        action: () => chooseBundledSkin(name),
      })),
      {
        text: "&Colors",
        items: ["cherry", "amber", "emerald"].map((c) => ({
          text: c[0].toUpperCase() + c.slice(1),
          checked: currentSkin === c,
          action: () => chooseSkin(c),
        })),
      },
      sep,
      { text: "Load .wsz from disk…", checked: isWorn("file"), action: loadWszSkin },
      { text: "Skin &Museum…", checked: isWorn("museum"), action: openSkinMuseum },
    ];
    const windowItems = [
      ...(controllerMode ? [] : [{ text: "Library", accelerator: "L", action: openLibraryWindow }]),
      { text: "Visualizer", action: () => invoke("set_visualizer_window_visible", { visible: true }) },
      ...(controllerMode
        ? []
        : [
            { text: "Lyrics", action: () => invoke("set_lyrics_window_visible", { visible: true }) },
            { text: "Album art", action: () => invoke("set_art_window_visible", { visible: true }) },
            { text: "Listening stats", action: () => invoke("set_stats_window_visible", { visible: true }) },
          ]),
      sep,
      { text: "Always on top", checked: alwaysOnTop, action: toggleAlwaysOnTop },
      { text: "Taskbar extras (title, progress, buttons)", checked: taskbarExtras, action: toggleTaskbarExtras },
      {
        text: "S&cale",
        items: [1, 1.5, 2, 3].map((z) => ({
          text: `${z}×`,
          checked: REACTIVE_WINDOW_SIZE.zoom === z,
          ...(z === 2 ? { accelerator: "Ctrl+D" } : {}),
          action: () => setUiScale(z),
        })),
      },
    ];
    const audioItems = [
      { text: "Normalize volume", checked: normalizeVolume, action: toggleNormalize },
      {
        text: "&Output device",
        items: [
          { text: "System default", checked: !currentAudioDevice, action: () => pickAudioDevice(null) },
          ...(audioDevices.length ? [sep] : []),
          ...audioDevices.map((d) => ({
            text: menuText(d),
            checked: currentAudioDevice === d,
            action: () => pickAudioDevice(d),
          })),
        ],
      },
    ];
    const helpItems = [
      { text: "What's new and keyboard keys", action: openWhatsNew },
      { text: "Copy diagnostic info", action: copyDiagnosticInfo },
      {
        text: updateBusy
          ? "Checking for updates…"
          : updateAvailable
            ? `Update to ${updateAvailable}…`
            : "Check for updates",
        enabled: !updateBusy,
        action: checkForUpdates,
      },
      sep,
      { text: "Join our Discord", action: openDiscord },
    ];
    return [
      ...songItems(),
      { text: "&Play", items: playItems },
      { text: "P&laylist", items: listItems },
      { text: "&Skins", items: skinItems },
      { text: "&Windows", items: windowItems },
      ...(controllerMode ? [] : [{ text: "&Audio", items: audioItems }]),
      sep,
      { text: "Jump to track…", accelerator: "J", action: () => playlist.openJump() },
      { text: "Play selected next", accelerator: "Q", action: () => playlist.toggleQueue() },
      ...(controllerMode
        ? []
        : [
            {
              text: "Instant &mix: 20 similar songs",
              items: [
                { text: "Add them to the end of the playlist", action: () => runInstantMix("append") },
                { text: "Queue them to play next", action: () => runInstantMix("queue") },
              ],
            },
          ]),
      { text: "Copy Now Playing card", action: copyNowPlayingCard },
      {
        text: "Sleep &timer",
        items: SLEEP_STEPS.map((m) => ({
          text: m ? `${m} minutes` : "Off",
          checked: sleepMinutes === m,
          action: () => setSleep(m),
        })),
      },
      sep,
      ...(controllerMode ? [{ text: "Premium sign-in…", action: switchToPremium }] : []),
      { text: "&Help", items: helpItems },
    ];
  }

  // UI scale for every window (Windows tab); Ctrl+D in the main window toggles
  // 1x / 2x. The menu closes first, it's laid out at the old scale.
  /** @param {number} s */
  function setUiScale(s) {
    invoke("set_ui_scale", { pct: Math.round(s * 100) }).catch(() => {});
  }

  // --- audio output device picker ---
  let audioDevices = $state([]);
  let currentAudioDevice = $state(null);
  async function loadAudioDevices() {
    try {
      const info = await invoke("list_audio_devices");
      audioDevices = info.devices || [];
      currentAudioDevice = info.current ?? null;
    } catch {
      audioDevices = [];
    }
  }
  async function pickAudioDevice(name) {
    currentAudioDevice = name;
    await invoke("set_audio_device", { device: name }).catch(() => {});
    // the player was rebuilt on the new device — ask the player window to
    // resume the current track there.
    await emit("audioDeviceChanged", {});
  }

  // Normalize volume: evens out loudness between tracks. The player is rebuilt
  // for it, so the current track is picked up again the same way as after a
  // device switch.
  async function toggleNormalize() {
    normalizeVolume = !normalizeVolume;
    await invoke("set_normalization", { enabled: normalizeVolume }).catch(() => {});
    await emit("audioDeviceChanged", {});
  }

  // --- save the current queue as an app-local list (browse them in the Library
  //     window's "Spotiamp+" tree node) ---
  // A native menu can't hold a text box, so "Save as a list…" asks for the
  // name in a small box over the playlist (like J's).
  let newListName = $state("");
  let saveListOpen = $state(false);
  /** @type {HTMLInputElement | undefined} */
  let saveListInput = $state();
  async function openSaveList() {
    newListName = "";
    saveListOpen = true;
    await tick();
    await getCurrentWindow().setFocus().catch(() => {});
    saveListInput?.focus();
  }
  async function saveCurrentAsList() {
    const name = newListName.trim();
    if (!name) return;
    // Saved lists are Spotify uris; skip local rows (they have no real uri).
    const uris = playlist.rows
      .filter((r) => !r.isLocal)
      .map((r) => r.uri.asString);
    await invoke("save_list", { name, uris }).catch(() => {});
    newListName = "";
    saveListOpen = false;
    showToast(`Saved "${name}": it's in the Library under Spotiamp+`);
  }

  // --- Now Playing card ---
  // One click copies a shareable image: the real player exactly as it looks
  // right now (captured from its webview), the cover, and the track.
  let toast = $state("");
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let toastTimer;
  /** @param {string} text */
  function showToast(text) {
    toast = text;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 2600);
  }
  playlist.notify = showToast;
  // Typed a letter that isn't a shortcut: people expect that to search, so
  // say where searching is. Again on the next such key once the last hint has
  // faded (a 20 s wait read as "it stopped working").
  let lastTypedHintAt = 0;
  $effect(() => {
    if (!playlist.typedHint) return;
    const now = Date.now();
    if (now - lastTypedHintAt < 2800) return;
    lastTypedHintAt = now;
    untrack(() => showToast("Press J to jump to a track"));
  });
  // An add skipped songs already in the playlist (no song is listed twice).
  $effect(() => {
    const { count, seq } = playlist.duplicateNotice;
    if (!seq) return;
    untrack(() =>
      showToast(
        count === 1
          ? "That song is already in the playlist"
          : `${count} songs were already in the playlist, skipped`,
      ),
    );
  });

  /** @param {string} src @returns {Promise<HTMLImageElement>} */
  function loadImage(src) {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.crossOrigin = "anonymous"; // the cover CDN allows it; keeps the canvas exportable
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error("image failed to load"));
      img.src = src;
    });
  }
  /**
   * Shorten `text` with an ellipsis until it fits `max` px in the current font.
   * @param {CanvasRenderingContext2D} ctx @param {string} text @param {number} max
   */
  function fitText(ctx, text, max) {
    if (ctx.measureText(text).width <= max) return text;
    let s = text;
    while (s.length > 1 && ctx.measureText(s + "…").width > max) s = s.slice(0, -1);
    return s + "…";
  }
  /** @returns {Promise<Blob>} */
  async function buildNowPlayingCard() {
    const png = /** @type {ArrayBuffer} */ (await invoke("capture_window_png", { label: "player" }));
    const player = await createImageBitmap(new Blob([png], { type: "image/png" }));
    const row = /** @type {any} */ (playlist.loadedRow);
    const track = row && !row.isLocal ? row.track : null;
    const title = track?.name ?? (row?.isLocal ? row.displayName : "");
    const subtitle = [track?.artist, track?.album].filter(Boolean).join(" · ");
    const cover = track?.albumArt ? await loadImage(track.albumArt).catch(() => null) : null;

    // The capture is in physical pixels at the current UI scale; draw the player
    // at 2x its classic size, so the card looks the same at any display or UI
    // scale and the pixel-art skin stays crisp.
    const dpr = window.devicePixelRatio || 1;
    const uiScale = REACTIVE_WINDOW_SIZE.zoom || 1;
    const pw = Math.round((player.width / (dpr * uiScale)) * 2);
    const ph = Math.round((player.height / (dpr * uiScale)) * 2);
    const PAD = 24;
    const GAP = 20;
    const COVER = 232;
    const top = Math.max(COVER, ph);
    const W = PAD + COVER + GAP + pw + PAD;
    const H = PAD + top + 18 + 64 + PAD;

    // The card takes the skin's colours: its playlist background, text and
    // current-track colours (a .wsz brings its own from PLEDIT; the built-ins
    // fall back exactly like the playlist does), so every skin gets a card that
    // matches it.
    const css = getComputedStyle(document.body);
    /** @param {string} name @param {string} fallback */
    const skinColor = (name, fallback) => css.getPropertyValue(name).trim() || fallback;
    const bg = skinColor("--skin-plbg", "#000000");
    const fg = skinColor("--skin-plnormal", "rgb(0, 255, 0)");
    const hi = skinColor("--skin-plcurrent", "#ffffff");

    const canvas = document.createElement("canvas");
    canvas.width = W;
    canvas.height = H;
    const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, W, H);
    // a faint frame in the skin's text colour
    ctx.globalAlpha = 0.35;
    ctx.strokeStyle = fg;
    ctx.strokeRect(0.5, 0.5, W - 1, H - 1);
    ctx.globalAlpha = 1;

    // cover (or a quiet placeholder for local files / no art)
    const coverY = PAD + (top - COVER) / 2;
    if (cover) {
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(cover, PAD, coverY, COVER, COVER);
    } else {
      ctx.globalAlpha = 0.12;
      ctx.fillStyle = fg;
      ctx.fillRect(PAD, coverY, COVER, COVER);
      ctx.globalAlpha = 1;
      ctx.font = "96px 'Segoe UI Symbol', sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText("♪", PAD + COVER / 2, coverY + COVER / 2);
    }

    // the player, pixel-exact
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(player, PAD + COVER + GAP, PAD + (top - ph) / 2, pw, ph);

    // the track
    const textX = PAD;
    const textW = W - PAD * 2;
    let y = PAD + top + 18;
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    ctx.fillStyle = fg;
    ctx.font = "bold 11px 'Segoe UI', sans-serif";
    ctx.fillText("NOW PLAYING", textX, y);
    y += 16;
    ctx.fillStyle = hi;
    ctx.font = "bold 22px 'Segoe UI', sans-serif";
    ctx.fillText(fitText(ctx, title || "Spotiamp+", textW), textX, y);
    y += 28;
    if (subtitle) {
      ctx.globalAlpha = 0.75;
      ctx.fillStyle = fg;
      ctx.font = "15px 'Segoe UI', sans-serif";
      ctx.fillText(fitText(ctx, subtitle, textW), textX, y);
      ctx.globalAlpha = 1;
    }

    const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) throw new Error("couldn't encode the card");
    return /** @type {Blob} */ (blob);
  }
  async function copyNowPlayingCard() {
    // The clipboard only takes writes from a focused page, and the menu may
    // have been opened over the main window: bring this one forward first.
    await getCurrentWindow().setFocus().catch(() => {});
    window.focus();
    try {
      // Hand the clipboard a promise so the write starts inside the click while
      // the card is still being put together.
      await navigator.clipboard.write([
        new ClipboardItem({ "image/png": buildNowPlayingCard() }),
      ]);
      showToast("Now Playing card copied, paste it anywhere");
    } catch (e) {
      showToast("Couldn't copy the card");
      invoke("log_frontend_error", {
        window: "playlist",
        message: `now playing card: ${e}`,
      }).catch(() => {});
    }
  }

  // Local files: pick from disk and add them straight into the playlist as
  // LocalRows (the first starts playing). Same as the player's O / Shift+O.
  async function addLocalFiles() {
    const paths = /** @type {string[]} */ (
      await invoke("local_pick_files").catch(() => [])
    );
    if (paths?.length) playlist.addLocalFiles(paths);
  }
  async function addLocalFolder() {
    const paths = /** @type {string[]} */ (
      await invoke("local_pick_folder").catch(() => [])
    );
    if (paths?.length) playlist.addLocalFiles(paths);
  }

  async function openSkinMuseum() {
    await invoke("show_museum").catch(() => {});
  }
  async function openWhatsNew() {
    await invoke("show_whats_new").catch(() => {});
  }
  async function copyDiagnosticInfo() {
    await copyDiagnostics();
  }

  async function openDiscord() {
    // The URL itself lives in Rust's allowlist — we only name the target.
    await invoke("open_external", { target: "discord" }).catch(() => {});
  }

  // --- sleep timer ---
  // Pauses playback after a while (Off, 15, 30, 45 or 60 minutes); picking a
  // length restarts the countdown. Works in both modes, since
  // PauseRequested is what the player already listens for.
  let sleepMinutes = $state(0);
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let sleepTimer;
  const SLEEP_STEPS = [0, 15, 30, 45, 60];
  /** @param {number} next minutes, 0 = off */
  function setSleep(next) {
    clearTimeout(sleepTimer);
    sleepMinutes = next;
    if (next > 0) {
      sleepTimer = setTimeout(
        () => {
          emitWindowEvent("playlistWindow", { PauseRequested: null });
          sleepMinutes = 0;
        },
        next * 60 * 1000,
      );
    }
  }

  // Controller mode → Premium: forget the mode flag and relaunch into the
  // normal OAuth + librespot path.
  async function switchToPremium() {
    await invoke("leave_controller_mode").catch(() => {});
    await relaunch().catch(() => {});
  }
  async function chooseSkin(skin) {
    currentSkin = skin;
    setWorn("");
    await invoke("set_skin", { skin });
    emitWindowEvent("skinChanged", { skin });
  }
  // load a classic Winamp 2.x skin (.wsz) from disk
  async function loadWszSkin() {
    try {
      const name = await invoke("pick_and_load_skin");
      if (name === null) return; // cancelled
      currentSkin = "custom";
      setWorn("file");
      emitWindowEvent("skinChanged", { skin: "custom" });
    } catch (e) {
      handleError(new Error(`Could not load skin: ${e}`));
    }
  }

  // .wsz skins shipped with the app, selectable straight from the menu
  let bundledSkins = $state([]);
  invoke("list_bundled_skins")
    .then((names) => (bundledSkins = names))
    .catch(() => {});
  const prettySkinName = (name) => name.replaceAll("_", " ");
  async function chooseBundledSkin(name) {
    try {
      await invoke("load_bundled_skin", { name });
      currentSkin = "custom";
      setWorn(`bundled:${name}`);
      emitWindowEvent("skinChanged", { skin: "custom" });
    } catch (e) {
      handleError(new Error(`Could not load skin: ${e}`));
    }
  }

  async function openLibrary() {
    showLibrary = true;
    if (libraryPlaylists.length > 0) return;
    libraryLoading = true;
    libraryError = "";
    try {
      libraryPlaylists = await invoke("get_user_playlists");
    } catch (e) {
      libraryError = String(e);
    } finally {
      libraryLoading = false;
    }
  }

  async function loadPlaylist(uri) {
    showLibrary = false;
    await playlist.clear();
    // uri is "spotify:playlist:ID" but addUrls expects an open.spotify.com URL
    const id = uri.split(":").pop();
    await playlist.addUrls([`https://open.spotify.com/playlist/${id}`], false);
  }


  /**
   * @param {DocumentEventMap["keydown"]} e
   */
  function preventKeyboardScrolling(e) {
    if (
      ["Space", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].indexOf(
        e.code,
      ) != -1
    ) {
      e.preventDefault();
    }
  }

  onMount(() => {
    const cleanupDropHandler = handleDrop(async (urls) => {
      await playlist.addUrls(urls);
    });

    emitWindowEvent("playlistWindow", { Ready: null });

    // Quietly check for a new version on launch so the pill can flag it.
    checkForUpdatesSilently();

    // Right-click on the main window: the same menu, shown there.
    /** @type {(() => void) | undefined} */
    let unsubMenu;
    subscribeToWindowEvent("playerWindow", (e) => {
      if (e.MenuRequested !== undefined) showMenu("player");
    }).then((u) => (unsubMenu = u));
    // A skin put on from the Skin Museum window: tick it in the menu.
    /** @type {(() => void) | undefined} */
    let unsubSkin;
    subscribeToWindowEvent("skinChanged", (e) => {
      if (e.skin === "custom" && e.from === "museum") {
        currentSkin = "custom";
        setWorn("museum");
      }
    }).then((u) => (unsubSkin = u));

    // Cleanups
    return () => {
      cleanupDropHandler();
      playlist.dispose();
      unsubMenu?.();
      unsubSkin?.();
      openedMenu?.close().catch(() => {});
    };
  });

  /**
   * @param {HTMLElement} element
   */
  function makeResizable(element) {
    element.onpointerdown = function (event) {
      document.onmousemove = function (event) {
        const pointerX = Math.max(
          Math.ceil(event.clientX / REACTIVE_WINDOW_SIZE.zoom / 25),
          11,
        );
        const pointerY = Math.max(
          Math.ceil(event.clientY / REACTIVE_WINDOW_SIZE.zoom / 29),
          4,
        );

        REACTIVE_WINDOW_SIZE.setSize(pointerX * 25, pointerY * 29);
        invoke("set_playlist_inner_size", {
          width: REACTIVE_WINDOW_SIZE.width,
          height: REACTIVE_WINDOW_SIZE.height,
        });
      };

      document.onmouseup = function () {
        document.onmousemove = null;

        element.releasePointerCapture(event.pointerId);
      };

      element.setPointerCapture(event.pointerId);
    };

    element.onselectstart = () => false;
  }

  /**
   * @param {HTMLElement} element
   */
  function makeWindowDraggable(element) {
    makeDockedDraggable(element, "playlist", "playlistWindow");
  }
  let scroll = $state(0);
  const PLAYLIST_ROW_HEIGHT = 14.5;
  /**
   * @type {HTMLElement | undefined}
   */
  let scrollElement = $state();
  let wheelDelta = 0;

  function scrollMax() {
    return scrollElement
      ? scrollElement.scrollHeight - scrollElement.clientHeight
      : 0;
  }

  function scrollRowHeight() {
    // The UI scale is a CSS zoom on the whole page, while the rows keep their
    // natural 14.5px (--zoom is 1) and scrollTop is measured in the page's own
    // unzoomed CSS pixels, so a row is 14.5 at every scale. (Not offsetHeight:
    // it rounds the fractional height and would drift over long lists.)
    return PLAYLIST_ROW_HEIGHT;
  }

  function syncScrollThumb() {
    const max = scrollMax();
    if (scrollElement && max > 0) {
      const value = Math.min(Math.max(0, scrollElement.scrollTop), max);
      scroll = (value / max) * 100;
    } else {
      scroll = 0;
    }
  }

  /**
   * @param {number} row
   */
  function scrollToRow(row) {
    if (!scrollElement) {
      return;
    }

    scrollElement.scrollTop = Math.min(
      Math.max(0, row * scrollRowHeight()),
      scrollMax(),
    );
    syncScrollThumb();
  }

  /**
   * @param {number} offset
   */
  function scrollByRows(offset) {
    if (!scrollElement) {
      return;
    }

    scrollToRow(
      Math.round(scrollElement.scrollTop / scrollRowHeight()) + offset,
    );
  }

  /**
   * @param {WheelEvent} event
   */
  function onWheelScroll(event) {
    let delta = event.deltaY || event.deltaX;
    if (delta === 0) {
      return;
    }

    event.preventDefault();
    if (event.deltaMode == WheelEvent.DOM_DELTA_LINE) {
      delta *= scrollRowHeight();
    } else if (event.deltaMode == WheelEvent.DOM_DELTA_PAGE && scrollElement) {
      delta *= scrollElement.clientHeight;
    }

    wheelDelta += delta;
    const rows =
      wheelDelta > 0
        ? Math.floor(wheelDelta / scrollRowHeight())
        : Math.ceil(wheelDelta / scrollRowHeight());
    if (rows === 0) {
      return;
    }

    scrollByRows(rows);
    wheelDelta -= rows * scrollRowHeight();
  }

  /**
   * @param {Event} event
   */
  function onManualScroll(event) {
    if (scrollElement && event.target instanceof HTMLInputElement) {
      const targetTop = (parseInt(event.target.value, 10) / 100) * scrollMax();
      scrollToRow(Math.round(targetTop / scrollRowHeight()));
    }
  }

  // ------ Drag to reorder ------
  // Winamp-style: the selection shifts by however many rows the pointer has
  // travelled from where the drag started, regardless of which row it's over.
  const EDGE_SCROLL_ZONE = 12;
  const EDGE_SCROLL_INTERVAL_MS = 80;

  /**
   * A playlist row: a Spotify track or a local file.
   * @typedef {import('$lib/playlist.svelte').TrackRow | import('$lib/playlist.svelte').LocalRow} Row
   */

  /**
   * @type {{
   *   row: Row,
   *   startY: number,
   *   rowHeight: number,
   *   block: Row[],
   *   remaining: Row[],
   *   baseInsert: number,
   *   originalRows: Row[],
   *   appliedOffset: number,
   *   moved: boolean,
   *   pointerY: number,
   * } | undefined}
   */
  let drag;
  let isDragging = $state(false);
  /** @type {number | undefined} */
  let edgeScrollFrame;
  let lastEdgeScrollAt = 0;

  /**
   * Shift the dragged selection by `offset` rows relative to its start.
   *
   * @param {number} offset
   */
  function applyDragOffset(offset) {
    if (!drag || offset === drag.appliedOffset) {
      return;
    }
    drag.appliedOffset = offset;

    if (offset === 0) {
      // Back at the start: restore the original order verbatim (this also
      // preserves any gaps in a non-contiguous selection).
      playlist.rows = [...drag.originalRows];
    } else {
      drag.moved = true;
      isDragging = true;
      playlist.placeSelection(
        drag.block,
        drag.remaining,
        drag.baseInsert + offset,
      );
    }
  }

  /**
   * Recompute the offset from the current pointer position and apply it.
   */
  function updateDragFromPointer() {
    if (!drag) {
      return;
    }
    const offset = Math.round((drag.pointerY - drag.startY) / drag.rowHeight);
    applyDragOffset(offset);
  }

  /**
   * Continuously scroll while the pointer rests near the top/bottom edge,
   * keeping the offset consistent by shifting the drag origin as we scroll.
   */
  /**
   * @param {number} now
   */
  function edgeScrollTick(now) {
    edgeScrollFrame = undefined;
    if (!drag || !scrollElement) {
      return;
    }

    const rect = scrollElement.getBoundingClientRect();
    let delta = 0;
    if (drag.pointerY < rect.top + EDGE_SCROLL_ZONE) {
      delta = -1;
    } else if (drag.pointerY > rect.bottom - EDGE_SCROLL_ZONE) {
      delta = 1;
    }

    if (delta !== 0 && now - lastEdgeScrollAt >= EDGE_SCROLL_INTERVAL_MS) {
      const before = scrollElement.scrollTop;
      scrollByRows(delta);
      // Move the drag origin by however much we actually scrolled so the
      // pointer-to-row mapping keeps growing while held at the edge.
      drag.startY -= scrollElement.scrollTop - before;
      updateDragFromPointer();
      lastEdgeScrollAt = now;
    }

    if (delta !== 0) {
      edgeScrollFrame = requestAnimationFrame(edgeScrollTick);
    }
  }

  /**
   * @param {MouseEvent} e
   * @param {Row} row
   */
  function onRowMouseDown(e, row) {
    if (e.button !== 0) {
      return;
    }

    const ctrl = e.ctrlKey || e.metaKey;
    const shift = e.shiftKey;
    if (ctrl || shift) {
      playlist.select(row, { ctrl, shift });
      return;
    }

    // Keep an existing multi-selection intact so it can be dragged as a group;
    // a plain click that doesn't turn into a drag collapses to this row on release.
    if (!playlist.selectedRows.includes(row)) {
      playlist.select(row);
    }

    const selected = new Set(playlist.selectedRows);
    const block = playlist.rows.filter((r) => selected.has(r));
    const remaining = playlist.rows.filter((r) => !selected.has(r));
    const topIndex = Math.min(...block.map((r) => playlist.rows.indexOf(r)));
    // Where the block sits among the non-dragged rows at the start.
    const baseInsert = remaining.filter(
      (r) => playlist.rows.indexOf(r) < topIndex,
    ).length;
    const dragElement =
      row.element ??
      (e.currentTarget instanceof HTMLElement ? e.currentTarget : undefined);
    const rowHeight = dragElement?.getBoundingClientRect().height || 1;

    drag = {
      row,
      startY: e.clientY,
      rowHeight,
      block,
      remaining,
      baseInsert,
      originalRows: [...playlist.rows],
      appliedOffset: 0,
      moved: false,
      pointerY: e.clientY,
    };
    lastEdgeScrollAt = 0;
    window.addEventListener("mousemove", onDragMove);
    window.addEventListener("mouseup", onDragEnd);
  }

  /**
   * @param {MouseEvent} e
   */
  function onDragMove(e) {
    if (!drag) {
      return;
    }
    drag.pointerY = e.clientY;
    updateDragFromPointer();

    if (edgeScrollFrame === undefined) {
      edgeScrollFrame = requestAnimationFrame(edgeScrollTick);
    }
  }

  function onDragEnd() {
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragEnd);
    if (edgeScrollFrame !== undefined) {
      cancelAnimationFrame(edgeScrollFrame);
      edgeScrollFrame = undefined;
    }

    if (drag && !drag.moved) {
      // A plain click (no drag): collapse the selection to the clicked row.
      playlist.select(drag.row);
    } else if (drag) {
      // The order changed — persist the new arrangement.
      playlist.persist();
    }
    drag = undefined;
    isDragging = false;
    lastEdgeScrollAt = 0;
  }
</script>

<span
  style:--playlist-w={playlist.width}
  style:--playlist-h={playlist.height}
  style:--track-row-height={`${PLAYLIST_ROW_HEIGHT}px`}
  oncontextmenu={openMenu}
>
  <!-- our "my playlists" browser (opens a list of the user's Spotify playlists) -->
  <button class="my-playlists-btn" onclick={openLibraryWindow}>♪ library</button>
  {#if updateAvailable}
    <button
      class="update-pill"
      onclick={checkForUpdates}
      title="Spotiamp+ {updateAvailable} is available — click to update"
    >
      ⬆ v{updateAvailable}
    </button>
  {/if}
  {#if showLibrary}
    <div class="library-overlay">
      <div class="library-head">
        <span>MY PLAYLISTS</span>
        <button class="library-close" onclick={() => (showLibrary = false)}>×</button>
      </div>
      {#if libraryPlaylists.length > 0}
        <input
          class="library-search"
          type="text"
          placeholder="search playlists…"
          bind:value={librarySearch}
        />
      {/if}
      <div class="library-list">
        {#if libraryLoading}
          <div class="library-msg">
            loading your playlists…<br />
            (first open fetches each playlist's name — can take a few seconds;
            it's instant after that)
          </div>
        {:else if libraryError}
          <div class="library-msg err">
            in-app browsing isn't wired up yet — Spotify blocks the permission
            this needs (work in progress). for now: drag a playlist straight from
            the Spotify app onto this window and it loads.
          </div>
        {:else if libraryPlaylists.length === 0}
          <div class="library-msg">no playlists found</div>
        {:else if filteredPlaylists.length === 0}
          <div class="library-msg">no matches for "{librarySearch}"</div>
        {:else}
          {#each filteredPlaylists as pl}
            <button class="library-item" onclick={() => loadPlaylist(pl.uri)}>
              <span class="library-name">{pl.name}</span>
              <span class="library-count">{pl.track_count}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
  <div
    class="tracks-container"
    onkeydown={preventKeyboardScrolling}
    onwheel={onWheelScroll}
    role="scrollbar"
    tabindex="0"
    aria-controls="playlist-tracks"
    aria-valuenow={scroll}
    onscroll={syncScrollThumb}
    bind:this={scrollElement}
  >
    <table id="playlist-tracks" class:dragging={isDragging}>
      <tbody>
        {#each playlist.rows as row, index}
          <tr
            class="playlist-track"
            class:loaded={row.isLoaded()}
            class:selected={row.isSelected()}
            class:unavailable={row.unavailable}
            onmousedown={(e) => onRowMouseDown(e, row)}
            ondblclick={() => row.play()}
            use:enterExitViewport
            bind:this={row.element}
            onenterViewport={row.getOnEnterViewport()}
          >
            <td class="playlist-track-main">
              <span class="playlist-track-number">{index + 1}.&nbsp;</span>
              {#if playlist.queuePosition(row)}
                <span class="playlist-track-queue">[{playlist.queuePosition(row)}]&nbsp;</span>
              {/if}
              <span class="playlist-track-name">{row.displayName}</span>
            </td>
            <td class="playlist-track-duration">{row.displayDuration}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <input
      class="sprite scroll-bar"
      type="range"
      bind:value={scroll}
      oninput={onManualScroll}
    />
  </div>

  {#if playlist.jumpOpen}
    <!-- J: jump to file -->
    <div class="jump-backdrop" role="presentation" onmousedown={closeJump}></div>
    <div class="jump-box" role="dialog" aria-label="Jump to file">
      <input
        class="jump-input"
        bind:this={jumpInput}
        bind:value={jumpQuery}
        onkeydown={onJumpKey}
        placeholder="jump to file…"
        spellcheck="false"
        aria-label="Jump to file"
      />
      <div class="jump-list" role="listbox" bind:this={jumpListEl}>
        {#each jumpMatches as hit, i (hit.row)}
          <div
            class="jump-item"
            class:active={i === jumpActive}
            role="option"
            aria-selected={i === jumpActive}
            tabindex="-1"
            onmousedown={(e) => {
              // keep focus in the box so the arrows/Enter keep working
              e.preventDefault();
              jumpPlay(hit.row);
            }}
          >
            {hit.n}. {hit.row.displayName}
          </div>
        {:else}
          <div class="jump-empty">{jumpLoadingNames ? "loading names…" : "no match"}</div>
        {/each}
      </div>
    </div>
  {/if}

  {#if toast}
    <div class="np-toast" role="status">{toast}</div>
  {/if}

  <!-- Top corners -->
  <div class="sprite playlist-sprite playlist-tl-sprite"></div>

  <div
    class="sprite playlist-sprite playlist-tr-sprite"
    style:--x={playlist.width}
  ></div>

  <!-- Left/Right -->
  {#each range(1, playlist.height - 2) as y}
    <div class="sprite playlist-sprite playlist-l-sprite" style:--y={y}></div>
    <div
      class="sprite playlist-sprite playlist-r-sprite"
      style:--y={y}
      style:--x={playlist.width}
    ></div>
  {/each}

  <!-- Top/Bottom -->
  {#each range(1, playlist.width - 2) as x}
    <div
      class="sprite playlist-sprite playlist-t-sprite"
      style:--x={x}
      use:makeWindowDraggable
    ></div>
    {#if x >= 5 && x < playlist.width - 6}
      <div
        class="sprite playlist-sprite playlist-b-sprite"
        style:--y={playlist.height - 1}
        style:--x={x}
      ></div>
    {/if}
  {/each}

  <!-- Title -->
  <div
    class="sprite playlist-sprite playlist-title-sprite"
    style:--x={playlist.width / 2 - 2}
    use:makeWindowDraggable
  ></div>

  <!-- Bottom corners -->
  <div
    class="sprite playlist-sprite playlist-bl-sprite"
    style:--y={playlist.height}
  ></div>

  <div
    class="sprite playlist-sprite playlist-br-sprite"
    style:--y={playlist.height - 1}
    style:--x={playlist.width - 9}
  ></div>

  <!-- transparent click overlays over the baked-in ADD/REM/SEL/MISC buttons -->
  <button
    class="pl-btn"
    style:--pl-btn-x="11px"
    onclick={openLibraryWindow}
    aria-label="Add — open library"
    title="open library"
  ></button>
  <button
    class="pl-btn"
    style:--pl-btn-x="40px"
    onclick={() => playlist.removeSelected()}
    aria-label="Remove selected"
    title="remove selected"
  ></button>
  <button
    class="pl-btn"
    style:--pl-btn-x="69px"
    onclick={() => (playlist.selectedRows = [...playlist.rows])}
    aria-label="Select all"
    title="select all"
  ></button>
  <button
    class="pl-btn"
    style:--pl-btn-x="98px"
    onclick={() => playlist.clear()}
    aria-label="Clear playlist"
    title="clear playlist"
  ></button>

  <!-- bottom-right LCD readouts over the two black areas:
       total playlist time (wide, upper) + current track elapsed (small row) -->
  <div class="pl-time pl-time-total">{fmtTime(playlist.totalDurationMs)}</div>
  <div class="pl-time pl-time-elapsed">{fmtTime(playlist.positionMs)}</div>

  <div class="draggable-corner" use:makeResizable></div>

  {#if saveListOpen}
    <!-- "Save as a list…": name the list -->
    <div class="jump-backdrop" role="presentation" onmousedown={() => (saveListOpen = false)}></div>
    <div class="jump-box save-box" role="dialog" aria-label="Save as a list">
      <input
        class="jump-input"
        bind:this={saveListInput}
        bind:value={newListName}
        onkeydown={(e) => {
          if (e.key === "Enter") saveCurrentAsList();
          else if (e.key === "Escape") saveListOpen = false;
        }}
        placeholder="name for this list…"
        spellcheck="false"
        aria-label="List name"
      />
      <div class="jump-empty">Enter saves it, Esc cancels. Lists live in the Library under Spotiamp+.</div>
    </div>
  {/if}
</span>

<style>
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }

  .draggable-corner {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --width: 15px;
    --height: 15px;
    width: calc(var(--width) * var(--zoom));
    height: calc(var(--height) * var(--zoom));
    background-color: transparent;
    position: absolute;
    --x: var(--playlist-w);
    --y: var(--playlist-h);
    left: calc(((var(--x)) * 25px - var(--width)) * var(--zoom));
    top: calc(((var(--y)) * 29px - var(--height)) * var(--zoom));
    display: inline-block;
  }
  /* ------ TRACKS ------ */
  .tracks-container {
    /* pushed down 14px to make room for the "my playlists" button strip */
    margin-top: calc(34px * var(--zoom));
    margin-left: calc(10px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 29px) * var(--zoom));
    height: calc(
      (var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom) -
        14px * var(--zoom)
    );
    overflow-x: hidden;
    overflow-y: scroll;
  }

  /* Hide scrollbar for Chrome, Safari and Opera */
  .tracks-container::-webkit-scrollbar {
    display: none;
  }

  /* Hide scrollbar for IE, Edge and Firefox */
  .tracks-container {
    -ms-overflow-style: none; /* IE and Edge */
    scrollbar-width: none; /* Firefox */
  }

  input.scroll-bar {
    cursor: url(/src/static/assets/skins/base-2.91/EQSLID.CUR), default;
    writing-mode: vertical-lr;
    direction: ltr;
    appearance: none;
    --x: var(--playlist-w);
    --y: var(--playlist-h);
    --width: 10px;

    left: calc(((var(--x)) * 25px - var(--width)) * var(--zoom) - 5px);
    top: 20px;

    height: calc(
      (var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom)
    );
    vertical-align: bottom;
    position: absolute;
    z-index: 1000;
  }

  input.scroll-bar::-webkit-slider-thumb {
    background: var(--skin-pledit);
    appearance: none;
    width: 8px;
    height: 18px;
    margin-bottom: 1px;
    background-position: -52px -53px;
  }

  input.scroll-bar::-webkit-slider-thumb:active {
    background-position-x: -61px;
  }

  #playlist-tracks {
    color: var(--skin-plnormal, rgb(0, 255, 0));
    border-collapse: collapse;
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(7px * var(--zoom));
    font-smooth: never;
    -webkit-font-smoothing: none;

    letter-spacing: calc(0.3px * var(--zoom));
    -webkit-user-select: none;
    -ms-user-select: none;
    user-select: none;
    width: 100%;
  }

  .playlist-track {
    outline: none;
    height: calc(var(--track-row-height) * var(--zoom));
  }

  #playlist-tracks.dragging .playlist-track {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), grabbing;
  }

  .playlist-track-main {
    /* The max-width:0 + width:100% combo lets the cell take the remaining
       space while still honouring text-overflow within a table layout. */
    max-width: 0;
    width: 100%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .playlist-track-number {
    padding-left: calc(3px * var(--zoom));
  }

  /* Q: queue position, in the "current track" colour so it stands out */
  .playlist-track-queue {
    color: var(--skin-plcurrent, #fff);
  }

  /* ------ J: jump to file ------ */
  /* Fixed over the track area, sized with the same maths as .tracks-container
     (the custom properties inherit through the DOM even for fixed boxes). */
  .jump-backdrop {
    position: fixed;
    inset: 0;
    z-index: 2000;
  }
  .jump-box {
    position: fixed;
    z-index: 2001;
    top: calc(34px * var(--zoom));
    left: calc(10px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 29px) * var(--zoom));
    height: calc(
      (var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom) -
        14px * var(--zoom)
    );
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    background: var(--skin-plbg, #000);
    border: 1px solid var(--skin-plnormal, rgb(0, 255, 0));
    color: var(--skin-plnormal, rgb(0, 255, 0));
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(7px * var(--zoom));
    -webkit-font-smoothing: none;
    letter-spacing: calc(0.3px * var(--zoom));
  }
  .jump-input {
    flex: 0 0 auto;
    margin: 2px;
    padding: 1px 3px;
    background: #000;
    color: var(--skin-plcurrent, #fff);
    border: 1px solid var(--skin-plnormal, rgb(0, 255, 0));
    font: inherit;
    outline: none;
  }
  .jump-input::placeholder {
    color: var(--skin-plnormal, rgb(0, 255, 0));
    opacity: 0.5;
  }
  .jump-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .jump-list::-webkit-scrollbar {
    display: none;
  }
  .jump-item {
    padding: 0 3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .jump-item.active {
    background: var(--skin-plselbg, #0000c6);
    color: var(--skin-plcurrent, #fff);
  }
  .save-box {
    height: auto;
  }
  .jump-empty {
    padding: 2px 3px;
    opacity: 0.6;
  }

  /* short confirmation for one-click actions (Now Playing card) */
  .np-toast {
    position: fixed;
    z-index: 3000;
    left: 50%;
    bottom: calc(40px * var(--zoom));
    transform: translateX(-50%);
    max-width: 90%;
    padding: 3px 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    background: rgba(0, 0, 0, 0.85);
    border: 1px solid var(--skin-plnormal, rgb(0, 255, 0));
    color: var(--skin-plcurrent, #fff);
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(7px * var(--zoom));
    -webkit-font-smoothing: none;
    pointer-events: none;
  }

  .playlist-track-duration {
    padding-right: calc(5px * var(--zoom));
    text-align: right;
    white-space: nowrap;
  }

  .playlist-track.selected {
    background-color: var(--skin-plselbg, rgb(0, 0, 198));
  }

  .playlist-track.loaded {
    color: var(--skin-plcurrent, white);
  }

  .playlist-track.unavailable {
    color: rgb(80, 80, 80);
  }

  .playlist-track.unavailable.loaded {
    color: rgb(140, 140, 140);
  }

  /* ------ /TRACKS ------ */

  /* ------ PLAYLIST ------ */
  .playlist-sprite {
    --x: 0;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    --sprite-y: calc(var(--y) * 20px);
  }

  .playlist-tl-sprite {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
  }

  .playlist-t-sprite {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    background-position: -127px 0px;
  }

  .playlist-title-sprite {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --sprite-url: var(--skin-pledit);
    width: 100px;
    height: 20px;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    background-position: -26px 0px;
  }

  .playlist-tr-sprite {
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
    --x: var(--playlist-w);
    --y: 0;
    --sprite-x: calc((var(--x) - 1) * 25px);
    background-position: -153px 0px;
  }

  .playlist-l-sprite {
    --sprite-url: var(--skin-pledit);
    width: 10px;
    height: 29px;
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: 0px -42px;
  }

  .playlist-r-sprite {
    --sprite-url: var(--skin-pledit);
    width: 19px;
    height: 29px;
    --x: var(--playlist-w);
    --sprite-x: calc((var(--x) - 1) * 25px + 6px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: -32px -42px;
  }

  .playlist-bl-sprite {
    --sprite-url: var(--skin-pledit);
    width: 125px;
    height: 38px;
    --y: var(--playlist-h);
    --sprite-y: calc((var(--y) - 1) * 29px - 9px);
    background-position: 0px -72px;
  }

  .playlist-b-sprite {
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 38px;
    --y: var(--playlist-h);
    --sprite-x: calc(var(--x) * 25px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: -179px 0px;
  }

  .playlist-br-sprite {
    --sprite-url: var(--skin-pledit);
    width: 150px;
    height: 38px;
    --x: var(--playlist-w);
    --y: var(--playlist-h);
    --sprite-x: calc(var(--x) * 25px + 75px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: 154px -72px;
  }
  /* ------ /PLAYLIST ------ */

  /* transparent click targets over the baked-in ADD/REM/SEL/MISC sprites */
  .pl-btn {
    position: absolute;
    left: calc(var(--pl-btn-x) * var(--zoom));
    top: calc(((var(--playlist-h) - 1) * 29px - 5px) * var(--zoom));
    width: calc(22px * var(--zoom));
    height: calc(18px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 60;
  }

  /* bottom-right LCD time readouts (green seven-seg-ish) */
  .pl-time {
    position: absolute;
    text-align: right;
    font-family: monospace;
    font-size: calc(7px * var(--zoom));
    line-height: 1;
    color: #14e614;
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
    z-index: 55;
  }
  /* positioned from the window's bottom-right corner (plain px = easy to tweak
     in devtools; stays put on resize). Adjust right / bottom / width. */
  .pl-time-elapsed {
    right: calc(58px * var(--zoom));
    width: calc(26px * var(--zoom));
    bottom: calc(7px * var(--zoom));
  }
  .pl-time-total {
    right: calc(120px * var(--zoom));
    width: calc(76px * var(--zoom));
    bottom: calc(21px * var(--zoom));
  }


  /* ------ MY PLAYLISTS browser (our addition) ------ */
  .my-playlists-btn {
    position: absolute;
    top: calc(21px * var(--zoom));
    left: calc(11px * var(--zoom));
    z-index: 40;
    padding: 0 6px;
    font-family: monospace;
    font-size: 9px;
    line-height: 12px;
    /* follows the active skin (PLEDIT text colour), green on the base skin */
    color: var(--skin-plnormal, #00ff41);
    background: linear-gradient(
      color-mix(in srgb, var(--skin-plbg, #12151c) 55%, #6a6a6a),
      var(--skin-plbg, #12151c)
    );
    border: 1px solid #000;
    box-shadow: inset 1px 1px 0 rgba(255, 255, 255, 0.15);
    cursor: pointer;
  }
  .my-playlists-btn:active {
    box-shadow: inset -1px -1px 0 rgba(255, 255, 255, 0.15);
  }
  /* "update available" pill — only rendered when a launch check found a newer
     version. Sits next to the library button, softly pulsing so it's noticed
     without nagging; clicking it runs the normal download/install flow. */
  .update-pill {
    position: absolute;
    top: calc(21px * var(--zoom));
    left: calc(74px * var(--zoom));
    z-index: 41;
    padding: 0 6px;
    font-family: monospace;
    font-size: 9px;
    line-height: 12px;
    color: #ffe08a;
    background: linear-gradient(#5a4a1a, #2e2510);
    border: 1px solid #000;
    box-shadow: inset 1px 1px 0 rgba(255, 255, 255, 0.18);
    cursor: pointer;
    animation: updatePulse 2.4s ease-in-out infinite;
  }
  .update-pill:hover {
    color: #fff6d8;
  }
  @keyframes updatePulse {
    0%,
    100% {
      box-shadow: inset 1px 1px 0 rgba(255, 255, 255, 0.18);
    }
    50% {
      box-shadow:
        inset 1px 1px 0 rgba(255, 255, 255, 0.18),
        0 0 6px -1px rgba(255, 216, 120, 0.75);
    }
  }
  .library-overlay {
    position: absolute;
    inset: 20px 12px 30px 12px;
    z-index: 50;
    background: #0a0d12;
    border: 1px solid #00ff41;
    box-shadow: 0 0 12px -2px rgba(0, 255, 65, 0.5);
    display: flex;
    flex-direction: column;
    font-family: monospace;
  }
  .library-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 3px 8px;
    color: #050805;
    background: #00cc22;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }
  .library-close {
    background: none;
    border: none;
    color: #050805;
    font-size: 13px;
    cursor: pointer;
    line-height: 1;
  }
  .library-search {
    margin: 4px 6px;
    padding: 2px 6px;
    background: #05170a;
    border: 1px solid #1e6b32;
    color: #00ff41;
    font-family: monospace;
    font-size: 11px;
    outline: none;
  }
  .library-search::placeholder {
    color: #3f7a4e;
  }
  .library-list {
    flex: 1;
    overflow-y: auto;
  }
  .library-item {
    display: flex;
    justify-content: space-between;
    width: 100%;
    padding: 3px 10px;
    background: none;
    border: none;
    color: #00ff41;
    font-family: monospace;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .library-item:hover {
    background: #163a1e;
  }
  .library-count {
    color: #5c9e6b;
    padding-left: 10px;
  }
  .library-msg {
    padding: 12px 10px;
    color: #8fbf9f;
    font-size: 12px;
  }
  .library-msg.err {
    color: #ff6b6b;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
