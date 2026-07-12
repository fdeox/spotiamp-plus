<script>
  import { invoke } from "@tauri-apps/api/core";
  import { Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import {
    makeTauriWindowDraggable,
    isDocked,
    rectFromPositionAndSize,
    SNAP_DISTANCE,
    snapPosition,
    STICKY_SNAP_DISTANCE,
  } from "$lib/window-docking.svelte.js";

  let playlists = $state([]);
  let loading = $state(true);
  let error = $state("");
  let search = $state("");

  let selectedUri = $state(null);
  // when true the right pane shows Spotify search results instead of a playlist
  let searchMode = $state(false);
  let searchQuery = $state("");
  let tracks = $state([]);
  // every track uri of the selected playlist (from get_track_ids, arrives in
  // one call) — used so playing a track loads the rest of the list after it
  let trackUris = $state([]);
  let tracksLoading = $state(false);
  let tracksError = $state("");
  // bumped every time we switch playlists so a slow in-flight load bails out
  let loadToken = 0;

  const filtered = $derived(
    search.trim()
      ? playlists.filter((p) =>
          p.name.toLowerCase().includes(search.toLowerCase()),
        )
      : playlists,
  );

  onMount(async () => {
    REACTIVE_WINDOW_SIZE.setSize(360, 420);
    REACTIVE_WINDOW_SIZE.setZoom(1);
    try {
      playlists = await invoke("get_user_playlists");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  const playlistUrl = (uri) =>
    `https://open.spotify.com/playlist/${uri.split(":").pop()}`;
  const trackUrl = (uri) =>
    `https://open.spotify.com/track/${uri.split(":").pop()}`;

  // Resolve names for a list of track uris, filling the right pane top-down.
  async function loadTrackMetas(ids, token) {
    for (const uri of ids) {
      if (token !== loadToken) return; // switched away, abandon
      try {
        const meta = await invoke("get_track_metadata", { uri });
        if (token !== loadToken) return;
        tracks = [...tracks, meta];
      } catch (e) {
        /* skip a track we can't read */
      }
    }
  }

  async function selectPlaylist(pl) {
    searchMode = false;
    selectedUri = pl.uri;
    tracks = [];
    trackUris = [];
    tracksError = "";
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const ids = await invoke("get_track_ids", { uri: pl.uri });
      if (token !== loadToken) return;
      trackUris = ids;
      await loadTrackMetas(ids, token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }

  // Search the Spotify catalogue (Enter in the search box); results land in the
  // right pane and play like any other track.
  async function doSearch() {
    const q = search.trim();
    if (!q) return;
    searchMode = true;
    searchQuery = q;
    selectedUri = null;
    tracks = [];
    trackUris = [];
    tracksError = "";
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const ids = await invoke("search", { query: q });
      if (token !== loadToken) return;
      trackUris = ids;
      await loadTrackMetas(ids, token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }

  // Reuse the existing "UrlsDropped" event the main playlist window already
  // listens for (clear + load). Works cross-window via Tauri's global emit.
  const loadPlaylistIntoMain = (pl) =>
    emitWindowEvent("playerWindow", { UrlsDropped: [playlistUrl(pl.uri)] });
  // Play a track *and* queue the rest of the playlist after it, so playback
  // keeps going instead of stopping on that one song.
  const loadTrackIntoMain = (index) =>
    emitWindowEvent("playerWindow", {
      UrlsDropped: trackUris.slice(index).map(trackUrl),
    });

  function fmt(ms) {
    const s = Math.round(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }

  // hide via the Rust command (app commands aren't capability-gated, so this is
  // reliable regardless of window permissions)
  const close = () => invoke("set_library_window_visible", { visible: false });

  // Drag the library window, snapping to the player like the playlist does.
  // The DragStarted/DragEnded events drive the Rust docking (native follow +
  // snap-on-release); mapPosition adds the live visual snap while dragging.
  function makeLibraryDraggable(element) {
    makeTauriWindowDraggable(element, {
      async onStart({ startPosition, windowSize }) {
        const playerWindow = await Window.getByLabel("player");
        if (!playerWindow) return false;
        await emitWindowEvent("libraryWindow", { DragStarted: null });
        const [playerPosition, playerSize] = await Promise.all([
          playerWindow.outerPosition(),
          playerWindow.outerSize(),
        ]);
        const playerRect = rectFromPositionAndSize(playerPosition, playerSize);
        return {
          playerRect,
          librarySize: windowSize,
          docked: isDocked(
            rectFromPositionAndSize(startPosition, windowSize),
            playerRect,
          ),
        };
      },
      mapPosition(rawPosition, context) {
        const rawRect = {
          ...rawPosition,
          width: context.librarySize.width,
          height: context.librarySize.height,
        };
        const snapDistance = context.docked
          ? STICKY_SNAP_DISTANCE
          : SNAP_DISTANCE;
        const snappedPosition = snapPosition(
          rawRect,
          context.playerRect,
          snapDistance,
        );
        context.docked = snappedPosition !== undefined;
        return snappedPosition ?? rawPosition;
      },
      async onEnd() {
        await emitWindowEvent("libraryWindow", { DragEnded: null });
      },
    });
  }

  // Resize from the bottom-right corner, like the Winamp playlist. Updating
  // REACTIVE_WINDOW_SIZE makes the layout effect resize the OS window; the panes
  // are flexbox so they reflow to fill it.
  function makeLibraryResizable(element) {
    element.onpointerdown = function (event) {
      event.preventDefault();
      element.setPointerCapture(event.pointerId);
      document.onpointermove = function (e) {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        const width = Math.max(Math.round(e.clientX / zoom) + 3, 240);
        const height = Math.max(Math.round(e.clientY / zoom) + 3, 220);
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

<div class="lib-window">
  <div class="lib-titlebar" use:makeLibraryDraggable>
    <div class="lib-tl"></div>
    <span class="lib-title">LIBRARY</span>
    <button
      class="lib-close"
      onpointerdown={(e) => e.stopPropagation()}
      onclick={close}
      aria-label="Close"
    ></button>
  </div>

  <div class="lib-body">
    <!-- left: playlists -->
    <div class="lib-pane lib-playlists">
      <div class="lib-pane-head">PLAYLISTS</div>
      <input
        class="lib-search"
        placeholder="filter · Enter = search Spotify"
        bind:value={search}
        onkeydown={(e) => e.key === "Enter" && doSearch()}
      />
      <div class="lib-list">
        {#if loading}
          <div class="lib-msg">loading playlists…</div>
        {:else if error}
          <div class="lib-msg lib-err">{error}</div>
        {:else if filtered.length === 0}
          <div class="lib-msg">no playlists</div>
        {:else}
          {#each filtered as pl}
            <button
              class="lib-row"
              class:selected={selectedUri === pl.uri}
              onclick={() => selectPlaylist(pl)}
              ondblclick={() => loadPlaylistIntoMain(pl)}
              title="double-click to load &amp; play"
            >
              <span class="lib-row-name">{pl.name}</span>
              <span class="lib-row-count">{pl.track_count}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>

    <!-- right: tracks of the selected playlist, or search results -->
    <div class="lib-pane lib-tracks">
      <div class="lib-pane-head">
        {searchMode ? `SEARCH: ${searchQuery}` : "TRACKS"}
      </div>
      <div class="lib-list">
        {#if !selectedUri && !searchMode}
          <div class="lib-msg">← select a playlist<br />or search Spotify ↑</div>
        {:else}
          {#each tracks as t, i}
            <button
              class="lib-row"
              ondblclick={() => loadTrackIntoMain(i)}
              title="double-click to play from here"
            >
              <span class="lib-row-idx">{i + 1}.</span>
              <span class="lib-row-name">{t.artist} - {t.name}</span>
              <span class="lib-row-count">{fmt(t.duration)}</span>
            </button>
          {/each}
          {#if tracksLoading}
            <div class="lib-msg">
              {searchMode ? "searching…" : "loading tracks…"}
            </div>
          {:else if tracksError}
            <div class="lib-msg lib-err">{tracksError}</div>
          {:else if tracks.length === 0}
            <div class="lib-msg">{searchMode ? "no results" : "empty"}</div>
          {/if}
        {/if}
      </div>
    </div>
  </div>

  <div class="lib-footer">
    double-click a playlist to load · a track to play it
  </div>

  <div class="lib-resize" use:makeLibraryResizable></div>
</div>

<style>
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }

  .lib-window {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* dark frame toned to sit next to the PLEDIT playlist window */
    background: #1c1d26;
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #34384a, inset -1px -1px 0 #0e0f16;
    font-family: "Segoe UI", Tahoma, sans-serif;
    color: #c9d2e0;
    user-select: none;
  }

  /* ---- title bar: authentic pieces cropped from GEN.BMP ---- */
  .lib-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: url(/src/static/assets/skins/base-2.91/gen-tiles/gen_fill.png)
      repeat-x;
    cursor: default;
  }
  .lib-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: url(/src/static/assets/skins/base-2.91/gen-tiles/gen_tl.png)
      no-repeat;
  }
  /* the title sits in a dark "notch" over the gold bar, like real Winamp */
  .lib-title {
    position: absolute;
    left: 50%;
    top: 3px;
    transform: translateX(-50%);
    height: 14px;
    display: flex;
    align-items: center;
    padding: 0 8px;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 2px;
    color: #cdd6ea;
    background: #26264a;
    z-index: 1;
  }
  .lib-close {
    position: absolute;
    right: 0;
    top: 0;
    width: 15px;
    height: 20px;
    background: url(/src/static/assets/skins/base-2.91/gen-tiles/gen_tr.png)
      no-repeat;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 2;
  }

  /* ---- body: two panes ---- */
  .lib-body {
    flex: 1;
    display: flex;
    gap: 3px;
    padding: 3px;
    min-height: 0;
  }
  .lib-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    /* without min-width:0 the long (nowrap) playlist names force this flex
       item wider than its basis, collapsing the tracks pane to zero width */
    min-width: 0;
    background: #000;
    border: 1px solid #10121a;
    box-shadow: inset 1px 1px 0 #171922;
  }
  .lib-playlists {
    flex: 0 0 45%;
  }
  .lib-tracks {
    flex: 1;
  }
  .lib-pane-head {
    font-size: 9px;
    letter-spacing: 1px;
    color: #7f8aa3;
    padding: 2px 5px;
    background: #14151d;
    border-bottom: 1px solid #0a0b10;
  }

  .lib-search {
    margin: 3px 4px;
    padding: 2px 5px;
    background: #05170a;
    border: 1px solid #1e6b32;
    color: #00ff41;
    font-family: monospace;
    font-size: 11px;
    outline: none;
  }
  .lib-search::placeholder {
    color: #3f7a4e;
  }

  .lib-list {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .lib-row {
    display: flex;
    align-items: baseline;
    gap: 5px;
    width: 100%;
    text-align: left;
    padding: 1px 5px;
    background: transparent;
    border: none;
    /* match the playlist window: bright green on black, blue selection */
    color: rgb(0, 255, 0);
    font-family: monospace;
    font-size: 11px;
    line-height: 15px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
  }
  .lib-row:hover {
    background: #0a1f0f;
  }
  .lib-row.selected {
    background: rgb(0, 0, 198);
    color: #fff;
  }
  .lib-row-idx {
    color: rgb(0, 160, 0);
    flex: 0 0 auto;
  }
  .lib-row-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lib-row-count {
    flex: 0 0 auto;
    color: rgb(0, 160, 0);
  }
  .lib-row.selected .lib-row-idx,
  .lib-row.selected .lib-row-count {
    color: #cfe0ff;
  }

  .lib-msg {
    padding: 6px 8px;
    font-family: monospace;
    font-size: 11px;
    color: #6a7488;
  }
  .lib-err {
    color: #d06a6a;
    white-space: normal;
  }

  /* resize grip, bottom-right corner (like the playlist) */
  .lib-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    z-index: 20;
  }

  /* ---- footer ---- */
  .lib-footer {
    height: 16px;
    display: flex;
    align-items: center;
    padding: 0 6px;
    font-size: 9px;
    color: #7f8aa3;
    background: #14151d;
    border-top: 1px solid #0a0b10;
  }

  /* chunky beveled Winamp-style scrollbar */
  .lib-list::-webkit-scrollbar {
    width: 9px;
  }
  .lib-list::-webkit-scrollbar-track {
    background: #05070a;
  }
  .lib-list::-webkit-scrollbar-thumb {
    background: #1f2630;
    border: 1px solid #0a0c10;
    box-shadow: inset 1px 1px 0 #3a4350, inset -1px -1px 0 #0e1116;
  }
  .lib-list::-webkit-scrollbar-thumb:hover {
    background: #262f3b;
  }
</style>
