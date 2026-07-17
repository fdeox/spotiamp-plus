<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";

  let playlists = $state([]);
  let loading = $state(true);
  let error = $state("");
  let search = $state("");

  let selectedUri = $state(null);
  // when true the right pane shows Spotify search results instead of a playlist
  let searchMode = $state(false);
  let searchQuery = $state("");
  let tracks = $state([]);
  let trackUris = $state([]);
  let tracksLoading = $state(false);
  let tracksError = $state("");
  let loadToken = 0;

  // tree UI state
  let expandLocal = $state(true);
  let expandPlaylists = $state(true);
  // which tree item is active: "search" | "liked" | "list:<name>" | a playlist uri
  let activeNode = $state(null);
  // selected row in the track list (for the Play / Enqueue buttons)
  let selectedTrack = $state(-1);
  // width of the left tree pane (px), draggable via the splitter
  let treeWidth = $state(150);

  let searchInput;

  // Winamp-style scrollbar: the native one is hidden and this input range
  // (PLEDIT sprite thumb, same as the playlist window) drives scrollTop.
  let rowsEl = $state();
  let scrollPos = $state(0);
  let scrollMax = $state(0);
  function syncScroll() {
    if (!rowsEl) return;
    scrollMax = Math.max(0, rowsEl.scrollHeight - rowsEl.clientHeight);
    scrollPos = rowsEl.scrollTop;
  }
  $effect(() => {
    tracks.length; // re-measure whenever rows are added
    tick().then(syncScroll);
  });

  // Drag the splitter between the tree and the content pane.
  function makeSplitter(element) {
    element.onpointerdown = (event) => {
      event.preventDefault();
      element.setPointerCapture(event.pointerId);
      const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
      document.onpointermove = (e) => {
        const w = Math.round(e.clientX / zoom) - 4;
        treeWidth = Math.max(90, Math.min(w, REACTIVE_WINDOW_SIZE.width - 140));
      };
      document.onpointerup = () => {
        document.onpointermove = null;
        element.releasePointerCapture(event.pointerId);
      };
    };
  }

  onMount(async () => {
    REACTIVE_WINDOW_SIZE.setSize(500, 380);
    REACTIVE_WINDOW_SIZE.setZoom(1);
    loadSavedListsLib();
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
    activeNode = pl.uri;
    selectedTrack = -1;
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

  // Load the user's Spotify "Liked Songs" into the right pane (newest first).
  async function selectLiked() {
    searchMode = false;
    selectedUri = null;
    activeNode = "liked";
    selectedTrack = -1;
    tracks = [];
    trackUris = [];
    tracksError = "";
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const ids = await invoke("get_liked_songs");
      if (token !== loadToken) return;
      trackUris = ids;
      await loadTrackMetas(ids, token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }

  // Double-click the Favorite Songs node: send the whole collection to the
  // player (just the uris — no need to wait for names to resolve).
  async function loadLikedIntoMain() {
    try {
      const ids = await invoke("get_liked_songs");
      if (ids.length) {
        emitWindowEvent("playerWindow", { UrlsDropped: ids.map(trackUrl) });
      }
    } catch {
      /* ignore */
    }
  }

  // Search the Spotify catalogue (Enter in the search box); results land in the
  // right pane and play like any other track.
  async function doSearch() {
    const q = search.trim();
    if (!q) return;
    searchMode = true;
    activeNode = "search";
    searchQuery = q;
    selectedUri = null;
    selectedTrack = -1;
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

  async function focusSearch() {
    activeNode = "search";
    searchMode = true;
    await tick();
    searchInput?.focus();
  }

  function clearPane() {
    activeNode = null;
    searchMode = false;
    selectedUri = null;
    tracks = [];
    trackUris = [];
    tracksError = "";
    selectedTrack = -1;
    ++loadToken;
  }

  function clearSearch() {
    search = "";
    if (searchMode) clearPane();
  }

  // Reuse the existing "UrlsDropped" event the main playlist window already
  // listens for (clear + load). Works cross-window via Tauri's global emit.
  const loadPlaylistIntoMain = (pl) =>
    emitWindowEvent("playerWindow", { UrlsDropped: [playlistUrl(pl.uri)] });

  // Double-clicking a track in the right pane:
  //  - search results  → append just that one track
  //  - playlist tracks → play it and queue the rest of that playlist
  function loadTrackIntoMain(index) {
    selectedTrack = index;
    if (searchMode) {
      emitWindowEvent("playerWindow", {
        UrlsAppended: [trackUrl(trackUris[index])],
      });
    } else {
      emitWindowEvent("playerWindow", {
        UrlsDropped: trackUris.slice(index).map(trackUrl),
      });
    }
  }

  // Bottom-bar buttons operate on the selected row (falling back to the first).
  function playSelected() {
    const i = selectedTrack >= 0 ? selectedTrack : 0;
    if (!trackUris[i]) return;
    emitWindowEvent("playerWindow", { UrlsDropped: [trackUrl(trackUris[i])] });
  }
  function enqueueSelected() {
    const i = selectedTrack >= 0 ? selectedTrack : 0;
    if (!trackUris[i]) return;
    emitWindowEvent("playerWindow", { UrlsAppended: [trackUrl(trackUris[i])] });
  }
  function playAll() {
    if (trackUris.length === 0) return;
    emitWindowEvent("playerWindow", { UrlsDropped: trackUris.map(trackUrl) });
  }

  // Add the selected track to an app-local list (kept in Spotiamp+, not Spotify).
  let savedLists = $state([]);
  let showListMenu = $state(false);
  let newLibListName = $state("");
  async function loadSavedListsLib() {
    try {
      savedLists = await invoke("get_saved_lists");
    } catch {
      savedLists = [];
    }
  }
  function toggleListMenu() {
    showListMenu = !showListMenu;
    if (showListMenu) loadSavedListsLib();
  }
  function selectedUriString() {
    const i = selectedTrack >= 0 ? selectedTrack : 0;
    return trackUris[i] || null;
  }
  async function addSelectedToList(name) {
    const uri = selectedUriString();
    if (!uri) return;
    await invoke("add_to_list", { name, uri }).catch(() => {});
    showListMenu = false;
  }
  async function createListWithSelected() {
    const name = newLibListName.trim();
    if (!name) return;
    await addSelectedToList(name);
    newLibListName = "";
    await loadSavedListsLib();
  }

  // Show a saved app-local list's tracks in the right pane (its uris are already
  // concrete tracks, so no expansion needed).
  async function selectSavedList(list) {
    searchMode = false;
    selectedUri = null;
    activeNode = "list:" + list.name;
    selectedTrack = -1;
    tracks = [];
    trackUris = [];
    tracksError = "";
    tracksLoading = true;
    const token = ++loadToken;
    try {
      trackUris = list.uris;
      await loadTrackMetas(list.uris, token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }
  function loadListIntoMain(list) {
    if (list.uris.length) {
      emitWindowEvent("playerWindow", { UrlsDropped: list.uris.map(trackUrl) });
    }
  }
  async function deleteSavedList(name) {
    await invoke("delete_list", { name }).catch(() => {});
    await loadSavedListsLib();
    if (activeNode === "list:" + name) {
      tracks = [];
      trackUris = [];
      activeNode = null;
    }
  }

  function fmt(ms) {
    const s = Math.round(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }

  const headTitle = $derived(
    activeNode === "liked"
      ? "Favorite Songs"
      : activeNode?.startsWith?.("list:")
        ? activeNode.slice(5)
        : searchMode
          ? `Search: ${searchQuery}`
          : selectedUri
            ? playlists.find((p) => p.uri === selectedUri)?.name ?? "Audio"
            : "Spotiamp+",
  );

  const close = () => invoke("set_library_window_visible", { visible: false });

  // Drag the library window, snapping to any other open window.
  function makeLibraryDraggable(element) {
    makeDockedDraggable(element, "library", "libraryWindow");
  }

  // Resize from the bottom-right corner, like the Winamp playlist.
  function makeLibraryResizable(element) {
    element.onpointerdown = function (event) {
      event.preventDefault();
      element.setPointerCapture(event.pointerId);
      document.onpointermove = function (e) {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        const width = Math.max(Math.round(e.clientX / zoom) + 3, 380);
        const height = Math.max(Math.round(e.clientY / zoom) + 3, 260);
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

<div class="ml-window">
  <div class="ml-titlebar" use:makeLibraryDraggable>
    <div class="ml-tl"></div>
    <span class="ml-title">WINAMP LIBRARY</span>
    <button class="ml-close" data-no-drag onclick={close} aria-label="Close"
    ></button>
  </div>

  <div class="ml-body">
    <!-- left: navigation tree -->
    <div class="ml-tree" style="flex-basis: {treeWidth}px;">
      <div
        class="ml-node ml-root"
        class:active={activeNode === "liked"}
        role="button"
        tabindex="0"
        title="your Spotify Liked Songs — double-click to load all"
        onclick={selectLiked}
        ondblclick={loadLikedIntoMain}
        onkeydown={(e) => e.key === "Enter" && selectLiked()}
      >
        <span class="ml-ic ml-ic-fav"></span>Favorite Songs
      </div>

      <div
        class="ml-node ml-root"
        role="button"
        tabindex="0"
        title="your Spotiamp+ lists"
        onclick={() => {
          expandLocal = !expandLocal;
          if (expandLocal) loadSavedListsLib();
        }}
        onkeydown={(e) => e.key === "Enter" && (expandLocal = !expandLocal)}
      >
        <span class="ml-tw">{expandLocal ? "▾" : "▸"}</span>
        <span class="ml-ic ml-ic-media"></span>Spotiamp+
      </div>
      {#if expandLocal}
        {#if savedLists.length === 0}
          <div class="ml-node ml-child ml-dim">no lists yet</div>
        {:else}
          {#each savedLists as list}
            <div
              class="ml-node ml-child ml-listnode"
              class:active={activeNode === "list:" + list.name}
              role="button"
              tabindex="0"
              title="double-click to load into the player"
              onclick={() => selectSavedList(list)}
              ondblclick={() => loadListIntoMain(list)}
              onkeydown={(e) => e.key === "Enter" && selectSavedList(list)}
            >
              <span class="ml-ic ml-ic-list"></span>{list.name}
              <span class="ml-listcount">({list.uris.length})</span>
              <button
                class="ml-listdel"
                title="delete list"
                onclick={(e) => {
                  e.stopPropagation();
                  deleteSavedList(list.name);
                }}>×</button
              >
            </div>
          {/each}
        {/if}
      {/if}

      <div
        class="ml-node ml-root"
        role="button"
        tabindex="0"
        onclick={() => (expandPlaylists = !expandPlaylists)}
        onkeydown={(e) =>
          e.key === "Enter" && (expandPlaylists = !expandPlaylists)}
      >
        <span class="ml-tw">{expandPlaylists ? "▾" : "▸"}</span>
        <span class="ml-ic ml-ic-pl"></span>Playlists
      </div>
      {#if expandPlaylists}
        {#if loading}
          <div class="ml-node ml-child ml-dim">loading…</div>
        {:else if error}
          <div class="ml-node ml-child ml-err">{error}</div>
        {:else if playlists.length === 0}
          <div class="ml-node ml-child ml-dim">no playlists</div>
        {:else}
          {#each playlists as pl}
            <div
              class="ml-node ml-child"
              class:active={activeNode === pl.uri}
              role="button"
              tabindex="0"
              title="double-click to load into the player"
              onclick={() => selectPlaylist(pl)}
              ondblclick={() => loadPlaylistIntoMain(pl)}
              onkeydown={(e) => e.key === "Enter" && selectPlaylist(pl)}
            >
              <span class="ml-ic ml-ic-list"></span>{pl.name}
            </div>
          {/each}
        {/if}
      {/if}

      <div
        class="ml-node ml-root"
        class:active={activeNode === "search"}
        role="button"
        tabindex="0"
        onclick={focusSearch}
        onkeydown={(e) => e.key === "Enter" && focusSearch()}
      >
        <span class="ml-ic ml-ic-search"></span>Search
      </div>
    </div>

    <!-- draggable splitter -->
    <div class="ml-splitter" use:makeSplitter></div>

    <!-- right: search bar + column list -->
    <div class="ml-content">
      <div class="ml-searchbar">
        <span class="ml-search-label">Search:</span>
        <input
          class="ml-search"
          bind:this={searchInput}
          placeholder="artist, song… (Enter)"
          bind:value={search}
          onkeydown={(e) => e.key === "Enter" && doSearch()}
        />
        <button class="ml-clear" onclick={clearSearch}>Clear</button>
      </div>

      <div class="ml-view-head">{headTitle}</div>

      <div class="ml-cols">
        <div class="ml-col ml-c-artist">Artist</div>
        <div class="ml-col ml-c-album">Album</div>
        <div class="ml-col ml-c-title">Title</div>
        <div class="ml-col ml-c-time">Time</div>
      </div>

      <div class="ml-listwrap">
      <div class="ml-rows" bind:this={rowsEl} onscroll={syncScroll}>
        {#if activeNode === "audio" && !searchMode && !selectedUri}
          <div class="ml-hint">
            Select a playlist on the left, or search Spotify above.
          </div>
        {:else}
          {#each tracks as t, i}
            <div
              class="ml-row"
              class:sel={selectedTrack === i}
              class:odd={i % 2 === 1}
              role="button"
              tabindex="0"
              onclick={() => (selectedTrack = i)}
              ondblclick={() => loadTrackIntoMain(i)}
              onkeydown={(e) => e.key === "Enter" && loadTrackIntoMain(i)}
            >
              <div class="ml-col ml-c-artist">{t.artist}</div>
              <div class="ml-col ml-c-album">{t.album}</div>
              <div class="ml-col ml-c-title">{t.name}</div>
              <div class="ml-col ml-c-time">{fmt(t.duration)}</div>
            </div>
          {/each}
          {#if tracksLoading}
            <div class="ml-hint">{searchMode ? "searching…" : "loading…"}</div>
          {:else if tracksError}
            <div class="ml-hint ml-err">{tracksError}</div>
          {:else if tracks.length === 0}
            <div class="ml-hint">{searchMode ? "no results" : "empty"}</div>
          {/if}
        {/if}
      </div>
      <input
        type="range"
        class="ml-scroll"
        min="0"
        max={scrollMax}
        step="1"
        value={scrollPos}
        oninput={(e) => rowsEl && (rowsEl.scrollTop = +e.currentTarget.value)}
        aria-label="Scroll tracks"
      />
      </div>
    </div>
  </div>

  <div class="ml-footer">
    <button class="ml-btn" onclick={playSelected}>Play</button>
    <button class="ml-btn" onclick={enqueueSelected}>Enqueue</button>
    <button class="ml-btn" onclick={playAll}>Play all</button>
    <div class="ml-listwrap">
      <button class="ml-btn" onclick={toggleListMenu} title="add the selected track to a list">
        + List
      </button>
      {#if showListMenu}
        <div class="ml-listbackdrop" onpointerdown={() => (showListMenu = false)}></div>
        <div class="ml-listmenu">
          <div class="ml-listnewrow">
            <input
              class="ml-listinput"
              bind:value={newLibListName}
              placeholder="new list…"
              onkeydown={(e) => e.key === "Enter" && createListWithSelected()}
            />
            <button class="ml-listadd" onclick={createListWithSelected}>+</button>
          </div>
          {#if savedLists.length === 0}
            <div class="ml-listempty">no lists yet</div>
          {/if}
          {#each savedLists as list}
            <button class="ml-listitem" onclick={() => addSelectedToList(list.name)}>
              {list.name}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <span class="ml-count">
      {tracks.length}
      {tracks.length === 1 ? "item" : "items"}
    </span>
  </div>

  <div class="ml-resize" use:makeLibraryResizable></div>
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

  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }

  .ml-window {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* colours follow real Winamp plugin-window rules: GENEX.BMP palette when
       a .wsz is loaded (fallback PLEDIT.TXT), classic green-on-black otherwise */
    background: var(--skin-genexwndbg, var(--skin-plbg, #000));
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #2a2f3a, inset -1px -1px 0 #0e0f16;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    font-smooth: never;
    letter-spacing: 0.3px;
    color: var(--skin-genexwndtext, var(--skin-plnormal, rgb(0, 255, 0)));
    user-select: none;
  }

  /* ---- titlebar (gen tiles) ---- */
  .ml-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill) repeat-x;
    cursor: default;
  }
  .ml-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl) no-repeat;
  }
  .ml-title {
    position: absolute;
    left: 50%;
    top: 4px;
    transform: translateX(-50%);
    height: 11px;
    display: flex;
    align-items: center;
    padding: 0 6px;
    /* match the tiny bitmap lettering of the playlist titlebar */
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    letter-spacing: 1px;
    color: #cdd6ea;
    background: #26264a;
    z-index: 1;
  }
  .ml-close {
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

  /* ---- body: tree | content ---- */
  .ml-body {
    flex: 1;
    display: flex;
    min-height: 0;
    padding: 3px;
    gap: 3px;
  }

  .ml-tree {
    flex: 0 0 150px;
    overflow: auto;
    background: var(--skin-genexitembg, var(--skin-plbg, #000));
    border: 1px solid var(--skin-genexdivider, #060a12);
    padding: 2px 0;
  }
  .ml-node {
    display: flex;
    align-items: center;
    height: 15px;
    padding: 0 4px 0 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--skin-genexitemfg, var(--skin-plnormal, rgb(0, 230, 0)));
    cursor: default;
  }
  .ml-root {
    color: var(--skin-genexitemfg, var(--skin-plnormal, rgb(0, 255, 0)));
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }
  .ml-child {
    padding-left: 18px;
  }
  .ml-node:hover {
    background: color-mix(
      in srgb,
      var(--skin-genexselbg, var(--skin-plselbg, #0f4a1a)) 45%,
      transparent
    );
  }
  .ml-node.active {
    background: var(--skin-genexselbg, var(--skin-plselbg, rgb(0, 0, 198)));
    color: var(--skin-plcurrent, #fff);
  }
  .ml-dim {
    color: color-mix(
      in srgb,
      var(--skin-genexitemfg, var(--skin-plnormal, rgb(0, 120, 0))) 55%,
      transparent
    );
    font-style: italic;
  }
  .ml-tw {
    display: inline-block;
    width: 9px;
    color: var(--skin-genexitemfg, var(--skin-plnormal, rgb(0, 200, 0)));
  }
  .ml-ic {
    display: inline-block;
    width: 12px;
    height: 12px;
    margin-right: 4px;
    flex: 0 0 12px;
    background-repeat: no-repeat;
    background-position: center;
  }
  /* tiny CSS glyph icons — drawn with currentColor so they follow the
     skin's list text colour automatically */
  .ml-ic-media {
    background: color-mix(in srgb, currentColor 25%, transparent);
    border: 1px solid color-mix(in srgb, currentColor 60%, transparent);
    border-radius: 1px;
    height: 8px;
    margin-top: 1px;
  }
  .ml-ic-audio {
    background: currentColor;
    border-radius: 50%;
    width: 7px;
    height: 7px;
    margin-left: 2px;
    margin-right: 6px;
    opacity: 0.9;
  }
  .ml-ic-fav {
    background: none;
    color: #e0455e;
    font-size: 11px;
    line-height: 12px;
    text-align: center;
  }
  .ml-ic-fav::before {
    content: "♥";
  }
  .ml-ic-pl {
    background: color-mix(in srgb, currentColor 35%, transparent);
    height: 8px;
    border: 1px solid color-mix(in srgb, currentColor 60%, transparent);
    border-radius: 1px;
    margin-top: 1px;
  }
  .ml-ic-list {
    background: color-mix(in srgb, currentColor 20%, transparent);
    width: 8px;
    height: 6px;
    border-top: 2px solid color-mix(in srgb, currentColor 75%, transparent);
    border-bottom: 2px solid color-mix(in srgb, currentColor 75%, transparent);
    margin-left: 2px;
    margin-right: 6px;
  }
  .ml-ic-search {
    background: transparent;
    border: 1.5px solid color-mix(in srgb, currentColor 75%, transparent);
    border-radius: 50%;
    width: 7px;
    height: 7px;
  }

  /* ---- right content ---- */
  .ml-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .ml-searchbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 0 3px 0;
  }
  .ml-search-label {
    color: var(--skin-genexwndtext, var(--skin-plnormal, rgb(0, 210, 0)));
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .ml-search {
    flex: 1;
    min-width: 0;
    background: var(--skin-genexitembg, #05170a);
    color: var(--skin-genexitemfg, #00ff41);
    border: 1px solid var(--skin-genexdivider, #1e6b32);
    font-family: "Consolas", monospace;
    font-size: 11px;
    padding: 1px 4px;
    outline: none;
  }
  /* the skin's real GENEX button face (base skin included) */
  .ml-clear {
    background: none;
    border: 4px solid transparent;
    border-image: var(--skin-genexbtn) 4 fill / 4px stretch;
    color: var(--skin-genexbtntext, #393942);
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    text-transform: uppercase;
    padding: 1px 5px;
    cursor: pointer;
  }
  .ml-clear:active {
    border-image: var(--skin-genexbtnp) 4 fill / 4px stretch;
  }

  .ml-view-head {
    font-size: 7px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--skin-genexwndtext, var(--skin-plnormal, rgb(0, 200, 0)));
    padding: 2px 3px;
    border-bottom: 1px solid var(--skin-genexdivider, #0a0f0a);
  }

  .ml-cols,
  .ml-row {
    display: flex;
    align-items: center;
  }
  .ml-cols {
    height: 15px;
    background: var(--skin-genexhdrbg, #0a0d12);
    border-bottom: 1px solid var(--skin-genexdivider, #1e6b32);
    color: var(--skin-genexhdrtext, rgb(0, 210, 0));
    font-size: 7px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .ml-col {
    padding: 0 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ml-cols .ml-col {
    border-right: 1px solid var(--skin-genexdivider, #0e2214);
    height: 15px;
    line-height: 15px;
  }
  .ml-c-artist {
    flex: 0 0 26%;
  }
  .ml-c-album {
    flex: 0 0 26%;
  }
  .ml-c-title {
    flex: 1;
    min-width: 0;
  }
  .ml-c-time {
    flex: 0 0 44px;
    text-align: right;
  }

  .ml-rows {
    flex: 1;
    overflow-y: scroll;
    overflow-x: hidden;
    background: var(--skin-genexitembg, var(--skin-plbg, #000));
    border: 1px solid var(--skin-genexdivider, #060a12);
    min-height: 0;
  }
  .ml-row {
    height: 13px;
    color: var(--skin-genexitemfg, var(--skin-plnormal, rgb(0, 255, 0)));
    cursor: default;
  }
  .ml-row.odd {
    background: rgba(255, 255, 255, 0.03);
  }
  .ml-row:hover {
    background: color-mix(
      in srgb,
      var(--skin-genexselbg, var(--skin-plselbg, #0f4a1a)) 45%,
      transparent
    );
  }
  .ml-row.sel {
    background: var(--skin-genexselbg, var(--skin-plselbg, rgb(0, 0, 198)));
    color: var(--skin-plcurrent, #fff);
  }
  .ml-hint {
    padding: 8px 10px;
    color: color-mix(
      in srgb,
      var(--skin-genexitemfg, var(--skin-plnormal, rgb(0, 130, 0))) 60%,
      transparent
    );
    font-style: italic;
  }
  .ml-err {
    color: #ff6b6b;
  }

  /* the real scrollbar is the PLEDIT-sprite slider next to the list */
  .ml-listwrap {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .ml-listwrap .ml-rows {
    flex: 1;
    -ms-overflow-style: none;
    scrollbar-width: none;
  }
  .ml-rows::-webkit-scrollbar {
    display: none;
  }
  .ml-scroll {
    cursor: url(/src/static/assets/skins/base-2.91/EQSLID.CUR), default;
    writing-mode: vertical-lr;
    direction: ltr;
    appearance: none;
    width: 10px;
    flex: 0 0 10px;
    background: var(--skin-genexwndbg, #050a05);
    box-shadow: inset 1px 0 0 var(--skin-genexdivider, #0e1a0e);
  }
  .ml-scroll::-webkit-slider-thumb {
    background: var(--skin-pledit);
    appearance: none;
    width: 8px;
    height: 18px;
    background-position: -52px -53px;
  }
  .ml-scroll::-webkit-slider-thumb:active {
    background-position-x: -61px;
  }
  .ml-tree::-webkit-scrollbar {
    width: 9px;
  }
  .ml-tree::-webkit-scrollbar-track {
    background: var(--skin-genexwndbg, #050a05);
  }
  .ml-tree::-webkit-scrollbar-thumb {
    background: var(--skin-genexhdrbg, #16241a);
    border: 1px solid var(--skin-genexdivider, #0a0f0a);
  }

  /* ---- footer ---- */
  .ml-footer {
    flex: 0 0 22px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 5px;
    border-top: 1px solid var(--skin-genexdivider, #0a1a0a);
    background: var(--skin-genexwndbg, #050a05);
  }
  .ml-btn {
    background: none;
    border: 4px solid transparent;
    border-image: var(--skin-genexbtn) 4 fill / 4px stretch;
    color: var(--skin-genexbtntext, #393942);
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 1px 7px;
    cursor: pointer;
  }
  .ml-btn:active {
    border-image: var(--skin-genexbtnp) 4 fill / 4px stretch;
  }
  .ml-count {
    margin-left: auto;
    color: var(--skin-genexwndtext, var(--skin-plnormal, rgb(0, 170, 0)));
    padding-right: 8px;
  }

  /* "+ List" popup — add the selected track to an app-local list */
  .ml-listwrap {
    position: relative;
    display: inline-flex;
  }
  .ml-listbackdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .ml-listmenu {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 3px;
    z-index: 41;
    min-width: 130px;
    max-height: 180px;
    overflow-y: auto;
    background: #1a1c22;
    border: 1px solid var(--skin-genexdivider, #3a3d4a);
    padding: 3px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.5);
  }
  .ml-listnewrow {
    display: flex;
    gap: 3px;
    margin-bottom: 3px;
  }
  .ml-listinput {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    background: #0c0d12;
    color: #d8d8e8;
    border: 1px solid #3a3d4a;
    padding: 1px 3px;
  }
  .ml-listadd {
    width: 18px;
    border: 1px solid #3a3d4a;
    background: #2a2d3a;
    color: #d8d8e8;
    cursor: pointer;
  }
  .ml-listitem {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: var(--skin-plnormal, #c8c8d4);
    font-size: 11px;
    padding: 2px 5px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ml-listitem:hover {
    background: var(--skin-genexselbg, #2b6fd6);
    color: #fff;
  }
  .ml-listempty {
    color: #808080;
    font-size: 10px;
    padding: 2px 5px;
    font-style: italic;
  }

  /* saved-list rows in the Spotiamp+ tree node */
  .ml-listnode {
    display: flex;
    align-items: center;
  }
  .ml-listcount {
    margin-left: 4px;
    font-size: 10px;
    color: color-mix(in srgb, currentColor 50%, transparent);
  }
  .ml-listdel {
    margin-left: auto;
    width: 14px;
    border: none;
    background: transparent;
    color: #c0504a;
    cursor: pointer;
    opacity: 0;
  }
  .ml-listnode:hover .ml-listdel {
    opacity: 1;
  }
  .ml-listdel:hover {
    color: #ff6b60;
  }

  /* ---- splitter between tree and content ---- */
  .ml-splitter {
    flex: 0 0 4px;
    cursor: ew-resize;
    background: transparent;
    box-shadow: inset 1px 0 0 var(--skin-genexdivider, #1e6b32);
  }
  .ml-splitter:hover {
    background: color-mix(
      in srgb,
      var(--skin-genexdivider, #123a1a) 40%,
      transparent
    );
  }

  .ml-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    background: linear-gradient(
      135deg,
      transparent 0 8px,
      var(--skin-genexdivider, #1e6b32) 8px 9px,
      transparent 9px 11px,
      var(--skin-genexdivider, #1e6b32) 11px 12px,
      transparent 12px
    );
  }
</style>
