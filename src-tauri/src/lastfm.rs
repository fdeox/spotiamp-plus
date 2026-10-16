//! Last.fm (0.7.8): what you play goes on your Last.fm profile (scrobbling).
//! Spotiamp+ is registered with Last.fm as an app; its key and secret come in
//! at build time (SPOTIAMP_LASTFM_KEY / SPOTIAMP_LASTFM_SECRET, never in the
//! repo), and a build without them has no Last.fm. You sign in on Last.fm's
//! own page in the browser; what's kept here is the session it hands back
//! (config/lastfm.json), never a password. Scrobbles that can't go out (no
//! internet, Last.fm busy) wait in config/lastfm-queue.json for the next try.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use md5::{Digest, Md5};
use oauth2::reqwest;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::settings::get_config_dir;

const API: &str = "https://ws.audioscrobbler.com/2.0/";

/// Where the calls go: Last.fm, or for the smoke tests a stand-in of their
/// own (SPOTIAMP_LASTFM_API), so a test never touches anyone's profile.
fn api() -> String {
    std::env::var("SPOTIAMP_LASTFM_API").unwrap_or_else(|_| API.to_string())
}
/// Last.fm takes up to 50 scrobbles in one request.
const BATCH: usize = 50;
/// The most kept waiting (weeks offline); the oldest go first.
const QUEUE_MAX: usize = 1000;

fn credentials() -> Option<(&'static str, &'static str)> {
    match (option_env!("SPOTIAMP_LASTFM_KEY"), option_env!("SPOTIAMP_LASTFM_SECRET")) {
        (Some(key), Some(secret)) if !key.is_empty() && !secret.is_empty() => Some((key, secret)),
        _ => None,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Session {
    user: String,
    key: String,
    #[serde(default = "on")]
    scrobbling: bool,
}

fn on() -> bool {
    true
}

/// A song as it's sent: who, what, from which album, how long, when it began.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Play {
    pub artist: String,
    pub track: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration_ms: u32,
    /// When it started playing, epoch ms.
    pub started_at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LastfmStatus {
    /// This build has Last.fm at all.
    available: bool,
    user: Option<String>,
    scrobbling: bool,
    /// Scrobbles waiting to go out.
    waiting: usize,
}

fn session_path() -> Option<PathBuf> {
    get_config_dir().map(|d| d.join("lastfm.json"))
}

fn queue_path() -> Option<PathBuf> {
    get_config_dir().map(|d| d.join("lastfm-queue.json"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: Option<PathBuf>) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path?).ok()?).ok()
}

fn write_json<T: serde::Serialize>(path: Option<PathBuf>, value: &T) {
    let Some(path) = path else { return };
    let tmp = path.with_extension("json.tmp");
    let written = serde_json::to_vec(value)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&tmp, bytes).map_err(|e| e.to_string()))
        .and_then(|()| std::fs::rename(&tmp, &path).map_err(|e| e.to_string()));
    if let Err(e) = written {
        log::warn!("Last.fm: couldn't save ({e})");
    }
}

fn session() -> Option<Session> {
    read_json(session_path())
}

fn queue() -> Vec<Play> {
    read_json(queue_path()).unwrap_or_default()
}

/// Last.fm's signature: every parameter but `format`, name then value, in
/// name order, then the secret; its MD5 in hex.
fn signature(params: &BTreeMap<String, String>, secret: &str) -> String {
    let mut text = String::new();
    for (name, value) in params {
        if name != "format" {
            text.push_str(name);
            text.push_str(value);
        }
    }
    text.push_str(secret);
    Md5::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug)]
enum Failure {
    /// No answer (no internet): worth another try later.
    Offline,
    /// Last.fm said to come back later (busy, down, too many requests).
    Later,
    /// The session's gone (taken back on Last.fm's site): sign in again.
    SignedOut,
    /// Signing in: not allowed on Last.fm's page yet.
    NotYet,
    /// Turned down for good, with Last.fm's reason.
    Refused(String),
}

impl Failure {
    fn say(&self) -> String {
        match self {
            Failure::Offline => "Last.fm didn't answer (no internet?)".into(),
            Failure::Later => "Last.fm is busy, try again in a bit".into(),
            Failure::SignedOut => "Last.fm signed Spotiamp+ out".into(),
            Failure::NotYet => "not allowed on Last.fm yet".into(),
            Failure::Refused(why) => why.clone(),
        }
    }
}

