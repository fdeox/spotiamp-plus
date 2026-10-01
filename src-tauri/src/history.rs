//! Listening history, kept by Spotiamp+ itself (Spotify's own history isn't
//! readable with the access we have). It feeds the Library's "Recently
//! played" and "Most played", and later the listening stats.
//!
//! A play is written once it counts (30 s in, or half of a short track), as
//! one JSON line in `history.jsonl` in the config folder; if it then keeps
//! playing, a small follow-up line records the longer listening time. It
//! stays on this computer and never goes anywhere else.

use std::collections::HashMap;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::settings::get_config_dir;

/// Past this, the file is trimmed to its newest lines (years of listening).
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const KEEP_LINES: usize = 40_000;

#[derive(Serialize, Deserialize, Clone)]
pub struct Play {
    /// `spotify:track:…` or `local:<path>`
    pub uri: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    /// When the play started (epoch ms); also its id for follow-up lines.
    pub at: u64,
    /// How long it was listened to (ms).
    pub ms: u64,
}

/// A follow-up line: the play `at` went on to `ms` of listening.
#[derive(Serialize, Deserialize)]
struct Extend {
    id: u64,
    ms: u64,
}

#[derive(Serialize)]
pub struct HistoryItem {
    uri: String,
    /// last time it was played (epoch ms)
    at: u64,
    plays: u32,
}

fn path() -> Option<std::path::PathBuf> {
    get_config_dir().map(|d| d.join("history.jsonl"))
}

fn append(line: &str) -> Result<(), String> {
    let path = path().ok_or("no config dir")?;
    trim_if_big(&path);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

fn trim_if_big(path: &std::path::Path) {
    let Ok(meta) = std::fs::metadata(path) else { return };
    if meta.len() <= MAX_BYTES {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let lines: Vec<&str> = text.lines().collect();
    let keep = &lines[lines.len().saturating_sub(KEEP_LINES)..];
    let _ = std::fs::write(path, keep.join("\n") + "\n");
}

/// Every play, oldest first (follow-up lines skipped).
fn read_plays() -> Vec<Play> {
    let Some(path) = path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter_map(|l| serde_json::from_str::<Play>(l).ok())
        .collect()
}

/// Every play with its follow-up lines folded in, so `ms` is the whole
/// listening time. Oldest first.
fn read_plays_full() -> Vec<Play> {
    let Some(path) = path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut plays: Vec<Play> = Vec::new();
    let mut by_at: HashMap<u64, usize> = HashMap::new();
    for line in text.lines() {
        if let Ok(p) = serde_json::from_str::<Play>(line) {
            by_at.insert(p.at, plays.len());
            plays.push(p);
        } else if let Ok(e) = serde_json::from_str::<Extend>(line) {
            if let Some(&i) = by_at.get(&e.id) {
                plays[i].ms = plays[i].ms.max(e.ms);
            }
        }
    }
    plays
}

#[derive(Serialize)]
pub struct TopTrack {
    uri: String,
    title: String,
    artist: String,
    plays: u32,
    ms: u64,
}

#[derive(Serialize)]
pub struct TopArtist {
    name: String,
    plays: u32,
    ms: u64,
}

#[derive(Serialize)]
pub struct Stats {
    plays: u32,
    ms: u64,
    tracks: u32,
    artists: u32,
    /// first play ever (epoch ms), 0 if none; for "since …"
    first_at: u64,
    top_tracks: Vec<TopTrack>,
    top_artists: Vec<TopArtist>,
    /// plays of your own music files (not Spotify), for a badge
    local_plays: u32,
    /// `[at, ms]` of every play in the period, for the day / hour charts
    /// (bucketed in the window, which knows the local time zone).
    timeline: Vec<(u64, u64)>,
}

/// Listening stats for plays at or after `since` (epoch ms; 0 = all time) and,
/// when given, before `until` (Rewind's calendar year, looked at in January).
#[tauri::command(async)]
pub fn history_stats(since: u64, until: Option<u64>, limit: usize) -> Stats {
    let all = read_plays_full();
    let first_at = all.first().map(|p| p.at).unwrap_or(0);
    let plays: Vec<&Play> = all
        .iter()
        .filter(|p| p.at >= since && until.is_none_or(|u| p.at < u))
        .collect();

    let mut tracks: HashMap<&str, TopTrack> = HashMap::new();
    let mut artists: HashMap<String, TopArtist> = HashMap::new();
    let mut total_ms = 0;
    for p in &plays {
        total_ms += p.ms;
        let t = tracks.entry(p.uri.as_str()).or_insert_with(|| TopTrack {
            uri: p.uri.clone(),
            title: String::new(),
            artist: String::new(),
            plays: 0,
            ms: 0,
        });
        t.plays += 1;
        t.ms += p.ms;
        // the newest non-empty names win (tags or metadata can improve)
        if !p.title.is_empty() {
            t.title = p.title.clone();
        }
        if !p.artist.is_empty() {
            t.artist = p.artist.clone();
        }
        let name = p.artist.trim();
        if !name.is_empty() {
            // case-insensitive, so "AKON" and "Akon" from file tags meet
            let a = artists.entry(name.to_lowercase()).or_insert_with(|| TopArtist {
                name: name.to_string(),
                plays: 0,
                ms: 0,
            });
            a.plays += 1;
            a.ms += p.ms;
        }
    }

    let n_tracks = tracks.len() as u32;
    let n_artists = artists.len() as u32;
    let mut top_tracks: Vec<TopTrack> = tracks.into_values().collect();
    top_tracks.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.ms.cmp(&a.ms)));
    top_tracks.truncate(limit);
    let mut top_artists: Vec<TopArtist> = artists.into_values().collect();
    top_artists.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.ms.cmp(&a.ms)));
    top_artists.truncate(limit);

    Stats {
        plays: plays.len() as u32,
        ms: total_ms,
        tracks: n_tracks,
        artists: n_artists,
        first_at,
        top_tracks,
        top_artists,
        local_plays: plays.iter().filter(|p| p.uri.starts_with("local:")).count() as u32,
        timeline: plays.iter().map(|p| (p.at, p.ms)).collect(),
    }
}

