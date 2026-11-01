//! Modern skins (Winamp 3 and 5's .wal, 0.7.10). The skin is drawn by Webamp
//! Modern's engine (src/lib/modern) in one transparent window over all the
//! screens: its own windows (Winamp's containers: the player, playlist,
//! equalizer...) are drawn and moved inside it, and the window is there only
//! where they are (its region), so a click anywhere else goes to whatever is
//! below. While a modern skin is worn the classic player, playlist and
//! equalizer windows keep running, hidden: they still play, the modern
//! window shows them and drives them.
//!
//! The skin file is kept in config/modern-skins; settings.modern_skin names
//! it, and None is the classic look.

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::settings::{get_config_dir, Settings};

const LABEL: &str = "modern";

/// A modern skin is being worn.
pub fn active() -> bool {
    Settings::current().modern_skin.is_some()
}

fn skins_dir() -> Option<std::path::PathBuf> {
    get_config_dir().map(|d| d.join("modern-skins"))
}

/// The worn skin's file.
fn worn_path() -> Option<std::path::PathBuf> {
    let name = Settings::current().modern_skin.clone()?;
    Some(skins_dir()?.join(name))
}

/// All the screens together, in physical pixels: (x, y, width, height).
fn virtual_screen(app: &AppHandle) -> (i32, i32, u32, u32) {
    let monitors = app.available_monitors().unwrap_or_default();
    if monitors.is_empty() {
        return (0, 0, 1920, 1080);
    }
    let (mut left, mut top, mut right, mut bottom) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for m in &monitors {
        let (p, s) = (m.position(), m.size());
        left = left.min(p.x);
        top = top.min(p.y);
        right = right.max(p.x + s.width as i32);
        bottom = bottom.max(p.y + s.height as i32);
    }
    (left, top, (right - left) as u32, (bottom - top) as u32)
}

fn build_window(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    let always_on_top = Settings::current().player.always_on_top;
    let window = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("modern".into()))
        .title("Spotiamp+")
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(true)
        .closable(false)
        .visible(false)
        .disable_drag_drop_handler()
        .accept_first_mouse(true)
        .always_on_top(always_on_top)
        .build()?;
    // nothing of it is there until the skin says where its windows are
    set_region(&window, &[]);
    cover_screens(&window);
    #[cfg(target_os = "windows")]
    {
        // one CSS pixel is one screen pixel: the page scales each of the
        // skin's windows for the screen it's on (modern/+page.svelte)
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller3;
        use windows::core::Interface;
        let _ = window.with_webview(|webview| unsafe {
            if let Ok(controller) = webview.controller().cast::<ICoreWebView2Controller3>() {
                let _ = controller.SetShouldDetectMonitorScaleChanges(false);
                let _ = controller.SetRasterizationScale(1.0);
            }
        });
        crate::app_window::watch_for_renderer_crash(&window);
    }
    crate::file_drop::listen(&window);
    // closing it (Alt+F4, the taskbar's Close window) quits, as the player's does
    window.on_window_event(|event| {
        if let tauri::WindowEvent::CloseRequested { .. } = event {
            crate::quit();
        }
    });
    Ok(window)
}

/// Over all the screens (again after a screen is added or taken away).
fn cover_screens(window: &WebviewWindow) {
    let (x, y, w, h) = virtual_screen(window.app_handle());
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = window.set_size(tauri::PhysicalSize::new(w, h));
}

/// Where the window is at all: these rectangles (physical pixels, from its
/// top left), and nothing else.
fn set_region(window: &WebviewWindow, rects: &[[i32; 4]]) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, RGN_OR};
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let shape = CreateRectRgn(0, 0, 0, 0);
            for [x, y, w, h] in rects {
                let part = CreateRectRgn(*x, *y, x + w, y + h);
                CombineRgn(Some(shape), Some(shape), Some(part), RGN_OR);
                let _ = DeleteObject(part.into());
            }
            // the system owns the region from here (and frees the one before)
            SetWindowRgn(hwnd, Some(shape), true);
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, rects);
}

