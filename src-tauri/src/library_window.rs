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
    let size = crate::settings::Settings::current()
        .window_inner_size("library")
        .unwrap_or(LIBRARY_SIZE);
    let window = app_window::build_frameless_window(app, "library", "Library", "library", size)?;
    app_window::restore_and_remember(&window, "library", initial_position);
    Ok(window)
}

//NOTE: The command needs to be async for Windows to be able to create new
//      windows in it (see the same note in player_window::set_playlist_window_visible).
#[tauri::command]
pub async fn set_library_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    // (a modern skin with a Media Library window of its own: that opens, and
    // this is laid into it; closed with it)
    if visible && crate::modern_window::open_slot(&app_handle, "library") {
        return Ok(());
    }
    if !visible {
        crate::modern_window::close_slot(&app_handle, "library");
    }
    // (in Free Mode there's no Spotify session: the page shows Local Media alone)
    let library_window = match app_handle.get_webview_window("library") {
        Some(window) => window,
        None => {
            // Open it just to the right of the player window.
            // (a modern skin worn: its main window, the player being hidden)
            let (mut position, size, scale_factor) = app_window::anchor(&app_handle).ok_or(())?;
            position.x += size.width as i32;
            let window =
                build_window(&app_handle, position.to_logical(scale_factor)).map_err(|_| ())?;
            // Register with the docking manager so it snaps into the group and
            // moves with the player.
            app_window::register_dock_window(&window);
            window
        }
    };

    if visible {
        library_window.show().map_err(|_| ())?;
        library_window.set_focus().map_err(|_| ())?;
    } else {
        library_window.hide().map_err(|_| ())?;
    }
    app_window::set_dock_visible(&library_window, visible);
    // Remember it's open so the next launch reopens it.
    crate::settings::Settings::current_mut().set_window_visible("library", visible);
    Ok(())
}
