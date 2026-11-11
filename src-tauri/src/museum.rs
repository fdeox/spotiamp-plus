//! In-app Winamp Skin Museum (skins.webamp.org): browse, search and put on
//! any of the museum's reviewed classic skins without leaving Spotiamp+.
//!
//! Uses the museum's public GraphQL API, with the same queries the Discord
//! bot's /skin commands were checked against (sort: MUSEUM is the museum's own
//! "classics first" order; filter: APPROVED keeps random picks to the ~12,700
//! skins a person reviewed). The chosen .wsz is downloaded only from the
//! museum's own hosts, over HTTPS, size-capped, and then goes through the
//! same loader as a skin opened from disk (which only ever writes the known
//! sprite files, by name).

use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use oauth2::reqwest;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{app_window, settings::InnerWindowSize, settings::Settings};

// Two columns of previews at about the skins' own size (275 px wide), so
// they're crisp rather than shrunk.
const WINDOW_SIZE: InnerWindowSize = InnerWindowSize {
    width: 600,
    height: 620,
};

/// Open the museum window (centred; it's a browser you visit, not part of
/// the docked group).
#[tauri::command]
pub async fn show_museum(app_handle: AppHandle) -> Result<(), ()> {
    let window = match app_handle.get_webview_window("museum") {
        Some(window) => window,
        None => {
            let window = app_window::build_frameless_window(
                &app_handle,
                "museum",
                "Skin Museum - Spotiamp+",
                "museum",
                WINDOW_SIZE,
            )
            .map_err(|_| ())?;
            let _ = window.center();
            app_window::own_by_player(&app_handle, &window);
            window
        }
    };
    window.show().map_err(|_| ())?;
    window.set_focus().map_err(|_| ())?;
    Ok(())
}

#[tauri::command]
pub fn close_museum(app_handle: AppHandle) {
    if let Some(window) = app_handle.get_webview_window("museum") {
        let _ = window.destroy();
    }
}

const API: &str = "https://api.webamp.org/graphql";
const FIELDS: &str = "filename md5 nsfw screenshot_url download_url museum_url";
/// Live `skins(filter: APPROVED) { count }` when this was written.
const APPROVED_TOTAL: u32 = 12694;
const PAGE: u32 = 24;
const MAX_WSZ_BYTES: usize = 8 * 1024 * 1024;
/// Where skins may come from. download_url points at the museum's CDN.
const SKIN_HOSTS: &[&str] = &["r2.webampskins.org", "cdn.webampskins.org", "skins.webamp.org"];
/// A modern skin's picture: the museum's own worker reads it out of the
/// file (Spotiamp+ reads it itself, see shot_protocol).
const SHOT_HOSTS: &[&str] = &["r2.webampskins.org", "cdn.webampskins.org", "skins.webamp.org", "zip-worker.jordan1320.workers.dev"];
/// What the museum says about a modern skin (it has no museum page of its own).
const MODERN_FIELDS: &str = "filename md5 nsfw screenshot_url download_url";
/// Modern skins in the museum when this was written (a random page is
/// picked from these).
const MODERN_TOTAL: u32 = 3143;
const MAX_WAL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Serialize, serde::Deserialize, Clone, Hash)]
pub struct MuseumSkin {
    name: String,
    md5: String,
    /// "" for a modern skin the museum has no picture of
    screenshot: String,
    download: String,
    page: Option<String>,
    /// Winamp 5's modern skin (.wal), worn in the modern skin window
    #[serde(default)]
    modern: bool,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        // The CDN answers directly; a redirect could lead anywhere.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("Spotiamp+/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("couldn't set up the connection ({e})"))
}

async fn gql(query: &str, variables: serde_json::Value) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({ "query": query, "variables": variables }).to_string();
    let res = client()?
        .post(API)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|_| "Couldn't reach the Skin Museum. Check your connection and try again.".to_string())?;
    if !res.status().is_success() {
        return Err(format!("The Skin Museum answered {}.", res.status()));
    }
    let text = res.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if let Some(message) = json["errors"].get(0).and_then(|e| e["message"].as_str()) {
        return Err(format!("The Skin Museum said: {message}"));
    }
    Ok(json["data"].clone())
}

