//! Lala the llama: Spotiamp+'s mascot, sitting on the player's top edge. It
//! dances while music plays, falls asleep when it stops and reacts to a few
//! things (a new badge, a loved song, a song that won't load); the mascot page
//! does all of that. This side owns its window: a small transparent one that
//! the player owns (so it stays above it and minimizes with it) and that
//! follows the player around. On by default; the Windows menu switches it off
//! and picks its size.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use crate::settings::Settings;

/// One animation frame, at the art's own size (a 64 px tall llama).
const FRAME_W: f64 = 80.0;
const FRAME_H: f64 = 84.0;
/// Empty rows under its feet in a frame, so it sits right on the player.
const FEET_GAP: f64 = 5.0;
/// How far in from the player's right edge it sits by default (1x px).
const RIGHT_INSET: f64 = 22.0;

/// Its spot while it's being dragged (1x px from the player's right end);
/// saved to settings only when the drag ends, not on every step.
fn dragged_inset() -> &'static Mutex<Option<f64>> {
    static INSET: Mutex<Option<f64>> = Mutex::new(None);
    &INSET
}

fn inset() -> f64 {
    if let Some(i) = dragged_inset().lock().ok().and_then(|i| *i) {
        return i;
    }
    if visiting() {
        return visit_inset().lock().map(|v| *v).unwrap_or(RIGHT_INSET);
    }
    Settings::current().player.mascot_inset.map(f64::from).unwrap_or(RIGHT_INSET)
}

// --- Visits: she can go and sit on another window for a while ---------------

/// The window she's visiting ("" = home, on the player).
fn host() -> &'static Mutex<String> {
    static HOST: Mutex<String> = Mutex::new(String::new());
    &HOST
}

/// Her spot on the window she's visiting (1x px from its right end); her
/// spot on the player stays the saved one.
fn visit_inset() -> &'static Mutex<f64> {
    static INSET: Mutex<f64> = Mutex::new(RIGHT_INSET);
    &INSET
}

fn visiting() -> bool {
    host().lock().map(|h| !h.is_empty()).unwrap_or(false)
}

/// The window she's sitting on: the one she's visiting while it's open,
/// else the player.
fn host_window(app: &AppHandle) -> Option<WebviewWindow> {
    let label = host().lock().map(|h| h.clone()).unwrap_or_default();
    if !label.is_empty() {
        if let Some(window) = app.get_webview_window(&label).filter(|w| w.is_visible().unwrap_or(false)) {
            return Some(window);
        }
        // it closed under her: she's home
        if let Ok(mut h) = host().lock() {
            h.clear();
        }
    }
    app.get_webview_window("player")
}

/// A window moved or changed size: if she's on it, she comes along.
pub fn host_moved(app: &AppHandle, label: &str) {
    if host().lock().map(|h| h.as_str() == label).unwrap_or(false) {
        follow_player(app);
    }
}

/// The open windows she could go and sit on: room above their top edge, on
/// a screen and not covered by another of ours.
#[tauri::command]
pub fn mascot_hosts(app: AppHandle) -> Vec<String> {
    const OURS: [&str; 8] = ["player", "playlist", "eq", "library", "lyrics", "visualizer", "art", "stats"];
    let rects: Vec<(&str, (i32, i32, i32, i32), WebviewWindow)> = OURS
        .iter()
        .filter_map(|label| {
            let window = app.get_webview_window(label)?;
            if !window.is_visible().ok()? {
                return None;
            }
            let (p, s) = (window.outer_position().ok()?, window.outer_size().ok()?);
            Some((*label, (p.x, p.y, s.width as i32, s.height as i32), window))
        })
        .collect();
    let mut hosts = Vec::new();
    for (label, (x, y, w, _), window) in &rects {
        if *label == "player" {
            continue;
        }
        let sf = window.scale_factor().unwrap_or(1.0);
        let need = (logical_size(current().size).1 * sf).round() as i32;
        let (top, bottom) = (y - need, *y);
        let cx = x + w / 2;
        let on_screen = window.available_monitors().is_ok_and(|monitors| {
            monitors.iter().any(|m| {
                let a = m.work_area();
                cx >= a.position.x
                    && cx < a.position.x + a.size.width as i32
                    && top >= a.position.y
                    && top < a.position.y + a.size.height as i32
            })
        });
        let covered = rects.iter().any(|(other, (ox, oy, ow, oh), _)| {
            other != label && *ox < x + w && ox + ow > *x && *oy < bottom && oy + oh > top
        });
        if on_screen && !covered {
            hosts.push(label.to_string());
        }
    }
    hosts
}

