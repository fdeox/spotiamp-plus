//! Winamp's playlist files: open a .m3u / .m3u8 into the playlist, or save
//! the playlist as one. Local files are written as their paths and Spotify
//! songs as open.spotify.com links, so Spotiamp+ reads its own files back
//! whole, and other players still get the local files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::local_player::AUDIO_EXTS;

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Entry {
    /// `spotify:track:…`
    Spotify(String),
    /// an audio file's absolute path
    Local(String),
}

#[derive(Serialize)]
pub struct Opened {
    entries: Vec<Entry>,
    /// lines that were songs we can't play: missing files, other streams
    skipped: usize,
}

/// One playlist row to save.
#[derive(Deserialize)]
pub struct SaveRow {
    /// `spotify:track:…` or a local path
    location: String,
    title: String,
    duration_ms: u64,
}

/// Text of a playlist file: UTF-8 (with or without a BOM), else the system
/// code page, which is what Winamp and friends wrote plain .m3u files in.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};
        unsafe {
            let n = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
            if n > 0 {
                let mut wide = vec![0u16; n as usize];
                MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut wide));
                return String::from_utf16_lossy(&wide);
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// A Spotify track uri from a line, if it is one (a `spotify:track:` uri or
/// an open.spotify.com track link, with or without `intl-xx/` and `?si=`).
fn spotify_track(line: &str) -> Option<String> {
    if let Some(id) = line.strip_prefix("spotify:track:") {
        return Some(format!("spotify:track:{id}"));
    }
    let url = url::Url::parse(line).ok()?;
    if url.host_str()? != "open.spotify.com" {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|s| !s.is_empty() && !s.starts_with("intl-"));
    match (parts.next(), parts.next()) {
        (Some("track"), Some(id)) if id.chars().all(|c| c.is_ascii_alphanumeric()) => {
            Some(format!("spotify:track:{id}"))
        }
        _ => None,
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn parse(text: &str, base: &Path) -> Opened {
    let mut entries = Vec::new();
    let mut skipped = 0;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(uri) = spotify_track(line) {
            entries.push(Entry::Spotify(uri));
            continue;
        }
        let path: PathBuf = if line.starts_with("file:") {
            match url::Url::parse(line).ok().and_then(|u| u.to_file_path().ok()) {
                Some(p) => p,
                None => {
                    skipped += 1;
                    continue;
                }
            }
        } else if line.contains("://") {
            // a web stream or another service: nothing we can play
            skipped += 1;
            continue;
        } else {
            // relative paths are relative to the playlist file
            base.join(line)
        };
        if path.is_file() && is_audio(&path) {
            entries.push(Entry::Local(path.to_string_lossy().into_owned()));
        } else {
            skipped += 1;
        }
    }
    Opened { entries, skipped }
}

/// Pick a .m3u / .m3u8 and read the songs in it, in order. None = cancelled.
#[tauri::command]
pub async fn m3u_open(app_handle: AppHandle) -> Result<Option<Opened>, String> {
    let Some(path) = app_handle
        .dialog()
        .file()
        .add_filter("Playlist", &["m3u8", "m3u"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
    else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the file ({e})"))?;
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok(Some(parse(&decode(&bytes), &base)))
}

/// Save the rows as an extended M3U (UTF-8, .m3u8). false = cancelled.
#[tauri::command]
pub async fn m3u_save(app_handle: AppHandle, rows: Vec<SaveRow>) -> Result<bool, String> {
    let Some(path) = app_handle
        .dialog()
        .file()
        .add_filter("Playlist (UTF-8)", &["m3u8"])
        .add_filter("Playlist", &["m3u"])
        .set_file_name("Spotiamp+ playlist.m3u8")
        .blocking_save_file()
        .and_then(|f| f.into_path().ok())
    else {
        return Ok(false);
    };
    let mut out = String::from("#EXTM3U\r\n");
    for row in rows {
        let location = match row.location.strip_prefix("spotify:track:") {
            Some(id) => format!("https://open.spotify.com/track/{id}"),
            None => row.location,
        };
        // a line break in a title would start a bogus entry
        let title = row.title.replace(['\r', '\n'], " ");
        let secs = if row.duration_ms > 0 { (row.duration_ms / 1000) as i64 } else { -1 };
        out.push_str(&format!("#EXTINF:{secs},{title}\r\n{location}\r\n"));
    }
    std::fs::write(&path, out).map_err(|e| format!("Could not write the file ({e})"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_links() {
        assert_eq!(spotify_track("spotify:track:abc123").as_deref(), Some("spotify:track:abc123"));
        assert_eq!(
            spotify_track("https://open.spotify.com/intl-tr/track/4k9pqSKBHYdTGjzNeRyQ0o?si=x").as_deref(),
            Some("spotify:track:4k9pqSKBHYdTGjzNeRyQ0o")
        );
        assert_eq!(spotify_track("https://open.spotify.com/album/abc"), None);
        assert_eq!(spotify_track("C:\\Music\\a.mp3"), None);
    }

    #[test]
    fn parses_mixed_file() {
        let dir = std::env::temp_dir().join("spotiamp-m3u-test");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("şarkı.mp3"), b"x").unwrap();
        let text = "#EXTM3U\n#EXTINF:10,x\nsub\\şarkı.mp3\nhttps://open.spotify.com/track/abc\nmissing.mp3\nhttp://radio/stream\n";
        let got = parse(text, &dir);
        assert_eq!(got.entries.len(), 2);
        assert!(matches!(&got.entries[0], Entry::Local(p) if p.ends_with("şarkı.mp3")));
        assert!(matches!(&got.entries[1], Entry::Spotify(u) if u == "spotify:track:abc"));
        assert_eq!(got.skipped, 2);
    }

    #[test]
    fn decodes_bom_and_utf8() {
        assert_eq!(decode(b"\xEF\xBB\xBFa.mp3"), "a.mp3");
        assert_eq!(decode("ç.mp3".as_bytes()), "ç.mp3");
    }
}
