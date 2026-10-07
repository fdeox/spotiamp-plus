import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { enterExitViewportObserver, REACTIVE_WINDOW_SIZE } from "./common.svelte";
import { emitWindowEvent, subscribeToWindowEvent } from "./events.svelte";
import { SpotifyTrack, SpotifyUri, durationToString } from "./spotify.svelte";

/**
 * A playlist row: either a Spotify track or a local file. Both extend
 * PlaylistRow and expose displayName / displayDuration / play() / isLoaded etc.
 * @typedef {TrackRow | LocalRow} Row
 */

/** Failed tries at a row's track info before the background fill stops
 *  asking for it (playing the row still asks). */
const MAX_TRACK_TRIES = 3;
/** Tracks asked for per request by the background fill. */
const PRELOAD_BATCH = 50;
/** Songs in a row that next/previous skips for not loading before giving up
 *  (Spotify unreachable: no point walking the whole list). */
const MAX_SKIPPED = 5;

/** @param {number} ms */
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

class PlaylistRow {
    /**
     * @type {HTMLElement | undefined}
     */
    element = $state();
    /** Local files (LocalRow) set this true; Spotify tracks leave it false. */
    isLocal = false;

    /**
     * @param {SpotifyUri} uri
     * @param {Playlist} playlist
     */
    constructor(uri, playlist) {
        this.uri = uri;
        this.playlist = playlist;
    }

    isLoaded() {
        return false;
    }

    play() {
        // noop
    }

    getOnEnterViewport() {
        return () => { };
    }
}

export class TrackRow extends PlaylistRow {
    /**
     * @type {SpotifyTrack | undefined}
     */
    track = $state()
    /**
     * @type {Promise<SpotifyTrack> | undefined}
     */
    trackPromise
    loadingMessage = $state("")
    displayName = $derived(this.track ? this.track.displayName : this.loadingMessage)
    displayDuration = $derived(this.track ? this.track.displayDuration : '')
    unavailable = $derived(this.track ? this.track.unavailable : false)

    /**
     * @param {SpotifyUri} uri
     * @param {Playlist} playlist
     */
    constructor(uri, playlist) {
        super(uri, playlist);
        // shown until the track's metadata loads (lazily, as the row scrolls in);
        // a raw "spotify:track:..." uri looks ugly, so use a subtle placeholder
        this.loadingMessage = "loading…";
    }

    /** Failed attempts at this row's track info (see MAX_TRACK_TRIES). */
    failures = 0;

    populateTrack() {
        if (!this.trackPromise) {
            this.trackPromise = this.settle(this.loadAlone());
        }

        return this.trackPromise;
    }

    /** One request for just this row's track info. */
    loadAlone() {
        return SpotifyTrack.loadFromUri(this.uri).catch((e) => {
            // "missing": Spotify has nothing for it, asking again won't help
            this.failures = e === "missing" ? MAX_TRACK_TRIES : this.failures + 1;
            throw e;
        });
    }

    /**
     * Keep the track `request` brings. On a failure forget the attempt, so
     * playing the row, scrolling to it or the next background pass asks
     * again: a busy moment shouldn't leave a song dead for the whole session.
     * @param {Promise<SpotifyTrack>} request
     * @returns {Promise<SpotifyTrack>}
     */
    settle(request) {
        const promise = request.then(
            (track) => {
                this.track = track;
                return track;
            },
            (e) => {
                if (this.trackPromise === promise) this.trackPromise = undefined;
                this.loadingMessage =
                    e === "missing" ? "Unavailable track"
                    : this.failures >= MAX_TRACK_TRIES ? "Couldn't load track info"
                    : "loading…";
                throw e;
            },
        );
        return promise;
    }

    /**
     * Fill in many rows' track info with one request. A row the batch has
     * nothing for is asked for on its own, one at a time, which also finds
     * out why.
     * @param {TrackRow[]} rows
     */
    static async populateMany(rows) {
        const batch = SpotifyTrack.loadMany(rows.map((row) => row.uri));
        /** @type {Promise<unknown>} */
        let alone = Promise.resolve();
        rows.forEach((row, i) => {
            const promise = row.settle(
                batch.then(
                    (tracks) => {
                        const track = tracks[i];
                        if (track) return track;
                        const turn = alone.then(() => row.loadAlone());
                        alone = turn.catch(() => {}).then(() => sleep(150));
                        return turn;
                    },
                    // The whole batch failing isn't this row's own failure:
                    // leave it on "loading…" for a later pass.
                    () => Promise.reject("busy"),
                ),
            );
            row.trackPromise = promise;
            promise.catch(() => {});
        });
        await batch;
        await alone;
    }

    getOnEnterViewport() {
        // NOTE: `this` is overridden with the HTMLElement when attaching event listeners to elements.
        //       We capture `this` as `self` before returning the actual event callback so that we can access `this` in the callback.
        const self = this;
        /**
         * @this HTMLElement
         */
        function eventCallback() {
            const element = this;
            // Stop watching only once it has loaded: a row that failed tries
            // again the next time it scrolls into view.
            self.populateTrack().then(
                () => enterExitViewportObserver.unobserve(element),
                (/** @type {unknown} */ e) => {
                    console.warn(`Could not load metadata for ${self.uri.id}`, e);
                },
            );
        };
        return eventCallback;
    }

