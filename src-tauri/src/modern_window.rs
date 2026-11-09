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

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::settings::{get_config_dir, Settings};

const LABEL: &str = "modern";

/// A modern skin is being worn.
pub fn active() -> bool {
    Settings::current().modern_skin.is_some()
}

/// Where the skin's windows are on the screens (physical pixels: x, y,
/// width, height): Spotiamp+'s own windows snap to them and open beside the
/// main one, and Lala sits on it.
#[derive(Default)]
struct Geometry {
    windows: Vec<[i32; 4]>,
    main: Option<[i32; 4]>,
}

fn geometry() -> &'static Mutex<Geometry> {
    static GEOMETRY: Mutex<Geometry> = Mutex::new(Geometry { windows: Vec::new(), main: None });
    &GEOMETRY
}

/// The skin's main window on the screens, while a modern skin is worn and drawn.
pub fn main_rect() -> Option<[i32; 4]> {
    if !active() {
        return None;
    }
    geometry().lock().ok()?.main
}

/// The modern skin's window, while one is worn: what Spotiamp+'s other
/// windows belong to then, instead of the hidden classic player.
pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    if active() { app.get_webview_window(LABEL) } else { None }
}

/// The skin's look for Spotiamp+'s own windows (made by its page, see
/// src/lib/modern/look.js), and the skin it's for. Kept on disk too, so the
/// windows that open with the app wear it before the skin is drawn again.
fn kept_look() -> &'static Mutex<Option<(String, std::sync::Arc<serde_json::Value>)>> {
    static LOOK: Mutex<Option<(String, std::sync::Arc<serde_json::Value>)>> = Mutex::new(None);
    &LOOK
}

fn look_path(skin: &str) -> Option<std::path::PathBuf> {
    Some(skins_dir()?.join(format!("{skin}.look.json")))
}

// ---- Winamp's Media Library and visualizer, in the skin ----
// A modern skin has places for them (a windowholder or component of its
// main window, as Big Bento's, or a window of its own, as Winamp Modern's):
// Winamp put its own there, and Spotiamp+ lays its Library and visualizer
// there, without a frame of their own, moving and sizing with the skin.

/// One such place, as the skin's page sees it.
#[derive(serde::Deserialize, Default, Clone, PartialEq)]
pub struct Slot {
    /// where it is now, shown (physical pixels from the modern window's top left)
    #[serde(default)]
    rect: Option<[i32; 4]>,
    /// a window of the skin's own for it, closed now, that can be opened
    #[serde(default)]
    openable: bool,
    /// a smaller place for it shown (in a main window): the visualizer goes
    /// there once it's on, as Winamp's did
    #[serde(default)]
    inline: Option<[i32; 4]>,
}

#[derive(serde::Deserialize, Default, Clone, PartialEq)]
pub struct Slots {
    #[serde(default)]
    ml: Slot,
    #[serde(default)]
    vis: Slot,
}

/// Winamp's name for each place, and the window of Spotiamp+'s laid there.
const SLOTTED: [(&str, &str); 2] = [("ml", "library"), ("vis", "visualizer")];

fn slots() -> &'static Mutex<Slots> {
    static SLOTS: Mutex<Slots> = Mutex::new(Slots {
        ml: Slot { rect: None, openable: false, inline: None },
        vis: Slot { rect: None, openable: false, inline: None },
    });
    &SLOTS
}

/// The windows laid into the skin now.
fn laid() -> &'static Mutex<Vec<String>> {
    static LAID: Mutex<Vec<String>> = Mutex::new(Vec::new());
    &LAID
}

/// The window is laid into the skin (its page draws no frame of its own,
/// and its size is the place's).
pub fn embedded(label: &str) -> bool {
    active() && laid().lock().is_ok_and(|l| l.iter().any(|x| x == label))
}

fn set_embedded(label: &str, on: bool) {
    if let Ok(mut l) = laid().lock() {
        l.retain(|x| x != label);
        if on {
            l.push(label.to_string());
        }
    }
}

