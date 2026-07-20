<script>
  import {
    enterExitViewport,
    range,
    handleDrop,
    handleError,
    REACTIVE_WINDOW_SIZE,
  } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { emit } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { Playlist } from "$lib/playlist.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { ask, message } from "@tauri-apps/plugin-dialog";

  /** @type {{data: import('./$types').PageData}} */
  const { data: playlistSettings } = $props();

  function applyInitialWindowSize() {
    if (!playlistSettings.window_state.inner_size) {
      return;
    }

    const { width, height } = playlistSettings.window_state.inner_size;
    REACTIVE_WINDOW_SIZE.setSize(width, height);
  }

  // Controller ("free") mode: no librespot session exists, so loading the
  // saved tracks (each needs session metadata) would only produce errors —
  // the playlist starts empty and the saved URIs stay untouched in settings
  // for when the user returns to Premium mode.
  const controllerMode = playlistSettings.controller_mode === true;

  function createInitialPlaylist() {
    return new Playlist(controllerMode ? [] : playlistSettings.uris, !controllerMode);
  }

  applyInitialWindowSize();
  const playlist = createInitialPlaylist();

  // --- "my playlists" library browser (our addition) ---
  let showLibrary = $state(false);
  let libraryLoading = $state(false);
  let libraryError = $state("");
  let libraryPlaylists = $state([]);
  let librarySearch = $state("");
  const filteredPlaylists = $derived(
    librarySearch.trim()
      ? libraryPlaylists.filter((p) =>
          p.name.toLowerCase().includes(librarySearch.toLowerCase()),
        )
      : libraryPlaylists,
  );

  // Opens the standalone Library window (built on demand in Rust).
  const openLibraryWindow = () =>
    invoke("set_library_window_visible", { visible: true });

  // m:ss for the bottom-bar time readouts
  function fmtTime(ms) {
    const s = Math.floor((ms || 0) / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }

  // --- right-click menu (skins + shortcuts) ---
  const SKINS = ["classic", "cherry", "amber", "emerald"];
  let menu = $state({ show: false, x: 0, y: 0 });
  let currentSkin = $state("classic");
  invoke("get_skin")
    .then((s) => (currentSkin = s || "classic"))
    .catch(() => {});

  // --- in-app updates ---
  // The manifest and the downloaded package are both signature-checked against
  // the public key baked into tauri.conf.json, so a tampered update is refused.
  let updateBusy = $state(false);
  async function checkForUpdates() {
    if (updateBusy) return;
    updateBusy = true;
    try {
      const update = await check();
      if (!update) {
        await message("You're running the latest version.", {
          title: "Spotiamp+",
          kind: "info",
        });
        return;
      }
      const wanted = await ask(
        `Spotiamp+ ${update.version} is available (you have ${update.currentVersion}).\n\nDownload and install it now?`,
        { title: "Update available", kind: "info" },
      );
      if (!wanted) return;
      await update.downloadAndInstall();
      const restart = await ask(
        "Update installed. Restart Spotiamp+ now to finish?",
        { title: "Spotiamp+", kind: "info" },
      );
      if (restart) await relaunch();
    } catch (e) {
      await message(`Couldn't check for updates.\n\n${e}`, {
        title: "Spotiamp+",
        kind: "error",
      });
    } finally {
      updateBusy = false;
      closeMenu();
    }
  }

  // --- always on top ---
  let alwaysOnTop = $state(false);
  async function loadAlwaysOnTop() {
    try {
      const settings = await invoke("get_player_settings");
      alwaysOnTop = Boolean(settings?.always_on_top);
    } catch {
      alwaysOnTop = false;
    }
  }
  async function toggleAlwaysOnTop() {
    closeMenu();
    alwaysOnTop = !alwaysOnTop;
    await invoke("set_always_on_top", { active: alwaysOnTop }).catch(() => {});
  }

  function openMenu(e) {
    e.preventDefault();
    loadAudioDevices();
    loadAlwaysOnTop();
    const mw = 200,
      mh = 220;
    menu = {
      show: true,
      x: Math.min(e.clientX, Math.max(2, window.innerWidth - mw)),
      y: Math.min(e.clientY, Math.max(2, window.innerHeight - mh)),
    };
  }

  // --- audio output device picker ---
  let audioDevices = $state([]);
  let currentAudioDevice = $state(null);
  async function loadAudioDevices() {
    try {
      const info = await invoke("list_audio_devices");
      audioDevices = info.devices || [];
      currentAudioDevice = info.current ?? null;
    } catch {
      audioDevices = [];
    }
  }
  async function pickAudioDevice(name) {
    closeMenu();
    currentAudioDevice = name;
    await invoke("set_audio_device", { device: name }).catch(() => {});
    // the player was rebuilt on the new device — ask the player window to
    // resume the current track there.
    await emit("audioDeviceChanged", {});
  }

  // --- save the current queue as an app-local list (browse them in the Library
  //     window's "Spotiamp+" tree node) ---
  let newListName = $state("");
  async function saveCurrentAsList() {
    const name = newListName.trim();
    if (!name) return;
    const uris = playlist.rows.map((r) => r.uri.asString);
    await invoke("save_list", { name, uris }).catch(() => {});
    newListName = "";
    closeMenu();
  }
  const closeMenu = () => (menu.show = false);
  let menuTab = $state("skins");
  async function openDiscord() {
    closeMenu();
    // The URL itself lives in Rust's allowlist — we only name the target.
    await invoke("open_external", { target: "discord" }).catch(() => {});
  }

  // Controller mode → Premium: forget the mode flag and relaunch into the
  // normal OAuth + librespot path.
  async function switchToPremium() {
    closeMenu();
    await invoke("leave_controller_mode").catch(() => {});
    await relaunch().catch(() => {});
  }
  async function chooseSkin(skin) {
    currentSkin = skin;
    closeMenu();
    await invoke("set_skin", { skin });
    emitWindowEvent("skinChanged", { skin });
  }
  // load a classic Winamp 2.x skin (.wsz) from disk
  async function loadWszSkin() {
    closeMenu();
    try {
      const name = await invoke("pick_and_load_skin");
      if (name === null) return; // cancelled
      currentSkin = "custom";
      emitWindowEvent("skinChanged", { skin: "custom" });
    } catch (e) {
      handleError(new Error(`Could not load skin: ${e}`));
    }
  }

  // .wsz skins shipped with the app, selectable straight from the menu
  let bundledSkins = $state([]);
  invoke("list_bundled_skins")
    .then((names) => (bundledSkins = names))
    .catch(() => {});
  const prettySkinName = (name) => name.replaceAll("_", " ");
  async function chooseBundledSkin(name) {
    closeMenu();
    try {
      await invoke("load_bundled_skin", { name });
      currentSkin = "custom";
      emitWindowEvent("skinChanged", { skin: "custom" });
    } catch (e) {
      handleError(new Error(`Could not load skin: ${e}`));
    }
  }

  async function openLibrary() {
    showLibrary = true;
    if (libraryPlaylists.length > 0) return;
    libraryLoading = true;
    libraryError = "";
    try {
      libraryPlaylists = await invoke("get_user_playlists");
    } catch (e) {
      libraryError = String(e);
    } finally {
      libraryLoading = false;
    }
  }

  async function loadPlaylist(uri) {
    showLibrary = false;
    await playlist.clear();
    // uri is "spotify:playlist:ID" but addUrls expects an open.spotify.com URL
    const id = uri.split(":").pop();
    await playlist.addUrls([`https://open.spotify.com/playlist/${id}`]);
  }


  /**
   * @param {DocumentEventMap["keydown"]} e
   */
  function preventKeyboardScrolling(e) {
    if (
      ["Space", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].indexOf(
        e.code,
      ) != -1
    ) {
      e.preventDefault();
    }
  }

  onMount(() => {
    const cleanupDropHandler = handleDrop(async (urls) => {
      await playlist.addUrls(urls);
    });

    emitWindowEvent("playlistWindow", { Ready: null });

    // Cleanups
    return () => {
      cleanupDropHandler();
      playlist.dispose();
    };
  });

  /**
   * @param {HTMLElement} element
   */
  function makeResizable(element) {
    element.onpointerdown = function (event) {
      document.onmousemove = function (event) {
        const pointerX = Math.max(
          Math.ceil(event.clientX / REACTIVE_WINDOW_SIZE.zoom / 25),
          11,
        );
        const pointerY = Math.max(
          Math.ceil(event.clientY / REACTIVE_WINDOW_SIZE.zoom / 29),
          4,
        );

        REACTIVE_WINDOW_SIZE.setSize(pointerX * 25, pointerY * 29);
        invoke("set_playlist_inner_size", {
          width: REACTIVE_WINDOW_SIZE.width,
          height: REACTIVE_WINDOW_SIZE.height,
        });
      };

      document.onmouseup = function () {
        document.onmousemove = null;

        element.releasePointerCapture(event.pointerId);
      };

      element.setPointerCapture(event.pointerId);
    };

    element.onselectstart = () => false;
  }

  /**
   * @param {HTMLElement} element
   */
  function makeWindowDraggable(element) {
    makeDockedDraggable(element, "playlist", "playlistWindow");
  }
  let scroll = $state(0);
  const PLAYLIST_ROW_HEIGHT = 14.5;
  /**
   * @type {HTMLElement | undefined}
   */
  let scrollElement = $state();
  let wheelDelta = 0;

  function scrollMax() {
    return scrollElement
      ? scrollElement.scrollHeight - scrollElement.clientHeight
      : 0;
  }

  function scrollRowHeight() {
    return PLAYLIST_ROW_HEIGHT * REACTIVE_WINDOW_SIZE.zoom;
  }

  function syncScrollThumb() {
    const max = scrollMax();
    if (scrollElement && max > 0) {
      const value = Math.min(Math.max(0, scrollElement.scrollTop), max);
      scroll = (value / max) * 100;
    } else {
      scroll = 0;
    }
  }

  /**
   * @param {number} row
   */
  function scrollToRow(row) {
    if (!scrollElement) {
      return;
    }

    scrollElement.scrollTop = Math.min(
      Math.max(0, row * scrollRowHeight()),
      scrollMax(),
    );
    syncScrollThumb();
  }

  /**
   * @param {number} offset
   */
  function scrollByRows(offset) {
    if (!scrollElement) {
      return;
    }

    scrollToRow(
      Math.round(scrollElement.scrollTop / scrollRowHeight()) + offset,
    );
  }

  /**
   * @param {WheelEvent} event
   */
  function onWheelScroll(event) {
    let delta = event.deltaY || event.deltaX;
    if (delta === 0) {
      return;
    }

    event.preventDefault();
    if (event.deltaMode == WheelEvent.DOM_DELTA_LINE) {
      delta *= scrollRowHeight();
    } else if (event.deltaMode == WheelEvent.DOM_DELTA_PAGE && scrollElement) {
      delta *= scrollElement.clientHeight;
    }

    wheelDelta += delta;
    const rows =
      wheelDelta > 0
        ? Math.floor(wheelDelta / scrollRowHeight())
        : Math.ceil(wheelDelta / scrollRowHeight());
    if (rows === 0) {
      return;
    }

    scrollByRows(rows);
    wheelDelta -= rows * scrollRowHeight();
  }

  /**
   * @param {Event} event
   */
  function onManualScroll(event) {
    if (scrollElement && event.target instanceof HTMLInputElement) {
      const targetTop = (parseInt(event.target.value, 10) / 100) * scrollMax();
      scrollToRow(Math.round(targetTop / scrollRowHeight()));
    }
  }

  // ------ Drag to reorder ------
  // Winamp-style: the selection shifts by however many rows the pointer has
  // travelled from where the drag started, regardless of which row it's over.
  const EDGE_SCROLL_ZONE = 12;
  const EDGE_SCROLL_INTERVAL_MS = 80;

  /**
   * @typedef {import('$lib/playlist.svelte').TrackRow} Row
   */

  /**
   * @type {{
   *   row: Row,
   *   startY: number,
   *   rowHeight: number,
   *   block: Row[],
   *   remaining: Row[],
   *   baseInsert: number,
   *   originalRows: Row[],
   *   appliedOffset: number,
   *   moved: boolean,
   *   pointerY: number,
   * } | undefined}
   */
  let drag;
  let isDragging = $state(false);
  /** @type {number | undefined} */
  let edgeScrollFrame;
  let lastEdgeScrollAt = 0;

  /**
   * Shift the dragged selection by `offset` rows relative to its start.
   *
   * @param {number} offset
   */
  function applyDragOffset(offset) {
    if (!drag || offset === drag.appliedOffset) {
      return;
    }
    drag.appliedOffset = offset;

    if (offset === 0) {
      // Back at the start: restore the original order verbatim (this also
      // preserves any gaps in a non-contiguous selection).
      playlist.rows = [...drag.originalRows];
    } else {
      drag.moved = true;
      isDragging = true;
      playlist.placeSelection(
        drag.block,
        drag.remaining,
        drag.baseInsert + offset,
      );
    }
  }

  /**
   * Recompute the offset from the current pointer position and apply it.
   */
  function updateDragFromPointer() {
    if (!drag) {
      return;
    }
    const offset = Math.round((drag.pointerY - drag.startY) / drag.rowHeight);
    applyDragOffset(offset);
  }

  /**
   * Continuously scroll while the pointer rests near the top/bottom edge,
   * keeping the offset consistent by shifting the drag origin as we scroll.
   */
  /**
   * @param {number} now
   */
  function edgeScrollTick(now) {
    edgeScrollFrame = undefined;
    if (!drag || !scrollElement) {
      return;
    }

    const rect = scrollElement.getBoundingClientRect();
    let delta = 0;
    if (drag.pointerY < rect.top + EDGE_SCROLL_ZONE) {
      delta = -1;
    } else if (drag.pointerY > rect.bottom - EDGE_SCROLL_ZONE) {
      delta = 1;
    }

    if (delta !== 0 && now - lastEdgeScrollAt >= EDGE_SCROLL_INTERVAL_MS) {
      const before = scrollElement.scrollTop;
      scrollByRows(delta);
      // Move the drag origin by however much we actually scrolled so the
      // pointer-to-row mapping keeps growing while held at the edge.
      drag.startY -= scrollElement.scrollTop - before;
      updateDragFromPointer();
      lastEdgeScrollAt = now;
    }

    if (delta !== 0) {
      edgeScrollFrame = requestAnimationFrame(edgeScrollTick);
    }
  }

  /**
   * @param {MouseEvent} e
   * @param {Row} row
   */
  function onRowMouseDown(e, row) {
    if (e.button !== 0) {
      return;
    }

    const ctrl = e.ctrlKey || e.metaKey;
    const shift = e.shiftKey;
    if (ctrl || shift) {
      playlist.select(row, { ctrl, shift });
      return;
    }

    // Keep an existing multi-selection intact so it can be dragged as a group;
    // a plain click that doesn't turn into a drag collapses to this row on release.
    if (!playlist.selectedRows.includes(row)) {
      playlist.select(row);
    }

    const selected = new Set(playlist.selectedRows);
    const block = playlist.rows.filter((r) => selected.has(r));
    const remaining = playlist.rows.filter((r) => !selected.has(r));
    const topIndex = Math.min(...block.map((r) => playlist.rows.indexOf(r)));
    // Where the block sits among the non-dragged rows at the start.
    const baseInsert = remaining.filter(
      (r) => playlist.rows.indexOf(r) < topIndex,
    ).length;
    const dragElement =
      row.element ??
      (e.currentTarget instanceof HTMLElement ? e.currentTarget : undefined);
    const rowHeight = dragElement?.getBoundingClientRect().height || 1;

    drag = {
      row,
      startY: e.clientY,
      rowHeight,
      block,
      remaining,
      baseInsert,
      originalRows: [...playlist.rows],
      appliedOffset: 0,
      moved: false,
      pointerY: e.clientY,
    };
    lastEdgeScrollAt = 0;
    window.addEventListener("mousemove", onDragMove);
    window.addEventListener("mouseup", onDragEnd);
  }

  /**
   * @param {MouseEvent} e
   */
  function onDragMove(e) {
    if (!drag) {
      return;
    }
    drag.pointerY = e.clientY;
    updateDragFromPointer();

    if (edgeScrollFrame === undefined) {
      edgeScrollFrame = requestAnimationFrame(edgeScrollTick);
    }
  }

  function onDragEnd() {
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragEnd);
    if (edgeScrollFrame !== undefined) {
      cancelAnimationFrame(edgeScrollFrame);
      edgeScrollFrame = undefined;
    }

    if (drag && !drag.moved) {
      // A plain click (no drag): collapse the selection to the clicked row.
      playlist.select(drag.row);
    } else if (drag) {
      // The order changed — persist the new arrangement.
      playlist.persist();
    }
    drag = undefined;
    isDragging = false;
    lastEdgeScrollAt = 0;
  }
