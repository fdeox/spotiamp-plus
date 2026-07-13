<script>
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { invoke } from "@tauri-apps/api/core";
  import { subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { onMount } from "svelte";
  import "./global.css";
  /**
   * @type {{children: import("svelte").Snippet}}
   */
  let { children } = $props();
  $effect(() => {
    const win = getCurrentWindow();
    // the login window is sized by Rust (600x800) and then redirects to
    // Spotify — don't shrink it to the player's dimensions
    if (win.label === "login") return;
    win.setSize(
      new LogicalSize(
        REACTIVE_WINDOW_SIZE.width * REACTIVE_WINDOW_SIZE.zoom,
        REACTIVE_WINDOW_SIZE.height * REACTIVE_WINDOW_SIZE.zoom
      )
    );
  });

  // Skins: every window reads the active skin and reapplies it live when it
  // changes (broadcast from the player's right-click menu).
  function applySkin(skin) {
    if (skin && skin !== "classic") document.body.dataset.skin = skin;
    else delete document.body.dataset.skin;
  }
  onMount(() => {
    invoke("get_skin").then(applySkin).catch(() => {});
    let unsub;
    subscribeToWindowEvent("skinChanged", (e) => applySkin(e.skin)).then(
      (u) => (unsub = u),
    );
    return () => unsub?.();
  });
</script>

{@render children()}
