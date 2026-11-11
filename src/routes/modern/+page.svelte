<script>
  // A modern skin (Winamp 3 and 5's .wal; modern_window.rs). Webamp Modern's
  // engine (src/lib/modern) draws it in this window, which lies over all the
  // screens; the skin's own windows (Winamp's containers) are moved around in
  // it, and only where they are does the window exist (its region), so every
  // other click goes to what's below. The skin plays nothing itself: it shows
  // what the hidden classic player does (skinState, skinPlaylist, the EQ, the
  // spectrum) and its buttons drive that player (skinCommand and friends).
  import "$lib/modern/WebampModern"; // first: the engine's own load order
  import "$lib/modern/skin/classNames"; // its classes' names, which the build shortens
  import "$lib/modern/css/webamp.css";
  import { UIRoot } from "$lib/modern/UIRoot";
  import SkinEngineWAL from "$lib/modern/skin/SkinEngine_WAL";
  import { ZipFileExtractor } from "$lib/modern/skin/FileExtractor";
  import AUDIO_PLAYER from "$lib/modern/skin/AudioPlayer";
  import { HOST } from "$lib/modern/host";
  import { buildLook } from "$lib/modern/look.js";
  import { ensurePlaylistWindow, playlistContainer } from "$lib/modern/pleditFrame.js";
  import { frameInsets, paintFrame, setFrameActive, wireFrame } from "$lib/modern-look.js";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { message } from "@tauri-apps/plugin-dialog";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";
  import { handleDrop } from "$lib/common.svelte.js";
  import { onMount, tick } from "svelte";

  /** @type {HTMLDivElement} */
  let root;
  /** @type {UIRoot | null} */
  let uiRoot = null;
  /** the engine that drew it, which draws the look for the other windows too @type {any} */
  let engine = null;
  let skinName = "";

  // ---- the skin's windows: where they are, at what size, and saved ----

  /** @type {import("$lib/modern/host").Screen[]} */
  let screens = [];
  /** The screens, here and for the skin's own screen questions (host.ts). @param {any} list */
  function setScreens(list) {
    screens = Array.isArray(list) ? list : [];
    HOST.screens = screens;
  }
  HOST.mainSpot = () => {
    const main = containers().find((c) => c.getId() === "main");
    return main ? { x: main._x + 8, y: main._y + 8 } : null;
  };
  const positionsKey = () => `modern-pos:${skinName}`;

  /** The skin's windows, as the engine has them. @returns {any[]} */
  const containers = () => uiRoot?.getContainers() ?? [];

  /** Where a skin window is on the page (its layout, as drawn). @param {any} c */
  function rectOf(c) {
    if (!c.getVisible() || !c._activeLayout) return null;
    const r = c._activeLayout.getDiv().getBoundingClientRect();
    return r.width > 0 && r.height > 0 ? r : null;
  }

  // How far a skin window's parts reach past its layout's box (a knob
  // sticking out, a drawer): measured part by part when nothing is being
  // dragged, and reused while it is (measuring every frame would be slow).
  /** @type {WeakMap<any, { l: number, t: number, r: number, b: number }>} */
  const overhang = new WeakMap();
  /** The whole of a skin window, parts reaching out included. @param {any} c @param {boolean} measure */
  function fullRectOf(c, measure) {
    const box = rectOf(c);
    if (!box) return null;
    if (measure || !overhang.has(c)) {
      let l = box.left, t = box.top, r = box.right, b = box.bottom;
      const parts = c._activeLayout.getDiv().querySelectorAll("*");
      for (let i = 0; i < parts.length && i < 4000; i++) {
        const p = parts[i].getBoundingClientRect();
        if (p.width < 1 || p.height < 1) continue;
        l = Math.min(l, p.left);
        t = Math.min(t, p.top);
        r = Math.max(r, p.right);
        b = Math.max(b, p.bottom);
      }
      // (never more than a little: an odd page-wide part mustn't make the
      // window catch clicks all over the screen)
      const most = (/** @type {number} */ v) => Math.min(256, Math.max(0, v));
      overhang.set(c, { l: most(box.left - l), t: most(box.top - t), r: most(r - box.right), b: most(b - box.bottom) });
    }
    const o = /** @type {{ l: number, t: number, r: number, b: number }} */ (overhang.get(c));
    return { left: box.left - o.l, top: box.top - o.t, width: box.width + o.l + o.r, height: box.height + o.t + o.b };
  }

  /** The screen a point is on (the first one when it's on none). @param {number} x @param {number} y */
  function screenAt(x, y) {
    return screens.find((s) => x >= s.x && y >= s.y && x < s.x + s.width && y < s.y + s.height) ?? screens[0];
  }

  /** Spotiamp+'s Scale (Ctrl+D: 2x), for the skin's windows too. */
  let uiScale = 1;

  /** Each skin window at its screen's scale (125 %: a quarter bigger), as the
   *  classic windows are, times Spotiamp+'s Scale; one that wouldn't fit on
   *  its screen so (a big skin at 2x) is drawn as big as fits, so all of it,
   *  its buttons too, can be reached. */
  function fitToScreens() {
    for (const c of containers()) {
      const s = screenAt(c._x + 8, c._y + 8);
      let scale = (s?.scale ?? 1) * uiScale;
      const layout = c._activeLayout?.getDiv();
      const w = layout?.offsetWidth ?? 0;
      const h = layout?.offsetHeight ?? 0;
      if (s && w > 0 && h > 0) {
        const room = Math.min((s.work_width ?? s.width) / w, (s.work_height ?? s.height) / h);
        if (room < scale) scale = Math.max(0.5, Math.floor(room * 100) / 100);
      }
      const div = c.getDiv();
      const want = scale === 1 ? "" : `scale(${scale})`;
      if (div.style.transform !== want) {
        div.style.transformOrigin = "0 0";
        div.style.transform = want;
      }
    }
  }

  function savePositions() {
    /** @type {Record<string, [number, number, boolean]>} */
    const saved = {};
    for (const c of containers()) if (c.getId()) saved[c.getId()] = [c._x, c._y, c.getVisible()];
    try {
      localStorage.setItem(positionsKey(), JSON.stringify(saved));
    } catch {}
  }

  /** Back where they were left; the first time, the skin opens where the classic player was. */
  async function placeWindows() {
    /** @type {Record<string, [number, number, boolean]> | null} */
    let saved = null;
    try {
      saved = JSON.parse(localStorage.getItem(positionsKey()) || "null");
    } catch {}
    if (saved) {
      for (const c of containers()) {
        const at = saved[c.getId()];
        if (!at) continue;
        settled.add(c.getId());
        c.setLocation(at[0], at[1]);
        if (at[2] && !c.getVisible()) c.show();
        else if (!at[2] && c.getVisible()) c.hide();
      }
    } else {
      // the playlist open, as Winamp opened it the first time when its
      // playlist was open (pe_open): Spotiamp+'s playlist window was
      const shown = await invoke("get_player_settings")
        .then((/** @type {any} */ p) => p?.show_playlist ?? p?.showPlaylist ?? true)
        .catch(() => true);
      const pl = playlistContainer(uiRoot);
      if (shown && pl && !pl.getVisible()) pl.show();
      // the skin's own spots, moved as one so its main window is where the player was
      const [px, py] = /** @type {[number, number]} */ (await invoke("modern_player_spot").catch(() => [100, 100]));
      const main = containers().find((c) => c.getId() === "main") ?? containers().find((c) => c.getVisible());
      if (main) {
        const dx = px - main._x;
        const dy = py - main._y;
        for (const c of containers()) c.setLocation(c._x + dx, c._y + dy);
      }
      for (const c of containers()) {
        if (c.getVisible() && (c === main || c._placedBySkin)) settled.add(c.getId());
      }
    }
    fitToScreens();
    placeNewcomers();
    rescueOffscreen();
    keepOnScreens();
  }

  // The skin's windows that have their spot: left where they were, or put
  // where the skin says. One the skin gives no spot (Winamp Modern's album
  // art, say, shown by its script after loading) goes under the others the
  // first time it shows, as Winamp puts it, not over the player.
  // A window that shows up right over the main one (Winamp Modern's
  // visualizer, whose spot is the player's own: all of a skin's windows are
  // moved as one to where the classic player was) goes under them too, so
  // the player isn't hidden.
  const settled = new Set();
  /** the skin's windows shown when last looked at */
  const shownNow = new Set();
  /** Does a window lie mostly where the main one is (not a little part of it)? @param {any} c @param {any} main */
  function liesOnMain(c, main) {
    if (!main || c === main) return false;
    const a = rectOf(c);
    const b = rectOf(main);
    if (!a || !b) return false;
    const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
    const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
    if (w <= 0 || h <= 0) return false;
    return w * h >= 0.5 * a.width * a.height && w * h >= 0.5 * b.width * b.height;
  }
  function placeNewcomers() {
    const main = containers().find((o) => o.getId() === "main" && o.getVisible() && rectOf(o));
    for (const c of containers()) {
      const id = c.getId();
      if (!id) continue;
      if (!c.getVisible() || !rectOf(c)) {
        shownNow.delete(id);
        continue;
      }
      const appeared = !shownNow.has(id);
      shownNow.add(id);
      const onMain = appeared && liesOnMain(c, main);
      if (settled.has(id) && !onMain) continue;
      settled.add(id);
      if (id === "main" || (c._placedBySkin && !onMain)) continue;
      const others = containers().filter((o) => o !== c && o.getVisible() && settled.has(o.getId()));
      const anchor = others.find((o) => o.getId() === "main") ?? others[0];
      const bottom = Math.max(...others.map((o) => rectOf(o)?.bottom ?? -Infinity));
      if (anchor && Number.isFinite(bottom)) {
        c.setLocation(anchor._x, Math.round(bottom));
        keepOnScreens([c], true);
      }
    }
  }

  /** Skin windows reaching past the screens (Big Bento opened where the
   *  classic player was, near a screen's edge, or a spot left on a PC with
   *  more screens): each moved onto the screen it's mostly on, as far as it
   *  fits there, its title bar and buttons first. A window's left where it
   *  is when all its corners are on a screen, unless `clear` (one put there
   *  by Spotiamp+: kept clear of the taskbar too). @param {any[]} [list] @param {boolean} [clear] */
  function keepOnScreens(list = containers(), clear = false) {
    if (!screens.length) return;
    const onSome = (/** @type {number} */ x, /** @type {number} */ y) =>
      screens.some((s) => x >= s.x && y >= s.y && x < s.x + s.width && y < s.y + s.height);
    for (const c of list) {
      const r = rectOf(c);
      if (!r) continue;
      if (!clear && onSome(r.left, r.top) && onSome(r.right - 1, r.top) && onSome(r.left, r.bottom - 1) && onSome(r.right - 1, r.bottom - 1)) continue;
      const s = screenAt(r.left + Math.min(r.width / 2, 120), r.top + Math.min(r.height / 2, 12));
      if (!s) continue;
      const wx = s.work_x ?? s.x;
      const wy = s.work_y ?? s.y;
      const ww = s.work_width ?? s.width;
      const wh = s.work_height ?? s.height;
      let dx = 0;
      let dy = 0;
      if (r.right > wx + ww) dx = wx + ww - r.right;
      if (r.left + dx < wx) dx = wx - r.left;
      if (r.bottom > wy + wh) dy = wy + wh - r.bottom;
      if (r.top + dy < wy) dy = wy - r.top;
      if (dx || dy) c.setLocation(Math.round(c._x + dx), Math.round(c._y + dy));
    }
  }

  /** A window wholly off every screen (one taken away, say) comes back onto the first. */
  function rescueOffscreen() {
    const first = screens[0];
    if (!first) return;
    for (const c of containers()) {
      const r = rectOf(c);
      if (!r) continue;
      const seen = screens.some(
        (s) => r.right > s.x + 20 && r.bottom > s.y + 20 && r.left < s.x + s.width - 20 && r.top < s.y + s.height - 20,
      );
      if (!seen) c.setLocation(first.x + 60, first.y + 60);
    }
  }

  // ---- Winamp's Media Library and visualizer, in the skin ----
  // The skin's places for them (windowholders and components, marked by the
  // engine: SkinEngine_WAL.ts): Spotiamp+'s Library and visualizer are laid
  // there (modern_window.rs), as Winamp put its own, and go when they're
  // hidden (the skin's window for them closed, another tab picked).

  /** the parts that are such places @type {any[]} */
  let slotParts = [];
  let slotsScanned = 0;
  function scanSlots() {
    slotParts = [];
    /** @param {any} o */
    const walk = (o) => {
      if (o._spotiampSlot) slotParts.push(o);
      for (const child of o._children ?? []) walk(child);
    };
    for (const c of containers()) for (const layout of c._layouts ?? []) walk(layout);
    slotsScanned = performance.now();
  }
  /** what a skin window is for: Winamp's Media Library ("ml"), its visualizer ("vis"), or neither @param {any} c */
  const windowFor = (c) => (c._componentAlias === "ml" ? "ml" : ["vis", "avs"].includes(c._componentAlias) ? "vis" : "");
  /** the skin window a part is in @param {any} part */
  const windowOf = (part) => {
    let o = part;
    while (o && !o._layouts) o = o._parent;
    return o;
  };

  /** Where each place is, shown, and whether a closed window of the skin's own has one. */
  function slotState() {
    if (!pointerDown && performance.now() - slotsScanned > 2000) scanSlots();
    /** @typedef {[number, number, number, number]} Box */
    /** @type {Record<"ml" | "vis", { rect: Box | null, openable: boolean, inline: Box | null }>} */
    const state = { ml: { rect: null, openable: false, inline: null }, vis: { rect: null, openable: false, inline: null } };
    for (const kind of /** @type {const} */ (["ml", "vis"])) {
      /** @type {Box | null} */
      let best = null;
      /** @type {Box | null} */
      let inline = null;
      let area = 0;
      let inlineArea = 0;
      for (const part of slotParts) {
        if (part._spotiampSlot !== kind) continue;
        const el = part.getDiv();
        const c = windowOf(part);
        if (!c?.getVisible() || !el.isConnected || !el.checkVisibility({ opacityProperty: true, visibilityProperty: true })) continue;
        const r = el.getBoundingClientRect();
        if (r.width < 24 || r.height < 12) continue;
        /** @type {Box} */
        const box = [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)];
        // a visualizer's own window (or a big place) has it whenever it shows;
        // a smaller place in a main window, once the visualizer is on
        const own = kind === "ml" || windowFor(c) === "vis" || r.width * r.height >= 200 * 150;
        if (own && r.width * r.height > area) {
          area = r.width * r.height;
          best = box;
        } else if (!own && r.width * r.height > inlineArea) {
          inlineArea = r.width * r.height;
          inline = box;
        }
      }
      state[kind].rect = best;
      state[kind].inline = inline;
      state[kind].openable =
        !best && slotParts.some((p) => p._spotiampSlot === kind && windowFor(windowOf(p) ?? {}) === kind && !windowOf(p).getVisible());
    }
    return state;
  }

  // told when they change; one at a time, the newest wins (a drag sends many)
  let lastSlots = "";
  let slotsBusy = false;
  let slotsAgain = false;
  function sendSlots() {
    const state = slotState();
    const key = JSON.stringify(state);
    if (key === lastSlots) return;
    if (slotsBusy) {
      slotsAgain = true;
      return;
    }
    lastSlots = key;
    slotsBusy = true;
    invoke("modern_set_slots", { slotsNow: state })
      .catch(() => {})
      .finally(() => {
        slotsBusy = false;
        if (slotsAgain) {
          slotsAgain = false;
          sendSlots();
        }
      });
  }

  /** The skin's own window for the Library or the visualizer, opened (Spotiamp+'s is laid into it) or closed. @param {string} kind @param {boolean} open */
  function showSlotWindow(kind, open) {
    for (const c of containers()) {
      if (windowFor(c) !== kind || c.getVisible() === open) continue;
      if (open) {
        if (!slotParts.some((p) => p._spotiampSlot === kind && windowOf(p) === c)) continue;
        c.show();
        break;
      }
      c.hide();
    }
    HOST.layoutChanged();
  }

  // The window is there only where the skin's windows are: told whenever
  // that changes. Watched every frame while a button is down (a window being
  // dragged), four times a second otherwise.
  let lastRegion = "";
  let lastShape = "";
  let lastSaved = "";
  let pointerDown = false;
  function updateRegion() {
    if (!uiRoot) return;
    if (!pointerDown) placeNewcomers();
    fitToScreens();
    /** @type {[number, number, number, number][]} */
    const rects = [];
    for (const c of containers()) {
      const r = fullRectOf(c, !pointerDown);
      if (r) rects.push([Math.floor(r.left), Math.floor(r.top), Math.ceil(r.width), Math.ceil(r.height)]);
    }
    // the engine's own pop-ups (drop-down lists and the like), and Spotiamp+'s
    // boxes and notes, too
    for (const el of document.querySelectorAll(".popup, .dropdown-list, .sp-dialog, .sp-toast")) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) rects.push([Math.floor(r.left), Math.floor(r.top), Math.ceil(r.width), Math.ceil(r.height)]);
    }
    // where the skin's windows are (and its main one): Spotiamp+'s windows
    // snap to them and open beside the main one, and Lala sits on it
    /** @type {[number, number, number, number][]} */
    const windows = [];
    /** @type {[number, number, number, number] | null} */
    let main = null;
    for (const c of containers()) {
      const r = rectOf(c);
      if (!r) continue;
      /** @type {[number, number, number, number]} */
      const box = [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)];
      windows.push(box);
      if (c.getId() === "main") main = box;
    }
    main ??= windows[0] ?? null;
    const key = JSON.stringify(rects);
    const shape = JSON.stringify([windows, main]);
    if (key !== lastRegion || shape !== lastShape) {
      lastRegion = key;
      lastShape = shape;
      invoke("modern_set_region", { rects, windows, main, moving: pointerDown }).catch(() => {});
    }
    sendSlots();
    if (!pointerDown) {
      const placed = JSON.stringify(containers().map((c) => [c._x, c._y, c.getVisible()]));
      if (placed !== lastSaved) {
        lastSaved = placed;
        savePositions();
      }
    }
  }
  HOST.layoutChanged = () => requestAnimationFrame(updateRegion);

  // ---- what plays, from the hidden classic player ----

  /** Where the song is, counted on between the player's once-a-second word. */
  let positionAt = { ms: 0, at: performance.now(), playing: false };
  /** The EQ as Rust has it (dB), so a slider sends all of it back. */
  let eq = { enabled: true, preamp: 0, bands: Array(10).fill(0) };

  /** @param {any} s the player's skinState */
  function applyState(s) {
    const audio = AUDIO_PLAYER;
    positionAt = { ms: s.positionMs, at: performance.now(), playing: s.state === "playing" };
    audio._setLength((s.durationMs || 0) / 1000);
    audio._setPosition((s.positionMs || 0) / 1000);
    audio._setState(s.state);
    audio._setVolume((s.volume ?? 100) / 100);
    audio._setBalance((s.balance ?? 0) / 100);
    if (uiRoot) {
      const info = { kbps: s.kbps || 0, khz: s.khz || 0, stereo: true };
      const artChanged = (s.art ?? null) !== audio._albumArtUrl;
      audio._albumArtUrl = s.art ?? null;
      const meta = { artist: s.artist ?? "", album: s.album ?? "" };
      const titleChanged =
        s.title !== uiRoot._songTitle || artChanged || meta.artist !== uiRoot._songMeta.artist || meta.album !== uiRoot._songMeta.album;
      const infoChanged = JSON.stringify(info) !== JSON.stringify(uiRoot._songInfo);
      uiRoot._songTitle = s.title;
      uiRoot._songMeta = meta;
      uiRoot._songInfo = info;
      if (titleChanged || infoChanged) uiRoot.playlist.trigger("trackchange");
      // Spotiamp+'s repeat: 0 off, 1 all, 2 the song; Winamp's: 0, 1, -1
      uiRoot.playlist._setShuffleRepeat(!!s.shuffle, s.repeat === 2 ? -1 : s.repeat || 0);
    }
  }

  /** @param {any} v the EQ as Rust has it */
  function applyEq(v) {
    if (!v) return;
    eq = { enabled: !!v.enabled, preamp: v.preamp ?? 0, bands: [...(v.bands ?? eq.bands)] };
    const unit = (/** @type {number} */ db) => Math.min(1, Math.max(0, (db + 12) / 24));
    AUDIO_PLAYER._setEqEnabled(eq.enabled);
    AUDIO_PLAYER._setEq("preamp", unit(eq.preamp));
    eq.bands.forEach((db, i) => AUDIO_PLAYER._setEq(String(i + 1), unit(db)));
  }
  const sendEq = () => invoke("set_eq", { enabled: eq.enabled, preamp: eq.preamp, bands: [...eq.bands] }).catch(() => {});

  // what the skin's buttons, sliders and lists ask
  AUDIO_PLAYER.commands = {
    play: () => emitWindowEvent("skinCommand", { play: true }),
    pause: () => emitWindowEvent("skinCommand", { pause: true }),
    stop: () => emitWindowEvent("skinCommand", { stop: true }),
    seek: (ms) => {
      positionAt = { ...positionAt, ms, at: performance.now() };
      emitWindowEvent("seekTo", { positionMs: Math.round(ms) });
    },
    volume: (v) => emitWindowEvent("skinCommand", { volume: v * 100 }),
    balance: (b) => emitWindowEvent("skinCommand", { balance: b * 100 }),
    eq: (kind, value) => {
      const db = Math.round((value * 24 - 12) * 10) / 10;
      if (kind === "preamp") eq.preamp = db;
      else if (Number(kind) >= 1 && Number(kind) <= 10) eq.bands[Number(kind) - 1] = db;
      sendEq();
    },
    eqEnabled: (on) => {
      eq.enabled = on;
      sendEq();
    },
  };

  HOST.next = () => emitWindowEvent("skinCommand", { next: true });
  HOST.previous = () => emitWindowEvent("skinCommand", { previous: true });
  HOST.eject = () => emitWindowEvent("forwardedKey", { key: "o", shift: false, ctrl: false });
  HOST.menu = (name) => emitWindowEvent("modernMenu", { name });
  HOST.close = () => emitWindowEvent("playerWindow", { CloseRequested: null });
  HOST.minimize = () => getCurrentWindow().minimize();
  HOST.openVisualizer = () => invoke("set_visualizer_window_visible", { visible: true }).catch(() => {});
  HOST.toggleWindow = (id) => {
    if (id === "ml") {
      invoke("set_library_window_visible", { visible: true }).catch(() => {});
      return true;
    }
    if (id === "vis" || id === "avs") {
      HOST.openVisualizer();
      return true;
    }
    return false;
  };

  // ---- the spectrum and waveform, for the skin's visualizers ----

  let freeMode = false;
  async function feedVisualizers() {
    const analyser = AUDIO_PLAYER._analyser;
    if (AUDIO_PLAYER.getState() !== "playing") {
      analyser._frequencies.fill(0);
      analyser._waveform.fill(128);
      return;
    }
    const [spectrum, wave] = await Promise.all([
      invoke(freeMode ? "loopback_spectrum" : "take_latest_spectrum").catch(() => null),
      invoke(freeMode ? "loopback_waveform" : "take_latest_waveform").catch(() => null),
    ]);
    if (Array.isArray(spectrum) && spectrum.length) {
      // the app's bands, by frequency, spread over the analyser's 512 bins
      // (bin i is i x 22050 / 512 Hz, as Web Audio's at 44.1 kHz)
      const pairs = /** @type {[number, number][]} */ (spectrum);
      let p = 0;
      for (let i = 0; i < 512; i++) {
        const hz = (i * 22050) / 512;
        while (p < pairs.length - 1 && pairs[p + 1][0] <= hz) p++;
        analyser._frequencies[i] = Math.round(Math.min(1, Math.max(0, pairs[p][1])) * 255);
      }
    }
    if (wave instanceof ArrayBuffer && wave.byteLength >= 1024) {
      analyser._waveform.set(new Uint8Array(wave, 0, 1024));
      let sum = 0;
      for (let i = 0; i < 1024; i += 4) {
        const v = (analyser._waveform[i] - 128) / 128;
        sum += v * v;
      }
      AUDIO_PLAYER._vuMeter = Math.sqrt(sum / 256);
    }
  }

  // ---- Spotiamp+'s boxes and notes, in the skin's frame ----
  // J (jump to a song), Ctrl+J (to a time), Ctrl+L (open a link) and "Save as
  // a list" show over the playlist in the classic look, which is hidden now:
  // they show over the skin's main window instead, in its frame
  // (modern-look.js), and so do the playlist's little notes.

  /** The skin's look for them (look.js), once made. @type {import("$lib/modern-look.js").Look | null} */
  let look = $state(null);
  /** Until it's made (or for a skin it can't be made for): plain colours. */
  const PLAIN_LOOK = {
    skin: "",
    frames: {},
    client: {},
    vars: "",
    fonts: "",
    colors: {
      windowBg: "#2b3242",
      windowText: "#e6e6e6",
      listBg: "#0d0f14",
      listText: "#cfd6e4",
      current: "#ffffff",
      selBg: "#3a4a6a",
      buttonText: "#e6e6e6",
      headerBg: "#20252f",
      headerText: "#e6e6e6",
      border: "#151820",
      accent: "#3a4458",
    },
  };
  const lookNow = () => look ?? PLAIN_LOOK;

  /** Each box: its title on the frame, what's typed in it, and the size of its insides (1x px). */
  const DIALOGS = {
    jump: { title: "Jump to File", placeholder: "jump to file…", hint: "", w: 300, h: 220 },
    time: { title: "Jump to Time", placeholder: "jump to time, like 1:23", hint: "", w: 300, h: 46 },
    url: {
      title: "Open Location",
      placeholder: "a Spotify link or a radio station",
      hint: "A song, album or playlist from Spotify, or a station's stream (http://…, .pls, .m3u).",
      w: 300,
      h: 58,
    },
    savelist: {
      title: "Save as a List",
      placeholder: "name for this list…",
      hint: "Enter saves it, Esc cancels. Lists live in the Library under Spotiamp+.",
      w: 300,
      h: 58,
    },
  };
  /** @typedef {keyof typeof DIALOGS} DialogKind */

  /** @type {{ kind: DialogKind, x: number, y: number, scale: number, hint: string } | null} */
  let dialog = $state(null);
  let dialogText = $state("");
  let dialogActive = $state(0);
  /** the playlist's songs, for the jump box @type {{ title: string, durationMs: number }[]} */
  let listRows = $state([]);
  let listCurrent = $state(-1);
  /** @type {HTMLInputElement | undefined} */
  let dialogInput = $state();
  /** @type {HTMLDivElement | undefined} */
  let dialogFrame = $state();
  /** @type {HTMLDivElement | undefined} */
  let dialogList = $state();

  /** Where to put something over the skin's main window: its box, its screen and scale. */
  function overMain() {
    const main = containers().find((c) => c.getId() === "main" && c.getVisible()) ?? containers().find((c) => c.getVisible());
    const box = main ? rectOf(main) : null;
    const s = screenAt(box ? box.left + 8 : 100, box ? box.top + 8 : 100);
    return { box, screen: /** @type {any} */ (s), scale: (s?.scale ?? 1) * uiScale };
  }

  /** @param {number} ms */
  const clock = (ms) => {
    const s = Math.floor(Math.max(0, ms) / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };

  /** @param {string} kind */
  function openDialog(kind) {
    if (!uiRoot || !(kind in DIALOGS)) return;
    const spec = DIALOGS[/** @type {DialogKind} */ (kind)];
    const ins = frameInsets(lookNow(), "dialog", spec.title);
    const { box, screen, scale } = overMain();
    const w = (spec.w + ins.left + ins.right) * scale;
    const h = (spec.h + ins.top + ins.bottom) * scale;
    let x = box ? box.left + box.width / 2 - w / 2 : (screen?.x ?? 0) + 100;
    let y = box ? box.top + box.height / 2 - h / 2 : (screen?.y ?? 0) + 100;
    if (screen) {
      const left = screen.work_x ?? screen.x;
      const top = screen.work_y ?? screen.y;
      x = Math.min(Math.max(x, left), left + (screen.work_width ?? screen.width) - w);
      y = Math.min(Math.max(y, top), top + (screen.work_height ?? screen.height) - h);
    }
    const now = positionAt.ms + (positionAt.playing ? performance.now() - positionAt.at : 0);
    const hint = kind === "time" ? `Now at ${clock(now)}. Enter jumps there, Esc cancels.` : spec.hint;
    dialogText = "";
    dialogActive = 0;
    dialog = { kind: /** @type {DialogKind} */ (kind), x: Math.round(x), y: Math.round(y), scale, hint };
    if (kind === "jump") emitWindowEvent("skinPlaylistCommand", { loadNames: true });
    getCurrentWindow().setFocus().catch(() => {});
    tick().then(() => {
      dialogInput?.focus();
      updateRegion();
    });
  }

  function closeDialog() {
    dialog = null;
    tick().then(updateRegion);
  }

  /** @param {string} s */
  const fold = (s) =>
    (s || "")
      .toLowerCase()
      .normalize("NFD")
      .replace(/\p{M}/gu, "")
      .replace(/ı/g, "i");
  const jumpMatches = $derived.by(() => {
    if (dialog?.kind !== "jump") return [];
    const q = fold(dialogText.trim());
    const out = [];
    for (let i = 0; i < listRows.length && out.length < 200; i++) {
      if (!q || fold(listRows[i].title).includes(q)) out.push({ i, title: listRows[i].title });
    }
    return out;
  });
  // a new search starts from the top match
  $effect(() => {
    dialogText;
    dialogActive = 0;
  });

  function submitDialog() {
    const kind = dialog?.kind;
    const text = dialogText.trim();
    if (kind === "jump") {
      const hit = jumpMatches[dialogActive];
      if (!hit) return;
      emitWindowEvent("skinPlaylistCommand", { play: hit.i });
    } else if (kind === "time") {
      // 1:23, 83 (seconds) or 1:02:03
      const parts = text.split(":").map((p) => p.trim());
      if (!parts[0] || parts.length > 3 || parts.some((p) => !/^\d+$/.test(p))) return;
      const seconds = parts.reduce((total, p) => total * 60 + Number(p), 0);
      emitWindowEvent("seekTo", { positionMs: seconds * 1000 });
    } else if (kind === "url") {
      if (!text) return;
      emitWindowEvent("skinPlaylistCommand", { openUrl: text });
    } else if (kind === "savelist") {
      if (!text) return;
      emitWindowEvent("skinPlaylistCommand", { saveList: text });
    }
    closeDialog();
  }

  /** @param {KeyboardEvent} e */
  function onDialogKey(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeDialog();
    } else if (e.key === "Enter") {
      e.preventDefault();
      submitDialog();
    } else if (dialog?.kind === "jump" && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      dialogActive = Math.max(0, Math.min(jumpMatches.length - 1, dialogActive + (e.key === "ArrowDown" ? 1 : -1)));
      tick().then(() => dialogList?.querySelector(".active")?.scrollIntoView({ block: "nearest" }));
    }
  }

  // the box's frame: the skin's (a dialog's), its close button closing it
  $effect(() => {
    if (!dialog || !dialogFrame) return;
    paintFrame(dialogFrame, lookNow(), "dialog", DIALOGS[dialog.kind].title);
    setFrameActive(dialogFrame, true);
    return wireFrame(dialogFrame, { onAction: (action) => action === "CLOSE" && closeDialog() });
  });

  /** Moved by its frame, as a window is. @param {PointerEvent} e */
  function dragDialog(e) {
    if (e.button !== 0 || !dialog) return;
    const el = /** @type {HTMLElement} */ (e.currentTarget);
    const start = { x: e.clientX, y: e.clientY, left: dialog.x, top: dialog.y };
    el.setPointerCapture(e.pointerId);
    el.onpointermove = (m) => {
      if (!dialog) return;
      dialog.x = Math.round(start.left + m.clientX - start.x);
      dialog.y = Math.round(start.top + m.clientY - start.y);
    };
    el.onpointerup = () => {
      el.onpointermove = null;
      el.onpointerup = null;
    };
  }

  /** A note under the skin's main window (over it when there's no room below). @type {{ text: string, x: number, y: number, scale: number } | null} */
  let toast = $state(null);
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let toastTimer;
  /** @param {string} text */
  function showToast(text) {
    if (!text || !uiRoot) return;
    const { box, screen, scale } = overMain();
    const x = box ? box.left + box.width / 2 : (screen?.x ?? 0) + 200;
    let y = box ? box.bottom + 4 : (screen?.y ?? 0) + 100;
    if (screen && y + 24 * scale > (screen.work_y ?? screen.y) + (screen.work_height ?? screen.height)) {
      y = (box?.top ?? y) - 4 - 22 * scale;
    }
    toast = { text, x: Math.round(x), y: Math.round(y), scale };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = null;
      tick().then(updateRegion);
    }, 2600);
    tick().then(updateRegion);
  }

  /**
   * The Now Playing card (made by the hidden playlist, as a PNG in base64) to
   * the clipboard, which takes it only from a page that has the focus.
   * @param {string} png
   */
  async function copyCard(png) {
    await getCurrentWindow().setFocus().catch(() => {});
    window.focus();
    try {
      const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
      await navigator.clipboard.write([new ClipboardItem({ "image/png": new Blob([bytes], { type: "image/png" }) })]);
      showToast("Now Playing card copied, paste it anywhere");
    } catch (e) {
      showToast("Couldn't copy the card");
      invoke("log_frontend_error", { window: "modern", message: `now playing card: ${e}` }).catch(() => {});
    }
  }

  // ---- loading the skin ----

  /** Couldn't be shown: back to the classic look, saying so. @param {unknown} why */
  async function fail(why) {
    const text = String(why instanceof Error ? why.message : why).slice(0, 300);
    await invoke("leave_modern_skin", { failed: `${skinName}: ${text}` }).catch(() => {});
    message(
      `Spotiamp+ couldn't show the modern skin "${skinName.replace(/\.(wal|zip)$/i, "")}", so it's back to the classic look.\n\n` +
        "Some modern skins use parts of Winamp that Spotiamp+ doesn't have yet.",
      { title: "Spotiamp+", kind: "warning" },
    ).catch(() => {});
  }

  async function loadSkin() {
    const worn = /** @type {{ name: string } | null} */ (await invoke("modern_skin"));
    if (!worn) throw new Error("no modern skin is worn");
    skinName = worn.name;
    const bytes = /** @type {ArrayBuffer} */ (await invoke("modern_skin_file"));
    const ui = new UIRoot("ui-root-1");
    ui._idleTitle = `Spotiamp+ ${await getVersion().catch(() => "")}`.trim();
    root.appendChild(ui.getRootDiv());
    const extractor = new ZipFileExtractor();
    await extractor.prepare(skinName, new Response(new Blob([bytes])));
    ui.setFileExtractor(/** @type {any} */ (extractor));
    ui.SkinEngineClass = /** @type {any} */ (SkinEngineWAL);
    uiRoot = ui;
    // the engine works from these; the page has no others (see +page.svelte's head)
    engine = new SkinEngineWAL(ui);
    await engine.buildUI();
    // what the skin's playlist and its shuffle and repeat ask of Spotiamp+'s
    // (only now: a skin's stored shuffle or repeat, read while it loads,
    // isn't sent; the player's own come in instead)
    ui.playlist.commands = {
      play: (index) => emitWindowEvent("skinPlaylistCommand", { play: index }),
      remove: (index) => emitWindowEvent("skinPlaylistCommand", { remove: index }),
      move: (from, to) => emitWindowEvent("skinPlaylistCommand", { move: [from, to] }),
      shuffle: (on) => emitWindowEvent("skinCommand", { shuffle: on }),
      // Winamp's repeat: 0, 1 all, -1 the song; Spotiamp+'s: 0, 1, 2
      repeat: (mode) => emitWindowEvent("skinCommand", { repeat: mode === -1 ? 2 : mode ? 1 : 0 }),
      select: (rows) => emitWindowEvent("skinPlaylistCommand", { select: rows }),
    };
    // the playlist's window: the skin's, or Winamp's own in the skin's frame
    await ensurePlaylistWindow(ui, engine).catch((e) => console.warn("modern skin: no playlist window:", e));
    // the skin's pictures in their colours, before the window shows
    await ui._cssVarsRunning;
    if (!containers().some((c) => c.getVisible())) throw new Error("the skin shows no window");
  }

  onMount(() => {
    /** @type {(() => void)[]} */
    const cleanups = [];
    let stopped = false;
    (async () => {
      freeMode = /** @type {boolean} */ (await invoke("is_controller_mode").catch(() => false));
      uiScale = /** @type {number} */ (await invoke("get_ui_scale").catch(() => 1)) || 1;
      setScreens(await invoke("modern_screens").catch(() => []));
      try {
        // a skin that hangs is as good as one that fails
        await Promise.race([
          loadSkin(),
          new Promise((_, reject) => setTimeout(() => reject(new Error("it took too long to load")), 20000)),
        ]);
        await placeWindows();
        updateRegion();
        if (!JSON.parse(lastRegion || "[]").length) throw new Error("none of its windows is on a screen");
      } catch (e) {
        console.error("modern skin:", e);
        fail(e);
        return;
      }
      if (stopped) return;
      invoke("modern_ready").catch(() => {});
      // for the app's own tests (.smoke/t-modern.mjs)
      /** @type {any} */ (window).__modern = { uiRoot, audio: AUDIO_PLAYER, region: () => lastRegion, skin: skinName, look: () => look, slots: slotState };
      scanSlots();
      // the Library or the visualizer opened (or closed) while the skin has a
      // window of its own for it
      const openSlot = getCurrentWindow().listen("modernOpenSlot", (e) => showSlotWindow(String(e.payload), true));
      const closeSlot = getCurrentWindow().listen("modernCloseSlot", (e) => showSlotWindow(String(e.payload), false));
      // one of them opened: where the places are, said again
      const slotsAgain = getCurrentWindow().listen("modernSlotsAgain", () => {
        lastSlots = "";
        sendSlots();
      });
      cleanups.push(() => openSlot.then((u) => u()), () => closeSlot.then((u) => u()), () => slotsAgain.then((u) => u()));
      // the skin's frame and colours for Spotiamp+'s own windows (look.js)
      buildLook(uiRoot, engine, skinName)
        .then((made) => {
          if (stopped) return;
          look = made;
          return invoke("modern_set_look", { look: made });
        })
        .catch((e) => console.warn("modern skin: no look for the other windows:", e));

      // what plays, the playlist and the EQ: asked for once, then as they change
      const state = subscribeToWindowEvent("skinState", applyState);
      const rows = subscribeToWindowEvent("skinPlaylist", (p) => {
        uiRoot?.playlist._setRows(p.rows, p.current);
        listRows = p.rows;
        listCurrent = p.current;
      });
      // the hidden playlist's boxes and notes, shown here
      const dialogs = subscribeToWindowEvent("modernDialog", (d) => {
        if (d.kind === "toast") showToast(d.text ?? "");
        else if (d.kind === "card") copyCard(d.text ?? "");
        else openDialog(d.kind);
      });
      const eqChanged = listen("eqChanged", (e) => applyEq(e.payload));
      cleanups.push(
        () => state.then((u) => u()),
        () => rows.then((u) => u()),
        () => dialogs.then((u) => u()),
        () => eqChanged.then((u) => u()),
      );
      emitWindowEvent("skinPlaylistCommand", { ask: true });
      invoke("get_eq").then(applyEq).catch(() => {});

      // the time counts on smoothly between the player's ticks
      const clock = setInterval(() => {
        if (positionAt.playing) AUDIO_PLAYER._setPosition((positionAt.ms + performance.now() - positionAt.at) / 1000);
      }, 250);
      // the visualizers, about 30 times a second while something plays
      let feeding = false;
      const vis = setInterval(() => {
        if (feeding) return;
        feeding = true;
        feedVisualizers().finally(() => (feeding = false));
      }, 33);
      const region = setInterval(() => !pointerDown && updateRegion(), 250);
      cleanups.push(() => clearInterval(clock), () => clearInterval(vis), () => clearInterval(region));

      // a classic skin put on: the classic look comes back
      const skinChanged = subscribeToWindowEvent("skinChanged", () => {
        invoke("leave_modern_skin", { failed: null }).catch(() => {});
      });
      // a screen added, taken away or rescaled
      const moved = getCurrentWindow().onScaleChanged(async () => {
        setScreens(await invoke("modern_screens").catch(() => screens));
        fitToScreens();
        rescueOffscreen();
        keepOnScreens();
        updateRegion();
      });
      // Spotiamp+'s Scale changed (the menu, Ctrl+D)
      const scaled = listen("uiScale", (e) => {
        uiScale = Number(/** @type {any} */ (e.payload)?.scale) || 1;
        fitToScreens();
        keepOnScreens();
        updateRegion();
      });
      cleanups.push(() => skinChanged.then((u) => u()), () => moved.then((u) => u()), () => scaled.then((u) => u()));
    })();

    // dragging a window: the region follows it every frame
    const down = (/** @type {PointerEvent} */ e) => {
      if (dialog && !(e.target instanceof Element && e.target.closest(".sp-dialog"))) closeDialog();
      pointerDown = true;
      const follow = () => {
        updateRegion();
        if (pointerDown) requestAnimationFrame(follow);
      };
      requestAnimationFrame(follow);
    };
    const up = () => {
      pointerDown = false;
      requestAnimationFrame(updateRegion);
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointerup", up, true);
    const blur = () => dialog && closeDialog();
    window.addEventListener("blur", blur);
    // right-click: Spotiamp+'s menu, as on the classic player
    const context = (/** @type {MouseEvent} */ e) => {
      e.preventDefault();
      emitWindowEvent("modernMenu", { name: "context" });
    };
    window.addEventListener("contextmenu", context);
    // Alt+E / Alt+G: the skin's own playlist and equalizer windows
    const altKeys = (/** @type {KeyboardEvent} */ e) => {
      if (!e.altKey || e.ctrlKey || e.metaKey || !uiRoot) return;
      const k = e.key.toLowerCase();
      if (k !== "e" && k !== "g") return;
      e.preventDefault();
      e.stopImmediatePropagation();
      const target = k === "e" ? ["guid:pl", "pl"] : ["eq", "guid:eq"];
      const c = target.map((id) => uiRoot?.findContainer(id)).find(Boolean);
      if (c) {
        c.toggle();
        HOST.layoutChanged();
      }
    };
    window.addEventListener("keydown", altKeys, true);
    // the main window's keys (Z X C V B, the arrows...) work here as on the classic player
    const stopForwarding = forwardShortcuts();
    // Spotify links and music files dropped on it play, as on the classic player
    const stopDrop = handleDrop((urls) => emitWindowEvent("playerWindow", { UrlsDropped: urls }));
    const offDropped = getCurrentWindow().listen("filesDropped", (e) => {
      const paths = /** @type {string[]} */ (e.payload);
      if (paths?.length) emitWindowEvent("playerWindow", { AddLocalFiles: paths });
    });

    return () => {
      stopped = true;
      for (const f of cleanups) f();
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("pointerup", up, true);
      window.removeEventListener("blur", blur);
      clearTimeout(toastTimer);
      window.removeEventListener("contextmenu", context);
      window.removeEventListener("keydown", altKeys, true);
      stopForwarding();
      stopDrop();
      offDropped.then((f) => f()).catch(() => {});
    };
  });
</script>

<svelte:head>
  <!-- the engine writes the skin's bitmaps and fonts into these -->
  <style id="bitmap-css"></style>
  <style id="truetypefont-css"></style>
</svelte:head>

<div class="modern-root" bind:this={root}></div>

{#if dialog}
  {@const spec = DIALOGS[dialog.kind]}
  {@const ins = frameInsets(lookNow(), "dialog", spec.title)}
  {@const c = lookNow().colors}
  <div
    class="sp-dialog"
    role="dialog"
    aria-label={spec.title}
    style:left="{dialog.x}px"
    style:top="{dialog.y}px"
    style:width="{spec.w + ins.left + ins.right}px"
    style:height="{spec.h + ins.top + ins.bottom}px"
    style:transform="scale({dialog.scale})"
    style:--d-bg={c.listBg}
    style:--d-fg={c.listText}
    style:--d-sel={c.selBg}
    style:--d-cur={c.current}
    style:--d-win={c.windowBg}
    style:--d-text={c.windowText}
    style:--d-border={c.border}
  >
    <div class="sp-dialog-frame" role="presentation" bind:this={dialogFrame} onpointerdown={dragDialog}></div>
    <div
      class="sp-dialog-body"
      style:left="{ins.left}px"
      style:top="{ins.top}px"
      style:right="{ins.right}px"
      style:bottom="{ins.bottom}px"
    >
      <input
        class="sp-dialog-input"
        bind:this={dialogInput}
        bind:value={dialogText}
        onkeydown={onDialogKey}
        placeholder={spec.placeholder}
        spellcheck="false"
        aria-label={spec.title}
      />
      {#if dialog.kind === "jump"}
        <div class="sp-dialog-list" role="listbox" bind:this={dialogList}>
          {#each jumpMatches as hit, n (hit.i)}
            <div
              class="sp-dialog-item"
              class:active={n === dialogActive}
              class:current={hit.i === listCurrent}
              role="option"
              aria-selected={n === dialogActive}
              tabindex="-1"
              onmousedown={(e) => {
                // keep the typing in the box
                e.preventDefault();
                dialogActive = n;
                submitDialog();
              }}
            >
              {hit.i + 1}. {hit.title}
            </div>
          {:else}
            <div class="sp-dialog-hint">{listRows.some((r) => !r.title) ? "loading names…" : "no match"}</div>
          {/each}
        </div>
      {:else}
        <div class="sp-dialog-hint">{dialog.hint}</div>
      {/if}
    </div>
  </div>
{/if}

{#if toast}
  {@const c = lookNow().colors}
  <div
    class="sp-toast"
    role="status"
    style:left="{toast.x}px"
    style:top="{toast.y}px"
    style:transform="translateX(-50%) scale({toast.scale})"
    style:background={c.listBg}
    style:color={c.listText}
    style:border-color={c.border}
  >
    {toast.text}
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent !important;
    overflow: hidden;
    width: 100%;
    height: 100%;
  }
  .modern-root {
    position: absolute;
    inset: 0;
  }
  /* the classic skin's cursor (global.css) isn't a modern skin's: an arrow,
     unless the skin names its own */
  :global(body *) {
    cursor: default;
  }

  /* Spotiamp+'s boxes in the skin's frame, and its notes */
  .sp-dialog {
    position: absolute;
    transform-origin: 0 0;
    z-index: 10;
    font: 11px Arial, sans-serif;
  }
  .sp-dialog-frame {
    position: absolute;
    inset: 0;
  }
  .sp-dialog-body {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px;
    box-sizing: border-box;
    background: var(--d-win);
    color: var(--d-text);
  }
  .sp-dialog-input {
    flex: none;
    height: 18px;
    padding: 0 4px;
    box-sizing: border-box;
    border: 1px solid var(--d-border);
    background: var(--d-bg);
    color: var(--d-fg);
    font: 11px Arial, sans-serif;
    user-select: text;
    cursor: text;
  }
  .sp-dialog-input::placeholder {
    color: color-mix(in srgb, var(--d-fg) 50%, transparent);
  }
  .sp-dialog-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--d-border);
    background: var(--d-bg);
    color: var(--d-fg);
  }
  .sp-dialog-item {
    padding: 0 4px;
    line-height: 15px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sp-dialog-item.current {
    color: var(--d-cur);
  }
  .sp-dialog-item.active {
    background: var(--d-sel);
  }
  .sp-dialog-hint {
    font-size: 10px;
    opacity: 0.85;
    line-height: 13px;
  }
  .sp-dialog-list .sp-dialog-hint {
    padding: 2px 4px;
    font-style: italic;
  }
  .sp-toast {
    position: absolute;
    transform-origin: 50% 0;
    z-index: 11;
    max-width: 420px;
    padding: 3px 8px;
    border: 1px solid;
    font: 11px Arial, sans-serif;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
  }
</style>