</script>

<span
  style:--playlist-w={playlist.width}
  style:--playlist-h={playlist.height}
  style:--track-row-height={`${PLAYLIST_ROW_HEIGHT}px`}
  oncontextmenu={openMenu}
>
  <!-- our "my playlists" browser (opens a list of the user's Spotify playlists) -->
  <button class="my-playlists-btn" onclick={openLibraryWindow}>♪ library</button>
  {#if showLibrary}
    <div class="library-overlay">
      <div class="library-head">
        <span>MY PLAYLISTS</span>
        <button class="library-close" onclick={() => (showLibrary = false)}>×</button>
      </div>
      {#if libraryPlaylists.length > 0}
        <input
          class="library-search"
          type="text"
          placeholder="search playlists…"
          bind:value={librarySearch}
        />
      {/if}
      <div class="library-list">
        {#if libraryLoading}
          <div class="library-msg">
            loading your playlists…<br />
            (first open fetches each playlist's name — can take a few seconds;
            it's instant after that)
          </div>
        {:else if libraryError}
          <div class="library-msg err">
            in-app browsing isn't wired up yet — Spotify blocks the permission
            this needs (work in progress). for now: drag a playlist straight from
            the Spotify app onto this window and it loads.
          </div>
        {:else if libraryPlaylists.length === 0}
          <div class="library-msg">no playlists found</div>
        {:else if filteredPlaylists.length === 0}
          <div class="library-msg">no matches for "{librarySearch}"</div>
        {:else}
          {#each filteredPlaylists as pl}
            <button class="library-item" onclick={() => loadPlaylist(pl.uri)}>
              <span class="library-name">{pl.name}</span>
              <span class="library-count">{pl.track_count}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
  <div
    class="tracks-container"
    onkeydown={preventKeyboardScrolling}
    onwheel={onWheelScroll}
    role="scrollbar"
    tabindex="0"
    aria-controls="playlist-tracks"
    aria-valuenow={scroll}
    onscroll={syncScrollThumb}
    bind:this={scrollElement}
  >
    <table id="playlist-tracks" class:dragging={isDragging}>
      <tbody>
        {#each playlist.rows as row, index}
          <tr
            class="playlist-track"
            class:loaded={row.isLoaded()}
            class:selected={row.isSelected()}
            class:unavailable={row.unavailable}
            onmousedown={(e) => onRowMouseDown(e, row)}
            ondblclick={() => row.play()}
            use:enterExitViewport
            bind:this={row.element}
            onenterViewport={row.getOnEnterViewport()}
          >
            <td class="playlist-track-main">
              <span class="playlist-track-number">{index + 1}.&nbsp;</span>
              <span class="playlist-track-name">{row.displayName}</span>
            </td>
            <td class="playlist-track-duration">{row.displayDuration}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <input
      class="sprite scroll-bar"
      type="range"
      bind:value={scroll}
      oninput={onManualScroll}
    />
  </div>

  <!-- Top corners -->
  <div class="sprite playlist-sprite playlist-tl-sprite"></div>

  <div
    class="sprite playlist-sprite playlist-tr-sprite"
    style:--x={playlist.width}
  ></div>

  <!-- Left/Right -->
  {#each range(1, playlist.height - 2) as y}
    <div class="sprite playlist-sprite playlist-l-sprite" style:--y={y}></div>
    <div
      class="sprite playlist-sprite playlist-r-sprite"
      style:--y={y}
      style:--x={playlist.width}
    ></div>
  {/each}

  <!-- Top/Bottom -->
  {#each range(1, playlist.width - 2) as x}
    <div
      class="sprite playlist-sprite playlist-t-sprite"
      style:--x={x}
      use:makeWindowDraggable
    ></div>
    {#if x >= 5 && x < playlist.width - 6}
      <div
        class="sprite playlist-sprite playlist-b-sprite"
        style:--y={playlist.height - 1}
        style:--x={x}
      ></div>
    {/if}
  {/each}

  <!-- Title -->
  <div
    class="sprite playlist-sprite playlist-title-sprite"
    style:--x={playlist.width / 2 - 2}
    use:makeWindowDraggable
  ></div>

  <!-- Bottom corners -->
  <div
    class="sprite playlist-sprite playlist-bl-sprite"
    style:--y={playlist.height}
  ></div>

  <div
    class="sprite playlist-sprite playlist-br-sprite"
    style:--y={playlist.height - 1}
    style:--x={playlist.width - 9}
  ></div>

  <!-- transparent click overlays over the baked-in ADD/REM/SEL/MISC buttons -->
  <button
    class="pl-btn"
    style:--pl-btn-x="11px"
    onclick={openLibraryWindow}
    aria-label="Add — open library"
    title="open library"
  ></button>
  <button
    class="pl-btn"
    style:--pl-btn-x="40px"
    onclick={() => playlist.removeSelected()}
    aria-label="Remove selected"
    title="remove selected"
  ></button>
  <button
    class="pl-btn"
    style:--pl-btn-x="69px"
    onclick={() => (playlist.selectedRows = [...playlist.rows])}
    aria-label="Select all"
    title="select all"
  ></button>
  <button
    class="pl-btn"
    style:--pl-btn-x="98px"
    onclick={() => playlist.clear()}
    aria-label="Clear playlist"
    title="clear playlist"
  ></button>

  <!-- bottom-right LCD readouts over the two black areas:
       total playlist time (wide, upper) + current track elapsed (small row) -->
  <div class="pl-time pl-time-total">{fmtTime(playlist.totalDurationMs)}</div>
  <div class="pl-time pl-time-elapsed">{fmtTime(playlist.positionMs)}</div>

  <div class="draggable-corner" use:makeResizable></div>

  {#if menu.show}
    <div
      class="ctx-backdrop"
      onclick={closeMenu}
      oncontextmenu={(e) => {
        e.preventDefault();
        closeMenu();
      }}
    ></div>
    <div class="ctx-menu" style:left="{menu.x}px" style:top="{menu.y}px">
      <div class="ctx-tabs">
        <!-- controller mode: no audio pipeline of our own, so no device picker -->
        {#each (controllerMode
          ? [["skins", "Skins"], ["colors", "Colors"], ["windows", "Windows"]]
          : [["skins", "Skins"], ["colors", "Colors"], ["windows", "Windows"], ["audio", "Audio"], ["list", "List"]]) as [id, label]}
          <button
            class="ctx-tab"
            class:active={menuTab === id}
            onclick={() => (menuTab = id)}>{label}</button
          >
        {/each}
      </div>

      {#if menuTab === "skins"}
        <button class="ctx-item" onclick={() => chooseSkin("classic")}>
          <span class="ctx-dot">{currentSkin === "classic" ? "●" : ""}</span>classic
        </button>
        {#each bundledSkins as name}
          <button class="ctx-item" onclick={() => chooseBundledSkin(name)}>
            <span class="ctx-dot"></span>{prettySkinName(name)}
          </button>
        {/each}
        <button class="ctx-item" onclick={loadWszSkin}>
          <span class="ctx-dot">{currentSkin === "custom" ? "●" : ""}</span>load .wsz…
        </button>
      {:else if menuTab === "colors"}
        {#each ["cherry", "amber", "emerald"] as s}
          <button class="ctx-item" onclick={() => chooseSkin(s)}>
            <span class="ctx-dot">{currentSkin === s ? "●" : ""}</span>{s}
          </button>
        {/each}
      {:else if menuTab === "windows"}
        <!-- these three browse/render through the librespot session -->
        {#if !controllerMode}
          <button
            class="ctx-item"
            onclick={() => {
              closeMenu();
              openLibraryWindow();
            }}>Library…</button
          >
          <button
            class="ctx-item"
            onclick={() => {
              closeMenu();
              invoke("set_visualizer_window_visible", { visible: true });
            }}>Visualizer…</button
          >
          <button
            class="ctx-item"
            onclick={() => {
              closeMenu();
              invoke("set_lyrics_window_visible", { visible: true });
            }}>Lyrics…</button
          >
        {/if}
        <button
          class="ctx-item"
          onclick={() => {
            closeMenu();
            playlist.clear();
          }}>Clear playlist</button
        >
        <button class="ctx-item" onclick={toggleAlwaysOnTop}>
          <span class="ctx-dot">{alwaysOnTop ? "●" : ""}</span>Always on top
        </button>
      {:else if menuTab === "audio"}
        <button class="ctx-item" onclick={() => pickAudioDevice(null)}>
          <span class="ctx-dot">{!currentAudioDevice ? "●" : ""}</span>System default
        </button>
        {#each audioDevices as dev}
          <button class="ctx-item" title={dev} onclick={() => pickAudioDevice(dev)}>
            <span class="ctx-dot">{currentAudioDevice === dev ? "●" : ""}</span>{dev}
          </button>
        {/each}
      {:else if menuTab === "list"}
        <div class="ctx-listrow" onpointerdown={(e) => e.stopPropagation()}>
          <input
            class="ctx-listinput"
            bind:value={newListName}
            placeholder="save queue as…"
            onkeydown={(e) => e.key === "Enter" && saveCurrentAsList()}
          />
          <button class="ctx-listbtn" onclick={saveCurrentAsList}>Save</button>
        </div>
        <div class="ctx-hint">browse lists in Library ▸ Spotiamp+</div>
      {/if}

      <div class="ctx-sep"></div>
      {#if controllerMode}
        <!-- back to the OAuth + librespot path on next launch -->
        <button
          class="ctx-item"
          title="have Premium now? switch back to full streaming mode"
          onclick={switchToPremium}
        >
          ★ Premium sign-in…
        </button>
      {:else}
        <button
          class="ctx-item"
          title="when the queue ends, keep playing similar songs (Spotify radio)"
          onclick={() => (playlist.autoplay = !playlist.autoplay)}
        >
          <span class="ctx-dot">{playlist.autoplay ? "☑" : "☐"}</span>Autoplay similar
        </button>
      {/if}
      <button class="ctx-item" onclick={checkForUpdates}>
        {updateBusy ? "⏳ Checking…" : "⬆️ Check for updates"}
      </button>
      <button class="ctx-item ctx-discord" onclick={openDiscord}>
        💬 Join our Discord
      </button>
    </div>
  {/if}
</span>

<style>
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }

  .draggable-corner {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --width: 15px;
    --height: 15px;
    width: calc(var(--width) * var(--zoom));
    height: calc(var(--height) * var(--zoom));
    background-color: transparent;
    position: absolute;
    --x: var(--playlist-w);
    --y: var(--playlist-h);
    left: calc(((var(--x)) * 25px - var(--width)) * var(--zoom));
    top: calc(((var(--y)) * 29px - var(--height)) * var(--zoom));
    display: inline-block;
  }
  /* ------ TRACKS ------ */
  .tracks-container {
    /* pushed down 14px to make room for the "my playlists" button strip */
    margin-top: calc(34px * var(--zoom));
    margin-left: calc(10px * var(--zoom));
    width: calc((var(--playlist-w) * 25px - 29px) * var(--zoom));
    height: calc(
      (var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom) -
        14px * var(--zoom)
    );
    overflow-x: hidden;
    overflow-y: scroll;
  }

  /* Hide scrollbar for Chrome, Safari and Opera */
  .tracks-container::-webkit-scrollbar {
    display: none;
  }

  /* Hide scrollbar for IE, Edge and Firefox */
  .tracks-container {
    -ms-overflow-style: none; /* IE and Edge */
    scrollbar-width: none; /* Firefox */
  }

  input.scroll-bar {
    cursor: url(/src/static/assets/skins/base-2.91/EQSLID.CUR), default;
    writing-mode: vertical-lr;
    direction: ltr;
    appearance: none;
    --x: var(--playlist-w);
    --y: var(--playlist-h);
    --width: 10px;

    left: calc(((var(--x)) * 25px - var(--width)) * var(--zoom) - 5px);
    top: 20px;

    height: calc(
      (var(--playlist-h) - 2) * 2 * var(--track-row-height) * var(--zoom)
    );
    vertical-align: bottom;
    position: absolute;
    z-index: 1000;
  }

  input.scroll-bar::-webkit-slider-thumb {
    background: var(--skin-pledit);
    appearance: none;
    width: 8px;
    height: 18px;
    margin-bottom: 1px;
    background-position: -52px -53px;
  }

  input.scroll-bar::-webkit-slider-thumb:active {
    background-position-x: -61px;
  }

  #playlist-tracks {
    color: var(--skin-plnormal, rgb(0, 255, 0));
    border-collapse: collapse;
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(7px * var(--zoom));
    font-smooth: never;
    -webkit-font-smoothing: none;

    letter-spacing: calc(0.3px * var(--zoom));
    -webkit-user-select: none;
    -ms-user-select: none;
    user-select: none;
    width: 100%;
  }

  .playlist-track {
    outline: none;
    height: calc(var(--track-row-height) * var(--zoom));
  }

  #playlist-tracks.dragging .playlist-track {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), grabbing;
  }

  .playlist-track-main {
    /* The max-width:0 + width:100% combo lets the cell take the remaining
       space while still honouring text-overflow within a table layout. */
    max-width: 0;
    width: 100%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .playlist-track-number {
    padding-left: calc(3px * var(--zoom));
  }

  .playlist-track-duration {
    padding-right: calc(5px * var(--zoom));
    text-align: right;
    white-space: nowrap;
  }

  .playlist-track.selected {
    background-color: var(--skin-plselbg, rgb(0, 0, 198));
  }

  .playlist-track.loaded {
    color: var(--skin-plcurrent, white);
  }

  .playlist-track.unavailable {
    color: rgb(80, 80, 80);
  }

  .playlist-track.unavailable.loaded {
    color: rgb(140, 140, 140);
  }

  /* ------ /TRACKS ------ */

  /* ------ PLAYLIST ------ */
  .playlist-sprite {
    --x: 0;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    --sprite-y: calc(var(--y) * 20px);
  }

  .playlist-tl-sprite {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
  }

  .playlist-t-sprite {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    background-position: -127px 0px;
  }

  .playlist-title-sprite {
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
    --sprite-url: var(--skin-pledit);
    width: 100px;
    height: 20px;
    --y: 0;
    --sprite-x: calc(var(--x) * 25px);
    background-position: -26px 0px;
  }

  .playlist-tr-sprite {
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 20px;
    --x: var(--playlist-w);
    --y: 0;
    --sprite-x: calc((var(--x) - 1) * 25px);
    background-position: -153px 0px;
  }

  .playlist-l-sprite {
    --sprite-url: var(--skin-pledit);
    width: 10px;
    height: 29px;
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: 0px -42px;
  }

  .playlist-r-sprite {
    --sprite-url: var(--skin-pledit);
    width: 19px;
    height: 29px;
    --x: var(--playlist-w);
    --sprite-x: calc((var(--x) - 1) * 25px + 6px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: -32px -42px;
  }

  .playlist-bl-sprite {
    --sprite-url: var(--skin-pledit);
    width: 125px;
    height: 38px;
    --y: var(--playlist-h);
    --sprite-y: calc((var(--y) - 1) * 29px - 9px);
    background-position: 0px -72px;
  }

  .playlist-b-sprite {
    --sprite-url: var(--skin-pledit);
    width: 25px;
    height: 38px;
    --y: var(--playlist-h);
    --sprite-x: calc(var(--x) * 25px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: -179px 0px;
  }

  .playlist-br-sprite {
    --sprite-url: var(--skin-pledit);
    width: 150px;
    height: 38px;
    --x: var(--playlist-w);
    --y: var(--playlist-h);
    --sprite-x: calc(var(--x) * 25px + 75px);
    --sprite-y: calc(var(--y) * 29px - 9px);
    background-position: 154px -72px;
  }
  /* ------ /PLAYLIST ------ */

  /* transparent click targets over the baked-in ADD/REM/SEL/MISC sprites */
  .pl-btn {
    position: absolute;
    left: calc(var(--pl-btn-x) * var(--zoom));
    top: calc(((var(--playlist-h) - 1) * 29px - 5px) * var(--zoom));
    width: calc(22px * var(--zoom));
    height: calc(18px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 60;
  }

  /* bottom-right LCD time readouts (green seven-seg-ish) */
  .pl-time {
    position: absolute;
    text-align: right;
    font-family: monospace;
    font-size: calc(7px * var(--zoom));
    line-height: 1;
    color: #14e614;
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
    z-index: 55;
  }
  /* positioned from the window's bottom-right corner (plain px = easy to tweak
     in devtools; stays put on resize). Adjust right / bottom / width. */
  .pl-time-elapsed {
    right: calc(58px * var(--zoom));
    width: calc(26px * var(--zoom));
    bottom: calc(7px * var(--zoom));
  }
  .pl-time-total {
    right: calc(120px * var(--zoom));
    width: calc(76px * var(--zoom));
    bottom: calc(21px * var(--zoom));
  }

  /* right-click menu — classic Win98 look, like Winamp's own menus */
  .ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
  }
  .ctx-menu {
    position: fixed;
    z-index: 201;
    min-width: 196px;
    max-width: 230px;
    max-height: 92vh;
    overflow-y: auto;
    background: #d4d0c8;
    border: 1px solid #000;
    box-shadow: 1px 1px 0 rgba(0, 0, 0, 0.4);
    padding: 2px;
    font-family: "MS Sans Serif", Tahoma, sans-serif;
    font-size: 11px;
    color: #000;
  }
  .ctx-head {
    padding: 1px 18px 2px 6px;
    color: #505050;
    font-size: 10px;
    font-weight: bold;
  }
  .ctx-tabs {
    display: flex;
    gap: 1px;
    margin: -1px -1px 3px;
  }
  .ctx-tab {
    flex: 1;
    font-family: inherit;
    font-size: 10px;
    padding: 2px 2px;
    border: 1px solid #808080;
    border-top: none;
    background: #bdb9ad;
    color: #000;
    cursor: pointer;
  }
  .ctx-tab.active {
    background: #d4d0c8;
    font-weight: bold;
    border-color: #000;
  }
  .ctx-hint {
    padding: 2px 6px;
    color: #707070;
    font-size: 9px;
    font-style: italic;
  }
  .ctx-discord {
    color: #5865f2;
    font-weight: bold;
  }
  .ctx-discord:hover {
    background: #5865f2;
    color: #fff;
  }
  .ctx-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 2px 10px 2px 4px;
    background: transparent;
    border: none;
    color: #000;
    font-family: inherit;
    font-size: 11px;
    cursor: default;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ctx-item:hover {
    background: #000080;
    color: #fff;
  }
  .ctx-dot {
    display: inline-block;
    width: 11px;
    text-align: center;
  }
  .ctx-sep {
    height: 0;
    border-top: 1px solid #808080;
    border-bottom: 1px solid #fff;
    margin: 3px 2px;
  }
  .ctx-listrow {
    display: flex;
    gap: 3px;
    padding: 2px 4px;
  }
  .ctx-listinput {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: 11px;
    border: 1px solid #808080;
    padding: 1px 3px;
  }
  .ctx-listbtn {
    font-family: inherit;
    font-size: 10px;
    border: 1px solid #808080;
    background: #ece9d8;
    cursor: pointer;
  }
  .ctx-listitem {
    display: flex;
    align-items: stretch;
  }
  .ctx-listload {
    flex: 1;
    min-width: 0;
  }
  .ctx-count {
    color: #808080;
  }
  .ctx-listload:hover .ctx-count {
    color: #cfcfe0;
  }
  .ctx-del {
    width: 16px;
    border: none;
    background: transparent;
    color: #a00;
    font-weight: bold;
    cursor: pointer;
  }
  .ctx-del:hover {
    background: #a00;
    color: #fff;
  }

  /* ------ MY PLAYLISTS browser (our addition) ------ */
  .my-playlists-btn {
    position: absolute;
    top: calc(21px * var(--zoom));
    left: calc(11px * var(--zoom));
    z-index: 40;
    padding: 0 6px;
    font-family: monospace;
    font-size: 9px;
    line-height: 12px;
    /* follows the active skin (PLEDIT text colour), green on the base skin */
    color: var(--skin-plnormal, #00ff41);
    background: linear-gradient(
      color-mix(in srgb, var(--skin-plbg, #12151c) 55%, #6a6a6a),
      var(--skin-plbg, #12151c)
    );
    border: 1px solid #000;
    box-shadow: inset 1px 1px 0 rgba(255, 255, 255, 0.15);
    cursor: pointer;
  }
  .my-playlists-btn:active {
    box-shadow: inset -1px -1px 0 rgba(255, 255, 255, 0.15);
  }
  .library-overlay {
    position: absolute;
    inset: 20px 12px 30px 12px;
    z-index: 50;
    background: #0a0d12;
    border: 1px solid #00ff41;
    box-shadow: 0 0 12px -2px rgba(0, 255, 65, 0.5);
    display: flex;
    flex-direction: column;
    font-family: monospace;
  }
  .library-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 3px 8px;
    color: #050805;
    background: #00cc22;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }
  .library-close {
    background: none;
    border: none;
    color: #050805;
    font-size: 13px;
    cursor: pointer;
    line-height: 1;
  }
  .library-search {
    margin: 4px 6px;
    padding: 2px 6px;
    background: #05170a;
    border: 1px solid #1e6b32;
    color: #00ff41;
    font-family: monospace;
    font-size: 11px;
    outline: none;
  }
  .library-search::placeholder {
    color: #3f7a4e;
  }
  .library-list {
    flex: 1;
    overflow-y: auto;
  }
  .library-item {
    display: flex;
    justify-content: space-between;
    width: 100%;
    padding: 3px 10px;
    background: none;
    border: none;
    color: #00ff41;
    font-family: monospace;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .library-item:hover {
    background: #163a1e;
  }
  .library-count {
    color: #5c9e6b;
    padding-left: 10px;
  }
  .library-msg {
    padding: 12px 10px;
    color: #8fbf9f;
    font-size: 12px;
  }
  .library-msg.err {
    color: #ff6b6b;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
