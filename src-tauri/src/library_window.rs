use tauri::{AppHandle, LogicalPosition, Manager, WebviewWindow};

use crate::{app_window, settings::InnerWindowSize};

/// The library window is wider than the player/playlist windows so it can hold
/// the two panes (playlists on the left, tracks on the right).
const LIBRARY_SIZE: InnerWindowSize = InnerWindowSize {
    width: 500,
    height: 380,
};

pub fn build_window(
    app: &AppHandle,
    initial_position: LogicalPosition<i32>,
) -> Result<WebviewWindow, tauri::Error> {
    let window =
        app_window::build_frameless_window(app, "library", "Library", "library", LIBRARY_SIZE)?;
    app_window::apply_position(&window, Some(initial_position));
    Ok(window)
}

//NOTE: The command needs to be async for Windows to be able to create new
//      windows in it (see the same note in player_window::set_playlist_window_visible).
#[tauri::command]
pub async fn set_library_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    let library_window = match app_handle.get_webview_window("library") {
        Some(window) => window,
        None => {
            // Open it just to the right of the player window.
            let anchor = app_handle
                .get_webview_window("player")
                .expect("a player window to anchor the library to");
            let mut position = anchor.outer_position().map_err(|_| ())?;
            let size = anchor.outer_size().map_err(|_| ())?;
            position.x += size.width as i32;
            let scale_factor = anchor.scale_factor().unwrap_or(1.0);
            let window =
                build_window(&app_handle, position.to_logical(scale_factor)).map_err(|_| ())?;
            // Dock the library to the player just like the playlist does. A
            // distinct subclass id (2 vs the playlist's 1) lets the player drive
            // both followers, so dragging the player moves all three together.
            app_window::dock_windows(&anchor, &window, "playerWindow", "libraryWindow", 2);
            window
        }
    };

    if visible {
        library_window.show().map_err(|_| ())?;
        library_window.set_focus().map_err(|_| ())?;
    } else {
        library_window.hide().map_err(|_| ())?;
    }
    Ok(())
}