/// Go and sit on another window (`label`), or back home on the player
/// (None). Returns whether she went.
#[tauri::command]
pub fn mascot_visit(app: AppHandle, label: Option<String>) -> bool {
    match label {
        Some(label) => {
            if !mascot_hosts(app.clone()).contains(&label) {
                return false;
            }
            // somewhere along its top edge, not always the same spot
            if let Some(window) = app.get_webview_window(&label)
                && let (Ok(size), Ok(sf)) = (window.outer_size(), window.scale_factor())
            {
                let ui = crate::app_window::ui_scale();
                let llama = FRAME_W * current().size as f64 / 64.0;
                let room = (size.width as f64 / (ui * sf) - llama).max(0.0);
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                let pick = 0.2 + 0.6 * (nanos % 1000) as f64 / 1000.0;
                if let Ok(mut inset) = visit_inset().lock() {
                    *inset = room * pick;
                }
            }
            if let Ok(mut h) = host().lock() {
                *h = label;
            }
        }
        None => {
            if let Ok(mut h) = host().lock() {
                h.clear();
            }
        }
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        follow_player(&app2);
        raise(&app2);
    });
    true
}

/// Above the other windows (theirs and ours), without taking the focus: on
/// a visited window her feet would otherwise tuck under its title bar.
fn raise(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    if let Some(mascot) = app.get_webview_window("mascot")
        && let Ok(hwnd) = mascot.hwnd()
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}
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

/// Extra room to its left for a speech bubble, in the art's own px (0 = none).
fn bubble() -> &'static Mutex<f64> {
    static BUBBLE: Mutex<f64> = Mutex::new(0.0);
    &BUBBLE
}

/// Window size in logical px: the frame (plus any speech bubble) scaled to
/// the chosen height, times the UI scale like every other window.
fn logical_size(size: u16) -> (f64, f64) {
    let k = size as f64 / 64.0 * crate::app_window::ui_scale();
    let extra = bubble().lock().map(|b| *b).unwrap_or(0.0);
    (((FRAME_W + extra) * k).round(), (FRAME_H * k).round())
}

fn build(app: &AppHandle, size: u16) -> Result<WebviewWindow, tauri::Error> {
    let (w, h) = logical_size(size);
    tauri::WebviewWindowBuilder::new(app, "mascot", tauri::WebviewUrl::App("mascot".into()))
        .title("Spotiamp+ Lala")
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

/// Where the window goes, in physical px: its size (the frame plus any speech
/// bubble) and its spot on the player's top edge (near the right end, or
/// wherever she was dragged or walked to).
/// Where she goes: x, y, width, height, and whether there's room for her at
/// all (none when the player is at the very top of the screen).
fn bounds(app: &AppHandle) -> Option<(i32, i32, u32, u32, bool)> {
    // the player, or the window she's visiting
    let player = host_window(app)?;
    let pos = player.outer_position().ok()?;
    let psize = player.outer_size().ok()?;
    let sf = player.scale_factor().ok()?;
    let ui = crate::app_window::ui_scale();
    let size = current().size;
    let (lw, lh) = logical_size(size);
    let (w, h) = ((lw * sf).round() as u32, (lh * sf).round() as u32);
    let k = size as f64 / 64.0 * ui * sf;
    // along the top edge, never past either end (the llama itself; a speech
    // bubble may hang past the player's left end)
    let max_inset = ((psize.width as f64 - FRAME_W * k) / (ui * sf)).max(0.0);
    let x = pos.x + psize.width as i32 - w as i32 - (inset().clamp(0.0, max_inset) * ui * sf).round() as i32;
    let y = pos.y - h as i32 + (FEET_GAP * k).round() as i32;
    // Room above the player? Her head must be on a screen. (She used to be
    // pushed down onto the player then, sitting on its display.)
    let (cx, top) = (x + w as i32 / 2, y + h as i32 / 4);
    let room = player.available_monitors().map_or(true, |monitors| {
        monitors.iter().any(|m| {
            let a = m.work_area();
            cx >= a.position.x
                && cx < a.position.x + a.size.width as i32
                && top >= a.position.y
                && top < a.position.y + a.size.height as i32
        })
    });
    Some((x, y, w, h, room))
}

/// She was hidden for want of room, and comes back once there is.
static PARKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Move (and with `resize`, size) the window in one go: two separate steps
/// showed a frame of it in between, a little jump whenever a speech bubble
/// came or went. With no room above the player she's hidden until there is.
/// Returns whether she has room. MUST run on the main thread.
fn apply_bounds(app: &AppHandle, mascot: &WebviewWindow, resize: bool) -> bool {
    let Some((x, y, w, h, room)) = bounds(app) else {
        return true;
    };
    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = mascot.hwnd() {
        use std::sync::atomic::Ordering;
        use windows::Win32::UI::WindowsAndMessaging::{
            IsWindowVisible, SET_WINDOW_POS_FLAGS, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE,
            SWP_NOZORDER, SetWindowPos, ShowWindow,
        };
        if !room {
            if unsafe { IsWindowVisible(hwnd) }.as_bool() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                PARKED.store(true, Ordering::SeqCst);
            }
            return false;
        }
        let flags = SWP_NOZORDER | SWP_NOACTIVATE | if resize { SET_WINDOW_POS_FLAGS(0) } else { SWP_NOSIZE };
        unsafe {
            let _ = SetWindowPos(hwnd, None, x, y, w as i32, h as i32, flags);
        }
        // Landing on a screen with another scale, Windows resizes it once
        // more by itself (she came out 1.25x too big on a 125 % screen): set
        // the size again now that she's there.
        if resize && mascot.outer_size().is_ok_and(|s| (s.width, s.height) != (w, h)) {
            unsafe {
                let _ = SetWindowPos(hwnd, None, x, y, w as i32, h as i32, flags);
            }
        }
        // room again: back she comes, without taking the focus
        if PARKED.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        return true;
    }
    if resize {
        let _ = mascot.set_size(tauri::PhysicalSize::new(w, h));
    }
    let _ = mascot.set_position(PhysicalPosition::new(x, y));
    room
}

