<script>
  import {
    enterExitViewport,
    range,
    handleDrop,
    fromFileUrl,
    handleError,
    copyDiagnostics,
    REACTIVE_WINDOW_SIZE,
    SKIN_APPLIED,
  } from "$lib/common.svelte.js";
  import { paintPe, rowName, winampPeAvailable } from "$lib/modern/skin/winampPe";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { Menu } from "@tauri-apps/api/menu";
  import { emit } from "@tauri-apps/api/event";
  import { onMount, tick, untrack } from "svelte";
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { Playlist } from "$lib/playlist.svelte";
  import { durationToString } from "$lib/spotify.svelte.js";
  import TextTicker from "../../TextTicker.svelte";
  import MiniTime from "../../MiniTime.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { watchWindowActive } from "$lib/window-active.js";

  // lit while this window is in front, dim otherwise (lib/window-active.js)
  let windowActive = $state(true);
  onMount(() => watchWindowActive((active) => (windowActive = active)));

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
  // the settings as the window opened (they're page data; this doesn't change in a run)
  const controllerMode = untrack(() => playlistSettings.controller_mode === true);

  // Free Mode: the Spotify app's queue (webapi.rs), shown in place of the
  // list while the app is what plays, once switched on in the Playlist menu.
  // Asked again when its song changes, and every half minute in case the
  // queue was changed in the app.
  let freeQueueOn = $state(untrack(() => playlistSettings.free_queue === true));
  /** @typedef {{ state: string, songs: { title: string, artist: string, duration_ms: number }[] }} FreeQueue */
  let freeQueue = $state(/** @type {FreeQueue} */ ({ state: "", songs: [] }));
  /** the Spotify app's song (null while it isn't what plays) */
  let appSong = $state(/** @type {string | null} */ (null));
  const queueShown = $derived(controllerMode && freeQueueOn && appSong !== null);
  /** @returns {Promise<FreeQueue>} */
  const askFreeQueue = () => invoke("free_queue").then((q) => /** @type {FreeQueue} */ (q), () => ({ state: "error", songs: [] }));
  async function refreshFreeQueue() {
    if (controllerMode && freeQueueOn) freeQueue = await askFreeQueue();
  }
  onMount(() => {
    if (!controllerMode) return;
    const songSubscription = subscribeToWindowEvent("art", (e) => {
      const song = e.appSong ? `${e.appSong.artist} - ${e.appSong.title}` : null;
      if (song === appSong) return;
      appSong = song;
      if (song) refreshFreeQueue();
    });
    const timer = setInterval(() => {
      if (queueShown && !document.hidden) refreshFreeQueue();
    }, 30_000);
    return () => {
      songSubscription.then((unlisten) => unlisten());
      clearInterval(timer);
    };
  });
  async function toggleFreeQueue() {
    if (freeQueueOn) {
      // off: the sign-in goes too
      freeQueueOn = false;
      invoke("free_queue_sign_out").catch(() => {});
      invoke("set_free_queue", { on: false }).catch(() => {});
      return;
    }
    let answer = await askFreeQueue();
    if (answer.state === "signin") {
      const sure = await ask(
        "To show what's coming up in the Spotify app, Spotiamp+ signs in to your Spotify account once, " +
          "on Spotify's own page. It only reads the queue.",
        { title: "Spotiamp+", kind: "info", okLabel: "Sign in" },
      );
      if (!sure) return;
      const failed = await invoke("free_queue_sign_in").then(() => false, () => true);
      if (failed) return showToast("The Spotify sign-in didn't finish");
      answer = await askFreeQueue();
    }
    freeQueue = answer;
    freeQueueOn = true;
    invoke("set_free_queue", { on: true }).catch(() => {});
  }
  const freeQueueNote = $derived(
    {
      signin: "Sign in again for the queue: Playlist menu, Show the Spotify app's queue",
      refused: "Spotify doesn't share this account's queue",
      error: "Spotify isn't answering; trying again soon",
    }[freeQueue.state] ?? (freeQueue.state ? "Nothing coming up in the Spotify app" : "Asking Spotify for the queue…"),
  );

  function createInitialPlaylist() {
    // Free Mode: its own list of local files and stations
    return new Playlist(controllerMode ? (playlistSettings.free_uris ?? []) : playlistSettings.uris, !controllerMode);
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
      // J from the main window while the playlist is hidden: nothing to show
      // (a modern skin worn: it shows in the skin's window)
      win
        .isVisible()
        .then((visible) => {
          if (!visible) {
            playlist.jumpOpen = false;
            if (modernOn) emitWindowEvent("modernDialog", { kind: "jump" });
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
  let libraryPlaylists = $state(/** @type {{ name: string, uri: string, track_count: number, image: string | null }[]} */ ([]));
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
  /** @param {number} ms */
  function fmtTime(ms) {
    const s = Math.floor((ms || 0) / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }
  // Winamp's running time: m:ss, or h:mm:ss from an hour on
  /** @param {number} ms */
  function fmtRunning(ms) {
    const s = Math.floor((ms || 0) / 1000);
    const pad = (/** @type {number} */ n) => String(n).padStart(2, "0");
    return s >= 3600
      ? `${Math.floor(s / 3600)}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`
      : `${Math.floor(s / 60)}:${pad(s % 60)}`;
  }
  // Free Mode's queue in view: the corner adds up the queue, and the list
  // underneath doesn't take keys
  const queueMs = $derived(freeQueue.songs.reduce((sum, song) => sum + (song.duration_ms || 0), 0));
  const runningTime = $derived(
    queueShown
      ? `0:00/${fmtRunning(queueMs)}`
      : `${fmtRunning(playlist.selectedDurationMs)}/${fmtRunning(playlist.totalDurationMs)}`,
  );
  $effect(() => {
    playlist.hidden = queueShown;
  });

  // A modern skin's playlist (routes/modern) shows these rows: sent when
  // they change (a moment later, so a burst goes once) and when it asks.
  // Its clicks come back to play, remove or move a row.
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let skinRowsTimer;
  const sendSkinRows = () => emitWindowEvent("skinPlaylist", playlist.skinRows());
  $effect(() => {
    playlist.skinRows(); // what it reads is what makes it send again
    clearTimeout(skinRowsTimer);
    skinRowsTimer = setTimeout(sendSkinRows, 150);
  });
  // A modern skin worn: this window is hidden, so what it shows over itself
  // (the jump box, the link box, a note...) shows in the skin's window.
  let modernOn = false;
  // what the skin's window is told once it's up (an update came first)
  let modernNote = "";
  invoke("modern_skin")
    .then((s) => (modernOn = !!s))
    .catch(() => {});
  onMount(() => {
    const commands = subscribeToWindowEvent("skinPlaylistCommand", (c) => {
      if (typeof c.play === "number") playlist.rows[c.play]?.play();
      if (typeof c.remove === "number") playlist.removeAt(c.remove);
      if (Array.isArray(c.move)) playlist.moveRow(c.move[0], c.move[1]);
      // the rows selected in the skin's playlist: the menus act on them
      if (Array.isArray(c.select)) {
        const picked = c.select.map((i) => playlist.rows[i]).filter(Boolean);
        playlist.selectedRows = picked;
        playlist.focusedRow = picked[picked.length - 1];
        playlist.selectionAnchor = picked[0];
      }
      // the skin window's jump box searches the names
      if (c.loadNames) playlist.loadAllNames();
      // its link box and its "Save as a list"
      if (typeof c.openUrl === "string") {
        urlText = c.openUrl;
        openUrl();
      }
      if (typeof c.saveList === "string") {
        newListName = c.saveList;
        saveCurrentAsList();
      }
      if (c.ask) {
        sendSkinRows();
        if (modernNote) {
          emitWindowEvent("modernDialog", { kind: "toast", text: modernNote });
          modernNote = "";
        }
      }
    });
    const face = subscribeToWindowEvent("modernFace", (on) => (modernOn = on));
    return () => {
      commands.then((unlisten) => unlisten());
      face.then((unlisten) => unlisten());
    };
  });
  const songTime = $derived.by(() => {
    const s = Math.floor((playlist.positionMs || 0) / 1000);
    return { m: String(Math.min(99, Math.floor(s / 60))), s: String(s % 60) };
  });

  // --- right-click menu ---
  let currentSkin = $state("classic");
  invoke("get_skin")
    .then((s) => (currentSkin = s || "classic"))
    .catch(() => {});
  // Which .wsz is on: the settings only say "custom", so the menu remembers
  // the pick itself ("bundled:<name>", "file:<name>" or "museum:<name>") to
  // tick the right entry. Only a convenience: without it, nothing custom is
  // ticked.
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
      if (update) {
        updateAvailable = update.version;
        // the pill is on this window: under a modern skin, a note says it
        modernNote = `Spotiamp+ ${update.version} is out: right-click, Help, Check for updates`;
        if (modernOn) emitWindowEvent("modernDialog", { kind: "toast", text: modernNote });
      }
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
  // Spotify's streaming quality in kbps (Audio > Quality)
  let audioQuality = $state(320);
  let taskbarExtras = $state(false);
  // Winamp's global hotkeys (Ctrl+Alt with Ins, Home, End, PgUp, PgDn and the
  // arrows), wherever you are. Opt-in.
  let globalHotkeys = $state(false);
  async function toggleGlobalHotkeys() {
    globalHotkeys = !globalHotkeys;
    await invoke("set_global_hotkeys", { enabled: globalHotkeys }).catch(() => {});
    if (globalHotkeys) showToast("Ctrl+Alt + Ins play · Home pause · End stop · PgUp/PgDn previous/next · arrows volume and jump");
  }
  // Song title on the taskbar button, progress across it, and prev/play/next
  // under its thumbnail. Opt-in.
  async function toggleTaskbarExtras() {
    taskbarExtras = !taskbarExtras;
    await invoke("set_taskbar_extras", { enabled: taskbarExtras }).catch(() => {});
  }
  // On-screen display: the new song in the screen's corner (osd.rs). Opt-in.
  let osdEnabled = $state(false);
  async function toggleOsd() {
    osdEnabled = !osdEnabled;
    await invoke("set_osd", { enabled: osdEnabled }).catch(() => {});
    if (osdEnabled) showToast("The next song will show in the corner of the screen");
  }
  // Lala, the llama on the player (mascot.rs): on unless switched off, three sizes.
  let mascotOn = $state(true);
  let mascotSize = $state(56);
  /** @param {{enabled?: boolean, size?: number}} change */
  function setMascot(change) {
    invoke("mascot_set", change).catch(() => {});
  }
  async function loadAlwaysOnTop() {
    try {
      const settings = await invoke("get_player_settings");
      alwaysOnTop = Boolean(settings?.always_on_top);
      normalizeVolume = Boolean(settings?.normalize);
      audioQuality = [96, 160].includes(settings?.quality) ? settings.quality : 320;
      taskbarExtras = Boolean(settings?.taskbar_extras);
      globalHotkeys = Boolean(settings?.global_hotkeys);
      osdEnabled = Boolean(settings?.osd);
      mascotOn = settings?.mascot !== false;
      mascotSize = settings?.mascot_size ?? 56;
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
  // Spotify Soloist in the menu only for those it's for: Spotify refused this
  // account's songs once (the player noted it), or it's set up already
  let soloistShown = $state(false);
  async function loadSoloistShown() {
    let refused = false;
    try {
      refused = localStorage.getItem("soloist-relevant") === "1";
    } catch {}
    const s = /** @type {any} */ (await invoke("soloist_status").catch(() => null));
    soloistShown = refused || !!s?.installed || !!s?.on;
  }
  /**
   * @param {string | null} windowLabel where to show it (null = here)
   * @param {() => any[]} [items] what's in it (the whole right-click menu by default)
   */
  async function showMenu(windowLabel, items = menuItems) {
    if (menuBusy) return;
    menuBusy = true;
    try {
      await Promise.all([loadAudioDevices(), loadAlwaysOnTop(), loadMenuLists(), loadLastfm(), loadModernSkin(), loadVisualizerOn(), loadSoloistShown()]);
      // Order matters here. Tauri's popup holds this page's resource table
      // locked on a worker until the menu closes, while it waits for the main
      // thread to show it; closing or creating a menu runs ON the main thread
      // and needs that same lock. Fired at the same time (the old close wasn't
      // awaited) the two could wait on each other forever: a blank white menu
      // and every window frozen. So: finish closing the old menu, then create
      // the new one, and only then pop it up.
      const old = openedMenu;
      openedMenu = null;
      if (old) await old.close().catch(() => {});
      const menu = await Menu.new({ items: items() });
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
    // (under a modern skin this playlist, and what's selected in it, is hidden)
    if (!rows.length || controllerMode || modernOn) return [];
    const uris = playlist.selectedSpotifyUris();
    const allLoved = uris.length > 0 && uris.every((u) => playlist.loved.has(u));
    const local = rows.length === 1 && rows[0].isLocal ? /** @type {{path: string, isStream: boolean}} */ (/** @type {any} */ (rows[0])) : null;
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
    if (local && !local.isStream) {
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

  // Last.fm (lastfm.rs): signed in or not, for the menu
  // The modern skin worn (a .wal; modern_window.rs), or null: the classic look.
  /** @type {string | null} */
  let modernSkin = null;
  async function loadModernSkin() {
    const worn = /** @type {{ name: string } | null} */ (await invoke("modern_skin").catch(() => null));
    modernSkin = worn?.name ?? null;
  }
  // whether the visualizer is open, for the menu's tick
  let visualizerOn = false;
  async function loadVisualizerOn() {
    const viz = await Window.getByLabel("visualizer").catch(() => null);
    visualizerOn = viz ? await viz.isVisible().catch(() => false) : false;
  }
  async function loadModernSkinFile() {
    await invoke("pick_modern_skin").catch((e) => handleError(new Error(`Could not load the skin: ${e}`)));
  }

  /** @type {{ available: boolean, user: string | null, scrobbling: boolean, love: boolean, waiting: number }} */
  let lastfm = { available: false, user: null, scrobbling: false, love: false, waiting: 0 };
  async function loadLastfm() {
    lastfm = await invoke("lastfm_status").catch(() => lastfm);
  }
  async function connectLastfm() {
    showToast("Allow Spotiamp+ on the Last.fm page in your browser…");
    try {
      const user = await invoke("lastfm_connect");
      showToast(`Scrobbling to Last.fm as ${user}`);
    } catch (e) {
      showToast(`${e}`);
    }
  }
  function lastfmItems() {
    const sep = { item: /** @type {const} */ ("Separator") };
    if (!lastfm.user) return [{ text: "Connect your Last.fm…", action: connectLastfm }];
    return [
      { text: menuText(`Signed in as ${lastfm.user}`), enabled: false },
      {
        text: "Scrobble what I play",
        checked: lastfm.scrobbling,
        action: () => invoke("lastfm_set_scrobbling", { on: !lastfm.scrobbling }).catch(() => {}),
      },
      {
        text: "Love songs there too (♡)",
        checked: lastfm.love,
        action: () => invoke("lastfm_set_love", { on: !lastfm.love }).catch(() => {}),
      },
      ...(lastfm.waiting ? [{ text: `${lastfm.waiting} waiting to go out (no internet?)`, enabled: false }] : []),
      sep,
      {
        text: "Disconnect",
        action: () => invoke("lastfm_disconnect").then(() => showToast("Last.fm disconnected")).catch(() => {}),
      },
    ];
  }

  // The list commands: the right-click menu's Playlist submenu, and what the
  // LIST OPTS button (bottom right, like Winamp's) opens, with the Library first.
  function listMenuItems() {
    const sep = { item: /** @type {const} */ ("Separator") };
    return controllerMode
      ? [
          { text: "Add file(s)…", accelerator: "O", action: addLocalFiles },
          { text: "Add folder…", accelerator: "Shift+O", action: addLocalFolder },
          { text: "Open a radio station…", accelerator: "Ctrl+L", action: () => (playlist.urlBoxOpen = true) },
          sep,
          { text: "Show the Spotify app's queue", checked: freeQueueOn, action: toggleFreeQueue },
          sep,
          { text: "Clear playlist", action: () => playlist.clear() },
        ]
      : [
          { text: "Add file(s)…", accelerator: "O", action: addLocalFiles },
          { text: "Add folder…", accelerator: "Shift+O", action: addLocalFolder },
          { text: "Open a link (Spotify or radio)…", accelerator: "Ctrl+L", action: () => (playlist.urlBoxOpen = true) },
          sep,
          { text: "Open playlist file (.m3u)…", action: openM3u },
          { text: "Save as playlist file (.m3u)…", action: saveM3u },
          sep,
          { text: "Save as a list…", action: openSaveList },
          { text: "Clear playlist", action: () => playlist.clear() },
        ];
  }
  function listOptsItems() {
    const sep = { item: /** @type {const} */ ("Separator") };
    return [{ text: "Library…", action: openLibraryWindow }, sep, ...listMenuItems()];
  }

  function menuItems() {
    const sep = { item: /** @type {const} */ ("Separator") };
    // (a modern skin worn: none of the classic ones is)
    const isWorn = (/** @type {string} */ custom) => !modernSkin && currentSkin === "custom" && wornCustom === custom;
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
    const listItems = listMenuItems();
    const skinItems = [
      { text: "Classic", checked: !modernSkin && currentSkin === "classic", action: () => chooseSkin("classic") },
      ...bundledSkins.map((name) => ({
        text: menuText(prettySkinName(name)),
        checked: isWorn(`bundled:${name}`),
        action: () => chooseBundledSkin(name),
      })),
      {
        text: "&Colors",
        items: ["cherry", "amber", "emerald"].map((c) => ({
          text: c[0].toUpperCase() + c.slice(1),
          checked: !modernSkin && currentSkin === c,
          action: () => chooseSkin(c),
        })),
      },
      // a skin from the Museum or a file: its name, ticked (the two below
      // are things to do, not choices, so they're never ticked)
      ...(!modernSkin && currentSkin === "custom" && /^(museum|file):./.test(wornCustom)
        ? [{ text: menuText(wornCustom.replace(/^(museum|file):/, "")), checked: true, enabled: false }]
        : []),
      // the modern skin worn, ticked
      ...(modernSkin ? [{ text: menuText(modernSkin.replace(/\.(wal|zip)$/i, "")), checked: true, enabled: false }] : []),
      sep,
      { text: "Load .wsz from disk…", action: loadWszSkin },
      { text: "Load a modern skin (.wal)…", action: loadModernSkinFile },
      { text: "Skin &Museum…", action: openSkinMuseum },
      sep,
      {
        text: "Use the skin's font",
        checked: rowFont === "skin",
        action: () => setRowFont(rowFont === "skin" ? "winamp" : "skin"),
      },
      {
        text: "Pixel font",
        checked: rowFont === "pixel",
        action: () => setRowFont(rowFont === "pixel" ? "winamp" : "pixel"),
      },
    ];
    const windowItems = [
      { text: "Library", accelerator: "L", action: openLibraryWindow },
      { text: "Visualizer", checked: visualizerOn, action: () => invoke("set_visualizer_window_visible", { visible: !visualizerOn }) },
      { text: "Lyrics", action: () => invoke("set_lyrics_window_visible", { visible: true }) },
      { text: "Album art", action: () => invoke("set_art_window_visible", { visible: true }) },
      ...(controllerMode
        ? []
        : [{ text: "Listening stats", action: () => invoke("set_stats_window_visible", { visible: true }) }]),
      sep,
      { text: "Always on top", checked: alwaysOnTop, action: toggleAlwaysOnTop },
      { text: "Taskbar extras (title, progress, buttons)", checked: taskbarExtras, action: toggleTaskbarExtras },
      { text: "Global hotkeys (Ctrl+Alt+…)", checked: globalHotkeys, action: toggleGlobalHotkeys },
      ...(controllerMode
        ? []
        : [{ text: "Show the song on screen when it changes", checked: osdEnabled, action: toggleOsd }]),
      {
        text: "&Lala the llama",
        items: [
          { text: "Show Lala", checked: mascotOn, action: () => setMascot({ enabled: !mascotOn }) },
          sep,
          ...[[48, "Small"], [56, "Normal"], [64, "Large"]].map(([px, label]) => ({
            text: /** @type {string} */ (label),
            checked: mascotOn && mascotSize === px,
            action: () => setMascot({ enabled: true, size: /** @type {number} */ (px) }),
          })),
        ],
      },
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
        text: "&Quality",
        items: [[320, "Very high"], [160, "High"], [96, "Normal"]].map(([kbps, label]) => ({
          text: `${label} (${kbps} kbps)`,
          checked: audioQuality === kbps,
          action: () => pickQuality(/** @type {number} */ (kbps)),
        })),
      },
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
      {
        text: "Keyboard shortcuts",
        accelerator: "F1",
        action: () => invoke("show_whats_new", { keysOnly: true }).catch(() => {}),
      },
      { text: "What's new in this version", action: openWhatsNew },
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
      ...(controllerMode || !lastfm.available ? [] : [{ text: "Last.&fm", items: lastfmItems() }]),
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
      ...(controllerMode
        ? [{ text: "Premium sign-in…", action: switchToPremium }]
        : [
            { text: "Free Mode: play through the Spotify app…", action: switchToFree },
            ...(soloistShown ? [{ text: "Spotify Soloist: Spotify's own player…", action: () => invoke("show_soloist_setup").catch(() => {}) }] : []),
            { text: "Log out of Spotify…", action: logOut },
          ]),
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
  let audioDevices = $state(/** @type {string[]} */ ([]));
  let currentAudioDevice = $state(/** @type {string | null} */ (null));
  async function loadAudioDevices() {
    try {
      const info = await invoke("list_audio_devices");
      audioDevices = info.devices || [];
      currentAudioDevice = info.current ?? null;
    } catch {
      audioDevices = [];
    }
  }
  /** @param {string | null} name null = the system default */
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

  // Streaming quality: some Premium plans don't stream 320 kbps. Read when
  // the player is built, so the song is picked up again like after normalize.
  /** @param {number} kbps */
  async function pickQuality(kbps) {
    audioQuality = kbps;
    await invoke("set_audio_quality", { kbps }).catch(() => {});
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
  // Ctrl+J: Winamp's jump to time. Takes 1:23, 83 (seconds) or 1:02:03.
  let timeJumpText = $state("");
  let timeJumpInput = $state(/** @type {HTMLInputElement | undefined} */ (undefined));
  $effect(() => {
    if (!playlist.timeJumpOpen) return;
    untrack(() => {
      timeJumpText = "";
      const win = getCurrentWindow();
      // asked for from the main window while the playlist is hidden: no box
      // (a modern skin worn: it shows in the skin's window)
      win
        .isVisible()
        .then((visible) => {
          if (!visible) {
            playlist.timeJumpOpen = false;
            if (modernOn) emitWindowEvent("modernDialog", { kind: "time" });
            return;
          }
          win.setFocus().catch(() => {});
          tick().then(() => timeJumpInput?.focus());
        })
        .catch(() => {});
    });
  });
  // Ctrl+L: Winamp's open location. A Spotify link or a radio station's.
  let urlText = $state("");
  let urlInput = $state(/** @type {HTMLInputElement | undefined} */ (undefined));
  $effect(() => {
    if (!playlist.urlBoxOpen) return;
    untrack(() => {
      urlText = "";
      const win = getCurrentWindow();
      // asked for from the main window while the playlist is hidden: no box
      // (a modern skin worn: it shows in the skin's window)
      win
        .isVisible()
        .then((visible) => {
          if (!visible) {
            playlist.urlBoxOpen = false;
            if (modernOn) emitWindowEvent("modernDialog", { kind: "url" });
            return;
          }
          win.setFocus().catch(() => {});
          tick().then(() => urlInput?.focus());
        })
        .catch(() => {});
    });
  });
  async function openUrl() {
    const text = urlText;
    playlist.urlBoxOpen = false;
    // Free Mode plays Spotify through its own app: a link of its can't play here
    if (controllerMode && /spotify/i.test(text)) {
      showToast("Spotify links need Premium; in Free Mode, play them in the Spotify app");
      return;
    }
    const why = await playlist.openUrl(text);
    if (why) showToast(why);
  }

  function jumpToTime() {
    const parts = timeJumpText.trim().split(":").map((p) => p.trim());
    if (!parts[0] || parts.length > 3 || parts.some((p) => !/^\d+$/.test(p))) return;
    const seconds = parts.reduce((total, p) => total * 60 + Number(p), 0);
    playlist.timeJumpOpen = false;
    emitWindowEvent("seekTo", { positionMs: seconds * 1000 });
  }

  async function openSaveList() {
    if (modernOn) {
      emitWindowEvent("modernDialog", { kind: "savelist" });
      return;
    }
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
    // this window hidden under a modern skin: the note shows in the skin's
    if (modernOn) emitWindowEvent("modernDialog", { kind: "toast", text });
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
  /** The player as it looks now: the classic one, or a modern skin's main window. */
  async function playerPicture() {
    if (!modernOn) {
      const png = /** @type {ArrayBuffer} */ (await invoke("capture_window_png", { label: "player" }));
      return createImageBitmap(new Blob([png], { type: "image/png" }));
    }
    const main = /** @type {[number, number, number, number] | null} */ (await invoke("modern_main_rect"));
    const modern = await Window.getByLabel("modern");
    if (!main || !modern) throw new Error("the modern skin isn't drawn");
    const origin = await modern.outerPosition();
    const png = /** @type {ArrayBuffer} */ (await invoke("capture_window_png", { label: "modern" }));
    const whole = await createImageBitmap(new Blob([png], { type: "image/png" }));
    return createImageBitmap(whole, main[0] - origin.x, main[1] - origin.y, main[2], main[3]);
  }

  /** @returns {Promise<Blob>} */
  async function buildNowPlayingCard() {
    const player = await playerPicture();
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
    // (a big modern skin less than that, so the card stays a card)
    const twice = Math.min(2, 1100 / Math.max(1, player.width / (dpr * uiScale)));
    const pw = Math.round((player.width / (dpr * uiScale)) * twice);
    const ph = Math.round((player.height / (dpr * uiScale)) * twice);
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
    // (a modern skin's own list colours: this window wears the classic ones)
    const modern = modernOn ? /** @type {any} */ (await invoke("modern_look").catch(() => null))?.colors : null;
    const bg = modern?.listBg ?? skinColor("--skin-plbg", "#000000");
    const fg = modern?.listText ?? skinColor("--skin-plnormal", "rgb(0, 255, 0)");
    const hi = modern?.current ?? skinColor("--skin-plcurrent", "#ffffff");

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
    if (modernOn) {
      // hidden under a modern skin, this page can't have the focus the
      // clipboard wants: the skin's window, which has it, puts the card there
      try {
        const bytes = new Uint8Array(await (await buildNowPlayingCard()).arrayBuffer());
        let text = "";
        for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        emitWindowEvent("modernDialog", { kind: "card", text: btoa(text) });
      } catch (e) {
        showToast("Couldn't copy the card");
        invoke("log_frontend_error", { window: "playlist", message: `now playing card: ${e}` }).catch(() => {});
      }
      return;
    }
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

  // Winamp's playlist files (m3u.rs): local files by path, Spotify songs as
  // open.spotify.com links, so other players still get the local ones.
  async function openM3u() {
    try {
      const opened = /** @type {{entries: ({kind: "spotify" | "local", value: string} | {kind: "stream", value: {url: string, name: string}})[], skipped: number} | null} */ (
        await invoke("m3u_open")
      );
      if (!opened) return; // cancelled
      if (!opened.entries.length) {
        showToast("No songs in that file that Spotiamp+ can play");
        return;
      }
      await playlist.openM3uEntries(opened.entries);
      showToast(
        `Opened ${opened.entries.length} songs` +
          (opened.skipped ? `, skipped ${opened.skipped} (missing files or other streams)` : ""),
      );
    } catch (e) {
      showToast(`Couldn't open it: ${e}`);
    }
  }
  async function saveM3u() {
    if (!playlist.rows.length) {
      showToast("The playlist is empty");
      return;
    }
    try {
      const saved = await invoke("m3u_save", { rows: await playlist.m3uRows() });
      if (saved) showToast("Playlist file saved");
    } catch (e) {
      showToast(`Couldn't save it: ${e}`);
    }
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
    // Lala goes to sleep with you
    emit("sleepTimer", { minutes: next }).catch(() => {});
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
  // Free Mode by choice (Spotify refusing the account's songs, say): the
  // Spotify app plays and Spotiamp+ is its Winamp face. Premium sign-in in
  // this menu comes back.
  async function switchToFree() {
    const sure = await ask(
      "Switch to Free Mode? Spotiamp+ restarts and then drives the Spotify app, which does the playing. " +
        "Premium sign-in in this menu brings the built-in player back.",
      { title: "Spotiamp+", kind: "info", okLabel: "Free Mode" },
    );
    if (!sure) return;
    await invoke("enter_controller_mode").catch(() => {});
    await relaunch().catch(() => {});
  }
  // Log out: the saved login goes and Spotiamp+ starts again at the sign-in,
  // where any account can sign in (Spotify's page forgets the old one too).
  async function logOut() {
    const sure = await ask("Log out of Spotify? Spotiamp+ restarts and asks you to sign in again.", {
      title: "Spotiamp+",
      kind: "info",
      okLabel: "Log out",
    });
    if (!sure) return;
    await invoke("log_out").catch(() => {});
    await relaunch().catch(() => {});
  }
  /** @param {string} skin */
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
      setWorn(`file:${name}`);
      emitWindowEvent("skinChanged", { skin: "custom" });
    } catch (e) {
      handleError(new Error(`Could not load skin: ${e}`));
    }
  }

  // .wsz skins shipped with the app, selectable straight from the menu
  let bundledSkins = $state(/** @type {string[]} */ ([]));
  invoke("list_bundled_skins")
    .then((names) => (bundledSkins = names))
    .catch(() => {});
  const prettySkinName = (/** @type {string} */ name) => name.replaceAll("_", " ");

  /**
   * The selected row's name, when it's cut off, scrolls through like the
   * main window's title: a moment's wait, along to its end one screen pixel
   * at a time (so the pixel font stays sharp), a wait, and back to the start.
   * @param {HTMLElement} node
   * @param {boolean} on
   */
  function marquee(node, on) {
    /** @type {ReturnType<typeof setInterval> | undefined} */
    let timer;
    const cell = node.parentElement;
    function stop() {
      clearInterval(timer);
      timer = undefined;
      node.style.transform = "";
      cell?.classList.remove("marquee");
    }
    function start() {
      stop();
      if (!cell) return;
      const step = 1 / (window.devicePixelRatio || 1); // one screen pixel
      // by the clock, not by ticks: a window in the background gets its
      // timers slowed right down
      let phase = "wait";
      let since = performance.now();
      let end = 0;
      const measure = () => {
        cell.classList.remove("marquee");
        node.style.transform = "";
        end = Math.ceil((cell.scrollWidth - cell.clientWidth) / step) * step;
        if (end > 0) cell.classList.add("marquee");
        phase = "wait";
        since = performance.now();
      };
      measure();
      timer = setInterval(() => {
        const t = performance.now() - since;
        if (phase === "wait") {
          // a moment first (and the window may have narrowed or widened since)
          if (t < 1200) return;
          if (end <= 0) return measure();
          phase = "scroll";
          since = performance.now();
        } else if (phase === "scroll") {
          // a screen pixel every 40 ms
          const offset = Math.min(Math.floor(t / 40) * step, end);
          node.style.transform = `translateX(${-offset}px)`;
          if (offset >= end) {
            phase = "end";
            since = performance.now();
          }
        } else if (t >= 1600) {
          // a wait at the end, then from the start again
          measure();
        }
      }, 40);
    }
    if (on) start();
    return {
      /** @param {boolean} next */
      update(next) {
        if (next && !timer) start();
        else if (!next) stop();
      },
      destroy: stop,
    };
  }

  // The rows' font, as Winamp 5 had it: Arial (its default, "custom font"
  // on), the skin's (PLEDIT.TXT Font=, "Use the skin's font"), or the pixel
  // font of Spotiamp+'s earlier versions. The first two are painted the way
  // Winamp painted its rows (winampPe.ts); the skin's font also reaches the
  // other windows' lists (see +layout.svelte).
  /** @returns {"winamp" | "skin" | "pixel"} */
  function readRowFont() {
    try {
      if (localStorage.getItem("pixel-font") === "1") return "pixel";
      return localStorage.getItem("skin-font") === "1" ? "skin" : "winamp";
    } catch {
      return "winamp";
    }
  }
  let rowFont = $state(readRowFont());
  /** @param {"winamp" | "skin" | "pixel"} font */
  function setRowFont(font) {
    rowFont = font;
    try {
      localStorage.setItem("skin-font", font === "skin" ? "1" : "0");
      localStorage.setItem("pixel-font", font === "pixel" ? "1" : "0");
    } catch {}
    // this window too (the storage event only reaches the others)
    window.dispatchEvent(new StorageEvent("storage", { key: "skin-font" }));
  }

  /** @param {string} name */
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

  /** @param {string} uri */
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
      // a song dragged from the Library's Local Media comes as a file: link
      const files = urls.filter((u) => u.startsWith("file:"));
      if (files.length) await playlist.addLocalFiles(files.map(fromFileUrl), false);
      // a radio station's link (from the Library's Radio, or a web page)
      const isStation = (/** @type {string} */ u) => /^https?:\/\//i.test(u) && !/open\.spotify\.com/i.test(u);
      const stations = urls.filter(isStation);
      if (stations.length) await playlist.addLocalFiles(stations, false);
      const rest = urls.filter((u) => !u.startsWith("file:") && !isStation(u));
      if (rest.length) await playlist.addUrls(rest);
    });

    emitWindowEvent("playlistWindow", { Ready: null });

    // music files dropped here from Explorer: added at the end, as Winamp does
    const offDropped = getCurrentWindow().listen("filesDropped", (e) => {
      const paths = /** @type {string[]} */ (e.payload);
      if (paths?.length) playlist.addLocalFiles(paths, false);
    });

    // Quietly check for a new version on launch so the pill can flag it.
    checkForUpdatesSilently();

    // Right-click on the main window: the same menu, shown there.
    /** @type {(() => void) | undefined} */
    let unsubMenu;
    subscribeToWindowEvent("playerWindow", (e) => {
      if (e.MenuRequested !== undefined) showMenu("player");
    }).then((u) => (unsubMenu = u));
    // ...and on a modern skin's window, from its menus and its right-click
    /** @type {(() => void) | undefined} */
    let unsubModernMenu;
    subscribeToWindowEvent("modernMenu", () => showMenu("modern")).then((u) => (unsubModernMenu = u));
    // A skin put on from the Skin Museum window: tick it in the menu.
    /** @type {(() => void) | undefined} */
    let unsubSkin;
    subscribeToWindowEvent("skinChanged", (e) => {
      if (e.skin === "custom" && e.from === "museum") {
        currentSkin = "custom";
        setWorn(`museum:${e.name ?? ""}`);
      }
    }).then((u) => (unsubSkin = u));

    // Cleanups
    return () => {
      cleanupDropHandler();
      offDropped.then((f) => f()).catch(() => {});
      playlist.dispose();
      unsubMenu?.();
      unsubModernMenu?.();
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
  // Winamp's rows (not the pixel font): painted as Winamp painted them, a
  // row the font's height (15 px for Arial), whole rows scrolled
  const nativeRows = $derived(rowFont !== "pixel" && winampPeAvailable());
  let peRowH = $state(15);
  const rowHeight = $derived(nativeRows ? peRowH : PLAYLIST_ROW_HEIGHT);
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
    return rowHeight;
  }

  function syncScrollThumb() {
    // Winamp's rows: whole rows only, as Winamp scrolled its list
    if (nativeRows && scrollElement) {
      const want = Math.min(Math.round(scrollElement.scrollTop / rowHeight) * rowHeight, scrollMax());
      if (Math.abs(scrollElement.scrollTop - want) > 0.5) scrollElement.scrollTop = want;
    }
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
    if (nativeRows) {
      winampWheel(event);
      return;
    }
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

  // Winamp's wheel (Pledit.cpp): a notch (120) two rows, the rest carried
  // over to the next one
  let wheelCarry = 0;
  /** @param {WheelEvent} event */
  function winampWheel(event) {
    const d = event.deltaY || event.deltaX;
    let z = Math.round(event.deltaMode === 1 ? -d * 40 : event.deltaMode === 2 ? -d * 360 : (-d / 100) * 120);
    if ((z < 0 && wheelCarry > 0) || (z > 0 && wheelCarry < 0)) wheelCarry = 0;
    else z += wheelCarry;
    z *= 2;
    let lines = Math.trunc(z / 120);
    wheelCarry = z - lines * 120;
    if (!lines && z > 0) lines = 1;
    if (lines) scrollByRows(-lines);
  }

  // ---- Winamp's rows, painted ----
  /** @type {HTMLCanvasElement | undefined} */
  let peCanvas = $state();
  let skinTick = $state(0);
  let pePainting = false;
  let peAgain = false;
  /** the list's colours (PLEDIT.TXT), as the skin's variables have them */
  function peColors() {
    const style = getComputedStyle(document.body);
    const ctx = document.createElement("canvas").getContext("2d");
    /** @param {string} name @param {string} fallback @returns {[number, number, number]} */
    const read = (name, fallback) => {
      if (!ctx) return [0, 0, 0];
      ctx.fillStyle = fallback;
      ctx.fillStyle = style.getPropertyValue(name).trim() || fallback;
      const hex = String(ctx.fillStyle);
      const m = hex.match(/^#([0-9a-f]{6})$/i);
      if (m) {
        const v = parseInt(m[1], 16);
        return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
      }
      const n = hex.match(/\d+/g) ?? ["0", "0", "0"];
      return [Number(n[0]), Number(n[1]), Number(n[2])];
    };
    return [read("--skin-plnormal", "#00ff00"), read("--skin-plcurrent", "#ffffff"), read("--skin-plbg", "#000000"), read("--skin-plselbg", "#0000c6")];
  }
  function peFace() {
    if (rowFont !== "skin") return "Arial";
    const face = getComputedStyle(document.body).getPropertyValue("--skin-plfont").trim().replace(/^["']|["']$/g, "");
    return face || "Arial";
  }
  function schedulePePaint() {
    if (pePainting) {
      peAgain = true;
      return;
    }
    pePainting = true;
    paintPeRows()
      .catch(() => {})
      .finally(() => {
        pePainting = false;
        if (peAgain) {
          peAgain = false;
          schedulePePaint();
        }
      });
  }
  async function paintPeRows() {
    const canvas = peCanvas;
    if (!canvas || !nativeRows) return;
    const W = playlist.width * 25;
    const H = playlist.height * 29;
    const w = W - 32;
    const h = H - 58;
    if (w <= 0 || h <= 0) return;
    const offs = scrollElement ? Math.round(scrollElement.scrollTop / rowHeight) : 0;
    const fits = Math.max(0, Math.floor((H - 60) / peRowH)) + 2;
    /** @type {{ name: string, time: number, sel: number, fg?: [number, number, number] | null }[]} */
    const rows = [];
    if (queueShown) {
      const songs = freeQueue.state === "ok" ? freeQueue.songs : [];
      for (let i = offs; i < songs.length && rows.length < fits; i++) {
        const song = songs[i];
        const title = song.artist ? `${song.artist} - ${song.title}` : song.title;
        rows.push({ name: rowName(i, songs.length, title), time: song.duration_ms ? Math.floor(song.duration_ms / 1000) : -1, sel: i === 0 ? 2 : 0 });
      }
      if (!songs.length && rows.length < fits) rows.push({ name: freeQueueNote, time: -1, sel: 0 });
    } else {
      const list = playlist.rows;
      // the playing row, or the first while nothing is loaded: Winamp's
      // playlist always has its place (the first row to begin with)
      const current = playlist.loadedRow ?? list[0];
      for (let i = offs; i < list.length && rows.length < fits; i++) {
        const row = list[i];
        const queued = playlist.queuePosition(row);
        const loved = !row.isLocal && playlist.loved.has(row.uri.asString);
        const title = `${queued ? `[${queued}] ` : ""}${loved ? "\u2661 " : ""}${row.displayName ?? ""}`;
        const ms = /** @type {any} */ (row).durationMs || /** @type {any} */ (row).track?.durationInMs || 0;
        const isCurrent = row === current;
        rows.push({
          name: rowName(i, list.length, title),
          time: ms > 0 ? Math.floor(ms / 1000) : -1,
          sel: (row.isSelected() ? 1 : 0) | (isCurrent ? 2 : 0),
          fg: row.unavailable ? (isCurrent ? [140, 140, 140] : [80, 80, 80]) : null,
        });
      }
    }
    const painted = await paintPe({ face: peFace(), size: 12, w, h, rows, colors: peColors() });
    if (!painted) return;
    if (painted.rowH !== peRowH) {
      // the font's own height: the list's rows follow, then again
      peRowH = painted.rowH;
      peAgain = true;
    }
    const pad = Math.max(0, h - 2 - painted.rows * painted.rowH);
    document.body.style.setProperty("--pe-pad-bottom", `${pad}px`);
    if (canvas.width !== painted.w) canvas.width = painted.w;
    if (canvas.height !== painted.h) canvas.height = painted.h;
    canvas.getContext("2d")?.putImageData(new ImageData(/** @type {any} */ (painted.data), painted.w, painted.h), 0, 0);
  }
  $effect(() => {
    if (!nativeRows || !peCanvas) return;
    // what the rows show: repainted when any of it changes
    void scroll;
    void skinTick;
    void rowFont;
    void peRowH;
    void playlist.width;
    void playlist.height;
    void playlist.loadedRow;
    void playlist.selectedRows;
    void playlist.queue;
    void queueShown;
    void freeQueue.songs;
    const list = playlist.rows;
    const offs = scrollElement ? Math.round(scrollElement.scrollTop / rowHeight) : 0;
    for (let i = offs; i < Math.min(list.length, offs + 60); i++) {
      void list[i].displayName;
      void list[i].displayDuration;
      void list[i].unavailable;
    }
    schedulePePaint();
  });
  // Winamp's place in the list for the main window's title: the loaded row
  // (its number), or the first row while nothing is loaded
  let lastTitleRow = "";
  let titleAgain = $state(0);
  $effect(() => {
    void titleAgain;
    const list = playlist.rows;
    const loaded = playlist.loadedRow;
    const row = loaded ?? list[0];
    const index = row ? list.indexOf(row) : -1;
    const info =
      index < 0 || queueShown
        ? null
        : {
            position: index + 1,
            name: row?.displayName ?? "",
            durationMs: /** @type {any} */ (row)?.durationMs || /** @type {any} */ (row)?.track?.durationInMs || 0,
            loaded: !!loaded,
          };
    const key = JSON.stringify(info);
    if (key === lastTitleRow) return;
    lastTitleRow = key;
    emitWindowEvent("playlistWindow", { TitleRow: info });
  });
  onMount(() => {
    const again = () => {
      skinTick++;
    };
    window.addEventListener(SKIN_APPLIED, again);
    // a player window that's (re)started is told the title row again
    const ready = subscribeToWindowEvent("playerWindow", (e) => {
      if (e.PlayerReady !== undefined) {
        lastTitleRow = "";
        titleAgain++;
      }
    });
    return () => {
      window.removeEventListener(SKIN_APPLIED, again);
      ready.then((unlisten) => unlisten());
    };
  });

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

    // One pass over the rows: looking each one up again (indexOf) made a
    // click in a 1400-song list take 150 ms.
    const selected = playlist.selectedSet;
    /** @type {Row[]} */
    const block = [];
    /** @type {Row[]} */
    const remaining = [];
    // Where the block sits among the non-dragged rows at the start.
    let baseInsert = 0;
    for (const r of playlist.rows) {
      if (selected.has(r)) {
        block.push(r);
      } else {
        remaining.push(r);
        if (!block.length) baseInsert++;
      }
    }
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
  role="presentation"
  class:inactive={!windowActive}
  class:pe-native={nativeRows}
  style:--playlist-w={playlist.width}
  style:--playlist-h={playlist.height}
  style:--track-row-height={`${rowHeight}px`}
  oncontextmenu={openMenu}
>
  {#if updateAvailable}
    <button
      class="update-pill"
      onclick={checkForUpdates}
      title="Spotiamp+ {updateAvailable} is available: click to update"
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
        {#if queueShown}
          <!-- Free Mode: the Spotify app's song and its queue, read only -->
          {#each freeQueue.state === "ok" ? freeQueue.songs : [] as song, index}
            <tr class="playlist-track" class:loaded={index === 0} title="the Spotify app's queue: change it there">
              <td class="playlist-track-main">
                <span class="playlist-track-number">{index + 1}.&nbsp;</span>
                <span class="playlist-track-name">{song.artist ? `${song.artist} - ${song.title}` : song.title}</span>
              </td>
              <td class="playlist-track-duration">{durationToString(song.duration_ms)}</td>
            </tr>
          {:else}
            <tr class="playlist-track">
              <td class="playlist-track-main" colspan="2"><span class="playlist-track-name">{freeQueueNote}</span></td>
            </tr>
          {/each}
        {:else}
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
              {#if !row.isLocal && playlist.loved.has(row.uri.asString)}
                <!-- loved (F); in the row's own colour so it suits every skin -->
                <span class="playlist-track-loved" title="loved">♡&nbsp;</span>
              {/if}
              <!-- the full name on hover: a long one is cut off with "…" -->
              <span
                class="playlist-track-name"
                title={row.displayName}
                use:marquee={!nativeRows && playlist.selectedRows.length === 1 && row.isSelected()}
              >{row.displayName}</span>
            </td>
            <td class="playlist-track-duration">{row.displayDuration}</td>
          </tr>
        {/each}
        {/if}
      </tbody>
    </table>
    <input
      class="sprite scroll-bar"
      type="range"
      bind:value={scroll}
      oninput={onManualScroll}
    />
  </div>
  {#if nativeRows}
    <!-- Winamp's rows, painted over the list (whose rows stay for the mouse) -->
    <canvas class="pe-rows" bind:this={peCanvas}></canvas>
  {/if}

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
    aria-label="Add: open the Library"
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

  <!-- LIST OPTS, bottom right: the Library and the list commands, as in Winamp -->
  <button
    class="pl-listopts"
    onclick={() => showMenu(null, listOptsItems)}
    aria-label="List options"
    title="library and list options"
  ></button>

  <!-- the close button PLEDIT.BMP draws in the top-right corner; the main
       window keeps the PL switch, so it's told instead of this one hiding -->
  <button
    class="pl-close"
    data-no-drag
    onclick={() => emitWindowEvent("playlistWindow", { CloseRequested: null })}
    aria-label="Close"
  ></button>

  <!-- the corner's two black areas, in the skin's TEXT.BMP letters as Winamp
       draws them: the running time (selected/total) above, the song's time
       below, around the colon the skin paints there -->
  <div class="pl-corner">
    <TextTicker unavailable={false} text={runningTime.slice(0, 18).padEnd(18)} textOverride={undefined} x={7} y={10} chars={18} />
    <MiniTime minutes={songTime.m} seconds={songTime.s} x={66} y={23} clear={playlist.playState === "stopped"} />
  </div>

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

  {#if playlist.timeJumpOpen}
    <!-- Ctrl+J: jump to a time in the song -->
    <div class="jump-backdrop" role="presentation" onmousedown={() => (playlist.timeJumpOpen = false)}></div>
    <div class="jump-box save-box" role="dialog" aria-label="Jump to time">
      <input
        class="jump-input"
        bind:this={timeJumpInput}
        bind:value={timeJumpText}
        onkeydown={(e) => {
          if (e.key === "Enter") jumpToTime();
          else if (e.key === "Escape") playlist.timeJumpOpen = false;
        }}
        placeholder="jump to time, like 1:23"
        spellcheck="false"
        aria-label="Time to jump to"
      />
      <div class="jump-empty">Now at {fmtTime(playlist.positionMs)}. Enter jumps there, Esc cancels.</div>
    </div>
  {/if}

  {#if playlist.urlBoxOpen}
    <!-- Ctrl+L: open a link, Spotify's or a radio station's -->
    <div class="jump-backdrop" role="presentation" onmousedown={() => (playlist.urlBoxOpen = false)}></div>
    <div class="jump-box save-box" role="dialog" aria-label="Open a link">
      <input
        class="jump-input"
        bind:this={urlInput}
        bind:value={urlText}
        onkeydown={(e) => {
          if (e.key === "Enter") openUrl();
          else if (e.key === "Escape") playlist.urlBoxOpen = false;
        }}
        placeholder="a Spotify link or a radio station"
        spellcheck="false"
        aria-label="Link to open"
      />
      <div class="jump-empty">A song, album or playlist from Spotify, or a station's stream (http://…, .pls, .m3u).</div>
    </div>
  {/if}
</span>

<style>
  /* the playlist's own cursor (Winamp's PNORMAL), if the skin has one */
  :global(body),
  :global(body *) {
    cursor: var(--skin-cur-pnormal, var(--skin-cur-normal, url(/src/static/assets/skins/base-2.91/MAINMENU.CUR))), auto;
  }
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }

  .draggable-corner {
    cursor: var(--skin-cur-psize, url(/src/static/assets/skins/base-2.91/PSIZE.CUR)), default;
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
    /* right under the title bar, as in Winamp (it was 14px lower while the
       "♪ library" label sat above it) */
    margin-top: calc(20px * var(--zoom));
    margin-left: calc(10px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 29px) * var(--zoom));
    height: calc((var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom));
    overflow-x: hidden;
    overflow-y: scroll;
    /* the whole list in the skin's PLEDIT.TXT NormalBG, rows or not, as
       Winamp fills it (an empty list showed the window's black) */
    background: var(--skin-plbg, #000);
  }

  /* Winamp's rows: the list where Winamp had it (x 12 to width - 20, y 20 to
     height - 38, the first row 2 px down), its rows painted on .pe-rows */
  .pe-native .tracks-container {
    box-sizing: border-box;
    margin-left: calc(12px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 32px) * var(--zoom));
    height: calc((var(--playlist-h) * 29px - 58px) * var(--zoom));
    padding-top: calc(2px * var(--zoom));
    padding-bottom: calc(var(--pe-pad-bottom, 0px) * var(--zoom));
  }
  .pe-native #playlist-tracks,
  .pe-native .playlist-track.loaded,
  .pe-native .playlist-track.unavailable,
  .pe-native .playlist-track-queue {
    color: transparent;
  }
  .pe-native .playlist-track.selected {
    background-color: transparent;
  }
  .pe-rows {
    position: absolute;
    left: calc(12px * var(--zoom));
    top: calc(20px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 32px) * var(--zoom));
    height: calc((var(--playlist-h) * 29px - 58px) * var(--zoom));
    pointer-events: none;
    image-rendering: pixelated;
    z-index: 1;
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
    cursor: var(--skin-cur-pvscroll, url(/src/static/assets/skins/base-2.91/EQSLID.CUR)), default;
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
    font-size: calc(8px * var(--zoom) * var(--px-crisp, 1));
    font-smooth: never;
    -webkit-font-smoothing: none;

    letter-spacing: 0;
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
  /* the selected row's name scrolling through (see marquee()) */
  .playlist-track-main:global(.marquee) {
    text-overflow: clip;
  }
  .playlist-track-main:global(.marquee) .playlist-track-name {
    display: inline-block;
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
    top: calc(20px * var(--zoom));
    left: calc(10px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 29px) * var(--zoom));
    height: calc((var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom));
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    background: var(--skin-plbg, #000);
    border: 1px solid var(--skin-plnormal, rgb(0, 255, 0));
    color: var(--skin-plnormal, rgb(0, 255, 0));
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(8px * var(--zoom) * var(--px-crisp, 1));
    -webkit-font-smoothing: none;
    letter-spacing: 0;
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
    font-size: calc(8px * var(--zoom) * var(--px-crisp, 1));
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
    cursor: var(--skin-cur-ptbar, url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR)), default;
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
  }

  .playlist-t-sprite {
    cursor: var(--skin-cur-ptbar, url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR)), default;
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    background-position: -127px 0px;
  }

  .playlist-title-sprite {
    cursor: var(--skin-cur-ptbar, url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR)), default;
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
  /* another window in front: PLEDIT's dim titlebar, 21 px lower, as Winamp's */
  .inactive > .playlist-tl-sprite {
    background-position: 0px -21px;
  }
  .inactive > .playlist-t-sprite {
    background-position: -127px -21px;
  }
  .inactive > .playlist-title-sprite {
    background-position: -26px -21px;
  }
  .inactive > .playlist-tr-sprite {
    background-position: -153px -21px;
  }

  /* the sides as Winamp drew them: 12 px from PLEDIT.BMP (0, 42) and 20 px
     from (31, 42), the list between (x 12 to width - 20) */
  .playlist-l-sprite {
    --sprite-url: var(--skin-pledit);
    width: 12px;
    height: 29px;
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: 0px -42px;
  }

  .playlist-r-sprite {
    --sprite-url: var(--skin-pledit);
    width: 20px;
    height: 29px;
    --x: var(--playlist-w);
    --sprite-x: calc((var(--x) - 1) * 25px + 5px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: -31px -42px;
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

  /* LIST OPTS: a clear button over the one PLEDIT.BMP draws */
  .pl-listopts {
    position: absolute;
    right: calc(22px * var(--zoom));
    bottom: calc(12px * var(--zoom));
    width: calc(22px * var(--zoom));
    height: calc(18px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 60;
  }

  /* the titlebar's close button, where Winamp has it */
  .pl-close {
    position: absolute;
    right: calc(2px * var(--zoom));
    top: calc(3px * var(--zoom));
    width: calc(9px * var(--zoom));
    height: calc(9px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: var(--skin-cur-close, url(/src/static/assets/skins/base-2.91/CLOSE.CUR)), default;
    z-index: 60;
  }

  /* PLEDIT.BMP's bottom-right corner piece (150 x 38): the readouts are
     placed in it as in Winamp */
  .pl-corner {
    position: absolute;
    right: 0;
    bottom: 0;
    width: calc(150px * var(--zoom));
    height: calc(38px * var(--zoom));
    pointer-events: none;
    z-index: 55;
  }


  /* ------ MY PLAYLISTS browser (our addition) ------ */
  /* "update available" pill — only rendered when a launch check found a newer
     version. Sits on the title bar's left end, softly pulsing so it's noticed
     without nagging; clicking it runs the normal download/install flow. */
  .update-pill {
    position: absolute;
    /* on the titlebar (20 px), centred, past its left corner piece; every
       size scales with the window so it stays put at any zoom */
    top: calc(3px * var(--zoom));
    left: calc(26px * var(--zoom));
    z-index: 41;
    height: calc(14px * var(--zoom));
    padding: 0 calc(6px * var(--zoom));
    font-family: monospace;
    font-size: calc(9px * var(--zoom));
    line-height: calc(12px * var(--zoom));
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
