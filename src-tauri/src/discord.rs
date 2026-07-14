//! Discord Rich Presence — shows the current track on the user's Discord
//! profile. Best-effort: if Discord isn't running or no client id is set it
//! silently does nothing and never blocks playback.

use std::{
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity, activity::ActivityType};

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
#[allow(clippy::too_many_arguments)]
pub fn set_discord_activity(
    name: String,
    artist: String,
    album: String,
    album_art: Option<String>,
    // kept for payload compatibility; a party made Discord render the activity
    // as a group session and hid it from the compact profile card
    _playlist_index: i32,
    _playlist_length: i32,
    elapsed_ms: i64,
    duration_ms: i64,
    playing: bool,
) {
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
    // start + end give Discord a Spotify-style progress bar
    let start = now - elapsed_ms / 1000;
    let timestamps = activity::Timestamps::new()
        .start(start)
        .end(start + duration_ms / 1000);

    // real album cover as the big image (with the app logo tucked in the
    // corner); fall back to just the logo when there's no cover art
    let large_image = album_art.as_deref().unwrap_or("logo");
    let large_text = if album.is_empty() { "Spotiamp+" } else { &album };
    let mut assets = activity::Assets::new()
        .large_image(large_image)
        .large_text(large_text);
    if album_art.is_some() {
        assets = assets.small_image("logo").small_text("Spotiamp+");
    }

    // a single clickable button visible to anyone viewing your profile —
    // Spotify's own presence can't do this
    let buttons = vec![activity::Button::new(
        "⚡  Get Spotiamp+",
        "https://github.com/fdeox/spotiamp-plus",
    )];

    // "Listening to Spotiamp+" (type 2), like Spotify — not "Playing"
    let mut act = activity::Activity::new()
        .activity_type(ActivityType::Listening)
        .details(&name)
        .state(&state)
        .assets(assets)
        .buttons(buttons);
    if playing && duration_ms > 0 {
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
