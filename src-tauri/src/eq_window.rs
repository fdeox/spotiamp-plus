use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewWindow};

use crate::{app_window, settings::InnerWindowSize};

const EQ_SIZE: InnerWindowSize = InnerWindowSize {
    width: 275,
    height: 116,
};

pub fn build_window(
    app: &AppHandle,
    initial_position: LogicalPosition<i32>,
) -> Result<WebviewWindow, tauri::Error> {
    // EQ is fixed-size, so only its position is remembered (not size).
    let window = app_window::build_frameless_window(app, "eq", "Equalizer", "eq", EQ_SIZE)?;
    app_window::restore_and_remember(&window, "eq", initial_position);
    Ok(window)
}

//NOTE: async so Windows can create the window inside the command.
#[tauri::command]
pub async fn set_eq_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    // The EQ shapes audio we produce ourselves — controller mode has no
    // pipeline, so the window stays closed whatever asked for it.
    if visible && crate::settings::Settings::current().controller_mode {
        return Ok(());
    }
    let window = match app_handle.get_webview_window("eq") {
        Some(window) => window,
        None => {
            // open it just below the player window
            let anchor = app_handle
                .get_webview_window("player")
                .expect("a player window to place the equalizer under");
            let mut position = anchor.outer_position().map_err(|_| ())?;
            let size = anchor.outer_size().map_err(|_| ())?;
            position.y += size.height as i32;
            let scale_factor = anchor.scale_factor().unwrap_or(1.0);
            let window =
                build_window(&app_handle, position.to_logical(scale_factor)).map_err(|_| ())?;
            // Register with the docking manager so it snaps into the group.
            app_window::register_dock_window(&window);
            window
        }
    };

    if visible {
        window.show().map_err(|_| ())?;
        window.set_focus().map_err(|_| ())?;
    } else {
        window.hide().map_err(|_| ())?;
    }
    app_window::set_dock_visible(&window, visible);
    // Remember it's open so the next launch reopens it (and the docked playlist,
    // which sits below the EQ, comes back at the right place).
    crate::settings::Settings::current_mut().player.show_eq = visible;

    // All follow-up geometry runs on the main thread: window getters/setters
    // called off the main thread block until the main thread services them and
    // can deadlock against the window-event handlers that run there.
    let app = app_handle.clone();
    let _ = window.run_on_main_thread(move || {
        // Classic Winamp stacking: player / EQ / playlist. Push the playlist
        // below the EQ (or pull it back up) to make room.
        shift_docked_playlist(&app, visible);
        // The playlist just moved programmatically; re-sync the docking group and
        // native ownership at its new spot, as if the user had dropped it there.
        let _ = app.emit("playlistWindow", serde_json::json!({ "DragEnded": null }));
    });
    Ok(())
}

/// If the playlist window is docked directly under the player (or under the
/// EQ), move it below/above the EQ as the EQ is shown/hidden.
/// MUST run on the main thread (window getters/setters).
fn shift_docked_playlist(app_handle: &AppHandle, eq_shown: bool) {
    let Some(player) = app_handle.get_webview_window("player") else {
        return;
    };
    let Some(playlist) = app_handle.get_webview_window("playlist") else {
        return;
    };
    let Some(eq) = app_handle.get_webview_window("eq") else {
        return;
    };
    let (Ok(player_pos), Ok(player_size)) = (player.outer_position(), player.outer_size()) else {
        return;
    };
    let Ok(playlist_pos) = playlist.outer_position() else {
        return;
    };
    let (Ok(eq_pos), Ok(eq_size)) = (eq.outer_position(), eq.outer_size()) else {
        return;
    };

    let player_bottom = player_pos.y + player_size.height as i32;
    let eq_height = eq_size.height as i32;
    // Only when the EQ really sits right under the player. It reopens at its
    // remembered spot, and if the user had undocked it somewhere else, pushing
    // the playlist down would just leave a gap under the player.
    let eq_docked =
        (eq_pos.x - player_pos.x).abs() <= 4 && (eq_pos.y - player_bottom).abs() <= 4;
    if !eq_docked {
        return;
    }
    let aligned_x = (playlist_pos.x - player_pos.x).abs() <= 4;
    if !aligned_x {
        return;
    }

    if eq_shown {
        // playlist sitting right under the player? push it below the EQ
        if (playlist_pos.y - player_bottom).abs() <= 4 {
            let _ = playlist.set_position(tauri::PhysicalPosition::new(
                playlist_pos.x,
                player_bottom + eq_height,
            ));
        }
    } else {
        // playlist sitting right under the EQ? pull it back up to the player
        if (playlist_pos.y - (player_bottom + eq_height)).abs() <= 4 {
            let _ = playlist.set_position(tauri::PhysicalPosition::new(
                playlist_pos.x,
                player_bottom,
            ));
        }
    }
}

// --- The EQ's state, and AUTO ------------------------------------------------
// The curve set by hand is saved (it used to start flat on every launch). With
// AUTO on, like Winamp's, a song's own preset loads when it starts, else its
// artist's, else the curve set by hand. Presets are saved from the EQ's
// presets menu for the song or artist playing.

use crate::settings::{EqCurve, Settings};

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EqScope {
    Song,
    Artist,
}

/// What's playing (its preset keys), and which preset is on now, if any.
struct NowPlaying {
    song: Option<String>,
    artist: Option<String>,
    applied: Option<EqScope>,
    /// the curve set by hand when this song began: tweaking the EQ for a song
    /// changes it too, and saving the tweak as the song's preset puts it back,
    /// so the other songs keep the usual curve
    usual: Option<EqCurve>,
}

static NOW: std::sync::Mutex<NowPlaying> =
    std::sync::Mutex::new(NowPlaying { song: None, artist: None, applied: None, usual: None });

