// Listening badges, worked out from history_stats (all time) and the loved
// count. Shared by the Stats window (the BADGES list) and the llama, which
// celebrates when one more is done.

const DAY = 86_400_000;

/** Local midnight of the day `at` falls on. */
function dayStart(/** @type {number} */ at) {
  const d = new Date(at);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/**
 * @typedef {{uri: string, title: string, artist: string, plays: number, ms: number}} TopTrack
 * @typedef {{name: string, plays: number, ms: number}} TopArtist
 * @typedef {{plays: number, ms: number, tracks: number, artists: number, first_at: number,
 *   top_tracks: TopTrack[], top_artists: TopArtist[], local_plays: number, timeline: [number, number][]}} Stats
 */

/**
 * Badges, from all the listening ever noted (whatever period is picked), in
 * groups, from first steps to long-term goals, so there's always a next one.
 * @typedef {{group: string, glyph: string, name: string, desc: string, value: number, goal: number, unit: string, done: boolean}} Badge
 * @returns {Badge[]}
 */
export function badgeList(/** @type {Stats | null} */ all, /** @type {number} */ loved) {
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
