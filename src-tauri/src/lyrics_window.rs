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
    let window = app_window::build_frameless_window(app, "lyrics", "Lyrics", "lyrics", LYRICS_SIZE)?;
    app_window::apply_position(&window, Some(initial_position));
    Ok(window)
}

//NOTE: async so Windows can create the window inside the command.
#[tauri::command]
pub async fn set_lyrics_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    let window = match app_handle.get_webview_window("lyrics") {
        Some(window) => window,
        None => {
            // open it to the right of the player window
            let anchor = app_handle
                .get_webview_window("player")
                .expect("a player window to place the lyrics next to");
            let mut position = anchor.outer_position().map_err(|_| ())?;
            let size = anchor.outer_size().map_err(|_| ())?;
            position.x += size.width as i32;
            let scale_factor = anchor.scale_factor().unwrap_or(1.0);
            let window =
                build_window(&app_handle, position.to_logical(scale_factor)).map_err(|_| ())?;
            // dock to the player (subclass id 6, alongside playlist/library/viz/eq)
            app_window::dock_windows(&anchor, &window, "playerWindow", "lyricsWindow", 6);
            window
        }
    };

    if visible {
        window.show().map_err(|_| ())?;
        window.set_focus().map_err(|_| ())?;
    } else {
        window.hide().map_err(|_| ())?;
    }
    Ok(())
}
