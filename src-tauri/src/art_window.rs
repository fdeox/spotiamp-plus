use tauri::{AppHandle, LogicalPosition, Manager, WebviewWindow};

use crate::{app_window, settings::InnerWindowSize};

// Square by default: album covers are square, so this frames one 1:1.
const ART_SIZE: InnerWindowSize = InnerWindowSize {
    width: 275,
    height: 275,
};

pub fn build_window(
    app: &AppHandle,
    initial_position: LogicalPosition<i32>,
) -> Result<WebviewWindow, tauri::Error> {
    let size = crate::settings::Settings::current()
        .window_inner_size("art")
        .unwrap_or(ART_SIZE);
    let window = app_window::build_frameless_window(app, "art", "Album Art", "art", size)?;
    app_window::restore_and_remember(&window, "art", initial_position);
    Ok(window)
}

//NOTE: async so Windows can create the window inside the command.
#[tauri::command]
pub async fn set_art_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    // The cover comes from the librespot metadata the player fetches; controller
    // mode has no such session, so the window stays closed whatever asked for it.
    if visible && crate::settings::Settings::current().controller_mode {
        return Ok(());
    }
    let window = match app_handle.get_webview_window("art") {
        Some(window) => window,
        None => {
            // open it to the right of the player window
            let anchor = app_handle
                .get_webview_window("player")
                .expect("a player window to place the album art next to");
            let mut position = anchor.outer_position().map_err(|_| ())?;
            let size = anchor.outer_size().map_err(|_| ())?;
            position.x += size.width as i32;
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
    // Remember it's open so the next launch reopens it.
    crate::settings::Settings::current_mut().set_window_visible("art", visible);
    Ok(())
}
