<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE, SKIN_APPLIED } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";
  import { watchWindowActive } from "$lib/window-active.js";

  // The main window's keys work here too (lib/shortcuts.js).
  onMount(() => forwardShortcuts());

  // lit while this window is in front, dim otherwise (lib/window-active.js)
  let windowActive = $state(true);
  onMount(() => watchWindowActive((active) => (windowActive = active)));

  // Classic Winamp presets, dB per band (60,170,310,600,1k,3k,6k,12k,14k,16k)
  const PRESETS = {
    Flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    Rock: [8, 5, -3, -6, -2, 2, 5, 8, 8, 8],
    Pop: [-2, 4, 6, 6, 3, 0, -2, -2, -1, -1],
    Jazz: [4, 3, 1, 2, -2, -2, 0, 1, 3, 4],
    Classical: [0, 0, 0, 0, 0, 0, -5, -6, -6, -8],
    "Full Bass": [10, 9, 7, 4, 1, 0, 0, 0, 0, 0],
    "Full Treble": [0, 0, 0, 0, 0, 2, 5, 8, 10, 10],
    Dance: [9, 7, 2, 0, 0, -3, -4, -4, 0, 0],
    Vocal: [-2, -3, -3, 1, 4, 4, 3, 1, 0, -2],
    Live: [-3, 0, 2, 3, 3, 3, 2, 1, 1, 1],
  };

  let enabled = $state(true);
  let preamp = $state(0);
  let bands = $state([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
  let menuOpen = $state(false);
  let activeFader = $state(-99); // which fader is being dragged (-1 = preamp)
  // AUTO (Winamp's auto-load presets): a song's or artist's own curve loads
  // when it starts. The rest mirrors what Rust says (eq_window.rs).
  let auto = $state(false);
  let applied = $state(/** @type {"song" | "artist" | null} */ (null));
  let canSong = $state(false);
  let canArtist = $state(false);
  let hasSongPreset = $state(false);
  let hasArtistPreset = $state(false);
  // the saved EQ comes in first; pushing the flat start-up values before it
  // did would have overwritten it
  let loaded = $state(false);

  // What Rust last said the EQ is. Showing it mustn't send it back: arriving
  // late, that echo overwrote a newer change (a preset saved just after).
  let fromRust = "";
  const eqKey = () => JSON.stringify([enabled, preamp, bands]);

  /** @param {any} v the EQ as Rust has it */
  function applyView(v) {
    fromRust = JSON.stringify([v.enabled, v.preamp, v.bands]);
    enabled = v.enabled;
    preamp = v.preamp;
    bands = [...v.bands];
    auto = v.auto;
    applied = v.applied;
    canSong = v.song;
    canArtist = v.artist;
    hasSongPreset = v.has_song_preset;
    hasArtistPreset = v.has_artist_preset;
    loaded = true;
  }
  onMount(() => {
    invoke("get_eq").then(applyView).catch(() => (loaded = true));
    const changed = listen("eqChanged", (e) => applyView(e.payload));
    return () => changed.then((unlisten) => unlisten());
  });
  /** @param {"song" | "artist"} scope */
  function autoSave(scope) {
    menuOpen = false;
    invoke("eq_auto_save", { scope }).catch(() => {});
  }
  /** @param {"song" | "artist"} scope */
  function autoForget(scope) {
    menuOpen = false;
    invoke("eq_auto_delete", { scope }).catch(() => {});
  }

  // window-space geometry (px), Winamp's: band faders every 18px from x=78,
  // each 63px tall from y=38, its thumb 11px with 52px to travel
  const BAND_X = [78, 96, 114, 132, 150, 168, 186, 204, 222, 240];
  const WELL_H = 63;
  const THUMB_H = 11;
  const TRAVEL = 52;

  // dB (+12..-12) as Winamp's slider position: 0 at the top (+12 dB), 63 at
  // the bottom, its flat 31 (Winamp's dB is (31.5 - pos) / 31.5 * 12)
  /** @param {number} db */
  function winampPos(db) {
    return Math.max(0, Math.min(63, Math.round(31.5 - (db * 31.5) / 12 - 1e-6)));
  }
  // the fader's background, one of EQMAIN's 28 frames: 27 (red) at the top,
  // 0 (green) at the bottom
  /** @param {number} db */
  function frameBg(db) {
    const n = 27 - Math.floor((winampPos(db) * 28) / 64);
    const x = -(13 + (n % 14) * 15);
    const y = -(n < 14 ? 164 : 229);
    return `${x}px ${y}px`;
  }
  // the thumb, from the fader's top
  /** @param {number} db */
  function thumbTop(db) {
    return WELL_H - 12 - Math.floor(((63 - winampPos(db)) * TRAVEL) / 64);
  }

  // --- the graph (the little curve in the top box), as Winamp draws it ---
  // From the skin's EQMAIN.BMP: the graph's background, the preamp line at
  // the preamp's height and the bands' curve through them 12 px apart, each
  // pixel row in the skin's own line colour for it. What the bitmap hasn't
  // got stays as the window has it there (Winamp copies nothing for it).
  const GRAPH_W = 113;
  const GRAPH_H = 19;
  /** @type {HTMLCanvasElement | undefined} */
  let graphCanvas = $state();
  /** @type {HTMLDivElement | undefined} */
  let bgElement = $state();
  /** @type {{ bg: HTMLCanvasElement, preampLine: HTMLCanvasElement, colors: HTMLCanvasElement } | null} */
  let graphSkin = $state(null);

  async function loadGraphSkin() {
    // the window's background as drawn, its address resolved (a built-in
    // skin's is relative to the stylesheet)
    const value = bgElement ? getComputedStyle(bgElement).backgroundImage : "";
    const url = value.match(/^url\(["']?(.*?)["']?\)$/)?.[1];
    if (!url) return (graphSkin = null);
    const img = new Image();
    img.src = url;
    try {
      await img.decode();
    } catch {
      return (graphSkin = null);
    }
    /** what the bitmap has of x,y w*h (the rest transparent) @param {number} x @param {number} y @param {number} w @param {number} h */
    const part = (x, y, w, h) => {
      const c = document.createElement("canvas");
      c.width = w;
      c.height = h;
      const cw = Math.min(w, img.width - x);
      const ch = Math.min(h, img.height - y);
      if (cw > 0 && ch > 0) /** @type {CanvasRenderingContext2D} */ (c.getContext("2d")).drawImage(img, x, y, cw, ch, 0, 0, cw, ch);
      return c;
    };
    graphSkin = { bg: part(0, 294, GRAPH_W, GRAPH_H), preampLine: part(0, 314, GRAPH_W, 1), colors: part(115, 294, 1, GRAPH_H) };
  }

  /**
   * The curve's height at `frame` (1 = the first band, 10 the last) through
   * `keys` (a key before and after the bands, copies of the end ones): a
   * cubic Hermite between neighbouring keys, its slopes 0.45 times the
   * difference of the keys on either side (a Kochanek-Bartels spline of
   * tension 0.1, as Winamp's).
   * @param {number[]} keys @param {number} frame
   */
  function curveAt(keys, frame) {
    const i = Math.floor(frame);
    const at = (/** @type {number} */ k) => keys[(k + keys.length) % keys.length];
    const p0 = at(i), p1 = at(i + 1);
    const m0 = 0.45 * (p1 - at(i - 1));
    const m1 = 0.45 * (at(i + 2) - p0);
    const s = frame - i, s2 = s * s, s3 = s2 * s;
    return (2 * s3 - 3 * s2 + 1) * p0 + (s3 - 2 * s2 + s) * m0 + (-2 * s3 + 3 * s2) * p1 + (s3 - s2) * m1;
  }

  $effect(() => {
    const canvas = graphCanvas;
    if (!canvas) return;
    const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
    ctx.clearRect(0, 0, GRAPH_W, GRAPH_H);
    const skin = graphSkin;
    // (read before any return, so the effect follows them)
    const pre = winampPos(preamp);
    const keys = bands.map((db) => (winampPos(db) * 19) / 64);
    if (!skin) return;
    ctx.drawImage(skin.bg, 0, 0);
    ctx.drawImage(skin.preampLine, 0, GRAPH_H - 1 - Math.trunc((pre * 19) / 64));
    keys.unshift(keys[0]);
    keys.push(keys[keys.length - 1]);
    ctx.fillStyle = /** @type {CanvasPattern} */ (ctx.createPattern(skin.colors, "repeat-x"));
    let last = -1;
    for (let x = 0; x < 109; x++) {
      const y = Math.max(0, Math.min(GRAPH_H - 1, Math.trunc(curveAt(keys, 1 + x / 12))));
      const top = last === -1 ? y : Math.min(y, last);
      ctx.fillRect(2 + x, top, 1, 1 + (last === -1 ? 0 : Math.abs(last - y)));
      last = y;
    }
  });

  function push() {
    invoke("set_eq", { enabled, preamp, bands: [...bands] }).catch(() => {});
  }
  $effect(() => {
    const key = eqKey();
    if (loaded && key !== fromRust) {
      fromRust = key; // Rust has this now
      push();
    }
  });

  /**
   * @param {number} clientY
   * @param {DOMRect} rect
   * @param {number} fader a band, or -1 for the preamp
   */
  function setFromY(clientY, rect, fader) {
    const relPx = ((clientY - rect.top) / rect.height) * WELL_H;
    const f = Math.max(0, Math.min(1, (relPx - THUMB_H / 2) / TRAVEL));
    let db = Math.round(12 - f * 24);
    db = Math.max(-12, Math.min(12, db));
    if (fader === -1) preamp = db;
    else bands[fader] = db;
  }
  /**
   * @param {PointerEvent & { currentTarget: HTMLElement }} e
   * @param {number} fader
   */
  function beginDrag(e, fader) {
    e.preventDefault();
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    activeFader = fader;
    setFromY(e.clientY, rect, fader);
    const move = (/** @type {PointerEvent} */ ev) => setFromY(ev.clientY, rect, fader);
    const up = () => {
      activeFader = -99;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
  /** @param {number} fader */
  function resetFader(fader) {
    if (fader === -1) preamp = 0;
    else bands[fader] = 0;
  }
  /**
   * @param {KeyboardEvent} e
   * @param {number} fader
   */
  function nudge(e, fader) {
    let d = 0;
    if (e.key === "ArrowUp") d = 1;
    else if (e.key === "ArrowDown") d = -1;
    else return;
    e.preventDefault();
    const cur = fader === -1 ? preamp : bands[fader];
    const next = Math.max(-12, Math.min(12, cur + d));
    if (fader === -1) preamp = next;
    else bands[fader] = next;
  }

  const PRESET_NAMES = /** @type {(keyof typeof PRESETS)[]} */ (Object.keys(PRESETS));
  /** @param {keyof typeof PRESETS} name */
  function applyPreset(name) {
    bands = [...PRESETS[name]];
    menuOpen = false;
  }

  // Load a classic Winamp .EQF preset file. The file's ±20 dB range is clamped
  // to this EQ's ±12 dB sliders.
  async function loadEqf() {
    menuOpen = false;
    try {
      const preset = await invoke("import_eqf");
      if (!preset) return; // cancelled
      const clamp = (/** @type {number} */ db) => Math.max(-12, Math.min(12, Math.round(db)));
      bands = preset.bands.map(clamp);
      preamp = clamp(preset.preamp);
    } catch {
      /* invalid file — leave the current curve untouched */
    }
  }

  // Save the current curve to a .EQF file — the other half of Load .EQF, so a
  // tweaked EQ can be kept and reloaded (or used in real Winamp).
  async function saveEqf() {
    menuOpen = false;
    try {
      await invoke("export_eqf", {
        name: "Spotiamp+ preset",
        bands: [...bands],
        preamp,
      });
    } catch {
      /* dialog cancelled or write failed — nothing to change */
    }
  }

  // ask the player to untoggle (it owns the EQ button's lit state and will
  // hide us through set_eq_window_visible)
  const close = () => emitWindowEvent("eqWindow", { CloseRequested: null });

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(275, 116);
  });

  /** @param {HTMLElement} element */
  function makeEqDraggable(element) {
    makeDockedDraggable(element, "eq", "eqWindow");
  }

  // The skin's shape for this window (REGION.TXT), redone when the skin, the
  // window's size (shade mode, another screen's scale) or its UI scale changes.
  let shapeTick = $state(0);
  onMount(() => {
    const redo = () => shapeTick++;
    loadGraphSkin();
    window.addEventListener(SKIN_APPLIED, loadGraphSkin);
    window.addEventListener(SKIN_APPLIED, redo);
    window.addEventListener("resize", redo);
    return () => {
      window.removeEventListener(SKIN_APPLIED, loadGraphSkin);
      window.removeEventListener(SKIN_APPLIED, redo);
      window.removeEventListener("resize", redo);
    };
  });
  $effect(() => {
    void shapeTick;
    invoke("apply_skin_region", { section: "Equalizer", scale: window.devicePixelRatio * REACTIVE_WINDOW_SIZE.zoom }).catch(() => {});
  });
</script>

<!-- --zoom stays the body's 1: the UI scale is a CSS zoom on the whole page,
     and scaling the sprites here too would double it. -->
<div class="eq" class:inactive={!windowActive}>
  <!-- full-window skin background -->
  <div class="eq-bg" bind:this={bgElement}></div>

  <!-- titlebar (draggable); the X is baked into the sprite, we just add a hit area -->
  <div class="eq-titlebar" use:makeEqDraggable></div>
  <button class="eq-close" data-no-drag onclick={close} aria-label="Close"></button>

  <!-- ON / AUTO -->
  <button
    class="eq-on"
    class:on={enabled}
    onclick={() => (enabled = !enabled)}
    aria-label="Toggle EQ"
  ></button>
  <button
    class="eq-auto"
    class:on={auto}
    onclick={() => invoke("eq_set_auto", { on: !auto }).catch(() => {})}
    aria-label="Auto-load presets"
    title={applied ? `AUTO: this ${applied}'s preset is on` : "AUTO: load a song's or artist's own preset (save them from Presets)"}
  ></button>

  <!-- PRESETS -->
  <button
    class="eq-presets"
    class:pressed={menuOpen}
    onclick={() => (menuOpen = !menuOpen)}
    aria-label="Presets"
  ></button>
  {#if menuOpen}
    <div class="eq-menu">
      {#each PRESET_NAMES as name}
        <div
          class="eq-menu-item"
          role="button"
          tabindex="0"
          onclick={() => applyPreset(name)}
          onkeydown={(e) => e.key === "Enter" && applyPreset(name)}
        >
          {name}
        </div>
      {/each}
      <div class="eq-menu-sep"></div>
      <div
        class="eq-menu-item"
        role="button"
        tabindex="0"
        onclick={loadEqf}
        onkeydown={(e) => e.key === "Enter" && loadEqf()}
      >
        Load .EQF…
      </div>
      <div
        class="eq-menu-item"
        role="button"
        tabindex="0"
        onclick={saveEqf}
        onkeydown={(e) => e.key === "Enter" && saveEqf()}
      >
        Save .EQF…
      </div>
      {#if canSong || canArtist}
        <div class="eq-menu-sep"></div>
        {#if canSong}
          <div class="eq-menu-item" class:current={applied === "song"} role="button" tabindex="0" onclick={() => autoSave("song")} onkeydown={(e) => e.key === "Enter" && autoSave("song")}>
            Save for this song
          </div>
        {/if}
        {#if canArtist}
          <div class="eq-menu-item" class:current={applied === "artist"} role="button" tabindex="0" onclick={() => autoSave("artist")} onkeydown={(e) => e.key === "Enter" && autoSave("artist")}>
            Save for this artist
          </div>
        {/if}
        {#if hasSongPreset}
          <div class="eq-menu-item" role="button" tabindex="0" onclick={() => autoForget("song")} onkeydown={(e) => e.key === "Enter" && autoForget("song")}>
            Remove song preset
          </div>
        {/if}
        {#if hasArtistPreset}
          <div class="eq-menu-item" role="button" tabindex="0" onclick={() => autoForget("artist")} onkeydown={(e) => e.key === "Enter" && autoForget("artist")}>
            Remove artist preset
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  <!-- preamp fader -->
  <div
    class="fader"
    class:dragging={activeFader === -1}
    style="left: 21px; background-position: {frameBg(preamp)};"
    role="slider"
    tabindex="0"
    aria-label="Preamp"
    aria-valuemin="-12"
    aria-valuemax="12"
    aria-valuenow={preamp}
    onpointerdown={(e) => beginDrag(e, -1)}
    ondblclick={() => resetFader(-1)}
    onkeydown={(e) => nudge(e, -1)}
  >
    <div class="thumb" style="top: {thumbTop(preamp)}px;"></div>
  </div>

  <!-- band faders -->
  {#each bands as db, i}
    <div
      class="fader"
      class:dragging={activeFader === i}
      style="left: {BAND_X[i]}px; background-position: {frameBg(db)};"
      role="slider"
      tabindex="0"
      aria-label="Band {i + 1}"
      aria-valuemin="-12"
      aria-valuemax="12"
      aria-valuenow={db}
      onpointerdown={(e) => beginDrag(e, i)}
      ondblclick={() => resetFader(i)}
      onkeydown={(e) => nudge(e, i)}
    >
      <div class="thumb" style="top: {thumbTop(db)}px;"></div>
    </div>
  {/each}

  <!-- the graph, drawn as Winamp draws it (see the script) -->
  <canvas class="eq-graph" width="113" height="19" bind:this={graphCanvas}></canvas>
</div>

<style>
  /* the equalizer's own cursors (Winamp's EQNORMAL, EQTITLE, EQCLOSE,
     EQSLID), if the skin has them */
  :global(body),
  :global(body *) {
    cursor: var(--skin-cur-eqnormal, var(--skin-cur-normal, url(/src/static/assets/skins/base-2.91/MAINMENU.CUR))), auto;
  }
  .eq-titlebar {
    cursor: var(--skin-cur-eqtitle, url(/src/static/assets/skins/base-2.91/EQTITLE.CUR)), default;
  }
  .eq-close {
    cursor: var(--skin-cur-eqclose, var(--skin-cur-close, url(/src/static/assets/skins/base-2.91/CLOSE.CUR))), default;
  }
  .fader {
    cursor: var(--skin-cur-eqslid, url(/src/static/assets/skins/base-2.91/EQSLID.CUR)), default;
  }
  :global(html),
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }

  .eq {
    position: absolute;
    top: 0;
    left: 0;
    width: 275px;
    height: 116px;
    transform-origin: top left;
    transform: scale(var(--zoom));
    user-select: none;
  }

  .eq-bg {
    position: absolute;
    inset: 0;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    background-position: 0 0;
  }

  .eq-titlebar {
    position: absolute;
    top: 0;
    left: 0;
    width: 275px;
    height: 14px;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    background-position: 0 -134px;
  }
  /* another window in front: EQMAIN's dim titlebar, as Winamp's */
  .eq.inactive .eq-titlebar {
    background-position: 0 -149px;
  }
  .eq-close {
    position: absolute;
    top: 3px;
    left: 263px;
    width: 10px;
    height: 9px;
    background: transparent;
    cursor: pointer;
  }

  .eq-on {
    position: absolute;
    top: 18px;
    left: 14px;
    width: 25px;
    height: 12px;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    background-position: -10px -119px; /* off */
    cursor: pointer;
  }
  .eq-on.on {
    background-position: -187px -119px; /* on (green) */
  }
  .eq-auto {
    position: absolute;
    top: 18px;
    left: 40px;
    width: 31px;
    height: 12px;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    background-position: -36px -119px; /* auto, off */
    border: none;
    padding: 0;
    cursor: pointer;
  }
  .eq-auto.on {
    background-position: -213px -119px; /* auto, on (green) */
  }

  .eq-presets {
    position: absolute;
    top: 18px;
    left: 217px;
    width: 44px;
    height: 12px;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    background-position: -224px -164px;
    cursor: pointer;
  }
  .eq-presets.pressed {
    background-position: -224px -176px;
  }
  .eq-menu {
    position: absolute;
    top: 31px;
    left: 150px;
    width: 111px;
    max-height: 78px;
    overflow-y: auto;
    background: #0a0a12;
    border: 1px solid #4a4a6a;
    z-index: 10;
    font-family: Tahoma, "MS Sans Serif", sans-serif;
    font-size: 9px;
  }
  .eq-menu-item {
    padding: 1px 5px;
    color: #d8d8e8;
    cursor: pointer;
    white-space: nowrap;
  }
  .eq-menu-item:hover {
    background: #2b6fd6;
    color: #fff;
  }
  /* the song's or artist's preset that AUTO has on now */
  .eq-menu-item.current {
    color: #6cff6c;
  }
  .eq-menu-sep {
    height: 1px;
    margin: 2px 3px;
    background: #4a4a6a;
  }

  .eq-graph {
    position: absolute;
    left: 86px;
    top: 17px;
    width: 113px;
    height: 19px;
    pointer-events: none;
    image-rendering: pixelated;
  }

  .fader {
    position: absolute;
    top: 38px;
    width: 14px;
    height: 63px;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    cursor: pointer;
    touch-action: none;
  }
  .thumb {
    position: absolute;
    left: 1px;
    width: 11px;
    height: 11px;
    background-image: var(--skin-eqmain);
    background-repeat: no-repeat;
    background-position: 0 -164px; /* normal handle */
    pointer-events: none;
  }
  .fader.dragging .thumb {
    background-position: 0 -176px; /* pressed handle */
  }
</style>
