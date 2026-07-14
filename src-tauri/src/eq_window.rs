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
    let window = app_window::build_frameless_window(app, "eq", "Equalizer", "eq", EQ_SIZE)?;
    app_window::apply_position(&window, Some(initial_position));
    Ok(window)
}

//NOTE: async so Windows can create the window inside the command.
#[tauri::command]
pub async fn set_eq_window_visible(visible: bool, app_handle: AppHandle) -> Result<(), ()> {
    let (window, created) = match app_handle.get_webview_window("eq") {
        Some(window) => (window, false),
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
            // dock to the player (subclass id 4, alongside playlist/library/viz)
            app_window::dock_windows(&anchor, &window, "playerWindow", "eqWindow", 4);
            (window, true)
        }
    };

    if visible {
        window.show().map_err(|_| ())?;
        window.set_focus().map_err(|_| ())?;
    } else {
        window.hide().map_err(|_| ())?;
    }
    // let the (player → eq) docking pair track visibility, same as the playlist
    // does — a hidden EQ must not be dragged along or docked against.
    let _ = app_handle.emit(
        "eqWindow",
        serde_json::json!({ "VisibilityChanged": { "visible": visible } }),
    );

    // All follow-up geometry runs on the main thread: window getters/setters
    // called off the main thread block until the main thread services them and
    // can deadlock against the window-event handlers that run there (see the
    // note in app_window::verify_attachment_after_resize).
    let app = app_handle.clone();
    let _ = window.run_on_main_thread(move || {
        // Classic Winamp stacking: player / EQ / playlist.
        shift_docked_playlist(&app, visible);

        if created {
            // Chain the stack natively: the playlist also docks to the EQ
            // (subclass id 5), so dragging the player moves player → EQ →
            // playlist in one native motion. Chained = position-only; the
            // playlist's native owner stays with its primary pair (player).
            if let (Some(eq), Some(playlist)) = (
                app.get_webview_window("eq"),
                app.get_webview_window("playlist"),
            ) {
                app_window::dock_windows_chained(&eq, &playlist, "eqWindow", "playlistWindow", 5);
            }
        }

        // Re-evaluate the playlist's attachments (player pair + eq pair) at its
        // new spot, exactly as if the user had just dropped it there.
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
    let Ok(eq_size) = eq.outer_size() else {
        return;
    };

    let player_bottom = player_pos.y + player_size.height as i32;
    let eq_height = eq_size.height as i32;
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
