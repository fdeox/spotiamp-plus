<script>
  // The song's title in the main window the way Winamp 5 wrote it when it
  // wasn't using the skin's bitmap font (its default): Windows writes it in
  // the playlist font at 10 px (draw_main.cpp's _draw_songname_winfont), in
  // the two colours of the skin's TEXT.BMP (the background at 150,4, and the
  // pixel of its first letters furthest from it), in the 154 x 12 strip at
  // 111,24. A title wider than 146 px runs on as "title  ***  title", 5 px
  // every 200 ms. Read for its behaviour; none of its code is used.
  import { onDestroy, onMount } from "svelte";
  import { SKIN_APPLIED } from "$lib/common.svelte.js";
  import { paintLine } from "$lib/modern/skin/winampPe";

  /**
   * @typedef {Object} Props
   * @property {string} text
   * @property {string | undefined} textOverride a message shown a moment (volume, seeking)
   * @property {boolean} unavailable
   */
  /** @type {Props} */
  let { text, textOverride, unavailable } = $props();

  const AREA_W = 154;
  const AREA_H = 12;
  const SCROLL_FROM = 258 - 112;
  const SPACER = "  ***  ";

  /** @type {HTMLCanvasElement | undefined} */
  let canvas = $state();
  /** the skin's two colours, as Winamp read them off TEXT.BMP */
  let colors = $state(/** @type {{ fg: number[], bg: number[] } | null} */ (null));
  let skinTick = $state(0);
  /** the painted line ("title  ***  title" when it runs) and its widths */
  let strip = /** @type {HTMLCanvasElement | null} */ (null);
  let cycle = 0;
  let position = 0;
  let painting = 0;

  function face() {
    try {
      if (localStorage.getItem("skin-font") === "1") {
        const f = getComputedStyle(document.body).getPropertyValue("--skin-plfont").trim().replace(/^["']|["']$/g, "");
        if (f) return f;
      }
    } catch {}
    return "Arial";
  }

  /** TEXT.BMP's background and letter colours (draw_reinit_plfont) */
  async function readColors() {
    // (the picture's address as the page resolves it: the variable's own
    // text is relative to the stylesheet that set it, not to this page)
    const probe = document.createElement("div");
    probe.style.cssText = "position:absolute;width:0;height:0;background-image:var(--skin-text)";
    document.body.appendChild(probe);
    const value = getComputedStyle(probe).backgroundImage;
    probe.remove();
    const url = value.match(/url\((["']?)(.*?)\1\)/)?.[2];
    if (!url) return null;
    const img = new Image();
    try {
      await new Promise((resolve, reject) => {
        img.onload = resolve;
        img.onerror = reject;
        img.src = url;
      });
    } catch {
      return null;
    }
    const c = document.createElement("canvas");
    c.width = Math.max(img.naturalWidth, 155);
    c.height = Math.max(img.naturalHeight, 6);
    const ctx = c.getContext("2d", { willReadFrequently: true });
    if (!ctx) return null;
    ctx.drawImage(img, 0, 0);
    const bg = [...ctx.getImageData(150, 4, 1, 1).data.slice(0, 3)];
    let fg = bg;
    let far = 0;
    const px = ctx.getImageData(0, 0, 20, 6).data;
    for (let i = 0; i < px.length; i += 4) {
      const d = (px[i] - bg[0]) ** 2 + (px[i + 1] - bg[1]) ** 2 + (px[i + 2] - bg[2]) ** 2;
      if (d > far) {
        far = d;
        fg = [px[i], px[i + 1], px[i + 2]];
      }
    }
    return { fg, bg };
  }

  /** @param {{ w: number, h: number, data: Uint8ClampedArray }} line */
  function toCanvas(line) {
    const c = document.createElement("canvas");
    c.width = line.w;
    c.height = line.h;
    c.getContext("2d")?.putImageData(new ImageData(/** @type {any} */ (line.data), line.w, line.h), 0, 0);
    return c;
  }

  async function prepare() {
    const shown = textOverride || text || "";
    const look = colors;
    if (!look) return;
    const ticket = ++painting;
    const f = face();
    const ask = (/** @type {string} */ t) => paintLine({ face: f, size: 10, text: t, fg: look.fg, bg: look.bg });
    const one = await ask(shown);
    if (ticket !== painting || !one) return;
    // a message (volume, seeking) stands still, as Winamp held it
    if (one.w > SCROLL_FROM && !textOverride) {
      const spacer = await ask(SPACER);
      const run = await ask(`${shown}${SPACER}${shown}`);
      if (ticket !== painting || !spacer || !run) return;
      strip = toCanvas(run);
      cycle = one.w + spacer.w;
    } else {
      strip = toCanvas(one);
      cycle = 0;
    }
    if (position >= cycle) position = 0;
    draw();
  }

  function draw() {
    const ctx = canvas?.getContext("2d");
    if (!ctx || !colors) return;
    ctx.fillStyle = `rgb(${colors.bg.join(",")})`;
    ctx.fillRect(0, 0, AREA_W, AREA_H);
    // (the line was written from y 23; the strip shows from y 24)
    if (strip) ctx.drawImage(strip, cycle ? position : 0, 1, AREA_W, AREA_H, 0, 0, AREA_W, AREA_H);
  }

  // a new title starts from its beginning
  $effect(() => {
    void text;
    position = 0;
  });
  $effect(() => {
    void text;
    void textOverride;
    void colors;
    prepare();
  });
  // (the last skin's read wins, whichever comes back first)
  let colorTicket = 0;
  $effect(() => {
    void skinTick;
    const ticket = ++colorTicket;
    readColors().then((c) => {
      if (ticket !== colorTicket) return;
      // (a skin whose TEXT.BMP can't be read: Winamp's base colours)
      colors = c ?? { fg: [0, 226, 0], bg: [0, 0, 0] };
    });
  });

  const timer = setInterval(() => {
    if (!cycle || textOverride) return;
    position += 5;
    if (position >= cycle) position -= cycle;
    draw();
  }, 200);
  onDestroy(() => clearInterval(timer));
  onMount(() => {
    const again = () => skinTick++;
    window.addEventListener(SKIN_APPLIED, again);
    const font = (/** @type {StorageEvent} */ e) => e.key === "skin-font" && prepare();
    window.addEventListener("storage", font);
    return () => {
      window.removeEventListener(SKIN_APPLIED, again);
      window.removeEventListener("storage", font);
    };
  });
</script>

<canvas class="winamp-title" width={AREA_W} height={AREA_H} bind:this={canvas} style:opacity={unavailable ? "50%" : "100%"}></canvas>

<style>
  .winamp-title {
    position: absolute;
    left: calc(111px * var(--zoom));
    top: calc(24px * var(--zoom));
    width: calc(154px * var(--zoom));
    height: calc(12px * var(--zoom));
    image-rendering: pixelated;
    pointer-events: none;
  }
</style>