    /** @param {boolean} [playNow] have the player start it (see play()) */
    async loadTrack(playNow = false) {
        try {
            await this.populateTrack().catch(async (e) => {
                // Once more after a moment: most failures are a busy second.
                if (e === "missing") throw e;
                await sleep(1500);
                return this.populateTrack();
            });
            if (this.track) {
                this.playlist.loadedRow = this;
                await emitWindowEvent("playlistWindow", { TrackLoaded: this.track, PlayNow: playNow });
                // broadcast the track's position for Discord's "(N of M)" party
                const rows = this.playlist.rows;
                await emitWindowEvent("trackPosition", {
                    index: rows.indexOf(this) + 1,
                    length: rows.length,
                });
                return this.track;
            }
        } catch (e) {
            console.warn(`Could not load track metadata for ${this.uri.id}`, e);
        }
    }

    async play() {
        // One message: load it and play it. Sending "loaded" and then "play"
        // made a player that was already playing load the new track twice.
        await this.loadTrack(true);
    }

    isLoaded() {
        return this == this.playlist.loadedRow;
    }

    isSelected() {
        return this.playlist.selectedRows.includes(this);
    }
}

/**
 * A local file (off disk) living in the playlist alongside Spotify rows, so
 * added files actually show up in the list instead of playing invisibly. It
 * carries its own name/duration (from the file's tags) and, when played, tells
 * the player to run it through the local engine rather than librespot.
 */
export class LocalRow extends PlaylistRow {
    displayName = $state("")
    displayDuration = $state("")
    unavailable = false
    isLocal = true

    /**
     * @param {string} path
     * @param {Playlist} playlist
     * @param {{name?: string, artist?: string, durationMs?: number, sampleRate?: number, kbps?: number}} [meta]
     */
    constructor(path, playlist, meta) {
        // PlaylistRow expects a Spotify uri; a local file has none, so pass a
        // minimal stub. The only readers of `row.uri` (persist / save-as-list)
        // filter local rows out, so the stub is never actually resolved.
        super(/** @type {any} */ ({ asString: `local:${path}`, id: path }), playlist);
        this.path = path;
        const base = path.split(/[\\/]/).pop() ?? path;
        const name = meta?.name || base;
        this.displayName = meta?.artist ? `${meta.artist} - ${name}` : name;
        this.durationMs = meta?.durationMs ?? 0;
        this.displayDuration = this.durationMs ? durationToString(this.durationMs) : "";
        this.sampleRate = meta?.sampleRate ?? 0;
        this.kbps = meta?.kbps ?? 0;
    }

    async loadTrack() {
        this.playlist.loadedRow = this;
        await emitWindowEvent("playlistWindow", {
            LocalTrackLoaded: {
                path: this.path,
                name: this.displayName,
                durationMs: this.durationMs,
                sampleRate: this.sampleRate,
                kbps: this.kbps,
            },
        });
        const rows = this.playlist.rows;
        await emitWindowEvent("trackPosition", {
            index: rows.indexOf(this) + 1,
            length: rows.length,
        });
        return true;
    }

    async play() {
        // The local engine auto-plays on load, so loadTrack is enough.
        await this.loadTrack();
    }

    isLoaded() {
        return this === this.playlist.loadedRow;
    }

    isSelected() {
        return this.playlist.selectedRows.includes(this);
    }
}

/**
 * A row's length in ms (0 while it isn't known yet).
 * @param {Row} r
 */
function rowMs(r) {
    return r instanceof LocalRow ? (r.durationMs ?? 0) : (r.track?.durationInMs ?? 0);
}

export class Playlist {
    width = $derived(Math.ceil(REACTIVE_WINDOW_SIZE.width / 25));
    height = $derived(Math.ceil(REACTIVE_WINDOW_SIZE.height / 29));

    /**
     * @type {Row | undefined}
     */
    loadedRow = $state();
    /**
     * @type {Row[]}
     */
    rows = $state([]);
    /**
     * @type {Row[]}
     */
    selectedRows = $state([]);
    /**
     * The currently "active" row (keyboard focus). Used as the target for
     * scroll-into-view and as the row that gets played on Enter.
     * @type {Row | undefined}
     */
    focusedRow = $state();
    /**
     * Fixed reference point for range (shift) selection.
     * @type {Row | undefined}
     */
    selectionAnchor = $state();

    /** Play in random order (toggled from the player's shuffle button). */
    shuffle = false;
    /** Rows already played this shuffle cycle. Without it a shuffled queue
     *  picks randomly forever and never signals "end reached", so autoplay
     *  radio (and plain stop) never fire. We play every row once, then the
     *  cycle ends — repeat restarts it, otherwise the queue is genuinely done.
     *  @type {Set<Row>} */
    shuffleBag = new Set();
    /** 0 = off, 1 = repeat all (wrap at ends), 2 = repeat one (loop track). */
    repeat = 0;
    /** When the queue runs out (repeat off), keep playing Spotify radio seeded
     *  from the last track. Toggled from the playlist menu. */
    autoplay = $state(false);
    /** Guards against firing autoplay twice for one end-of-queue. */
    autoplayBusy = false;
    /** Winamp's play queue (Q): rows that play next, in this order, before the
     *  normal (or shuffled) order resumes. Shown as [n] on the row.
     *  @type {Row[]} */
    queue = $state([]);
    /** J: the jump-to-file box over the playlist is open. */
    jumpOpen = $state(false);
    /** Ctrl+J: Winamp's jump to time (the box is the playlist page's) */
    timeJumpOpen = $state(false);
    /** Bumped when a letter with no shortcut is typed in the playlist, so the
     *  page can point at J (people expect typing to search). */
    typedHint = $state(0);
    /** Winamp's "stop after current" (Ctrl+V): when this track ends, stop
     *  instead of going on. One-shot: it switches itself off once it fires. */
    stopAfterCurrent = $state(false);
    /** Spotify track uris loved in Spotiamp+ (F, lists.rs set_loved).
     *  @type {Set<string>} */
    loved = $state(new Set());
    /** Where short messages go (the page sets its toast here).
     *  @type {(message: string) => void} */
    notify = () => {};
    /** Resume last session, a one-shot per launch: it waits for both the saved
     *  rows to be back and the player window to be listening (they start in
     *  parallel, so either can come first). */
    resumeDone = false;
    rowsRestored = false;
    playerReady = false;
    /** The row auto-loaded while restoring (addTrackRow loads the first one so
     *  the player shows something). Only a default: the resume cue replaces it.
     *  @type {Row | undefined} */
    startupRow = undefined;