fn preset_key(now: &NowPlaying, scope: EqScope) -> Option<String> {
    match scope {
        EqScope::Song => now.song.as_ref().map(|s| format!("song:{s}")),
        EqScope::Artist => now.artist.as_ref().map(|a| format!("artist:{}", a.to_lowercase())),
    }
}

fn find_preset(key: &str) -> Option<EqCurve> {
    Settings::current().player.eq.presets.iter().find(|(k, _)| k == key).map(|(_, c)| c.clone())
}

/// The EQ as the window shows it.
#[derive(Clone, serde::Serialize)]
pub struct EqView {
    enabled: bool,
    preamp: i8,
    bands: [i8; 10],
    auto: bool,
    /// whose preset is on now (None = the curve set by hand)
    applied: Option<EqScope>,
    /// something is playing, so its presets can be saved
    song: bool,
    artist: bool,
    has_song_preset: bool,
    has_artist_preset: bool,
}

fn view() -> EqView {
    let (enabled, preamp, bands) = {
        let eq = crate::eq::shared();
        let eq = eq.lock().unwrap();
        (eq.enabled, eq.preamp_db.round() as i8, eq.bands_db.map(|b| b.round() as i8))
    };
    let now = NOW.lock().unwrap();
    let has = |scope| preset_key(&now, scope).is_some_and(|k| find_preset(&k).is_some());
    EqView {
        enabled,
        preamp,
        bands,
        auto: Settings::current().player.eq.auto,
        applied: now.applied,
        song: now.song.is_some(),
        artist: now.artist.is_some(),
        has_song_preset: has(EqScope::Song),
        has_artist_preset: has(EqScope::Artist),
    }
}

fn apply_curve(curve: &EqCurve) {
    let eq = crate::eq::shared();
    let mut eq = eq.lock().unwrap();
    eq.preamp_db = curve.preamp as f32;
    eq.bands_db = curve.bands.map(|b| b as f32);
}

/// Put on the right curve for what's playing, and tell the EQ window.
fn reapply(app: &AppHandle) {
    let saved = Settings::current().player.eq.clone();
    let choice = {
        let mut now = NOW.lock().unwrap();
        let pick = if saved.auto {
            [EqScope::Song, EqScope::Artist]
                .into_iter()
                .find_map(|scope| preset_key(&now, scope).and_then(|k| find_preset(&k)).map(|c| (scope, c)))
        } else {
            None
        };
        now.applied = pick.as_ref().map(|(scope, _)| *scope);
        pick.map(|(_, curve)| curve).unwrap_or(saved.curve)
    };
    apply_curve(&choice);
    let _ = app.emit_to("eq", "eqChanged", view());
}

/// Set from the EQ window. The curve is saved as the one set by hand, unless
/// a song's or artist's preset is on (then it's a change for now only, till
/// it's saved for them from the presets menu).
pub fn set_from_window(enabled: bool, preamp: f32, bands: &[f32]) {
    let clamp = |v: f32| v.round().clamp(-12.0, 12.0) as i8;
    let mut curve = EqCurve { preamp: clamp(preamp), bands: [0; 10] };
    for (i, v) in bands.iter().take(10).enumerate() {
        curve.bands[i] = clamp(*v);
    }
    {
        let eq = crate::eq::shared();
        let mut eq = eq.lock().unwrap();
        eq.enabled = enabled;
    }
    apply_curve(&curve);
    let by_hand = NOW.lock().unwrap().applied.is_none();
    let mut settings = Settings::current_mut();
    settings.player.eq.enabled = enabled;
    if by_hand {
        settings.player.eq.curve = curve;
    }
}

#[tauri::command]
pub fn get_eq() -> EqView {
    view()
}

/// The player started a song: `song` is its uri (or "local:<path>"), `artist`
/// its first artist.
#[tauri::command]
pub fn eq_track(app: AppHandle, song: Option<String>, artist: Option<String>) {
    {
        let mut now = NOW.lock().unwrap();
        let artist = artist.filter(|a| !a.trim().is_empty());
        if now.song == song && now.artist == artist {
            return;
        }
        now.song = song;
        now.artist = artist;
        now.usual = Some(Settings::current().player.eq.curve.clone());
    }
    reapply(&app);
}

#[tauri::command]
pub fn eq_set_auto(app: AppHandle, on: bool) {
    Settings::current_mut().player.eq.auto = on;
    reapply(&app);
}

/// Keep the curve playing now as the song's (or artist's) own; it loads with
/// AUTO on (which this switches on, as Winamp's "save auto-load preset" did).
#[tauri::command]
pub fn eq_auto_save(app: AppHandle, scope: EqScope) {
    let Some(key) = preset_key(&NOW.lock().unwrap(), scope) else {
        return;
    };
    let curve = {
        let eq = crate::eq::shared();
        let eq = eq.lock().unwrap();
        EqCurve { preamp: eq.preamp_db.round() as i8, bands: eq.bands_db.map(|b| b.round() as i8) }
    };
    let usual = NOW.lock().unwrap().usual.clone();
    {
        let mut settings = Settings::current_mut();
        let presets = &mut settings.player.eq.presets;
        presets.retain(|(k, _)| *k != key);
        presets.push((key, curve));
        settings.player.eq.auto = true;
        if let Some(usual) = usual {
            settings.player.eq.curve = usual;
        }
    }
    reapply(&app);
}

#[tauri::command]
pub fn eq_auto_delete(app: AppHandle, scope: EqScope) {
    let Some(key) = preset_key(&NOW.lock().unwrap(), scope) else {
        return;
    };
    Settings::current_mut().player.eq.presets.retain(|(k, _)| *k != key);
    reapply(&app);
}
