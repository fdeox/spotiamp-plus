use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

use serde::Deserialize;
use tauri::{
    AppHandle, Emitter, Listener, LogicalPosition, Manager, PhysicalPosition, WebviewWindow,
};

use crate::settings::InnerWindowSize;

/// The UI scale every window renders at (1.0 = classic size), from settings.
pub fn ui_scale() -> f64 {
    let pct = crate::settings::Settings::current()
        .player
        .ui_scale_pct
        .unwrap_or(100)
        .clamp(100, 300);
    pct as f64 / 100.0
}

pub fn build_frameless_window(
    app: &AppHandle,
    label: &str,
    title: &str,
    route: &str,
    inner_size: InnerWindowSize,
) -> Result<WebviewWindow, tauri::Error> {
    // Sizes are stored at 1x; every window opens at the UI scale straight away,
    // and the page learns the same number before its first paint, so nothing
    // flashes at 1x first (the whole page is CSS-zoomed, see global.css).
    let scale = ui_scale();
    tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(route.into()))
        .title(title)
        .inner_size(
            inner_size.width as f64 * scale,
            inner_size.height as f64 * scale,
        )
        .initialization_script(format!("window.__SPOTIAMP_UI_SCALE__ = {scale};"))
        .decorations(false)
        .shadow(false)
        .closable(false)
        .maximizable(false)
        // Keep windows minimizable: a frameless, NON-minimizable window that
        // Windows minimizes anyway (Show Desktop / Win+D / a taskbar toggle)
        // gets stranded at (-32000,-32000) and can't be brought back by clicking
        // the taskbar. Minimizable windows minimize to the taskbar and restore
        // normally (the owned sub-windows follow the player).
        .minimizable(true)
        .resizable(false)
        .disable_drag_drop_handler()
        .accept_first_mouse(true)
        // Applied here so every window — including ones opened later — comes up
        // matching the user's always-on-top choice.
        .always_on_top(crate::settings::Settings::current().player.always_on_top)
        .build()
        .inspect(|window| {
            #[cfg(target_os = "windows")]
            watch_for_renderer_crash(window);
        })
}

/// The big "!" error page: when a window's WebView2 renderer process dies (seen
/// after a long idle / a sleep-resume) WebView2 swaps the page for its own error
/// screen and leaves it there until the app restarts. Catch that, log why (kind,
/// reason, exit code — tells a crash from an out-of-memory kill), and reload the
/// page so the window heals itself. Playback lives in Rust and isn't touched.
#[cfg(target_os = "windows")]
fn watch_for_renderer_crash(window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2ProcessFailedEventArgs2, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
        COREWEBVIEW2_PROCESS_FAILED_REASON,
    };
    use webview2_com::ProcessFailedEventHandler;
    use windows::core::Interface;

    let label = window.label().to_string();
    let result = window.with_webview(move |webview| unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        // A page that dies again straight after a reload would otherwise spin in
        // a reload loop; past the first retry in 10 s, leave it and just log.
        let mut last_reload: Option<std::time::Instant> = None;
        let handler = ProcessFailedEventHandler::create(Box::new(move |sender, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            let _ = args.ProcessFailedKind(&mut kind);
            let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
            let mut exit_code = 0i32;
            if let Ok(args2) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
                let _ = args2.Reason(&mut reason);
                let _ = args2.ExitCode(&mut exit_code);
            }
            // Only the page's own renderer is ours to restart; the GPU and other
            // helper processes are brought back by WebView2 itself.
            let ours = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE;
            let too_soon = last_reload
                .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(10));
            let reload = ours && !too_soon;
            log::error!(
                "[{label}] webview process failed: kind {} reason {} exit code {exit_code}{}",
                kind.0,
                reason.0,
                if reload {
                    ", reloading"
                } else if ours {
                    ", failed again right after a reload, leaving it"
                } else {
                    ""
                },
            );
            if reload {
                last_reload = Some(std::time::Instant::now());
                if let Some(core) = sender {
                    let _ = core.Reload();
                }
            }
            Ok(())
        }));
        let mut token = 0i64;
        if let Err(e) = core.add_ProcessFailed(&handler, &mut token) {
            log::warn!("could not watch the webview for crashes: {e}");
        }
    });
    if let Err(e) = result {
        log::warn!("could not reach the webview to watch for crashes: {e}");
    }
}