    /** Elapsed time of the current track in ms (fed by player events). */
    positionMs = $state(0);
    /** Total time of all loaded tracks in ms (for the bottom-bar readout). */
    totalDurationMs = $derived(this.rows.reduce((sum, r) => sum + rowMs(r), 0));
    /** ...and of the selected ones (Winamp shows both: selected/total). */
    selectedDurationMs = $derived(this.selectedRows.reduce((sum, r) => sum + rowMs(r), 0));

    /**
     * @argument {string[]} uris
     * @argument {boolean} [persistEnabled] false in controller mode, where the
     *   playlist runs empty — persisting would wipe the URIs saved for
     *   Premium mode.
     */
    constructor(uris, persistEnabled = true) {
        this.persistEnabled = persistEnabled;
        $effect(() => {
            const focusedRow = this.focusedRow;
            if (!focusedRow?.element) {
                return;
            }

            const selectedRowElement = focusedRow.element;
            const rect = selectedRowElement.getBoundingClientRect();
            // The rect is in on-screen (UI-scaled) pixels, the limits in the
            // page's own units.
            const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;

            if (rect.top / zoom < 20) {
                selectedRowElement.scrollIntoView(true);
            } else if (rect.bottom / zoom > this.height * 29 - 38) {
                selectedRowElement.scrollIntoView(false);
            }
        })

        /**
        * @param {DocumentEventMap["keydown"]} e
        */
        const playlistKeyDownListener = (e) => {
            const t = /** @type {HTMLElement} */ (e.target);
            if (
                t &&
                (t.tagName == "INPUT" ||
                    t.tagName == "TEXTAREA" ||
                    t.isContentEditable)
            ) {
                return;
            }
            const ctrl = e.ctrlKey || e.metaKey;
            if (e.key == "ArrowDown") {
                e.preventDefault();
                if (e.altKey) {
                    this.moveSelected(1);
                } else {
                    this.selectRelative(1, e.shiftKey);
                }
            } else if (e.key == "ArrowUp") {
                e.preventDefault();
                if (e.altKey) {
                    this.moveSelected(-1);
                } else {
                    this.selectRelative(-1, e.shiftKey);
                }
            } else if (e.key == "Delete" || e.key == "Backspace") {
                e.preventDefault();
                this.removeSelected();
            } else if (e.key == "Enter") {
                e.preventDefault();
                this.playSelected();
            } else if (ctrl && e.key.toLowerCase() == "a") {
                e.preventDefault();
                this.selectedRows = [...this.rows];
            } else if (ctrl && !e.altKey && e.key.toLowerCase() == "v") {
                // Ctrl+V: Winamp's stop after current
                e.preventDefault();
                this.toggleStopAfterCurrent();
            } else if (ctrl && !e.altKey && e.key.toLowerCase() == "d") {
                // Ctrl+D: scale, handled by the main window
                e.preventDefault();
                emitWindowEvent("forwardedKey", { key: e.key, shift: e.shiftKey, ctrl: true });
            } else if (ctrl && !e.altKey && e.key.toLowerCase() == "j") {
                // Ctrl+J: Winamp's jump to time
                e.preventDefault();
                this.timeJumpOpen = true;
            } else if (!ctrl && !e.altKey) {
                // Winamp transport keys, forwarded to the player
                const k = e.key.toLowerCase();
                if (k == "z") {
                    e.preventDefault();
                    this.previous(true);
                } else if (k == "b") {
                    e.preventDefault();
                    this.next(true);
                } else if (k == "x") {
                    e.preventDefault();
                    emitWindowEvent("playlistWindow", { PlayRequested: null });
                } else if (k == "c") {
                    e.preventDefault();
                    emitWindowEvent("playlistWindow", { PauseRequested: null });
                } else if (k == "v") {
                    e.preventDefault();
                    emitWindowEvent("playlistWindow", { StopRequested: null });
                } else if (k == "q") {
                    // Winamp's queue: play the selection next
                    e.preventDefault();
                    this.toggleQueue();
                } else if (k == "j") {
                    // Winamp's jump-to-file
                    e.preventDefault();
                    this.openJump();
                } else if (k == "f") {
                    // love (or unlove) the selection
                    e.preventDefault();
                    this.toggleLoveSelected();
                } else if (
                    k == "s" || k == "r" || k == "l" || k == "o" || k == "f1" ||
                    (k == " " && t?.tagName != "BUTTON")
                ) {
                    // The main window's other keys (shuffle, repeat, library,
                    // add files, play/pause) work here too: it handles them.
                    e.preventDefault();
                    emitWindowEvent("forwardedKey", { key: e.key, shift: e.shiftKey, ctrl: false });
                } else if (/^[\p{L}\p{N}]$/u.test(e.key) && this.rows.length) {
                    // Typing here doesn't search (the letters are shortcuts):
                    // let the page show where searching is.
                    this.typedHint++;
                }
            }
        }
        document.addEventListener("keydown", playlistKeyDownListener);

        // Loved songs: loaded once, then kept in step with every window
        // (the main window's F, the Library).
        this.loadLoved();
        listen("lovedChanged", () => this.loadLoved()).catch(() => {});

        const playerWindowSubscription = subscribeToWindowEvent(
            "playerWindow",
            (event) => {
                if (event.NextPressed !== undefined) {
                    this.next(true);
                } else if (event.TrackEnded !== undefined) {
                    // a local file played to its end (Spotify's comes on "player")
                    this.trackEnded();
                } else if (event.StopAfterCurrentToggle !== undefined) {
                    this.toggleStopAfterCurrent();
                } else if (event.PreviousPressed !== undefined) {
                    this.previous(true);
                } else if (event.ShuffleChanged !== undefined) {
                    this.shuffle = event.ShuffleChanged;
                    // Fresh cycle each time shuffle is toggled.
                    this.shuffleBag.clear();
                } else if (event.RepeatChanged !== undefined) {
                    this.repeat = event.RepeatChanged;
                } else if (event.UrlsDropped) {
                    const urls = event.UrlsDropped;
                    this.clear().then(() => {
                        this.addUrls(urls, false);
                    });
                } else if (event.UrlsAppended) {
                    // append without clearing (e.g. adding one search result)
                    this.addUrls(event.UrlsAppended);
                } else if (event.AddLocalFiles) {
                    // local files picked from the player's O / Shift+O
                    this.addLocalFiles(event.AddLocalFiles);
                } else if (event.JumpRequested !== undefined) {
                    // J pressed in the main window, like Winamp
                    this.openJump();
                } else if (event.JumpToTimeRequested !== undefined) {
                    // Ctrl+J pressed in the main window
                    this.timeJumpOpen = true;
                } else if (event.QueueRequested !== undefined) {
                    // Q pressed in the main window: queue the playlist's selection
                    this.toggleQueue();
                } else if (event.PlayerReady !== undefined) {
                    this.playerReady = true;
                    this.maybeCueResume();
                }
            },
        );

        const playerSubscription = subscribeToWindowEvent("player", (event) => {
            if (event.EndOfTrack) {
                this.trackEnded();
            } else if (event.Unavailable) {
                this.loadFailed(event.Unavailable.uri);
            } else if (event.Playing) {
                this.loadFailures = 0;
                this.positionMs = event.Playing.position_ms;
            } else if (event.PositionChanged) {
                this.positionMs = event.PositionChanged.position_ms;
            } else if (event.PositionCorrection) {
                this.positionMs = event.PositionCorrection.position_ms;
            } else if (event.Seeked) {
                this.positionMs = event.Seeked.position_ms;
            } else if (event.Paused) {
                this.positionMs = event.Paused.position_ms;
            } else if (event.Stopped) {
                this.positionMs = 0;
            }
        });

        (async () => {
            for (const uri of uris) {
                if (uri.startsWith("local:")) {
                    // A local file saved from a previous session.
                    await this.addLocalRow(uri.slice("local:".length));
                } else {
                    await this.addUri(SpotifyUri.fromString(uri));
                }
            }
            // Persist after the initial load so any legacy playlist/album URIs
            // get normalised to the individual track URIs they expand into.
            this.persist();
            this.startupRow = this.loadedRow;
            this.rowsRestored = true;
            this.maybeCueResume();
            this.schedulePreload(5000);
        })();

        this.dispose = () => {
            clearTimeout(this.preloadTimer);
            document.removeEventListener("keydown", playlistKeyDownListener);
            playerWindowSubscription.then((unlisten) => unlisten());
            playerSubscription.then((unlisten) => unlisten());
        }
    }
    async clear() {
        this.rows = [];
        this.selectedRows = [];
        this.focusedRow = undefined;
        this.selectionAnchor = undefined;
        this.loadedRow = undefined;
        this.shuffleBag.clear(); // drop refs to the now-gone rows
        this.queue = [];
        this.jumpOpen = false;
    }

