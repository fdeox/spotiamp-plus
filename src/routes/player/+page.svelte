<script>
  import { invoke } from "@tauri-apps/api/core";

  import {
    handleError,
    handleDrop,
    REACTIVE_WINDOW_SIZE,
  } from "$lib/common.svelte.js";
  import {
    emitWindowEvent,
    subscribeToWindowEvent,
  } from "$lib/events.svelte.js";
  import {
    durationToMMSS,
    durationToString,
    SpotifyTrack,
  } from "$lib/spotify.svelte.js";
  import TextTicker from "../../TextTicker.svelte";
  import NumberDisplay from "../../NumberDisplay.svelte";
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Visualizer } from "$lib/visualizer.svelte";
  import {
    currentMonitor,
    getCurrentWindow,
    Window,
  } from "@tauri-apps/api/window";
  import {
    boundingRect,
    isDocked,
    makeTauriWindowDraggable,
    rectFromPositionAndSize,
    SNAP_DISTANCE,
    snapPosition,
    snapRectIntoBounds,
    STICKY_SNAP_DISTANCE,
  } from "$lib/window-docking.svelte.js";

  /** @type {{data: import('./$types').PageData}} */
  const { data: playerSettings } = $props();

  // Controller ("free") mode: no librespot player exists — the page mirrors
  // and drives the official Spotify app through the smtc_* commands instead.
  // Every player-backend invoke below is guarded on this.
  const controllerMode = playerSettings.controller_mode === true;

  function initialVolume() {
    return playerSettings.volume;
  }

  function initialShowPlaylist() {
    return playerSettings.show_playlist;
  }

  function initialDoubleSizeActive() {
    return playerSettings.double_size_active;
  }

  function initialWindowshadeActive() {
    return playerSettings.windowshade_active ?? false;
  }

  /**
   * @type {SpotifyTrack | undefined}
   */
  let loadedTrack = $state();
  let volume = $state(initialVolume());
  // -100 (full left) .. 0 (centre) .. +100 (full right)
  let balance = $state(0);
  let sliderSeekPosition = $state(0);
  let seekPosition = $state(0);
  // Wall-clock anchor for interpolating the playback position between backend
  // updates. Advancing seekPosition by elapsed real time (rather than a fixed
  // +1s per tick) keeps the clock from drifting when timers fire irregularly.
  let positionAnchorMs = 0;
  let positionAnchorAt = 0;

  /**
   * Set the playback position and re-anchor the interpolation clock to now.
   * @param {number} positionMs
   */
  function setPosition(positionMs) {
    seekPosition = positionMs;
    positionAnchorMs = positionMs;
    positionAnchorAt = performance.now();
  }
  let showPlaylist = $state(initialShowPlaylist());
  let showEq = $state(false);
  // 🦙 hidden about screen (click the titlebar logo)
  let showLlama = $state(false);
  let appVersion = $state("");
  // Version comes from the Rust side (Tauri config), so the About box can't
  // drift from the version that actually shipped.
  async function openAbout() {
    if (!appVersion) appVersion = await invoke("app_version").catch(() => "");
    showLlama = true;
  }
  /** @param {"github" | "discord" | "license"} target — must match the Rust allowlist */
  const openLink = (target) => invoke("open_external", { target }).catch(() => {});
  // the currently-playing track uri (from backend events), broadcast to the
  // lyrics window along with the interpolated position
  let currentTrackUri = $state(null);
  // track's place in the playlist, for Discord's "(N of M)" party
  let playlistPos = $state({ index: 0, length: 0 });
  let doubleSizeActive = $state(initialDoubleSizeActive());
  let shadeActive = $state(initialWindowshadeActive());
  let shuffle = $state(false);
  // 0 = off, 1 = repeat all (restart playlist), 2 = repeat one (loop track)
  let repeat = $state(0);
  /**
   * @type {'nothing' | 'seeking' | 'volume-change' | 'balance-change'}
   */
  let uiInputState = $state("nothing");
  /**
   * @type {"unavailable" | "stopped" | "playing" | "paused"}
   */
  let playerState = $state("stopped");
  let numberDisplayHidden = $state(true);

  const currentTime = $derived(durationToMMSS(seekPosition));

  // Windowshade seek bar: the thumb travels the track's 17px minus its own 3px.
  const shadeProgress = $derived(
    loadedTrack?.durationInMs
      ? Math.min(1, Math.max(0, seekPosition / loadedTrack.durationInMs))
      : 0,
  );
  /** @param {PointerEvent} e */
  function shadeSeek(e) {
    const duration = loadedTrack?.durationInMs;
    if (!duration) return;
    const rect = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect();
    const fraction = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    seek(fraction * duration);
  }
  const trackDisplayText = $derived(
    loadedTrack
      ? `${loadedTrack.displayName} (${loadedTrack.displayDuration})`
      : "Winamp 2.91",
  );
  const stoppedOrUnavailable = $derived.by(() =>
    playerState == "stopped" || playerState == "unavailable",
  );
  const timeDisplayHidden = $derived.by(() =>
    stoppedOrUnavailable || (playerState == "paused" && numberDisplayHidden),
  );
  const volumeSpriteRow = $derived(Math.floor((volume / 100) * 27));
  const tickerOverrideText = $derived.by(() => {
    if (uiInputState == "seeking") {
      return loadedTrack
        ? `SEEK TO: ${durationToString(sliderSeekPosition)}/${loadedTrack.displayDuration} (${Math.ceil((sliderSeekPosition / loadedTrack.durationInMs) * 100)}%)`
        : "NO TRACK LOADED";
    } else if (uiInputState == "volume-change") {
      return `VOLUME: ${volume}%`;
    } else if (uiInputState == "balance-change") {
      if (balance == 0) return "BALANCE: CENTER";
      return `BALANCE: ${Math.abs(balance)}% ${balance < 0 ? "LEFT" : "RIGHT"}`;
    }
  });

  function emitPreviousPressed() {
    if (controllerMode) {
      invoke("smtc_previous").catch(() => {});
      return;
    }
    emitWindowEvent("playerWindow", { PreviousPressed: null });
  }

  function emitNextPressed() {
    if (controllerMode) {
      invoke("smtc_next").catch(() => {});
      return;
    }
    emitWindowEvent("playerWindow", { NextPressed: null });
  }

  const controlButtons = [
    {
      label: "Previous",
      index: 0,
      width: undefined,
      click: emitPreviousPressed,
    },
    { label: "Play", index: 1, width: undefined, click: play },
    { label: "Pause", index: 2, width: undefined, click: pause },
    { label: "Stop", index: 3, width: undefined, click: stop },
    { label: "Next", index: 4, width: "22px", click: emitNextPressed },
  ];

  /**
   * @param {SpotifyTrack} track
   */
  async function loadTrack(track) {
    loadedTrack = track;
    if (playerState != "stopped") {
      playerState = "stopped";
      await play();
    }
  }

  async function play() {
    if (controllerMode) {
      playerState = "playing"; // snappy UI; the SMTC poll corrects if refused
      await invoke("smtc_play").catch(() => {});
      return;
    }
    if (playerState == "paused") {
      await invoke("play").catch(handleError);
    } else if (loadedTrack) {
      setPosition(0);
      sliderSeekPosition = 0;
      playerState = loadedTrack.unavailable ? "unavailable" : "playing";

      if (playerState == "unavailable") {
        await invoke("stop").catch(handleError);
      } else {
        await invoke("load_track", { uri: loadedTrack?.uri.asString }).catch(
          handleError,
        );
      }
    }
  }

  async function pause() {
    if (controllerMode) {
      playerState = "paused";
      await invoke("smtc_pause").catch(() => {});
      return;
    }
    if (playerState == "playing") {
      playerState = "paused"; // To make the UI a bit snappier
      await invoke("pause").catch(handleError);
    }
  }

  async function stop() {
    if (controllerMode) {
      // The official Spotify app has no real "stop" — pause is the honest map.
      playerState = "paused";
      await invoke("smtc_pause").catch(() => {});
      return;
    }
    setPosition(0);
    sliderSeekPosition = 0;
    playerState = "stopped"; // To make the UI a bit snappier
    await invoke("stop").catch(handleError);
  }

  // The audio-output device was changed (from the playlist menu), which rebuilds
  // the player on a fresh sink. Reload the current track on the new device at the
  // same spot so the switch is heard without the user pressing play again.
  async function reapplyAfterDeviceChange() {
    if (!loadedTrack || loadedTrack.unavailable) return;
    const wasPlaying = playerState === "playing";
    const position = Math.round(seekPosition);
    await invoke("load_track", { uri: loadedTrack.uri.asString }).catch(() => {});
    if (position > 0) await invoke("seek", { positionMs: position }).catch(() => {});
    if (!wasPlaying) await invoke("pause").catch(() => {});
  }

  /**
   * @param {number} positionMs
   */
  async function seek(positionMs) {
    // the backend command wants an integer u32; the interpolated position is a
    // float, so round before sending (otherwise Tauri rejects the call)
    positionMs = Math.round(positionMs);
    // To make the UI a bit snappier and to not glitch between new and old value
    setPosition(positionMs);
    sliderSeekPosition = positionMs;
    if (controllerMode) {
      await invoke("smtc_seek", { positionMs }).catch(() => {});
    } else {
      await invoke("seek", {
        positionMs,
      }).catch(handleError);
    }
    // jump the Discord progress bar to the new position too
    pushDiscordPresence();
  }

  // In controller mode the spectrum comes from the system-audio loopback
  // (loopback.rs) instead of the librespot sink.
  const visualizer = new Visualizer(
    controllerMode ? "loopback_spectrum" : "take_latest_spectrum",
  );
  $effect(() => {
    if (playerState != "playing") {
      visualizer.stop(stoppedOrUnavailable);
    } else {
      visualizer.start();
    }
  });

  $effect(() => {
    // No player backend exists in controller mode — the sliders stay visual.
    if (!controllerMode) invoke("set_volume", { volume });
  });

  // snap the balance to centre when it's close, like Winamp's detent
  const balanceRow = $derived(Math.round((Math.abs(balance) / 100) * 27));
  $effect(() => {
    if (!controllerMode) invoke("set_balance", { balance: balance / 100 });
  });

  $effect(() => {
    if (uiInputState != "seeking") {
      sliderSeekPosition = seekPosition;
    }
  });

  $effect(() => {
    invoke("set_playlist_window_visible", {
      visible: showPlaylist,
    }).catch(handleError);
  });

  $effect(() => {
    invoke("set_eq_window_visible", {
      visible: showEq,
    }).catch(handleError);
  });

  $effect(() => {
    invoke("set_double_size", { active: doubleSizeActive });
    REACTIVE_WINDOW_SIZE.setZoom(doubleSizeActive ? 2 : 1);
  });

  // Windowshade: the player collapses to the classic 275x14 title bar. The
  // layout's size effect picks the new height up and resizes the OS window.
  $effect(() => {
    invoke("set_windowshade", { active: shadeActive });
    REACTIVE_WINDOW_SIZE.setSize(275, shadeActive ? 14 : 116);
  });

  // Discord Rich Presence. Only shown while actually playing; pausing/stopping/
  // idling clears it, so Discord never keeps counting elapsed time past the
  // song. Re-anchored on track/state change AND on seek (so the progress bar
  // jumps with you) — but NOT every ticker second.
  function pushDiscordPresence() {
    const track = loadedTrack;
    if (playerState !== "playing" || !track?.name) {
      invoke("clear_discord_activity").catch(() => {});
      return;
    }
    invoke("set_discord_activity", {
      name: track.name,
      artist: track.artist,
      album: track.album ?? "",
      albumArt: track.albumArt ?? null,
      playlistIndex: playlistPos.index,
      playlistLength: playlistPos.length,
      elapsedMs: Math.round(seekPosition),
      durationMs: Math.round(track.durationInMs ?? 0),
      playing: true,
    }).catch(() => {});
  }
  $effect(() => {
    loadedTrack;
    playerState;
    untrack(pushDiscordPresence);
  });

  // The playlist window owns the actual next/previous navigation, so push the
  // shuffle/repeat toggles over to it whenever they change.
  $effect(() => {
    emitWindowEvent("playerWindow", { ShuffleChanged: shuffle });
  });
  $effect(() => {
    emitWindowEvent("playerWindow", { RepeatChanged: repeat });
  });

  // Controller mode: mirror the official Spotify app once a second. The data
  // is poured into the same state the normal player uses (loadedTrack,
  // playerState, seek position), so the ticker, time display, seek bar and
  // windowshade all just work without their own controller branches.
  onMount(() => {
    if (!controllerMode) return;
    const poll = async () => {
      const np = await invoke("smtc_now_playing").catch(() => null);
      if (!np?.available || !np.title) {
        playerState = "stopped";
        loadedTrack = undefined;
        return;
      }
      const trackChanged =
        np.title !== loadedTrack?.name || np.artist !== loadedTrack?.artist;
      if (trackChanged) {
        loadedTrack = /** @type {any} */ ({
          name: np.title,
          artist: np.artist,
          album: np.album,
          albumArt: null,
          durationInMs: np.duration_ms,
          displayName: `${np.artist} - ${np.title}`,
          displayDuration: durationToString(np.duration_ms),
          unavailable: false,
        });
      }
      const stateChanged = (playerState === "playing") !== np.playing;
      playerState = np.playing ? "playing" : "paused";
      // The local clock ticks smoothly between polls; only re-anchor it when
      // something real happened (new track, play/pause, or an external seek —
      // seen as a drift the local clock can't have produced). Re-anchoring on
      // every poll made the counter stutter and leap.
      const drift = Math.abs(np.position_ms - seekPosition);
      if (
        uiInputState != "seeking" &&
        (trackChanged || stateChanged || !np.playing || drift > 2000)
      ) {
        setPosition(np.position_ms);
      }
    };
    poll();
    const smtcInterval = setInterval(poll, 1000);
    return () => clearInterval(smtcInterval);
  });

  onMount(() => {
    // Tick seek position and blink number display
    const tickerInterval = setInterval(() => {
      if (playerState == "paused") {
        numberDisplayHidden = !numberDisplayHidden;
      } else if (playerState != "unavailable") {
        seekPosition =
          positionAnchorMs + (performance.now() - positionAnchorAt);
      }
      // feed the lyrics window the current track + position each tick; it
      // interpolates locally between ticks for smooth line highlighting
      emitWindowEvent("lyrics", {
        uri: currentTrackUri,
        positionMs: Math.round(seekPosition),
        playing: playerState == "playing",
      });
    }, 1000);

    const playlistWindowEventSubscription = subscribeToWindowEvent(
      "playlistWindow",
      (event) => {
        if (event.TrackLoaded) {
          let track = event.TrackLoaded;
          loadTrack(track);
        } else if (event.PlayRequested !== undefined) {
          play();
        } else if (event.PauseRequested !== undefined) {
          pause();
        } else if (event.StopRequested !== undefined) {
          stop();
        } else if (event.EndReached !== undefined) {
          stop();
        }
      },
    );

    const eqWindowEventSubscription = subscribeToWindowEvent(
      "eqWindow",
      (event) => {
        if (event.CloseRequested !== undefined) showEq = false;
      },
    );

    const trackPositionSubscription = subscribeToWindowEvent(
      "trackPosition",
      (event) => {
        playlistPos = { index: event.index ?? 0, length: event.length ?? 0 };
      },
    );

    const playerEventsSubscription = subscribeToWindowEvent(
      "player",
      (event) => {
        // every playback event carries the track uri — keep the latest for
        // the lyrics window
        const payload =
          event.Playing ||
          event.Paused ||
          event.PositionChanged ||
          event.PositionCorrection ||
          event.Seeked ||
          event.Stopped;
        if (payload?.uri) currentTrackUri = payload.uri;

        if (event.Playing) {
          const { position_ms } = event.Playing;
          playerState = "playing";
          setPosition(position_ms);
        } else if (event.Paused) {
          const { position_ms } = event.Paused;
          playerState = "paused";
          setPosition(position_ms);
        } else if (event.Stopped) {
          playerState = "stopped";
        } else if (event.PositionCorrection) {
          const { position_ms } = event.PositionCorrection;
          setPosition(position_ms);
        } else if (event.PositionChanged) {
          const { position_ms } = event.PositionChanged;
          setPosition(position_ms);
        } else if (event.Seeked) {
          const { position_ms } = event.Seeked;
          setPosition(position_ms);
        }
      },
    );

    const cleanupDropHandler = handleDrop((urls) => {
      emitWindowEvent("playerWindow", { UrlsDropped: urls });
    });

    // Classic Winamp keyboard shortcuts (main window)
    const onPlayerKeyDown = (e) => {
      const t = e.target;
      if (
        t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable)
      )
        return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const k = e.key.toLowerCase();
      const acted = () => e.preventDefault();
      switch (k) {
        case "z": acted(); emitPreviousPressed(); break;
        case "x": acted(); play(); break;
        case "c": acted(); pause(); break;
        case "v": acted(); stop(); break;
        case "b": acted(); emitNextPressed(); break;
        case " ": acted(); playerState === "playing" ? pause() : play(); break;
        case "s": acted(); shuffle = !shuffle; break;
        case "r": acted(); repeat = (repeat + 1) % 3; break;
        case "arrowup": acted(); volume = Math.min(100, volume + 5); break;
        case "arrowdown": acted(); volume = Math.max(0, volume - 5); break;
        case "arrowright":
          acted();
          if (loadedTrack)
            seek(Math.min(loadedTrack.durationInMs, seekPosition + 5000));
          break;
        case "arrowleft":
          acted();
          if (loadedTrack) seek(Math.max(0, seekPosition - 5000));
          break;
        case "l":
          acted();
          // The library browses via the librespot session, which controller
          // mode doesn't have.
          if (!controllerMode) {
            invoke("set_library_window_visible", { visible: true });
          }
          break;
      }
    };
    document.addEventListener("keydown", onPlayerKeyDown);

    const audioDeviceSubscription = listen("audioDeviceChanged", () =>
      reapplyAfterDeviceChange(),
    );

    return () => {
      clearInterval(tickerInterval);
      audioDeviceSubscription.then((unlisten) => unlisten());
      playerEventsSubscription.then((unlisten) => unlisten());
      playlistWindowEventSubscription.then((unlisten) => unlisten());
      eqWindowEventSubscription.then((unlisten) => unlisten());
      trackPositionSubscription.then((unlisten) => unlisten());
      cleanupDropHandler();
      document.removeEventListener("keydown", onPlayerKeyDown);
    };
  });

  /**
   * @param {HTMLElement} element
   */
  function makeWindowDraggable(element) {
    makeTauriWindowDraggable(element, {
      async onStart({ startPosition, windowSize }) {
        // Tell the native dock manager to freeze the player's connected group so
        // the whole stack moves in lockstep for this drag.
        await emitWindowEvent("playerWindow", { DragStarted: null });
        const playlistWindow = await Window.getByLabel("playlist");
        const [playlistPosition, playlistSize, monitor] = await Promise.all([
          playlistWindow?.outerPosition(),
          playlistWindow?.outerSize(),
          currentMonitor(),
        ]);
        const playlistRect =
          playlistPosition && playlistSize
            ? rectFromPositionAndSize(playlistPosition, playlistSize)
            : undefined;
        const startRect = rectFromPositionAndSize(startPosition, windowSize);
        const dockedAtStart = playlistRect
          ? isDocked(startRect, playlistRect)
          : false;

        return {
          docked: dockedAtStart,
          dockedAtStart,
          groupStartRect:
            dockedAtStart && playlistRect
              ? boundingRect([startRect, playlistRect])
              : startRect,
          playlistRect,
          screenBounds: monitor
            ? rectFromPositionAndSize(
                monitor.workArea.position,
                monitor.workArea.size,
              )
            : undefined,
          screenSnapped: false,
        };
      },
      mapPosition(rawPosition, context, { startPosition, windowSize }) {
        let position = rawPosition;
        if (context.playlistRect && !context.dockedAtStart) {
          const rawRect = {
            ...rawPosition,
            width: windowSize.width,
            height: windowSize.height,
          };
          const snapDistance = context.docked
            ? STICKY_SNAP_DISTANCE
            : SNAP_DISTANCE;
          const snappedPosition = snapPosition(
            rawRect,
            context.playlistRect,
            snapDistance,
          );
          position = snappedPosition ?? rawPosition;
          context.docked = snappedPosition !== undefined;
        }

        if (context.screenBounds) {
          const movingGroupRect = {
            x: context.groupStartRect.x + position.x - startPosition.x,
            y: context.groupStartRect.y + position.y - startPosition.y,
            width: context.groupStartRect.width,
            height: context.groupStartRect.height,
          };
          const snappedGroupPosition = snapRectIntoBounds(
            movingGroupRect,
            context.screenBounds,
            context.screenSnapped ? STICKY_SNAP_DISTANCE : SNAP_DISTANCE,
          );

          if (snappedGroupPosition) {
            position = {
              x: position.x + snappedGroupPosition.x - movingGroupRect.x,
              y: position.y + snappedGroupPosition.y - movingGroupRect.y,
            };
          }
          context.screenSnapped = snappedGroupPosition !== undefined;
        }

        return position;
      },
      async onEnd() {
        await emitWindowEvent("playerWindow", { DragEnded: null });
      },
    });
  }
