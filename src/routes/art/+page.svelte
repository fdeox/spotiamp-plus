<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable, makeSnappingResizer } from "$lib/window-docking.svelte.js";

  // idle | loading | ok | none
  let status = $state("idle");
  let artUrl = $state("");
  let title = $state("");
  let artist = $state("");

  let curUri = null;

  async function loadArt(uri) {
    if (!uri) {
      status = "none";
      artUrl = "";
      title = "";
      artist = "";
      return;
    }
    status = "loading";
    try {
      const meta = /** @type {any} */ (await invoke("get_track_metadata", { uri }));
      if (uri !== curUri) return; // switched away mid-fetch
      artUrl = meta?.albumArt ?? "";
      title = meta?.name ?? "";
      artist = meta?.artist ?? "";
      status = artUrl ? "ok" : "none";
    } catch (e) {
      if (uri === curUri) {
        artUrl = "";
        status = "none";
      }
    }
  }

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(275, 275);
    REACTIVE_WINDOW_SIZE.setZoom(1);
    // Reopen at the size it was last left (falls back to the default above).
    invoke("get_window_inner_size", { label: "art" })
      .then((s) => {
        if (s) REACTIVE_WINDOW_SIZE.setSize(s.width, s.height);
      })
      .catch(() => {});

    let unsub;
    subscribeToWindowEvent("art", (e) => {
      if (e.uri !== curUri) {
        curUri = e.uri;
        loadArt(e.uri);
      }
    }).then((u) => (unsub = u));

    return () => unsub?.();
  });

  const close = () => invoke("set_art_window_visible", { visible: false });

  function makeArtDraggable(element) {
    makeDockedDraggable(element, "art", "artWindow");
  }

  // Resize from the bottom-right corner, like the lyrics / library / visualizer.
  function makeArtResizable(element) {
    makeSnappingResizer(
      element,
      "art",
      (e) => {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        return {
          width: Math.max(Math.round(e.clientX / zoom) + 3, 120),
          height: Math.max(Math.round(e.clientY / zoom) + 3, 140),
        };
      },
      ({ width, height }) => REACTIVE_WINDOW_SIZE.setSize(Math.round(width), Math.round(height)),
      () => REACTIVE_WINDOW_SIZE.zoom || 1,
    );
  }
</script>

<div class="art">
  <div class="art-titlebar" use:makeArtDraggable>
    <div class="art-tl"></div>
    <span class="art-title">ALBUM ART</span>
    <button class="art-close" data-no-drag onclick={close} aria-label="Close"
    ></button>
  </div>

  <div class="art-body">
    {#if status === "idle"}
      <div class="art-msg">Waiting for a track…</div>
    {:else if status === "loading"}
      <div class="art-msg">Loading…</div>
    {:else if status === "none"}
      <div class="art-msg">No cover art.</div>
    {:else}
      <img
        class="art-img"
        src={artUrl}
        alt={artist && title ? `${artist} — ${title}` : "Album cover"}
        onerror={() => (status = "none")}
      />
    {/if}
  </div>

  <div class="art-resize" use:makeArtResizable></div>
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

  .art {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* Same skin-following plugin frame as the lyrics / library windows. */
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    background: var(--frame);
    box-sizing: border-box;
    padding: 0 2px 2px;
    border: 1px solid var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    box-shadow:
      inset 1px 1px 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset 2px 0 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset -2px -2px 0 var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    user-select: none;
  }

  .art-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill) repeat-x;
    cursor: default;
  }
  .art-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl) no-repeat;
  }
  .art-title {
    position: absolute;
    left: 50%;
    top: 0;
    transform: translateX(-50%);
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    line-height: 1;
    padding: 0 10px 5px;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    letter-spacing: 1px;
    color: var(--skin-titletext, var(--skin-genexhdrtext, #cdd6ea));
    background: var(--skin-gentitle, transparent) repeat-x;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.55);
    z-index: 1;
  }
  .art-close {
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

  .art-body {
    flex: 1;
    min-height: 0;
    margin: 0 4px 4px;
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .art-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    /* Covers are photos, not pixel art: undo the global pixelated rule so the
       image scales smoothly to the window. */
    image-rendering: auto;
    display: block;
  }

  .art-msg {
    font-family: "px sans nouveaux", sans-serif;
    font-size: 11px;
    -webkit-font-smoothing: none;
    font-style: italic;
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 55%, transparent);
  }

  /* bottom-right resize grip */
  .art-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    z-index: 3;
    background: linear-gradient(
      135deg,
      transparent 0 8px,
      color-mix(in srgb, var(--skin-plnormal, #00ff41) 45%, transparent) 8px 9px,
      transparent 9px 11px,
      color-mix(in srgb, var(--skin-plnormal, #00ff41) 45%, transparent) 11px 12px,
      transparent 12px
    );
  }
</style>