    /**
     * Persist the current playlist as the ordered list of track URIs.
     */
    persist() {
        if (!this.persistEnabled) return;
        // Spotify rows persist as their uri; local rows as their "local:<path>"
        // stub — so the whole queue, in order, comes back after a restart. The
        // launch reload turns each back into the right kind of row.
        invoke("set_uris", {
            uris: this.rows.map((r) => r.uri.asString),
        });
    }

    /**
     * Add a single track row to the playlist (without persisting). A song
     * already in the list isn't added a second time.
     * @param {SpotifyUri} uri
     * @returns {Promise<boolean>} false if it was already there
     */
    async addTrackRow(uri) {
        if (this.rows.some((r) => r.uri?.asString === uri.asString)) return false;
        const row = new TrackRow(uri, this);
        this.rows.push(row);
        this.schedulePreload(3000);
        if (!this.loadedRow) {
            await row.loadTrack();
        }
        return true;
    }

    /** Bumped when an add skipped songs already in the list, so the page can
     *  say so instead of the drop seeming to do nothing. */
    duplicateNotice = $state({ count: 0, seq: 0 });

    /** @type {ReturnType<typeof setTimeout> | undefined} */
    preloadTimer = undefined;
    preloadRunning = false;
    preloadAgain = false;

    /**
     * Fill in every row's track info in the background, not just the rows that
     * have scrolled into view: the total time at the bottom then counts the
     * whole list, and J has every name ready. Starts a moment after rows stop
     * arriving and asks for PRELOAD_BATCH tracks per request, one request at a
     * time: librespot allows 300 requests per 30 s, shared with playback, and
     * one per song used it all up on a long playlist (songs then failed to
     * load or play). Not in Free Mode (no session to ask).
     * @param {number} delayMs
     */
    schedulePreload(delayMs) {
        if (!this.persistEnabled) return;
        clearTimeout(this.preloadTimer);
        this.preloadTimer = setTimeout(() => this.preloadAll(), delayMs);
    }

