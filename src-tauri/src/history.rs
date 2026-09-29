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

#[tauri::command]
pub fn history_add(entry: Play) -> Result<(), String> {
    if entry.uri.is_empty() {
        return Ok(());
    }
    append(&serde_json::to_string(&entry).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn history_extend(at: u64, ms: u64) -> Result<(), String> {
    append(&serde_json::to_string(&Extend { id: at, ms }).map_err(|e| e.to_string())?)
}

/// The most recently played tracks, newest first, each once.
#[tauri::command]
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
#[tauri::command]
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

#[tauri::command]
pub fn history_clear() -> Result<(), String> {
    if let Some(path) = path() {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