/// Toggle always-on-top across every open Spotiamp+ window, so a docked group
/// stays together instead of one window floating above the rest.
pub fn apply_always_on_top(app: &AppHandle, active: bool) {
    for (_, window) in app.webview_windows() {
        let _ = window.set_always_on_top(active);
    }
}

pub fn apply_position(window: &WebviewWindow, position: Option<LogicalPosition<i32>>) {
    if let Some(position) = position {
        let _ = window.set_position(position);
    }
}

/// Restore a remembered position for one of the on-demand windows (library /
/// visualizer / lyrics / eq) and keep saving it as it moves. Falls back to
/// `default_pos` the first time, before the user has placed it. Size is
/// restored separately, at build time, from `Settings::window_inner_size`.
pub fn restore_and_remember(window: &WebviewWindow, label: &'static str, default_pos: LogicalPosition<i32>) {
    let saved = crate::settings::Settings::current().window_position(label);
    apply_position(window, Some(saved.unwrap_or(default_pos)));
    remember_position(window, label, move |position| {
        crate::settings::Settings::current_mut().set_window_position(label, position);
    });
}

/// Persist an on-demand window's inner size (library / visualizer / lyrics),
/// called from the frontend when the user resizes it. Size lives in the
/// frontend (the resize grip drives `REACTIVE_WINDOW_SIZE`), so it comes back
/// over the bridge rather than being read here.
#[tauri::command]
pub fn set_window_inner_size(label: String, width: u32, height: u32) {
    crate::settings::Settings::current_mut()
        .set_window_inner_size(&label, crate::settings::InnerWindowSize { width, height });
}

/// The remembered inner size for a window, or None if it's never been resized —
/// each window reads this on mount so it opens at the size it was left, instead
/// of snapping back to its hardcoded default.
#[tauri::command]
pub fn get_window_inner_size(label: String) -> Option<crate::settings::InnerWindowSize> {
    crate::settings::Settings::current().window_inner_size(&label)
}

/// Labels of the on-demand windows (library / visualizer / lyrics) that were
/// open when the app was last used. The player reads this once it's up and
/// reopens each via the normal show command — post-launch, so it never touches
/// the fragile init-time window creation.
#[tauri::command]
pub fn windows_to_reopen() -> Vec<String> {
    crate::settings::Settings::current().open_windows()
}

pub fn remember_position(
    window: &WebviewWindow,
    scale_factor_context: &'static str,
    save_position: impl Fn(LogicalPosition<i32>) + Send + 'static,
) {
    let window = window.clone();
    window.clone().on_window_event(move |window_event| {
        if let tauri::WindowEvent::Moved(physical_position) = window_event {
            // Windows parks minimized windows at (-32000,-32000); persisting that
            // would relaunch the app off-screen ("it never appears"). Skip it.
            if physical_position.x <= -30000 || physical_position.y <= -30000 {
                return;
            }
            // A fullscreen visualizer sits at the monitor's corner; that isn't
            // where the window lives, and quitting in fullscreen would otherwise
            // reopen it there.
            if window.is_fullscreen().unwrap_or(false) {
                return;
            }
            save_position(
                physical_position.to_logical(
                    window.scale_factor().unwrap_or_else(|_| {
                        panic!("a scale factor for the {scale_factor_context}")
                    }),
                ),
            );
        }
    });
}

// ===========================================================================
// Docking
//
// A single process-wide manager keeps every frameless window snapped into one
// movable group, classic-Winamp style. The player is the master: dragging it
// moves the whole *connected* group in lockstep. On Windows a window-procedure
// subclass on the player repositions each group member inside the player's own
// move message — before the frame is painted — so the group moves with no
// trailing.
//
// Followers snap to any other window on the frontend (see
// `window-docking.svelte.js`). The group is recomputed live from the actual
// window positions every time a drag starts or ends, so it can never go stale —
// even after a window in the *middle* of a stack is dragged out, the next player
// drag re-derives exactly which windows are still attached.
// ===========================================================================

/// The master window that drags the whole group.
const MASTER: &str = "player";

#[derive(Clone, Copy)]
struct WindowRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl WindowRect {
    fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
}

#[derive(Deserialize)]
enum DockingWindowEvent {
    DragStarted,
    DragEnded,
    #[allow(dead_code)]
    VisibilityChanged { visible: bool },
}