    async preloadAll() {
        // Wait for the saved playlist to finish coming back first.
        if (!this.rowsRestored) return this.schedulePreload(3000);
        if (this.preloadRunning) {
            this.preloadAgain = true;
            return;
        }
        this.preloadRunning = true;
        let failures = 0;
        try {
            const pending = this.unloadedRows();
            for (let i = 0; i < pending.length && failures < 3; i += PRELOAD_BATCH) {
                // skip rows loaded meanwhile or no longer in the list
                const rows = pending
                    .slice(i, i + PRELOAD_BATCH)
                    .filter((row) => !row.track && !row.trackPromise && this.rows.includes(row));
                if (rows.length === 0) continue;
                try {
                    await TrackRow.populateMany(rows);
                    failures = 0;
                } catch {
                    // Most likely the session isn't up (yet); the rows stay
                    // on "loading…" for the next round.
                    failures++;
                }
                await sleep(300);
            }
        } finally {
            this.preloadRunning = false;
        }
        // Several failures in a row: Spotify's unreachable for now, try later.
        if (failures >= 3) this.schedulePreload(60000);
        else if (this.preloadAgain) {
            this.preloadAgain = false;
            this.schedulePreload(1000);
        }
        // Rows that failed on their own get another go in a while.
        else if (this.unloadedRows().length) this.schedulePreload(30000);
    }

    /** Spotify rows still without track info that are worth asking for. */
    unloadedRows() {
        return /** @type {TrackRow[]} */ (
            this.rows.filter(
                (r) => r instanceof TrackRow && !r.track && !r.trackPromise && r.failures < MAX_TRACK_TRIES,
            )
        );
    }

    /**
     * Add a URI to the playlist. Playlist/album URIs are unwrapped into their
     * individual track URIs so the playlist always consists of concrete tracks
     * (whose metadata is still lazily loaded as they enter the viewport).
     * @param {SpotifyUri} uri
     * @returns {Promise<number>} how many songs were skipped as already listed
     */
    async addUri(uri) {
        let skipped = 0;
        if (uri.type == "playlist" || uri.type == "album") {
            // get_track_ids returns {uri, added_ms} refs (for the library's Date
            // column); here we only need the uris.
            /** @type {{uri: string, added_ms: number|null}[]} */
            let trackRefs;
            try {
                trackRefs = await invoke("get_track_ids", { uri: uri.asString });
            } catch (e) {
                console.warn(`Could not expand ${uri.asString}`, e);
                return 0;
            }
            for (const ref of trackRefs) {
                if (!(await this.addTrackRow(SpotifyUri.fromString(ref.uri)))) skipped++;
            }
        } else if (!(await this.addTrackRow(uri))) {
            skipped++;
        }
        return skipped;
    }

    /**
     * @param {string[]} urls
     * @param {boolean} [notify] say if songs already in the list were skipped
     *   (off when the list was just cleared for these: nothing was "already" there)
     */
    async addUrls(urls, notify = true) {
        let skipped = 0;
        for (const url of urls) {
            skipped += await this.addUri(SpotifyUri.fromUrl(url));
        }
        this.persist();
        if (notify && skipped) {
            this.duplicateNotice = { count: skipped, seq: this.duplicateNotice.seq + 1 };
        }
    }

    /**
     * Add local files (off disk) as rows and start playing the first one. Each
     * row pulls its name/duration from the file's tags. Called from the playlist
     * menu directly and from the player's O / Shift+O via an AddLocalFiles event.
     * @param {string[]} paths
     */
    async addLocalFiles(paths) {
        /** @type {LocalRow | undefined} */
        let first;
        for (const path of paths) {
            if (typeof path !== "string" || !path) continue;
            const row = await this.addLocalRow(path);
            first ??= row;
        }
        if (first) {
            await first.play();
            this.persist();
        }
    }

    /**
     * Replace the playlist with an opened .m3u's songs, in the file's order
     * (Spotify songs and local files mixed as they come).
     * @param {{kind: "spotify" | "local", value: string}[]} entries
     */
    async openM3uEntries(entries) {
        await this.clear();
        for (const e of entries) {
            if (e.kind === "spotify") {
                await this.addTrackRow(SpotifyUri.fromString(e.value));
            } else {
                await this.addLocalRow(e.value);
            }
        }
        this.persist();
    }

    /** The rows as .m3u lines: where each is, its name and its length. */
    async m3uRows() {
        // names of rows that haven't been in view yet
        await this.loadAllNames().catch(() => {});
        return this.rows.map((r) =>
            r instanceof LocalRow
                ? { location: r.path, title: r.displayName, duration_ms: r.durationMs ?? 0 }
                : {
                      location: r.uri.asString,
                      title: r.track ? r.displayName : "",
                      duration_ms: r.track?.durationInMs ?? 0,
                  },
        );
    }

    /**
     * Append one local file as a row (reading its tags), without playing it.
     * Used both by addLocalFiles and by the launch reload.
     * @param {string} path
     */
    async addLocalRow(path) {
        // Already listed: hand back that row instead of adding it again.
        const existing = this.rows.find((r) => r instanceof LocalRow && r.path === path);
        if (existing) return /** @type {LocalRow} */ (existing);
        let meta;
        try {
            const m = await invoke("local_metadata", { path });
            meta = {
                name: m?.title || undefined,
                artist: m?.artist || undefined,
                durationMs: m?.duration_ms || 0,
                sampleRate: m?.sample_rate || 0,
                kbps: m?.kbps || 0,
            };
        } catch {
            meta = undefined;
        }
        const row = new LocalRow(path, this, meta);
        this.rows.push(row);
        return row;
    }

