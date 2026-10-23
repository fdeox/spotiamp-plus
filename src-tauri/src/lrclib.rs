//! Lyrics for your own songs and for radio songs (0.7.9), from LRCLIB
//! (lrclib.net), a free and open database of synced lyrics. Spotify songs keep
//! Spotify's own. What goes out is the song's artist and title, nothing else.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use oauth2::reqwest;
use serde_json::Value;

use crate::spotify::{LyricsData, LyricsLine};

const API: &str = "https://lrclib.net/api/search";

/// Asked once per song (an answer of none too), as the Lyrics window asks
/// again whenever the song comes back.
static FOUND: LazyLock<Mutex<HashMap<String, Option<LyricsData>>>> = LazyLock::new(Default::default);

/// Lyrics for `artist` - `title`; `duration_ms` (0 when unknown, as for a
/// radio song) picks the version that's the same length.
#[tauri::command]
pub async fn get_lyrics_lrclib(artist: String, title: String, duration_ms: u32) -> Result<LyricsData, String> {
    let key = format!("{}\u{1}{}\u{1}{}", artist.to_lowercase(), title.to_lowercase(), duration_ms / 1000);
    if let Some(found) = FOUND.lock().map_err(|_| "busy")?.get(&key) {
        return found.clone().ok_or_else(|| "no lyrics".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("Spotiamp+/", env!("CARGO_PKG_VERSION"), " (https://github.com/fdeox/spotiamp-plus)"))
        .build()
        .map_err(|e| e.to_string())?;
    let text = client
        .get(API)
        .query(&[("artist_name", artist.as_str()), ("track_name", title.as_str())])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    // an answer that isn't a list (LRCLIB down for a moment) isn't kept
    let Ok(Value::Array(results)) = serde_json::from_str::<Value>(&text) else {
        return Err("LRCLIB didn't answer".into());
    };
    let found = pick(&results, duration_ms).and_then(|(r, synced)| lyrics_of(r, synced));
    if let Ok(mut cache) = FOUND.lock() {
        cache.insert(key, found.clone());
    }
    found.ok_or_else(|| "no lyrics".into())
}

/// The version to use, and whether in time: with a known length, the synced
/// one closest to it (within 5 s); without, the length most versions share
/// (the original, usually: live and radio edits come once each). Otherwise
/// plain lyrics, not in time: another version's timing would be off.
fn pick(results: &[Value], duration_ms: u32) -> Option<(&Value, bool)> {
    let secs = |r: &Value| r.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    let synced: Vec<&Value> = results
        .iter()
        .filter(|r| r.get("syncedLyrics").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty()))
        .collect();
    if duration_ms > 0 {
        let want = duration_ms as f64 / 1000.0;
        let closest = synced
            .iter()
            .min_by(|a, b| (secs(a) - want).abs().total_cmp(&(secs(b) - want).abs()))
            .filter(|r| (secs(r) - want).abs() <= 5.0);
        if let Some(r) = closest {
            return Some((r, true));
        }
    } else if !synced.is_empty() {
        let count = |r: &Value| synced.iter().filter(|o| (secs(o) - secs(r)).abs() <= 2.0).count();
        // the first of the most shared (max_by_key would take the last)
        return synced.iter().min_by_key(|r| std::cmp::Reverse(count(r))).map(|r| (*r, true));
    }
    results
        .iter()
        .find(|r| r.get("plainLyrics").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty()))
        .map(|r| (r, false))
}

fn lyrics_of(r: &Value, synced: bool) -> Option<LyricsData> {
    let text = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("");
    let lines = if synced { parse_lrc(text("syncedLyrics")) } else { Vec::new() };
    if !lines.is_empty() {
        return Some(LyricsData { synced: true, provider: "LRCLIB".into(), lines });
    }
    let plain: Vec<LyricsLine> =
        text("plainLyrics").lines().map(|l| LyricsLine { time_ms: 0, text: l.trim().to_string() }).collect();
    (!plain.is_empty()).then(|| LyricsData { synced: false, provider: "LRCLIB".into(), lines: plain })
}

/// "[01:23.45] words" lines (a line can carry several times) into lines in
/// time order. Tags that aren't times ([ar:...]) are skipped.
fn parse_lrc(lrc: &str) -> Vec<LyricsLine> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(end) = rest.strip_prefix('[').and_then(|r| r.find(']')) {
            let tag = &rest[1..=end];
            match time_ms(tag) {
                Some(ms) => times.push(ms),
                None => break,
            }
            rest = rest[end + 2..].trim_start();
        }
        for ms in times {
            lines.push(LyricsLine { time_ms: ms, text: rest.trim().to_string() });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

/// "mm:ss.xx" (or .xxx, or no fraction) in ms.
fn time_ms(tag: &str) -> Option<u32> {
    let (min, sec) = tag.split_once(':')?;
    let min: u32 = min.trim().parse().ok()?;
    let (s, frac) = sec.split_once('.').unwrap_or((sec, "0"));
    let s: u32 = s.trim().parse().ok()?;
    let frac_ms = match frac.len() {
        1 => frac.parse::<u32>().ok()? * 100,
        2 => frac.parse::<u32>().ok()? * 10,
        _ => frac.get(..3)?.parse::<u32>().ok()?,
    };
    Some(min * 60_000 + s * 1000 + frac_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lrc_lines_in_time_order() {
        let lines = parse_lrc("[ar:Someone]\n[00:32.71] I'm holding on\n[00:10.5][01:02.003]Again\n[00:40]\nnot a line");
        let got: Vec<(u32, &str)> = lines.iter().map(|l| (l.time_ms, l.text.as_str())).collect();
        assert_eq!(got, [(10_500, "Again"), (32_710, "I'm holding on"), (40_000, ""), (62_003, "Again")]);
    }

    #[test]
    fn the_right_version() {
        let v = |d: f64, synced: bool| json!({ "duration": d, "syncedLyrics": if synced { "[00:01.00] x" } else { "" }, "plainLyrics": "x" });
        let results = vec![v(148.0, true), v(222.0, true), v(208.0, true), v(209.0, true), v(213.0, true)];
        let got = |r: Option<(&Value, bool)>| r.map(|(r, synced)| (r["duration"].as_f64().unwrap_or(0.0), synced));
        // a file of 3:28: that one, in time; of 3:00: none is close, so plain
        assert_eq!(got(pick(&results, 208_400)), Some((208.0, true)));
        assert_eq!(got(pick(&results, 180_000)), Some((148.0, false)));
        assert!(pick(&results, 180_000).and_then(|(r, s)| lyrics_of(r, s)).is_some_and(|l| !l.synced));
        // a radio song: the length most versions share
        assert_eq!(got(pick(&results, 0)), Some((208.0, true)));
        assert_eq!(got(pick(&[v(200.0, false)], 0)), Some((200.0, false)));
    }
}
