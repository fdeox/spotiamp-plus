use std::sync::Arc;

use librespot::playback::player::{PlayerEvent, PlayerEventChannel};
use serde::{Deserialize, Serialize};
use spotify::{SessionError, SpotifyPlayer};
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use thiserror::Error;

use crate::oauth::OAuthError;
use crate::spotify::{SpotifySession, pending_auth_url};
mod app_window;
mod discord;
mod eq;
mod eq_window;
mod eqf;
mod library_window;
mod lyrics_window;
mod oauth;
mod player_window;
mod playlist_window;
mod settings;
mod visualizer_window;
mod sink;
pub mod spotify;
mod visualizer;
mod wsz;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
enum StartError {
    #[error("Failed to create {window_name} window ({e:?}")]
    WindowCreationFailed {
        window_name: String,
        e: tauri::Error,
    },

    #[error("Failed to login ({e:?}")]
    LoginFailed { e: SessionError },
}

#[derive(Clone, Serialize)]
enum SpotiampPlayerEvent {
    Stopped { uri: String },
    Paused { uri: String, position_ms: u32 },
    EndOfTrack { uri: String },
    PositionCorrection { uri: String, position_ms: u32 },
    PositionChanged { uri: String, position_ms: u32 },
    Seeked { uri: String, position_ms: u32 },
    Playing { uri: String, position_ms: u32 },
}

impl SpotiampPlayerEvent {
    fn from_player_event(player_event: PlayerEvent) -> Option<Self> {
        match player_event {
            PlayerEvent::Playing {
                track_id,
                position_ms,
                ..
            } => Some(Self::Playing {
                uri: track_id.to_uri().expect("a valid uri"),
                position_ms,
            }),
            PlayerEvent::Stopped { track_id, .. } => Some(Self::Stopped {
                uri: track_id.to_uri().expect("a valid uri"),
            }),
            PlayerEvent::Paused {
                track_id,
                position_ms,
                ..
            } => Some(Self::Paused {
                uri: track_id.to_uri().expect("a valid uri"),
                position_ms,
            }),
            PlayerEvent::EndOfTrack { track_id, .. } => Some(Self::EndOfTrack {
                uri: track_id.to_uri().expect("a valid uri"),
            }),
            PlayerEvent::PositionCorrection {
                track_id,
                position_ms,
                ..
            } => Some(Self::PositionCorrection {
                uri: track_id.to_uri().expect("a valid uri"),
                position_ms,
            }),
            PlayerEvent::PositionChanged {
                track_id,
                position_ms,
                ..
            } => Some(Self::PositionChanged {
                uri: track_id.to_uri().expect("a valid uri"),
                position_ms,
            }),
            PlayerEvent::Seeked {
                track_id,
                position_ms,
                ..
            } => Some(Self::Seeked {
                uri: track_id.to_uri().expect("a valid uri"),
                position_ms,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Deserialize)]
enum PlayerWindowEvent {
    CloseRequested,
    DragStarted,
    DragEnded,
}

#[tauri::command]
fn get_auth_url() -> Result<String, String> {
    pending_auth_url()
        .lock()
        .map_err(|_| "lock error".to_string())?
        .take()
        .ok_or_else(|| "no pending auth URL".to_string())
}

async fn start_app(app_handle: &AppHandle) -> Result<(), StartError> {
    let session = SpotifySession::default();
    session
        .login(app_handle)
        .await
        .map_err(|e| StartError::LoginFailed { e })?;

    let player_window =
        player_window::build_window(app_handle).map_err(|e| StartError::WindowCreationFailed {
            window_name: "Player".to_string(),
            e,
        })?;
    let player = Arc::new(tokio::sync::Mutex::new(SpotifyPlayer::new(session)));
    app_handle.manage(player.clone());

    // Forward playback events from the current player to the UI.
    let channel = player.lock().await.get_player_event_channel();
    spawn_event_forwarder(player_window.clone(), channel);

    // Watch the Spotify connection: it gets force-closed every so often
    // (os error 10054) and a librespot session can't be revived, so when it
    // drops we rebuild session + player and re-forward events — playback
    // recovers on the next play without needing an app restart.
    {
        let player = player.clone();
        let app_handle = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                if !player.lock().await.is_session_invalid() {
                    continue;
                }
                log::warn!("Spotify session dropped — reconnecting…");
                let result = player.lock().await.reconnect(&app_handle).await;
                match result {
                    Ok(channel) => {
                        log::info!("Reconnected to Spotify.");
                        spawn_event_forwarder(player_window.clone(), channel);
                    }
                    Err(e) => log::warn!("Reconnect failed (will retry in 10s): {e:?}"),
                }
            }
        });
    }

    Ok(())
}

/// Pump playback events from a player's channel out to the player window.
/// Ends on its own when the channel closes (e.g. when a stale player is
/// replaced during a reconnect).
fn spawn_event_forwarder(player_window: WebviewWindow, mut channel: PlayerEventChannel) {
    tauri::async_runtime::spawn(async move {
        while let Some(player_event) = channel.recv().await {
            if let Some(player_event) = SpotiampPlayerEvent::from_player_event(player_event) {
                let _ = player_window.emit("player", player_event);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_auth_url,
            player_window::get_track_metadata,
            player_window::load_track,
            player_window::get_track_ids,
            player_window::get_user_playlists,
            player_window::search,
            player_window::get_lyrics,
            player_window::play,
            player_window::pause,
            player_window::stop,
            player_window::get_player_settings,
            player_window::get_skin,
            player_window::set_skin,
            wsz::pick_and_load_skin,
            wsz::get_custom_skin,
            wsz::list_bundled_skins,
            wsz::load_bundled_skin,
            player_window::set_eq,
            player_window::set_balance,
            player_window::set_volume,
            player_window::set_double_size,
            player_window::take_latest_spectrum,
            player_window::seek,
            player_window::set_playlist_window_visible,
            playlist_window::get_playlist_settings,
            playlist_window::set_uris,
            playlist_window::set_playlist_inner_size,
            library_window::set_library_window_visible,
            visualizer_window::set_visualizer_window_visible,
            eq_window::set_eq_window_visible,
            eqf::import_eqf,
            lyrics_window::set_lyrics_window_visible,
            discord::set_discord_activity,
            discord::clear_discord_activity,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            app_handle.listen("playerWindow", move |event| {
                match serde_json::from_str::<PlayerWindowEvent>(event.payload()) {
                    Ok(e) => match e {
                        PlayerWindowEvent::CloseRequested => {
                            std::process::exit(0);
                        }
                        PlayerWindowEvent::DragStarted | PlayerWindowEvent::DragEnded => {}
                    },
                    Err(e) => log::debug!(
                        "Could not deserialize playlistWindow event: '{:?}' ({e:?}) - ignoring",
                        event.payload()
                    ),
                }
            });
            tauri::async_runtime::spawn(async move {
                if let Err(e) = start_app(&app_handle).await {
                    let is_cancelled = matches!(
                        &e,
                        StartError::LoginFailed {
                            e: SessionError::TokenExchangeFailure {
                                e: OAuthError::Cancelled
                            }
                        }
                    );
                    if is_cancelled {
                        log::info!("Login cancelled by user");
                    } else {
                        log::error!("Failed to start ({e:?})");
                        let _ = app_handle
                            .dialog()
                            .message(format!("{e}"))
                            .title("Spotiamp - Startup Error")
                            .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                            .blocking_show();
                    }
                    std::process::exit(1);
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
