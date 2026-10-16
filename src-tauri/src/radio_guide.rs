//! The Library's Radio (0.7.8): stations to find and keep. The guide is
//! radio-browser.info, a free and open directory of tens of thousands of
//! stations. Only stations Spotiamp+ can really play are shown: ones the
//! directory found working at its last check, not HLS (streamed in pieces),
//! and in MP3, AAC or Ogg Vorbis (not Opus, which comes as "OGG" too). "Your
//! stations" are the ones that played here, newest first.

use oauth2::reqwest;
use serde_json::Value;

use crate::settings::Settings;

/// The directory's servers: the round-robin name first, then single mirrors.
const SERVERS: [&str; 4] = [
    "https://all.api.radio-browser.info",
    "https://de1.api.radio-browser.info",
    "https://de2.api.radio-browser.info",
    "https://fi1.api.radio-browser.info",
];
/// How many "your stations" are kept.
const KEEP: usize = 60;

/// The stations the guide showed this run, by link: when one of them plays,
/// its genre, country and id go with it into your stations.
fn shown() -> std::sync::MutexGuard<'static, std::collections::HashMap<String, Station>> {
    static SHOWN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Station>>> =
        std::sync::OnceLock::new();
    SHOWN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Station {
    pub url: String,
    pub name: String,
    /// Genres, comma separated.
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub bitrate: u32,
    /// The directory's id for it ("" for a link pasted with Ctrl+L).
    #[serde(default)]
    pub uuid: String,
}

/// Can Spotiamp+ play it, by what the directory says about it?
fn playable(s: &Value) -> bool {
    let text = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let num = |k: &str| s.get(k).and_then(Value::as_i64).unwrap_or(0);
    let url = text("url_resolved");
    let codec = text("codec").to_ascii_uppercase();
    let opus = format!("{} {}", text("name"), url).to_ascii_lowercase().contains("opus");
    num("lastcheckok") == 1
        && num("hls") == 0
        && (url.starts_with("http://") || url.starts_with("https://"))
        && match codec.as_str() {
            "MP3" | "AAC" | "AAC+" => true,
            "OGG" => !opus,
            _ => false,
        }
}

fn station(s: &Value) -> Station {
    let text = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    Station {
        url: text("url_resolved"),
        name: text("name"),
        tags: text("tags").replace(',', ", "),
        country: text("country"),
        codec: text("codec"),
        bitrate: s.get("bitrate").and_then(Value::as_u64).unwrap_or(0) as u32,
        uuid: text("stationuuid"),
    }
}

/// One query to the directory (`path` with its parameters), trying the next
/// server when one doesn't answer.
async fn ask(path: &str) -> Result<Vec<Value>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent(concat!("Spotiamp+/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    for server in SERVERS {
        let Ok(response) = client.get(format!("{server}{path}")).send().await else { continue };
        let Ok(text) = response.text().await else { continue };
        if let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&text) {
            return Ok(list);
        }
    }
    Err("the radio guide didn't answer (no internet?)".into())
}

/// What can be played of a list, most listened first, each stream once.
fn keep_playable(lists: Vec<Vec<Value>>, limit: usize) -> Vec<Station> {
    let mut all: Vec<&Value> = lists.iter().flatten().filter(|s| playable(s)).collect();
    all.sort_by_key(|s| std::cmp::Reverse(s.get("clickcount").and_then(Value::as_i64).unwrap_or(0)));
    let mut seen = std::collections::HashSet::new();
    let list: Vec<Station> = all
        .into_iter()
        .map(station)
        .filter(|s| seen.insert(s.url.clone()))
        .take(limit)
        .collect();
    let mut shown = shown();
    if shown.len() > 5000 {
        shown.clear();
    }
    for s in &list {
        shown.insert(s.url.clone(), s.clone());
    }
    list
}