/// One signed call. `post` for what changes something (scrobbles).
async fn call(method: &str, mut params: BTreeMap<String, String>, post: bool) -> Result<Value, Failure> {
    let (key, secret) = credentials().ok_or_else(|| Failure::Refused("this build has no Last.fm".into()))?;
    params.insert("method".into(), method.to_string());
    params.insert("api_key".into(), key.to_string());
    let sig = signature(&params, secret);
    params.insert("api_sig".into(), sig);
    params.insert("format".into(), "json".to_string());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("Spotiamp+/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| Failure::Offline)?;
    let api = api();
    let request = if post { client.post(&api).form(&params) } else { client.get(&api).query(&params) };
    let text = request
        .send()
        .await
        .map_err(|_| Failure::Offline)?
        .text()
        .await
        .map_err(|_| Failure::Offline)?;
    let value: Value = serde_json::from_str(&text).map_err(|_| Failure::Later)?;
    match value.get("error").and_then(Value::as_i64) {
        None => Ok(value),
        Some(9) => Err(Failure::SignedOut),
        Some(14) => Err(Failure::NotYet),
        // 8: "operation failed, try again" (Last.fm's backend had a moment)
        Some(8 | 11 | 16 | 29) => Err(Failure::Later),
        Some(code) => {
            let message = value.get("message").and_then(Value::as_str).unwrap_or("no reason given");
            log::warn!("Last.fm: {method} turned down ({code}: {message})");
            Err(Failure::Refused(format!("Last.fm: {message}")))
        }
    }
}

fn signed_out(app: &AppHandle) {
    if let Some(p) = session_path() {
        let _ = std::fs::remove_file(p);
    }
    log::warn!("Last.fm: the session was taken back; signed out");
    let _ = app.emit("lastfmChanged", ());
}

#[tauri::command]
pub fn lastfm_status() -> LastfmStatus {
    let session = session();
    LastfmStatus {
        available: credentials().is_some(),
        scrobbling: session.as_ref().is_some_and(|s| s.scrobbling),
        user: session.map(|s| s.user),
        waiting: queue().len(),
    }
}

/// Sign in: Last.fm's page opens in the browser, and once Spotiamp+ is
/// allowed there, the session is kept. Gives the Last.fm user name.
#[tauri::command]
pub async fn lastfm_connect(app_handle: AppHandle) -> Result<String, String> {
    let (key, _) = credentials().ok_or("this build has no Last.fm")?;
    let token = call("auth.getToken", BTreeMap::new(), false)
        .await
        .map_err(|e| e.say())?
        .get("token")
        .and_then(Value::as_str)
        .ok_or("Last.fm sent no token")?
        .to_string();
    // (a test's stand-in allows by itself: no browser then)
    if std::env::var_os("SPOTIAMP_LASTFM_API").is_none() {
        crate::open_in_browser(&format!("https://www.last.fm/api/auth/?api_key={key}&token={token}"));
    }
    // waiting for "Yes, allow access" (a token lasts an hour; three minutes is plenty)
    for _ in 0..90 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let params = BTreeMap::from([("token".to_string(), token.clone())]);
        match call("auth.getSession", params, false).await {
            Ok(answer) => {
                let s = &answer["session"];
                let (Some(user), Some(session_key)) = (s["name"].as_str(), s["key"].as_str()) else {
                    return Err("Last.fm sent no session".into());
                };
                write_json(session_path(), &Session { user: user.to_string(), key: session_key.to_string(), scrobbling: true });
                log::info!("Last.fm: signed in");
                let _ = app_handle.emit("lastfmChanged", ());
                return Ok(user.to_string());
            }
            Err(Failure::Refused(why)) => return Err(why),
            // not allowed yet, or a moment without internet: keep waiting
            Err(_) => continue,
        }
    }
    Err("Spotiamp+ wasn't allowed on Last.fm's page in time".into())
}

#[tauri::command]
pub fn lastfm_disconnect(app_handle: AppHandle) {
    for path in [session_path(), queue_path()].into_iter().flatten() {
        let _ = std::fs::remove_file(path);
    }
    log::info!("Last.fm: signed out");
    let _ = app_handle.emit("lastfmChanged", ());
}

#[tauri::command]
pub fn lastfm_set_scrobbling(on: bool, app_handle: AppHandle) {
    if let Some(mut s) = session() {
        s.scrobbling = on;
        write_json(session_path(), &s);
        let _ = app_handle.emit("lastfmChanged", ());
    }
}

