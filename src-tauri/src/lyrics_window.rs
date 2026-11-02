use tauri::{AppHandle, LogicalPosition, Manager, WebviewWindow};

use crate::{app_window, settings::InnerWindowSize};

const LYRICS_SIZE: InnerWindowSize = InnerWindowSize {
    width: 275,
    height: 232,
};

pub fn build_window(
    app: &AppHandle,
    initial_position: LogicalPosition<i32>,
) -> Result<WebviewWindow, tauri::Error> {
    let size = crate::settings::Settings::current()
        .window_inner_size("lyrics")
        .unwrap_or(LYRICS_SIZE);
    let window = app_window::build_frameless_window(app, "lyrics", "Lyrics", "lyrics", size)?;
    app_window::restore_and_remember(&window, "lyrics", initial_position);
    Ok(window)
}

//NOTE: async so Windows can create the window inside the command.
#[tauri::command]
pub async fn set_lyrics_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    // Closing gives the window's memory back: it's destroyed, and built again
    // (at its saved spot and size) when next opened. See close_dock_window.
    if !visible {
        if let Some(window) = app_handle.get_webview_window("lyrics") {
            app_window::close_dock_window(&window);
        }
        crate::settings::Settings::current_mut().set_window_visible("lyrics", false);
        return Ok(());
    }
    let window = match app_handle.get_webview_window("lyrics") {
        Some(window) => window,
        None => {
            // open it to the right of the player window
            // (a modern skin worn: its main window, the player being hidden)
            let (mut position, size, scale_factor) = app_window::anchor(&app_handle).ok_or(())?;
            position.x += size.width as i32;
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
    // Remember it's open so the next launch reopens it.
    crate::settings::Settings::current_mut().set_window_visible("lyrics", visible);
    Ok(())
}
