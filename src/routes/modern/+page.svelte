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
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { message } from "@tauri-apps/plugin-dialog";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";
  import { handleDrop } from "$lib/common.svelte.js";
  import { onMount } from "svelte";

  /** @type {HTMLDivElement} */
  let root;
  /** @type {UIRoot | null} */
  let uiRoot = null;
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
   *  classic windows are, times Spotiamp+'s Scale. */
  function fitToScreens() {
    for (const c of containers()) {
      const s = screenAt(c._x + 8, c._y + 8);
      const scale = (s?.scale ?? 1) * uiScale;
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
  }

  // The skin's windows that have their spot: left where they were, or put
  // where the skin says. One the skin gives no spot (Winamp Modern's album
  // art, say, shown by its script after loading) goes under the others the
  // first time it shows, as Winamp puts it, not over the player.
  const settled = new Set();
  function placeNewcomers() {
    for (const c of containers()) {
      const id = c.getId();
      if (!id || settled.has(id) || !c.getVisible() || !rectOf(c)) continue;
      settled.add(id);
      if (c._placedBySkin || id === "main") continue;
      const others = containers().filter((o) => o !== c && o.getVisible() && settled.has(o.getId()));
      const anchor = others.find((o) => o.getId() === "main") ?? others[0];
      const bottom = Math.max(...others.map((o) => rectOf(o)?.bottom ?? -Infinity));
      if (anchor && Number.isFinite(bottom)) c.setLocation(anchor._x, Math.round(bottom));
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

  // The window is there only where the skin's windows are: told whenever
  // that changes. Watched every frame while a button is down (a window being
  // dragged), four times a second otherwise.
  let lastRegion = "";
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
    // the engine's own pop-ups (drop-down lists and the like) too
    for (const el of document.querySelectorAll(".popup, .dropdown-list")) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) rects.push([Math.floor(r.left), Math.floor(r.top), Math.ceil(r.width), Math.ceil(r.height)]);
    }
    const key = JSON.stringify(rects);
    if (key !== lastRegion) {
      lastRegion = key;
      invoke("modern_set_region", { rects }).catch(() => {});
    }
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
      const titleChanged = s.title !== uiRoot._songTitle || artChanged;
      const infoChanged = JSON.stringify(info) !== JSON.stringify(uiRoot._songInfo);
      uiRoot._songTitle = s.title;
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
    root.appendChild(ui.getRootDiv());
    const extractor = new ZipFileExtractor();
    await extractor.prepare(skinName, new Response(new Blob([bytes])));
    ui.setFileExtractor(/** @type {any} */ (extractor));
    ui.SkinEngineClass = /** @type {any} */ (SkinEngineWAL);
    uiRoot = ui;
    // the engine works from these; the page has no others (see +page.svelte's head)
    await new SkinEngineWAL(ui).buildUI();
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
      /** @type {any} */ (window).__modern = { uiRoot, audio: AUDIO_PLAYER, region: () => lastRegion, skin: skinName };

      // what plays, the playlist and the EQ: asked for once, then as they change
      const state = subscribeToWindowEvent("skinState", applyState);
      const rows = subscribeToWindowEvent("skinPlaylist", (p) => uiRoot?.playlist._setRows(p.rows, p.current));
      const eqChanged = listen("eqChanged", (e) => applyEq(e.payload));
      cleanups.push(
        () => state.then((u) => u()),
        () => rows.then((u) => u()),
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
        rescueOffscreen();
        updateRegion();
      });
      // Spotiamp+'s Scale changed (the menu, Ctrl+D)
      const scaled = listen("uiScale", (e) => {
        uiScale = Number(/** @type {any} */ (e.payload)?.scale) || 1;
        updateRegion();
      });
      cleanups.push(() => skinChanged.then((u) => u()), () => moved.then((u) => u()), () => scaled.then((u) => u()));
    })();

    // dragging a window: the region follows it every frame
    const down = () => {
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
</style>
