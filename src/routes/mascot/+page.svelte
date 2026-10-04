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
  import dizzy from "$lib/mascot/dizzy.png";
  import yawn from "$lib/mascot/yawn.png";
  import idle_party from "$lib/mascot/idle_party.png";
  import dance_party from "$lib/mascot/dance_party.png";
  import walkStrip from "$lib/mascot/walk.png";
  import stand from "$lib/mascot/stand.png";
  import heldStrip from "$lib/mascot/held.png";

  // Lala, the llama (mascot.rs owns the window). It dances while music plays,
  // falls asleep a minute after it stops, and reacts: a new badge (a hop), a
  // loved song (a heart), a song that won't load (sad), a click (happy).
  // Everything comes from events the other windows already send.

  const STRIPS = { idle, dance, sleep, celebrate, happy, surprised, sad, love, dizzy, yawn, idle_party, dance_party, walk: walkStrip, stand, held: heldStrip };
  /** reactions that play through once rather than for a while */
  const ONCE = ["celebrate", "yawn"];
  // April 21, Winamp's birthday: a party hat all day
  const PARTY = (() => {
    const d = new Date();
    return d.getMonth() === 3 && d.getDate() === 21;
  })();
  const FRAME_W = ANIMS_JSON._frame.width;
  const FRAME_H = ANIMS_JSON._frame.height;
  /** each animation: its frame count and how long each frame shows */
  const ANIMS = /** @type {Record<string, {frames: number, ms: number[]}>} */ (/** @type {unknown} */ (ANIMS_JSON));
  const SLEEP_AFTER_MS = 60_000;
  // a pat keeps her up this long (she went straight back to sleep)
  const WOKEN_MS = 2 * 60_000;
  const BADGE_CHECK_MS = 5 * 60_000;
  /** how much the bass has to jump between two looks, against its usual level */
  const BEAT_RISE = 0.2;

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
  /** the song last seen starting, so each one is noticed once */
  let songKey = /** @type {string | null} */ (null);
  let lastPlayingAt = Date.now();
  /** a reaction playing over the usual mood: once through, or until a time */
  let reaction = /** @type {{name: string, once: boolean, until: number} | null} */ (null);
  let current = "";
  let frame = 0;
  let yawnedForSleep = false;
  let awakeUntil = 0;
  /** being dragged along the player's edge */
  let held = false;
  /** the player is being dragged with her on it: she holds on */
  let ridingUntil = 0;

  // A stroll: now and then, awake with no music on, she gets up, walks a few
  // steps along the player's edge (facing the way she goes), stands a moment
  // and sits down again where she ended up.
  let walk = /** @type {{dir: number, steps: number, step: number, total: number, standUntil: number} | null} */ (null);
  /** @param {boolean} [asked] a double-click: go now, even mid-smile */
  async function stroll(asked = false) {
    if (walk || playing || held) return;
    if (asked) reaction = null;
    else if (reaction || current !== (PARTY ? "idle_party" : "idle")) return;
    const room = /** @type {[number, number]} */ (await invoke("mascot_room").catch(() => [0, 0]));
    const px = window.innerHeight / FRAME_H; // screen px per art px
    const dir = room[0] > room[1] ? -1 : room[1] > room[0] ? 1 : Math.random() < 0.5 ? -1 : 1;
    const dist = Math.min((20 + Math.random() * 40) * px, (dir < 0 ? room[0] : room[1]) - 2);
    if (dist < 8 * px) return;
    const step = 3 * px;
    walk = { dir, steps: Math.ceil(dist / step), step, total: 0, standUntil: 0 };
  }
  /** stop strolling (music, a drag) and stay where she got to */
  function endWalk() {
    if (!walk) return;
    if (walk.total) invoke("mascot_drag", { dx: walk.total, done: true }).catch(() => {});
    walk = null;
  }

  // The beat: the bass in the spectrum jumping up. The spectrum is smoothed
  // and coarse (19 points), so only some beats show, but the gaps between
  // them give the tempo. The llama nods on a clock at that tempo and snaps
  // back in line with every beat it does catch; with no tempo it free-runs.
  let spectrumCommand = "take_latest_spectrum"; // the loopback one in Free Mode
  let lastBeatAt = 0;
  /** @type {number[]} recent gaps between beats that look like one beat */
  const gaps = [];
  let period = 0;
  let nextNodAt = 0;
  /** @type {number[]} dance frames left of the nod in progress */
  let nod = [];
  let nodRight = true;
  const onTempo = () => period > 0 && Date.now() - lastBeatAt < 6000;

  /** @param {number} now */
  function onBeat(now) {
    const gap = now - lastBeatAt;
    if (gap >= 260 && gap <= 750) {
      gaps.push(gap);
      if (gaps.length > 10) gaps.shift();
      if (gaps.length >= 3) period = [...gaps].sort((a, b) => a - b)[gaps.length >> 1];
    }
    lastBeatAt = now;
    if (!period) return;
    nextNodAt = now; // in line with the beat
    if (current.startsWith("dance") && !nod.length) {
      clearTimeout(timer);
      tick();
    }
  }

  let alive = true;
  async function listenForBeats() {
    let prev = 0;
    let avg = 0;
    while (alive) {
      if (playing && !held) {
        try {
          const spectrum = /** @type {[number, number][]} */ (await invoke(spectrumCommand));
          let sum = 0;
          let n = 0;
          for (const [freq, volume] of spectrum) {
            if (freq < 150) {
              sum += volume;
              n++;
            }
          }
          const bass = n ? sum / n : 0;
          const rise = bass - prev;
          prev = bass;
          avg = avg * 0.92 + bass * 0.08;
          const now = Date.now();
          // a jump of a fifth of the usual bass level, so quiet and loud songs alike
          if (rise > avg * BEAT_RISE && bass >= avg && now - lastBeatAt > 260) onBeat(now);
        } catch {}
      }
      await new Promise((r) => setTimeout(r, 45));
    }
  }

  // Speech bubble: a short line in Winamp's display colours to its left, for
  // a few seconds (a new badge, hello, Rewind). The window widens for it.
  let bubbleText = "";
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let bubbleTimer;
  const FONT = `7px "px sans nouveaux", monospace`;
  /** @param {string} text @param {number} [ms] */
  async function say(text, ms = 5000) {
    await document.fonts.load(FONT).catch(() => {});
    const c = /** @type {CanvasRenderingContext2D} */ (document.createElement("canvas").getContext("2d"));
    c.font = FONT;
    bubbleText = text;
    // text, padding, the tail, a margin (art px)
    await invoke("mascot_bubble", { width: Math.ceil(c.measureText(text).width) + 18 }).catch(() => {});
    fit();
    clearTimeout(bubbleTimer);
    bubbleTimer = setTimeout(() => {
      bubbleText = "";
      invoke("mascot_bubble", { width: 0 }).catch(() => {});
      fit();
    }, ms);
  }

  // --- Chatter: now and then she says something useful or friendly ---------
  // A hello for the time of day, a listening streak, round play counts of the
  // song playing, a Spotify hiccup, and tips on what the app can do (each tip
  // once). Never while asleep or held, never over another line, and only so
  // often, so she stays company rather than a pop-up. Right-click: off.
  let chatty = true;
  try {
    chatty = localStorage.getItem("lala-chatty") !== "0";
  } catch {}
  let lastChat = 0;
  /** @param {number} gap ms since the last line */
  function canChat(gap) {
    // asleep, but the music just started: she's waking up for it
    const awake = playing || current !== "sleep";
    return chatty && awake && !bubbleText && !held && Date.now() - lastChat >= gap;
  }
  /**
   * @param {string} text
   * @param {{gap?: number, ms?: number, mood?: string}} [options] mood "" leaves her face alone
   */
  function chat(text, { gap = 3 * 60_000, ms = 6000, mood = "happy" } = {}) {
    if (!canChat(gap)) return false;
    lastChat = Date.now();
    if (mood) react(mood, 2500);
    say(text, ms);
    return true;
  }
  /** true only the first time for `key`, across runs */
  function firstTime(/** @type {string} */ key) {
    try {
      if (localStorage.getItem(key)) return false;
      localStorage.setItem(key, "1");
      return true;
    } catch {
      return false;
    }
  }

  // days in a row with music, up to yesterday (from the badge check's stats)
  let streakDays = 0;
  /** @param {[number, number][]} timeline */
  function streakFrom(timeline) {
    const days = new Set(timeline.map(([at]) => new Date(at).toDateString()));
    const d = new Date();
    d.setDate(d.getDate() - 1);
    let n = 0;
    while (days.has(d.toDateString())) {
      n++;
      d.setDate(d.getDate() - 1);
    }
    return n;
  }

  /** A song started (Spotify uri or local:path). */
  async function onSong(/** @type {string | null} */ uri) {
    const now = new Date();
    // the day's first song: the streak, or a hello for the time of day (the
    // day's hello isn't spent while she can't say it)
    if (canChat(0) && firstTime(`lala-day-${now.toDateString()}`)) {
      const h = now.getHours();
      const streak = streakDays + 1;
      if (streak >= 3) chat(`Day ${streak} in a row!`, { gap: 0, mood: "celebrate" });
      else if (h >= 5 && h < 12) chat("Good morning! Music time", { gap: 0 });
      else if (h >= 18) chat("Good evening!", { gap: 0 });
      else if (h < 5) chat("Still up? Me too", { gap: 0, mood: "yawn" });
      else chat("Hello again!", { gap: 0 });
      return;
    }
    if (!uri) return;
    const before = /** @type {number} */ (await invoke("history_song_plays", { uri }).catch(() => -1));
    if (before < 0) return;
    const nth = before + 1;
    if ([10, 25, 50, 100, 250, 500, 1000].includes(nth)) {
      chat(`Play number ${nth} of this one!`, { gap: 30_000, mood: "love" });
    } else if (before === 0 && Math.random() < 0.25) {
      chat("Ooh, a new one!", { mood: "surprised" });
    }
  }

  // Things the app can do that are easy to miss; each said once, in a long
  // stretch of music.
  const TIPS = [
    "Tip: J finds any song in the list",
    "Tip: Ctrl+J jumps to a time",
    "Tip: F loves the song playing",
    "Tip: Q plays the selected song next",
    "Tip: EQ's AUTO remembers a song's sound",
    "Tip: F1 shows every shortcut",
    "Tip: double-click me for a walk",
    "Tip: Ctrl+D makes everything bigger",
    "Tip: your top songs are in Stats",
    "Tip: drag me along the player's edge",
    "Tip: right-click the playlist for skins",
  ];
  const TIP_EVERY_MIN = 25;
  let minutesSinceTip = 0;
  function maybeTip() {
    if (!playing || ++minutesSinceTip < TIP_EVERY_MIN || !canChat(10 * 60_000)) return;
    let next = 0;
    try {
      next = Number(localStorage.getItem("lala-tip") || 0);
    } catch {}
    if (next >= TIPS.length) return;
    try {
      localStorage.setItem("lala-tip", String(next + 1));
    } catch {}
    minutesSinceTip = 0;
    chat(TIPS[next], { gap: 0, ms: 8000 });
  }

  /** @param {string} name @param {number} [ms] how long, for a still mood */
  function react(name, ms = 2500) {
    const once = ONCE.includes(name);
    reaction = { name, once, until: Date.now() + ms };
  }

  function mood() {
    const now = Date.now();
    if (reaction && (reaction.once ? !(reaction.name === current && frame >= ANIMS[current].frames) : now < reaction.until)) {
      return reaction.name;
    }
    reaction = null;
    if (held) return "held";
    if (now < ridingUntil) return "surprised";
    if (walk) {
      if (walk.steps > 0) return "walk";
      if (now < walk.standUntil) return "stand";
      walk = null;
    }
    if (playing) return PARTY ? "dance_party" : "dance";
    if (now - lastPlayingAt > SLEEP_AFTER_MS && now > awakeUntil) {
      // a yawn first, then she lies down
      if (current !== "sleep" && !yawnedForSleep) {
        yawnedForSleep = true;
        react("yawn");
        return "yawn";
      }
      return "sleep";
    }
    yawnedForSleep = false;
    return PARTY ? "idle_party" : "idle";
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
    const spin = spinUntil - Date.now();
    if (walk && walk.dir < 0 && (name === "walk" || name === "stand")) {
      // facing left: mirrored about her middle (x 31 of the frame), so she
      // turns round on the spot
      ctx.save();
      ctx.translate(W - w + (62 * w) / FRAME_W, 0);
      ctx.scale(-1, 1);
      ctx.drawImage(big, 0, H - h, w, h);
      ctx.restore();
    } else if (spin > 0) {
      // a full turn on the spot (the start of being dizzy)
      ctx.save();
      ctx.translate(W - w / 2, H - h * 0.45);
      ctx.rotate(((SPIN_MS - spin) / SPIN_MS) * Math.PI * 2);
      ctx.drawImage(big, -w / 2, -h * 0.55, w, h);
      ctx.restore();
    } else {
      ctx.drawImage(big, W - w, H - h, w, h);
    }
    if (bubbleText) drawBubble(ctx, W - w, H - h, h / FRAME_H);
  }

  /**
   * The bubble left of the frame at `x0`, about head height, its tail
   * reaching into the frame's empty margin towards the llama.
   * @param {CanvasRenderingContext2D} ctx @param {number} x0 @param {number} y0 @param {number} k px per art px
   */
  function drawBubble(ctx, x0, y0, k) {
    const px = (/** @type {number} */ v) => Math.round(v * k);
    const top = y0 + px(17);
    const height = px(13);
    const left = px(1);
    const right = x0 + px(4);
    // in the skin's own colours (its list window), like Winamp's plugins
    const css = getComputedStyle(document.body);
    const skin = (/** @type {string} */ name, /** @type {string} */ fallback) => css.getPropertyValue(name).trim() || fallback;
    const edge = skin("--skin-genexdivider", "#5a5f74");
    ctx.imageSmoothingEnabled = false;
    ctx.fillStyle = edge;
    ctx.fillRect(left, top, right - left, height);
    ctx.fillStyle = skin("--skin-genexitembg", "#000");
    ctx.fillRect(left + px(1), top + px(1), right - left - px(2), height - px(2));
    // the tail
    ctx.fillStyle = edge;
    ctx.beginPath();
    ctx.moveTo(right - 1, top + px(4));
    ctx.lineTo(x0 + px(12), top + px(7));
    ctx.lineTo(right - 1, top + px(9));
    ctx.fill();
    ctx.fillStyle = skin("--skin-genexitemfg", "#00e000");
    ctx.font = `${px(7)}px "px sans nouveaux", monospace`;
    ctx.textBaseline = "middle";
    ctx.fillText(bubbleText, left + px(5), top + height / 2 + px(0.5));
  }

  // Pats: one makes it smile; ten in a row and it spins round and goes dizzy.
  const SPIN_MS = 700;
  let spinUntil = 0;
  /** @type {number[]} */
  let pats = [];
  function pat() {
    const now = Date.now();
    const asleep = current === "sleep";
    awakeUntil = now + WOKEN_MS;
    if (asleep) {
      // woken up: a start, a word, and she stays up a while
      yawnedForSleep = false;
      react("surprised", 1200);
      // once she's no longer drawn asleep (the next frame), she can talk
      setTimeout(() => chat("I'm up, I'm up!", { gap: 60_000, mood: "" }), 400);
      return;
    }
    pats = [...pats.filter((t) => now - t < 4000), now];
    if (pats.length >= 10) {
      pats = [];
      spinUntil = now + SPIN_MS;
      react("dizzy", 3200);
      say("Whoa... the room is spinning", 3500);
      spinFrames();
    } else {
      react("happy", 1800);
    }
  }
  // redraw smoothly while spinning (the frames themselves change slower)
  function spinFrames() {
    if (Date.now() >= spinUntil || !current) return;
    draw(current, Math.min(frame, ANIMS[current].frames - 1));
    setTimeout(spinFrames, 30);
  }

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer;
  function tick() {
    const name = mood();
    if (name !== current) {
      current = name;
      frame = 0;
      // what it's doing, readable from outside (handy when checking it)
      if (canvas) canvas.dataset.mood = name;
    }
    if (canvas) canvas.dataset.tempo = String(period);
    if (current.startsWith("dance") && onTempo()) {
      // one nod per beat, right then left (frames 1-3, 5-7; 0 and 4 rest),
      // each frame a quarter of the beat
      const now = Date.now();
      if (!nod.length && now >= nextNodAt - 20) {
        nodRight = !nodRight;
        nod = nodRight ? [1, 2, 3, 4] : [5, 6, 7, 0];
        nextNodAt = Math.max(nextNodAt + period, now + period / 2);
      }
      const f = nod.length ? /** @type {number} */ (nod.shift()) : nodRight ? 4 : 0;
      draw(current, f);
      const step = Math.min(200, Math.max(50, period / 4));
      timer = setTimeout(tick, nod.length ? step : Math.max(15, Math.min(step, nextNodAt - Date.now())));
      return;
    }
    if (current === "walk" && walk) {
      const f = frame % ANIMS.walk.frames;
      draw("walk", f);
      walk.total += walk.dir * walk.step;
      walk.steps -= 1;
      invoke("mascot_drag", { dx: walk.total, done: walk.steps <= 0 }).catch(() => {});
      if (walk.steps <= 0) {
        walk.total = 0; // saved: nothing left to put down
        walk.standUntil = Date.now() + 900;
      }
      frame = f + 1;
      timer = setTimeout(tick, ANIMS.walk.ms[f]);
      return;
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
    canvas.style.width = `${window.innerWidth}px`;
    canvas.style.height = `${window.innerHeight}px`;
    if (current) draw(current, Math.min(frame, ANIMS[current].frames - 1));
  }

  // A new badge since the last look: hop. The first look only sets the count.
  let badgesDone = /** @type {string[] | null} */ (null);
  async function checkBadges() {
    try {
      const [all, loved] = await Promise.all([
        invoke("history_stats", { since: 0, until: null, limit: 1 }),
        invoke("get_loved").catch(() => []),
      ]);
      const done = badgeList(/** @type {any} */ (all), /** @type {string[]} */ (loved).length).filter((b) => b.done);
      streakDays = streakFrom(/** @type {any} */ (all).timeline ?? []);
      if (badgesDone !== null && done.length > badgesDone.length) {
        const fresh = done.find((b) => !badgesDone?.includes(b.name));
        react("celebrate");
        if (fresh) say(`New badge: ${fresh.name}!`, 6000);
      }
      badgesDone = done.map((b) => b.name);
    } catch {}
  }

  // Drag it along the player's top edge (it looks startled while held); a
  // click without moving is a pat, and it smiles.
  let down = /** @type {{x: number, dx: number, moved: boolean} | null} */ (null);
  /** @param {PointerEvent} e */
  function onPointerDown(e) {
    if (e.button !== 0 || !canvas) return;
    endWalk();
    down = { x: e.screenX, dx: 0, moved: false };
    canvas.setPointerCapture(e.pointerId);
  }
  /** @param {PointerEvent} e */
  function onPointerMove(e) {
    if (!down) return;
    // how far from where it was pressed (the window moves under the pointer,
    // the screen doesn't)
    down.dx = e.screenX - down.x;
    if (!down.moved && Math.abs(down.dx) < 3) return;
    down.moved = true;
    held = true;
    invoke("mascot_drag", { dx: down.dx, done: false }).catch(() => {});
  }
  function onPointerUp() {
    if (!down) return;
    const { moved, dx } = down;
    down = null;
    held = false;
    if (moved) invoke("mascot_drag", { dx, done: true }).catch(() => {});
    else pat();
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
          {
            text: "Talk now and then",
            checked: chatty,
            action: () => {
              chatty = !chatty;
              try {
                localStorage.setItem("lala-chatty", chatty ? "1" : "0");
              } catch {}
            },
          },
          { item: "Separator" },
          { text: "Hide Lala", action: () => invoke("mascot_set", { enabled: false }).catch(() => {}) },
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
      const p = /** @type {{playing?: boolean, uri?: string | null, title?: string}} */ (e.payload);
      const was = playing;
      playing = !!p?.playing;
      const song = p?.uri ?? p?.title ?? null;
      if (playing && song && song !== songKey) {
        songKey = song;
        onSong(p?.uri ?? null);
      }
      if (playing) lastPlayingAt = Date.now();
      if (playing && walk) endWalk();
      // asleep and the music starts: wake up with a start
      if (playing && !was && current === "sleep") react("surprised", 1200);
    }).then((u) => offs.push(u));
    invoke("is_controller_mode")
      .then((on) => {
        if (on) spectrumCommand = "loopback_spectrum";
      })
      .catch(() => {});
    listenForBeats();
    listen("mascotReact", (e) => react(String(e.payload))).then((u) => offs.push(u));
    listen("mascotRide", () => {
      ridingUntil = Date.now() + 350;
    }).then((u) => offs.push(u));
    // a new skin: she notices
    listen("skinChanged", () => {
      if (current === "sleep") return; // let her sleep
      react("happy", 2500);
      say("Ooh, new look!", 3500);
    }).then((u) => offs.push(u));
    // the playlist ran out
    listen("playlistWindow", (e) => {
      const ev = /** @type {Record<string, unknown>} */ (e.payload ?? {});
      if (ev && "EndReached" in ev) say("That was the last song!", 4000);
    }).then((u) => offs.push(u));
    // a song loved anywhere (F, a song menu, the Library)
    listen("lovedChanged", (e) => {
      if (e.payload !== true) return;
      react("love");
      chat("Saved to your Loved songs", { gap: 2 * 60_000, mood: "" });
    }).then((u) => offs.push(u));
    // Spotify dropped the connection and the song carried on after it
    listen("spotifyReconnected", () => {
      chat("Spotify hiccuped. We're back!", { gap: 60_000, mood: "surprised" });
    }).then((u) => offs.push(u));
    const firstLook = setTimeout(checkBadges, 10_000);
    // things it says once: hello the first time, Rewind once a December
    /** @param {string} key */
    const once = (key) => {
      try {
        if (localStorage.getItem(key)) return false;
        localStorage.setItem(key, "1");
        return true;
      } catch {
        return false;
      }
    };
    const now = new Date();
    const rewindYear = now.getMonth() === 0 ? now.getFullYear() - 1 : now.getFullYear();
    const y = now.getFullYear();
    const day = `${now.getMonth() + 1}-${now.getDate()}`;
    const greet = setTimeout(() => {
      if (once("llama-hello")) say("Hi, I'm Lala! Right-click me for options", 8000);
      else if (PARTY && once(`llama-birthday-${y}`)) {
        react("celebrate");
        say("Happy birthday, Winamp!", 7000);
      } else if (day === "1-1" && once(`llama-newyear-${y}`)) {
        react("celebrate");
        say(`Happy new year! Hello ${y}`, 7000);
      } else if (day === "10-31" && once(`llama-halloween-${y}`)) {
        react("surprised", 2500);
        say("Boo! Happy Halloween", 6000);
      } else if ((now.getMonth() === 11 || now.getMonth() === 0) && once(`llama-rewind-${rewindYear}`)) {
        react("happy", 3000);
        say(`Your Rewind ${rewindYear} is ready! (Stats)`, 8000);
      }
    }, 4000);
    // after midnight it gets sleepy: a yawn now and then; and after two
    // hours of music in one go, it suggests a stretch (once)
    let playingSince = 0;
    let stretched = false;
    const strolls = setInterval(() => {
      if (Math.random() < 0.4) stroll();
    }, 30_000);
    const nightly = setInterval(() => {
      const t = new Date();
      if (playing) {
        playingSince ||= Date.now();
        if (!stretched && Date.now() - playingSince > 2 * 3600_000) {
          stretched = true;
          react("happy", 3000);
          say("Two hours of music! Time to stretch?", 7000);
        }
      } else if (Date.now() - lastPlayingAt > 10 * 60_000) {
        playingSince = 0;
      }
      maybeTip();
      if (t.getHours() < 5 && !reaction && !held && current !== "sleep" && Math.random() < 0.25) react("yawn");
    }, 60_000);
    const badgeTimer = setInterval(checkBadges, BADGE_CHECK_MS);
    return () => {
      alive = false;
      clearTimeout(timer);
      clearTimeout(firstLook);
      clearTimeout(greet);
      clearInterval(nightly);
      clearInterval(strolls);
      clearTimeout(bubbleTimer);
      clearInterval(badgeTimer);
      window.removeEventListener("resize", fit);
      for (const off of offs) off();
    };
  });
</script>

<canvas
  bind:this={canvas}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={() => stroll(true)}
  oncontextmenu={(e) => {
    e.preventDefault();
    showMenu();
  }}
></canvas>

<style>
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: transparent !important;
  }
  canvas {
    display: block;
    /* pinned to the bottom-right with a fixed size (see fit): while the window
       grows or shrinks for a speech bubble she stays exactly where she is */
    position: fixed;
    right: 0;
    bottom: 0;
    cursor: grab;
  }
</style>
