import { invoke } from "@tauri-apps/api/core";
import { enterExitViewportObserver, REACTIVE_WINDOW_SIZE } from "./common.svelte";
import { emitWindowEvent, subscribeToWindowEvent } from "./events.svelte";
import { SpotifyTrack, SpotifyUri, durationToString } from "./spotify.svelte";

/**
 * A playlist row: either a Spotify track or a local file. Both extend
 * PlaylistRow and expose displayName / displayDuration / play() / isLoaded etc.
 * @typedef {TrackRow | LocalRow} Row
 */

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

    populateTrack() {
        if (!this.trackPromise) {
            this.trackPromise = SpotifyTrack.loadFromUri(this.uri)
                .then((track) => {
                    this.track = track;
                    return track;
                })
                .catch((e) => {
                    this.loadingMessage = `Failed to load track ${this.uri.id} (${e})`;
                    throw e;
                });
        }

        return this.trackPromise;
    }

    getOnEnterViewport() {
        // NOTE: `this` is overridden with the HTMLElement when attaching event listeners to elements.
        //       We capture `this` as `self` before returning the actual event callback so that we can access `this` in the callback.
        const self = this;
        /**
         * @this HTMLElement
         */
        function eventCallback() {
            enterExitViewportObserver.unobserve(this);
            self.populateTrack().catch((/** @type {unknown} */ e) => {
                console.warn(`Could not load metadata for ${self.uri.id}`, e);
            });
        };
        return eventCallback;
    }

    async loadTrack() {
        try {
            await this.populateTrack();
            if (this.track) {
                this.playlist.loadedRow = this;
                await emitWindowEvent("playlistWindow", { TrackLoaded: this.track });
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
        await this.loadTrack();
        await emitWindowEvent("playlistWindow", { PlayRequested: null })
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
     * @param {{name?: string, artist?: string, durationMs?: number}} [meta]
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
    }

    async loadTrack() {
        this.playlist.loadedRow = this;
        await emitWindowEvent("playlistWindow", {
            LocalTrackLoaded: {
                path: this.path,
                name: this.displayName,
                durationMs: this.durationMs,
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

    /** Elapsed time of the current track in ms (fed by player events). */
    positionMs = $state(0);
    /** Total time of all loaded tracks in ms (for the bottom-bar readout). */
    totalDurationMs = $derived(
        this.rows.reduce(
            (sum, r) =>
                sum +
                (r instanceof LocalRow
                    ? (r.durationMs ?? 0)
                    : (r.track?.durationInMs ?? 0)),
            0,
        ),
    );

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

            if (rect.top < 20) {
                selectedRowElement.scrollIntoView(true);
            } else if (rect.bottom > this.height * 29 - 38) {
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
                }
            }
        }
        document.addEventListener("keydown", playlistKeyDownListener);

        const playerWindowSubscription = subscribeToWindowEvent(
            "playerWindow",
            (event) => {
                if (event.NextPressed !== undefined) {
                    this.next(true);
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
                        this.addUrls(urls);
                    });
                } else if (event.UrlsAppended) {
                    // append without clearing (e.g. adding one search result)
                    this.addUrls(event.UrlsAppended);
                } else if (event.AddLocalFiles) {
                    // local files picked from the player's O / Shift+O
                    this.addLocalFiles(event.AddLocalFiles);
                }
            },
        );

        const playerSubscription = subscribeToWindowEvent("player", (event) => {
            if (event.EndOfTrack) {
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
            } else if (event.Playing) {
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
        })();

        this.dispose = () => {
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
     * Add a single track row to the playlist (without persisting).
     * @param {SpotifyUri} uri
     */
    async addTrackRow(uri) {
        const row = new TrackRow(uri, this);
        this.rows.push(row);
        if (!this.loadedRow) {
            await row.loadTrack();
        }
    }

    /**
     * Add a URI to the playlist. Playlist/album URIs are unwrapped into their
     * individual track URIs so the playlist always consists of concrete tracks
     * (whose metadata is still lazily loaded as they enter the viewport).
     * @param {SpotifyUri} uri
     */
    async addUri(uri) {
        if (uri.type == "playlist" || uri.type == "album") {
            // get_track_ids returns {uri, added_ms} refs (for the library's Date
            // column); here we only need the uris.
            /** @type {{uri: string, added_ms: number|null}[]} */
            let trackRefs;
            try {
                trackRefs = await invoke("get_track_ids", { uri: uri.asString });
            } catch (e) {
                console.warn(`Could not expand ${uri.asString}`, e);
                return;
            }
            for (const ref of trackRefs) {
                await this.addTrackRow(SpotifyUri.fromString(ref.uri));
            }
        } else {
            await this.addTrackRow(uri);
        }
    }

    /**
     * @param {string[]} urls
     */
    async addUrls(urls) {
        for (const url of urls) {
            await this.addUri(SpotifyUri.fromUrl(url));
        }
        this.persist();
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
     * Append one local file as a row (reading its tags), without playing it.
     * Used both by addLocalFiles and by the launch reload.
     * @param {string} path
     */
    async addLocalRow(path) {
        let meta;
        try {
            const m = await invoke("local_metadata", { path });
            meta = {
                name: m?.title || undefined,
                artist: m?.artist || undefined,
                durationMs: m?.duration_ms || 0,
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

    /**
     * @param {number} offset
     * @param {boolean} skipUnavailable
     * @returns {Promise<boolean>} true if the end in that direction has been reached
     */
    async move(offset, skipUnavailable) {
        if (this.rows.length === 0) {
            return true;
        }
        const currRowIndex = this.loadedRow
            ? this.rows.indexOf(this.loadedRow)
            : 0;

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
        } else if (row instanceof LocalRow) {
            await row.loadTrack(); // plays through the local engine
        }

        return false;
    }

    /** Pick a random row index, avoiding `exclude` when there's a choice. */
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