#[tauri::command(async)]
pub fn history_add(entry: Play) -> Result<(), String> {
    if entry.uri.is_empty() {
        return Ok(());
    }
    append(&serde_json::to_string(&entry).map_err(|e| e.to_string())?)
}

#[tauri::command(async)]
pub fn history_extend(at: u64, ms: u64) -> Result<(), String> {
    append(&serde_json::to_string(&Extend { id: at, ms }).map_err(|e| e.to_string())?)
}

/// The most recently played tracks, newest first, each once.
#[tauri::command(async)]
pub fn history_recent(limit: usize) -> Vec<HistoryItem> {
    let plays = read_plays();
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for p in &plays {
        *counts.entry(p.uri.as_str()).or_default() += 1;
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in plays.iter().rev() {
        if out.len() >= limit {
            break;
        }
        if seen.insert(p.uri.as_str()) {
            out.push(HistoryItem {
                uri: p.uri.clone(),
                at: p.at,
                plays: counts[p.uri.as_str()],
            });
        }
    }
    out
}

/// The most played tracks, most first (ties: the more recent first).
#[tauri::command(async)]
pub fn history_top(limit: usize) -> Vec<HistoryItem> {
    let mut by_uri: HashMap<String, (u32, u64)> = HashMap::new();
    for p in read_plays() {
        let e = by_uri.entry(p.uri).or_insert((0, 0));
        e.0 += 1;
        e.1 = e.1.max(p.at);
    }
    let mut items: Vec<HistoryItem> = by_uri
        .into_iter()
        .map(|(uri, (plays, at))| HistoryItem { uri, at, plays })
        .collect();
    items.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.at.cmp(&a.at)));
    items.truncate(limit);
    items
}

#[tauri::command(async)]
pub fn history_clear() -> Result<(), String> {
    if let Some(path) = path() {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
