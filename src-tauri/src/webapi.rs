//! The Spotify app's queue in Free Mode (0.8.0). Windows' media session only
//! names the song playing, so the songs coming up are asked of Spotify's Web
//! API, signed in as the account the app plays (on Spotify's own page, as
//! for Premium). Only the queue is read; nothing is changed.
//!
//! config/spotify-web.json keeps the access token, the refresh token that
//! renews it, and when it runs out. Signing out deletes it.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use oauth2::TokenResponse;
use serde::{Deserialize, Serialize};

use crate::settings::get_config_dir;

const CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

/// Spotify's address for `path`, or a test's stand-in (SPOTIAMP_WEBAPI), so
/// a test never needs a real sign-in.
fn url(real: &str, path: &str) -> String {
    match std::env::var("SPOTIAMP_WEBAPI") {
        Ok(base) => format!("{base}{path}"),
        Err(_) => format!("{real}{path}"),
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    access_token: String,
    refresh_token: Option<String>,
    /// Unix seconds
    expires_at: u64,
}

fn path() -> Option<PathBuf> {
    get_config_dir().map(|d| d.join("spotify-web.json"))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn load() -> Option<Saved> {
    serde_json::from_slice(&std::fs::read(path()?).ok()?).ok()
}

fn save(saved: &Saved) {
    let Some(path) = path() else { return };
    let tmp = path.with_extension("json.tmp");
    let written = serde_json::to_vec(saved)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&tmp, bytes).map_err(|e| e.to_string()))
        .and_then(|()| std::fs::rename(&tmp, &path).map_err(|e| e.to_string()));
    if let Err(e) = written {
        log::warn!("Spotify queue: couldn't keep the sign-in ({e})");
    }
}

fn forget() {
    if let Some(path) = path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Keep a sign-in's token for reading the queue.
pub(crate) fn keep_token(token: &crate::oauth::TokenType) {
    save(&Saved {
        access_token: token.access_token().secret().clone(),
        refresh_token: token.refresh_token().map(|r| r.secret().clone()),
        expires_at: now() + token.expires_in().map_or(3600, |d| d.as_secs()),
    });
}

fn client() -> Option<oauth2::reqwest::Client> {
    oauth2::reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().ok()
}

/// What asking for the queue came to.
enum Failed {
    /// No sign-in, or one Spotify no longer takes.
    SignIn,
    /// Spotify said no to this account (403).
    Refused,
    /// No answer, or one that couldn't be read.
    Other,
}

/// A token that works now: the kept one, or a fresh one from its refresh
/// token (Spotify may hand out a new refresh token with it).
async fn access_token() -> Result<String, Failed> {
    let saved = load().ok_or(Failed::SignIn)?;
    if saved.expires_at > now() + 60 {
        return Ok(saved.access_token);
    }
    let refresh = saved.refresh_token.ok_or(Failed::SignIn)?;
    let response = client()
        .ok_or(Failed::Other)?
        .post(url("https://accounts.spotify.com", "/api/token"))
        .form(&[("grant_type", "refresh_token"), ("refresh_token", refresh.as_str()), ("client_id", CLIENT_ID)])
        .send()
        .await
        .map_err(|_| Failed::Other)?;
    let status = response.status();
    if status.as_u16() == 400 || status.as_u16() == 401 {
        log::info!("Spotify queue: the sign-in ran out ({status}); signing in again is needed");
        forget();
        return Err(Failed::SignIn);
    }
    #[derive(Deserialize)]
    struct Fresh {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
    }
    let fresh: Fresh = serde_json::from_str(&response.text().await.map_err(|_| Failed::Other)?)
        .map_err(|_| Failed::Other)?;
    let token = fresh.access_token.clone();
    save(&Saved {
        access_token: fresh.access_token,
        refresh_token: fresh.refresh_token.or(Some(refresh)),
        expires_at: now() + fresh.expires_in.unwrap_or(3600),
    });
    Ok(token)
}

/// A song in the queue, as the playlist shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct QueueSong {
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
}

