<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, tick } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable, makeSnappingResizer } from "$lib/window-docking.svelte.js";
  import { forwardShortcuts } from "$lib/shortcuts.js";

  // Listening stats, from Spotiamp+'s own history (history.rs). Everything is
  // worked out on this computer; nothing is sent anywhere.

  // The main window's keys work here too (lib/shortcuts.js).
  onMount(() => forwardShortcuts());

  const DAY = 86_400_000;
  const PERIODS = [
    { id: "week", label: "7 DAYS", days: 7 },
    { id: "month", label: "30 DAYS", days: 30 },
    { id: "year", label: "YEAR", days: 365 },
    { id: "all", label: "ALL TIME", days: 0 },
  ];

  // Rewind opens on December 1 and stays through January (still the year just
  // gone); before that its tab only counts down, so it's a surprise.
  const NOW = new Date();
  const REWIND_OPEN = NOW.getMonth() === 11 || NOW.getMonth() === 0;
  const YEAR = NOW.getMonth() === 0 ? NOW.getFullYear() - 1 : NOW.getFullYear();

  /**
   * @typedef {{uri: string, title: string, artist: string, plays: number, ms: number}} TopTrack
   * @typedef {{name: string, plays: number, ms: number}} TopArtist
   * @typedef {{plays: number, ms: number, tracks: number, artists: number, first_at: number,
   *   top_tracks: TopTrack[], top_artists: TopArtist[], local_plays: number, timeline: [number, number][]}} Stats
   */

  let period = $state("month");
  let listTab = $state("songs");
  let stats = $state(/** @type {Stats | null} */ (null));
  /** every play ever, for the streak */
  let allTimeline = $state(/** @type {[number, number][]} */ ([]));
  /** all time, for the badges */
  let allStats = $state(/** @type {Stats | null} */ (null));
  let lovedCount = $state(0);
  let loading = $state(true);

  // The top list scrolls with a Winamp handle (see .st-scroll), not the
  // browser's scrollbar.
  /** @type {HTMLDivElement | undefined} */
  let listEl = $state();
  let scrollPos = $state(0);
  let scrollMax = $state(0);
  function syncScroll() {
    if (!listEl) return;
    scrollMax = Math.max(0, listEl.scrollHeight - listEl.clientHeight);
    scrollPos = listEl.scrollTop;
  }

  /** Local midnight of the day `at` falls on. */
  function dayStart(at) {
    const d = new Date(at);
    d.setHours(0, 0, 0, 0);
    return d.getTime();
  }

  /** The start of the period, in epoch ms: whole days, counting today. */
  function sinceFor(id) {
    if (id === "rewind") return new Date(YEAR, 0, 1).getTime();
    const p = PERIODS.find((p) => p.id === id);
    if (!p || !p.days) return 0;
    return dayStart(Date.now()) - (p.days - 1) * DAY;
  }

  let token = 0;
  async function refresh() {
    const my = ++token;
    try {
      const [s, all, loved] = await Promise.all([
        invoke("history_stats", {
          since: sinceFor(period),
          until: period === "rewind" ? new Date(YEAR + 1, 0, 1).getTime() : null,
          limit: 50,
        }),
        // all time, with the single top song and artist, for the badges
        invoke("history_stats", { since: 0, until: null, limit: 1 }),
        invoke("get_loved").catch(() => []),
      ]);
      if (my !== token) return;
      stats = /** @type {Stats} */ (s);
      allStats = /** @type {Stats} */ (all);
      allTimeline = allStats.timeline;
      lovedCount = /** @type {string[]} */ (loved).length;
    } catch {
      if (my === token) stats = null;
    } finally {
      if (my === token) loading = false;
    }
  }

  $effect(() => {
    period; // re-run on a period change
    refresh();
  });

  /** "27h 5m", "45m", "0m" */
  function fmtDuration(ms) {
    const mins = Math.round(ms / 60000);
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    return h ? `${h}h ${m}m` : `${m}m`;
  }

  function fmtDate(at) {
    const d = new Date(at);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  }

  const MONTHS = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"];
  const WEEKDAYS = ["S", "M", "T", "W", "T", "F", "S"];

  /**
   * The bar chart for the period: one bar a day for 7 / 30 days, one a month
   * for the year and for all time (one a year past three years).
   * @returns {{label: string, tip: string, ms: number}[]}
   */
  function buckets(s, id) {
    if (!s) return [];
    const now = Date.now();
    const p = PERIODS.find((p) => p.id === id);
    // A young history (under two months) reads better a day at a time, even
    // for "all time": one month-wide bar says nothing.
    const young = id === "all" && s.first_at && dayStart(now) - dayStart(s.first_at) < 60 * DAY;
    if ((p && p.days && p.days <= 30) || young) {
      const first = young ? dayStart(s.first_at) : sinceFor(id);
      const n = young ? Math.round((dayStart(now) - first) / DAY) + 1 : p?.days ?? 1;
      const out = Array.from({ length: n }, (_, i) => {
        const d = new Date(first + i * DAY + DAY / 2);
        return {
          // every day for a week; every 5th day, counting back from today, for more
          label: n <= 7 ? WEEKDAYS[d.getDay()] : (n - 1 - i) % 5 === 0 ? String(d.getDate()) : "",
          tip: fmtDate(d.getTime()),
          ms: 0,
        };
      });
      for (const [at, ms] of s.timeline) {
        // by calendar day, not 24 h steps (daylight saving makes some 23 / 25)
        const i = Math.round((dayStart(at) - first) / DAY);
        if (out[i]) out[i].ms += ms;
      }
      return out;
    }
    // monthly (or yearly for a long history)
    const start = new Date(id === "all" ? s.first_at || now : sinceFor(id));
    const end = new Date(now);
    const months = (end.getFullYear() - start.getFullYear()) * 12 + end.getMonth() - start.getMonth() + 1;
    if (months > 36) {
      const y0 = start.getFullYear();
      const out = Array.from({ length: end.getFullYear() - y0 + 1 }, (_, i) => ({
        label: String(y0 + i).slice(2),
        tip: String(y0 + i),
        ms: 0,
      }));
      for (const [at, ms] of s.timeline) {
        const o = out[new Date(at).getFullYear() - y0];
        if (o) o.ms += ms;
      }
      return out;
    }
    const y0 = start.getFullYear();
    const m0 = start.getMonth();
    const out = Array.from({ length: Math.max(months, 1) }, (_, i) => {
      const d = new Date(y0, m0 + i, 1);
      return {
        label: MONTHS[d.getMonth()],
        tip: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`,
        ms: 0,
      };
    });
    for (const [at, ms] of s.timeline) {
      const d = new Date(at);
      const o = out[(d.getFullYear() - y0) * 12 + d.getMonth() - m0];
      if (o) o.ms += ms;
    }
    return out;
  }

  /** Listening time per hour of the day (0-23). */
  function hours(s) {
    const out = new Array(24).fill(0);
    if (s) for (const [at, ms] of s.timeline) out[new Date(at).getHours()] += ms;
    return out;
  }

  /** Days in a row with some listening, ending today (or yesterday, so the
   *  streak doesn't read 0 in the morning before the first song). */
  function streak(timeline) {
    const days = new Set(timeline.map(([at]) => dayStart(at)));
    let d = dayStart(Date.now());
    if (!days.has(d)) d = dayStart(d - DAY / 2);
    let n = 0;
    while (days.has(d)) {
      n++;
      d = dayStart(d - DAY / 2);
    }
    return n;
  }

  const chart = $derived(buckets(stats, period));
  const chartMax = $derived(Math.max(1, ...chart.map((b) => b.ms)));
  const hourly = $derived(hours(stats));
  const hourMax = $derived(Math.max(1, ...hourly));
  const peakHour = $derived(hourly.indexOf(Math.max(...hourly)));
  const streakDays = $derived(streak(allTimeline));
  /** @param {string} uri */
  const trackUrl = (uri) => `https://open.spotify.com/track/${uri.split(":").pop()}`;
  /** @param {string} uri */
  const isSpotify = (uri) => uri.startsWith("spotify:track:");

  /** The top list as rows; `uri` only for a Spotify song (double-click plays it). */
  const topList = $derived(
    listTab === "songs"
      ? (stats?.top_tracks ?? []).map((t) => {
          const label = `${t.artist ? `${t.artist} - ` : ""}${t.title || "Unknown"}`;
          const uri = isSpotify(t.uri) ? t.uri : null;
          return {
            label,
            plays: t.plays,
            uri,
            tip: `${label} · ${fmtDuration(t.ms)}${uri ? " · double-click to play" : ""}`,
          };
        })
      : (stats?.top_artists ?? []).map((a) => ({
          label: a.name,
          plays: a.plays,
          uri: null,
          tip: `${a.name} · ${fmtDuration(a.ms)}`,
        })),
  );
  const topMax = $derived(Math.max(1, ...topList.map((t) => t.plays)));

  /**
   * Badges, from all the listening ever noted (whatever period is picked), in
   * groups, from first steps to long-term goals, so there's always a next one.
   * @typedef {{group: string, glyph: string, name: string, desc: string, value: number, goal: number, unit: string, done: boolean}} Badge
   * @returns {Badge[]}
   */
  function badgeList(/** @type {Stats | null} */ all, /** @type {number} */ loved) {
    const tl = all?.timeline ?? [];
    const hours = (all?.ms ?? 0) / 3_600_000;
    const plays = all?.plays ?? 0;
    /** @type {Map<number, number>} listening time per day */
    const dayMs = new Map();
    /** @type {Map<number, number>} songs per day */
    const daySongs = new Map();
    let night = 0;
    let early = 0;
    let lunch = 0;
    let llamaDay = 0;
    let newYear = 0;
    let halloween = 0;
    for (const [at, ms] of tl) {
      const d = dayStart(at);
      dayMs.set(d, (dayMs.get(d) ?? 0) + ms);
      daySongs.set(d, (daySongs.get(d) ?? 0) + 1);
      const when = new Date(at);
      const h = when.getHours();
      if (h < 5) night = 1;
      else if (h < 8) early = 1;
      else if (h >= 12 && h < 14) lunch = 1;
      const md = `${when.getMonth() + 1}-${when.getDate()}`;
      if (md === "4-21") llamaDay = 1;
      else if (md === "1-1") newYear = 1;
      else if (md === "10-31") halloween = 1;
    }
    const bestDayHours = Math.max(0, ...dayMs.values()) / 3_600_000;
    const bestDaySongs = Math.max(0, ...daySongs.values());
    const sortedDays = [...dayMs.keys()].sort((a, b) => a - b);
    const hasDay = (/** @type {number} */ d) => dayMs.has(dayStart(d));
    // the longest run of days in a row with some listening
    let longest = 0;
    let run = 0;
    /** @type {number | null} */
    let prev = null;
    for (const d of sortedDays) {
      run = prev !== null && Math.round((d - prev) / DAY) === 1 ? run + 1 : 1;
      longest = Math.max(longest, run);
      prev = d;
    }
    // a Saturday and the Sunday after it; Monday to Friday of one week
    let weekend = 0;
    let workweek = 0;
    for (const d of sortedDays) {
      const wd = new Date(d).getDay();
      if (wd === 6 && hasDay(d + DAY + DAY / 2)) weekend = 1;
      if (wd === 1 && [1, 2, 3, 4].every((k) => hasDay(d + k * DAY + DAY / 2))) workweek = 1;
    }
    const artists = all?.artists ?? 0;
    const tracks = all?.tracks ?? 0;
    const topSong = all?.top_tracks?.[0]?.plays ?? 0;
    const topArtist = all?.top_artists?.[0]?.plays ?? 0;
    const local = all?.local_plays ?? 0;
    /** @type {string} */
    let group = "";
    /** @returns {Badge} */
    const b = (glyph, name, desc, value, goal, unit = "") => ({
      group, glyph, name, desc, value: Math.min(value, goal), goal, unit, done: value >= goal,
    });
    /** @type {Badge[]} */
    const out = [];
    const section = (/** @type {string} */ g, /** @type {() => Badge[]} */ make) => {
      group = g;
      out.push(...make());
    };
    section("LISTENING TIME", () => [
      b("◷", "Warming up", "One hour of music", hours, 1, " h"),
      b("◷", "Ten hours", "Ten hours of music", hours, 10, " h"),
      b("◎", "A full day", "24 hours of music", hours, 24, " h"),
      b("◎", "Fifty", "50 hours of music", hours, 50, " h"),
      b("★", "Century", "100 hours of music", hours, 100, " h"),
      b("★", "Devoted", "250 hours of music", hours, 250, " h"),
      b("★", "Lifer", "500 hours of music", hours, 500, " h"),
      b("♛", "Llama legend", "1000 hours of music. It really whips.", hours, 1000, " h"),
    ]);
    section("SONGS PLAYED", () => [
      b("♫", "First spin", "Play your first song", plays, 1),
      b("♫", "Mixtape", "Ten songs played", plays, 10),
      b("♫", "Hundred", "100 songs played", plays, 100),
      b("♫", "Jukebox", "500 songs played", plays, 500),
      b("♫", "Thousand", "1000 songs played", plays, 1000),
      b("♛", "Radio station", "5000 songs played", plays, 5000),
    ]);
    section("DAYS IN A ROW", () => [
      b("✦", "On a roll", "Music three days in a row", longest, 3, " days"),
      b("✦", "Every day", "Music seven days in a row", longest, 7, " days"),
      b("✦", "Fortnight", "Music 14 days in a row", longest, 14, " days"),
      b("✦", "Habit", "Music 30 days in a row", longest, 30, " days"),
      b("♛", "Unstoppable", "Music 100 days in a row", longest, 100, " days"),
    ]);
    section("IN ONE DAY", () => [
      b("»", "Marathon", "Three hours of music in one day", bestDayHours, 3, " h"),
      b("»", "Ultra marathon", "Six hours of music in one day", bestDayHours, 6, " h"),
      b("»", "Binge", "50 songs in one day", bestDaySongs, 50),
    ]);
    section("VARIETY", () => [
      b("♪", "Curious", "Ten different artists", artists, 10),
      b("♪", "Explorer", "50 different artists", artists, 50),
      b("♪", "Globetrotter", "100 different artists", artists, 100),
      b("♪", "Encyclopedia", "250 different artists", artists, 250),
      b("♬", "Collection", "50 different songs", tracks, 50),
      b("♬", "Crate digger", "250 different songs", tracks, 250),
      b("♬", "Record store", "500 different songs", tracks, 500),
      b("♬", "Library card", "1000 different songs", tracks, 1000),
    ]);
    section("FAVOURITES", () => [
      b("↻", "On repeat", "One song played ten times", topSong, 10),
      b("↻", "Earworm", "One song played 25 times", topSong, 25),
      b("↻", "Anthem", "One song played 100 times", topSong, 100),
      b("♥", "Big fan", "50 plays of one artist", topArtist, 50),
      b("♥", "Superfan", "200 plays of one artist", topArtist, 200),
      b("♡", "First love", "Love a song (F)", loved, 1),
      b("♡", "Collector", "Love ten songs", loved, 10),
      b("♡", "Heart full", "Love 50 songs", loved, 50),
    ]);
    section("WHEN YOU LISTEN", () => [
      b("☾", "Night owl", "A song between midnight and 5 am", night, 1),
      b("☼", "Early bird", "A song between 5 and 8 in the morning", early, 1),
      b("◐", "Lunch break", "A song between noon and 2 pm", lunch, 1),
      b("⌂", "Weekend", "Music on a Saturday and the Sunday after", weekend, 1),
      b("▤", "Nine to five", "Music every weekday of one week", workweek, 1),
    ]);
    section("SPECIAL DAYS", () => [
      b("♛", "Llama day", "Music on April 21, Winamp's birthday", llamaDay, 1),
      b("✶", "New year", "Music on January 1", newYear, 1),
      b("✧", "Spooky", "Music on Halloween", halloween, 1),
    ]);
    section("YOUR OWN MUSIC", () => [
      b("▣", "Old school", "Play one of your own music files", local, 1),
      b("▣", "Home collection", "100 plays of your own files", local, 100),
    ]);
    return out;
  }
  const badges = $derived(badgeList(allStats, lovedCount));
  const badgesDone = $derived(badges.filter((x) => x.done).length);
  /** "3.4/10 h", "12/50" */
  function progress(/** @type {Badge} */ x) {
    const v = x.unit === " h" ? Math.floor(x.value * 10) / 10 : Math.floor(x.value);
    return `${v}/${x.goal}${x.unit}`;
  }

  // REWIND: this calendar year as a few big slides, one fact each, from the
  // same history. Click or the arrow keys step through; COPY takes the slide.
  const DAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  const MONTH_NAMES = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

  /** @param {number} h the hour most listening happens around */
  function persona(h) {
    if (h >= 5 && h < 11) return { name: "EARLY BIRD", line: "the music comes on in the morning" };
    if (h >= 11 && h < 17) return { name: "DAY SHIFT", line: "the music keeps you going through the day" };
    if (h >= 17 && h < 22) return { name: "EVENING SPINNER", line: "evenings are when the music comes on" };
    return { name: "NIGHT OWL", line: "the music really starts after dark" };
  }

  /** @param {Stats | null} s the year's stats */
  function rewindFacts(s) {
    if (!s?.plays) return null;
    /** @type {Map<number, number>} listening time per day */
    const perDay = new Map();
    const weekday = new Array(7).fill(0);
    const months = new Array(12).fill(0);
    let firstAt = Infinity;
    for (const [at, ms] of s.timeline) {
      firstAt = Math.min(firstAt, at);
      const d = dayStart(at);
      perDay.set(d, (perDay.get(d) ?? 0) + ms);
      const dt = new Date(at);
      weekday[dt.getDay()] += ms;
      months[dt.getMonth()] += ms;
    }
    let bigDay = 0;
    let bigMs = 0;
    for (const [d, ms] of perDay) {
      if (ms > bigMs) {
        bigDay = d;
        bigMs = ms;
      }
    }
    // the longest run of days in a row with some listening
    let longest = 0;
    let run = 0;
    let prev = 0;
    for (const d of [...perDay.keys()].sort((a, b) => a - b)) {
      run = prev && dayStart(prev + DAY * 1.5) === d ? run + 1 : 1;
      longest = Math.max(longest, run);
      prev = d;
    }
    const byHour = hours(s);
    const peak = byHour.indexOf(Math.max(...byHour));
    return {
      firstAt,
      minutes: Math.round(s.ms / 60000),
      activeDays: perDay.size,
      bigDay,
      bigMs,
      longest,
      peak,
      persona: persona(peak),
      weekday: weekday.indexOf(Math.max(...weekday)),
      // up to this month, or all twelve once the year is over
      months: YEAR < new Date().getFullYear() ? months : months.slice(0, new Date().getMonth() + 1),
      topMonth: months.indexOf(Math.max(...months)),
    };
  }
  const rewind = $derived(period === "rewind" && REWIND_OPEN ? rewindFacts(stats) : null);
  const slides = $derived(
    rewind && stats
      ? [
          "intro",
          "time",
          ...(stats.top_artists.length ? ["artist"] : []),
          ...(stats.top_tracks.length ? ["song"] : []),
          "clock",
          "days",
          "months",
          "summary",
        ]
      : [],
  );
  const monthMax = $derived(Math.max(1, ...(rewind?.months ?? [])));
  let slide = $state(0);
  /** @param {number} d */
  function step(d) {
    slide = Math.min(Math.max(slide + d, 0), Math.max(slides.length - 1, 0));
  }
  function openRewind() {
    slide = 0;
    period = "rewind";
  }
  onMount(() => {
    // Arrow keys step the slides. On the document, so it runs before the
    // shortcut forwarding on the window, which then leaves the key alone.
    /** @param {KeyboardEvent} e */
    const onKey = (e) => {
      if (!rewind || e.ctrlKey || e.altKey) return;
      if (e.key === "ArrowRight") step(1);
      else if (e.key === "ArrowLeft") step(-1);
      else return;
      e.preventDefault();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  // re-measure the scroll handle's range when the list or the window changes
  $effect(() => {
    listTab;
    topList.length;
    REACTIVE_WINDOW_SIZE.height;
    tick().then(syncScroll);
  });

  /** @param {string | null} uri */
  function playTrack(uri) {
    if (uri) emitWindowEvent("playerWindow", { UrlsDropped: [trackUrl(uri)] });
  }
  /** Replace the playlist with the period's top songs, most played first. */
  function playTop() {
    const urls = (stats?.top_tracks ?? []).filter((t) => isSpotify(t.uri)).map((t) => trackUrl(t.uri));
    if (urls.length) emitWindowEvent("playerWindow", { UrlsDropped: urls });
  }

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(360, 460);
    // Reopen at the size it was last left (falls back to the default above).
    invoke("get_window_inner_size", { label: "stats" })
      .then((s) => {
        if (s) REACTIVE_WINDOW_SIZE.setSize(s.width, s.height);
      })
      .catch(() => {});

    // New plays land while it's open or hidden: freshen on each show, and
    // every minute while it's up.
    /** @type {() => void} */
    let unlisten = () => {};
    getCurrentWindow()
      .listen("statsShown", () => refresh())
      .then((u) => (unlisten = u));
    const timer = setInterval(() => {
      if (!document.hidden) refresh();
    }, 60_000);
    return () => {
      unlisten();
      clearInterval(timer);
    };
  });

  const close = () => invoke("set_stats_window_visible", { visible: false });

  // COPY: this window as a picture (skin and all), with a small Spotiamp+
  // strip under it, straight to the clipboard for Discord and the like.
  let copyLabel = $state("COPY");
  let capturing = $state(false);
  async function buildCard() {
    capturing = true; // hides the COPY button itself
    await tick();
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    let shot;
    try {
      const png = /** @type {ArrayBuffer} */ (await invoke("capture_window_png", { label: "stats" }));
      shot = await createImageBitmap(new Blob([png], { type: "image/png" }));
    } finally {
      capturing = false;
    }
    // the capture is in screen pixels; size the strip to match
    const px = (window.devicePixelRatio || 1) * (REACTIVE_WINDOW_SIZE.zoom || 1);
    const strip = Math.round(22 * px);
    const canvas = document.createElement("canvas");
    canvas.width = shot.width;
    canvas.height = shot.height + strip;
    const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
    const css = getComputedStyle(/** @type {Element} */ (document.querySelector(".st")));
    ctx.fillStyle = css.getPropertyValue("--frame").trim() || "#1a1a2a";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(shot, 0, 0);
    ctx.fillStyle = css.getPropertyValue("--fg").trim() || "#00ff41";
    ctx.textBaseline = "middle";
    ctx.textAlign = "center";
    const p = PERIODS.find((p) => p.id === period);
    const text = rewind ? `SPOTIAMP+  ·  REWIND ${YEAR}` : `SPOTIAMP+  ·  MY LISTENING, ${p?.label ?? ""}`;
    // shrink the line until it fits a narrow window
    let size = Math.round(14 * px);
    do {
      ctx.font = `${size}px "px sans nouveaux", sans-serif`;
    } while (ctx.measureText(text).width > canvas.width - 12 * px && --size > 6);
    ctx.fillText(text, canvas.width / 2, shot.height + strip / 2);
    return await new Promise((resolve, reject) =>
      canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("no image"))), "image/png"),
    );
  }
  async function copyCard() {
    window.focus();
    try {
      // a promise, so the clipboard write starts inside the click
      await navigator.clipboard.write([new ClipboardItem({ "image/png": buildCard() })]);
      copyLabel = "COPIED";
    } catch (e) {
      copyLabel = "FAILED";
      invoke("log_frontend_error", { window: "stats", message: `stats card: ${e}` }).catch(() => {});
    }
    setTimeout(() => (copyLabel = "COPY"), 1800);
  }

  function makeStatsDraggable(element) {
    makeDockedDraggable(element, "stats", "statsWindow");
  }

  // Resize from the bottom-right corner, like the other plugin windows.
  function makeStatsResizable(element) {
    makeSnappingResizer(
      element,
      "stats",
      (e) => {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        return {
          width: Math.max(Math.round(e.clientX / zoom) + 3, 275),
          height: Math.max(Math.round(e.clientY / zoom) + 3, 300),
        };
      },
      ({ width, height }) => REACTIVE_WINDOW_SIZE.setSize(Math.round(width), Math.round(height)),
      () => REACTIVE_WINDOW_SIZE.zoom || 1,
    );
  }
</script>

<div class="st">
  <div class="st-titlebar" use:makeStatsDraggable>
    <div class="st-tl"></div>
    <span class="st-title">LISTENING STATS</span>
    <button class="st-close" data-no-drag onclick={close} aria-label="Close"></button>
  </div>

  <div class="st-body">
    <div class="st-tabs">
      {#each PERIODS as p}
        <button class="st-tab" class:on={period === p.id} onclick={() => (period = p.id)}>
          {p.label}
        </button>
      {/each}
      <button
        class="st-tab"
        class:on={period === "rewind"}
        onclick={openRewind}
        title="your {YEAR} so far, one big fact at a time"
      >
        REWIND
      </button>
      {#if stats?.plays && !(period === "rewind" && !rewind)}
        <button
          class="st-play"
          style:visibility={capturing ? "hidden" : "visible"}
          onclick={copyCard}
          title="copy these stats as a picture, to paste into Discord or anywhere"
        >
          {copyLabel}
        </button>
      {/if}
    </div>

    {#if period === "rewind" && !REWIND_OPEN}
      {@const daysLeft = Math.round((new Date(YEAR, 11, 1).getTime() - dayStart(Date.now())) / DAY)}
      <div class="rw still">
        <div class="rw-k">YOUR YEAR IN SPOTIAMP+</div>
        <div class="rw-big">REWIND {YEAR}</div>
        <div class="rw-sub">opens on December 1<br />keep listening, it's made from your plays</div>
        <div class="rw-gap"></div>
        <div class="rw-big">{daysLeft}</div>
        <div class="rw-k">{daysLeft === 1 ? "DAY TO GO" : "DAYS TO GO"}</div>
      </div>
    {:else if loading && !stats}
      <div class="st-msg">Loading…</div>
    {:else if !stats || !stats.plays}
      <div class="st-msg">
        {#if allTimeline.length}
          Nothing played in this period yet.
        {:else}
          No listening history yet.<br />
          A song counts after 30 seconds of playing.
        {/if}
      </div>
    {:else if rewind}
      {@const a0 = stats.top_artists[0]}
      {@const t0 = stats.top_tracks[0]}
      {@const hh = String(rewind.peak).padStart(2, "0")}
      <div class="rw" role="button" tabindex="-1" onclick={() => step(1)} onkeydown={() => {}}>
        {#if slides[slide] === "intro"}
          <div class="rw-k">YOUR YEAR IN SPOTIAMP+</div>
          <div class="rw-big">REWIND {YEAR}</div>
          <div class="rw-sub">
            {stats.plays.toLocaleString("en-US")} plays since {fmtDate(rewind.firstAt)}<br />
            click, or use the arrow keys
          </div>
        {:else if slides[slide] === "time"}
          <div class="rw-k">YOU LISTENED FOR</div>
          <div class="rw-big">{rewind.minutes.toLocaleString("en-US")}</div>
          <div class="rw-k">MINUTES</div>
          <div class="rw-sub">
            that's {fmtDuration(stats.ms)} over {rewind.activeDays} days<br />
            {stats.tracks} songs · {stats.artists} artists
          </div>
        {:else if slides[slide] === "artist"}
          <div class="rw-k">YOUR #1 ARTIST</div>
          <div class="rw-name" title={a0.name}>{a0.name}</div>
          <div class="rw-sub">{a0.plays} plays · {fmtDuration(a0.ms)}</div>
          <ol class="rw-list">
            {#each stats.top_artists.slice(1, 5) as a, i}
              <li><span>{i + 2}. {a.name}</span><span>{a.plays}×</span></li>
            {/each}
          </ol>
        {:else if slides[slide] === "song"}
          <div class="rw-k">YOUR #1 SONG</div>
          <div class="rw-name" title={t0.title}>{t0.title || "Unknown"}</div>
          <div class="rw-sub">{t0.artist ? `${t0.artist} · ` : ""}{t0.plays} plays</div>
          <ol class="rw-list">
            {#each stats.top_tracks.slice(1, 5) as t, i}
              <li><span>{i + 2}. {t.title || "Unknown"}</span><span>{t.plays}×</span></li>
            {/each}
          </ol>
        {:else if slides[slide] === "clock"}
          <div class="rw-k">YOU ARE A</div>
          <div class="rw-name">{rewind.persona.name}</div>
          <div class="rw-sub">
            {rewind.persona.line}, mostly around {hh}:00<br />
            your day of the week: {DAY_NAMES[rewind.weekday]}
          </div>
          <div class="st-hours rw-wide">
            {#each hourly as ms, h}
              <div
                class="st-hbar"
                class:peak={h === rewind.peak}
                style:height="{Math.max(ms ? 8 : 0, (ms / hourMax) * 100)}%"
              ></div>
            {/each}
          </div>
        {:else if slides[slide] === "days"}
          <div class="rw-k">YOUR BIGGEST DAY</div>
          <div class="rw-name">{fmtDate(rewind.bigDay)}</div>
          <div class="rw-sub">{fmtDuration(rewind.bigMs)} of music in one day</div>
          <div class="rw-gap"></div>
          <div class="rw-k">LONGEST STREAK</div>
          <div class="rw-big">{rewind.longest}</div>
          <div class="rw-k">{rewind.longest === 1 ? "DAY" : "DAYS IN A ROW"}</div>
        {:else if slides[slide] === "months"}
          <div class="rw-k">YOUR TOP MONTH</div>
          <div class="rw-name">{MONTH_NAMES[rewind.topMonth]}</div>
          <div class="rw-sub">{fmtDuration(rewind.months[rewind.topMonth])} that month</div>
          <div class="st-chart rw-wide" style:--n={rewind.months.length}>
            {#each rewind.months as ms, m}
              <div class="st-col">
                <div class="st-bar" style:height="{(ms / monthMax) * 100}%"></div>
                <div class="st-lbl">{MONTHS[m]}</div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="rw-k">REWIND {YEAR}</div>
          <dl class="rw-sum">
            <dt>MINUTES</dt><dd>{rewind.minutes.toLocaleString("en-US")}</dd>
            {#if a0}<dt>TOP ARTIST</dt><dd>{a0.name}</dd>{/if}
            {#if t0}<dt>TOP SONG</dt><dd>{t0.artist ? `${t0.artist} - ` : ""}{t0.title || "Unknown"}</dd>{/if}
            <dt>YOU ARE A</dt><dd>{rewind.persona.name}</dd>
            <dt>BIGGEST DAY</dt><dd>{fmtDate(rewind.bigDay)}</dd>
            <dt>STREAK</dt><dd>{rewind.longest} {rewind.longest === 1 ? "day" : "days"}</dd>
            <dt>BADGES</dt><dd>{badgesDone}/{badges.length}</dd>
          </dl>
        {/if}
      </div>
      <div class="rw-nav" style:visibility={capturing ? "hidden" : "visible"}>
        <button class="st-tab" onclick={() => step(-1)} disabled={slide === 0}>&lt; BACK</button>
        <span class="rw-dots">{slide + 1}/{slides.length}</span>
        <button class="st-tab" onclick={() => step(1)} disabled={slide >= slides.length - 1}>NEXT &gt;</button>
      </div>
    {:else}
      <div class="st-totals">
        <div class="st-big" title="time listened in this period">{fmtDuration(stats.ms)}</div>
        <div class="st-nums">
          <span><b>{stats.plays}</b> plays</span>
          <span><b>{stats.tracks}</b> songs</span>
          <span><b>{stats.artists}</b> artists</span>
          {#if streakDays > 1}
            <span title="days in a row with some listening"><b>{streakDays}</b> day streak</span>
          {/if}
        </div>
      </div>

      <div class="st-chart" style:--n={chart.length}>
        {#each chart as b}
          <div class="st-col" title="{b.tip}: {fmtDuration(b.ms)}">
            <div class="st-bar" style:height="{(b.ms / chartMax) * 100}%"></div>
            <div class="st-lbl">{b.label}</div>
          </div>
        {/each}
      </div>

      <div class="st-hours" title="when you listen (hour of the day)">
        {#each hourly as ms, h}
          <div
            class="st-hbar"
            class:peak={h === peakHour}
            style:height="{Math.max(ms ? 8 : 0, (ms / hourMax) * 100)}%"
            title="{String(h).padStart(2, '0')}:00  {fmtDuration(ms)}"
          ></div>
        {/each}
      </div>
      <div class="st-hourcap">
        <span>00</span><span>MOSTLY AROUND {String(peakHour).padStart(2, "0")}:00</span><span>23</span>
      </div>

      <div class="st-listhead">
        <button class="st-tab" class:on={listTab === "songs"} onclick={() => (listTab = "songs")}>TOP SONGS</button>
        <button class="st-tab" class:on={listTab === "artists"} onclick={() => (listTab = "artists")}>TOP ARTISTS</button>
        <button
          class="st-tab"
          class:on={listTab === "badges"}
          onclick={() => (listTab = "badges")}
          title="badges for all your listening in Spotiamp+"
        >
          BADGES {badgesDone}/{badges.length}
        </button>
        {#if listTab === "songs"}
          <button class="st-play" onclick={playTop} title="put these songs in the playlist, most played first">PLAY THESE</button>
        {/if}
      </div>
      <div class="st-listwrap">
      <div class="st-list" bind:this={listEl} onscroll={syncScroll}>
        {#if listTab === "badges"}
          {#each badges as x, i}
            {#if i === 0 || badges[i - 1].group !== x.group}
              <div class="st-group">
                {x.group}
                <span>{badges.filter((y) => y.group === x.group && y.done).length}/{badges.filter((y) => y.group === x.group).length}</span>
              </div>
            {/if}
            <div class="st-row st-badge" class:done={x.done} title="{x.name}: {x.desc}">
              <div class="st-fill" style:width="{(x.value / x.goal) * 100}%"></div>
              <span class="st-rank">{x.glyph}</span>
              <span class="st-name">{x.name} <span class="st-desc">· {x.desc}</span></span>
              <span class="st-count">{x.done ? "✓" : progress(x)}</span>
            </div>
          {/each}
        {:else}
        {#each topList as t, i}
          <div
            class="st-row"
            role="button"
            tabindex="-1"
            ondblclick={() => playTrack(t.uri)}
            title={t.tip}
          >
            <div class="st-fill" style:width="{(t.plays / topMax) * 100}%"></div>
            <span class="st-rank">{i + 1}.</span>
            <span class="st-name">{t.label}</span>
            <span class="st-count">{t.plays}×</span>
          </div>
        {/each}
        {/if}
      </div>
      <!-- Winamp's own scroll handle (the skin's PLEDIT sprite), like the Library's -->
      <input
        type="range"
        class="st-scroll"
        min="0"
        max={scrollMax}
        step="1"
        value={scrollPos}
        oninput={(e) => listEl && (listEl.scrollTop = +e.currentTarget.value)}
        aria-label="Scroll the list"
      />
      </div>

      <div class="st-foot">
        since {fmtDate(stats.first_at)} · kept on this PC only
      </div>
    {/if}
  </div>

  <div class="st-resize" use:makeStatsResizable></div>
</div>

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
    overflow: hidden;
    background: #000;
  }

  .st {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* Same skin-following plugin frame as the library / lyrics / art windows. */
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    --fg: var(--skin-genexitemfg, var(--skin-plnormal, #00ff41));
    --hi: var(--skin-plcurrent, #fff);
    --bg: var(--skin-genexitembg, var(--skin-plbg, #000));
    --sel: var(--skin-genexselbg, var(--skin-plselbg, #0000c6));
    background: var(--frame);
    box-sizing: border-box;
    padding: 0 2px 2px;
    border: 1px solid var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    box-shadow:
      inset 1px 1px 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset 2px 0 0 color-mix(in srgb, var(--frame) 72%, #fff),
      inset -2px -2px 0 var(--skin-genexdivider, color-mix(in srgb, var(--frame) 50%, #000));
    user-select: none;
    font-family: "px sans nouveaux", sans-serif;
    font-size: 7px;
    -webkit-font-smoothing: none;
  }

  .st-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill) repeat-x;
    cursor: default;
  }
  .st-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl) no-repeat;
  }
  .st-title {
    position: absolute;
    left: 50%;
    top: 0;
    transform: translateX(-50%);
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    line-height: 1;
    padding: 0 10px 5px;
    letter-spacing: 1px;
    white-space: nowrap;
    color: var(--skin-titletext, var(--skin-genexhdrtext, #cdd6ea));
    background: var(--skin-gentitle, transparent) repeat-x;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.55);
    z-index: 1;
  }
  .st-close {
    position: absolute;
    right: 0;
    top: 0;
    width: 15px;
    height: 20px;
    background: var(--skin-gentr) no-repeat;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 2;
  }

  .st-body {
    flex: 1;
    min-height: 0;
    margin: 0 4px 4px;
    padding: 5px 6px 4px;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow: hidden;
  }

  button {
    font: inherit;
    -webkit-font-smoothing: none;
  }

  .st-tabs,
  .st-listhead {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    flex: 0 0 auto;
  }
  .st-tab,
  .st-play {
    background: transparent;
    color: color-mix(in srgb, var(--fg) 60%, transparent);
    border: 1px solid color-mix(in srgb, var(--fg) 25%, transparent);
    padding: 2px 5px 3px;
    letter-spacing: 0.6px;
    cursor: pointer;
  }
  .st-tab:hover,
  .st-play:hover {
    color: var(--fg);
  }
  .st-tab.on {
    background: var(--sel);
    color: var(--hi);
    border-color: var(--sel);
  }
  .st-play {
    margin-left: auto;
  }

  .st-msg {
    margin: auto;
    text-align: center;
    font-size: 11px;
    line-height: 1.5;
    font-style: italic;
    color: color-mix(in srgb, var(--fg) 55%, transparent);
  }

  .st-totals {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex: 0 0 auto;
  }
  .st-big {
    font-size: 21px;
    color: var(--hi);
    letter-spacing: 1px;
    /* "1h 15m" on one line, even in a narrow window */
    white-space: nowrap;
    flex: 0 0 auto;
  }
  .st-nums {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 9px;
    font-size: 7px;
    letter-spacing: 0.4px;
  }
  .st-nums b {
    font-weight: normal;
    color: var(--hi);
  }

  .st-chart {
    flex: 0 0 64px;
    display: grid;
    grid-template-columns: repeat(var(--n), 1fr);
    gap: 1px;
    align-items: end;
    border-bottom: 1px solid color-mix(in srgb, var(--fg) 30%, transparent);
  }
  .st-col {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    min-width: 0;
  }
  .st-bar {
    background: var(--fg);
    min-height: 0;
  }
  .st-col:hover .st-bar {
    background: var(--hi);
  }
  .st-lbl {
    flex: 0 0 9px;
    height: 9px;
    margin-bottom: -10px;
    text-align: center;
    overflow: visible;
    white-space: nowrap;
    color: color-mix(in srgb, var(--fg) 60%, transparent);
  }

  .st-hours {
    margin-top: 14px;
    flex: 0 0 22px;
    display: grid;
    grid-template-columns: repeat(24, 1fr);
    gap: 1px;
    align-items: end;
  }
  .st-hbar {
    background: color-mix(in srgb, var(--fg) 55%, transparent);
  }
  .st-hbar.peak {
    background: var(--hi);
  }
  .st-hourcap {
    display: flex;
    justify-content: space-between;
    flex: 0 0 auto;
    margin-top: -3px;
    color: color-mix(in srgb, var(--fg) 60%, transparent);
    letter-spacing: 0.4px;
  }

  .st-listwrap {
    flex: 1;
    min-height: 0;
    display: flex;
    border: 1px solid color-mix(in srgb, var(--fg) 20%, transparent);
  }
  .st-list {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .st-list::-webkit-scrollbar {
    display: none;
  }
  /* the same handle as the Library's list and the playlist */
  .st-scroll {
    cursor: url(/src/static/assets/skins/base-2.91/EQSLID.CUR), default;
    writing-mode: vertical-lr;
    direction: ltr;
    appearance: none;
    width: 10px;
    flex: 0 0 10px;
    margin: 0;
    background: var(--skin-genexwndbg, var(--bg));
    box-shadow: inset 1px 0 0 var(--skin-genexdivider, #0e1a0e);
  }
  .st-scroll::-webkit-slider-thumb {
    background: var(--skin-pledit);
    appearance: none;
    width: 8px;
    height: 18px;
    background-position: -52px -53px;
  }
  .st-scroll::-webkit-slider-thumb:active {
    background-position-x: -61px;
  }
  .st-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 13px;
    padding: 0 4px;
    white-space: nowrap;
    cursor: default;
  }
  .st-row:hover {
    color: var(--hi);
  }
  .st-fill {
    position: absolute;
    left: 0;
    top: 1px;
    bottom: 1px;
    background: color-mix(in srgb, var(--sel) 45%, transparent);
    pointer-events: none;
  }
  .st-rank,
  .st-name,
  .st-count {
    position: relative;
  }
  .st-rank {
    flex: 0 0 16px;
    text-align: right;
    color: color-mix(in srgb, var(--fg) 60%, transparent);
  }
  .st-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .st-count {
    flex: 0 0 auto;
    color: var(--hi);
  }
  /* badges: dim until earned */
  .st-badge {
    color: color-mix(in srgb, var(--fg) 55%, transparent);
  }
  .st-badge .st-count {
    color: inherit;
  }
  .st-badge.done {
    color: var(--fg);
  }
  .st-badge.done .st-rank,
  .st-badge.done .st-count {
    color: var(--hi);
  }
  .st-badge .st-rank {
    text-align: center;
  }
  .st-desc {
    color: color-mix(in srgb, var(--fg) 55%, transparent);
  }
  /* a badge group's title, with how many of it are earned */
  .st-group {
    display: flex;
    justify-content: space-between;
    padding: 5px 4px 2px;
    letter-spacing: 0.8px;
    color: var(--hi);
    border-bottom: 1px solid color-mix(in srgb, var(--fg) 20%, transparent);
  }
  .st-group span {
    color: color-mix(in srgb, var(--fg) 60%, transparent);
  }

  .st-foot {
    flex: 0 0 auto;
    text-align: center;
    letter-spacing: 0.4px;
    color: color-mix(in srgb, var(--fg) 45%, transparent);
  }

  /* bottom-right resize grip */
  .st-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    z-index: 3;
    background: linear-gradient(
      135deg,
      transparent 0 8px,
      color-mix(in srgb, var(--fg) 45%, transparent) 8px 9px,
      transparent 9px 11px,
      color-mix(in srgb, var(--fg) 45%, transparent) 11px 12px,
      transparent 12px
    );
  }

  /* REWIND slides: one big fact, centred, in sizes the pixel font stays crisp at */
  .rw {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
    text-align: center;
    overflow: hidden;
    cursor: pointer;
    outline: none;
  }
  .rw.still {
    cursor: default;
  }
  .rw-k {
    letter-spacing: 1px;
    color: color-mix(in srgb, var(--fg) 70%, transparent);
  }
  .rw-big {
    font-size: 28px;
    line-height: 1;
    color: var(--hi);
    letter-spacing: 1px;
    white-space: nowrap;
  }
  .rw-name {
    font-size: 14px;
    color: var(--hi);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rw-sub {
    line-height: 1.7;
    letter-spacing: 0.4px;
  }
  .rw-gap {
    height: 8px;
  }
  .rw-list {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    width: 100%;
    max-width: 280px;
  }
  .rw-list li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 0;
    border-top: 1px solid color-mix(in srgb, var(--fg) 15%, transparent);
  }
  .rw-list li span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rw-wide {
    width: 100%;
    max-width: 280px;
    margin: 6px 0 12px;
  }
  .rw-sum {
    width: 100%;
    max-width: 280px;
    margin: 4px 0 0;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 5px 10px;
    text-align: left;
  }
  .rw-sum dt {
    color: color-mix(in srgb, var(--fg) 60%, transparent);
    letter-spacing: 0.6px;
  }
  .rw-sum dd {
    margin: 0;
    color: var(--hi);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rw-nav {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .rw-nav .st-tab:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .rw-dots {
    letter-spacing: 1px;
  }
</style>
