<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";

  // The on-screen display (osd.rs): the song that just started, for a few
  // seconds, then it fades and hides itself. It never takes focus.

  /** @typedef {{title: string, artist: string, art: string | null, seq: number}} Card */

  /** @type {Card | null} */
  let card = $state(null);
  let artOk = $state(true);
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let hideTimer;

  /** @param {Card} c */
  function present(c) {
    if (card && card.seq === c.seq) return;
    card = c;
    artOk = true;
    // (no fade: the window isn't see-through, so only its contents would fade)
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => invoke("osd_hide").catch(() => {}), 4000);
  }

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(300, 72);
    // The first show's event can come before this page is listening.
    invoke("osd_current")
      .then((c) => c && present(/** @type {Card} */ (c)))
      .catch(() => {});
    const off = getCurrentWindow().listen("osdShow", (e) => present(/** @type {Card} */ (e.payload)));
    return () => {
      off.then((f) => f()).catch(() => {});
      clearTimeout(hideTimer);
    };
  });
</script>

<div class="osd">
  {#if card}
    <div class="osd-art">
      {#if card.art && artOk}
        <img src={card.art} alt="" onerror={() => (artOk = false)} />
      {:else}
        <span>♪</span>
      {/if}
    </div>
    <div class="osd-text">
      <div class="osd-label">NOW PLAYING</div>
      <div class="osd-title">{card.title}</div>
      {#if card.artist}<div class="osd-artist">{card.artist}</div>{/if}
    </div>
  {/if}
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
    background: var(--skin-plbg, #000);
  }

  .osd {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    box-sizing: border-box;
    padding: 6px 10px 6px 7px;
    /* the same skin-following bevel as the plugin windows */
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    background: var(--skin-plbg, #000);
    border: 2px solid var(--frame);
    box-shadow:
      inset 1px 1px 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset -1px -1px 0 color-mix(in srgb, var(--frame) 50%, #000);
    color: var(--skin-plnormal, #00ff41);
    user-select: none;
    cursor: default;
  }

  .osd-art {
    flex: 0 0 56px;
    width: 56px;
    height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--skin-plnormal, #00ff41) 12%, transparent);
    font-size: 26px;
  }
  .osd-art img {
    width: 56px;
    height: 56px;
    object-fit: cover;
    /* a photo, not pixel art */
    image-rendering: auto;
    display: block;
  }

  .osd-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .osd-label {
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    letter-spacing: 1px;
    -webkit-font-smoothing: none;
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 65%, transparent);
  }
  .osd-title,
  .osd-artist {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: Arial, sans-serif;
  }
  .osd-title {
    font-size: 13px;
    font-weight: bold;
    color: var(--skin-plcurrent, #fff);
  }
  .osd-artist {
    font-size: 11px;
  }
</style>
