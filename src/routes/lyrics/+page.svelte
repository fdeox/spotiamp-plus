<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";

  let lines = $state([]);
  let synced = $state(false);
  let provider = $state("");
  // idle | loading | ok | none
  let status = $state("idle");

  let curUri = null;
  let posMs = $state(0);
  let playing = false;
  let anchorMs = 0;
  let anchorAt = 0;
  let listEl;

  // last line whose start time has passed (only meaningful for synced lyrics)
  const activeIndex = $derived.by(() => {
    if (!synced || status !== "ok") return -1;
    let idx = -1;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].time_ms <= posMs + 250) idx = i;
      else break;
    }
    return idx;
  });

  // keep the active line centred
  $effect(() => {
    const i = activeIndex;
    if (i < 0 || !listEl) return;
    const el = listEl.querySelector(`[data-i="${i}"]`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  });

  async function loadLyrics(uri) {
    if (!uri) {
      status = "none";
      lines = [];
      return;
    }
    status = "loading";
    lines = [];
    try {
      const data = await invoke("get_lyrics", { uri });
      if (uri !== curUri) return; // switched away mid-fetch
      lines = data.lines;
      synced = data.synced;
      provider = data.provider;
      status = lines.length ? "ok" : "none";
    } catch (e) {
      if (uri === curUri) {
        lines = [];
        status = "none";
      }
    }
  }

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(275, 232);
    REACTIVE_WINDOW_SIZE.setZoom(1);

    // smooth interpolation between the player's 1 s position ticks
    const timer = setInterval(() => {
      if (playing) posMs = anchorMs + (performance.now() - anchorAt);
    }, 150);

    let unsub;
    subscribeToWindowEvent("lyrics", (e) => {
      anchorMs = e.positionMs;
      anchorAt = performance.now();
      playing = e.playing;
      if (!playing) posMs = e.positionMs;
      if (e.uri !== curUri) {
        curUri = e.uri;
        loadLyrics(e.uri);
      }
    }).then((u) => (unsub = u));

    return () => {
      clearInterval(timer);
      unsub?.();
    };
  });

  const close = () => invoke("set_lyrics_window_visible", { visible: false });

  function makeLyricsDraggable(element) {
    makeDockedDraggable(element, "lyrics", "lyricsWindow");
  }

  // Resize from the bottom-right corner, like the playlist / library / visualizer.
  function makeLyricsResizable(element) {
    element.onpointerdown = function (event) {
      event.preventDefault();
      element.setPointerCapture(event.pointerId);
      document.onpointermove = function (e) {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        const width = Math.max(Math.round(e.clientX / zoom) + 3, 200);
        const height = Math.max(Math.round(e.clientY / zoom) + 3, 140);
        REACTIVE_WINDOW_SIZE.setSize(width, height);
      };
      document.onpointerup = function () {
        document.onpointermove = null;
        element.releasePointerCapture(event.pointerId);
      };
    };
    element.onselectstart = () => false;
  }
</script>

<div class="lyr">
  <div class="lyr-titlebar" use:makeLyricsDraggable>
    <div class="lyr-tl"></div>
    <span class="lyr-title">LYRICS</span>
    <button class="lyr-close" data-no-drag onclick={close} aria-label="Close"
    ></button>
  </div>

  <div
    class="lyr-body"
    class:unsynced={status === "ok" && !synced}
    bind:this={listEl}
  >
    {#if status === "idle"}
      <div class="lyr-msg">Waiting for a track…</div>
    {:else if status === "loading"}
      <div class="lyr-msg">Loading lyrics…</div>
    {:else if status === "none"}
      <div class="lyr-msg">No lyrics for this track.</div>
    {:else}
      {#each lines as line, i}
        <div
          class="lyr-line"
          class:active={i === activeIndex}
          class:past={synced && i < activeIndex}
          data-i={i}
        >
          {line.text || "♪"}
        </div>
      {/each}
      {#if provider}
        <div class="lyr-provider">lyrics by {provider}</div>
      {/if}
    {/if}
  </div>

  <div class="lyr-resize" use:makeLyricsResizable></div>
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

  .lyr {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #2a2f3a, inset -1px -1px 0 #0e0f16;
    user-select: none;
  }

  .lyr-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill) repeat-x;
    cursor: default;
  }
  .lyr-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl) no-repeat;
  }
  .lyr-title {
    position: absolute;
    left: 50%;
    top: 4px;
    transform: translateX(-50%);
    height: 11px;
    display: flex;
    align-items: center;
    padding: 0 6px;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    letter-spacing: 1px;
    color: #cdd6ea;
    background: #26264a;
    z-index: 1;
  }
  .lyr-close {
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

  .lyr-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 8px 10px;
    text-align: center;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 11px;
    -webkit-font-smoothing: none;
    line-height: 1.55;
    scrollbar-width: none;
  }
  .lyr-body::-webkit-scrollbar {
    display: none;
  }

  .lyr-line {
    color: color-mix(
      in srgb,
      var(--skin-plnormal, #00ff41) 42%,
      transparent
    );
    transition: color 0.2s;
    padding: 1px 0;
  }
  .lyr-line.past {
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 30%, transparent);
  }
  .lyr-line.active {
    color: var(--skin-plcurrent, #fff);
    text-shadow: 0 0 5px
      color-mix(in srgb, var(--skin-plnormal, #00ff41) 60%, transparent);
    font-weight: 700;
  }
  /* unsynced lyrics: no timing, so every line is shown at full colour */
  .lyr-body.unsynced .lyr-line {
    color: var(--skin-plnormal, #00ff41);
  }

  .lyr-msg {
    margin-top: 40%;
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 55%, transparent);
    font-style: italic;
  }
  .lyr-provider {
    margin-top: 14px;
    font-size: 8px;
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 35%, transparent);
  }

  /* bottom-right resize grip */
  .lyr-resize {
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