/// Opening the Library or the visualizer while the skin has a window of its
/// own for it, closed: that's opened instead, and Spotiamp+'s is laid into
/// it once it shows. Whether it was.
pub fn open_slot(app: &AppHandle, label: &str) -> bool {
    let Some((kind, _)) = SLOTTED.iter().find(|(_, l)| *l == label) else { return false };
    if !active() {
        return false;
    }
    let slot = slots().lock().map(|s| if *kind == "ml" { s.ml.clone() } else { s.vis.clone() }).unwrap_or_default();
    if slot.rect.is_none() && slot.openable {
        let _ = app.emit_to(LABEL, "modernOpenSlot", *kind);
        return true;
    }
    false
}

/// Closing a window laid into a window of the skin's own: that closes too.
pub fn close_slot(app: &AppHandle, label: &str) {
    if let Some((kind, _)) = SLOTTED.iter().find(|(_, l)| *l == label)
        && embedded(label)
    {
        let _ = app.emit_to(LABEL, "modernCloseSlot", *kind);
        set_embedded(label, false);
    }
}

/// The Library or the visualizer was opened: the skin's page says again
/// where their places are, so it's laid into one if there is one.
pub fn slots_again(app: &AppHandle) {
    if active() {
        let _ = app.emit_to(LABEL, "modernSlotsAgain", ());
    }
}

/// Is this window laid into the skin (asked by its page when it loads).
#[tauri::command]
pub fn modern_embedded(window: WebviewWindow) -> bool {
    embedded(window.label())
}

/// The skin's places for the Library and the visualizer changed (shown,
/// hidden, moved, sized): Spotiamp+'s windows are laid there, or go.
#[tauri::command]
pub async fn modern_set_slots(window: WebviewWindow, slots_now: Slots) {
    let app = window.app_handle().clone();
    let origin = window.outer_position().unwrap_or_default();
    if let Ok(mut s) = slots().lock() {
        *s = slots_now.clone();
    }
    for (kind, label) in SLOTTED {
        let slot = if kind == "ml" { &slots_now.ml } else { &slots_now.vis };
        let was = embedded(label);
        let on = app.get_webview_window(label).is_some_and(|w| w.is_visible().unwrap_or(false));
        // a smaller place in a main window takes the visualizer only once it's on
        let place = slot.rect.or(if on { slot.inline } else { None });
        match place {
            Some([x, y, w, h]) if w > 8 && h > 8 => {
                // (marked first: a window built now draws no frame from the start)
                set_embedded(label, true);
                let shown = app.get_webview_window(label).is_some_and(|w| w.is_visible().unwrap_or(false));
                if !shown {
                    let _ = if label == "library" {
                        crate::library_window::set_library_window_visible(true, app.clone()).await
                    } else {
                        crate::visualizer_window::set_visualizer_window_visible(true, app.clone()).await
                    };
                }
                let Some(target) = app.get_webview_window(label) else { continue };
                let at = tauri::PhysicalPosition::new(x + origin.x, y + origin.y);
                let size = tauri::PhysicalSize::new(w as u32, h as u32);
                if target.outer_position().ok() != Some(at) {
                    let _ = target.set_position(at);
                }
                if target.inner_size().ok() != Some(size) {
                    let _ = target.set_size(size);
                }
                if !was {
                    let _ = app.emit_to(label, "modernEmbed", true);
                }
            }
            _ if was => {
                set_embedded(label, false);
                let _ = app.emit_to(label, "modernEmbed", false);
                // the skin closed its place: Spotiamp+'s window goes with it
                let _ = if label == "library" {
                    crate::library_window::set_library_window_visible(false, app.clone()).await
                } else {
                    crate::visualizer_window::set_visualizer_window_visible(false, app.clone()).await
                };
            }
            _ => {
                // open on its own (left open last time, or from the classic
                // look): into the skin's window for it
                if slot.openable && app.get_webview_window(label).is_some_and(|w| w.is_visible().unwrap_or(false)) {
                    let _ = app.emit_to(LABEL, "modernOpenSlot", kind);
                }
            }
        }
    }
}