/// The queue: the song playing first, then what comes next. `state` is
/// "ok", "signin" (no sign-in yet), "refused" (Spotify won't show this
/// account's queue) or "error" (no answer; tried again later).
#[derive(Serialize, Default)]
pub struct FreeQueue {
    pub state: &'static str,
    pub songs: Vec<QueueSong>,
}

/// The Web API's answer, songs and podcast episodes alike.
fn parse_queue(body: &str) -> Option<Vec<QueueSong>> {
    #[derive(Deserialize)]
    struct Named {
        #[serde(default)]
        name: String,
    }
    #[derive(Deserialize)]
    struct Item {
        #[serde(default)]
        name: String,
        #[serde(default)]
        duration_ms: u64,
        #[serde(default)]
        artists: Vec<Named>,
        show: Option<Named>,
    }
    #[derive(Deserialize)]
    struct Answer {
        currently_playing: Option<Item>,
        #[serde(default)]
        queue: Vec<Option<Item>>,
    }
    let answer: Answer = serde_json::from_str(body).ok()?;
    let song = |item: Item| QueueSong {
        artist: match item.artists.first() {
            Some(first) => first.name.clone(),
            None => item.show.map(|s| s.name).unwrap_or_default(),
        },
        title: item.name,
        duration_ms: item.duration_ms,
    };
    Some(answer.currently_playing.into_iter().chain(answer.queue.into_iter().flatten()).map(song).collect())
}

async fn ask_queue() -> Result<Vec<QueueSong>, Failed> {
    let token = access_token().await?;
    let response = client()
        .ok_or(Failed::Other)?
        .get(url("https://api.spotify.com", "/v1/me/player/queue"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| Failed::Other)?;
    match response.status().as_u16() {
        200 => parse_queue(&response.text().await.map_err(|_| Failed::Other)?).ok_or(Failed::Other),
        204 => Ok(Vec::new()),
        401 => {
            // taken back before it ran out: renew it next time
            if let Some(mut saved) = load() {
                saved.expires_at = 0;
                save(&saved);
            }
            Err(Failed::Other)
        }
        403 => {
            let body = response.text().await.unwrap_or_default();
            log::info!("Spotify queue: refused for this account ({})", body.chars().take(200).collect::<String>());
            Err(Failed::Refused)
        }
        status => {
            log::info!("Spotify queue: answered {status}");
            Err(Failed::Other)
        }
    }
}

#[tauri::command]
pub async fn free_queue() -> FreeQueue {
    match ask_queue().await {
        Ok(songs) => FreeQueue { state: "ok", songs },
        Err(Failed::SignIn) => FreeQueue { state: "signin", songs: Vec::new() },
        Err(Failed::Refused) => FreeQueue { state: "refused", songs: Vec::new() },
        Err(Failed::Other) => FreeQueue { state: "error", songs: Vec::new() },
    }
}

/// Sign in on Spotify's page (the window Premium signs in with), for the queue.
#[tauri::command]
pub async fn free_queue_sign_in(app: tauri::AppHandle) -> Result<(), String> {
    let token = crate::spotify::SpotifySession::sign_in(&app).await.map_err(|e| e.to_string())?;
    keep_token(&token);
    Ok(())
}

#[tauri::command]
pub fn free_queue_sign_out() {
    forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_reads_songs_and_episodes() {
        let body = r#"{
            "currently_playing": {"name": "Song A", "duration_ms": 200000, "artists": [{"name": "First"}, {"name": "Second"}]},
            "queue": [
                {"name": "Song B", "duration_ms": 180500, "artists": [{"name": "Other"}]},
                {"name": "Episode 12", "duration_ms": 3600000, "show": {"name": "A Podcast"}},
                null
            ]
        }"#;
        let song = |title: &str, artist: &str, duration_ms| QueueSong { title: title.into(), artist: artist.into(), duration_ms };
        assert_eq!(
            parse_queue(body).unwrap(),
            vec![song("Song A", "First", 200000), song("Song B", "Other", 180500), song("Episode 12", "A Podcast", 3600000)]
        );
        assert_eq!(parse_queue(r#"{"currently_playing": null, "queue": []}"#).unwrap(), vec![]);
        assert!(parse_queue("not json").is_none());
    }
}
