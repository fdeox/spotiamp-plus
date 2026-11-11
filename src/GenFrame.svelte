<script>
  // A classic skin's frame for Spotiamp+'s windows, drawn from GEN.BMP the
  // way Winamp 5 drew its embedded windows (the Media Library, Album Art, the
  // visualizer's): lib/winampml/genframe.js. The window's insides go in the
  // room it leaves (FRAME: 11 px left, 20 top, 8 right, 14 bottom; the
  // --gen-* variables say it); its titlebar moves the window, its close
  // button closes it, its corner sizes it. In a modern skin the skin's own
  // frame is worn instead (global.css), and this draws nothing.
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE, SKIN_APPLIED } from "$lib/common.svelte.js";
  import { watchWindowActive } from "$lib/window-active.js";
  import { BASE_GEN, cssImageUrl, loadImage, toCanvas } from "$lib/winampml/genex.js";
  import { drawGenFrame, scanLetters } from "$lib/winampml/genframe.js";

  /**
   * @typedef {Object} Props
   * @property {string} title
   * @property {() => void} onclose
   * @property {string} [titleVar] a skin var that can rename the window (libtitle)
   * @property {(el: HTMLElement) => void} [drag] makes the titlebar move the window
   * @property {(el: HTMLElement) => void} [resize] makes the corner size it
   */
  /** @type {Props} */
  let { title, onclose, titleVar, drag = () => {}, resize } = $props();

  /** @type {HTMLCanvasElement | undefined} */
  let canvas = $state();
  let gen = $state(/** @type {{ img: HTMLCanvasElement, letters: number[][] | null, own: boolean } | null} */ (null));
  let named = $state("");
  let modern = $state(false);
  let active = $state(true);
  let closePressed = $state(false);
  let seq = 0;

  async function read() {
    const my = ++seq;
    modern = "modern" in document.body.dataset;
    const style = getComputedStyle(document.body);
    named = titleVar ? style.getPropertyValue(`--skin-${titleVar}`).trim().replace(/^"(.*)"$/, "$1") : "";
    // the skin's GEN.BMP, else Winamp's own (the base skin's)
    const own = cssImageUrl("--skin-gen");
    const img = await loadImage(own ?? BASE_GEN);
    if (my !== seq || !img) return;
    const sheet = toCanvas(img);
    gen = { img: sheet, letters: scanLetters(sheet), own: !!own };
  }

  onMount(() => {
    read();
    window.addEventListener(SKIN_APPLIED, read);
    const stop = watchWindowActive((a) => (active = a));
    return () => {
      window.removeEventListener(SKIN_APPLIED, read);
      stop();
    };
  });

  const text = $derived(named || title);
  const w = $derived(Math.round(REACTIVE_WINDOW_SIZE.width));
  const h = $derived(Math.round(REACTIVE_WINDOW_SIZE.height));

  $effect(() => {
    const c = canvas;
    const g = gen;
    if (!c || !g || modern || w < 1 || h < 1) return;
    c.width = w;
    c.height = h;
    const ctx = c.getContext("2d");
    if (ctx) drawGenFrame(ctx, g.img, w, h, text, g.letters, active, closePressed);
  });

  /** @param {PointerEvent} e */
  function closeDown(e) {
    closePressed = true;
    /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
  }
  /** @param {PointerEvent} e */
  function closeUp(e) {
    const r = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX < r.right && e.clientY >= r.top && e.clientY < r.bottom;
    closePressed = false;
    if (inside) onclose();
  }
</script>

{#if !modern}
  <!-- (what it was drawn from, for the smoke tests) -->
  <canvas
    class="gf-frame"
    bind:this={canvas}
    data-sheet={gen ? (gen.own ? "skin" : "base") : ""}
    data-letters={gen?.letters ? [...text].filter((ch) => (gen?.letters?.[ch.toUpperCase().charCodeAt(0) - 65]?.[1] ?? 0) > 0).length : 0}
  ></canvas>
{/if}
<div class="gf-titlebar" data-gen="titlebar" use:drag></div>
{#if !modern}
  <div
    class="gf-close"
    data-gen="close"
    data-no-drag
    role="button"
    tabindex="-1"
    aria-label="Close"
    onpointerdown={closeDown}
    onpointerup={closeUp}
  ></div>
{:else}
  <!-- the skin's frame draws the close button; it clicks this one (+layout.svelte) -->
  <button data-gen="close" hidden aria-label="Close" onclick={() => onclose()}></button>
{/if}
{#if resize}
  <div class="gf-resize" data-gen="resize" use:resize></div>
{/if}

<style>
  /* behind the window's insides (its root isolates them) */
  .gf-frame {
    position: absolute;
    left: 0;
    top: 0;
    width: 100%;
    height: 100%;
    z-index: -1;
    image-rendering: pixelated;
    pointer-events: none;
  }
  :global(body[data-fractional-scale]) .gf-frame {
    image-rendering: auto;
  }
  .gf-titlebar {
    position: absolute;
    left: 0;
    top: 0;
    right: 14px;
    height: 20px;
    z-index: 3;
  }
  .gf-close {
    position: absolute;
    right: 2px;
    top: 3px;
    width: 9px;
    height: 9px;
    z-index: 4;
  }
  .gf-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 14px;
    height: 14px;
    cursor: nwse-resize;
    z-index: 4;
  }
</style>