#[derive(Default)]
struct Dock {
    windows: HashMap<String, WebviewWindow>,
    rects: HashMap<String, WindowRect>,
    visible: HashMap<String, bool>,
    /// Label of the window currently being dragged, if any.
    dragging: Option<String>,
    /// While the player is dragged, each other group member's fixed offset from
    /// the player, frozen at drag start so the whole group stays rigid.
    #[allow(dead_code)]
    group_offsets: HashMap<String, (i32, i32)>,
    /// Docked offset of a window that was hidden while docked, so it re-docks to
    /// the player when shown again.
    hidden_offset: HashMap<String, (i32, i32)>,
    #[cfg(target_os = "windows")]
    hwnds: HashMap<String, isize>,
    #[cfg(target_os = "windows")]
    owned_by_player: HashSet<String>,
}

fn dock() -> &'static Mutex<Dock> {
    static DOCK: OnceLock<Mutex<Dock>> = OnceLock::new();
    DOCK.get_or_init(|| Mutex::new(Dock::default()))
}

fn rect_of(window: &WebviewWindow) -> Option<WindowRect> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(WindowRect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// Pull live positions and visibility for every registered window. MUST run on
/// the main thread — the getters block until the main thread services them.
fn refresh_all_rects(dock: &mut Dock) {
    let labels: Vec<String> = dock.windows.keys().cloned().collect();
    for label in labels {
        let Some(window) = dock.windows.get(&label).cloned() else {
            continue;
        };
        if let Some(rect) = rect_of(&window) {
            dock.rects.insert(label.clone(), rect);
        }
        if let Ok(visible) = window.is_visible() {
            dock.visible.insert(label.clone(), visible);
        }
    }
}

/// Two windows are docked when one edge is flush (within a small tolerance) and
/// they overlap on the perpendicular axis.
fn adjacent(a: &WindowRect, b: &WindowRect) -> bool {
    const TOL: i32 = 3;
    let vertical_overlap = a.y < b.bottom() && b.y < a.bottom();
    let horizontal_overlap = a.x < b.right() && b.x < a.right();
    (vertical_overlap && ((a.right() - b.x).abs() <= TOL || (b.right() - a.x).abs() <= TOL))
        || (horizontal_overlap && ((a.bottom() - b.y).abs() <= TOL || (b.bottom() - a.y).abs() <= TOL))
}

/// Labels of every visible window transitively docked to `start` (inclusive).
fn connected_group(dock: &Dock, start: &str) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut stack = vec![start.to_string()];
    while let Some(current) = stack.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let Some(current_rect) = dock.rects.get(&current).copied() else {
            continue;
        };
        for (label, rect) in &dock.rects {
            if visited.contains(label) {
                continue;
            }
            if !dock.visible.get(label).copied().unwrap_or(false) {
                continue;
            }
            if adjacent(&current_rect, rect) {
                stack.push(label.clone());
            }
        }
    }
    visited
}

/// Freeze the player's connected group so it can be dragged as one rigid unit.
fn on_master_drag_started(dock: &mut Dock) {
    refresh_all_rects(dock);
    let group = connected_group(dock, MASTER);
    dock.group_offsets.clear();
    if let Some(player) = dock.rects.get(MASTER).copied() {
        for label in &group {
            if label == MASTER {
                continue;
            }
            if let Some(rect) = dock.rects.get(label) {
                dock.group_offsets
                    .insert(label.clone(), (rect.x - player.x, rect.y - player.y));
            }
        }
    }
    dock.dragging = Some(MASTER.to_string());
}

/// Any window was dropped: forget the drag and re-sync native ownership against
/// whatever is actually docked now.
fn on_drag_ended(dock: &mut Dock) {
    dock.dragging = None;
    dock.group_offsets.clear();
    refresh_all_rects(dock);
    #[cfg(target_os = "windows")]
    update_owners(dock);
}

#[cfg(target_os = "windows")]
fn set_owner(follower_hwnd: isize, owner_hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW};
    unsafe {
        SetWindowLongPtrW(
            HWND(follower_hwnd as *mut core::ffi::c_void),
            GWLP_HWNDPARENT,
            owner_hwnd,
        );
    }
}

