<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { Menu } from "@tauri-apps/api/menu";
  import { onMount } from "svelte";
  import { badgeList } from "$lib/badges.js";
  import ANIMS_JSON from "$lib/mascot/anims.json";
  import idle from "$lib/mascot/idle.png";
  import dance from "$lib/mascot/dance.png";
  import sleep from "$lib/mascot/sleep.png";
  import celebrate from "$lib/mascot/celebrate.png";
  import happy from "$lib/mascot/happy.png";
  import surprised from "$lib/mascot/surprised.png";
  import sad from "$lib/mascot/sad.png";
  import love from "$lib/mascot/love.png";

  // The llama (mascot.rs owns the window). It dances while music plays,
  // falls asleep a minute after it stops, and reacts: a new badge (a hop), a
  // loved song (a heart), a song that won't load (sad), a click (happy).
  // Everything comes from events the other windows already send.

  const STRIPS = { idle, dance, sleep, celebrate, happy, surprised, sad, love };
  const FRAME_W = ANIMS_JSON._frame.width;
  const FRAME_H = ANIMS_JSON._frame.height;
  /** each animation: its frame count and how long each frame shows */
  const ANIMS = /** @type {Record<string, {frames: number, ms: number[]}>} */ (/** @type {unknown} */ (ANIMS_JSON));
  const SLEEP_AFTER_MS = 60_000;
  const BADGE_CHECK_MS = 5 * 60_000;

  /** @type {Record<string, HTMLImageElement>} */
  const images = {};
  for (const [name, src] of Object.entries(STRIPS)) {
    const img = new Image();
    img.src = src;
    images[name] = img;
  }

  /** @type {HTMLCanvasElement | undefined} */
  let canvas = $state();
  let playing = false;
  let lastPlayingAt = Date.now();
  /** a reaction playing over the usual mood: once through, or until a time */
  let reaction = /** @type {{name: string, once: boolean, until: number} | null} */ (null);
  let current = "";
  let frame = 0;

  /** @param {string} name @param {number} [ms] how long, for a still mood */
  function react(name, ms = 2500) {
    const once = name === "celebrate";
    reaction = { name, once, until: Date.now() + ms };
  }

  function mood() {
    const now = Date.now();
    if (reaction && (reaction.once ? !(reaction.name === current && frame >= ANIMS[current].frames) : now < reaction.until)) {
      return reaction.name;
    }
    reaction = null;
    if (playing) return "dance";
    return now - lastPlayingAt > SLEEP_AFTER_MS ? "sleep" : "idle";
  }

  // Pixel art drawn at any size without going blurry: blow the frame up by a
  // whole number first (crisp), then scale that down smoothly to fit.
  const big = document.createElement("canvas");
  /** @param {string} name @param {number} i */
  function draw(name, i) {
    const img = images[name];
    if (!canvas || !img?.complete || !img.naturalWidth) return;
    const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
    const W = canvas.width;
    const H = canvas.height;
    const k = Math.max(1, Math.ceil(H / FRAME_H));
    if (big.width !== FRAME_W * k) {
      big.width = FRAME_W * k;
      big.height = FRAME_H * k;
    }
    const b = /** @type {CanvasRenderingContext2D} */ (big.getContext("2d"));
    b.imageSmoothingEnabled = false;
    b.clearRect(0, 0, big.width, big.height);
    b.drawImage(img, i * FRAME_W, 0, FRAME_W, FRAME_H, 0, 0, big.width, big.height);
    ctx.clearRect(0, 0, W, H);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    // keep its shape even if the window came out wider than asked (Windows
    // has a minimum width): fit the height, sit at the right, feet down
    const h = Math.min(H, Math.round((W * FRAME_H) / FRAME_W));
    const w = Math.round((h * FRAME_W) / FRAME_H);
    ctx.drawImage(big, W - w, H - h, w, h);
  }

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer;
  function tick() {
    const name = mood();
    if (name !== current) {
      current = name;
      frame = 0;
    }
    const anim = ANIMS[current];
    const shown = Math.min(frame, anim.frames - 1);
    draw(current, shown);
    const ms = anim.ms[shown] ?? 200;
    frame = reaction?.once ? frame + 1 : (frame + 1) % anim.frames;
    timer = setTimeout(tick, ms);
  }

  function fit() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(window.innerWidth * dpr);
    canvas.height = Math.round(window.innerHeight * dpr);
    if (current) draw(current, Math.min(frame, ANIMS[current].frames - 1));
  }

  // A new badge since the last look: hop. The first look only sets the count.
  let badgesDone = /** @type {number | null} */ (null);
  async function checkBadges() {
    try {
      const [all, loved] = await Promise.all([
        invoke("history_stats", { since: 0, until: null, limit: 1 }),
        invoke("get_loved").catch(() => []),
      ]);
      const done = badgeList(/** @type {any} */ (all), /** @type {string[]} */ (loved).length).filter((b) => b.done).length;
      if (badgesDone !== null && done > badgesDone) react("celebrate");
      badgesDone = done;
    } catch {}
  }

  // Right-click: hide it, or pick its size. The old menu is closed before a
  // new one is made (see the playlist's showMenu: doing both at once could
  // freeze every window).
  /** @type {Menu | null} */
  let openedMenu = null;
  let menuBusy = false;
  async function showMenu() {
    if (menuBusy) return;
    menuBusy = true;
    try {
      const settings = /** @type {{enabled: boolean, size: number}} */ (await invoke("mascot_settings"));
      const old = openedMenu;
      openedMenu = null;
      if (old) await old.close().catch(() => {});
      const size = (/** @type {number} */ px, /** @type {string} */ text) => ({
        text,
        checked: settings.size === px,
        action: () => invoke("mascot_set", { size: px }).catch(() => {}),
      });
      const menu = await Menu.new({
        items: [
          { text: "Size", items: [size(48, "Small"), size(56, "Normal"), size(64, "Large")] },
          { item: "Separator" },
          { text: "Hide the llama", action: () => invoke("mascot_set", { enabled: false }).catch(() => {}) },
        ],
      });
      openedMenu = menu;
      await menu.popup();
    } catch (e) {
      invoke("log_frontend_error", { window: "mascot", message: `menu: ${e}` }).catch(() => {});
    } finally {
      menuBusy = false;
    }
  }

  onMount(() => {
    fit();
    window.addEventListener("resize", fit);
    for (const img of Object.values(images)) img.onload = () => fit();
    tick();
    /** @type {(() => void)[]} */
    const offs = [];
    // the player sends the song and whether it's playing every second
    listen("art", (e) => {
      const p = /** @type {{playing?: boolean}} */ (e.payload);
      playing = !!p?.playing;
      if (playing) lastPlayingAt = Date.now();
    }).then((u) => offs.push(u));
    listen("mascotReact", (e) => react(String(e.payload))).then((u) => offs.push(u));
    // a song loved anywhere (F, a song menu, the Library)
    listen("lovedChanged", (e) => {
      if (e.payload === true) react("love");
    }).then((u) => offs.push(u));
    const firstLook = setTimeout(checkBadges, 10_000);
    const badgeTimer = setInterval(checkBadges, BADGE_CHECK_MS);
    return () => {
      clearTimeout(timer);
      clearTimeout(firstLook);
      clearInterval(badgeTimer);
      window.removeEventListener("resize", fit);
      for (const off of offs) off();
    };
  });
</script>

<canvas
  bind:this={canvas}
  onclick={() => react("happy", 1800)}
  oncontextmenu={(e) => {
    e.preventDefault();
    showMenu();
  }}
></canvas>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: transparent !important;
  }
  canvas {
    display: block;
    width: 100vw;
    height: 100vh;
    cursor: pointer;
  }
</style>
