<script module>
  /** focuses an element as soon as it's there @param {HTMLElement} el */
  export function focusNow(el) {
    setTimeout(() => el.focus(), 0);
  }
</script>

<script>
  // The Library, as Winamp 5's Media Library: its tree, its views (Local
  // Media as Winamp's Audio view with the Artist and Album filters, every
  // other list as its simple view), its buttons and status line, in the
  // skin's GENEX.BMP colours and pictures and written in the skin's font by
  // Windows (lib/winampml). In a classic skin it wears GEN.BMP's frame, as
  // Winamp's did; in a modern one it's laid into the skin.
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, untrack } from "svelte";
  import { REACTIVE_WINDOW_SIZE, SKIN_APPLIED, toFileUrl } from "$lib/common.svelte.js";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable, makeSnappingResizer } from "$lib/window-docking.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";
  import { watchWindowActive } from "$lib/window-active.js";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { Menu } from "@tauri-apps/api/menu";
  import { MlView, createLook } from "$lib/winampml/mlview.js";
  import { classicGenex, BASE_GEN, cssImageUrl, loadImage, modernGenex, parseColor, toCanvas } from "$lib/winampml/genex.js";
  import { FRAME, drawGenFrame, scanLetters } from "$lib/winampml/genframe.js";
  import { SkinnedMenu, menuColors } from "$lib/winampml/menu.js";

  const { data } = $props();

  const freeMode = untrack(() => data?.freeMode === true);

  // Of the main window's keys (lib/shortcuts.js) only Ctrl+D works here:
  // typing letters in the Library searches the list.
  onMount(() => forwardShortcuts({ keys: new Set() }));

  // Rows and playlists here can be dragged onto the playlist window. Dropped
  // back on the Library itself, the webview would open the link and lose the
  // page, so drops here are simply ignored.
  onMount(() => {
    /** @param {DragEvent} e */
    const ignore = (e) => e.preventDefault();
    window.addEventListener("dragover", ignore);
    window.addEventListener("drop", ignore);
    return () => {
      window.removeEventListener("dragover", ignore);
      window.removeEventListener("drop", ignore);
    };
  });

  /**
   * @typedef {{ name: string, uri: string, track_count: number, image: string | null, folder?: string[] }} UserPlaylist
   * @typedef {{ uri: string, name: string, artist: string, album: string, albumArt: string | null, duration: number, unavailable: boolean, addedMs: number | null, path?: string,
   *   trackNo?: string, year?: string, genre?: string, size?: number, plays?: number, lastPlayed?: number }} LibTrack
   * @typedef {{ path: string, title: string, artist: string, album: string, duration_ms: number, sample_rate: number, kbps: number, date_ms: number, size?: number, track?: string, year?: string, genre?: string }} LocalItem
   * @typedef {{ folders: string[], items: LocalItem[], scanning: boolean }} LocalLibrary
   * @typedef {{ name: string, uris: string[] }} SavedList
   * @typedef {{ uri: string, added_ms: number | null }} TrackRef
   * @typedef {import("$lib/winampml/widgets.js").Column} Column
   * @typedef {import("$lib/winampml/widgets.js").TreeItem} TreeItem
   */

  let playlists = $state(/** @type {UserPlaylist[]} */ ([]));
  let loading = $state(true);
  let error = $state("");
  let search = $state("");

  let selectedUri = $state(/** @type {string | null} */ (null));
  // when true the right pane shows Spotify search results instead of a playlist
  let searchMode = $state(false);
  let searchQuery = $state("");
  let tracks = $state(/** @type {LibTrack[]} */ ([]));
  let trackUris = $state(/** @type {string[]} */ ([]));
  let tracksLoading = $state(false);
  let tracksError = $state("");
  let loadToken = 0;

  // which tree item is active: "search" | "liked" | "recent" | "top" | "list:<name>" | a playlist uri
  let activeNode = $state(/** @type {string | null} */ (null));
  const isRadio = $derived(typeof activeNode === "string" && activeNode.startsWith("radio:"));
  // the tree's highlighted item (a parent or the stats item can be highlighted
  // without its own view)
  let treeKey = $state(/** @type {string | null} */ (null));
  // the selected rows, as shown (the Play / Enqueue buttons and the menus act on them)
  let selRows = $state(/** @type {number[]} */ ([]));
  /** @type {MlView | null} */
  let view = null;
  function clearSel() {
    selRows = [];
    view?.clearSelection();
  }

  // --- sorting: a header click; Local Media starts in Winamp's order -------
  /** the sorted column (null: the list's own order) */
  let sortCol = $state(/** @type {string | null} */ (null));
  let sortDir = $state(1); // 1 asc, -1 desc
  /** Local Media's first order: artist, album, track, as Winamp listed it (its Title arrow shown) */
  let localDefault = $state(true);
  /** @param {string} col */
  function sortBy(col) {
    if (activeNode === "local" && localDefault) {
      localDefault = false;
      sortCol = col;
      sortDir = col === "title" ? -1 : 1;
    } else if (sortCol === col) {
      sortDir = -sortDir;
    } else {
      sortCol = col;
      sortDir = 1;
    }
    clearSel(); // indices change under a re-sort
  }

  // Case- and accent-insensitive, so "s" finds "Şebnem" and "beyonce" finds
  // "Beyoncé". Dotless ı folds to i for the same reason.
  /** @param {string} s */
  const fold = (s) =>
    (s || "")
      .toLowerCase()
      .normalize("NFD")
      .replace(/\p{M}/gu, "")
      .replace(/ı/g, "i");
  /** how Winamp ordered names: no case, and a leading "The " doesn't count @param {string} s */
  const sortName = (s) => fold(s).replace(/^the /, "");

  // --- Local Media's filters (Winamp's Artist and Album panes) -------------
  let filtersOn = $state(true);
  try {
    filtersOn = localStorage.getItem("ml-filters") !== "0";
  } catch {}
  let artistSel = $state(/** @type {number[]} */ ([]));
  let albumSel = $state(/** @type {number[]} */ ([]));
  /** the search box's words, each to be found in a row */
  const searchWords = $derived(fold(search.trim()).split(/\s+/).filter(Boolean));
  /** @param {LibTrack} t */
  const matches = (t) => {
    if (!searchWords.length) return true;
    const hay = fold(`${t.name} ${t.artist} ${t.album} ${t.genre ?? ""} ${t.year ?? ""}`);
    return searchWords.every((w) => hay.includes(w));
  };
  const isLocal = $derived(activeNode === "local");
  /** the rows the search leaves */
  // (a catalogue search's own words stay in the box, as Winamp's did, and
  // don't filter the results they found; more words typed after do)
  const searched = $derived(isRadio || !searchWords.length || (searchMode && search.trim() === searchQuery) ? tracks : tracks.filter(matches));
  /** Local Media's artists: "All", then each with its albums and tracks (names without case, as Winamp's database compared them) */
  const artistRows = $derived.by(() => {
    if (!isLocal || !filtersOn) return [];
    /** @type {Map<string, { name: string, albums: Set<string>, tracks: number }>} */
    const by = new Map();
    for (const t of searched) {
      const key = (t.artist || "").toLowerCase();
      let e = by.get(key);
      if (!e) by.set(key, (e = { name: t.artist || "", albums: new Set(), tracks: 0 }));
      if (t.album) e.albums.add(t.album.toLowerCase());
      e.tracks++;
    }
    const list = [...by.values()].sort((a, b) => sortName(a.name).localeCompare(sortName(b.name)));
    const albums = new Set(searched.filter((t) => t.album).map((t) => t.album.toLowerCase()));
    return [{ name: `All (${list.length} artists)`, key: "", all: true, albums: albums.size, tracks: searched.length }, ...list.map((e) => ({ name: e.name, key: e.name.toLowerCase(), all: false, albums: e.albums.size, tracks: e.tracks }))];
  });
  const pickedArtists = $derived.by(() => {
    const picked = artistSel.filter((i) => i > 0 && artistRows[i]).map((i) => artistRows[i].key);
    return picked.length ? new Set(picked) : null;
  });
  const byArtist = $derived(pickedArtists ? searched.filter((t) => pickedArtists.has((t.artist || "").toLowerCase())) : searched);
  const albumRows = $derived.by(() => {
    if (!isLocal || !filtersOn) return [];
    /** @type {Map<string, { name: string, year: string, tracks: number }>} */
    const by = new Map();
    for (const t of byArtist) {
      if (!t.album) continue;
      const key = t.album.toLowerCase();
      let e = by.get(key);
      if (!e) by.set(key, (e = { name: t.album || "", year: "", tracks: 0 }));
      if (t.year && t.year > e.year) e.year = t.year;
      e.tracks++;
    }
    const list = [...by.values()].sort((a, b) => sortName(a.name).localeCompare(sortName(b.name)));
    return [{ name: `All (${list.length} albums)`, key: "", all: true, year: "", tracks: byArtist.length }, ...list.map((e) => ({ ...e, key: e.name.toLowerCase(), all: false }))];
  });
  const pickedAlbums = $derived.by(() => {
    const picked = albumSel.filter((i) => i > 0 && albumRows[i]).map((i) => albumRows[i].key);
    return picked.length ? new Set(picked) : null;
  });

  /** the column's value to sort a row by @param {LibTrack} t @param {string} col */
  const sortKey = (t, col) => {
    switch (col) {
      case "time":
        return t.duration ?? 0;
      case "date":
        return t.addedMs ?? 0;
      case "track":
        return parseInt(t.trackNo ?? "", 10) || 0;
      case "plays":
        return t.plays ?? 0;
      case "last":
        return t.lastPlayed ?? 0;
      case "size":
        return t.size ?? 0;
      case "title":
        return sortName(t.name);
      case "artist":
        return sortName(t.artist);
      case "album":
        return sortName(t.album);
      default:
        return fold(String(/** @type {any} */ (t)[col] ?? ""));
    }
  };
  // The rows actually shown: searched, filtered, sorted (and, for a test's
  // pictures, without private names). Every row op below reads through this
  // so clicks map to the right track.
  // (how long making them took, for the status line: not state, as it's
  // written while they're made)
  let queryMs = 0;
  const displayTracks = $derived.by(() => {
    const t0 = performance.now();
    let shown = isLocal && filtersOn && pickedAlbums ? byArtist.filter((t) => pickedAlbums.has((t.album || "").toLowerCase())) : isLocal && filtersOn ? byArtist : searched;
    if (isLocal && localDefault) {
      shown = [...shown].sort(
        (a, b) =>
          sortName(a.artist).localeCompare(sortName(b.artist)) ||
          sortName(a.album).localeCompare(sortName(b.album)) ||
          (parseInt(a.trackNo ?? "", 10) || 0) - (parseInt(b.trackNo ?? "", 10) || 0) ||
          sortName(a.name).localeCompare(sortName(b.name)),
      );
    } else if (sortCol) {
      const col = sortCol;
      shown = [...shown].sort((a, b) => {
        const ka = sortKey(a, col);
        const kb = sortKey(b, col);
        const c = typeof ka === "number" && typeof kb === "number" ? ka - kb : String(ka).localeCompare(String(kb));
        return sortDir * c;
      });
    }
    queryMs = performance.now() - t0;
    if (hideRe) {
      const re = hideRe;
      shown = shown.filter((t) => !re.test(`${t.name} ${t.artist} ${t.album}`));
    }
    return shown;
  });

  // tree UI state
  let expandLocal = $state(true);
  let expandPlaylists = $state(true);
  let expandPinned = $state(true);

  onMount(async () => {
    REACTIVE_WINDOW_SIZE.setSize(500, 380);
    // Reopen at the size it was last left (falls back to the default above).
    invoke("get_window_inner_size", { label: "library" })
      .then((s) => {
        if (s) REACTIVE_WINDOW_SIZE.setSize(s.width, s.height);
      })
      .catch(() => {});
    if (freeMode) {
      selectLocal();
      return;
    }
    loadSavedListsLib();
    // nothing picked yet: Recently played rather than an empty list (once
    // the playlists are in, so Spotify is there to name the songs)
    loadPlaylists().then(() => {
      if (activeNode === null && !searchMode) selectHistory("recent");
    });
  });

  // The Library can reopen with the app before Spotify has finished
  // connecting (a slow start takes a few seconds), and one failed fetch used
  // to stay an error until the window was reopened. Try a few more times
  // first; after that, the error line offers a retry.
  async function loadPlaylists() {
    loading = true;
    error = "";
    for (const wait of [0, 1500, 3000, 6000, 10000]) {
      if (wait) await new Promise((r) => setTimeout(r, wait));
      try {
        playlists = await invoke("get_user_playlists");
        error = "";
        break;
      } catch (e) {
        error = String(e);
      }
    }
    loading = false;
  }

  // Type-to-find: typing letters/digits jumps to the first track whose title
  // (then artist) starts with what you've typed; the buffer clears after a short
  // pause. Classic Winamp / file-explorer behaviour, handy in long lists.
  let findText = "";
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let findTimer;
  /** @param {KeyboardEvent} e */
  function typeToFind(e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const el = /** @type {HTMLElement} */ (e.target);
    if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
    // the tree and the filters have their own keys
    if (view && view.focus && view.focus !== "tracks") return;
    if (!/^[\p{L}\p{N} ]$/u.test(e.key)) return;
    if (e.key === " " && !findText) return;
    const rows = displayTracks;
    if (!rows.length) return;
    e.preventDefault();
    findText += e.key;
    clearTimeout(findTimer);
    findTimer = setTimeout(() => (findText = ""), 1200);
    const q = fold(findText);
    let idx = rows.findIndex((r) => fold(r.name).startsWith(q));
    if (idx < 0) idx = rows.findIndex((r) => fold(r.artist).startsWith(q));
    if (idx < 0) idx = rows.findIndex((r) => fold(r.name).includes(q));
    if (idx >= 0) view?.selectRow(idx);
  }
  onMount(() => {
    window.addEventListener("keydown", typeToFind);
    return () => window.removeEventListener("keydown", typeToFind);
  });

  const playlistUrl = (/** @type {string} */ uri) => `https://open.spotify.com/playlist/${uri.split(":").pop()}`;
  const trackUrl = (/** @type {string} */ uri) => `https://open.spotify.com/track/${uri.split(":").pop()}`;

  // Resolve names for a list of {uri, added_ms} refs, filling the right pane
  // top-down. `added_ms` is the playlist "date added" (null for liked/search).
  // 50 tracks per request: one request per song ran into Spotify's request
  // limit on long playlists, which then also kept songs from playing.
  /** @param {{uri: string, added_ms: number|null, plays?: number}[]} refs @param {number} token */
  async function loadTrackMetas(refs, token) {
    const BATCH = 50;
    for (let i = 0; i < refs.length; i += BATCH) {
      if (token !== loadToken) return; // switched away, abandon
      const chunk = refs.slice(i, i + BATCH);
      const askMeta = () => /** @type {Promise<any[]>} */ (invoke("get_tracks_metadata", { uris: chunk.map((ref) => ref.uri) }));
      /** @type {any[]} */
      let metas;
      try {
        metas = await askMeta();
      } catch (e) {
        // a busy moment: once more after a breather, then skip these
        await new Promise((r) => setTimeout(r, 1500));
        if (token !== loadToken) return;
        metas = await askMeta().catch(() => []);
      }
      if (token !== loadToken) return;
      // a track Spotify sent nothing for is skipped
      const found = chunk.flatMap((ref, j) =>
        metas[j] ? [{ ...metas[j], addedMs: ref.added_ms ?? null, ...(ref.plays !== undefined ? { plays: ref.plays, lastPlayed: ref.added_ms ?? 0 } : {}) }] : [],
      );
      tracks = [...tracks, ...found];
    }
  }

  /** a new list in the right pane: what was there goes @param {string | null} node */
  function startList(node) {
    // a catalogue search's words leave with it (they'd filter the next list)
    if (searchMode && searchQuery && search.trim() === searchQuery) {
      search = "";
      view?.setSearch("");
    }
    searchMode = false;
    selectedUri = null;
    activeNode = node;
    treeKey = node;
    clearSel();
    tracks = [];
    trackUris = [];
    tracksError = "";
    sortCol = null;
    sortDir = 1;
  }

  /** @param {UserPlaylist} pl */
  async function selectPlaylist(pl) {
    startList(pl.uri);
    selectedUri = pl.uri;
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const refs = /** @type {TrackRef[]} */ (await invoke("get_track_ids", { uri: pl.uri }));
      if (token !== loadToken) return;
      trackUris = refs.map((r) => r.uri);
      await loadTrackMetas(refs, token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }

  // Load the user's Spotify "Liked Songs" into the right pane (newest first).
  async function selectLiked() {
    startList("liked");
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const ids = /** @type {string[]} */ (await invoke("get_liked_songs"));
      if (token !== loadToken) return;
      trackUris = ids;
      await loadTrackMetas(ids.map((uri) => ({ uri, added_ms: null })), token);
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
      const ids = /** @type {string[]} */ (await invoke("get_liked_songs"));
      if (ids.length) emitWindowEvent("playerWindow", { UrlsDropped: ids.map(trackUrl) });
    } catch {
      /* ignore */
    }
  }

  // Search the Spotify catalogue (Enter in the search box); results land in the
  // right pane and play like any other track.
  async function doSearch() {
    const q = search.trim();
    if (!q) return;
    startList("search");
    searchMode = true;
    searchQuery = q;
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const ids = /** @type {string[]} */ (await invoke("search", { query: q }));
      if (token !== loadToken) return;
      trackUris = ids;
      await loadTrackMetas(ids.map((uri) => ({ uri, added_ms: null })), token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }

  function focusSearch() {
    treeKey = "search";
    if (!searchMode) {
      startList("search");
      searchMode = true;
      searchQuery = "";
    }
    searchInput?.focus();
  }

  function clearSearch() {
    search = "";
  }

  // Reuse the existing "UrlsDropped" event the main playlist window already
  // listens for (clear + load). Works cross-window via Tauri's global emit.
  const loadPlaylistIntoMain = (/** @type {UserPlaylist} */ pl) => emitWindowEvent("playerWindow", { UrlsDropped: [playlistUrl(pl.uri)] });

  // Spotify's playlist folders, as in its sidebar: shut until clicked, and
  // the open ones kept for next time. A folder plays, is added or is dragged
  // as all the playlists in it, one after another.
  const FOLDERS_KEY = "ml-open-folders";
  let openFolders = $state(/** @type {Set<string>} */ (new Set()));
  try {
    const saved = JSON.parse(localStorage.getItem(FOLDERS_KEY) || "[]");
    if (Array.isArray(saved)) openFolders = new Set(saved.filter((k) => typeof k === "string"));
  } catch {}
  /** @param {string[]} path */
  const folderKey = (path) => path.join("\u0001");
  /** @typedef {{ folder: string[], depth: number, pl?: undefined } | { pl: UserPlaylist, depth: number, folder?: undefined }} PlaylistRow */
  const playlistRows = $derived.by(() => {
    /** @type {PlaylistRow[]} */
    const rows = [];
    const listed = new Set();
    for (const pl of playlists) {
      const path = pl.folder ?? [];
      for (let d = 1; d <= path.length; d++) {
        const key = folderKey(path.slice(0, d));
        if (listed.has(key)) continue;
        listed.add(key);
        rows.push({ folder: path.slice(0, d), depth: d - 1 });
      }
      rows.push({ pl, depth: path.length });
    }
    // only what's in open folders shows
    return rows.filter((row) => {
      const path = row.folder ? row.folder.slice(0, -1) : (row.pl.folder ?? []);
      for (let d = 1; d <= path.length; d++) {
        if (!openFolders.has(folderKey(path.slice(0, d)))) return false;
      }
      return true;
    });
  });
  /** @param {string[]} path */
  function toggleFolder(path) {
    const key = folderKey(path);
    const open = new Set(openFolders);
    if (!open.delete(key)) open.add(key);
    openFolders = open;
    try {
      localStorage.setItem(FOLDERS_KEY, JSON.stringify([...open]));
    } catch {}
  }
  /** The links of every playlist in the folder, its folders' too. @param {string[]} path */
  const folderUrls = (path) => playlists.filter((pl) => path.every((name, i) => pl.folder?.[i] === name)).map((pl) => playlistUrl(pl.uri));
  /** @param {MouseEvent} e @param {string[]} path */
  function folderMenu(e, path) {
    e.preventDefault();
    const urls = folderUrls(path);
    popMenu([
      { text: `Play all ${urls.length} playlists`, action: () => emitWindowEvent("playerWindow", { UrlsDropped: urls }) },
      { text: "Add them all to the playlist", action: () => emitWindowEvent("playerWindow", { UrlsAppended: urls }) },
    ]);
  }

  // Double-clicking a track in the right pane:
  //  - search results  → append just that one track
  //  - playlist tracks → play it and queue the rest of that playlist
  /** @param {number} index */
  function loadTrackIntoMain(index) {
    const uri = displayTracks[index]?.uri;
    if (!uri) return;
    if (isRadio) {
      sendStations([displayTracks[index]], true);
    } else if (activeNode === "local") {
      sendLocalTracks(displayTracks.slice(index), true);
    } else if (searchMode) {
      emitWindowEvent("playerWindow", { UrlsAppended: [trackUrl(uri)] });
    } else {
      // play from here down the list *as currently sorted*
      emitWindowEvent("playerWindow", { UrlsDropped: displayTracks.slice(index).map((t) => trackUrl(t.uri)) });
    }
  }

  /** the rows the buttons act on: the selection, else all of them */
  const picked = () => (selRows.length ? selRows.map((i) => displayTracks[i]).filter(Boolean) : displayTracks);
  /** @param {LibTrack[]} list @param {boolean} play */
  function sendTracks(list, play) {
    if (!list.length) return;
    if (isRadio) return sendStations(list, play);
    if (list[0].path) return sendLocalTracks(list, play);
    const urls = list.map((t) => trackUrl(t.uri));
    if (play) emitWindowEvent("playerWindow", { UrlsDropped: urls });
    else emitWindowEvent("playerWindow", { UrlsAppended: urls });
  }
  const playSelected = () => sendTracks(picked(), true);
  const enqueueSelected = () => sendTracks(picked(), false);
  const playAll = () => sendTracks(displayTracks, true);
  const enqueueAll = () => sendTracks(displayTracks, false);
  /** Generate Playlist: the view's songs in a random order */
  function generatePlaylist() {
    const list = [...picked()];
    for (let i = list.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [list[i], list[j]] = [list[j], list[i]];
    }
    sendTracks(list, true);
  }

  // Add the selected track to an app-local list (kept in Spotiamp+, not Spotify).
  let savedLists = $state(/** @type {SavedList[]} */ ([]));
  let newListOpen = $state(false);
  let newLibListName = $state("");
  async function loadSavedListsLib() {
    try {
      savedLists = await invoke("get_saved_lists");
    } catch {
      savedLists = [];
    }
  }
  /** the selected Spotify songs' uris */
  const pickedUris = () => picked().map((t) => t.uri).filter((u) => u && u.startsWith("spotify:"));
  /** @param {string} name */
  async function addSelectedToList(name) {
    for (const uri of pickedUris()) await invoke("add_to_list", { name, uri }).catch(() => {});
    await loadSavedListsLib();
  }
  async function createListWithSelected() {
    const name = newLibListName.trim();
    if (!name) return;
    await addSelectedToList(name);
    newLibListName = "";
    newListOpen = false;
  }

  // Show a saved app-local list's tracks in the right pane (its uris are already
  // concrete tracks, so no expansion needed).
  /** @param {SavedList} list */
  async function selectSavedList(list) {
    startList("list:" + list.name);
    tracksLoading = true;
    const token = ++loadToken;
    try {
      trackUris = list.uris;
      await loadTrackMetas(list.uris.map((uri) => ({ uri, added_ms: null })), token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }
  /** @param {SavedList} list */
  function loadListIntoMain(list) {
    if (list.uris.length) emitWindowEvent("playerWindow", { UrlsDropped: list.uris.map(trackUrl) });
  }
  /** @param {string} name */
  async function deleteSavedList(name) {
    const sure = await ask(`Delete the list "${name}"?`, { title: "Delete list", kind: "warning" }).catch(() => false);
    if (!sure) return;
    await invoke("delete_list", { name }).catch(() => {});
    await loadSavedListsLib();
    if (activeNode === "list:" + name) startList(null);
  }

  // --- Loved songs (♡, the F key; kept in Spotiamp+, lists.rs) ---
  async function selectLoved() {
    startList("loved");
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const uris = /** @type {string[]} */ (await invoke("get_loved"));
      if (token !== loadToken) return;
      trackUris = uris;
      await loadTrackMetas(uris.map((uri) => ({ uri, added_ms: null })), token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }
  async function loadLovedIntoMain() {
    const uris = /** @type {string[]} */ (await invoke("get_loved").catch(() => []));
    if (uris.length) emitWindowEvent("playerWindow", { UrlsDropped: uris.map(trackUrl) });
  }
  // Which songs are loved, for the ♡ next to their title.
  let lovedSet = $state(/** @type {Set<string>} */ (new Set()));
  async function refreshLoved() {
    lovedSet = new Set(/** @type {string[]} */ (await invoke("get_loved").catch(() => [])));
  }
  // A song loved or unloved elsewhere while the list is up: show the change.
  onMount(() => {
    refreshLoved();
    const off = listen("lovedChanged", () => {
      refreshLoved();
      if (activeNode === "loved") selectLoved();
    });
    return () => {
      off.then((f) => f()).catch(() => {});
    };
  });

  // --- Radio: the station guide and your stations (radio_guide.rs) ---
  /** @typedef {{ url: string, name: string, tags: string, country: string, codec: string, bitrate: number, uuid: string }} Station */
  let expandRadio = $state(true);
  let radioQuery = $state("");
  // "Popular in …": the country set in Windows (its region, not its language;
  // the language's country until that's known)
  const languageCountry = (() => {
    try {
      const region = new Intl.Locale(navigator.language).maximize().region;
      return region && /^[A-Z]{2}$/.test(region) ? region : null;
    } catch {
      return null;
    }
  })();
  let myCountry = $state(/** @type {string | null} */ (languageCountry));
  invoke("radio_home_country")
    .then((c) => {
      if (typeof c === "string" && c) myCountry = c;
    })
    .catch(() => {});
  const myCountryName = $derived.by(() => {
    try {
      return myCountry ? (new Intl.DisplayNames(["en"], { type: "region" }).of(myCountry) ?? myCountry) : null;
    } catch {
      return myCountry;
    }
  });
  /** @param {Station} s @returns {LibTrack} */
  const stationTrack = (s) => ({
    uri: `radio:${s.url}`,
    path: s.url,
    name: s.name || s.url,
    artist: s.tags,
    album: s.country,
    albumArt: null,
    duration: s.bitrate,
    unavailable: false,
    addedMs: null,
  });
  /**
   * @param {"radio:yours" | "radio:popular" | "radio:local" | "radio:search"} node
   * @param {() => Promise<Station[]>} get
   */
  async function showRadio(node, get) {
    startList(node);
    tracksLoading = true;
    const token = ++loadToken;
    try {
      const list = await get();
      if (token !== loadToken) return;
      tracks = list.map(stationTrack);
      trackUris = tracks.map((t) => t.uri);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }
  const selectYourStations = () => showRadio("radio:yours", () => invoke("radio_stations"));
  const selectPopular = (/** @type {boolean} */ mine) =>
    showRadio(mine ? "radio:local" : "radio:popular", () => invoke("radio_popular", { country: mine ? myCountry : null }));
  function searchRadio() {
    const q = search.trim();
    if (!q) return;
    radioQuery = q;
    showRadio("radio:search", () => invoke("radio_search", { query: q }));
  }
  // A station to the playlist (`play`: and on), its name going with it. It
  // joins your stations once it really plays (the player says so).
  /** @param {LibTrack[]} list @param {boolean} play */
  function sendStations(list, play) {
    if (!list.length) return;
    emitWindowEvent("playerWindow", {
      LocalFilesSent: { items: list.map((t) => ({ path: t.path ?? "", name: t.name })), replace: false, play },
    });
  }

  // --- Local Media: the music folders added here (local_library.rs) ---
  let localFolders = $state(/** @type {string[]} */ ([]));
  let localItems = $state(/** @type {LocalItem[]} */ ([]));
  // how far a reading of the folders has got (null: not reading)
  let localScan = $state(/** @type {{ done: number, total: number } | null} */ (null));
  const localByPath = $derived(new Map(localItems.map((i) => [i.path, i])));
  /** how often and when each local song was played (Play Count, Played Last) */
  let localPlays = $state(/** @type {Map<string, { plays: number, at: number }>} */ (new Map()));
  async function refreshLocalPlays() {
    const items = /** @type {{ uri: string, at: number, plays: number }[]} */ (await invoke("history_top", { limit: 1000000 }).catch(() => []));
    localPlays = new Map(items.filter((i) => i.uri.startsWith("local:")).map((i) => [i.uri.slice(6), { plays: i.plays, at: i.at }]));
  }
  /** @param {string} p */
  const fileName = (p) => p.split(/[\\/]/).pop() ?? p;
  /** @param {LocalItem} i @returns {LibTrack} */
  const localTrack = (i) => ({
    uri: `local:${i.path}`,
    path: i.path,
    name: i.title || fileName(i.path).replace(/\.[^.]+$/, ""),
    artist: i.artist,
    album: i.album,
    albumArt: null,
    duration: i.duration_ms,
    unavailable: false,
    addedMs: i.date_ms || null,
    trackNo: i.track ?? "",
    year: i.year ?? "",
    genre: i.genre ?? "",
    size: i.size ?? 0,
    plays: localPlays.get(i.path)?.plays ?? 0,
    lastPlayed: localPlays.get(i.path)?.at ?? 0,
  });
  /** @param {LocalLibrary} lib */
  function showLocal(lib) {
    localFolders = lib.folders;
    localItems = lib.items;
    localScan = lib.scanning ? (localScan ?? { done: 0, total: 0 }) : null;
    if (activeNode === "local") {
      tracks = lib.items.map(localTrack);
      trackUris = tracks.map((t) => t.uri);
    }
  }
  async function selectLocal() {
    startList("local");
    localDefault = true;
    tracksLoading = false;
    const token = ++loadToken;
    await refreshLocalPlays();
    // what's known shows at once; the folders are checked for changes meanwhile
    let lib = /** @type {LocalLibrary | null} */ (await invoke("local_library", { rescan: true }).catch(() => null));
    // The reading's end comes as an event too, but one sent before this page
    // was listening (a small folder, just as the Library opened) would be
    // missed: ask again until it's done.
    while (lib?.scanning && token === loadToken) {
      showLocal(lib);
      await new Promise((r) => setTimeout(r, 500));
      lib = /** @type {LocalLibrary | null} */ (await invoke("local_library", { rescan: false }).catch(() => null));
    }
    if (lib && token === loadToken) showLocal(lib);
  }
  // Songs to the main playlist, their tags already read (no file opened again).
  /** @param {LocalItem[]} items @param {boolean} replace */
  function sendLocalItems(items, replace) {
    if (!items.length) return;
    emitWindowEvent("playerWindow", {
      LocalFilesSent: {
        items: items.map((i) => ({
          path: i.path,
          name: i.title || undefined,
          artist: i.artist || undefined,
          durationMs: i.duration_ms,
          sampleRate: i.sample_rate,
          kbps: i.kbps,
        })),
        replace,
      },
    });
  }
  /** @param {LibTrack[]} list @param {boolean} replace */
  function sendLocalTracks(list, replace) {
    sendLocalItems(
      list.flatMap((t) => {
        const item = t.path ? localByPath.get(t.path) : undefined;
        return item ? [item] : [];
      }),
      replace,
    );
  }
  async function loadLocalIntoMain() {
    const lib = /** @type {LocalLibrary | null} */ (await invoke("local_library", { rescan: false }).catch(() => null));
    if (lib) sendLocalItems(lib.items, true);
  }
  async function addLocalFolder() {
    const lib = /** @type {LocalLibrary | null} */ (await invoke("local_library_add_folder").catch(() => null));
    if (!lib) return;
    if (activeNode !== "local") await selectLocal();
    else showLocal(lib);
  }
  /** @param {string} folder */
  async function removeLocalFolder(folder) {
    const lib = /** @type {LocalLibrary | null} */ (await invoke("local_library_remove_folder", { folder }).catch(() => null));
    if (lib) showLocal(lib);
  }
  /** @param {MouseEvent} e */
  async function localMenu(e) {
    e.preventDefault();
    const lib = /** @type {LocalLibrary | null} */ (await invoke("local_library", { rescan: false }).catch(() => null));
    const folders = lib?.folders ?? [];
    popMenu([
      { text: "Add media to Library...", action: addLocalFolder },
      {
        text: "Rescan Watch Folders (in background)",
        enabled: folders.length > 0,
        action: () => (activeNode === "local" ? selectLocal() : invoke("local_library", { rescan: true }).catch(() => {})),
      },
      { item: "Separator" },
      {
        text: "Remove a folder",
        items: folders.length
          ? folders.map((f) => ({ text: f.replaceAll("&", "&&"), action: () => removeLocalFolder(f) }))
          : [{ text: "No folders yet", enabled: false }],
      },
    ]);
  }
  onMount(() => {
    const offs = [
      listen("localLibraryChanged", async () => {
        const lib = /** @type {LocalLibrary | null} */ (await invoke("local_library", { rescan: false }).catch(() => null));
        if (lib) showLocal(lib);
      }),
      listen("localLibraryScan", (e) => {
        const p = /** @type {{ done: number, total: number, scanning: boolean }} */ (e.payload);
        localScan = p.scanning ? { done: p.done, total: p.total } : null;
      }),
    ];
    return () => offs.forEach((off) => off.then((f) => f()).catch(() => {}));
  });

  // --- listening history (history.rs): Recently played / Most played ---
  /** @param {"recent" | "top"} kind */
  async function selectHistory(kind) {
    startList(kind);
    tracksLoading = true;
    const token = ++loadToken;
    try {
      /** @type {{uri: string, at: number, plays: number}[]} */
      const items = await invoke(kind === "recent" ? "history_recent" : "history_top", { limit: 100 });
      if (token !== loadToken) return;
      // Spotify tracks only here: this pane reads Spotify metadata.
      const spotify = items.filter((i) => i.uri.startsWith("spotify:track:"));
      trackUris = spotify.map((i) => i.uri);
      // the Date column shows when each was last played
      await loadTrackMetas(spotify.map((i) => ({ uri: i.uri, added_ms: i.at, plays: i.plays })), token);
    } catch (e) {
      if (token === loadToken) tracksError = String(e);
    } finally {
      if (token === loadToken) tracksLoading = false;
    }
  }
  /** @param {"recent" | "top"} kind */
  async function loadHistoryIntoMain(kind) {
    try {
      /** @type {{uri: string}[]} */
      const items = await invoke(kind === "recent" ? "history_recent" : "history_top", { limit: 100 });
      const urls = items.filter((i) => i.uri.startsWith("spotify:track:")).map((i) => trackUrl(i.uri));
      if (urls.length) emitWindowEvent("playerWindow", { UrlsDropped: urls });
    } catch {
      /* nothing to load */
    }
  }
  async function clearHistory() {
    const sure = await ask("Forget everything Spotiamp+ has noted as played on this computer?", {
      title: "Clear listening history",
      kind: "warning",
    }).catch(() => false);
    if (!sure) return;
    await invoke("history_clear").catch(() => {});
    if (activeNode === "recent" || activeNode === "top") selectHistory(activeNode);
  }

  // --- pinned playlists (kept in the settings) ---
  /** @type {string[]} */
  let pinned = $state([]);
  invoke("get_pinned_playlists")
    .then((uris) => (pinned = /** @type {string[]} */ (uris)))
    .catch(() => {});
  const pinnedPlaylists = $derived(
    pinned.flatMap((uri) => {
      const pl = playlists.find((p) => p.uri === uri);
      return pl ? [pl] : [];
    }),
  );
  /** @param {string} uri @param {boolean} on */
  async function setPinned(uri, on) {
    pinned = on ? [...pinned.filter((u) => u !== uri), uri] : pinned.filter((u) => u !== uri);
    await invoke("set_playlist_pinned", { uri, pinned: on }).catch(() => {});
  }

  // --- the menus: Winamp's skinned ones, drawn in the window (menu.js) ---
  /** @type {SkinnedMenu | null} */
  let skinMenu = null;
  /** where the mouse last went down or asked for a menu (the window's px) */
  let lastPoint = { x: 0, y: 0 };
  /** a button's menu goes by the button (Winamp: above those at the bottom, below the others) */
  let menuFromButton = false;
  /** @type {Menu | null} */
  let openedMenu = null;
  let menuBusy = false;
  /**
   * Windows' own menu, for one too tall for the window (a skinned menu is
   * drawn inside it). One at a time, the old one fully closed first: a popup
   * keeps this page's resource table locked until it closes, and overlapping
   * them froze the whole app (see the playlist's showMenu).
   * @param {import("$lib/winampml/menu.js").MenuItem[]} items
   */
  async function nativeMenu(items) {
    if (menuBusy) return;
    menuBusy = true;
    try {
      const old = openedMenu;
      openedMenu = null;
      if (old) await old.close().catch(() => {});
      const menu = await Menu.new({ items: /** @type {any} */ (items) });
      openedMenu = menu;
      await menu.popup();
    } catch {
      /* no menu then */
    } finally {
      menuBusy = false;
    }
  }
  /** @param {import("$lib/winampml/menu.js").MenuItem[]} items */
  function popMenu(items) {
    if (!skinMenu || !root) return;
    let { x, y } = lastPoint;
    const r = menuFromButton ? view?.pressedRect : null;
    menuFromButton = false;
    if (r) {
      x = inner.x + r.x;
      const below = r.y + r.h / 2 < inner.h / 2;
      y = inner.y + (below ? r.y + r.h : r.y);
      skinMenu.show(items, x, y, below ? "down" : "up");
      return;
    }
    skinMenu.show(items, x, y);
  }
  /** @param {MouseEvent} e @param {any} pl */
  function playlistMenu(e, pl) {
    e.preventDefault();
    const isPinned = pinned.includes(pl.uri);
    popMenu([
      { text: "Play", action: () => loadPlaylistIntoMain(pl) },
      { text: "Add to the playlist", action: () => emitWindowEvent("playerWindow", { UrlsAppended: [playlistUrl(pl.uri)] }) },
      { item: "Separator" },
      { text: isPinned ? "Unpin" : "Pin to the top", action: () => setPinned(pl.uri, !isPinned) },
    ]);
  }
  /** Right-click a song: its commands (the selection's, when it's in it). @param {MouseEvent} e @param {number} i */
  async function trackMenu(e, i) {
    e.preventDefault();
    const t = displayTracks[i];
    if (!t?.uri) return;
    const path = t.path;
    if (isRadio && path) {
      const yours = activeNode === "radio:yours";
      popMenu([
        { text: "Play selection\tEnter", action: () => loadTrackIntoMain(i) },
        { text: "Enqueue selection\tShift+Enter", action: () => sendStations([t], false) },
        { item: "Separator" },
        { text: "Copy the station's link", action: () => navigator.clipboard.writeText(path).catch(() => {}) },
        ...(yours ? [{ text: "Remove from your stations", action: () => invoke("radio_forget", { url: path }).then(selectYourStations).catch(() => {}) }] : []),
      ]);
      return;
    }
    if (path) {
      // (Winamp's, those Spotiamp+ has: its words and its keys)
      const artist = t.artist ?? "";
      const album = t.album ?? "";
      popMenu([
        { text: "Play selection\tEnter", action: () => (selRows.length > 1 ? playSelected() : loadTrackIntoMain(i)) },
        { text: "Enqueue selection\tShift+Enter", action: enqueueSelected },
        { item: "Separator" },
        { text: "Select all\tCtrl+A", action: () => view?.selectAll() },
        { item: "Separator" },
        ...(artist ? [{ text: `All tracks by artist "${artist.replaceAll("&", "&&")}"`, action: () => showArtist(artist) }] : []),
        ...(album ? [{ text: `All tracks from album "${album.replaceAll("&", "&&")}"`, action: () => showAlbum(album) }] : []),
        ...(artist || album ? [{ item: "Separator" }] : []),
        { text: "Explore item folder\tCtrl+F", action: () => invoke("local_reveal", { path }).catch(() => {}) },
      ]);
      return;
    }
    const uri = t.uri;
    const loved = lovedSet.has(uri);
    const lists = /** @type {{name: string}[]} */ (await invoke("get_saved_lists").catch(() => []));
    popMenu([
      { text: "Play selection\tEnter", action: () => (selRows.length > 1 ? playSelected() : loadTrackIntoMain(i)) },
      { text: "Enqueue selection\tShift+Enter", action: enqueueSelected },
      { item: "Separator" },
      { text: "Select all\tCtrl+A", action: () => view?.selectAll() },
      { item: "Separator" },
      { text: loved ? "Unlove" : "♡ Love", action: () => invoke("set_loved", { uris: pickedUris(), loved: !loved }).catch(() => {}) },
      {
        text: "Add to list",
        items: [
          ...lists.map((l) => ({ text: l.name.replaceAll("&", "&&"), action: () => addSelectedToList(l.name) })),
          ...(lists.length ? [{ item: "Separator" }] : []),
          { text: "New list...", action: () => (newListOpen = true) },
        ],
      },
      { text: "Copy Spotify link", action: () => navigator.clipboard.writeText(trackUrl(uri)).catch(() => {}) },
    ]);
  }
  /** Winamp's "All tracks by artist": the artist picked in the filter (searched for without filters) @param {string} name */
  function showArtist(name) {
    const k = filtersOn ? artistRows.findIndex((r) => !r.all && r.key === name.toLowerCase()) : -1;
    if (k > 0) {
      artistSel = [k];
      albumSel = [];
      clearSel();
    } else search = name;
  }
  /** "All tracks from album": all artists, that album picked @param {string} name */
  function showAlbum(name) {
    if (filtersOn) {
      artistSel = [];
      const k = albumRows.findIndex((r) => !r.all && r.key === name.toLowerCase());
      if (k > 0) {
        albumSel = [k];
        clearSel();
        return;
      }
    }
    search = name;
  }
  /** @param {MouseEvent} e @param {"recent" | "top"} kind */
  function historyMenu(e, kind) {
    e.preventDefault();
    popMenu([
      { text: "Play all", action: () => loadHistoryIntoMain(kind) },
      { item: "Separator" },
      { text: "Clear listening history...", action: clearHistory },
    ]);
  }

  // --- drag onto the playlist window (it takes open.spotify.com links, the
  //     same as a drag from the Spotify app) ---
  /** @param {DragEvent} e @param {string[]} urls */
  function dragLinks(e, urls) {
    if (!e.dataTransfer || !urls.length) return false;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/uri-list", urls.join("\r\n"));
    e.dataTransfer.setData("text/plain", urls.join("\n"));
    return true;
  }

  /** "m:ss" (whole seconds, as Winamp kept a song's length) @param {number} ms */
  function fmt(ms) {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }
  // "date added" (epoch ms) → compact YYYY-MM-DD for the Date column.
  /** @param {number} ms */
  function fmtDate(ms) {
    const d = new Date(ms);
    if (isNaN(d.getTime())) return "";
    const p = (/** @type {number} */ n) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
  }
  /** Played Last, as Windows writes a date and time here @param {number} ms */
  function fmtWhen(ms) {
    if (!ms) return "";
    const d = new Date(ms);
    if (isNaN(d.getTime())) return "";
    return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  }
  /** a total length the way Winamp's status line wrote it ("1 day+13:02:46") @param {number} seconds */
  function fmtTotal(seconds) {
    const s = Math.round(seconds);
    const days = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    const hms = `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
    return days ? `${days} day${days > 1 ? "s" : ""}+${hms}` : h ? hms : `${m}:${String(sec).padStart(2, "0")}`;
  }
  /** @param {number} bytes */
  function fmtSize(bytes) {
    if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
    if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(2)} MB`;
    return `${(bytes / 1024).toFixed(2)} KB`;
  }

  // Only some lists have a date (a playlist's added date, when it was last
  // played); Loved songs, Favorite Songs and search results don't, so there
  // the column goes rather than staying blank.
  const showDate = $derived(displayTracks.some((t) => t.addedMs));
  const dateLabel = $derived(activeNode === "recent" || activeNode === "top" ? "Played" : "Added");

  /** what's in the right pane, by name */
  const headTitle = $derived(
    activeNode === "radio:yours"
      ? "Your stations"
      : activeNode === "radio:popular"
        ? "Popular stations"
        : activeNode === "radio:local"
          ? `Popular in ${myCountryName}`
          : activeNode === "radio:search"
            ? `Stations: ${radioQuery}`
            : activeNode === "local"
              ? "Local Media"
              : activeNode === "recent"
                ? "Recently played"
                : activeNode === "top"
                  ? "Most played"
                  : activeNode === "liked"
                    ? "Favorite Songs"
                    : activeNode === "loved"
                      ? "Loved songs"
                      : activeNode?.startsWith?.("list:")
                        ? activeNode.slice(5)
                        : searchMode
                          ? `Search: ${searchQuery}`
                          : selectedUri
                            ? (playlists.find((p) => p.uri === selectedUri)?.name ?? "")
                            : "Spotiamp+",
  );

  const close = () => invoke("set_library_window_visible", { visible: false });

  // Drag the library window, snapping to any other open window.
  /** @param {HTMLElement} element */
  function makeLibraryDraggable(element) {
    makeDockedDraggable(element, "library", "libraryWindow");
  }

  // Resize from the bottom-right corner, like Winamp's.
  /** @param {HTMLElement} element */
  function makeLibraryResizable(element) {
    makeSnappingResizer(
      element,
      "library",
      (e) => {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        return {
          width: Math.max(Math.round(e.clientX / zoom) + 3, 380),
          height: Math.max(Math.round(e.clientY / zoom) + 3, 260),
        };
      },
      ({ width, height }) => REACTIVE_WINDOW_SIZE.setSize(Math.round(width), Math.round(height)),
      () => REACTIVE_WINDOW_SIZE.zoom || 1,
    );
  }

  // ------------------------------------------------------------------ the view

  // the columns, Winamp's widths to begin with; widths and the hidden ones kept
  /** @type {Record<string, Column[]>} */
  const COLUMNS = {
    artists: [
      { id: "name", title: "Artist", w: 155 },
      { id: "albums", title: "Albums", w: 48 },
      { id: "tracks", title: "Tracks", w: 45 },
    ],
    albums: [
      { id: "name", title: "Album", w: 155 },
      { id: "year", title: "Year", w: 47 },
      { id: "tracks", title: "Tracks", w: 47 },
    ],
    local: [
      { id: "artist", title: "Artist", w: 140 },
      { id: "album", title: "Album", w: 140 },
      { id: "track", title: "Track #", w: 55 },
      { id: "title", title: "Title", w: 140 },
      { id: "time", title: "Length", w: 55 },
      { id: "genre", title: "Genre", w: 75 },
      { id: "rating", title: "Rating", w: 62 },
      { id: "plays", title: "Play Count", w: 70 },
      { id: "last", title: "Played Last", w: 115 },
      { id: "year", title: "Year", w: 55 },
    ],
    spotify: [
      { id: "artist", title: "Artist", w: 140 },
      { id: "album", title: "Album", w: 140 },
      { id: "title", title: "Title", w: 180 },
      { id: "time", title: "Length", w: 55 },
      { id: "date", title: "Added", w: 80 },
    ],
    top: [
      { id: "plays", title: "Play Count", w: 70 },
      { id: "artist", title: "Artist", w: 140 },
      { id: "title", title: "Title", w: 140 },
      { id: "album", title: "Album", w: 140 },
      { id: "time", title: "Length", w: 55 },
      { id: "last", title: "Played Last", w: 115 },
    ],
    recent: [
      { id: "last", title: "Played Last", w: 115 },
      { id: "artist", title: "Artist", w: 140 },
      { id: "title", title: "Title", w: 140 },
      { id: "album", title: "Album", w: 140 },
      { id: "time", title: "Length", w: 55 },
      { id: "plays", title: "Play Count", w: 70 },
    ],
    radio: [
      { id: "title", title: "Station", w: 200 },
      { id: "artist", title: "Genre", w: 160 },
      { id: "album", title: "Country", w: 100 },
      { id: "time", title: "Bitrate", w: 55 },
    ],
  };
  /** @type {Record<string, Record<string, number>>} */
  let widths = $state({});
  try {
    widths = JSON.parse(localStorage.getItem("ml-widths") || "{}") ?? {};
  } catch {}
  let hiddenCols = $state(/** @type {string[]} */ ([]));
  try {
    hiddenCols = JSON.parse(localStorage.getItem("ml-hidden") || "[]") ?? [];
  } catch {}
  /** @param {string} set */
  const columnsOf = (set) =>
    COLUMNS[set]
      .filter((c) => !hiddenCols.includes(set === "local" ? c.id : `${set}:${c.id}`))
      .map((c) => ({ ...c, w: widths[set]?.[c.id] ?? c.w }));

  const kindSet = $derived(isLocal ? "local" : isRadio ? "radio" : activeNode === "top" || activeNode === "recent" ? activeNode : "spotify");

  // Local Media's ratings (Winamp's Rating column: a click on its stars),
  // kept on this PC
  let ratings = $state(/** @type {Record<string, number>} */ ({}));
  try {
    ratings = JSON.parse(localStorage.getItem("ml-ratings") || "{}") ?? {};
  } catch {}
  /** @param {number} i @param {number} stars */
  function rate(i, stars) {
    const t = displayTracks[i];
    if (!t) return;
    const key = t.path ?? t.uri;
    const now = ratings[key] ?? 0;
    const next = { ...ratings };
    // the same stars again: no rating
    if (stars === now) delete next[key];
    else next[key] = stars;
    ratings = next;
    try {
      localStorage.setItem("ml-ratings", JSON.stringify(ratings));
    } catch {}
  }

  /** one row's cells, in the columns' order @param {LibTrack} t @param {Column[]} cols @returns {(string | { t?: string, dim?: boolean, stars?: number })[]} */
  function cellsOf(t, cols) {
    return cols.map((c) => {
      switch (c.id) {
        case "artist":
          return t.artist ?? "";
        case "album":
          return t.album ?? "";
        case "title":
          return (lovedSet.has(t.uri) ? "♡ " : "") + (t.name ?? "");
        case "time":
          return isRadio ? (t.duration ? `${t.duration}k` : "") : t.duration ? fmt(t.duration) : "";
        case "track":
          return t.trackNo ?? "";
        case "genre":
          return t.genre ?? "";
        case "rating": {
          return { stars: ratings[t.path ?? t.uri] ?? 0 };
        }
        case "plays":
          return String(t.plays ?? 0);
        case "last":
          return fmtWhen(t.lastPlayed ?? 0);
        case "year":
          return t.year ?? "";
        case "date":
          return t.addedMs ? fmtDate(t.addedMs) : "";
        default:
          return "";
      }
    });
  }

  const hint = $derived.by(() => {
    if (tracksLoading && !tracks.length) return searchMode ? "searching…" : "loading…";
    if (tracksError && !tracks.length) return tracksError;
    if (displayTracks.length) return "";
    if (isRadio)
      return tracks.length === 0
        ? activeNode === "radio:yours"
          ? "The stations you play show up here. Pick one under Popular, or search for a station or a genre above."
          : "no stations found"
        : "no match";
    if (activeNode === "local" && tracks.length === 0)
      return localFolders.length === 0
        ? "Add a music folder and its songs show here, ready to play: press Library below, or right-click Local Media."
        : localScan
          ? "reading your files…"
          : "no music files in these folders";
    if (tracks.length > 0) return "";
    if (activeNode === "search" && !searchQuery) return "Type an artist or a song in the box above and press Enter.";
    if (activeNode === null) return "Select a playlist on the left, or search Spotify above.";
    return searchMode ? "no results" : activeNode === "recent" ? "nothing played yet: pick a playlist on the left, or search Spotify above" : "";
  });

  const status = $derived.by(() => {
    const n = displayTracks.length;
    const items = `${n} item${n === 1 ? "" : "s"}`;
    if (activeNode === "local") {
      if (localScan) return localScan.total ? `reading ${localScan.done} of ${localScan.total}…` : "looking for files…";
      const secs = displayTracks.reduce((s, t) => s + Math.floor((t.duration || 0) / 1000), 0);
      const bytes = displayTracks.reduce((s, t) => s + (t.size || 0), 0);
      return `${items} [${fmtTotal(secs)}] [${fmtSize(bytes)}] in ${(queryMs / 1000).toFixed(3)} sec.`;
    }
    if (isRadio) return items;
    if (tracksLoading && tracks.length) return `${items}, loading…`;
    const secs = displayTracks.reduce((s, t) => s + Math.floor((t.duration || 0) / 1000), 0);
    return n ? `${items} [${fmtTotal(secs)}]` : items;
  });

  const pane = $derived.by(() => {
    const set = kindSet;
    let cols = columnsOf(set);
    if (set === "spotify") cols = cols.filter((c) => c.id !== "date" || showDate).map((c) => (c.id === "date" ? { ...c, title: dateLabel } : c));
    const rows = displayTracks;
    void lovedSet;
    void ratings;
    /** @type {import("$lib/winampml/mlview.js").Pane} */
    const p = {
      kind: isLocal ? "audio" : "simple",
      filters: filtersOn,
      tracks: {
        columns: cols,
        count: rows.length,
        cells: (i) => cellsOf(rows[i], cols),
        sortCol: isLocal && localDefault ? "title" : !sortCol && activeNode === "top" ? "plays" : !sortCol && activeNode === "recent" ? "last" : sortCol,
        sortUp: isLocal && localDefault ? true : !sortCol && (activeNode === "top" || activeNode === "recent") ? false : sortDir > 0,
        hint,
        // the song playing, in the playlist's "current" colour
        current: nowKey ? rows.findIndex((t) => t.uri === nowKey) : -1,
      },
      buttons: isLocal
        ? [
            { id: "play", label: "Play", split: true },
            { id: "generate", label: "Generate Playlist" },
          ]
        : isRadio
          ? [{ id: "play", label: "Play", split: true }]
          : [
              { id: "play", label: "Play", split: true },
              { id: "list", label: "Add to List", dropdown: true, enabled: rows.length > 0 },
            ],
      status,
    };
    if (isLocal && filtersOn) {
      const ac = columnsOf("artists");
      const bc = columnsOf("albums");
      const ar = artistRows;
      const br = albumRows;
      p.artists = { columns: ac, count: ar.length, cells: (i) => [ar[i].name, String(ar[i].albums), String(ar[i].tracks)], selected: artistSel.length ? artistSel : [0], sortCol: "name", sortUp: true };
      p.albums = { columns: bc, count: br.length, cells: (i) => [br[i].name, br[i].year || "", String(br[i].tracks)], selected: albumSel.length ? albumSel : [0], sortCol: "name", sortUp: true };
    }
    return p;
  });

  /** names kept out of pictures (a smoke test's screenshots): matching tree items and rows go */
  let hideRe = $state(/** @type {RegExp | null} */ (null));

  // the tree: Spotiamp+'s places, drawn as Winamp's navigation
  const treeItems = $derived.by(() => {
    const all = treeAll;
    return hideRe ? all.filter((i) => !hideRe?.test(i.text)) : all;
  });
  const treeAll = $derived.by(() => {
    /** @type {TreeItem[]} */
    const items = [];
    if (!freeMode) {
      if (pinnedPlaylists.length) {
        items.push({ key: "pinned", text: "Pinned", level: 0, parent: true, open: expandPinned });
        if (expandPinned) for (const pl of pinnedPlaylists) items.push({ key: "pin:" + pl.uri, text: pl.name, level: 1, icon: "list" });
      }
      items.push({ key: "liked", text: "Favorite Songs", level: 0, icon: "fav" });
      items.push({ key: "loved", text: "Loved Songs", level: 0, icon: "loved" });
      items.push({ key: "recent", text: "Recently Played", level: 0, icon: "recent" });
      items.push({ key: "top", text: "Most Played", level: 0, icon: "top" });
      items.push({ key: "stats", text: "Listening Stats", level: 0, icon: "stats" });
    }
    items.push({ key: "local", text: "Local Media", level: 0, icon: "local" });
    items.push({ key: "radio", text: "Radio", level: 0, parent: true, open: expandRadio });
    if (expandRadio) {
      items.push({ key: "radio:yours", text: "Your stations", level: 1, icon: "radio" });
      items.push({ key: "radio:popular", text: "Popular", level: 1, icon: "radio" });
      if (myCountry) items.push({ key: "radio:local", text: `Popular in ${myCountryName}`, level: 1, icon: "radio" });
    }
    if (freeMode) {
      items.push({ key: "premium", text: "Your Spotify library needs Premium", level: 0 });
      return items;
    }
    items.push({ key: "lists", text: "Spotiamp+", level: 0, parent: true, open: expandLocal });
    if (expandLocal) for (const list of savedLists) items.push({ key: "list:" + list.name, text: `${list.name} (${list.uris.length})`, level: 1, icon: "list" });
    items.push({ key: "playlists", text: "Playlists", level: 0, parent: true, open: expandPlaylists });
    if (expandPlaylists) {
      if (loading) items.push({ key: "pl-loading", text: "loading…", level: 1 });
      else if (error) items.push({ key: "pl-error", text: "Couldn't load your playlists (double-click to retry)", level: 1 });
      else
        for (const row of playlistRows) {
          if (row.folder) items.push({ key: "folder:" + folderKey(row.folder), text: row.folder[row.folder.length - 1], level: 1 + row.depth, parent: true, open: openFolders.has(folderKey(row.folder)) });
          else items.push({ key: row.pl.uri, text: row.pl.name, level: 1 + row.depth, icon: "list" });
        }
    }
    items.push({ key: "search", text: "Search", level: 0, icon: "search" });
    return items;
  });

  /** the playlist a tree key stands for @param {string} key */
  const playlistOf = (key) => playlists.find((p) => p.uri === (key.startsWith("pin:") ? key.slice(4) : key));
  /** @param {string} key */
  const folderPath = (key) => key.slice(7).split("\u0001");

  /** @param {string} key */
  function treeSelect(key) {
    treeKey = key;
    if (key === "liked") selectLiked();
    else if (key === "loved") selectLoved();
    else if (key === "recent" || key === "top") selectHistory(key);
    else if (key === "stats") invoke("set_stats_window_visible", { visible: true });
    else if (key === "local") selectLocal();
    else if (key === "radio:yours") selectYourStations();
    else if (key === "radio:popular") selectPopular(false);
    else if (key === "radio:local") selectPopular(true);
    else if (key === "search") focusSearch();
    else if (key.startsWith("list:")) {
      const list = savedLists.find((l) => "list:" + l.name === key);
      if (list) selectSavedList(list);
    } else {
      const pl = playlistOf(key);
      if (pl) {
        selectPlaylist(pl);
        treeKey = key;
      }
    }
  }
  /** @param {string} key */
  function treeToggle(key) {
    if (key === "pinned") expandPinned = !expandPinned;
    else if (key === "radio") expandRadio = !expandRadio;
    else if (key === "lists") {
      expandLocal = !expandLocal;
      if (expandLocal) loadSavedListsLib();
    } else if (key === "playlists") expandPlaylists = !expandPlaylists;
    else if (key.startsWith("folder:")) toggleFolder(folderPath(key));
  }
  /** @param {string} key */
  function treeActivate(key) {
    const item = treeItems.find((i) => i.key === key);
    if (item?.parent) return treeToggle(key);
    if (key === "pl-error") return loadPlaylists();
    if (key === "liked") loadLikedIntoMain();
    else if (key === "loved") loadLovedIntoMain();
    else if (key === "recent" || key === "top") loadHistoryIntoMain(key);
    else if (key === "local") loadLocalIntoMain();
    else if (key.startsWith("list:")) {
      const list = savedLists.find((l) => "list:" + l.name === key);
      if (list) loadListIntoMain(list);
    } else {
      const pl = playlistOf(key);
      if (pl) loadPlaylistIntoMain(pl);
    }
  }
  /** @param {string} key @param {MouseEvent} e */
  function treeContext(key, e) {
    if (key === "liked" || key === "loved") return popMenu([{ text: "Play", action: () => treeActivate(key) }]);
    if (key === "recent" || key === "top") return historyMenu(e, key);
    if (key === "local") return localMenu(e);
    if (key.startsWith("folder:")) return folderMenu(e, folderPath(key));
    if (key.startsWith("list:")) {
      const list = savedLists.find((l) => "list:" + l.name === key);
      if (list)
        popMenu([
          { text: "Play", action: () => loadListIntoMain(list) },
          { text: "Add to the playlist", action: () => emitWindowEvent("playerWindow", { UrlsAppended: list.uris.map(trackUrl) }) },
          { item: "Separator" },
          { text: "Delete list...", action: () => deleteSavedList(list.name) },
        ]);
      return;
    }
    const pl = playlistOf(key);
    if (pl) playlistMenu(e, pl);
  }
  /** @param {string} key @param {DragEvent} e */
  function treeDrag(key, e) {
    if (key.startsWith("folder:")) return dragLinks(e, folderUrls(folderPath(key)));
    if (key.startsWith("list:")) {
      const list = savedLists.find((l) => "list:" + l.name === key);
      return list ? dragLinks(e, list.uris.map(trackUrl)) : false;
    }
    const pl = playlistOf(key);
    return pl ? dragLinks(e, [playlistUrl(pl.uri)]) : false;
  }

  /** the Library button's menu (Winamp's: its own commands) @param {MouseEvent} e */
  function libraryMenu(e) {
    popMenu([
      { text: "Add media to Library...", action: addLocalFolder },
      { text: "Rescan Watch Folders (in background)", enabled: localFolders.length > 0, action: () => (activeNode === "local" ? selectLocal() : invoke("local_library", { rescan: true }).catch(() => {})) },
      { item: "Separator" },
      { text: "Listening stats", action: () => invoke("set_stats_window_visible", { visible: true }) },
      ...(freeMode ? [] : [{ text: "Reload your playlists", action: loadPlaylists }, { text: "Clear listening history...", action: clearHistory }]),
    ]);
  }

  /** @param {string} id @param {"main" | "split"} part @param {MouseEvent} e */
  async function buttonPressed(id, part, e) {
    if (id === "play") {
      if (part === "main") return playSelected();
      popMenu([
        { text: "Play selection", enabled: selRows.length > 0, action: playSelected },
        { text: "Enqueue selection", enabled: selRows.length > 0, action: enqueueSelected },
        { item: "Separator" },
        { text: "Play all", action: playAll },
        { text: "Enqueue all", action: enqueueAll },
      ]);
    } else if (id === "generate") generatePlaylist();
    // (the Play button's menu entries by name, for the smoke tests)
    else if (id === "enqueue") enqueueSelected();
    else if (id === "explore") {
      const t = displayTracks[selRows[0] ?? -1];
      if (t?.path && isLocal) invoke("local_reveal", { path: t.path }).catch(() => {});
    }
    else if (id === "playall") playAll();
    else if (id === "enqueueall") enqueueAll();
    else if (id === "list") {
      await loadSavedListsLib();
      popMenu([
        ...savedLists.map((l) => ({ text: l.name.replaceAll("&", "&&"), action: () => addSelectedToList(l.name) })),
        ...(savedLists.length ? [{ item: "Separator" }] : []),
        { text: "New list...", action: () => (newListOpen = true) },
      ]);
    }
  }

  /** the search row's own buttons (Local Media) @param {string} id @param {MouseEvent} e */
  function toolbarPressed(id, e) {
    if (id === "art") invoke("set_art_window_visible", { visible: true }).catch(() => {});
    else if (id === "view")
      popMenu([
        { text: "Artist\\Album", checked: filtersOn, action: () => setFilters(true) },
        { text: "No filters", checked: !filtersOn, action: () => setFilters(false) },
      ]);
    else if (id === "columns") {
      // a menu for each list, as Winamp's (the first column stays)
      /** @param {string} set @param {string[]} keep */
      const toggles = (set, keep) =>
        COLUMNS[set]
          .filter((c) => !keep.includes(c.id))
          .map((c) => {
            const key = set === "local" ? c.id : `${set}:${c.id}`;
            return {
              text: c.title,
              checked: !hiddenCols.includes(key),
              action: () => {
                hiddenCols = hiddenCols.includes(key) ? hiddenCols.filter((h) => h !== key) : [...hiddenCols, key];
                try {
                  localStorage.setItem("ml-hidden", JSON.stringify(hiddenCols));
                } catch {}
              },
            };
          });
      popMenu([
        { text: "Artist", items: toggles("artists", ["name"]) },
        { text: "Album", items: toggles("albums", ["name"]) },
        { text: "Tracks", items: toggles("local", ["artist", "title"]) },
      ]);
    }
  }
  /** @param {boolean} on */
  function setFilters(on) {
    filtersOn = on;
    artistSel = [];
    albumSel = [];
    try {
      localStorage.setItem("ml-filters", on ? "1" : "0");
    } catch {}
  }

  // ------------------------------------------------------------------ the window

  /** @type {HTMLDivElement | undefined} */
  let root = $state();
  /** @type {HTMLCanvasElement | undefined} */
  let content = $state();
  /** @type {HTMLCanvasElement | undefined} */
  let frameCanvas = $state();
  /** @type {HTMLInputElement | undefined} */
  let searchInput = $state();
  let modern = $state(false);
  let active = $state(true);
  let closePressed = $state(false);
  let rootW = $state(0);
  let rootH = $state(0);
  /** GEN.BMP and its letters, for the frame */
  let gen = $state(/** @type {{ img: HTMLImageElement | HTMLCanvasElement, letters: number[][] | null } | null} */ (null));
  let title = $state("Winamp Library");
  let lookSeq = 0;
  /** the song playing: a list's uri for it ("spotify:track:…", "local:…", "radio:…") */
  let nowKey = $state("");
  /** @type {import("$lib/winampml/menu.js").MenuColors | null} */
  let menuCols = null;

  /** the skin's font: Winamp's playlist font (Arial, unless the skin's is chosen) */
  function skinFont() {
    let face = "Arial";
    try {
      if (localStorage.getItem("skin-font") === "1") {
        const f = getComputedStyle(document.body).getPropertyValue("--skin-plfont").trim().replace(/^["']|["']$/g, "");
        if (f) face = f;
      }
    } catch {}
    return { face, size: 12 };
  }

  /** the skin's look again: the sheet, the frame's pictures */
  async function reskin() {
    const seq = ++lookSeq;
    const isModern = "modern" in document.body.dataset;
    /** @type {import("$lib/winampml/genex.js").Genex | null} */
    let genex = null;
    /** @type {Record<string, number[]> | null} */
    let modernMenu = null;
    /** the playing song's colour: the playlist's (a modern skin's list.text.current) */
    let current = parseColor(getComputedStyle(document.body).getPropertyValue("--skin-plcurrent"));
    if (isModern) {
      const look = /** @type {any} */ (await invoke("modern_look").catch(() => null));
      genex = await modernGenex(look);
      modernMenu = look?.menu ?? null;
      current = parseColor(look?.colors?.current ?? "") ?? current;
    }
    if (!genex) genex = await classicGenex();
    if (seq !== lookSeq) return;
    const look = await createLook(genex, skinFont(), () => view?.invalidate());
    if (seq !== lookSeq) return;
    menuCols = menuColors(genex.c, (isModern && modernMenu) || {});
    modern = isModern;
    if (view) view.tracks.currentColor = current ?? [255, 255, 255];
    view?.setLook(look);
    // the frame: the skin's GEN.BMP, else Winamp's own (the base skin's)
    const style = getComputedStyle(document.body);
    const named = style.getPropertyValue("--skin-libtitle").trim().replace(/^"(.*)"$/, "$1");
    title = named || "Winamp Library";
    const url = cssImageUrl("--skin-gen") ?? BASE_GEN;
    const img = await loadImage(url);
    if (seq !== lookSeq) return;
    if (img) {
      const sheet = toCanvas(img);
      gen = { img: sheet, letters: scanLetters(sheet) };
    }
  }

  onMount(() => {
    if (!content || !searchInput) return;
    let state = {};
    try {
      state = JSON.parse(localStorage.getItem("ml-layout") || "{}") ?? {};
    } catch {}
    view = new MlView(
      content,
      searchInput,
      {
        treeSelect,
        treeActivate,
        treeToggle,
        treeContext,
        treeDrag,
        rowActivate: (i) => (selRows.length > 1 && selRows.includes(i) ? playSelected() : loadTrackIntoMain(i)),
        cellClick: (i, col, dx, starW) => {
          // (the stars are 12 px apart from 5 px in)
          if (col === "rating" && isLocal) rate(i, Math.max(1, Math.min(5, Math.floor((dx - 5) / 12) + 1)));
        },
        rowContext: (i, e) => trackMenu(e, i),
        rowsDrag: (rows, e) => {
          const list = rows.map((i) => displayTracks[i]).filter(Boolean);
          if (!list.length || isRadio) return false;
          return dragLinks(e, list.map((t) => (t.path ? toFileUrl(t.path) : trackUrl(t.uri))));
        },
        sort: (list, col) => {
          if (list === "tracks") sortBy(col);
        },
        columnsChanged: (list, w) => {
          const set = list === "tracks" ? kindSet : list;
          widths = { ...widths, [set]: { ...(widths[set] ?? {}), ...w } };
          try {
            localStorage.setItem("ml-widths", JSON.stringify(widths));
          } catch {}
        },
        filterChanged: (a, b) => {
          // a new artist picked: its albums, all of them
          if (a.join() !== artistSel.join()) {
            artistSel = a;
            albumSel = [];
          } else albumSel = b;
          clearSel();
        },
        selectionChanged: (rows) => (selRows = rows),
        search: (text) => (search = text),
        searchEnter: (text) => {
          search = text;
          if (activeNode === "local") return;
          if (isRadio) searchRadio();
          else doSearch();
        },
        clear: clearSearch,
        button: (id, part, e) => {
          menuFromButton = true;
          buttonPressed(id, part, e);
        },
        library: (e) => {
          menuFromButton = true;
          libraryMenu(e);
        },
        toolbar: (id, e) => {
          menuFromButton = true;
          toolbarPressed(id, e);
        },
        layoutChanged: (s) => {
          try {
            localStorage.setItem("ml-layout", JSON.stringify(s));
          } catch {}
        },
      },
      state,
    );
    const ro = new ResizeObserver(() => {
      if (!root) return;
      rootW = root.clientWidth;
      rootH = root.clientHeight;
    });
    if (root) ro.observe(root);
    // the menus, and where the mouse was when one was asked for
    skinMenu = root ? new SkinnedMenu(root, () => (view?.look && menuCols ? { font: view.look.font, colors: menuCols } : null), nativeMenu) : null;
    /** @param {MouseEvent} e */
    const point = (e) => {
      if (skinMenu?.open && skinMenu.back.contains(/** @type {Node} */ (e.target))) return;
      const r = root?.getBoundingClientRect();
      lastPoint = { x: e.clientX - (r?.left ?? 0), y: e.clientY - (r?.top ?? 0) };
      menuFromButton = false;
    };
    root?.addEventListener("pointerdown", point, true);
    root?.addEventListener("contextmenu", point, true);
    const shut = () => skinMenu?.close();
    window.addEventListener("blur", shut);
    // the song playing (the player says so every second): a Spotify song's
    // uri, a file's or a station's address
    /** @type {(() => void) | undefined} */
    let unsubNow;
    subscribeToWindowEvent("art", (e) => {
      const key = e.uri ? e.uri : e.path ? (/^https?:\/\//i.test(e.path) ? "radio:" : "local:") + e.path : "";
      if (key !== nowKey) nowKey = key;
    }).then((u) => (unsubNow = u));
    // the smoke tests' handle: the Library is one picture, so they ask it
    /** @param {string} what a tree key, or the start of an item's text */
    const findKey = (what) => {
      const lower = what.toLowerCase();
      return (treeAll.find((i) => i.key === what) ?? treeAll.find((i) => i.text.toLowerCase() === lower) ?? treeAll.find((i) => i.text.toLowerCase().startsWith(lower)))?.key ?? null;
    };
    /** @type {any} */ (window).__ml = {
      view: () => view,
      select: (/** @type {string} */ what) => {
        // (a playlist in a shut folder isn't in the tree: by its uri)
        const key = findKey(what) ?? (playlistOf(what) ? what : null);
        if (!key) return false;
        const item = treeAll.find((i) => i.key === key);
        if (item?.parent) treeToggle(key);
        else treeSelect(key);
        return true;
      },
      activate: (/** @type {string} */ what) => {
        const key = findKey(what);
        if (key) treeActivate(key);
        return !!key;
      },
      tree: () => treeItems.map((i) => i.text),
      playlists: () => playlists.map((p) => ({ uri: p.uri, name: p.name })),
      treeItems: () => treeItems.map((i) => ({ key: i.key, text: i.text, level: i.level, parent: !!i.parent })),
      rows: () => displayTracks.map((t) => cellsOf(t, pane.tracks.columns).map((c) => (typeof c === "string" ? c : (c.t ?? "")))),
      count: () => displayTracks.length,
      columns: () => pane.tracks.columns.map((c) => c.title),
      column: (/** @type {string} */ id) => {
        const cols = pane.tracks.columns;
        const k = cols.findIndex((c) => c.id === id || c.title === id);
        return k < 0 ? null : displayTracks.map((t) => {
          const c = cellsOf(t, cols)[k];
          return typeof c === "string" ? c : (c.t ?? "");
        });
      },
      status: () => status,
      hint: () => hint,
      head: () => headTitle,
      loading: () => tracksLoading,
      title: () => title,
      search: (/** @type {string} */ text, enter = false) => {
        search = text;
        view?.setSearch(text);
        if (enter) {
          if (activeNode === "local") return;
          if (isRadio) searchRadio();
          else doSearch();
        }
      },
      click: (/** @type {number} */ i) => view?.selectRow(i),
      dbl: (/** @type {number} */ i) => loadTrackIntoMain(i),
      selected: () => selRows,
      sort: (/** @type {string} */ col) => sortBy(col),
      sortState: () => ({ col: localDefault && isLocal ? "title" : sortCol, dir: sortDir }),
      press: (/** @type {string} */ id, part = "main") => buttonPressed(id, /** @type {any} */ (part), new MouseEvent("click")),
      hide: (/** @type {string | null} */ source) => (hideRe = source ? new RegExp(source, "i") : null),
      /** the menus: one shown at a point, an item lit, what's open */
      menu: async (/** @type {any[]} */ items, x = 0, y = 0, probe = false) => {
        if (!skinMenu) return null;
        if (!probe) return skinMenu.show(items, x, y);
        // (probe: whether it would go to Windows' menu, without opening one)
        const fallback = skinMenu.fallback;
        let used = false;
        skinMenu.fallback = () => (used = true);
        await skinMenu.show(items, x, y);
        skinMenu.fallback = fallback;
        return used;
      },
      menuHot: (/** @type {number} */ n) => {
        const l = skinMenu?.levels.at(-1);
        if (!l || !skinMenu) return false;
        l.hot = n;
        return skinMenu.draw(l).then(() => true);
      },
      menuOpen: () => skinMenu?.levels.map((l) => ({ x: l.x, y: l.y, w: l.w, h: l.h, items: l.items.map((i) => i.text ?? "-") })) ?? [],
      menuClose: () => skinMenu?.close(),
      /** the sheet's colours in use, and whether GEN.BMP's frame is drawn */
      colors: () => view?.look?.g.c ?? null,
      frame: () => {
        const c = frameCanvas;
        if (!c || modern || !c.width) return null;
        const d = c.getContext("2d")?.getImageData(0, 0, c.width, 20).data;
        let drawn = 0;
        if (d) for (let i = 3; i < d.length; i += 4) if (d[i]) drawn++;
        return { title, letters: gen?.letters ? title.replace(/[^a-z]/gi, "").length : 0, drawn };
      },
    };
    reskin();
    window.addEventListener(SKIN_APPLIED, reskin);
    const stopActive = watchWindowActive((a) => {
      active = a;
      view?.setActive(a);
    });
    return () => {
      ro.disconnect();
      window.removeEventListener(SKIN_APPLIED, reskin);
      window.removeEventListener("blur", shut);
      unsubNow?.();
      skinMenu?.destroy();
      skinMenu = null;
      stopActive();
      view?.destroy();
      view = null;
    };
  });

  // the insides' size: the frame's room in a classic skin, all of it in a modern one
  const inner = $derived(modern ? { x: 0, y: 0, w: rootW, h: rootH } : { x: FRAME.left, y: FRAME.top, w: Math.max(0, rootW - FRAME.left - FRAME.right), h: Math.max(0, rootH - FRAME.top - FRAME.bottom) });
  $effect(() => view?.setSize(inner.w, inner.h));
  $effect(() => view?.setRoot(inner.x, inner.y));
  $effect(() => view?.setTree(treeItems, treeKey ?? activeNode));
  $effect(() => view?.setPane(pane));
  $effect(() => view?.setSearch(search));
  // a new list: its filters start at "All"
  $effect(() => {
    void activeNode;
    artistSel = [];
    albumSel = [];
  });

  // the frame, redrawn as it's sized, focused or re-skinned
  $effect(() => {
    const c = frameCanvas;
    const g = gen;
    if (!c || !g || modern || rootW < 1 || rootH < 1) return;
    c.width = rootW;
    c.height = rootH;
    const ctx = c.getContext("2d");
    if (ctx) drawGenFrame(ctx, g.img, rootW, rootH, title, g.letters, active, closePressed);
  });

  /** @param {PointerEvent} e */
  function closeDown(e) {
    closePressed = true;
    /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
  }
  /** @param {PointerEvent} e */
  function closeUp(e) {
    const el = /** @type {HTMLElement} */ (e.currentTarget);
    const r = el.getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX < r.right && e.clientY >= r.top && e.clientY < r.bottom;
    closePressed = false;
    if (inside) close();
  }
</script>

<div class="mlw" class:modern data-gen-window bind:this={root}>
  {#if !modern}
    <canvas class="ml-frame" bind:this={frameCanvas}></canvas>
  {/if}
  <div class="ml-titlebar" data-gen="titlebar" use:makeLibraryDraggable></div>
  {#if !modern}
    <div
      class="ml-close"
      data-gen="close"
      data-no-drag
      role="button"
      tabindex="-1"
      aria-label="Close"
      onpointerdown={closeDown}
      onpointerup={closeUp}
    ></div>
  {/if}
  <canvas
    class="ml-content"
    tabindex="0"
    bind:this={content}
    style:left="{inner.x}px"
    style:top="{inner.y}px"
    style:width="{inner.w}px"
    style:height="{inner.h}px"
  ></canvas>
  <input class="ml-edit" bind:this={searchInput} value={search} spellcheck="false" autocomplete="off" aria-label="Search" />
  {#if newListOpen}
    <div class="ml-newlist-back" role="presentation" onpointerdown={() => (newListOpen = false)}></div>
    <div class="ml-newlist">
      <input
        class="ml-newlist-input"
        bind:value={newLibListName}
        placeholder="new list…"
        onkeydown={(e) => {
          if (e.key === "Enter") createListWithSelected();
          if (e.key === "Escape") newListOpen = false;
        }}
        use:focusNow
      />
    </div>
  {/if}
  <div class="ml-resize" data-gen="resize" use:makeLibraryResizable></div>
</div>


<style>
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }
  .mlw {
    position: fixed;
    inset: 0;
    overflow: hidden;
    user-select: none;
  }
  .ml-frame {
    position: absolute;
    left: 0;
    top: 0;
    width: 100%;
    height: 100%;
    image-rendering: pixelated;
  }
  .ml-content {
    position: absolute;
    outline: none;
    image-rendering: pixelated;
  }
  :global(body[data-fractional-scale]) .ml-content,
  :global(body[data-fractional-scale]) .ml-frame {
    image-rendering: auto;
  }
  .ml-titlebar {
    position: absolute;
    left: 0;
    top: 0;
    right: 14px;
    height: 20px;
  }
  .ml-close {
    position: absolute;
    right: 2px;
    top: 3px;
    width: 9px;
    height: 9px;
  }
  /* the search box takes the typing; Windows draws what's typed */
  .ml-edit {
    position: absolute;
    display: none;
    box-sizing: border-box;
    margin: 0;
    padding: 0 3px;
    border: 0;
    outline: none;
    background: transparent;
    color: transparent;
    caret-color: transparent;
    font: 12px Arial, sans-serif;
    opacity: 0;
  }
  .ml-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 14px;
    height: 14px;
    cursor: nwse-resize;
  }
  .modern .ml-resize {
    width: 10px;
    height: 10px;
  }
  .ml-newlist-back {
    position: absolute;
    inset: 0;
    z-index: 5;
  }
  .ml-newlist {
    position: absolute;
    left: 50%;
    top: 40%;
    transform: translate(-50%, -50%);
    z-index: 6;
    padding: 6px;
    background: var(--skin-genexwndbg, #383757);
    border: 1px solid var(--skin-genexdivider, #75748b);
  }
  .ml-newlist-input {
    width: 180px;
    font: 12px Arial, sans-serif;
    color: var(--skin-genexitemfg, #0f0);
    background: var(--skin-genexitembg, #000);
    border: 1px solid var(--skin-genexdivider, #75748b);
    outline: none;
    padding: 2px 4px;
  }
</style>