/// Leave out what can't be shown: NSFW skins and the half-indexed ones
/// without a screenshot.
fn usable(nodes: &serde_json::Value) -> Vec<MuseumSkin> {
    nodes
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|n| !n["nsfw"].as_bool().unwrap_or(false))
                .filter_map(|n| {
                    Some(MuseumSkin {
                        name: n["filename"]
                            .as_str()
                            .unwrap_or("skin")
                            .trim_end_matches(".wsz")
                            .trim_end_matches(".zip")
                            .replace('_', " "),
                        md5: n["md5"].as_str()?.to_string(),
                        screenshot: n["screenshot_url"].as_str()?.to_string(),
                        download: n["download_url"].as_str()?.to_string(),
                        page: n["museum_url"].as_str().map(str::to_string),
                        modern: false,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The modern skins in a list of nodes (a search mixes in classic ones):
/// not NSFW, a .wal or .zip each. `pictured`: only those the museum has a
/// picture of, for the lists one browses.
fn usable_modern(nodes: &serde_json::Value, pictured: bool) -> Vec<MuseumSkin> {
    nodes
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|n| n["__typename"].as_str().is_none_or(|t| t == "ModernSkin"))
                .filter(|n| !n["nsfw"].as_bool().unwrap_or(false))
                .filter(|n| !pictured || n["screenshot_url"].as_str().is_some())
                .filter_map(|n| {
                    let file = n["filename"].as_str().unwrap_or("skin");
                    let lower = file.to_ascii_lowercase();
                    if !(lower.ends_with(".wal") || lower.ends_with(".zip")) {
                        return None;
                    }
                    Some(MuseumSkin {
                        name: file[..file.len() - 4].replace('_', " "),
                        md5: n["md5"].as_str()?.to_string(),
                        screenshot: n["screenshot_url"].as_str().unwrap_or("").to_string(),
                        download: n["download_url"].as_str()?.to_string(),
                        page: None,
                        modern: true,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A page of skins: "classics" (the museum's own order), "random" (reviewed
/// skins from a random spot) or "search" (forgives misspellings).
#[tauri::command]
pub async fn museum_skins(
    kind: String,
    query: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<MuseumSkin>, String> {
    let offset = offset.unwrap_or(0);
    match kind.as_str() {
        "classics" => {
            let data = gql(
                &format!("query($o: Int) {{ skins(first: {PAGE}, offset: $o, sort: MUSEUM) {{ nodes {{ ... on ClassicSkin {{ {FIELDS} }} }} }} }}"),
                serde_json::json!({ "o": offset }),
            )
            .await?;
            Ok(usable(&data["skins"]["nodes"]))
        }
        "random" => {
            let at = pseudo_random(APPROVED_TOTAL.saturating_sub(PAGE));
            let data = gql(
                &format!("query($o: Int) {{ skins(first: {PAGE}, offset: $o, filter: APPROVED) {{ nodes {{ ... on ClassicSkin {{ {FIELDS} }} }} }} }}"),
                serde_json::json!({ "o": at }),
            )
            .await?;
            Ok(usable(&data["skins"]["nodes"]))
        }
        "search" => {
            let q = query.unwrap_or_default();
            let q = q.trim();
            if q.is_empty() {
                return Ok(Vec::new());
            }
            let q: String = q.chars().take(80).collect();
            let data = gql(
                &format!("query($q: String!) {{ search_classic_skins(query: $q, first: 48) {{ {FIELDS} }} }}"),
                serde_json::json!({ "q": q }),
            )
            .await?;
            Ok(usable(&data["search_classic_skins"]))
        }
        // Winamp 5's modern skins: those with a picture, from a spot (more
        // in one go, as about half have none)
        "modern" | "modern-random" => {
            // (a random spot may have few with a picture, or none: a few
            // more spots then, till there's a dozen)
            let tries = if kind == "modern" { 1 } else { 4 };
            let mut found: Vec<MuseumSkin> = Vec::new();
            for _ in 0..tries {
                let at = if kind == "modern" { offset * 2 } else { pseudo_random(MODERN_TOTAL.saturating_sub(PAGE * 2)) };
                let data = gql(
                    &format!("query($o: Int) {{ modern_skins(first: {}, offset: $o) {{ nodes {{ {MODERN_FIELDS} }} }} }}", PAGE * 2),
                    serde_json::json!({ "o": at }),
                )
                .await?;
                for skin in usable_modern(&data["modern_skins"]["nodes"], true) {
                    if !found.iter().any(|f| f.md5 == skin.md5) {
                        found.push(skin);
                    }
                }
                if found.len() >= 12 {
                    break;
                }
            }
            Ok(found)
        }
        "modern-search" => {
            let q = query.unwrap_or_default();
            let q: String = q.trim().chars().take(80).collect();
            if q.is_empty() {
                return Ok(Vec::new());
            }
            let data = gql(
                &format!("query($q: String!) {{ search_skins(query: $q, first: 60) {{ __typename ... on ModernSkin {{ {MODERN_FIELDS} }} }} }}"),
                serde_json::json!({ "q": q }),
            )
            .await?;
            Ok(usable_modern(&data["search_skins"], false))
        }
        other => Err(format!("unknown list: {other}")),
    }
}

/// Skins starred (★) in the museum, newest first.
#[tauri::command(async)]
pub fn museum_favorites() -> Vec<MuseumSkin> {
    crate::settings::Settings::current()
        .favorite_skins
        .iter()
        .rev()
        .cloned()
        .collect()
}

/// Star or unstar a skin (kaool's idea). Only real museum skins: the same
/// id and host checks as putting one on, since these are saved and used later.
#[tauri::command(async)]
pub fn museum_set_favorite(skin: MuseumSkin, favorite: bool) -> Result<(), String> {
    if skin.md5.len() != 32 || !skin.md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("That skin's id looks wrong.".into());
    }
    let mut links = vec![(&skin.download, SKIN_HOSTS)];
    // a modern skin may have no picture
    if !(skin.modern && skin.screenshot.is_empty()) {
        links.push((&skin.screenshot, SHOT_HOSTS));
    }
    for (link, hosts) in links {
        let url = url::Url::parse(link).map_err(|_| "That skin's link looks wrong.".to_string())?;
        if url.scheme() != "https" || !hosts.contains(&url.host_str().unwrap_or("")) {
            return Err("Only Skin Museum skins can be favorites.".into());
        }
    }
    let mut settings = crate::settings::Settings::current_mut();
    settings.favorite_skins.retain(|s| s.md5 != skin.md5);
    if favorite {
        settings.favorite_skins.push(skin);
        // plenty, and keeps the settings file small
        let extra = settings.favorite_skins.len().saturating_sub(300);
        settings.favorite_skins.drain(..extra);
    }
    Ok(())
}

/// A museum skin's file: only from the museum, only so big.
async fn download_skin(md5: &str, download: &str, max: usize) -> Result<Vec<u8>, String> {
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("That skin's id looks wrong.".into());
    }
    let url = url::Url::parse(download).map_err(|_| "That download link looks wrong.".to_string())?;
    let host = url.host_str().unwrap_or("");
    if url.scheme() != "https" || !SKIN_HOSTS.contains(&host) {
        return Err(format!("Skins are only downloaded from the Skin Museum, not {host}."));
    }
    let mut res = client()?
        .get(url)
        .send()
        .await
        .map_err(|_| "Couldn't download the skin. Check your connection and try again.".to_string())?;
    if !res.status().is_success() {
        return Err(format!("The Skin Museum answered {}.", res.status()));
    }
    if res.content_length().unwrap_or(0) as usize > max {
        return Err("That skin file is too big.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > max {
            return Err("That skin file is too big.".into());
        }
    }
    Ok(bytes)
}

/// Download a modern museum skin (.wal) and wear it (modern_window.rs).
#[tauri::command]
pub async fn museum_apply_modern(app: AppHandle, md5: String, download: String, name: String) -> Result<(), String> {
    let bytes = download_skin(&md5, &download, MAX_WAL_BYTES).await?;
    crate::modern_window::wear_modern_skin(app, format!("{}.wal", name.trim()), bytes).await
}

/// Download a museum skin and put it on (like "load .wsz…" from disk).
#[tauri::command]
pub async fn museum_apply(md5: String, download: String) -> Result<(), String> {
    let bytes = download_skin(&md5, &download, MAX_WSZ_BYTES).await?;
    crate::wsz::apply_wsz_bytes(&bytes)?;
    Settings::current_mut().skin = "custom".to_string();
    Ok(())
}

/// Good enough to pick a page: no rand crate in the tree for this.
fn pseudo_random(below: u32) -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut x = (nanos as u64) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x % u64::from(below.max(1))) as u32
}

// ---- Modern skins' pictures ----------------------------------------------
//
// The museum's pictures of modern skins are files inside the skins, which its
// worker reads out (zip-worker…/zip/<md5>.wal/screenshot.png). That worker
// goes down at times (it answered 530 for days in November 2026), so the same
// picture is read here instead: the end of the skin's file (the zip's table of
// contents), then just that one file, with range requests to the museum's
// CDN. The museum window asks for it as museumshot://<md5>/<name>.

/// Pictures already read, by "<md5>/<name>".
static SHOTS: LazyLock<Mutex<HashMap<String, Arc<Vec<u8>>>>> = LazyLock::new(Default::default);
const MAX_SHOTS_KEPT: usize = 400;
const MAX_SHOT_BYTES: usize = 6 * 1024 * 1024;
/// The zip's last record: 22 bytes and a comment of up to 64 KiB.
const TAIL_BYTES: usize = 22 + 65_535;
const MAX_TABLE_BYTES: u64 = 4 * 1024 * 1024;

pub fn shot_protocol<R: tauri::Runtime>(
    _ctx: tauri::UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let path = request.uri().path().to_string();
    tauri::async_runtime::spawn(async move {
        let response = match modern_shot(&path).await {
            Ok(bytes) => tauri::http::Response::builder()
                .header(tauri::http::header::CONTENT_TYPE, image_type(&bytes).unwrap_or("image/png"))
                .header(tauri::http::header::CACHE_CONTROL, "max-age=604800")
                .body(bytes.to_vec()),
            Err(e) => {
                log::info!("museum picture {path}: {e}");
                tauri::http::Response::builder().status(404).body(Vec::new())
            }
        };
        responder.respond(response.unwrap_or_default());
    });
}

async fn modern_shot(path: &str) -> Result<Arc<Vec<u8>>, String> {
    let path = percent_decode(path).ok_or("bad path")?;
    let (md5, name) = path.trim_start_matches('/').split_once('/').ok_or("no picture named")?;
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) || !shot_name_ok(name) {
        return Err("bad path".into());
    }
    let key = format!("{md5}/{name}");
    if let Some(hit) = SHOTS.lock().ok().and_then(|shots| shots.get(&key).cloned()) {
        return Ok(hit);
    }
    let url = format!("https://r2.webampskins.org/skins/{md5}.wal");
    let client = client()?;
    let (tail_at, tail, total) = fetch_range(&client, &url, &format!("bytes=-{TAIL_BYTES}")).await?;
    let (table_at, table_len) = table_of_contents(&tail).ok_or("not a zip file")?;
    let table_end = table_at + table_len;
    if table_end > total || table_len > MAX_TABLE_BYTES {
        return Err("broken zip file".into());
    }
    let table = if table_at >= tail_at && table_end <= tail_at + tail.len() as u64 {
        tail[(table_at - tail_at) as usize..(table_end - tail_at) as usize].to_vec()
    } else {
        fetch_range(&client, &url, &format!("bytes={table_at}-{}", table_end - 1)).await?.1
    };
    let entry = find_entry(&table, name).ok_or("the skin has no such picture")?;
    if entry.compressed > MAX_SHOT_BYTES || entry.size > MAX_SHOT_BYTES {
        return Err("the picture is too big".into());
    }
    // The file's own header repeats its name and has an extra field of its
    // own (mostly as long as the table's): ask for that much and a bit more,
    // and for the rest if it was longer.
    let start = entry.local_at;
    let guess = (30 + entry.name_len + entry.extra_len + 256 + entry.compressed) as u64;
    if start >= total {
        return Err("broken zip file".into());
    }
    let (_, mut part, _) = fetch_range(&client, &url, &format!("bytes={start}-{}", (start + guess).min(total) - 1)).await?;
    if let Some(needed) = local_data_end(&part, &entry) {
        if part.len() < needed && start + needed as u64 <= total {
            let from = start + part.len() as u64;
            let (_, rest, _) = fetch_range(&client, &url, &format!("bytes={from}-{}", start + needed as u64 - 1)).await?;
            part.extend_from_slice(&rest);
        }
    }
    let bytes = Arc::new(read_entry(&part, &entry)?);
    if let Ok(mut shots) = SHOTS.lock() {
        if shots.len() >= MAX_SHOTS_KEPT {
            shots.clear();
        }
        shots.insert(key, bytes.clone());
    }
    Ok(bytes)
}

/// A range of the skin's file: (where it starts, its bytes, the file's size).
async fn fetch_range(client: &reqwest::Client, url: &str, range: &str) -> Result<(u64, Vec<u8>, u64), String> {
    let mut res = client
        .get(url)
        .header(reqwest::header::RANGE, range)
        .send()
        .await
        .map_err(|_| "couldn't reach the museum".to_string())?;
    if res.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("the museum answered {}", res.status()));
    }
    let (start, total) = res
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(content_range)
        .ok_or("no range in the answer")?;
    if total as usize > MAX_WAL_BYTES {
        return Err("the skin file is too big".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_SHOT_BYTES + TAIL_BYTES {
            return Err("the answer is too big".into());
        }
    }
    Ok((start, bytes, total))
}

/// "bytes 100-199/1000" → (100, 1000)
fn content_range(v: &str) -> Option<(u64, u64)> {
    let (span, total) = v.strip_prefix("bytes ")?.split_once('/')?;
    Some((span.split_once('-')?.0.trim().parse().ok()?, total.trim().parse().ok()?))
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A picture's name inside a skin, as the museum gives it: a plain path.
fn shot_name_ok(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= 200
        && !name.contains("..")
        && !name.starts_with('/')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || " _-./()[]&+',!".contains(c))
        && [".png", ".jpg", ".jpeg", ".gif", ".bmp"].iter().any(|ext| lower.ends_with(ext))
}

fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?)))
}

