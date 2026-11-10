<script>
  import { REACTIVE_WINDOW_SIZE, SKIN_APPLIED } from "$lib/common.svelte";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { invoke } from "@tauri-apps/api/core";
  import { emitWindowEvent, subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { DOCKABLE_LABELS, makeDockedDraggable } from "$lib/window-docking.svelte.js";
  import { lookColorsCss, paintFrame, setFrameActive, useLookFonts, wireFrame } from "$lib/modern-look.js";
  import { onMount } from "svelte";
  import "./global.css";
  /**
   * @type {{children: import("svelte").Snippet}}
   */
  let { children } = $props();
  // UI scale: every window renders at the same scale (CSS zoom on <body>, set
  // here before first paint from the value the window was created with) and
  // follows live changes from the Scale menu / Ctrl+D. The login page (Spotify's
  // own OAuth) isn't ours to scale.
  // The login page, the llama (sized by mascot.rs, drawn to fit) and the
  // modern skin's window (over all the screens, scaling each of the skin's
  // windows itself) keep their own size and no zoom.
  const OWN_SIZE = ["login", "mascot", "modern"].includes(getCurrentWindow().label);
  if (!OWN_SIZE) {
    REACTIVE_WINDOW_SIZE.setZoom(REACTIVE_WINDOW_SIZE.zoom);
  }
  // The on-demand resizable windows whose size we remember across restarts
  // (player has its own field; playlist has set_playlist_inner_size; eq is
  // fixed-size).
  const REMEMBER_SIZE = ["library", "visualizer", "lyrics", "art", "stats"];
  // Laid into a modern skin's place for it (the Library or the visualizer in
  // the skin's Media Library or visualizer place, modern_window.rs): no frame
  // at all, and the place's size, which the skin sets.
  let embedded = $state(Boolean(/** @type {any} */ (window).__SPOTIAMP_EMBEDDED__));
  /** its own size from before, for when it's out again @type {{ width: number, height: number } | null} */
  let sizeBeforeEmbed = null;
  /** @param {boolean} on */
  function setEmbedded(on) {
    if (on === embedded) return;
    if (on) sizeBeforeEmbed = { width: REACTIVE_WINDOW_SIZE.width, height: REACTIVE_WINDOW_SIZE.height };
    else if (sizeBeforeEmbed) REACTIVE_WINDOW_SIZE.setSize(sizeBeforeEmbed.width, sizeBeforeEmbed.height);
    embedded = on;
  }
  $effect(() => {
    const win = getCurrentWindow();
    // the login window is sized by Rust (600x800) and then redirects to
    // Spotify — don't shrink it to the player's dimensions
    if (OWN_SIZE || embedded) return;
    const w = REACTIVE_WINDOW_SIZE.width;
    const h = REACTIVE_WINDOW_SIZE.height;
    win.setSize(new LogicalSize(w * REACTIVE_WINDOW_SIZE.zoom, h * REACTIVE_WINDOW_SIZE.zoom));
    // Persist the (unzoomed) inner size so reopening restores it.
    if (REMEMBER_SIZE.includes(win.label)) {
      invoke("set_window_inner_size", {
        label: win.label,
        width: Math.round(w),
        height: Math.round(h),
      }).catch(() => {});
    }
  });

  // Moved onto a screen with another scale (100 % to 125 %, say): Windows
  // doesn't resize these frameless windows, so the same size in logical
  // pixels is put on again. Without it the playlist and the EQ kept their
  // old size in screen pixels, a fifth too small, with their insides cut off.
  if (!OWN_SIZE) {
    getCurrentWindow()
      .onScaleChanged(() => {
        if (embedded) return;
        const zoom = REACTIVE_WINDOW_SIZE.zoom;
        getCurrentWindow()
          .setSize(new LogicalSize(REACTIVE_WINDOW_SIZE.width * zoom, REACTIVE_WINDOW_SIZE.height * zoom))
          .catch(() => {});
      })
      .catch(() => {});
  }

  // Skins: every window reads the active skin and reapplies it live when it
  // changes (broadcast from the player's right-click menu).
  const CUSTOM_SKIN_VARS = [
    "main", "cbuttons", "monoster", "numbers", "playpaus", "pledit",
    "posbar", "shufrep", "text", "titlebar", "volume", "balance", "eqmain",
    "gentl", "genfill", "gentr", "gentitle", "titlebarcolor", "titletext",
    "plnormal", "plcurrent", "plbg", "plselbg", "textcolor", "libtitle",
    "genexitembg", "genexitemfg", "genexwndbg", "genexbtntext",
    "genexwndtext", "genexdivider", "genexselbg", "genexhdrbg",
    "genexhdrtext", "genexbtn", "genexbtnp", "genexbtnw", "genex", "viscolor", "plfont",
    "gen", "genletters", "genbar", "genmid",
    // the skin's cursors
    "cur-close", "cur-eqclose", "cur-eqnormal", "cur-eqslid", "cur-eqtitle", "cur-mainmenu", "cur-min", "cur-normal", "cur-pclose", "cur-pnormal", "cur-posbar", "cur-psize", "cur-ptbar", "cur-pvscroll", "cur-pwinbut", "cur-pwsnorm", "cur-pwssize", "cur-songname", "cur-titlebar", "cur-volbal", "cur-volbar", "cur-winbut", "cur-wsclose", "cur-wsmin", "cur-wsnormal", "cur-wsposbar", "cur-wswinbut",
  ];
  // "Use the skin's font": the playlist and Library lists in the font the
  // skin names (PLEDIT.TXT Font=; Arial, Winamp's own, when it names none)
  // instead of the pixel font. A setting of this PC, shared by the windows
  // through localStorage.
  function applySkinFont() {
    let on = false;
    try {
      on = localStorage.getItem("skin-font") === "1";
    } catch {}
    if (on) document.body.dataset.skinFont = "";
    else delete document.body.dataset.skinFont;
  }
  window.addEventListener("storage", (e) => e.key === "skin-font" && applySkinFont());
  window.addEventListener(SKIN_APPLIED, applySkinFont);

  /** @param {string} skin */
  function applySkin(skin) {
    if (skin === "custom") {
      // a .wsz loaded from disk: override the sprite vars with data-URLs
      // (images) and raw values (GENEX/PLEDIT colours). data-skin="custom"
      // lets pages opt into bitmap-face styling (genex buttons).
      document.body.dataset.skin = "custom";
      invoke("get_custom_skin")
        .then((sprites) => {
          // clear any vars a previous custom skin set but this one doesn't,
          // so nothing stale leaks between skins
          for (const name of CUSTOM_SKIN_VARS) {
            document.body.style.removeProperty(`--skin-${name}`);
          }
          for (const [name, value] of Object.entries(sprites)) {
            document.body.style.setProperty(
              `--skin-${name}`,
              value.startsWith("data:") ? `url("${value}")` : value,
            );
          }
          window.dispatchEvent(new Event(SKIN_APPLIED));
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
    window.dispatchEvent(new Event(SKIN_APPLIED));
  }
  // A modern skin worn: Spotiamp+'s own windows in its frame and colours, as
  // Winamp 5 drew its Media Library (modern-look.js). The frame lies behind
  // the page, which keeps to the room the frame leaves (--mf-*, global.css);
  // pressing the frame moves the window, its buttons and corner do what
  // they say.
  /** @type {Record<string, string>} */
  const LOOK_TITLES = {
    library: "Library",
    lyrics: "Lyrics",
    visualizer: "Visualizer",
    art: "Album Art",
    stats: "Listening Stats",
    museum: "Skin Museum",
    whatsnew: "What's New",
  };
  const CUR_VARS = CUSTOM_SKIN_VARS.filter((name) => name.startsWith("cur-"));
  let lookOff = () => {};
  let lookSeq = 0;
  async function applyLook() {
    const label = getCurrentWindow().label;
    if (!(label in LOOK_TITLES)) return;
    const seq = ++lookSeq;
    const [look, inSkin] = await Promise.all([
      /** @type {Promise<import("$lib/modern-look.js").Look | null>} */ (invoke("modern_look").catch(() => null)),
      invoke("modern_embedded").catch(() => false),
    ]);
    if (seq !== lookSeq) return;
    lookOff();
    lookOff = () => {};
    setEmbedded(Boolean(inSkin));
    useLookFonts(look);
    if (!look && !embedded) {
      delete document.body.dataset.modern;
      window.dispatchEvent(new Event(SKIN_APPLIED));
      return;
    }
    const colors = document.createElement("style");
    // (the classic skin's cursors go too)
    const cursors = CUR_VARS.map((n) => `--skin-${n}: initial !important;`).join(" ");
    if (look) colors.textContent = `body[data-modern] { ${lookColorsCss(look.colors)} ${cursors} }`;
    document.head.appendChild(colors);
    if (embedded || !look) {
      // in the skin's place: all of the window is the insides
      for (const side of ["left", "top", "right", "bottom"]) document.body.style.setProperty(`--mf-${side}`, "0px");
      document.body.dataset.modern = "embedded";
      window.dispatchEvent(new Event(SKIN_APPLIED));
      lookOff = () => {
        colors.remove();
        for (const side of ["left", "top", "right", "bottom"]) document.body.style.removeProperty(`--mf-${side}`);
      };
      return;
    }
    const host = document.createElement("div");
    host.id = "modern-frame";
    document.body.prepend(host);
    // What's New also opens as the key guide
    const titlebar = () => document.querySelector('[data-gen="titlebar"]');
    const titleOf = () =>
      label === "whatsnew" && /keyboard/i.test(titlebar()?.textContent ?? "") ? "Keyboard Shortcuts" : LOOK_TITLES[label];
    let title = "";
    const paint = () => {
      if (titleOf() === title) return;
      title = titleOf();
      const box = paintFrame(host, look, "window", title);
      for (const [side, v] of Object.entries(box)) document.body.style.setProperty(`--mf-${side}`, `${v}px`);
      setFrameActive(host, document.hasFocus());
    };
    paint();
    const retitle = new MutationObserver(paint);
    const bar = titlebar();
    if (bar) retitle.observe(bar, { subtree: true, childList: true, characterData: true });
    const grip = /** @type {HTMLElement | null} */ (document.querySelector('[data-gen="resize"]'));
    const unwire = wireFrame(host, {
      onAction(action) {
        if (action === "CLOSE") /** @type {HTMLElement | null} */ (document.querySelector('[data-gen="close"]'))?.click();
        else if (action === "MINIMIZE") getCurrentWindow().minimize().catch(() => {});
        else if (action === "SYSMENU" || action === "MENU") emitWindowEvent("modernMenu", { name: "main" });
        else if (action === "PLAY") emitWindowEvent("skinCommand", { play: true });
        else if (action === "PAUSE") emitWindowEvent("skinCommand", { pause: true });
        else if (action === "STOP") emitWindowEvent("skinCommand", { stop: true });
        else if (action === "NEXT") emitWindowEvent("skinCommand", { next: true });
        else if (action === "PREV") emitWindowEvent("skinCommand", { previous: true });
      },
      // the window's own corner grip sizes it (it keeps a cover square, snaps...);
      // one that isn't resized is moved from there too
      onResize: grip
        ? (e) => grip.onpointerdown?.call(grip, e)
        : undefined,
    });
    // dragged like the window's own title bar: snapping to the others where it docks
    if (DOCKABLE_LABELS.includes(label)) {
      makeDockedDraggable(host, label, /** @type {import("$lib/events.svelte.js").DockEventName} */ (`${label}Window`));
    } else {
      host.onpointerdown = (e) => {
        if (e.button === 0) getCurrentWindow().startDragging().catch(() => {});
      };
    }
    const focus = getCurrentWindow().onFocusChanged(({ payload }) => setFrameActive(host, payload));
    document.body.dataset.modern = "";
    window.dispatchEvent(new Event(SKIN_APPLIED));
    lookOff = () => {
      retitle.disconnect();
      unwire();
      focus.then((f) => f()).catch(() => {});
      host.remove();
      colors.remove();
      for (const side of ["left", "top", "right", "bottom"]) document.body.style.removeProperty(`--mf-${side}`);
    };
  }

  onMount(() => {
    // Report JS failures to the Rust log. A crashed page otherwise just shows a
    // broken webview with nothing recorded anywhere, which is exactly the state
    // we've had no evidence for.
    const label = getCurrentWindow().label;
    /** @param {string} message */
    const report = (message) =>
      invoke("log_frontend_error", { window: label, message: String(message).slice(0, 1000) }).catch(
        () => {},
      );
    /** @param {ErrorEvent} e */
    const onError = (e) =>
      report(`${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`);
    /** @param {PromiseRejectionEvent} e */
    const onRejection = (e) => report(`unhandled rejection: ${e.reason}`);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);

    // the login window redirects to Spotify — it has no skinnable UI
    if (label === "login") {
      return () => {
        window.removeEventListener("error", onError);
        window.removeEventListener("unhandledrejection", onRejection);
      };
    }

    // Winamp has no browser menu: suppress the WebView2 default context menu
    // (Refresh / Save as / Print…) app-wide. Our own right-click menus render
    // themselves and are unaffected.
    const suppressContextMenu = (/** @type {MouseEvent} */ e) => e.preventDefault();
    document.addEventListener("contextmenu", suppressContextMenu);

    invoke("get_skin").then(applySkin).catch(() => {});
    /** @type {(() => void) | undefined} */
    let unsub;
    subscribeToWindowEvent("skinChanged", (e) => applySkin(e.skin)).then(
      (u) => (unsub = u),
    );
    // a modern skin's look: now, when it's ready, and when the look changes
    applyLook();
    const lookChanged = subscribeToWindowEvent("modernLook", applyLook);
    const faceChanged = subscribeToWindowEvent("modernFace", applyLook);
    // laid into the skin's place, or out of it
    const embedChanged = getCurrentWindow().listen("modernEmbed", applyLook);
    // once those are heard, asked again: a look made while they weren't yet
    // would have gone unnoticed
    Promise.all([lookChanged, faceChanged, embedChanged]).then(applyLook, () => {});
    // Live UI-scale changes; the size effect above then resizes this window.
    /** @type {(() => void) | undefined} */
    let unsubScale;
    if (!OWN_SIZE) {
      subscribeToWindowEvent("uiScale", (e) => REACTIVE_WINDOW_SIZE.setZoom(e.scale)).then(
        (u) => (unsubScale = u),
      );
      // A page that reloaded after the scale changed still has the old number
      // from its startup script; catch up with the real one.
      invoke("get_ui_scale")
        .then((s) => {
          if (typeof s === "number" && s !== REACTIVE_WINDOW_SIZE.zoom) {
            REACTIVE_WINDOW_SIZE.setZoom(s);
          }
        })
        .catch(() => {});
    }
    return () => {
      document.removeEventListener("contextmenu", suppressContextMenu);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
      unsub?.();
      unsubScale?.();
      lookChanged.then((u) => u());
      faceChanged.then((u) => u());
      embedChanged.then((u) => u());
      lookOff();
    };
  });
</script>

{@render children()}
