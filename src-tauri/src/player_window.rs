use librespot::{core::SpotifyUri, metadata::Track};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::{
    app_window, playlist_window,
    settings::{PlayerSettings, Settings},
    spotify::{SharedPlayer, UserPlaylist},
};

#[derive(Debug, Clone, Serialize)]
pub struct TrackMetadata {
    uri: String,
    artist: String,
    album: String,
    #[serde(rename = "albumArt")]
    album_art: Option<String>,
    name: String,
    duration: u32,
    unavailable: bool,
}
impl From<&Track> for TrackMetadata {
    fn from(track: &Track) -> Self {
        // first album cover → Spotify CDN url, for Discord Rich Presence art
        let album_art = track
            .album
            .covers
            .0
            .first()
            .and_then(|image| image.id.to_base16().ok())
            .map(|hex| format!("https://i.scdn.co/image/{hex}"));
        Self {
            unavailable: !track.restrictions.is_empty() && track.alternatives.is_empty(),
            uri: track.id.to_uri().expect("a valid uri"),
            artist: track
                .artists
                .first()
                .map(|artist| artist.name.clone())
                .unwrap_or("Unknown Artist".to_string()),
            album: track.album.name.clone(),
            album_art,
            name: track.name.clone(),
            duration: track.duration as u32,
        }
    }
}

#[tauri::command]
pub fn get_player_settings() -> PlayerSettings {
    Settings::current().player.clone()
}

#[tauri::command]
pub fn get_skin() -> String {
    Settings::current().skin.clone()
}

#[tauri::command]
pub fn set_skin(skin: String) {
    Settings::current_mut().skin = skin;
}

#[tauri::command]
pub async fn set_eq(
    enabled: bool,
    preamp: f32,
    bands: Vec<f32>,
    player: State<'_, SharedPlayer>,
) -> Result<(), ()> {
    let mut arr = [0.0f32; 10];
    for (i, v) in bands.iter().take(10).enumerate() {
        arr[i] = *v;
    }
    player.lock().await.set_eq(enabled, preamp, arr);
    Ok(())
}

#[tauri::command]
pub async fn set_balance(balance: f32, player: State<'_, SharedPlayer>) -> Result<(), ()> {
    player.lock().await.set_balance(balance);
    Ok(())
}

#[tauri::command]
pub async fn set_volume(volume: u16, player: State<'_, SharedPlayer>) -> Result<(), ()> {
    player.lock().await.set_volume(volume);
    Settings::current_mut().player.volume = volume;
    Ok(())
}

#[tauri::command]
pub fn set_double_size(active: bool) {
    Settings::current_mut().player.double_size_active = active;
}

#[tauri::command]
pub async fn take_latest_spectrum(player: State<'_, SharedPlayer>) -> Result<Vec<(f32, f32)>, ()> {
    Ok(player.lock().await.take_latest_spectrum())
}

#[tauri::command]
pub async fn load_track(uri: &str, player: State<'_, SharedPlayer>) -> Result<(), String> {
    player
        .lock()
        .await
        .load_track(uri)
        .await
        .map_err(|e| format!("Failed to load track ({e:?})"))
}

#[tauri::command]
pub async fn play(player: State<'_, SharedPlayer>) -> Result<(), String> {
    player.lock().await.play();

    Ok(())
}

#[tauri::command]
pub async fn pause(player: State<'_, SharedPlayer>) -> Result<(), String> {
    player
        .lock()
        .await
        .pause()
        .await
        .map_err(|e| format!("Failed to pause ({e:?})"))?;

    Ok(())
}

#[tauri::command]
pub async fn stop(player: State<'_, SharedPlayer>) -> Result<(), String> {
    player
        .lock()
        .await
        .stop()
        .await
        .map_err(|e| format!("Failed to stop ({e:?})"))?;

    Ok(())
}

