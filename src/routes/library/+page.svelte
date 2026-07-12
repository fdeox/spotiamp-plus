<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { makeTauriWindowDraggable } from "$lib/window-docking.svelte.js";

  let playlists = $state([]);
  let loading = $state(true);
  let error = $state("");
  let search = $state("");

  let selectedUri = $state(null);
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

  async function selectPlaylist(pl) {
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
      for (const uri of ids) {
        if (token !== loadToken) return; // switched playlist, abandon
        try {
          const meta = await invoke("get_track_metadata", { uri });
          if (token !== loadToken) return;
          tracks = [...tracks, meta];
        } catch (e) {
          /* skip a track we can't read */
        }
      }
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
</script>

<div class="lib-window">
  <div class="lib-titlebar" use:makeTauriWindowDraggable>
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
      <input class="lib-search" placeholder="search…" bind:value={search} />
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

    <!-- right: tracks of the selected playlist -->
    <div class="lib-pane lib-tracks">
      <div class="lib-pane-head">TRACKS</div>
      <div class="lib-list">
        {#if !selectedUri}
          <div class="lib-msg">← select a playlist</div>
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
            <div class="lib-msg">loading tracks…</div>
          {:else if tracksError}
            <div class="lib-msg lib-err">{tracksError}</div>
          {:else if tracks.length === 0}
            <div class="lib-msg">empty</div>
          {/if}
        {/if}
      </div>
    </div>
  </div>

  <div class="lib-footer">
    double-click a playlist to load · a track to play it
  </div>
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
    /* Winamp gen-window frame palette (matches the gold titlebar tiles) */
    background: #2b2b47;
    border: 1px solid #12121f;
    box-shadow: inset 1px 1px 0 #56567c, inset -1px -1px 0 #191930;
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
    background: #1a1d27;
    border-bottom: 1px solid #10121a;
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
    color: #12d012;
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
    background: #0b2f6b;
    color: #d8e4ff;
  }
  .lib-row-idx {
    color: #0a7a0a;
    flex: 0 0 auto;
  }
  .lib-row-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lib-row-count {
    flex: 0 0 auto;
    color: #0a7a0a;
  }
  .lib-row.selected .lib-row-idx,
  .lib-row.selected .lib-row-count {
    color: #9fb6e6;
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

  /* ---- footer ---- */
  .lib-footer {
    height: 16px;
    display: flex;
    align-items: center;
    padding: 0 6px;
    font-size: 9px;
    color: #7f8aa3;
    background: #1a1d27;
    border-top: 1px solid #10121a;
  }

  /* chunky Winamp-ish scrollbars */
  .lib-list::-webkit-scrollbar {
    width: 8px;
  }
  .lib-list::-webkit-scrollbar-track {
    background: #0a0c12;
  }
  .lib-list::-webkit-scrollbar-thumb {
    background: #3a4260;
    border: 1px solid #10121a;
  }
</style>