/// Make every window in the player's group owned by the player — so the group
/// minimizes/restores together, stays above it and shares one taskbar button —
/// and release any window that has left the group. MUST run on the main thread.
#[cfg(target_os = "windows")]
fn update_owners(dock: &mut Dock) {
    let Some(&player_hwnd) = dock.hwnds.get(MASTER) else {
        return;
    };
    let group = connected_group(dock, MASTER);
    let labels: Vec<String> = dock.windows.keys().cloned().collect();
    for label in labels {
        if label == MASTER {
            continue;
        }
        let Some(&hwnd) = dock.hwnds.get(&label) else {
            continue;
        };
        let in_group =
            group.contains(&label) && dock.visible.get(&label).copied().unwrap_or(false);
        let currently_owned = dock.owned_by_player.contains(&label);
        if in_group && !currently_owned {
            set_owner(hwnd, player_hwnd);
            dock.owned_by_player.insert(label);
        } else if !in_group && currently_owned {
            set_owner(hwnd, 0);
            dock.owned_by_player.remove(&label);
        }
    }
}

/// Reposition every frozen group member to follow the player to `pos`. Called
/// from the player's `Moved` event, which fires on the main thread, so the
/// follower `set_position` calls run inline. `try_lock` (never a blocking lock)
/// avoids re-entrant deadlocks: a main-thread dock handler can hold the lock and
/// synchronously nudge the player; skipping is correct since outside a drag there
/// is nothing to move.
///
/// (A raw Win32 `WM_WINDOWPOSCHANGED` subclass would move the group with zero
/// trailing, but subclassing the player's window procedure at creation time races
/// with WebView2's own subclassing and wedges the UI thread — see git history.
/// `on_window_event` is the safe, supported path.)
fn move_group_with_master(pos: PhysicalPosition<i32>) {
    let moves: Vec<(WebviewWindow, PhysicalPosition<i32>)> = {
        let Ok(dock) = dock().try_lock() else {
            return;
        };
        if dock.dragging.as_deref() != Some(MASTER) {
            return;
        }
        dock.group_offsets
            .iter()
            .filter_map(|(label, (dx, dy))| {
                dock.windows
                    .get(label)
                    .map(|w| (w.clone(), PhysicalPosition::new(pos.x + dx, pos.y + dy)))
            })
            .collect()
    };
    for (window, position) in moves {
        let _ = window.set_position(position);
    }
}

/// Register a frameless window with the docking manager. Call once per window
/// after it is created. The player must be registered too — it is the master.
pub fn register_dock_window(window: &WebviewWindow) {
    let label = window.label().to_string();

    // Read the geometry BEFORE taking the lock: off the main thread these getters
    // block until the main thread services them, so holding the lock across them
    // could deadlock against the main-thread dock handlers.
    let rect = rect_of(window);
    let visible = window.is_visible().unwrap_or(true);
    {
        let mut dock = dock().lock().expect("docking state lock");
        dock.windows.insert(label.clone(), window.clone());
        if let Some(rect) = rect {
            dock.rects.insert(label.clone(), rect);
        }
        dock.visible.insert(label.clone(), visible);
    }

    // Record the native handle (needed for owner management), on the main thread.
    #[cfg(target_os = "windows")]
    {
        let window = window.clone();
        let label = label.clone();
        let _ = window.clone().run_on_main_thread(move || {
            if let Ok(hwnd) = window.hwnd() {
                dock()
                    .lock()
                    .expect("docking state lock")
                    .hwnds
                    .insert(label.clone(), hwnd.0 as isize);
            }
        });
    }

    // The player drags the whole group: whenever it moves, reposition every
    // frozen group member to follow. (Non-player windows move only themselves.)
    if label == MASTER {
        window.clone().on_window_event(move |event| {
            if let tauri::WindowEvent::Moved(position) = event {
                move_group_with_master(*position);
            }
        });
    }

    // React to this window's drag lifecycle (emitted by the frontend on
    // `<label>Window`). The listener runs on a worker thread where window
    // getters/setters block, so the work hops to the main thread and runs inline.
    let event_name = format!("{label}Window");
    let app_handle = window.app_handle().clone();
    let window = window.clone();
    app_handle.listen(event_name, move |event| {
        let Ok(parsed) = serde_json::from_str::<DockingWindowEvent>(event.payload()) else {
            return;
        };
        let label = window.label().to_string();
        let _ = window.clone().run_on_main_thread(move || {
            let mut dock = dock().lock().expect("docking state lock");
            match parsed {
                DockingWindowEvent::DragStarted => {
                    if label == MASTER {
                        on_master_drag_started(&mut dock);
                    } else {
                        dock.dragging = Some(label);
                    }
                }
                DockingWindowEvent::DragEnded => on_drag_ended(&mut dock),
                DockingWindowEvent::VisibilityChanged { .. } => {}
            }
        });
    });
}