/// Windows keeps a new window at least ~136 px wide unless told the minimum,
/// and only takes it once the window exists. The minimum also gets the
/// invisible frame added on top, so it must stay well under her size or the
/// window comes out bigger than asked (and she sat lower and further right).
fn allow_size(mascot: &WebviewWindow) {
    let _ = mascot.set_min_size(Some(tauri::LogicalSize::new(1.0, 1.0)));
}

fn place(app: &AppHandle, mascot: &WebviewWindow) {
    // a move can take her to a screen with another scale, which resizes her;
    // only then is the size set too
    let resize = bounds(app).is_some_and(|(_, _, w, h, _)| mascot.outer_size().is_ok_and(|s| (s.width, s.height) != (w, h)));
    apply_bounds(app, mascot, resize);
}

/// The player moved or changed size: bring the llama along. Called from the
/// player's window events, on the main thread.
pub fn follow_player(app: &AppHandle) {
    if let Some(mascot) = app.get_webview_window("mascot") {
        place(app, &mascot);
    }
}

/// The player is being dragged: bring her along, and let her page know so
/// she holds on (a startled face) while it moves.
pub fn player_moved(app: &AppHandle) {
    if let Some(mascot) = app.get_webview_window("mascot") {
        place(app, &mascot);
        let _ = mascot.emit_to("mascot", "mascotRide", ());
    }
}

/// Size and spot again (the UI scale changed).
pub fn refresh(app: &AppHandle) {
    if let Some(mascot) = app.get_webview_window("mascot") {
        allow_size(&mascot);
        apply_bounds(app, &mascot, true);
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
        allow_size(&mascot);
        // no room above the player right now: she waits, hidden, until there is
        if !apply_bounds(&app, &mascot, true) {
            PARKED.store(true, std::sync::atomic::Ordering::SeqCst);
            return;
        }
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

/// Where it was when the drag started (1x px from the right end).
fn drag_start() -> &'static Mutex<Option<f64>> {
    static START: Mutex<Option<f64>> = Mutex::new(None);
    &START
}

/// Dragging it along the player's top edge: `dx` is how far the pointer has
/// moved since it was pressed (logical px, + to the right). `done` ends the
/// drag and remembers the spot.
#[tauri::command]
pub fn mascot_drag(app: AppHandle, dx: f64, done: bool) {
    let ui = crate::app_window::ui_scale();
    let start = {
        let Ok(mut s) = drag_start().lock() else { return };
        let start = *s.get_or_insert_with(inset);
        if done {
            *s = None;
        }
        start
    };
    let next = (start - dx / ui).max(0.0);
    if let Ok(mut d) = dragged_inset().lock() {
        *d = if done { None } else { Some(next) };
    }
    if done {
        // her spot on the player is kept; on a visited window, just for now
        if visiting() {
            if let Ok(mut inset) = visit_inset().lock() {
                *inset = next;
            }
        } else {
            Settings::current_mut().player.mascot_inset = Some(next.round() as u16);
        }
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || follow_player(&app2));
}

/// Make room for a speech bubble to its left (`width` in the art's px), or
/// give it back (0). The llama keeps its spot: the window grows leftwards.
#[tauri::command]
pub fn mascot_bubble(app: AppHandle, width: f64) {
    if let Ok(mut b) = bubble().lock() {
        *b = width.clamp(0.0, 400.0);
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || refresh(&app2));
}

/// How far she can stroll from where she sits, in logical px: (to the left,
/// to the right) along the player's top edge.
#[tauri::command]
pub fn mascot_room(app: AppHandle) -> (f64, f64) {
    let (Some(player), Some(_)) = (host_window(&app), app.get_webview_window("mascot")) else {
        return (0.0, 0.0);
    };
    let (Ok(psize), Ok(sf)) = (player.outer_size(), player.scale_factor()) else {
        return (0.0, 0.0);
    };
    let ui = crate::app_window::ui_scale();
    let llama_w = FRAME_W * current().size as f64 / 64.0 * ui * sf;
    let max_inset = ((psize.width as f64 - llama_w) / (ui * sf)).max(0.0);
    let now = inset().clamp(0.0, max_inset);
    // inset counts from the right end: walking left raises it
    ((max_inset - now) * ui, now * ui)
}