    /**
     * Update the selection for a row, mimicking native multi-select behaviour.
     *
     * @param {Row} row
     * @param {{ ctrl?: boolean, shift?: boolean }} [modifiers]
     */
    select(row, { ctrl = false, shift = false } = {}) {
        const index = this.rows.indexOf(row);
        if (index === -1) {
            return;
        }

        if (shift && this.selectionAnchor) {
            const anchorIndex = this.rows.indexOf(this.selectionAnchor);
            if (anchorIndex !== -1) {
                const [start, end] =
                    anchorIndex <= index ? [anchorIndex, index] : [index, anchorIndex];
                this.selectedRows = this.rows.slice(start, end + 1);
                this.focusedRow = row;
                return;
            }
        }

        if (ctrl) {
            this.selectedRows = this.selectedRows.includes(row)
                ? this.selectedRows.filter((r) => r !== row)
                : [...this.selectedRows, row];
        } else {
            this.selectedRows = [row];
        }
        this.selectionAnchor = row;
        this.focusedRow = row;
    }

    /**
     * Move the keyboard focus by `offset`, optionally extending the selection
     * from the current anchor (shift + arrow).
     *
     * @param {number} offset
     * @param {boolean} [extend]
     */
    selectRelative(offset, extend = false) {
        const current = this.focusedRow ?? this.selectedRows[0];
        const baseIndex = current ? this.rows.indexOf(current) : -1;
        const row = this.rows[baseIndex + offset];
        if (row) {
            this.select(row, { shift: extend });
        }
    }

    /**
     * Move all selected rows as a group by one step in `offset` direction.
     *
     * @param {number} offset -1 to move up, 1 to move down
     */
    moveSelected(offset) {
        if (this.selectedRows.length === 0 || (offset !== 1 && offset !== -1)) {
            return;
        }

        const indices = this.selectedRows
            .map((r) => this.rows.indexOf(r))
            .sort((a, b) => a - b);

        // Can't move past the edges of the list.
        if (offset < 0 && indices[0] === 0) {
            return;
        }
        if (offset > 0 && indices[indices.length - 1] === this.rows.length - 1) {
            return;
        }

        // When moving down, swap from the bottom-most row first so rows don't
        // clobber each other.
        const order = offset < 0 ? indices : [...indices].reverse();
        const rows = [...this.rows];
        for (const i of order) {
            const j = i + offset;
            [rows[i], rows[j]] = [rows[j], rows[i]];
        }
        this.rows = rows;
        this.persist();
    }

    /**
     * Rebuild the rows by inserting the dragged `block` into `remaining` at
     * `insertAt` (clamped). Used for drag-to-reorder: `block` and `remaining`
     * are snapshots captured when the drag started, so only the insertion
     * point changes as the pointer moves.
     *
     * @param {Row[]} block the rows being dragged
     * @param {Row[]} remaining the non-dragged rows, in order
     * @param {number} insertAt insertion index within `remaining`
     */
    placeSelection(block, remaining, insertAt) {
        const clamped = Math.max(0, Math.min(remaining.length, insertAt));
        const next = [...remaining];
        next.splice(clamped, 0, ...block);
        this.rows = next;
    }

    /**
     * Remove the selected rows from the playlist, then focus a neighbouring row.
     */
    removeSelected() {
        if (this.selectedRows.length === 0) {
            return;
        }

        const removed = new Set(this.selectedRows);
        const firstIndex = Math.min(
            ...this.selectedRows.map((r) => this.rows.indexOf(r)),
        );

        if (this.loadedRow && removed.has(this.loadedRow)) {
            this.loadedRow = undefined;
        }

        this.rows = this.rows.filter((r) => !removed.has(r));
        this.queue = this.queue.filter((r) => !removed.has(r));

        const next = this.rows[firstIndex] ?? this.rows[firstIndex - 1];
        if (next) {
            this.select(next);
        } else {
            this.selectedRows = [];
            this.focusedRow = undefined;
            this.selectionAnchor = undefined;
        }

        this.persist();
    }

    /**
     * Play the focused row (falling back to the first selected row).
     */
    playSelected() {
        const row = this.focusedRow ?? this.selectedRows[0];
        row?.play();
    }

    /** The playing track reached its end (Spotify or local file alike). */
    trackEnded() {
        if (this.stopAfterCurrent) {
            this.stopAfterCurrent = false;
            emitWindowEvent("playlistWindow", { StopRequested: null });
            return;
        }
        // repeat one: replay the current track instead of advancing
        if (this.repeat === 2 && this.loadedRow) {
            this.loadedRow.loadTrack();
            return;
        }
        this.next(true).then((endReached) => {
            if (!endReached) return;
            if (this.autoplay) {
                this.autoplayFromRadio();
            } else {
                emitWindowEvent("playlistWindow", { EndReached: null });
            }
        });
    }

    /** Songs that failed to load in a row (reset when one plays). */
    loadFailures = 0;