/// Where the zip's table of contents is and how long, from its last record
/// (somewhere in the file's last bytes, before an optional comment).
fn table_of_contents(tail: &[u8]) -> Option<(u64, u64)> {
    let last = tail.len().checked_sub(22)?;
    let at = (0..=last).rev().find(|&i| tail[i..i + 4] == [0x50, 0x4B, 0x05, 0x06])?;
    let (len, offset) = (u32_at(tail, at + 12)?, u32_at(tail, at + 16)?);
    // a zip64 file says 0xFFFFFFFF here; skins are never that big
    (offset != 0xFFFF_FFFF).then_some((offset, len))
}

#[derive(Debug, PartialEq)]
struct ZipEntry {
    method: usize,
    compressed: usize,
    size: usize,
    name_len: usize,
    extra_len: usize,
    local_at: u64,
}

/// A file in the table of contents by its path (any case), or else the
/// first one of that name in any folder.
fn find_entry(table: &[u8], name: &str) -> Option<ZipEntry> {
    let want = name.replace('\\', "/");
    let base = want.rsplit('/').next().unwrap_or(&want).to_string();
    let mut by_base = None;
    let mut p = 0;
    while table.get(p..p + 4) == Some(&[0x50, 0x4B, 0x01, 0x02]) {
        let (n, e, c) = (u16_at(table, p + 28)?, u16_at(table, p + 30)?, u16_at(table, p + 32)?);
        let path = String::from_utf8_lossy(table.get(p + 46..p + 46 + n)?).replace('\\', "/");
        let encrypted = u16_at(table, p + 8)? & 1 == 1;
        let entry = ZipEntry {
            method: u16_at(table, p + 10)?,
            compressed: u32_at(table, p + 20)? as usize,
            size: u32_at(table, p + 24)? as usize,
            name_len: n,
            extra_len: e,
            local_at: u32_at(table, p + 42)?,
        };
        if !encrypted {
            if path.eq_ignore_ascii_case(&want) {
                return Some(entry);
            }
            if by_base.is_none() && path.rsplit('/').next().is_some_and(|b| b.eq_ignore_ascii_case(&base)) {
                by_base = Some(entry);
            }
        }
        p += 46 + n + e + c;
    }
    by_base
}