/// Text as Last.fm should see it: no padding zeros or spaces around it.
fn clean(text: &str) -> String {
    text.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string()
}

fn song_params(play: &Play, index: Option<usize>) -> Vec<(String, String)> {
    let name = |n: &str| match index {
        Some(i) => format!("{n}[{i}]"),
        None => n.to_string(),
    };
    let mut out = vec![(name("artist"), clean(&play.artist)), (name("track"), clean(&play.track))];
    if !clean(&play.album).is_empty() {
        out.push((name("album"), clean(&play.album)));
    }
    if play.duration_ms > 0 {
        out.push((name("duration"), (play.duration_ms / 1000).to_string()));
    }
    if index.is_some() {
        out.push((name("timestamp"), (play.started_at / 1000).to_string()));
    }
    out
}

/// A song began: "now playing" on the profile. Quietly nothing when not
/// signed in or scrobbling is off.
#[tauri::command]
pub async fn lastfm_now_playing(play: Play, app_handle: AppHandle) {
    let Some(session) = session().filter(|s| s.scrobbling) else { return };
    if clean(&play.artist).is_empty() || clean(&play.track).is_empty() {
        return;
    }
    let mut params = BTreeMap::from([("sk".to_string(), session.key)]);
    params.extend(song_params(&play, None));
    if let Err(Failure::SignedOut) = call("track.updateNowPlaying", params, true).await {
        signed_out(&app_handle);
    }
}

/// A song played long enough (half of it, or 4 minutes) goes on the profile.
/// It's queued first, so one that can't go out now goes with the next.
#[tauri::command]
pub async fn lastfm_scrobble(play: Play, app_handle: AppHandle) {
    let Some(session) = session().filter(|s| s.scrobbling) else { return };
    if clean(&play.artist).is_empty() || clean(&play.track).is_empty() {
        return;
    }
    // one at a time, so a queue being sent isn't sent twice
    static SENDING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _sending = SENDING.lock().await;
    let mut waiting = queue();
    if waiting.last() != Some(&play) {
        waiting.push(play);
    }
    if waiting.len() > QUEUE_MAX {
        waiting.drain(..waiting.len() - QUEUE_MAX);
    }
    while !waiting.is_empty() {
        let batch: Vec<Play> = waiting.iter().take(BATCH).cloned().collect();
        let mut params = BTreeMap::from([("sk".to_string(), session.key.clone())]);
        for (i, p) in batch.iter().enumerate() {
            params.extend(song_params(p, Some(i)));
        }
        match call("track.scrobble", params, true).await {
            Ok(_) => {
                waiting.drain(..batch.len());
            }
            Err(Failure::SignedOut) => {
                signed_out(&app_handle);
                return;
            }
            Err(Failure::Refused(_)) => {
                // turned down for good (a song Last.fm won't take): don't keep it
                waiting.drain(..batch.len());
            }
            Err(Failure::Offline | Failure::Later | Failure::NotYet) => break,
        }
    }
    write_json(queue_path(), &waiting);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The signature as Last.fm documents it (names in order, `format` left out).
    #[test]
    fn calls_are_signed_the_way_last_fm_checks_them() {
        let params = BTreeMap::from([
            ("method".to_string(), "auth.getSession".to_string()),
            ("api_key".to_string(), "k".to_string()),
            ("token".to_string(), "t".to_string()),
            ("format".to_string(), "json".to_string()),
        ]);
        let expected: String = Md5::digest(b"api_keykmethodauth.getSessiontokentS").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(signature(&params, "S"), expected);
        // MD5 itself, on a known value
        assert_eq!(
            Md5::digest(b"abc").iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "900150983cd24fb0d6963f7d28e17f72"
        );
    }

    #[test]
    fn a_batch_names_each_song_by_its_place() {
        let play = Play { artist: "A".into(), track: "T".into(), album: "".into(), duration_ms: 185_400, started_at: 1_700_000_000_123 };
        let p = song_params(&play, Some(3));
        assert!(p.contains(&("artist[3]".into(), "A".into())));
        assert!(p.contains(&("duration[3]".into(), "185".into())));
        assert!(p.contains(&("timestamp[3]".into(), "1700000000".into())));
        assert!(!p.iter().any(|(n, _)| n.starts_with("album")));
        // now playing: plain names, no time
        let p = song_params(&play, None);
        assert!(p.contains(&("track".into(), "T".into())) && !p.iter().any(|(n, _)| n == "timestamp"));
    }
}