/// Back to the classic look, or another skin: nothing is laid in any more.
fn unlay_all(app: &AppHandle) {
    if let Ok(mut s) = slots().lock() {
        *s = Slots::default();
    }
    let labels: Vec<String> = laid().lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
    for label in labels {
        let _ = app.emit_to(label.as_str(), "modernEmbed", false);
    }
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
    // the skin's windows are somewhere else now (Lala waits for the new ones)
    if let Ok(mut g) = geometry().lock() {
        *g = Geometry::default();
    }
    unlay_all(app);
    if modern {
        if let Some(mascot) = app.get_webview_window("mascot") {
            let _ = mascot.hide();
        }
        let window = match app.get_webview_window(LABEL) {
            Some(window) => Ok(window),
            None => {
                watch_until_drawn(app);
                build_window(app)
            }
        };
        match window {
            // shown by the page once the skin is drawn (modern_ready)
            Ok(window) => cover_screens(&window),
            Err(e) => log::error!("Modern skin window: {e}"),
        }
    } else {
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.destroy();
        }
        if let Ok(mut l) = kept_look().lock() {
            *l = None;
        }
        // Lala back on the classic player
        let _ = crate::mascot::mascot_show(app.clone());
    }
    // the open windows belong to whichever is showing (they minimize and
    // come forward with it, and share its taskbar button)
    crate::app_window::reown_windows(app);
    // the windows' frames and colours: the skin's, or the classic ones again
    let _ = app.emit("modernFace", modern);
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

/// A modern skin's text painted as Winamp 5 painted it (wasabi_text.rs):
/// [w, h, text width] then its pixels; empty when it can't be.
#[tauri::command]
pub async fn wasabi_text_paint(paint: crate::wasabi_text::Paint) -> Result<tauri::ipc::Response, ()> {
    Ok(tauri::ipc::Response::new(crate::wasabi_text::paint(&paint).map(|p| p.to_bytes()).unwrap_or_default()))
}

/// A skin's font file (the request's bytes) loaded for its texts; its face,
/// as Winamp named it ("" when it isn't a font).
#[tauri::command]
pub async fn wasabi_text_font(request: tauri::ipc::Request<'_>) -> Result<String, ()> {
    Ok(match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => crate::wasabi_text::load_font(bytes).unwrap_or_default(),
        _ => String::new(),
    })
}

