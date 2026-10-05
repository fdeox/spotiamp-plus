use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewWindow};

use crate::{app_window, settings::InnerWindowSize};

/// A roomy square-ish window for the milkdrop-style visualizer.
const VIZ_SIZE: InnerWindowSize = InnerWindowSize {
    width: 320,
    height: 240,
};

pub fn build_window(
    app: &AppHandle,
    initial_position: LogicalPosition<i32>,
) -> Result<WebviewWindow, tauri::Error> {
    let size = crate::settings::Settings::current()
        .window_inner_size("visualizer")
        .unwrap_or(VIZ_SIZE);
    let window =
        app_window::build_frameless_window(app, "visualizer", "Visualizer", "visualizer", size)?;
    // Reopen where and how it was last left; `initial_position` (below the
    // player) is only used the first time, before it's been placed.
    app_window::restore_and_remember(&window, "visualizer", initial_position);
    Ok(window)
}

//NOTE: async so Windows can create the window inside the command (see the same
//      note on player_window::set_playlist_window_visible).
#[tauri::command]
pub async fn set_visualizer_window_visible(
    visible: bool,
    app_handle: AppHandle,
) -> Result<(), ()> {
    // Closing gives the window's memory back: it's destroyed, and built again
    // (at its saved spot and size) when next opened. See close_dock_window.
    if !visible {
        if let Some(window) = app_handle.get_webview_window("visualizer") {
            if window.is_fullscreen().unwrap_or(false) {
                let _ = window.set_fullscreen(false);
            }
            // MilkDrop draws into a child of it: stop that first
            #[cfg(target_os = "windows")]
            crate::milkdrop::stop_on_main(&app_handle);
            app_window::close_dock_window(&window);
        }
        crate::settings::Settings::current_mut().set_window_visible("visualizer", false);
        return Ok(());
    }
    // Controller mode CAN show the visualizer: it's fed from a system-audio
    // loopback (see loopback.rs) rather than our own pipeline.
    let window = match app_handle.get_webview_window("visualizer") {
        Some(window) => window,
        None => {
            // open it just below the player window
            let anchor = app_handle
                .get_webview_window("player")
                .expect("a player window to place the visualizer under");
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
        // Leave fullscreen first so the dock sees the real window rect, and it
        // comes back windowed next time.
        if window.is_fullscreen().unwrap_or(false) {
            let _ = window.set_fullscreen(false);
        }
        window.hide().map_err(|_| ())?;
    }
    app_window::set_dock_visible(&window, visible);
    // A hidden window keeps its page running; tell it, so it stops drawing
    // and polling the spectrum until it's shown again.
    let _ = app_handle.emit_to("visualizer", "vizVisible", visible);
    // Remember it's open so the next launch reopens it.
    crate::settings::Settings::current_mut().set_window_visible("visualizer", visible);
    Ok(())
}