    /**
     * The playing song couldn't be loaded. Move on like Spotify's own apps
     * do, but not forever: when Spotify isn't sending songs at all (it refuses
     * them for a minute or two now and then), skipping through the whole list
     * only makes it worse, so after three in a row stop and say so.
     * @param {string} uri
     */
    loadFailed(uri) {
        // a late failure of a song that's no longer the one loaded
        if (uri !== this.loadedRow?.uri?.asString) return;
        this.loadFailures += 1;
        if (this.loadFailures > 3) {
            this.loadFailures = 0;
            emitWindowEvent("playlistWindow", { StopRequested: null });
            this.notify("Spotify isn't sending songs right now. Try again in a minute.");
            return;
        }
        this.next(true).then((endReached) => {
            if (endReached) emitWindowEvent("playlistWindow", { EndReached: null });
        });
    }

    toggleStopAfterCurrent() {
        this.stopAfterCurrent = !this.stopAfterCurrent;
        // the main window shows it in its ticker
        emitWindowEvent("playlistWindow", { StopAfterCurrentChanged: this.stopAfterCurrent });
    }

    async loadLoved() {
        try {
            this.loved = new Set(/** @type {string[]} */ (await invoke("get_loved")));
        } catch {
            /* keep what we had */
        }
    }

    /** Spotify uris of the selected rows (local files can't be loved or linked). */
    selectedSpotifyUris() {
        return this.selectedRows.filter((r) => !r.isLocal).map((r) => r.uri.asString);
    }

    /**
     * F: love the selected Spotify songs, or unlove them if they all already
     * are. Loved songs are in the Library under "Loved songs".
     */
    async toggleLoveSelected() {
        const uris = this.selectedSpotifyUris();
        if (!uris.length) {
            this.notify(this.selectedRows.length ? "Only Spotify songs can be loved" : "Select a song to love first");
            return;
        }
        const love = !uris.every((u) => this.loved.has(u));
        await invoke("set_loved", { uris, loved: love }).catch(() => {});
        await this.loadLoved();
        const what = uris.length === 1 ? "" : ` ${uris.length} songs`;
        this.notify(love ? `♡ Loved${what}: it's in the Library under Loved songs` : `Unloved${what}`);
    }

    /**
     * Q: add the selected rows to the play-next queue, or take them back out if
     * they're already queued (a multi-selection goes in playlist order).
     */
    toggleQueue() {
        const picked = this.selectedRows.length
            ? this.selectedRows
            : this.focusedRow
              ? [this.focusedRow]
              : [];
        let queue = [...this.queue];
        for (const row of this.rows.filter((r) => picked.includes(r))) {
            queue = queue.includes(row)
                ? queue.filter((r) => r !== row)
                : [...queue, row];
        }
        this.queue = queue;
    }

    /**
     * 1-based position of `row` in the play queue, or 0 when it isn't queued.
     * @param {Row} row
     */
    queuePosition(row) {
        return this.queue.indexOf(row) + 1;
    }

    /**
     * Resume last session: once the saved rows are back and the player is
     * listening, select the track that was playing at the last exit and hand
     * it to the player without playing it; the player's first play then picks
     * up from the saved position. Runs once per launch, and not at all if the
     * user already loaded something (the first row auto-loaded while restoring
     * doesn't count). Only Spotify tracks are saved (a local row would start
     * playing just by being loaded).
     */
    async maybeCueResume() {
        if (this.resumeDone || !this.rowsRestored || !this.playerReady) return;
        this.resumeDone = true;
        /** @type {{uri?: string, index?: number} | undefined} */
        let point;
        try {
            point = /** @type {any} */ (await invoke("get_player_settings"))?.resume;
        } catch {
            return;
        }
        // Skip if the user already picked something themselves; the first row
        // auto-loaded during restore doesn't count.
        if (!point?.uri || (this.loadedRow && this.loadedRow !== this.startupRow)) return;
        const byIndex = this.rows[(point.index ?? 0) - 1];
        const row =
            byIndex?.uri?.asString === point.uri
                ? byIndex
                : this.rows.find((r) => r.uri?.asString === point.uri);
        if (!(row instanceof TrackRow)) return;
        this.select(row);
        await row.loadTrack();
    }

    /** J: open the jump-to-file box (the page renders it). */
    openJump() {
        if (this.rows.length) this.jumpOpen = true;
    }

    /**
     * Load the names of Spotify rows that haven't scrolled into view yet, so the
     * jump box searches the whole playlist instead of only what's been seen:
     * the background fill, started now instead of waiting for its turn.
     */
    async loadAllNames() {
        if (this.preloadRunning) return;
        clearTimeout(this.preloadTimer);
        await this.preloadAll();
    }

