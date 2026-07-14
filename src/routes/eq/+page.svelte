<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { makeTauriWindowDraggable } from "$lib/window-docking.svelte.js";

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

  // window-space geometry (px). Band faders every 18px starting at x=78.
  const BAND_X = [78, 96, 114, 132, 150, 168, 186, 204, 222, 240];
  const WELL_TOP = 37;
  const WELL_H = 64;
  const THUMB_H = 11;
  const TRAVEL = WELL_H - THUMB_H; // 53

  // dB (+12..-12) -> one of the 28 EQMAIN slider background frames (0 = top/green).
  function dbToV(db) {
    let v = Math.round(((12 - db) / 24) * 27);
    return Math.max(0, Math.min(27, v));
  }
  function frameBg(db) {
    const v = dbToV(db);
    const x = -(13 + (v % 14) * 15);
    const y = -(v < 14 ? 164 : 229);
    return `${x}px ${y}px`;
  }
  // smooth thumb position (unlike the snapped colour frame)
  function thumbTop(db) {
    let f = (12 - db) / 24;
    f = Math.max(0, Math.min(1, f));
    return f * TRAVEL;
  }

  // --- EQ response spline (the little animated curve in the top graph box) ---
  // Classic Winamp draws it in a compact 19px-tall preview over the grid, NOT
  // over the full fader travel, and colours it top=green .. bottom=red.
  const SP_X0 = 88; // first grid column
  const SP_STEP = 12; // grid columns are 12px apart
  const SP_TOP = 17; // +12 dB
  const SP_BOT = 34; // -12 dB
  function splineY(db) {
    return SP_TOP + ((12 - db) / 24) * (SP_BOT - SP_TOP);
  }
  function catmullRom(pts) {
    if (pts.length < 2) return "";
    let d = `M ${pts[0].x} ${pts[0].y}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[i - 1] ?? pts[i];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = pts[i + 2] ?? p2;
      const c1x = p1.x + (p2.x - p0.x) / 6;
      const c1y = p1.y + (p2.y - p0.y) / 6;
      const c2x = p2.x - (p3.x - p1.x) / 6;
      const c2y = p2.y - (p3.y - p1.y) / 6;
      d += ` C ${c1x} ${c1y}, ${c2x} ${c2y}, ${p2.x} ${p2.y}`;
    }
    return d;
  }
  const splinePath = $derived.by(() => {
    const pts = bands.map((db, i) => ({ x: SP_X0 + i * SP_STEP, y: splineY(db) }));
    const ext = [
      { x: SP_X0 - 2, y: pts[0].y },
      ...pts,
      { x: SP_X0 + 9 * SP_STEP + 2, y: pts[pts.length - 1].y },
    ];
    return catmullRom(ext);
  });

  function push() {
    invoke("set_eq", { enabled, preamp, bands: [...bands] }).catch(() => {});
  }
  $effect(() => {
    enabled;
    preamp;
    bands;
    push();
  });

  function setFromY(clientY, rect, fader) {
    const relPx = ((clientY - rect.top) / rect.height) * WELL_H;
    const f = Math.max(0, Math.min(1, (relPx - THUMB_H / 2) / TRAVEL));
    let db = Math.round(12 - f * 24);
    db = Math.max(-12, Math.min(12, db));
    if (fader === -1) preamp = db;
    else bands[fader] = db;
  }
  function beginDrag(e, fader) {
    e.preventDefault();
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    activeFader = fader;
    setFromY(e.clientY, rect, fader);
    const move = (ev) => setFromY(ev.clientY, rect, fader);
    const up = () => {
      activeFader = -99;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
  function resetFader(fader) {
    if (fader === -1) preamp = 0;
    else bands[fader] = 0;
  }
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

  function applyPreset(name) {
    bands = [...PRESETS[name]];
    menuOpen = false;
  }

  // ask the player to untoggle (it owns the EQ button's lit state and will
  // hide us through set_eq_window_visible)
  const close = () => emitWindowEvent("eqWindow", { CloseRequested: null });

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(275, 116);
    REACTIVE_WINDOW_SIZE.setZoom(1);
  });

  function makeEqDraggable(element) {
    makeTauriWindowDraggable(element, {
      async onStart() {
        await emitWindowEvent("eqWindow", { DragStarted: null });
        return {};
      },
      async onEnd() {
        await emitWindowEvent("eqWindow", { DragEnded: null });
      },
    });
  }
</script>

<div class="eq" style="--zoom: {REACTIVE_WINDOW_SIZE.zoom}">
  <!-- full-window skin background -->
  <div class="eq-bg"></div>

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
  <div class="eq-auto" aria-hidden="true"></div>

  <!-- PRESETS -->
  <button
    class="eq-presets"
    class:pressed={menuOpen}
    onclick={() => (menuOpen = !menuOpen)}
    aria-label="Presets"
  ></button>
  {#if menuOpen}
    <div class="eq-menu">
      {#each Object.keys(PRESETS) as name}
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

  <!-- live EQ response spline in the top preview box, rainbow-coloured by
       height like classic Winamp (green = boost, red = cut) -->
  <svg class="eq-curve" viewBox="0 0 275 116" preserveAspectRatio="none">
    <defs>
      <linearGradient
        id="eqspline"
        gradientUnits="userSpaceOnUse"
        x1="0"
        y1={SP_TOP}
        x2="0"
        y2={SP_BOT}
      >
        <stop offset="0%" stop-color="#2a9a16" />
        <stop offset="25%" stop-color="#a4e238" />
        <stop offset="45%" stop-color="#efdc31" />
        <stop offset="65%" stop-color="#e09228" />
        <stop offset="85%" stop-color="#ef5221" />
        <stop offset="100%" stop-color="#d3221b" />
      </linearGradient>
    </defs>
    <path
      d={splinePath}
      fill="none"
      stroke="url(#eqspline)"
      stroke-width="1"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
</div>

<style>
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

  .eq-curve {
    position: absolute;
    top: 0;
    left: 0;
    width: 275px;
    height: 116px;
    pointer-events: none;
  }

  .fader {
    position: absolute;
    top: 37px;
    width: 14px;
    height: 64px;
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
