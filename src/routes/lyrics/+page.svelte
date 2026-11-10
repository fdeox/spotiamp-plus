<script>
  import { invoke } from "@tauri-apps/api/core";
  import { Menu } from "@tauri-apps/api/menu";
  import { onMount, tick } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable, makeSnappingResizer } from "$lib/window-docking.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";
  import GenFrame from "../../GenFrame.svelte";

  // The main window's keys work here too (lib/shortcuts.js).
  onMount(() => forwardShortcuts());

  // Text size, right-click: small (the pixel font's own 8 px, crisp), medium
  // (0.7.6's size, the default) or large (one whole pixel step up, crisp).
  const SIZE_KEY = "lyrics-size";
  const SIZES = /** @type {const} */ (["small", "medium", "large"]);
  let size = $state(/** @type {(typeof SIZES)[number]} */ ("medium"));
  try {
    const saved = localStorage.getItem(SIZE_KEY);
    if (saved && SIZES.includes(/** @type {any} */ (saved))) size = /** @type {any} */ (saved);
  } catch {}

  /** @param {MouseEvent} e */
  async function onMenu(e) {
    e.preventDefault();
    /** @param {string} text @param {(typeof SIZES)[number]} value */
    const pick = (text, value) => ({
      text,
      checked: size === value,
      action: () => {
        size = value;
        try {
          localStorage.setItem(SIZE_KEY, value);
        } catch {}
      },
    });
    try {
      const menu = await Menu.new({
        items: [{ text: "Text size", items: [pick("Small", "small"), pick("Medium", "medium"), pick("Large", "large")] }],
      });
      await menu.popup();
    } catch (err) {
      invoke("log_frontend_error", { window: "lyrics", message: `lyrics menu: ${err}` }).catch(() => {});
    }
  }

  let lines = $state(/** @type {{ time_ms: number, text: string }[]} */ ([]));
  let synced = $state(false);
  let provider = $state("");
  // idle | loading | ok | none
  let status = $state("idle");

  /** The song shown: its Spotify uri, or the key of an own song or a radio
   * song (lyrics from LRCLIB). @type {string | null} */
  let curUri = null;
  /** A radio song can't be jumped in. */
  let seekable = $state(true);
  /** Whether a radio song's start is known (not for the one playing at
   * tune-in, until a click on the line being sung). */
  let inTime = $state(true);
  /** A radio song (its words stop at the song's end: a station can be slow to
   * name the next one, or never does). */
  let live = $state(false);
  /** How long the song the words were timed to is, 0 = not known. */
  let durationMs = $state(0);
  let posMs = $state(0);
  let playing = false;
  let anchorMs = 0;
  let anchorAt = 0;
  /** @type {HTMLElement | undefined} */
  let listEl;

  // a radio song that has played to its end while the station still names
  // it: its words stop there, back at the top
  const over = $derived(live && inTime && durationMs > 0 && posMs > durationMs + 3000);

  // last line whose start time has passed (only meaningful for synced lyrics)
  const activeIndex = $derived.by(() => {
    if (!synced || !inTime || over || status !== "ok") return -1;
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
    if (!listEl) return;
    if (over) listEl.scrollTo({ top: 0, behavior: "smooth" });
    if (i < 0) return;
    const el = listEl.querySelector(`[data-i="${i}"]`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  });

  /**
   * @param {string | null} uri a Spotify song's
   * @param {{ key: string, artist: string, title: string, durationMs: number } | null} song
   *   an own song or a radio song, by its name
   */
  async function loadLyrics(uri, song) {
    const key = uri ?? song?.key ?? null;
    if (!key) {
      status = "none";
      lines = [];
      return;
    }
    status = "loading";
    lines = [];
    try {
      const data = uri
        ? await invoke("get_lyrics", { uri })
        : await invoke("get_lyrics_lrclib", { artist: song?.artist, title: song?.title, durationMs: song?.durationMs ?? 0 });
      if (key !== curUri) return; // switched away mid-fetch
      lines = data.lines;
      synced = data.synced;
      provider = data.provider;
      durationMs = data.duration_ms ?? 0;
      status = lines.length ? "ok" : "none";
    } catch (e) {
      if (key === curUri) {
        lines = [];
        status = "none";
      }
    }
  }

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(275, 232);
    // Reopen at the size it was last left (falls back to the default above).
    invoke("get_window_inner_size", { label: "lyrics" })
      .then((s) => {
        if (s) REACTIVE_WINDOW_SIZE.setSize(s.width, s.height);
      })
      .catch(() => {});

    // smooth interpolation between the player's 1 s position ticks
    const timer = setInterval(() => {
      if (playing) posMs = anchorMs + (performance.now() - anchorAt);
    }, 150);

    /** @type {(() => void) | undefined} */
    let unsub;
    subscribeToWindowEvent("lyrics", (e) => {
      anchorMs = e.positionMs;
      anchorAt = performance.now();
      playing = e.playing;
      if (!playing) posMs = e.positionMs;
      inTime = !e.song?.live || Boolean(e.song?.timed);
      live = Boolean(e.song?.live);
      const key = e.uri ?? e.song?.key ?? null;
      if (key !== curUri) {
        curUri = key;
        seekable = !e.song?.live;
        loadLyrics(e.uri, e.song ?? null);
      }
    }).then((u) => (unsub = u));

    return () => {
      clearInterval(timer);
      unsub?.();
    };
  });

  const close = () => invoke("set_lyrics_window_visible", { visible: false });

  // Click a line of synced lyrics to jump the song there; a radio song's,
  // to say it's the one being sung now (the player times the words from
  // there). The position moves here straight away so the highlight follows
  // without waiting for the player's next tick.
  /** @param {number} timeMs */
  function jumpTo(timeMs) {
    if (!synced || !curUri) return;
    emitWindowEvent("lyricsSeek", { uri: curUri, positionMs: timeMs });
    anchorMs = timeMs;
    anchorAt = performance.now();
    posMs = timeMs;
    inTime = true;
  }

  /** @param {HTMLElement} element */
  function makeLyricsDraggable(element) {
    makeDockedDraggable(element, "lyrics", "lyricsWindow");
  }

  // Resize from the bottom-right corner, like the playlist / library / visualizer.
  /** @param {HTMLElement} element */
  function makeLyricsResizable(element) {
    makeSnappingResizer(
      element,
      "lyrics",
      (e) => {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        return {
          width: Math.max(Math.round(e.clientX / zoom) + 3, 200),
          height: Math.max(Math.round(e.clientY / zoom) + 3, 140),
        };
      },
      ({ width, height }) => REACTIVE_WINDOW_SIZE.setSize(Math.round(width), Math.round(height)),
      () => REACTIVE_WINDOW_SIZE.zoom || 1,
    );
  }