/// Where a file's packed bytes end, counted from its own header.
fn local_data_end(part: &[u8], entry: &ZipEntry) -> Option<usize> {
    if part.get(..4)? != [0x50, 0x4B, 0x03, 0x04] {
        return None;
    }
    Some(30 + u16_at(part, 26)? + u16_at(part, 28)? + entry.compressed)
}

/// One file of a zip from its header on: unpacked, and a picture.
fn read_entry(part: &[u8], entry: &ZipEntry) -> Result<Vec<u8>, String> {
    let end = local_data_end(part, entry).ok_or("broken zip file")?;
    let raw = part.get(end - entry.compressed..end).ok_or("the picture came cut off")?;
    let bytes = match entry.method {
        0 => raw.to_vec(),
        8 => {
            let mut out = Vec::with_capacity(entry.size.min(MAX_SHOT_BYTES));
            flate2::read::DeflateDecoder::new(raw)
                .take(MAX_SHOT_BYTES as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|e| format!("couldn't unpack the picture ({e})"))?;
            if out.len() > MAX_SHOT_BYTES {
                return Err("the picture is too big".into());
            }
            out
        }
        m => return Err(format!("the picture is packed in a way not read here ({m})")),
    };
    if image_type(&bytes).is_none() {
        return Err("that file isn't a picture".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn zip_with(files: &[(&str, &[u8], bool)], comment: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes, packed) in files {
            let method = if *packed { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
            zip.start_file(*name, zip::write::SimpleFileOptions::default().compression_method(method)).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.set_comment(comment);
        zip.finish().unwrap().into_inner()
    }

    fn picture(n: usize) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend((0..n).map(|i| (i % 7) as u8));
        png
    }

    #[test]
    fn a_picture_is_read_out_of_a_skin() {
        let shot = picture(5000);
        for packed in [true, false] {
            let file = zip_with(
                &[("skin.xml", b"<WinampAbstractionLayer/>", true), ("Some Skin/screenshot.png", &shot, packed), ("x/README.txt", b"hi", false)],
                "made with a comment",
            );
            // the tail is the whole (small) file here, starting at 0
            let (at, len) = table_of_contents(&file).unwrap();
            let table = &file[at as usize..(at + len) as usize];
            // by its full path in any case, and by its name alone
            for name in ["Some Skin/SCREENSHOT.png", "screenshot.png"] {
                let entry = find_entry(table, name).unwrap();
                assert_eq!(entry.method, if packed { 8 } else { 0 });
                let part = &file[entry.local_at as usize..];
                assert_eq!(read_entry(part, &entry).unwrap(), shot);
                // a header and data cut short say so rather than read past
                let end = local_data_end(part, &entry).unwrap();
                assert!(read_entry(&part[..end - 1], &entry).is_err());
            }
            assert!(find_entry(table, "skinshot.png").is_none());
            // something that isn't a picture isn't served
            let readme = find_entry(table, "README.txt").unwrap();
            assert!(read_entry(&file[readme.local_at as usize..], &readme).is_err());
        }
    }

    #[test]
    fn picture_paths_are_checked() {
        assert_eq!(content_range("bytes 100-199/1000"), Some((100, 1000)));
        assert_eq!(content_range("bytes */1000"), None);
        assert_eq!(percent_decode("/abc%2Fscreen%20shot.png").as_deref(), Some("/abc/screen shot.png"));
        assert_eq!(percent_decode("/bad%2"), None);
        assert!(shot_name_ok("screenshot.png"));
        assert!(shot_name_ok("Big Bento/skinshot.PNG"));
        assert!(!shot_name_ok("../screenshot.png"));
        assert!(!shot_name_ok("/screenshot.png"));
        assert!(!shot_name_ok("skin.xml"));
        assert!(!shot_name_ok("a\\b.png"));
        assert_eq!(image_type(b"\x89PNG...."), Some("image/png"));
        assert_eq!(image_type(b"<xml>"), None);
    }
}
