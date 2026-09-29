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

#[tauri::command]
pub async fn show_whats_new(app_handle: AppHandle) -> Result<(), ()> {
    let window = match app_handle.get_webview_window("whatsnew") {
        Some(window) => window,
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
    show_whats_new(app_handle).await?;
    Ok(true)
}