/// Update a follower's docked visibility. Call after showing/hiding it. Remembers
/// a docked window's offset so it re-docks when shown again, and keeps native
/// ownership in sync. Safe to call from an async command (hops to the main
/// thread internally).
pub fn set_dock_visible(window: &WebviewWindow, visible: bool) {
    let window = window.clone();
    let label = window.label().to_string();
    let _ = window.clone().run_on_main_thread(move || {
        let mut dock = dock().lock().expect("docking state lock");
        if visible {
            dock.visible.insert(label.clone(), true);
            // Re-dock to the player if we remembered where it was docked.
            if let Some((dx, dy)) = dock.hidden_offset.remove(&label) {
                if let Some(player) = dock.rects.get(MASTER).copied() {
                    let _ = window.set_position(PhysicalPosition::new(player.x + dx, player.y + dy));
                }
            }
            refresh_all_rects(&mut dock);
        } else {
            // Remember the docked offset while the window is still considered
            // docked (before refresh marks it hidden), so it can return to the
            // same spot when shown again.
            if label != MASTER {
                let group = connected_group(&dock, MASTER);
                if group.contains(&label) {
                    if let (Some(player), Some(rect)) =
                        (dock.rects.get(MASTER).copied(), rect_of(&window))
                    {
                        dock.hidden_offset
                            .insert(label.clone(), (rect.x - player.x, rect.y - player.y));
                    }
                }
            }
            dock.visible.insert(label.clone(), false);
            refresh_all_rects(&mut dock);
        }
        #[cfg(target_os = "windows")]
        update_owners(&mut dock);
    });
}

/// The UI scale changed by `ratio` (new / old). Every window resizes itself
/// from its top-left corner, so to keep the player's docked stack lined up each
/// attached window's top-left is moved proportionally around the player's (its
/// size scales by the same ratio). The remembered offsets of hidden docked
/// windows scale too. Windows not attached to the player stay where they are and
/// just grow in place. MUST run on the main thread.
fn rescale_docked_group(ratio: f64) {
    let scale = |d: i32| (d as f64 * ratio).round() as i32;
    let moves: Vec<(WebviewWindow, PhysicalPosition<i32>)> = {
        let mut dock = dock().lock().expect("docking state lock");
        refresh_all_rects(&mut dock);
        let Some(player) = dock.rects.get(MASTER).copied() else {
            return;
        };
        for offset in dock.hidden_offset.values_mut() {
            *offset = (scale(offset.0), scale(offset.1));
        }
        connected_group(&dock, MASTER)
            .iter()
            .filter(|label| label.as_str() != MASTER)
            .filter_map(|label| {
                let rect = dock.rects.get(label)?;
                let window = dock.windows.get(label)?.clone();
                let position = PhysicalPosition::new(
                    player.x + scale(rect.x - player.x),
                    player.y + scale(rect.y - player.y),
                );
                Some((window, position))
            })
            .collect()
    };
    for (window, position) in moves {
        let _ = window.set_position(position);
    }
}

/// Current UI scale, for a page that reloaded after the scale changed (its
/// startup script still carries the value from when the window was created).
#[tauri::command]
pub fn get_ui_scale() -> f64 {
    ui_scale()
}

/// Scale every window: 100 = classic size, 150, 200 (Winamp's double size), 300.
/// Saves it, keeps the docked stack together, then tells every page to re-zoom
/// (each resizes its own window to match).
#[tauri::command]
pub fn set_ui_scale(pct: u16, app_handle: AppHandle) {
    let old = ui_scale();
    crate::settings::Settings::current_mut().player.ui_scale_pct = Some(pct.clamp(100, 300));
    let new = ui_scale();
    if (new - old).abs() < 1e-6 {
        return;
    }
    let app = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        rescale_docked_group(new / old);
        let _ = app.emit("uiScale", serde_json::json!({ "scale": new }));
    });
    // The pages resize themselves asynchronously; once they've settled, re-derive
    // the docked group and native ownership from where everything ended up.
    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let _ = app.run_on_main_thread(|| {
            let mut dock = dock().lock().expect("docking state lock");
            on_drag_ended(&mut dock);
        });
    });
}