fn encode(text: &str) -> String {
    oauth2::url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

const COMMON: &str = "hidebroken=true&order=clickcount&reverse=true&limit=200";

/// Stations by name or genre ("jazz", "Radio Swiss", "türkçe pop").
#[tauri::command]
pub async fn radio_search(query: String) -> Result<Vec<Station>, String> {
    let q = encode(query.trim());
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let (name_path, tag_path) = (
        format!("/json/stations/search?name={q}&{COMMON}"),
        format!("/json/stations/search?tag={q}&{COMMON}"),
    );
    let (by_name, by_tag) = tokio::join!(ask(&name_path), ask(&tag_path));
    match (by_name, by_tag) {
        (Err(e), Err(_)) => Err(e),
        (by_name, by_tag) => Ok(keep_playable(vec![by_name.unwrap_or_default(), by_tag.unwrap_or_default()], 100)),
    }
}

/// The most listened stations, everywhere or in one country (its two-letter code).
#[tauri::command]
pub async fn radio_popular(country: Option<String>) -> Result<Vec<Station>, String> {
    let path = match country.filter(|c| c.len() == 2) {
        Some(c) => format!("/json/stations/search?countrycode={}&{COMMON}", encode(&c)),
        None => format!("/json/stations/search?{COMMON}"),
    };
    Ok(keep_playable(vec![ask(&path).await?], 100))
}

/// Your stations: the ones that played here, newest first.
#[tauri::command]
pub fn radio_stations() -> Vec<Station> {
    Settings::current().radio_stations.clone()
}

/// A station is playing (the player says so once it really plays): to the
/// top of your stations. One the guide showed keeps the guide's name, genre
/// and country; one pasted with Ctrl+L goes by the name it gives itself.
#[tauri::command]
pub async fn radio_remember(station: Station) {
    if station.url.is_empty() {
        return;
    }
    let guide = shown().get(&station.url).cloned();
    let uuid = {
        let mut settings = Settings::current_mut();
        let list = &mut settings.radio_stations;
        let mut entry = match list.iter().position(|s| s.url == station.url) {
            Some(i) => list.remove(i),
            None => Station { url: station.url.clone(), ..station.clone() },
        };
        if let Some(g) = guide {
            entry = g;
        }
        if entry.name.is_empty() {
            entry.name = station.name.clone();
        }
        if entry.bitrate == 0 {
            entry.bitrate = station.bitrate;
        }
        let uuid = entry.uuid.clone();
        list.insert(0, entry);
        list.truncate(KEEP);
        uuid
    };
    // the directory counts what's played, to rank its stations
    if !uuid.is_empty() {
        let _ = ask(&format!("/json/url/{}", encode(&uuid))).await;
    }
}

/// The country set in Windows (Settings, Time & language, Region), as its
/// two letters: what "Popular in …" is for. Not the display language: an
/// English Windows in Turkey is still in Turkey.
#[tauri::command]
pub fn radio_home_country() -> Option<String> {
    #[cfg(windows)]
    {
        let mut name = [0u16; 16];
        let len = unsafe { windows::Win32::Globalization::GetUserDefaultGeoName(&mut name) };
        if len > 1 {
            let code = String::from_utf16_lossy(&name[..len as usize - 1]);
            if code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
                return Some(code.to_ascii_uppercase());
            }
        }
    }
    None
}

#[tauri::command]
pub fn radio_forget(url: String) {
    Settings::current_mut().radio_stations.retain(|s| s.url != url);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_what_plays_is_listed() {
        let s = |codec: &str, hls: i64, ok: i64, name: &str, url: &str, clicks: i64| {
            json!({ "codec": codec, "hls": hls, "lastcheckok": ok, "name": name, "url_resolved": url, "clickcount": clicks, "stationuuid": name })
        };
        let list = vec![
            s("MP3", 0, 1, "Jazz", "http://a/jazz", 5),
            s("MP3", 1, 1, "Pieces", "http://a/hls", 9),
            s("MP3", 0, 0, "Broken", "http://a/broken", 9),
            s("OGG", 0, 1, "Jazz HD Opus", "http://a/jazz_opus", 9),
            s("OGG", 0, 1, "Vorbis FM", "http://a/vorbis", 7),
            s("AAC+", 0, 1, "Plus", "http://a/plus", 6),
            s("UNKNOWN", 0, 1, "Mystery", "http://a/x", 9),
            s("AAC,H.264", 0, 1, "Video", "http://a/v", 9),
            s("MP3", 0, 1, "Jazz again", "http://a/jazz", 1),
            s("MP3", 0, 1, "Not a link", "rtsp://a/x", 9),
        ];
        let got: Vec<String> = keep_playable(vec![list], 10).into_iter().map(|s| s.name).collect();
        // most listened first, each stream once
        assert_eq!(got, ["Vorbis FM", "Plus", "Jazz"]);
    }
}