    /**
     * @param {number} offset
     * @param {boolean} skipUnavailable
     * @param {Row} [fromRow] move on from this row instead of the loaded one
     *   (a row that couldn't be loaded never becomes the loaded one)
     * @param {number} [skipped] rows skipped so far, to give up at some point
     * @returns {Promise<boolean>} true if the end in that direction has been reached
     */
    async move(offset, skipUnavailable, fromRow = undefined, skipped = 0) {
        if (this.rows.length === 0) {
            return true;
        }
        const fromHere = fromRow ?? this.loadedRow;
        const currRowIndex = fromHere
            ? this.rows.indexOf(fromHere)
            : 0;

        // Queued rows (Q) play next, in queue order, before the normal or
        // shuffled order picks up again. A queued row removed from the playlist
        // in the meantime is simply skipped.
        if (offset > 0 && this.queue.length) {
            const [row, ...rest] = this.queue;
            this.queue = rest;
            if (!this.rows.includes(row)) {
                return await this.move(offset, skipUnavailable);
            }
            if (this.shuffle) {
                if (this.loadedRow) this.shuffleBag.add(this.loadedRow);
                this.shuffleBag.add(row);
            }
            if (row instanceof TrackRow) {
                const track = await row.loadTrack();
                if (track && skipUnavailable && track.unavailable) {
                    return await this.move(offset, skipUnavailable);
                }
                if (!track && skipUnavailable && skipped < MAX_SKIPPED) {
                    return await this.move(offset, skipUnavailable, fromHere ?? undefined, skipped + 1);
                }
            } else if (row instanceof LocalRow) {
                await row.loadTrack();
            }
            return false;
        }

        let nextIndex;
        if (this.shuffle && offset > 0) {
            // Shuffle only drives forward playback; previous stays sequential.
            // Remember the row we're leaving, then pick from the ones not yet
            // played this cycle so every track plays once before the queue is
            // "done". When the bag is full the cycle has ended: repeat restarts
            // it, otherwise we return end-reached so radio autoplay / stop can
            // kick in (the bug: this branch used to pick randomly forever).
            if (this.loadedRow) this.shuffleBag.add(this.loadedRow);
            const remaining = this.rows.filter((r) => !this.shuffleBag.has(r));
            if (remaining.length === 0) {
                if (this.repeat) {
                    this.shuffleBag.clear();
                    nextIndex = this.pickRandomIndex(currRowIndex);
                } else {
                    return true; // whole queue played once → end reached
                }
            } else {
                const pick =
                    remaining[Math.floor(Math.random() * remaining.length)];
                // Mark it played now (not just via loadedRow next time) so a
                // skipped-unavailable retry can't re-pick it and spin forever.
                this.shuffleBag.add(pick);
                nextIndex = this.rows.indexOf(pick);
            }
        } else {
            nextIndex = currRowIndex + offset;
            if (nextIndex < 0 || nextIndex >= this.rows.length) {
                if (this.repeat) {
                    nextIndex =
                        ((nextIndex % this.rows.length) + this.rows.length) %
                        this.rows.length;
                } else {
                    return true; // end (or top) reached
                }
            }
        }

        const row = this.rows[nextIndex];
        if (!row) {
            return true;
        }

        if (row instanceof TrackRow) {
            const track = await row.loadTrack();
            if (track && skipUnavailable && track.unavailable) {
                return await this.move(offset, skipUnavailable);
            }
            // Its info wouldn't load even on a second try: go on to the next
            // song instead of the music just stopping there.
            if (!track && skipUnavailable && skipped < MAX_SKIPPED) {
                return await this.move(offset, skipUnavailable, row, skipped + 1);
            }
        } else if (row instanceof LocalRow) {
            await row.loadTrack(); // plays through the local engine
        }

        return false;
    }

    /**
     * Pick a random row index, avoiding `exclude` when there's a choice.
     * @param {number} exclude
     */
    pickRandomIndex(exclude) {
        if (this.rows.length <= 1) {
            return 0;
        }
        let index;
        do {
            index = Math.floor(Math.random() * this.rows.length);
        } while (index === exclude);
        return index;
    }

    /**
     * @param {boolean} skipUnavailable
     * @returns {Promise<boolean>} true if end has been reached
     */
    async next(skipUnavailable = false) {
        return await this.move(1, skipUnavailable);
    }

    /**
     * @param {boolean} skipUnavailable
     * @returns {Promise<boolean>} true if top has been reached
     */
    async previous(skipUnavailable = false) {
        return await this.move(-1, skipUnavailable);
    }

    /**
     * The queue ran out with autoplay on: append Spotify radio seeded from the
     * last track and keep playing. Falls back to stopping if nothing comes back.
     */
    /**
     * Instant mix: 20 songs like the selected one (or the one playing), from
     * Spotify's radio for it, skipping any already in the list. Added at the
     * end, or queued to play next.
     * @param {"append" | "queue"} how
     * @returns {Promise<{added: number, seed: string}>} seed = its name
     */
    async instantMix(how) {
        const seedRow =
            this.selectedRows.find((r) => r instanceof TrackRow) ??
            (this.loadedRow instanceof TrackRow ? this.loadedRow : undefined);
        const seed = seedRow?.uri?.asString;
        if (!seed || !(seedRow instanceof TrackRow)) return { added: 0, seed: "" };
        const uris = /** @type {string[]} */ (await invoke("get_radio", { uri: seed }));
        const have = new Set(this.rows.map((r) => r.uri?.asString));
        const picks = (uris ?? []).filter((u) => !have.has(u)).slice(0, 20);
        /** @type {Row[]} */
        const added = [];
        for (const uri of picks) {
            if (await this.addTrackRow(SpotifyUri.fromString(uri))) {
                added.push(this.rows[this.rows.length - 1]);
            }
        }
        if (how === "queue") this.queue = [...this.queue, ...added];
        this.persist();
        return { added: added.length, seed: seedRow.track?.name ?? "" };
    }

    async autoplayFromRadio() {
        if (this.autoplayBusy) return;
        this.autoplayBusy = true;
        try {
            const seed =
                this.loadedRow?.uri?.asString ??
                this.rows[this.rows.length - 1]?.uri?.asString;
            if (!seed) {
                emitWindowEvent("playlistWindow", { EndReached: null });
                return;
            }
            const uris = await invoke("get_radio", { uri: seed });
            if (!uris || uris.length === 0) {
                emitWindowEvent("playlistWindow", { EndReached: null });
                return;
            }
            for (const uri of uris) {
                await this.addTrackRow(SpotifyUri.fromString(uri));
            }
            this.persist();
            // advance into the freshly-appended radio tracks and play
            const endReached = await this.next(true);
            if (endReached) {
                emitWindowEvent("playlistWindow", { EndReached: null });
            }
        } catch {
            emitWindowEvent("playlistWindow", { EndReached: null });
        } finally {
            this.autoplayBusy = false;
        }
    }
}
