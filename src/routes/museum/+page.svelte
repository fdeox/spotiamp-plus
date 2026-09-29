<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";

  // Set before the first paint so the window opens at this size.
  REACTIVE_WINDOW_SIZE.setSize(480, 470);

  /** @typedef {{name: string, md5: string, screenshot: string, download: string, page: string | null}} MuseumSkin */

  /** @type {MuseumSkin[]} */
  let skins = $state([]);
  // classics | random | search
  let kind = $state("classics");
  let query = $state("");
  let offset = $state(0);
  let loading = $state(false);
  let error = $state("");
  /** md5 of the skin being downloaded / the one put on last */
  let applying = $state("");
  let applied = $state("");
  let status = $state("");

  /** @param {string} nextKind @param {boolean} [more] */
  async function load(nextKind, more = false) {
    if (loading) return;
    if (nextKind === "search" && !query.trim()) return;
    kind = nextKind;
    offset = more ? offset + 24 : 0;
    loading = true;
    error = "";
    try {
      const list = /** @type {MuseumSkin[]} */ (
        await invoke("museum_skins", { kind, query: query.trim(), offset })
      );
      skins = more ? [...skins, ...list] : list;
      if (!more && listEl) listEl.scrollTop = 0;
      if (!list.length && !more) error = kind === "search" ? `Nothing found for "${query.trim()}".` : "Nothing came back. Try again.";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /** @param {MuseumSkin} skin */
  async function wear(skin) {
    if (applying) return;
    applying = skin.md5;
    status = `Putting on ${skin.name}…`;
    try {
      await invoke("museum_apply", { md5: skin.md5, download: skin.download });
      applied = skin.md5;
      status = `Wearing ${skin.name}.`;
      emitWindowEvent("skinChanged", { skin: "custom" });
    } catch (e) {
      status = `Couldn't put on ${skin.name}: ${e}`;
    } finally {
      applying = "";
    }
  }

  /** @type {HTMLElement | undefined} */
  let listEl = $state();

  onMount(() => {
    load("classics");
  });

  const close = () => invoke("close_museum").catch(() => {});
  const openMuseum = () => invoke("open_external", { target: "museum" }).catch(() => {});

  /** @param {MouseEvent} e */
  function drag(e) {
    if (e.button !== 0) return;
    if (/** @type {HTMLElement} */ (e.target).closest("[data-no-drag]")) return;
    getCurrentWindow().startDragging().catch(() => {});
  }

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (e.key === "Escape" && /** @type {HTMLElement} */ (e.target).tagName !== "INPUT") close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="mu">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="mu-titlebar" onmousedown={drag}>
    <div class="mu-tl"></div>
    <span class="mu-title">SKIN MUSEUM</span>
    <button class="mu-close" data-no-drag onclick={close} aria-label="Close"></button>
  </div>

  <div class="mu-bar">
    <input
      class="mu-search"
      placeholder="search 12,000+ skins…"
      bind:value={query}
      onkeydown={(e) => e.key === "Enter" && load("search")}
    />
    <button class="mu-btn" onclick={() => load("search")}>Search</button>
    <button class="mu-btn" class:on={kind === "classics"} onclick={() => load("classics")}>Classics</button>
    <button class="mu-btn" class:on={kind === "random"} onclick={() => load("random")}>Random</button>
  </div>

  <div class="mu-list" bind:this={listEl}>
    {#if error}
      <div class="mu-msg">{error}</div>
    {/if}
    <div class="mu-grid">
      {#each skins as skin (skin.md5)}
        <button
          class="mu-skin"
          class:applied={applied === skin.md5}
          class:busy={applying === skin.md5}
          title="Put on {skin.name}"
          onclick={() => wear(skin)}
        >
          <img src={skin.screenshot} alt={skin.name} loading="lazy" draggable="false" />
          <span class="mu-name">{skin.name}</span>
        </button>
      {/each}
    </div>
    {#if loading}
      <div class="mu-msg">Loading…</div>
    {:else if skins.length && kind !== "search"}
      <button class="mu-btn mu-more" onclick={() => load(kind, true)}>
        {kind === "random" ? "More random skins" : "More"}
      </button>
    {/if}
  </div>

  <div class="mu-footer">
    <span class="mu-status">{status || "Click a skin to put it on. Skins tab ▸ Classic to go back."}</span>
    <button class="mu-link" onclick={openMuseum}>skins.webamp.org</button>
  </div>
</div>

<style>
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }

  :global(html),
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }

  .mu {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    --fg: var(--skin-plnormal, #00ff41);
    --hi: var(--skin-plcurrent, #fff);
    background: var(--frame);
    box-sizing: border-box;
    padding: 0 2px 2px;
    border: 1px solid var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    box-shadow:
      inset 1px 1px 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset 2px 0 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset -2px -2px 0 var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    user-select: none;
    font-family: "px sans nouveaux", sans-serif;
    -webkit-font-smoothing: none;
  }

  .mu-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill) repeat-x;
    cursor: default;
  }
  .mu-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl) no-repeat;
  }
  .mu-title {
    position: absolute;
    left: 50%;
    top: 0;
    transform: translateX(-50%);
    height: 20px;
    display: flex;
    align-items: center;
    box-sizing: border-box;
    line-height: 1;
    padding: 0 10px 5px;
    font-size: 7px;
    letter-spacing: 1px;
    white-space: nowrap;
    color: var(--skin-titletext, var(--skin-genexhdrtext, #cdd6ea));
    background: var(--skin-gentitle, transparent) repeat-x;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.55);
    z-index: 1;
  }
  .mu-close {
    position: absolute;
    right: 0;
    top: 0;
    width: 15px;
    height: 20px;
    background: var(--skin-gentr) no-repeat;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 2;
  }

  .mu-bar {
    display: flex;
    gap: 4px;
    padding: 3px 4px 4px;
  }
  .mu-search {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 11px;
    padding: 2px 5px;
    color: var(--hi);
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16;
    outline: none;
    user-select: text;
  }
  .mu-btn {
    font: inherit;
    font-size: 10px;
    padding: 2px 8px;
    color: var(--skin-titletext, var(--skin-genexbtntext, #fff));
    background: color-mix(in srgb, var(--frame) 88%, #fff);
    border: 1px solid;
    border-color: color-mix(in srgb, var(--frame) 60%, #fff)
      color-mix(in srgb, var(--frame) 50%, #000) color-mix(in srgb, var(--frame) 50%, #000)
      color-mix(in srgb, var(--frame) 60%, #fff);
    cursor: pointer;
    white-space: nowrap;
  }
  .mu-btn:active,
  .mu-btn.on {
    background: color-mix(in srgb, var(--frame) 70%, #000);
    border-color: color-mix(in srgb, var(--frame) 50%, #000)
      color-mix(in srgb, var(--frame) 60%, #fff) color-mix(in srgb, var(--frame) 60%, #fff)
      color-mix(in srgb, var(--frame) 50%, #000);
  }

  .mu-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0 4px;
    padding: 6px;
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--fg) 40%, transparent) transparent;
  }
  .mu-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .mu-skin {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 3px;
    background: none;
    border: 1px solid transparent;
    cursor: pointer;
    color: var(--fg);
    font: inherit;
    font-size: 10px;
    text-align: left;
  }
  .mu-skin:hover {
    border-color: color-mix(in srgb, var(--fg) 45%, transparent);
  }
  .mu-skin.applied {
    border-color: var(--hi);
  }
  .mu-skin.busy {
    opacity: 0.55;
    cursor: progress;
  }
  .mu-skin img {
    width: 100%;
    aspect-ratio: 275 / 116;
    display: block;
    /* screenshots are shown a little smaller than 1:1: smooth, not blocky */
    image-rendering: auto;
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .mu-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mu-skin.applied .mu-name {
    color: var(--hi);
  }
  .mu-msg {
    padding: 14px 4px;
    font-size: 11px;
    font-style: italic;
    color: color-mix(in srgb, var(--fg) 65%, transparent);
    text-align: center;
  }
  .mu-more {
    display: block;
    margin: 10px auto 4px;
  }

  .mu-footer {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 2px;
    font-size: 10px;
    color: var(--skin-titletext, var(--skin-genexhdrtext, #cdd6ea));
  }
  .mu-status {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mu-link {
    font: inherit;
    font-size: 10px;
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
