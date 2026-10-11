<script>
  // The main window's visualizer, drawn the way Winamp drew it: 76x16 pixels
  // in the skin's own colours (VISCOLOR.TXT: 0 the background, 1 its dots,
  // 2-17 the bars top to bottom, 18-22 the oscilloscope, 23 the peaks).
  // Click it to go spectrum → oscilloscope → off, double-click for the
  // visualizer window, right-click for Winamp's options. They're kept per PC.
  import { invoke } from "@tauri-apps/api/core";
  import { Menu } from "@tauri-apps/api/menu";
  import { onMount } from "svelte";

  /**
   * @typedef {Object} Props
   * @property {import("$lib/visualizer.svelte").Visualizer} visualizer
   * @property {string} waveCommand the backend command for the waveform
   * @property {() => void} onOpen opens the visualizer window
   */

  /** @type {Props} */
  let { visualizer, waveCommand, onOpen } = $props();

  const W = 76;
  const H = 16;
  const BANDS = 19;
  /** Winamp's base skin, for a skin without VISCOLOR.TXT */
  const BASE =
    "0,0,0 24,33,41 239,49,16 206,41,16 214,90,0 214,102,0 214,115,0 198,123,8 222,165,24 214,181,33 " +
    "189,222,41 148,222,33 41,206,16 50,190,16 57,181,16 49,156,8 41,148,0 24,132,8 255,255,255 " +
    "214,214,222 181,189,189 160,170,175 148,156,165 150,150,150";
  /** how fast the bars and the peaks drop, slowest to fastest */
  const FALLOFF = ["Slowest", "Slow", "Moderate", "Fast", "Fastest"];
  const BAR_FALL = [0.8, 1.4, 2.4, 4, 7]; // full heights a second
  const PEAK_GRAVITY = [0.25, 0.5, 1, 2, 4]; // times the usual

  /** @param {string} key @param {string} fallback */
  function load(key, fallback) {
    try {
      return localStorage.getItem(`vis-${key}`) ?? fallback;
    } catch {
      return fallback;
    }
  }
  /** @param {string} key @param {string | number | boolean} value */
  function save(key, value) {
    try {
      localStorage.setItem(`vis-${key}`, String(value));
    } catch {}
  }

  let mode = $state(load("mode", "spectrum")); // spectrum | scope | off
  let style = $state(load("style", "normal")); // normal | fire | line
  let thin = $state(load("thin", "false") === "true");
  let peaks = $state(load("peaks", "true") === "true");
  let barFall = $state(Number(load("bar-fall", "2")));
  let peakFall = $state(Number(load("peak-fall", "2")));

  /** @type {number[][]} */
  let colours = $state(parse(BASE));

  /** @param {string} text @returns {number[][]} */
  function parse(text) {
    const list = text
      .split(/\s+/)
      .map((c) => c.split(",").map(Number))
      .filter((c) => c.length === 3 && c.every((v) => v >= 0 && v <= 255));
    return list.length === 24 ? list : parse(BASE);
  }
  function readColours() {
    const value = getComputedStyle(document.body).getPropertyValue("--skin-viscolor").trim();
    colours = parse(value || BASE);
  }

  /** @type {HTMLCanvasElement | undefined} */
  let canvas = $state();
  /** what the bars show: the readings, let down at the chosen speed */
  const shown = new Array(BANDS).fill(0);
  /** @type {Uint8Array | null} the latest waveform, mono, 0-255 */
  let wave = null;
  let lastWave = 0;
  let lastStep = 0;
  let raf = 0;

  $effect(() => {
    const g = 0.000005 * PEAK_GRAVITY[peakFall];
    for (const bar of visualizer.bars) bar.gravity = g;
  });

  // redraw when a reading or a setting changes
  $effect(() => {
    for (const bar of visualizer.bars) {
      void bar.value;
      void bar.hat;
    }
    void [mode, style, thin, peaks, barFall, colours];
    kick();
  });

  function kick() {
    if (!raf) raf = requestAnimationFrame(loop);
  }

  /** @param {number} now */
  function loop(now) {
    raf = 0;
    const dt = lastStep ? Math.min(0.1, (now - lastStep) / 1000) : 0;
    lastStep = now;
    let falling = false;
    visualizer.bars.forEach((bar, i) => {
      const v = Math.min(1, Math.max(0, bar.value));
      shown[i] = Math.max(v, shown[i] - BAR_FALL[barFall] * dt);
      if (shown[i] > v + 0.001) falling = true;
    });
    if (mode === "scope" && visualizer.running && now - lastWave > 33) {
      lastWave = now;
      invoke(waveCommand)
        .then((bytes) => {
          const all = bytes instanceof ArrayBuffer ? new Uint8Array(bytes) : new Uint8Array(/** @type {number[]} */ (bytes));
          wave = all.subarray(0, all.length / 3);
          kick();
        })
        .catch(() => {});
    }
    draw();
    if (falling || visualizer.running) raf = requestAnimationFrame(loop);
    else lastStep = 0;
  }

  /** a band's height at a column, the 19 readings spread over 76 */
  /** @param {number[]} values @param {number} x */
  function across(values, x) {
    const at = (x / (W - 1)) * (BANDS - 1);
    const i = Math.floor(at);
    const f = at - i;
    return values[i] * (1 - f) + (values[Math.min(i + 1, BANDS - 1)] ?? 0) * f;
  }

  function draw() {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    const img = ctx.createImageData(W, H);
    const px = img.data;
    /** @param {number} x @param {number} y @param {number[]} c */
    const put = (x, y, c) => {
      const o = (y * W + x) * 4;
      px[o] = c[0];
      px[o + 1] = c[1];
      px[o + 2] = c[2];
      px[o + 3] = 255;
    };
    if (mode === "off") {
      ctx.clearRect(0, 0, W, H);
      return;
    }
    // the background and its dots
    for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) put(x, y, colours[x % 2 === 0 && y % 2 === 1 ? 1 : 0]);
    if (mode === "scope") {
      if (wave && visualizer.running) {
        let prev = -1;
        for (let x = 0; x < W; x++) {
          const s = wave[Math.floor((x * wave.length) / W)] / 128 - 1;
          const y = Math.max(0, Math.min(H - 1, Math.round(7.5 - s * 8)));
          const from = prev < 0 ? y : Math.min(prev, y);
          const to = prev < 0 ? y : Math.max(prev, y);
          for (let yy = from; yy <= to; yy++) put(x, yy, colours[18 + Math.min(4, Math.floor(Math.abs(yy - 7.5) / 2))]);
          prev = y;
        }
      }
      ctx.putImageData(img, 0, 0);
      return;
    }
    const hats = visualizer.bars.map((b) => Math.min(1, Math.max(0, b.hat)));
    const columns = thin ? W : BANDS;
    for (let i = 0; i < columns; i++) {
      const value = thin ? across(shown, i) : shown[i];
      const hat = thin ? across(hats, i) : hats[i];
      const xs = thin ? [i] : [i * 4, i * 4 + 1, i * 4 + 2];
      const h = Math.round(value * H);
      const top = H - h;
      for (let y = top; y < H; y++) {
        // Normal: by the row; Fire: from the bar's top down; Line: one colour
        const row = style === "fire" ? y - top : style === "line" ? top : y;
        for (const x of xs) put(x, y, colours[2 + row]);
      }
      const p = Math.round(hat * H);
      if (peaks && p > 0) for (const x of xs) put(x, H - p, colours[23]);
    }
    ctx.putImageData(img, 0, 0);
  }

  onMount(() => {
    readColours();
    // a skin change sets new variables on the body
    const watch = new MutationObserver(readColours);
    watch.observe(document.body, { attributes: true, attributeFilter: ["style", "data-skin"] });
    kick();
    return () => {
      watch.disconnect();
      cancelAnimationFrame(raf);
    };
  });

  // a click cycles the modes, but not the two clicks of a double-click
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let clickTimer;
  function onClick() {
    clearTimeout(clickTimer);
    clickTimer = setTimeout(() => setMode(mode === "spectrum" ? "scope" : mode === "scope" ? "off" : "spectrum"), 250);
  }
  function onDoubleClick() {
    clearTimeout(clickTimer);
    onOpen();
  }
  /** @param {string} m */
  function setMode(m) {
    mode = m;
    save("mode", m);
  }

  /** @param {MouseEvent} e */
  async function onMenu(e) {
    e.preventDefault();
    e.stopPropagation();
    /** @param {string} text @param {string} value @param {string} current @param {(v: string) => void} set */
    const pick = (text, value, current, set) => ({ text, checked: current === value, action: () => set(value) });
    /** @param {number} current @param {(i: number) => void} set */
    const speeds = (current, set) => FALLOFF.map((text, i) => ({ text, checked: current === i, action: () => set(i) }));
    try {
      const menu = await Menu.new({
        items: [
          pick("Spectrum analyzer", "spectrum", mode, setMode),
          pick("Oscilloscope", "scope", mode, setMode),
          pick("Off", "off", mode, setMode),
          { item: "Separator" },
          {
            text: "Analyzer style",
            items: [
              pick("Normal", "normal", style, (v) => ((style = v), save("style", v))),
              pick("Fire", "fire", style, (v) => ((style = v), save("style", v))),
              pick("Line", "line", style, (v) => ((style = v), save("style", v))),
            ],
          },
          { text: "Thick bands", checked: !thin, action: () => ((thin = !thin), save("thin", thin)) },
          { text: "Show peaks", checked: peaks, action: () => ((peaks = !peaks), save("peaks", peaks)) },
          { text: "Analyzer falloff", items: speeds(barFall, (i) => ((barFall = i), save("bar-fall", i))) },
          { text: "Peaks falloff", items: speeds(peakFall, (i) => ((peakFall = i), save("peak-fall", i))) },
          { item: "Separator" },
          { text: "Open the visualizer window", action: onOpen },
        ],
      });
      await menu.popup();
    } catch (err) {
      invoke("log_frontend_error", { window: "player", message: `vis menu: ${err}` }).catch(() => {});
    }
  }
</script>

<canvas class="main-vis" width={W} height={H} bind:this={canvas}></canvas>
<button
  class="main-vis-hit"
  onclick={onClick}
  ondblclick={onDoubleClick}
  oncontextmenu={onMenu}
  aria-label="Visualizer: click to change, double-click to open the visualizer window"
  title="click: spectrum / oscilloscope / off · double-click: visualizer window · right-click: options"
></button>

<style>
  .main-vis {
    position: absolute;
    left: calc(24px * var(--zoom));
    top: calc(43px * var(--zoom));
    width: calc(76px * var(--zoom));
    height: calc(16px * var(--zoom));
    image-rendering: pixelated;
    pointer-events: none;
  }
  .main-vis-hit {
    position: absolute;
    left: calc(24px * var(--zoom));
    top: calc(43px * var(--zoom));
    width: calc(76px * var(--zoom));
    height: calc(16px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 30;
  }
</style>
