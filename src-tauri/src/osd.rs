//! On-screen display: when the song changes, a small skinned card with the
//! cover, title and artist shows for a few seconds in the corner of the
//! screen the player is on, like Winamp's OSD plugins. Opt-in (Windows menu).
//! It never takes focus and clicks go through it.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::settings::Settings;

/// Base size at 1x; the UI scale applies like everywhere else.
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 72.0;
/// Gap to the screen edge (logical px).
const MARGIN: f64 = 16.0;
/// How long a card stays up.
const SHOW_FOR: std::time::Duration = std::time::Duration::from_secs(4);

#[derive(Serialize, Clone)]
pub struct Card {
    title: String,
    artist: String,
    art: Option<String>,
    /// bumped per show, so the page restarts its timer for a new song
    seq: u64,
}

/// The card being shown, for a page that loads after the first show's event.
fn current() -> &'static Mutex<Option<Card>> {
    static CURRENT: Mutex<Option<Card>> = Mutex::new(None);
    &CURRENT
}

fn build(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    let scale = crate::app_window::ui_scale();
    tauri::WebviewWindowBuilder::new(app, "osd", tauri::WebviewUrl::App("osd".into()))
        .title("Spotiamp+ now playing")
        .inner_size(WIDTH * scale, HEIGHT * scale)
        .initialization_script(format!("window.__SPOTIAMP_UI_SCALE__ = {scale};"))
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()
        .inspect(|window| {
            let _ = window.set_ignore_cursor_events(true);
            #[cfg(target_os = "windows")]
            crate::app_window::watch_for_renderer_crash(window);
        })
}

/// Bottom-right of the work area (the screen minus the taskbar) of the
/// monitor the player is on.
fn place(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .get_webview_window("player")
        .and_then(|p| p.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let area = monitor.work_area();
    let margin = (MARGIN * monitor.scale_factor()).round() as i32;
    let x = area.position.x + area.size.width as i32 - size.width as i32 - margin;
    let y = area.position.y + area.size.height as i32 - size.height as i32 - margin;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Show the song (called by the player on every change; does nothing unless
/// the OSD is switched on).
#[tauri::command]
pub async fn osd_show(
    app: AppHandle,
    title: String,
    artist: String,
    art: Option<String>,
) -> Result<(), String> {
    let enabled = {
        let settings = Settings::current();
        settings.player.osd && !settings.controller_mode
    };
    if !enabled || title.is_empty() {
        return Ok(());
    }
    let card = {
        let mut slot = current().lock().map_err(|_| "osd state")?;
        let seq = slot.as_ref().map(|c| c.seq + 1).unwrap_or(1);
        let card = Card { title, artist, art, seq };
        *slot = Some(card.clone());
        card
    };
    let window = match app.get_webview_window("osd") {
        Some(w) => w,
        None => build(&app).map_err(|e| e.to_string())?,
    };
    place(&app, &window);
    let seq = card.seq;
    let _ = app.emit_to("osd", "osdShow", card);
    show_without_focus(&window);
    // Hide it from here, not from the page: shown without activation, the
    // page counts as hidden to WebView2 and its timers can't be relied on (the
    // card stayed up for good). A newer song's card keeps the window up.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_FOR).await;
        let latest = current().lock().ok().and_then(|c| c.as_ref().map(|c| c.seq));
        if latest == Some(seq) {
            if let Some(w) = app.get_webview_window("osd") {
                hide_window(&w);
            }
        }
    });
    Ok(())
}

/// The card for a freshly loaded page.
#[tauri::command]
pub fn osd_current() -> Option<Card> {
    current().lock().ok().and_then(|c| c.clone())
}

/// The page hides itself once its time is up.
#[tauri::command]
pub async fn osd_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window("osd") {
        hide_window(&w);
    }
}

/// Hide it the same way it was shown: straight through Win32. Shown behind
/// the window library's back (to avoid taking focus), the library still
/// thinks the window is hidden, so its own hide() did nothing and the card
/// stayed up for good.
fn hide_window(window: &WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        let window = window.clone();
        let _ = window.clone().run_on_main_thread(move || {
            if let Ok(hwnd) = window.hwnd() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window.hide();
}

/// Switch the OSD on or off (persisted). Off hides one that's up.
#[tauri::command]
pub async fn set_osd(enabled: bool, app: AppHandle) {
    Settings::current_mut().player.osd = enabled;
    if !enabled {
        osd_hide(app).await;
    }
}

/// Show it on top without activating it: the song change mustn't pull focus
/// out of whatever the user is typing into.
fn show_without_focus(window: &WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_TOPMOST, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
            ShowWindow,
        };
        let window = window.clone();
        let _ = window.clone().run_on_main_thread(move || {
            if let Ok(hwnd) = window.hwnd() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                    );
                }
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window.show();
}
