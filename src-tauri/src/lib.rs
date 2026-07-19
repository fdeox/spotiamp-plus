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
mod lists;
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

/// The fixed set of external links the UI may open. The frontend names a
/// target and the URL is resolved *here*, so no caller can hand the shell an
/// arbitrary URL, a local path or a `file:`/`javascript:` scheme.
#[tauri::command]
fn open_external(target: String) -> Result<(), String> {
    let url = match target.as_str() {
        "github" => "https://github.com/fdeox/spotiamp-plus",
        "discord" => "https://discord.gg/8Rq5Xycny4",
        "license" => "https://github.com/fdeox/spotiamp-plus/blob/main/LICENSE",
        other => return Err(format!("unknown link target: {other}")),
    };
    open_in_browser(url);
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_in_browser(url: &str) {
    unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::PCWSTR;
        let file: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        let op: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
        let _ = ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn open_in_browser(_url: &str) {}

/// The running version, read from the Tauri config — so the About box never
/// drifts from the version the installer actually shipped.
#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
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
pub(crate) fn spawn_event_forwarder(player_window: WebviewWindow, mut channel: PlayerEventChannel) {
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
        // Must be registered first: if a second copy is launched, focus the
        // existing player window instead of starting a duplicate instance.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("player") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_auth_url,
            open_external,
            app_version,
            player_window::get_track_metadata,
            player_window::load_track,
            player_window::get_track_ids,
            player_window::get_user_playlists,
            player_window::search,
            player_window::get_liked_songs,
            player_window::get_radio,
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
            player_window::list_audio_devices,
            player_window::set_audio_device,
            lists::get_saved_lists,
            lists::save_list,
            lists::delete_list,
            lists::add_to_list,
            player_window::set_double_size,
            player_window::set_windowshade,
            player_window::set_always_on_top,
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
                    let not_premium = matches!(
                        &e,
                        StartError::LoginFailed {
                            e: SessionError::NotPremium { .. }
                        }
                    );
                    if is_cancelled {
                        log::info!("Login cancelled by user");
                    } else if not_premium {
                        // The common first-run disappointment: say plainly what
                        // is wrong instead of showing a raw error string.
                        let _ = app_handle
                            .dialog()
                            .message(
                                "Spotiamp+ plays audio through your Spotify account, and Spotify \
                                 only allows that for Premium subscriptions — so playback won't \
                                 work on a free account.\n\n\
                                 This is a restriction on Spotify's side, not something Spotiamp+ \
                                 can work around.\n\n\
                                 If you have another account with Premium, you can sign in with \
                                 that one instead.",
                            )
                            .title("Spotiamp+ - Spotify Premium required")
                            .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                            .blocking_show();
                    } else {
                        log::error!("Failed to start ({e:?})");
                        let _ = app_handle
                            .dialog()
                            .message(format!("{e}"))
                            .title("Spotiamp+ - Startup Error")
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