</script>

<div class="lyr" data-gen-window>
  <GenFrame title="Lyrics" onclose={close} drag={makeLyricsDraggable} resize={makeLyricsResizable} />

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="lyr-body {size}"
    class:unsynced={status === "ok" && !synced}
    bind:this={listEl}
    oncontextmenu={onMenu}
  >
    {#if status === "idle"}
      <div class="lyr-msg">Waiting for a track…</div>
    {:else if status === "loading"}
      <div class="lyr-msg">Loading lyrics…</div>
    {:else if status === "none"}
      <div class="lyr-msg">No lyrics for this track.</div>
    {:else}
      {#if synced && !inTime}
        <div class="lyr-hint">Click the line being sung to follow along</div>
      {:else if over}
        <div class="lyr-hint">The song's over; the station hasn't named the next one yet</div>
      {/if}
      {#each lines as line, i}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div
          class="lyr-line"
          class:active={i === activeIndex}
          class:past={synced && i < activeIndex}
          class:seekable={synced}
          data-i={i}
          title={synced ? (seekable ? "Click to jump here" : "Out of time? Click the line being sung") : undefined}
          onclick={() => jumpTo(line.time_ms)}
        >
          {line.text || "♪"}
        </div>
      {/each}
      {#if provider}
        <div class="lyr-provider">lyrics by {provider}</div>
      {/if}
    {/if}
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

  .lyr {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* Plugin-window rule, same as the library and visualizer: GENEX palette
       from a loaded .wsz, PLEDIT as the fallback. The text area below sits
       inset on it so the window reads as framed instead of running its
       background straight to the edge. */
    /* Frame follows the skin: the border and bevel derive from the window
       colour instead of a fixed blue, so on a gold or grey skin the edges no
       longer clash with the titlebar. */
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    /* GEN.BMP's frame (GenFrame) behind the insides, which keep to its room */
    background: #000;
    box-sizing: border-box;
    padding: 20px 8px 14px 11px;
    isolation: isolate;
    user-select: none;
  }


  .lyr-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    /* Inset so the window background shows as a frame on three sides, and
       recessed the way the playlist and library panes are. */
    margin: 0;
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
    padding: 8px 10px;
    text-align: center;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 11px;
    -webkit-font-smoothing: none;
    line-height: 1.55;
    scrollbar-width: none;
  }
  .lyr-body.small {
    font-size: calc(8px * var(--px-crisp, 1));
    line-height: 1.75;
  }
  .lyr-body.large {
    font-size: calc(8px * var(--px-crisp-up, 2));
    line-height: 1.5;
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
  .lyr-line.seekable {
    cursor: pointer;
  }
  .lyr-line.seekable:hover {
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 80%, transparent);
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
    font-size: calc(8px * var(--px-crisp, 1));
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 35%, transparent);
  }

  .lyr-hint {
    margin-bottom: 10px;
    font-size: calc(8px * var(--px-crisp, 1));
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 60%, transparent);
  }

  /* bottom-right resize grip */
</style>