/// The face Winamp drew a font the skin doesn't have with (Windows' own).
#[tauri::command]
pub async fn wasabi_text_face(id: String) -> Result<String, ()> {
    Ok(crate::wasabi_text::face_of_system_font(&id))
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
    // a classic skin in a modern skin's name (Winamp 2's main.bmp, no
    // skin.xml): put on as the classic skin it is, as Winamp did
    if is_classic_skin(&bytes) {
        crate::wsz::apply_wsz_bytes(&bytes)?;
        {
            let mut settings = Settings::current_mut();
            settings.skin = "custom".to_string();
            settings.modern_skin = None;
        }
        log::info!("{name} is a classic skin: put on as one");
        apply_face(app);
        let _ = app.emit("skinChanged", serde_json::json!({ "skin": "custom", "from": "file", "name": name }));
        return Ok(name);
    }
    let dir = skins_dir().ok_or("no config folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&name), &bytes).map_err(|e| e.to_string())?;
    Settings::current_mut().modern_skin = Some(name.clone());
    log::info!("Modern skin worn: {name} ({} bytes)", bytes.len());
    match app.get_webview_window(LABEL) {
        // one shown already: its page puts on the new one
        Some(window) => {
            unlay_all(app);
            watch_until_drawn(app);
            let _ = window.eval("location.reload()");
        }
        None => apply_face(app),
    }
    Ok(name)
}

/// A zip with Winamp 2's main.bmp and no skin.xml: a classic skin.
fn is_classic_skin(bytes: &[u8]) -> bool {
    let Ok(zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else { return false };
    let names: Vec<String> = zip.file_names().map(|n| n.replace('\\', "/").to_lowercase()).collect();
    let has = |file: &str| names.iter().any(|n| n == file || n.ends_with(&format!("/{file}")));
    has("main.bmp") && !has("skin.xml")
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
    DRAWN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let _ = window.show();
    let _ = window.set_focus();
}

/// How many times a skin's page has said it's drawn (modern_ready).
static DRAWN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A skin is being put on: if its page doesn't say it's drawn in time (a
/// script of the skin's that never ends would have it stop answering, and
/// the classic windows are hidden meanwhile), the classic look comes back.
/// (The page gives up by itself on a skin that just fails, sooner.)
fn watch_until_drawn(app: &AppHandle) {
    let before = DRAWN.load(std::sync::atomic::Ordering::SeqCst);
    let skin = Settings::current().modern_skin.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if DRAWN.load(std::sync::atomic::Ordering::SeqCst) != before || Settings::current().modern_skin != skin {
            return;
        }
        let name = skin.unwrap_or_default();
        log::warn!("Modern skin {name} was never drawn (its page stopped answering): back to the classic look");
        Settings::current_mut().modern_skin = None;
        apply_face(&app);
        use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
        app.dialog()
            .message(format!(
                "Spotiamp+ couldn't show the modern skin \"{}\", so it's back to the classic look.",
                name.trim_end_matches(".wal").trim_end_matches(".zip")
            ))
            .title("Spotiamp+")
            .kind(MessageDialogKind::Warning)
            .show(|_| {});
    });
}

/// Where the skin's windows are now (physical pixels from the window's top
/// left): the window is there and nowhere else.
/// With it, where the skin's windows themselves are (`windows`, its main one
/// `main`), the same way; `moving`: one is being dragged.
#[tauri::command]
pub fn modern_set_region(
    window: WebviewWindow,
    rects: Vec<[i32; 4]>,
    windows: Option<Vec<[i32; 4]>>,
    main: Option<[i32; 4]>,
    moving: Option<bool>,
) {
    set_region(&window, &rects);
    let Some(windows) = windows else { return };
    // minimized, the window sits far off the screens: what's known from
    // before stays (it comes back to the same place)
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let origin = window.outer_position().unwrap_or_default();
    let on_screen = |[x, y, w, h]: [i32; 4]| [x + origin.x, y + origin.y, w, h];
    let main = main.map(on_screen);
    let (first, moved) = {
        let Ok(mut g) = geometry().lock() else { return };
        g.windows = windows.into_iter().map(on_screen).collect();
        let first = g.main.is_none() && main.is_some();
        let moved = g.main != main;
        g.main = main;
        (first, moved)
    };
    let app = window.app_handle().clone();
    if first {
        // the skin is up: Lala comes and sits on it (off this, the main,
        // thread: her window may have to be built)
        tauri::async_runtime::spawn(async move {
            let _ = crate::mascot::mascot_show(app);
        });
    } else if moved {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if moving == Some(true) {
                crate::mascot::player_moved(&app2);
            } else {
                crate::mascot::follow_player(&app2);
            }
        });
    }
}

/// The skin's main window on the screens (physical pixels), for the Now
/// Playing card's picture of it.
#[tauri::command]
pub fn modern_main_rect() -> Option<[i32; 4]> {
    main_rect()
}

/// The skin's windows on the screens (physical pixels), for Spotiamp+'s
/// windows to snap to; none in the classic look.
#[tauri::command]
pub fn modern_window_rects() -> Vec<[i32; 4]> {
    if !active() {
        return Vec::new();
    }
    geometry().lock().map(|g| g.windows.clone()).unwrap_or_default()
}

/// The skin's page made its look for the other windows: kept, and they're told.
#[tauri::command]
pub fn modern_set_look(app: AppHandle, look: serde_json::Value) {
    let Some(skin) = Settings::current().modern_skin.clone() else { return };
    if let Some(path) = look_path(&skin) {
        let text = look.to_string();
        std::thread::spawn(move || {
            if let Err(e) = std::fs::write(&path, text) {
                log::warn!("Modern skin look not kept: {e}");
            }
        });
    }
    if let Ok(mut l) = kept_look().lock() {
        *l = Some((skin, std::sync::Arc::new(look)));
    }
    let _ = app.emit("modernLook", ());
}

/// The worn modern skin's look for Spotiamp+'s windows (None in the classic
/// look, or before the skin has ever been drawn).
#[tauri::command]
pub fn modern_look() -> Option<std::sync::Arc<serde_json::Value>> {
    let skin = Settings::current().modern_skin.clone()?;
    if let Ok(l) = kept_look().lock()
        && let Some((worn, look)) = l.as_ref()
        && *worn == skin
    {
        return Some(look.clone());
    }
    let text = std::fs::read_to_string(look_path(&skin)?).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let value = std::sync::Arc::new(value);
    if let Ok(mut l) = kept_look().lock() {
        *l = Some((skin, value.clone()));
    }
    Some(value)
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