</script>

<main class:shade={shadeActive}>
  <div class="sprite main-sprite"></div>

  <!-- 🦙 easter egg: click the Winamp titlebar logo -->
  <button
    class="llama-trigger"
    data-no-drag
    onclick={openAbout}
    aria-label="About"
  ></button>
  {#if showLlama}
    <div class="llama-about" data-no-drag role="dialog" aria-label="About Spotiamp+">
      <!-- click anywhere that isn't a link to close -->
      <button class="llama-close-layer" aria-label="Close" onclick={() => (showLlama = false)}
      ></button>
      <div class="llama-content">
        <div class="llama-art">🦙</div>
        <div class="llama-phrase">IT REALLY WHIPS THE LLAMA'S ASS!</div>
        <div class="llama-version">
          Spotiamp+{appVersion ? ` v${appVersion}` : ""} · Spotify's music, Winamp's soul
        </div>
        <div class="llama-credits">
          Built on Spotiamp by Ted Steen · extended by fdeox<br />
          Tauri · Rust · Svelte · librespot
        </div>
        <div class="llama-links">
          <button onclick={() => openLink("github")}>GitHub</button>
          <button onclick={() => openLink("discord")}>Discord</button>
          <button onclick={() => openLink("license")}>License</button>
        </div>
        <div class="llama-hint">(click to close)</div>
      </div>
    </div>
  {/if}

  <div class="sprite stereo-mono-sprite stereo-mono-sprite-mono"></div>
  <div
    class="sprite stereo-mono-sprite stereo-mono-sprite-stereo"
    class:stereo-mono-sprite-enabled={playerState != "stopped" &&
      playerState != "unavailable"}
  ></div>

  <!-- kbps / kHz readouts (Spotify streams ~320kbps ogg @ 44.1kHz) -->
  {#if playerState != "stopped" && playerState != "unavailable"}
    <div class="lcd-info lcd-bitrate">320</div>
    <div class="lcd-info lcd-samplerate">44</div>
  {/if}

  <button
    class="sprite eq-btn"
    class:eq-btn-enabled={showEq}
    onclick={() => {
      // The EQ needs our own audio pipeline, which controller mode doesn't have.
      if (!controllerMode) showEq = !showEq;
    }}
    aria-label="Toggle equalizer"
  ></button>
  <button
    class="sprite playlist-btn"
    class:playlist-btn-enabled={showPlaylist}
    onclick={() => (showPlaylist = !showPlaylist)}
    aria-label="Toggle playlist"
  ></button>
  <button
    class="sprite shuffle-btn"
    class:active={shuffle}
    onclick={() => (shuffle = !shuffle)}
    aria-label="Shuffle"
  ></button>
  <button
    class="sprite repeat-btn"
    class:active={repeat > 0}
    class:repeat-one={repeat === 2}
    onclick={() => (repeat = (repeat + 1) % 3)}
    aria-label="Repeat"
  ></button>
  <div class="sprite playpause-sprite playpause-{playerState}"></div>

  <div
    use:makeWindowDraggable
    class="sprite titlebar-sprite"
    id="titlebar"
  ></div>

  <button
    class="sprite close-btn"
    onclick={() => emitWindowEvent("playerWindow", { CloseRequested: null })}
    aria-label="Close"
  ></button>
  <button
    class="sprite minimize-btn"
    onclick={() => getCurrentWindow().minimize()}
    aria-label="Minimize"
  ></button>
  <button
    class="sprite shade-btn"
    onclick={() => (shadeActive = true)}
    aria-label="Windowshade mode"
  ></button>

  <div class="sprite side-buttons"></div>
  <button
    class="sprite double-size-btn"
    onclick={() => (doubleSizeActive = !doubleSizeActive)}
    class:active={doubleSizeActive}
    aria-label="Toggle double size"
  ></button>

  <TextTicker
    unavailable={playerState == "unavailable"}
    text={trackDisplayText}
    textOverride={tickerOverrideText}
    x={111}
    y={27}
  />
  <div class:hidden={timeDisplayHidden}>
    <NumberDisplay
      number={currentTime.m.toString().padStart(2, "0")}
      x={48}
      y={26}
    />
    <NumberDisplay
      number={currentTime.s.toString().padStart(2, "0")}
      x={78}
      y={26}
    />
  </div>
  {#each visualizer.bars as bar}
    <div
      class="visualizer-bar"
      style:--bar-idx={bar.index}
      style:--height={bar.value}
    ></div>
    <div
      class="visualizer-bar-hat"
      style:--bar-idx={bar.index}
      style:--height={bar.hat}
      class:hidden={bar.hat < 0.01}
    ></div>
  {/each}
  <!-- double-click the spectrum to pop the milkdrop-style visualizer window -->
  <button
    class="viz-open-btn"
    ondblclick={() => invoke("set_visualizer_window_visible", { visible: true })}
    aria-label="Open visualizer"
    title="double-click for the visualizer"
  ></button>
  <input
    type="range"
    class="sprite volume-sprite"
    style:--volume={volume}
    style:--volume-row={volumeSpriteRow}
    id="volume"
    min="0"
    max="100"
    bind:value={volume}
    onmousedown={() => (uiInputState = "volume-change")}
    onmouseup={() => (uiInputState = "nothing")}
  />
  <input
    type="range"
    class="sprite balance-sprite"
    style:--balance-row={balanceRow}
    id="balance"
    min="-100"
    max="100"
    bind:value={balance}
    onmousedown={() => (uiInputState = "balance-change")}
    onmouseup={() => (uiInputState = "nothing")}
    ondblclick={() => (balance = 0)}
  />
  <input
    type="range"
    class="sprite seek-position-sprite"
    class:hidden={stoppedOrUnavailable}
    id="seek-position"
    min="0"
    max={loadedTrack?.durationInMs}
    step="1000"
    bind:value={sliderSeekPosition}
    onmousedown={() => (uiInputState = "seeking")}
    onmouseup={() => {
      seek(sliderSeekPosition);
      uiInputState = "nothing";
    }}
  />

  {#each controlButtons as button}
    <button
      class="sprite control-buttons-sprite"
      style:--button-x={`calc(16px + (var(--button-width) * ${button.index}))`}
      style:--button-y="88px"
      style:--button-idx={button.index}
      style:width={button.width}
      onclick={button.click}
      aria-label={button.label}
    ></button>
  {/each}

  <!-- <div
    class="sprite control-buttons-sprite"
    style:--button-width="23px"
    style:--button-x="calc(22px + (var(--button-width) * 5))"
    style:--button-y="89px"
    style:--button-idx="5"
    style:width="21px"
    style:height="16px"
    id="main"
  ></div> -->

  <!--
    Classic Winamp windowshade: the player collapses to a single 275x14 bar.
    Kept self-contained so the normal layout above stays untouched — `main.shade`
    hides every other child (see the CSS below).
  -->
  {#if shadeActive}
    <div class="shade-overlay">
      <div class="sprite shade-bar" use:makeWindowDraggable></div>
      <!--
        Classic shade layout (verified against the skin sprite's own recesses
        and Webamp's reference CSS): the title sits in the recess at x=79, and
        the time is the small TEXT.BMP "mini time" at x=127 — not the big
        NUMBERS.BMP digits the expanded window uses.
      -->
      <TextTicker
        unavailable={playerState == "unavailable"}
        text={trackDisplayText}
        textOverride={tickerOverrideText}
        x={79}
        y={4}
        chars={9}
      />
      <div class:hidden={timeDisplayHidden}>
        <TextTicker
          unavailable={false}
          text=""
          textOverride={`${currentTime.m}:${currentTime.s.toString().padStart(2, "0")}`}
          x={127}
          y={4}
          chars={6}
        />
      </div>
      <!--
        The shade bar sprite has the transport icons drawn into it, spaced ~10px
        apart from x=166 (measured off the skin's own bitmap). These are just the
        hit areas over them, reusing the expanded window's handlers.

        They fire on pointerdown rather than click: the drag handle calls
        preventDefault() on pointerdown, which suppresses the synthesized click
        that would normally follow, so an onclick here would never run.
      -->
      {#each controlButtons as button, i}
        <button
          class="shade-ctrl"
          data-no-drag
          style:--cx={166 + i * 10}
          onpointerdown={(e) => {
            e.stopPropagation();
            button.click();
          }}
          aria-label={button.label}
        ></button>
      {/each}
      <!-- seek bar: track sprite (0,36) 17x7 with the 3x7 thumb (20,36) -->
      {#if !stoppedOrUnavailable}
        <button
          class="sprite shade-position"
          data-no-drag
          onpointerdown={shadeSeek}
          aria-label="Seek"
        ></button>
        <div
          class="sprite shade-thumb"
          style:--sprite-x="{226 + shadeProgress * 14}px"
        ></div>
      {/if}
      <button
        class="sprite unshade-btn"
        onclick={() => (shadeActive = false)}
        aria-label="Leave windowshade mode"
      ></button>
      <button
        class="sprite minimize-btn"
        onclick={() => getCurrentWindow().minimize()}
        aria-label="Minimize"
      ></button>
      <button
        class="sprite close-btn"
        onclick={() => emitWindowEvent("playerWindow", { CloseRequested: null })}
        aria-label="Close"
      ></button>
    </div>
  {/if}
</main>

<style>
  button.close-btn {
    cursor: url(/src/static/assets/skins/base-2.91/CLOSE.CUR), default;
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 264px;
    --sprite-y: 3px;
    width: 9px;
    height: 9px;
    background-position: -18px 0px;
  }

  button.close-btn:active {
    background-position-y: -9px;
  }

  button.minimize-btn {
    cursor: url(/src/static/assets/skins/base-2.91/MAINMENU.CUR), auto;
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 244px;
    --sprite-y: 3px;
    width: 9px;
    height: 9px;
    background-position: -9px 0px;
  }

  button.minimize-btn:active {
    background-position-y: -9px;
  }

  /* ---- windowshade mode ------------------------------------------------
     TITLEBAR.BMP (344x87) classic layout:
       shade bar   (27,29) 275x14   ·  shade button   (0,18) 9x9
       pressed     (9,18)           ·  unshade button (0,27) 9x9
     The overlay is the only visible child while shaded.                  */
  main.shade > :not(.shade-overlay) {
    display: none;
  }
  .shade-overlay {
    position: absolute;
    inset: 0;
  }
  .shade-bar {
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 0px;
    --sprite-y: 0px;
    width: 275px;
    height: 14px;
    background-position: -27px -29px;
  }
  button.shade-btn,
  button.unshade-btn {
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 254px;
    --sprite-y: 3px;
    width: 9px;
    height: 9px;
  }
  button.shade-btn {
    background-position: 0px -18px;
  }
  button.shade-btn:active {
    background-position: -9px -18px;
  }
  button.unshade-btn {
    background-position: 0px -27px;
  }
  button.unshade-btn:active {
    background-position: -9px -27px;
  }
  /* windowshade seek bar — sprites verified against Webamp's skin sprite map */
  button.shade-position {
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 226px;
    --sprite-y: 4px;
    width: 17px;
    height: 7px;
    background-position: 0px -36px;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 5;
  }
  .shade-thumb {
    --sprite-url: var(--skin-titlebar);
    --sprite-y: 4px;
    width: 3px;
    height: 7px;
    background-position: -20px -36px;
    pointer-events: none;
    z-index: 6;
  }
  /* transparent hit areas over the transport icons drawn into the shade bar */
  button.shade-ctrl {
    position: absolute;
    z-index: 5;
    left: calc(var(--cx) * 1px * var(--zoom));
    top: 0;
    width: calc(10px * var(--zoom));
    height: calc(14px * var(--zoom));
    border: none;
    background: transparent;
    padding: 0;
    cursor: pointer;
  }

  .side-buttons {
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 10px;
    --sprite-y: 22px;
    width: 8px;
    height: 43px;
    background-position: -304px 0px;
  }

  button.double-size-btn {
    --sprite-url: var(--skin-titlebar);
    --sprite-x: 10px;
    --sprite-y: 48px;
    width: 8px;
    height: 8px;
    background-position: -328px -70px;
    opacity: 0;
  }
  button.double-size-btn.active {
    opacity: 1;
  }

  button.playlist-btn {
    --sprite-url: var(--skin-shufrep);
    --sprite-x: 242px;
    --sprite-y: 58px;
    width: 23px;
    height: 12px;
    background-position: -23px -61px;
  }
  button.playlist-btn:active {
    background-position-x: -69px;
  }

  button.playlist-btn-enabled {
    background-position-y: -73px;
  }

  /* EQ button — sits directly left of the PL button (SHUFREP.BMP col 0) */
  button.eq-btn {
    --sprite-url: var(--skin-shufrep);
    --sprite-x: 219px;
    --sprite-y: 58px;
    width: 23px;
    height: 12px;
    background-position: 0px -61px;
  }
  button.eq-btn:active {
    background-position-x: -46px;
  }
  button.eq-btn-enabled {
    background-position-y: -73px;
  }

  /* 🦙 easter egg — invisible hit area over the titlebar Winamp logo */
  .llama-trigger {
    position: absolute;
    left: calc(6px * var(--zoom));
    top: calc(3px * var(--zoom));
    width: calc(9px * var(--zoom));
    height: calc(9px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 50;
  }
  .llama-about {
    position: absolute;
    inset: 0;
    z-index: 60;
    background: rgba(2, 8, 2, 0.92);
    color: #00ff41;
    font-family: "px sans nouveaux", monospace;
    -webkit-font-smoothing: none;
    transform-origin: top left;
    transform: scale(var(--zoom));
    width: 275px;
    height: 116px;
  }
  /* full-bleed click target behind the text, so clicking the box closes it */
  .llama-close-layer {
    position: absolute;
    inset: 0;
    z-index: 1;
    border: none;
    background: transparent;
    cursor: pointer;
  }
  .llama-content {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: calc(2px * var(--zoom));
    /* let clicks fall through to the close layer — except on the links */
    pointer-events: none;
  }
  .llama-art {
    font-size: 24px;
    line-height: 1;
    animation: llama-bob 0.9s ease-in-out infinite alternate;
  }
  .llama-version {
    font-size: 7px;
    color: #6fdc8c;
    text-align: center;
  }
  .llama-links {
    display: flex;
    gap: 8px;
    pointer-events: auto;
  }
  .llama-links button {
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    font-size: 7px;
    color: #00ff41;
    text-decoration: underline;
    cursor: pointer;
  }
  .llama-links button:hover {
    color: #baffd0;
  }
  @keyframes llama-bob {
    from {
      transform: translateY(0) rotate(-4deg);
    }
    to {
      transform: translateY(-3px) rotate(4deg);
    }
  }
  .llama-phrase {
    font-size: 9px;
    letter-spacing: 1px;
    text-shadow: 0 0 5px rgba(0, 255, 65, 0.6);
  }
  .llama-credits {
    font-size: 7px;
    color: #3f9a55;
    text-align: center;
    line-height: 1.6;
  }
  .llama-hint {
    font-size: 6px;
    color: #2a6a3a;
  }

  /* SHUFREP.BMP: shuffle (47x15) + repeat (28x15), 4 states each
     (off / off-pressed / on / on-pressed stacked every 15px) */
  button.shuffle-btn {
    --sprite-url: var(--skin-shufrep);
    --sprite-x: 164px;
    --sprite-y: 89px;
    width: 47px;
    height: 15px;
    background-position: -28px 0px;
  }
  button.shuffle-btn:active {
    background-position: -28px -15px;
  }
  button.shuffle-btn.active {
    background-position: -28px -30px;
  }
  button.shuffle-btn.active:active {
    background-position: -28px -45px;
  }

  button.repeat-btn {
    --sprite-url: var(--skin-shufrep);
    --sprite-x: 211px;
    --sprite-y: 89px;
    width: 28px;
    height: 15px;
    background-position: 0px 0px;
  }
  button.repeat-btn:active {
    background-position: 0px -15px;
  }
  button.repeat-btn.active {
    background-position: 0px -30px;
  }
  button.repeat-btn.active:active {
    background-position: 0px -45px;
  }
  /* the base skin has no "repeat one" art, so mark that mode with a small 1 */
  button.repeat-btn.repeat-one::after {
    content: "1";
    position: absolute;
    right: 2px;
    top: 2px;
    font-family: monospace;
    font-size: 7px;
    font-weight: bold;
    line-height: 1;
    color: #00e000;
    text-shadow: 0 0 1px #000;
  }

  .stereo-mono-sprite {
    --sprite-url: var(--skin-monoster);
    --sprite-y: 41px;
    height: 12px;
  }

  .stereo-mono-sprite-mono {
    --sprite-x: 212px;
    width: 27px;
    background-position: -29px -12px;
  }

  .stereo-mono-sprite-stereo {
    --sprite-x: 239px;
    width: 29px;
    background-position: 0px -12px;
  }

  .stereo-mono-sprite-enabled {
    background-position-y: 0px;
  }

  /* kbps / kHz LCD readouts (positioned to the left of the MAIN.BMP labels) */
  .lcd-info {
    position: absolute;
    top: calc(43px * var(--zoom));
    color: #14e614;
    font-family: monospace;
    font-size: calc(6px * var(--zoom));
    line-height: calc(6px * var(--zoom));
    text-align: right;
    text-shadow: 0 0 calc(2px * var(--zoom)) rgba(20, 230, 20, 0.6);
    pointer-events: none;
  }
  .lcd-bitrate {
    left: calc(105px * var(--zoom));
    width: calc(15px * var(--zoom));
  }
  .lcd-samplerate {
    left: calc(147px * var(--zoom));
    width: calc(11px * var(--zoom));
  }

  /* ------ SEEK POSITION ------ */
  .seek-position-sprite {
    --sprite-url: var(--skin-posbar);
    --sprite-x: 16px;
    --sprite-y: 72px;
    width: 249px;
    height: 10px;
    background-position: 0px 0px;
  }

  #seek-position {
    appearance: none;
    cursor: url(/src/static/assets/skins/base-2.91/VOLBAL.CUR), default;
  }

  #seek-position::-webkit-slider-thumb {
    background: var(--skin-posbar);
    appearance: none;
    width: 28px;
    height: 11px;
    margin-bottom: 1px;
    background-position: -249px 11px;
  }

  #seek-position::-webkit-slider-thumb:active {
    background-position: -278px 11px;
  }

  /* ------ /SEEK POSITION ------ */

  /* ------ VISUALIZER ------ */
  .visualizer-bar {
    position: absolute;
    left: calc((24px + var(--bar-idx) * 4px) * var(--zoom));
    width: calc(var(--zoom) * 3px);

    --max-height: 16px;
    top: calc((59px - var(--max-height) * var(--height)) * var(--zoom));
    height: calc(var(--max-height) * var(--height) * var(--zoom));

    background: linear-gradient(
      rgb(213, 76, 0) 0% 6.67%,
      rgb(213, 89, 0) 6.67% 13.34%,
      rgb(215, 102, 0) 13.34% 20.009999999999998%,
      rgb(214, 115, 1) 20.009999999999998% 26.68%,
      rgb(197, 124, 4) 26.68% 33.35%,
      rgb(222, 165, 21) 33.35% 40.019999999999996%,
      rgb(213, 181, 34) 40.019999999999996% 46.69%,
      rgb(189, 222, 42) 46.69% 53.36%,
      rgb(148, 221, 34) 53.36% 60.03%,
      rgb(41, 206, 16) 60.03% 66.7%,
      rgb(50, 190, 16) 66.7% 73.37%,
      rgb(56, 181, 17) 73.37% 80.03999999999999%,
      rgb(49, 156, 6) 80.03999999999999% 86.71%,
      rgb(40, 148, 1) 86.71% 93.38%,
      rgb(27, 132, 6) 93.38% 100.05%
    );
    background-position: bottom;
    background-repeat: no-repeat;
    background-size: 100% calc(var(--max-height) * var(--zoom));
  }
  .visualizer-bar-hat {
    position: absolute;
    --max-height: 16px;
    background: rgb(150, 150, 150);
    top: calc((58px + (1px - var(--height) * var(--max-height))) * var(--zoom));
    width: calc(var(--zoom) * 3px);
    height: calc(var(--zoom) * 1px);
    left: calc((24px + var(--bar-idx) * 4px) * var(--zoom));
  }
  .viz-open-btn {
    position: absolute;
    left: calc(24px * var(--zoom));
    top: calc(43px * var(--zoom));
    width: calc(76px * var(--zoom));
    height: calc(16px * var(--zoom));
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 30;
  }
  /* ------ /VISUALIZER ------ */

  /* ------ TITLEBAR ------ */
  .titlebar-sprite {
    --sprite-url: var(--skin-titlebar);
    width: 275px;
    height: 14px;
    background-position: -27px 0px;
    cursor: url(/src/static/assets/skins/base-2.91/TITLEBAR.CUR), default;
  }

  /* ------ /TITLEBAR ------ */

  /* ------ MAIN ------ */
  .main-sprite {
    --sprite-url: var(--skin-main);
    width: 275px;
    height: 116px;
    background-position: 0px 0px;
  }

  /* ------ /MAIN ------ */

  /* ------ PLAYPAUSE ------ */
  .playpause-sprite {
    --sprite-url: var(--skin-playpaus);
    width: 9px;
    height: 9px;
    --sprite-x: 26px;
    --sprite-y: 28px;
  }
  .playpause-playing,
  .playpause-unavailable {
    background-position: -0px 0px;
  }
  .playpause-paused {
    background-position: -9px 0px;
  }

  .playpause-stopped,
  .playpause-loaded {
    background-position: -18px 0px;
  }

  /* ------ /PLAYPAUSE ------ */

  /* ------ VOLUME ------ */
  .volume-sprite {
    --sprite-url: var(--skin-volume);
    --sprite-x: 107px;
    --sprite-y: 57px;
    width: 65px;
    height: 14px;
    background-position: 0px 0px;
  }

  #volume {
    appearance: none;
    cursor: url(/src/static/assets/skins/base-2.91/VOLBAL.CUR), default;
    background-position-y: calc(var(--volume-row) * -15px);
  }

  #volume::-webkit-slider-thumb {
    background: var(--skin-volume);
    appearance: none;
    width: 14px;
    height: 11px;
    margin-bottom: 1px;
    background-position: -15px 11px;
  }

  #volume::-webkit-slider-thumb:active {
    background-position: 0px 11px;
  }

  /* ------ /VOLUME ------ */

  /* ------ BALANCE ------ */
  .balance-sprite {
    --sprite-url: var(--skin-balance);
    --sprite-x: 177px;
    --sprite-y: 57px;
    width: 38px;
    height: 14px;
    /* the balance trough bar sits at x=12..45 in the 68px-wide BMP */
    background-position: -10px 0px;
  }

  #balance {
    appearance: none;
    cursor: url(/src/static/assets/skins/base-2.91/VOLBAL.CUR), default;
    background-position-y: calc(var(--balance-row) * -15px);
  }

  #balance::-webkit-slider-thumb {
    background: var(--skin-balance);
    appearance: none;
    width: 14px;
    height: 11px;
    margin-bottom: 1px;
    background-position: -15px 11px;
  }

  #balance::-webkit-slider-thumb:active {
    background-position: 0px 11px;
  }

  /* ------ /BALANCE ------ */

  /* ------ CBUTTONS ------ */
  .control-buttons-sprite {
    --sprite-url: var(--skin-cbuttons);
    --button-width: 23px;
    --button-height: 18px;
    --button-state: 0;
    width: var(--button-width);
    height: var(--button-height);
    background-position: 0px 0px;
    left: calc(var(--button-x) * var(--zoom));
    top: calc(var(--button-y) * var(--zoom));
  }

  button.control-buttons-sprite {
    border: 0px;
    background-position: calc(var(--button-idx) * var(--button-width) * -1) 0px;
  }

  button.control-buttons-sprite:active {
    background-position: calc(var(--button-idx) * var(--button-width) * -1)
      calc(var(--button-height));
  }

  /* ------ /CBUTTONS ------ */
</style>
