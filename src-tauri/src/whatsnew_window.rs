//! "What's new": a small window listing what this version added and the keys
//! worth knowing. Opens by itself once after an update (and on a first run),
//! and any time from the playlist menu's "?" tab. Not part of the docked
//! group: it's a note you read and close, so it's centred and then destroyed.

use tauri::{AppHandle, Manager};

use crate::{app_window, settings::InnerWindowSize, settings::Settings};

const SIZE: InnerWindowSize = InnerWindowSize {
    width: 320,
    height: 380,
};

/// Whether the window was last asked for as the key guide (F1), read by the
/// page when it loads.
static KEYS_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub fn whats_new_keys_only() -> bool {
    KEYS_ONLY.load(std::sync::atomic::Ordering::SeqCst)
}

/// `keys_only`: F1's keyboard shortcut guide, the same window without the
/// version notes.
#[tauri::command]
pub async fn show_whats_new(app_handle: AppHandle, keys_only: Option<bool>) -> Result<(), ()> {
    let keys_only = keys_only.unwrap_or(false);
    KEYS_ONLY.store(keys_only, std::sync::atomic::Ordering::SeqCst);
    let window = match app_handle.get_webview_window("whatsnew") {
        Some(window) => {
            // already open: switch it to what was asked for
            use tauri::Emitter;
            let _ = app_handle.emit_to("whatsnew", "whatsNewMode", keys_only);
            window
        }
        None => {
            let window = app_window::build_frameless_window(
                &app_handle,
                "whatsnew",
                "What's new - Spotiamp+",
                "whatsnew",
                SIZE,
            )
            .map_err(|_| ())?;
            let _ = window.center();
            app_window::own_by_player(&app_handle, &window);
            window
        }
    };
    window.show().map_err(|_| ())?;
    window.set_focus().map_err(|_| ())?;
    Ok(())
}

#[tauri::command]
pub fn close_whats_new(app_handle: AppHandle) {
    if let Some(window) = app_handle.get_webview_window("whatsnew") {
        let _ = window.destroy();
    }
}

/// Called by the player a moment after launch: the first run of a new
/// version shows the window once. Returns whether it did.
#[tauri::command]
pub async fn check_whats_new(app_handle: AppHandle) -> Result<bool, ()> {
    let version = env!("CARGO_PKG_VERSION");
    if Settings::current().last_seen_version.as_deref() == Some(version) {
        return Ok(false);
    }
    Settings::current_mut().last_seen_version = Some(version.to_string());
    show_whats_new(app_handle, None).await?;
    Ok(true)
}
