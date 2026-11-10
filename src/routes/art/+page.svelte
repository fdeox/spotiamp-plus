<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable, makeSnappingResizer } from "$lib/window-docking.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";
  import GenFrame from "../../GenFrame.svelte";

  // The main window's keys work here too (lib/shortcuts.js).
  onMount(() => forwardShortcuts());

  // idle | loading | ok | none
  let status = $state("idle");
  let artUrl = $state("");
  let title = $state("");
  let artist = $state("");

  /** @type {string | null} */
  let curUri = null;

  /** @param {string | null} uri */
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

  /**
   * Free Mode: the cover the Spotify app gives Windows for its song. The app
   * can name a new song a moment before it has the new picture, so it's asked
   * a few times over the first seconds and the last answer stays.
   * @param {string} key
   * @param {{ artist: string, title: string }} song
   */
  async function loadAppArt(key, song) {
    status = "loading";
    artUrl = "";
    title = song.title;
    artist = song.artist;
    for (const wait of [0, 700, 1500, 3000]) {
      if (wait) await new Promise((r) => setTimeout(r, wait));
      if (key !== curUri) return;
      const cover = /** @type {any} */ (await invoke("smtc_cover").catch(() => null));
      if (key !== curUri) return;
      if (cover?.data_url && cover.title === song.title && cover.artist === song.artist) {
        artUrl = cover.data_url;
        status = "ok";
      }
    }
    if (!artUrl) status = "none";
  }

  onMount(() => {
    // a 256 px cover in GEN.BMP's frame
    REACTIVE_WINDOW_SIZE.setSize(275, 290);
    // Reopen at the size it was last left (falls back to the default above),
    // its box kept square (a size saved with the old, thinner frame too)
    invoke("get_window_inner_size", { label: "art" })
      .then((s) => {
        if (!s) return;
        const frame = frameSize();
        const side = Math.max(110, Math.round(s.width) - frame.w);
        REACTIVE_WINDOW_SIZE.setSize(side + frame.w, side + frame.h);
      })
      .catch(() => {});

    /** @type {(() => void) | undefined} */
    let unsub;
    subscribeToWindowEvent("art", (e) => {
      const app = !e.uri && e.appSong?.title ? e.appSong : null;
      const key = e.uri ?? (app ? `app:${app.artist} - ${app.title}` : null);
      if (key !== curUri) {
        curUri = key;
        if (app && key) loadAppArt(key, app);
        else loadArt(e.uri);
      }
    }).then((u) => (unsub = u));

    return () => unsub?.();
  });

  const close = () => invoke("set_art_window_visible", { visible: false });

  /** @param {HTMLElement} element */
  function makeArtDraggable(element) {
    makeDockedDraggable(element, "art", "artWindow");
  }

  // The cover's box is the window less the 20 px titlebar and the 4 px frame
  // (1 px border inside it): this much wider and taller than the box. In a
  // modern skin's frame, less what that frame takes (--mf-*, global.css).
  const frameSize = () => {
    // (GEN.BMP's frame: 11 px left, 8 right, 20 above, 14 below)
    if (!("modern" in document.body.dataset)) return { w: 19, h: 34 };
    const style = getComputedStyle(document.body);
    const mf = (/** @type {string} */ side) => parseFloat(style.getPropertyValue(`--mf-${side}`)) || 0;
    return { w: mf("left") + mf("right"), h: mf("top") + mf("bottom") };
  };

  // Resize from the bottom-right corner, like the lyrics / library / visualizer,
  // keeping the cover's box square: a cover fills it with no gaps at the sides.
  /** @param {HTMLElement} element */
  function makeArtResizable(element) {
    makeSnappingResizer(
      element,
      "art",
      (e) => {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        const frame = frameSize();
        const side = Math.max(Math.round(e.clientX / zoom) + 3 - frame.w, Math.round(e.clientY / zoom) + 3 - frame.h, 110);
        return { width: side + frame.w, height: side + frame.h };
      },
      ({ width, height }) => REACTIVE_WINDOW_SIZE.setSize(Math.round(width), Math.round(height)),
      () => REACTIVE_WINDOW_SIZE.zoom || 1,
    );
  }
</script>

<div class="art" data-gen-window>
  <GenFrame title="Album Art" onclose={close} drag={makeArtDraggable} resize={makeArtResizable} />

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
        alt={artist && title ? `${artist} - ${title}` : "Album cover"}
        onerror={() => (status = "none")}
      />
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

  .art {
    position: absolute;
    inset: 0;
    background: #000;
    isolation: isolate;
    user-select: none;
  }

  /* the room GEN.BMP's frame leaves, as Winamp's embedded windows had it */
  .art-body {
    position: absolute;
    inset: 20px 8px 14px 11px;
    background: #000;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  /* fills the box: a square box shows all of it; a box left a few px off
     square (an older saved size) trims a sliver rather than leave bars */
  .art-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    /* Covers are photos, not pixel art: undo the global pixelated rule so the
       image scales smoothly to the window. */
    image-rendering: auto;
    display: block;
  }

  .art-msg {
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(8px * var(--px-crisp, 1));
    -webkit-font-smoothing: none;
    font-style: italic;
    color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 55%, transparent);
  }

  :global(body[data-modern]) .art-body {
    inset: 0;
  }
</style>
