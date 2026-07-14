//! Discord Rich Presence — shows the current track on the user's Discord
//! profile. Best-effort: if Discord isn't running or no client id is set it
//! silently does nothing and never blocks playback.

use std::{
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};

/// The Discord *application* client id that Rich Presence shows under. Create a
/// free app at <https://discord.com/developers/applications> (the app name is
/// what appears as "Playing <name>") and paste its Client ID here. Empty = the
/// feature stays off.
const DISCORD_CLIENT_ID: &str = "1526643961886675024";

static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);

/// Try to (re)establish the Discord IPC connection. Returns false when disabled
/// or Discord isn't reachable.
fn ensure_connected(guard: &mut Option<DiscordIpcClient>) -> bool {
    if DISCORD_CLIENT_ID.is_empty() {
        return false;
    }
    if guard.is_none()
        && let Ok(mut client) = DiscordIpcClient::new(DISCORD_CLIENT_ID)
        && client.connect().is_ok()
    {
        *guard = Some(client);
    }
    guard.is_some()
}

#[tauri::command]
pub fn set_discord_activity(name: String, artist: String, elapsed_ms: i64, playing: bool) {
    let Ok(mut guard) = CLIENT.lock() else {
        return;
    };
    if !ensure_connected(&mut guard) {
        return;
    }
    let client = guard.as_mut().expect("connected client");

    let state = format!("by {artist}");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let timestamps = activity::Timestamps::new().start(now - elapsed_ms / 1000);
    let assets = activity::Assets::new()
        .large_image("logo")
        .large_text("Spotiamp+");

    let mut act = activity::Activity::new()
        .details(&name)
        .state(&state)
        .assets(assets);
    if playing {
        act = act.timestamps(timestamps);
    }

    // a failed update usually means Discord went away — drop the client so we
    // reconnect on the next track
    if client.set_activity(act).is_err() {
        *guard = None;
    }
}

#[tauri::command]
pub fn clear_discord_activity() {
    if let Ok(mut guard) = CLIENT.lock()
        && let Some(client) = guard.as_mut()
    {
        let _ = client.clear_activity();
    }
}