//NOTE: these metadata commands clone the session handle under a brief lock and
//      fetch WITHOUT holding the player mutex — otherwise loading a playlist's
//      names would block play/pause/stop for the whole load.
#[tauri::command]
pub async fn get_track_metadata(
    uri: &str,
    player: State<'_, SharedPlayer>,
) -> Result<TrackMetadata, String> {
    let session = player.lock().await.session_handle();
    Ok(TrackMetadata::from(
        &crate::spotify::fetch_track(
            &session,
            SpotifyUri::from_uri(uri)
                .map_err(|e| format!("Failed to get track by uri '{uri}' ({e:?})"))?,
        )
        .await
        .map_err(|e| format!("Could not load track ({e:?})"))?,
    ))
}

#[tauri::command]
pub async fn get_track_ids(
    uri: &str,
    player: State<'_, SharedPlayer>,
) -> Result<Vec<String>, String> {
    let session = player.lock().await.session_handle();
    Ok(crate::spotify::fetch_track_ids(
        &session,
        SpotifyUri::from_uri(uri)
            .map_err(|e| format!("Failed to get playlist by uri '{uri}' ({e:?})"))?,
    )
    .await
    .map_err(|e| format!("Could not load playlist tracks ({e:?})"))?
    .iter()
    .map(|track_uri| track_uri.to_uri().expect("a valid uri"))
    .collect())
}

#[tauri::command]
pub async fn get_user_playlists(
    player: State<'_, SharedPlayer>,
) -> Result<Vec<UserPlaylist>, String> {
    let session = player.lock().await.session_handle();
    crate::spotify::fetch_user_playlists(&session).await
}

#[tauri::command]
pub async fn search(query: &str, player: State<'_, SharedPlayer>) -> Result<Vec<String>, String> {
    let session = player.lock().await.session_handle();
    crate::spotify::fetch_search(&session, query).await
}

#[tauri::command]
pub async fn get_liked_songs(player: State<'_, SharedPlayer>) -> Result<Vec<String>, String> {
    let session = player.lock().await.session_handle();
    crate::spotify::fetch_liked_songs(&session).await
}

#[tauri::command]
pub async fn get_lyrics(
    uri: &str,
    player: State<'_, SharedPlayer>,
) -> Result<crate::spotify::LyricsData, String> {
    let session = player.lock().await.session_handle();
    crate::spotify::fetch_lyrics(&session, uri).await
}

#[tauri::command]
pub async fn seek(position_ms: u32, player: State<'_, SharedPlayer>) -> Result<(), String> {
    player.lock().await.seek(position_ms);
    Ok(())
}

//NOTE: The command needs to be async for Windows to be able to create new windows in it.
//      See https://github.com/tauri-apps/tauri/issues/4121 for details
#[tauri::command]
pub async fn set_playlist_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    let playlist_window = if let Some(playlist_window) = app_handle.get_webview_window("playlist") {
        playlist_window
    } else {
        let player_window = app_handle
            .get_webview_window("player")
            .expect("a player window");
        let mut initial_position = player_window
            .outer_position()
            .expect("a position for the player window");
        initial_position.y += player_window
            .outer_size()
            .expect("a player window position")
            .height as i32;

        let playlist_window = playlist_window::build_window(
            &app_handle,
            initial_position.to_logical(
                player_window
                    .scale_factor()
                    .expect("a scalefactor on the player window"),
            ),
        )
        .expect("a playlist window to be created");
        app_window::register_dock_window(&playlist_window);
        playlist_window
    };
    Settings::current_mut().player.show_playlist = visible;
    if visible {
        playlist_window.show().expect("Playlist window to show");
    } else {
        playlist_window.hide().expect("Playlist window to hide");
    }
    app_window::set_dock_visible(&playlist_window, visible);
    Ok(())
}

pub fn build_window(app_handle: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    let inner_size = Settings::current()
        .player
        .window_state
        .inner_size
        .clone()
        .unwrap_or_default();
    let window =
        app_window::build_frameless_window(app_handle, "player", "Player", "player", inner_size)?;

    app_window::apply_position(
        &window,
        Settings::current().player.window_state.get_position(),
    );
    app_window::remember_position(&window, "player window", |position| {
        Settings::current_mut()
            .player
            .window_state
            .set_position(position);
    });

    // Register the player as the docking master: dragging it moves the whole
    // connected group of windows in lockstep.
    app_window::register_dock_window(&window);

    Ok(window)
}
