//! Discord Rich Presence — shows the current track on the user's Discord
//! profile. Best-effort: if Discord isn't running or no client id is set it
//! silently does nothing and never blocks playback.

use std::{
    sync::{Mutex, OnceLock, mpsc},
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

/// One presence change, applied in order on the Discord thread.
enum Update {
    Set {
        name: String,
        artist: String,
        album: String,
        album_art: Option<String>,
        elapsed_ms: i64,
        duration_ms: i64,
        playing: bool,
    },
    Clear,
}

/// Talking to Discord is pipe I/O that can stall (Discord busy, updating,
/// frozen). These commands used to run it on the UI thread, where a stall
/// would freeze every window, so it happens on a thread of its own instead:
/// the commands only queue the change and return. One thread keeps the
/// changes in order, so a quick pause after a play never ends up "listening".
fn send(update: Update) {
    static QUEUE: OnceLock<Mutex<mpsc::Sender<Update>>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Update>();
        let _ = std::thread::Builder::new()
            .name("discord-presence".into())
            .spawn(move || {
                for update in rx {
                    match update {
                        Update::Set { name, artist, album, album_art, elapsed_ms, duration_ms, playing } => {
                            apply_activity(name, artist, album, album_art, elapsed_ms, duration_ms, playing)
                        }
                        Update::Clear => apply_clear(),
                    }
                }
            });
        Mutex::new(tx)
    });
    if let Ok(tx) = queue.lock() {
        let _ = tx.send(update);
    }
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
    send(Update::Set { name, artist, album, album_art, elapsed_ms, duration_ms, playing });
}

#[tauri::command]
pub fn clear_discord_activity() {
    send(Update::Clear);
}

fn apply_activity(
    name: String,
    artist: String,
    album: String,
    album_art: Option<String>,
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

    // Discord takes 2 to 128 characters here and drops the whole update
    // otherwise (a long file name did that to local files)
    let fit = |text: &str| {
        let mut t: String = text.trim().chars().take(128).collect();
        while t.chars().count() < 2 {
            t.push('\u{2800}');
        }
        t
    };
    let name = fit(&name);
    let state = fit(&if artist.trim().is_empty() { "Local file".to_string() } else { format!("by {artist}") });
    let album = if album.trim().is_empty() { String::new() } else { fit(&album) };
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
        return;
    }
    // Discord answers every command: read it (unread answers pile up in the
    // pipe) and note a refusal, which otherwise just shows nothing
    match client.recv() {
        Ok((_, reply)) if reply.get("evt").and_then(|e| e.as_str()) == Some("ERROR") => {
            log::warn!("Discord didn't take the activity: {}", reply.get("data").map(|d| d.to_string()).unwrap_or_default());
        }
        Ok(_) => {}
        Err(_) => *guard = None,
    }
}

fn apply_clear() {
    if let Ok(mut guard) = CLIENT.lock()
        && let Some(client) = guard.as_mut()
        && client.clear_activity().is_ok()
    {
        let _ = client.recv();
    }
}
