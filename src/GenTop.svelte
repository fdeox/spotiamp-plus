<script>
  // A generic window's titlebar built the way Winamp builds one from GEN.BMP:
  // the corners, the end caps either side of the title, the fills between
  // them (the title stays centred), and the title in the skin's own letters,
  // focused or not. It covers the window's tile titlebar, which is what's
  // left showing for a skin without a usable GEN.BMP (no --skin-gen).
  import { onMount } from "svelte";
  import { SKIN_APPLIED } from "$lib/common.svelte.js";

  /**
   * @typedef {Object} Props
   * @property {string} title
   * @property {() => void} onclose
   * @property {string} [titleVar] a skin var that can rename the window (libtitle)
   */

  /** @type {Props} */
  let { title, onclose, titleVar } = $props();

  let drawn = $state(false);
  /** @type {number[][] | null} */
  let letters = $state(null);
  /** @type {number[][] | null} */
  let letters2 = $state(null);
  let named = $state("");
  let focused = $state(true);

  /** "x:w x:w …" → 26 [x, w] pairs, or null */
  function parse(/** @type {string} */ value) {
    const cells = value.split(" ").filter(Boolean).map((c) => c.split(":").map(Number));
    const ok = cells.length === 26 && cells.every((c) => c.length === 2 && c.every((n) => n >= 0));
    return ok ? cells : null;
  }

  function read() {
    const style = getComputedStyle(document.body);
    drawn = style.getPropertyValue("--skin-gen").trim().startsWith("url(");
    letters = parse(style.getPropertyValue("--skin-genletters").trim());
    letters2 = parse(style.getPropertyValue("--skin-genletters2").trim()) ?? letters;
    named = titleVar
      ? style.getPropertyValue(`--skin-${titleVar}`).trim().replace(/^"(.*)"$/, "$1")
      : "";
  }

  onMount(() => {
    read();
    focused = document.hasFocus();
    const onFocus = () => (focused = true);
    const onBlur = () => (focused = false);
    window.addEventListener(SKIN_APPLIED, read);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener(SKIN_APPLIED, read);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  });

  const text = $derived(named || title);

  // [x, width] on the sheet per letter; a space is a 5 px gap (x -1), and
  // anything GEN.BMP has no letter for (an apostrophe) is left out
  const glyphs = $derived.by(() => {
    const set = focused ? letters : letters2;
    if (!set) return null;
    const out = [];
    for (const ch of text.toUpperCase()) {
      const i = ch.charCodeAt(0) - 65;
      if (i >= 0 && i < 26) out.push(set[i]);
      else if (ch === " ") out.push([-1, 5]);
    }
    return out;
  });
</script>

{#if drawn}
  <div class="gt" class:blur={!focused}>
    <div class="gt-piece gt-left"></div>
    <div class="gt-fill"></div>
    <div class="gt-piece gt-end"></div>
    <div class="gt-title">
      {#if glyphs}
        {#each glyphs as [x, w], i (i)}
          <span
            class="gt-letter"
            class:space={x < 0}
            style:width="{w}px"
            style:background-position="-{x}px -{focused ? 88 : 96}px"
          ></span>
        {/each}
      {:else}
        <span class="gt-text">{text}</span>
      {/if}
    </div>
    <div class="gt-piece gt-end right"></div>
    <div class="gt-fill right"></div>
    <div class="gt-piece gt-right">
      <button class="gt-close" data-no-drag onclick={onclose} aria-label="Close"></button>
    </div>
  </div>
{/if}

<style>
  /* GEN.BMP's titlebar: the focused row at y=0, the unfocused one at y=21,
     each piece 25x20 — top-left at x=0, the left end cap 26, (the title's
     fill 52), the right end cap 78, (the fill between 104), top-right with
     its close button 130. The two fills come as tiles of their own. */
  .gt {
    position: absolute;
    inset: 0;
    z-index: 3;
    display: flex;
    height: 20px;
    --row: 0px;
    --bar: var(--skin-genbar);
    --mid: var(--skin-genmid);
  }
  .gt.blur {
    --row: -21px;
    --bar: var(--skin-genbar2);
    --mid: var(--skin-genmid2);
  }
  .gt-piece {
    flex: 0 0 25px;
    height: 20px;
    background-image: var(--skin-gen);
    background-repeat: no-repeat;
  }
  .gt-left {
    background-position: 0 var(--row);
  }
  .gt-end {
    background-position: -26px var(--row);
  }
  .gt-end.right {
    background-position: -78px var(--row);
  }
  .gt-right {
    position: relative;
    background-position: -130px var(--row);
  }
  .gt-fill {
    flex: 1 1 0;
    min-width: 0;
    height: 20px;
    background: var(--bar) repeat-x left top;
  }
  .gt-fill.right {
    background-position: right top;
  }
  .gt-title {
    flex: 0 1 auto;
    min-width: 0;
    display: flex;
    align-items: flex-start;
    height: 20px;
    padding: 4px 3px 0 4px;
    box-sizing: border-box;
    overflow: hidden;
    background: var(--mid) repeat-x;
  }
  .gt-letter {
    flex: 0 0 auto;
    height: 7px;
    background-image: var(--skin-gen);
    background-repeat: no-repeat;
  }
  .gt-letter.space {
    background: none;
  }
  .gt-text {
    margin-top: -1px;
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(8px * var(--px-crisp, 1));
    line-height: 8px;
    -webkit-font-smoothing: none;
    letter-spacing: 1px;
    color: var(--skin-titletext, #f0f2f8);
    white-space: nowrap;
  }
  .gt-close {
    position: absolute;
    right: 0;
    top: 1px;
    width: 13px;
    height: 13px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
  }
  /* pressed: GEN.BMP's own pressed close button, 9x9 at 148,42 */
  .gt-close:active::after {
    content: "";
    position: absolute;
    right: 2px;
    top: 2px;
    width: 9px;
    height: 9px;
    background-image: var(--skin-gen);
    background-position: -148px -42px;
  }
</style>
