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

use std::time::Duration;

use oauth2::reqwest;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{app_window, settings::InnerWindowSize, settings::Settings};

const WINDOW_SIZE: InnerWindowSize = InnerWindowSize {
    width: 480,
    height: 470,
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

#[derive(Debug, Serialize, serde::Deserialize, Clone, Hash)]
pub struct MuseumSkin {
    name: String,
    md5: String,
    screenshot: String,
    download: String,
    page: Option<String>,
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
    for link in [&skin.download, &skin.screenshot] {
        let url = url::Url::parse(link).map_err(|_| "That skin's link looks wrong.".to_string())?;
        if url.scheme() != "https" || !SKIN_HOSTS.contains(&url.host_str().unwrap_or("")) {
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

/// Download a museum skin and put it on (like "load .wsz…" from disk).
#[tauri::command]
pub async fn museum_apply(md5: String, download: String) -> Result<(), String> {
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("That skin's id looks wrong.".into());
    }
    let url = url::Url::parse(&download).map_err(|_| "That download link looks wrong.".to_string())?;
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
    if res.content_length().unwrap_or(0) as usize > MAX_WSZ_BYTES {
        return Err("That skin file is too big.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_WSZ_BYTES {
            return Err("That skin file is too big.".into());
        }
    }
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