/// The classic windows or the modern one, as the settings say. The classic
/// player, playlist and equalizer are hidden, not closed: they still play.
/// Never on the main thread: it may build windows (the modern one, Lala's),
/// and building a webview window there can hang the app on Windows.
pub fn apply_face(app: &AppHandle) {
    let modern = active();
    let (show_playlist, show_eq) = {
        let settings = Settings::current();
        (settings.player.show_playlist, settings.player.show_eq)
    };
    for (label, shown) in [("player", true), ("playlist", show_playlist), ("eq", show_eq)] {
        if let Some(window) = app.get_webview_window(label) {
            if modern {
                let _ = window.hide();
            } else if shown {
                let _ = window.show();
            }
        }
    }
    // Lala lives on the classic player: away with it, back with it
    if modern {
        if let Some(mascot) = app.get_webview_window("mascot") {
            let _ = mascot.hide();
        }
    } else {
        let _ = crate::mascot::mascot_show(app.clone());
    }
    if modern {
        let window = match app.get_webview_window(LABEL) {
            Some(window) => Ok(window),
            None => build_window(app),
        };
        match window {
            // shown by the page once the skin is drawn (modern_ready)
            Ok(window) => cover_screens(&window),
            Err(e) => log::error!("Modern skin window: {e}"),
        }
    } else if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

#[derive(serde::Serialize)]
pub struct ModernSkin {
    name: String,
}

/// The worn modern skin, if one is.
#[tauri::command]
pub fn modern_skin() -> Option<ModernSkin> {
    Settings::current().modern_skin.clone().map(|name| ModernSkin { name })
}

/// The worn skin's file, whole.
#[tauri::command]
pub fn modern_skin_file() -> Result<tauri::ipc::Response, String> {
    let path = worn_path().ok_or("no modern skin")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Wear the modern skin in `path` (a .wal, or a .zip of one): a copy is kept
/// in the config folder. Empty bytes or not a zip: refused.
fn wear_file(app: &AppHandle, path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if !bytes.starts_with(b"PK") {
        return Err("That isn't a modern skin (a .wal file is a zip).".into());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .ok_or("no file name")?;
    let dir = skins_dir().ok_or("no config folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&name), &bytes).map_err(|e| e.to_string())?;
    Settings::current_mut().modern_skin = Some(name.clone());
    log::info!("Modern skin worn: {name} ({} bytes)", bytes.len());
    match app.get_webview_window(LABEL) {
        // one shown already: its page puts on the new one
        Some(window) => {
            let _ = window.eval("location.reload()");
        }
        None => apply_face(app),
    }
    Ok(name)
}

/// "Load a modern skin (.wal)…": pick a file and wear it. None if the
/// picker was closed.
#[tauri::command]
pub async fn pick_modern_skin(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("Load a modern skin")
        .add_filter("Winamp modern skins", &["wal", "zip"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    wear_file(&app, &path).map(Some)
}

/// Wear a modern skin from bytes (the Skin Museum's modern skins, or a test),
/// named `name`.
#[tauri::command]
pub async fn wear_modern_skin(app: AppHandle, name: String, bytes: Vec<u8>) -> Result<(), String> {
    let safe: String = name.chars().filter(|c| !r#"\/:*?"<>|"#.contains(*c)).collect();
    let dir = std::env::temp_dir().join("spotiamp-modern");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(if safe.is_empty() { "skin.wal".to_string() } else { safe });
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    wear_file(&app, &path).map(|_| ())
}

/// Back to the classic look (a classic skin was chosen, or the modern one
/// couldn't be shown: `failed` says why, for the log).
#[tauri::command]
pub async fn leave_modern_skin(app: AppHandle, failed: Option<String>) {
    if let Some(why) = failed {
        log::warn!("Modern skin couldn't be shown, back to the classic look: {why}");
    }
    Settings::current_mut().modern_skin = None;
    apply_face(&app);
}

/// The page drew the skin: show the window (it stays hidden till then, so a
/// skin that fails never shows half drawn).
#[tauri::command]
pub fn modern_ready(window: WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

/// Where the skin's windows are now (physical pixels from the window's top
/// left): the window is there and nowhere else.
#[tauri::command]
pub fn modern_set_region(window: WebviewWindow, rects: Vec<[i32; 4]>) {
    set_region(&window, &rects);
}

#[derive(serde::Serialize)]
pub struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    /// the part without the taskbar (Winamp's "viewport")
    work_x: i32,
    work_y: i32,
    work_width: u32,
    work_height: u32,
    scale: f64,
    primary: bool,
}

/// The screens, in the window's own pixels (from its top left), with their
/// scale, so each of the skin's windows is drawn at its screen's size.
#[tauri::command]
pub fn modern_screens(window: WebviewWindow) -> Vec<Screen> {
    let origin = window.outer_position().unwrap_or_default();
    let primary = window.primary_monitor().ok().flatten().map(|m| *m.position());
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let work = m.work_area();
            Screen {
                x: m.position().x - origin.x,
                y: m.position().y - origin.y,
                width: m.size().width,
                height: m.size().height,
                work_x: work.position.x - origin.x,
                work_y: work.position.y - origin.y,
                work_width: work.size.width,
                work_height: work.size.height,
                scale: m.scale_factor(),
                primary: primary == Some(*m.position()),
            }
        })
        .collect()
}

/// Where the classic player is (physical pixels from the modern window's top
/// left), so a skin first opens where the player was.
#[tauri::command]
pub fn modern_player_spot(window: WebviewWindow) -> (i32, i32) {
    let origin = window.outer_position().unwrap_or_default();
    window
        .app_handle()
        .get_webview_window("player")
        .and_then(|p| p.outer_position().ok())
        .map_or((100, 100), |p| (p.x - origin.x, p.y - origin.y))
}
