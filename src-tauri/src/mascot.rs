//! The llama: Spotiamp+'s mascot, sitting on the player's top edge. It
//! dances while music plays, falls asleep when it stops and reacts to a few
//! things (a new badge, a loved song, a song that won't load); the mascot page
//! does all of that. This side owns its window: a small transparent one that
//! the player owns (so it stays above it and minimizes with it) and that
//! follows the player around. On by default; the Windows menu switches it off
//! and picks its size.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use crate::settings::Settings;

/// One animation frame, at the art's own size (a 64 px tall llama).
const FRAME_W: f64 = 80.0;
const FRAME_H: f64 = 84.0;
/// Empty rows under its feet in a frame, so it sits right on the player.
const FEET_GAP: f64 = 5.0;
/// How far in from the player's right edge it sits (1x px).
const RIGHT_INSET: f64 = 22.0;
const SIZES: [u16; 3] = [48, 56, 64];

#[derive(Serialize, Clone)]
pub struct MascotSettings {
    enabled: bool,
    size: u16,
}

fn current() -> MascotSettings {
    let settings = Settings::current();
    MascotSettings {
        enabled: settings.player.mascot.unwrap_or(true),
        size: settings.player.mascot_size.filter(|s| SIZES.contains(s)).unwrap_or(56),
    }
}

/// Window size in logical px: the frame scaled to the chosen height, times the
/// UI scale like every other window.
fn logical_size(size: u16) -> (f64, f64) {
    let k = size as f64 / 64.0 * crate::app_window::ui_scale();
    ((FRAME_W * k).round(), (FRAME_H * k).round())
}

fn build(app: &AppHandle, size: u16) -> Result<WebviewWindow, tauri::Error> {
    let (w, h) = logical_size(size);
    tauri::WebviewWindowBuilder::new(app, "mascot", tauri::WebviewUrl::App("mascot".into()))
        .title("Spotiamp+ llama")
        .inner_size(w, h)
        // Windows otherwise keeps a window at least ~136 px wide
        .min_inner_size(w, h)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .disable_drag_drop_handler()
        .accept_first_mouse(true)
        .build()
        .inspect(|window| {
            #[cfg(target_os = "windows")]
            crate::app_window::watch_for_renderer_crash(window);
        })
}

/// Sit on the player's top edge, near its right end. MUST run on the main
/// thread (the window getters block anywhere else).
fn place(app: &AppHandle, mascot: &WebviewWindow) {
    let Some(player) = app.get_webview_window("player") else {
        return;
    };
    let (Ok(pos), Ok(psize), Ok(msize), Ok(sf)) =
        (player.outer_position(), player.outer_size(), mascot.outer_size(), player.scale_factor())
    else {
        return;
    };
    let ui = crate::app_window::ui_scale();
    let k = current().size as f64 / 64.0 * ui * sf;
    let x = pos.x + psize.width as i32 - msize.width as i32 - (RIGHT_INSET * ui * sf).round() as i32;
    let mut y = pos.y - msize.height as i32 + (FEET_GAP * k).round() as i32;
    // No room above (the player is at the top of the screen): keep it on
    // screen, over the player's corner, rather than lost off the top.
    if let Ok(Some(monitor)) = player.current_monitor() {
        y = y.max(monitor.work_area().position.y);
    }
    let _ = mascot.set_position(PhysicalPosition::new(x, y));
}

/// The player moved or changed size: bring the llama along. Called from the
/// player's window events, on the main thread.
pub fn follow_player(app: &AppHandle) {
    if let Some(mascot) = app.get_webview_window("mascot") {
        place(app, &mascot);
    }
}

/// Size and spot again (the UI scale changed).
pub fn refresh(app: &AppHandle) {
    if let Some(mascot) = app.get_webview_window("mascot") {
        let (w, h) = logical_size(current().size);
        let _ = mascot.set_min_size(Some(tauri::LogicalSize::new(w, h)));
        let _ = mascot.set_size(tauri::LogicalSize::new(w, h));
        place(app, &mascot);
    }
}

fn show(app: &AppHandle) -> Result<(), String> {
    let settings = current();
    let mascot = match app.get_webview_window("mascot") {
        Some(w) => w,
        None => build(app, settings.size).map_err(|e| e.to_string())?,
    };
    let app = app.clone();
    let _ = mascot.clone().run_on_main_thread(move || {
        // owned by the player: above it, minimized and restored with it
        #[cfg(target_os = "windows")]
        if let (Some(player), Ok(own)) = (app.get_webview_window("player"), mascot.hwnd()) {
            if let Ok(owner) = player.hwnd() {
                crate::app_window::set_owner(own.0 as isize, owner.0 as isize);
            }
        }
        // the size again now that it exists: at creation Windows kept it
        // ~136 px wide whatever was asked; a resize afterwards takes
        let (w, h) = logical_size(current().size);
        let _ = mascot.set_min_size(Some(tauri::LogicalSize::new(w, h)));
        let _ = mascot.set_size(tauri::LogicalSize::new(w, h));
        place(&app, &mascot);
        let _ = mascot.show();
        // showing it mustn't take the focus from the player
        if let Some(player) = app.get_webview_window("player") {
            let _ = player.set_focus();
        }
    });
    Ok(())
}

/// Called by the player once it's up: show the llama unless it's switched off.
#[tauri::command(async)]
pub fn mascot_show(app: AppHandle) -> Result<(), String> {
    if current().enabled { show(&app) } else { Ok(()) }
}

#[tauri::command]
pub fn mascot_settings() -> MascotSettings {
    current()
}

/// The Windows menu: switch it on or off, or pick its size (48, 56, 64).
#[tauri::command(async)]
pub fn mascot_set(app: AppHandle, enabled: Option<bool>, size: Option<u16>) -> Result<(), String> {
    {
        let mut settings = Settings::current_mut();
        if let Some(enabled) = enabled {
            settings.player.mascot = Some(enabled);
        }
        if let Some(size) = size.filter(|s| SIZES.contains(s)) {
            settings.player.mascot_size = Some(size);
        }
    }
    let now = current();
    let _ = app.emit("mascotSettings", now.clone());
    if !now.enabled {
        // gone for good until switched back on: its page and memory go too
        if let Some(mascot) = app.get_webview_window("mascot") {
            let _ = mascot.destroy();
        }
        return Ok(());
    }
    if size.is_some() {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || refresh(&app2));
    }
    show(&app)
}
