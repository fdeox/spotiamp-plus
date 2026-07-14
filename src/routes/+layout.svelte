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
  const CUSTOM_SKIN_VARS = [
    "main", "cbuttons", "monoster", "numbers", "playpaus", "pledit",
    "posbar", "shufrep", "text", "titlebar", "volume", "balance", "eqmain",
    "gentl", "genfill", "gentr",
    "plnormal", "plcurrent", "plbg", "plselbg",
    "genexitembg", "genexitemfg", "genexwndbg", "genexbtntext",
    "genexwndtext", "genexdivider", "genexselbg", "genexhdrbg",
    "genexhdrtext", "genexbtn", "genexbtnp",
  ];
  function applySkin(skin) {
    if (skin === "custom") {
      // a .wsz loaded from disk: override the sprite vars with data-URLs
      // (images) and raw values (GENEX/PLEDIT colours). data-skin="custom"
      // lets pages opt into bitmap-face styling (genex buttons).
      document.body.dataset.skin = "custom";
      invoke("get_custom_skin")
        .then((sprites) => {
          for (const [name, value] of Object.entries(sprites)) {
            document.body.style.setProperty(
              `--skin-${name}`,
              value.startsWith("data:") ? `url("${value}")` : value,
            );
          }
        })
        .catch(() => {});
      return;
    }
    // built-in skin: drop any custom overrides
    for (const name of CUSTOM_SKIN_VARS) {
      document.body.style.removeProperty(`--skin-${name}`);
    }
    if (skin && skin !== "classic") document.body.dataset.skin = skin;
    else delete document.body.dataset.skin;
  }
  onMount(() => {
    // the login window redirects to Spotify — it has no skinnable UI
    if (getCurrentWindow().label === "login") return;

    // Winamp has no browser menu: suppress the WebView2 default context menu
    // (Refresh / Save as / Print…) app-wide. Our own right-click menus render
    // themselves and are unaffected.
    const suppressContextMenu = (e) => e.preventDefault();
    document.addEventListener("contextmenu", suppressContextMenu);

    invoke("get_skin").then(applySkin).catch(() => {});
    let unsub;
    subscribeToWindowEvent("skinChanged", (e) => applySkin(e.skin)).then(
      (u) => (unsub = u),
    );
    return () => {
      document.removeEventListener("contextmenu", suppressContextMenu);
      unsub?.();
    };
  });
</script>

{@render children()}
