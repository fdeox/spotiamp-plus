//! Taskbar extras (opt-in, right-click the playlist → Windows): the song on
//! the taskbar button and in Alt+Tab, the track's progress drawn across the
//! taskbar button, and ⏮ ⏯ ⏭ buttons under its hover thumbnail — the things
//! Winamp did on Windows 7 and later. Off by default.
//!
//! Nothing here runs at window creation: the first update after launch (the
//! player sends one a second) sets it up, on the main thread, and only when
//! the setting is on. Hooking the player's window procedure at creation time
//! races WebView2's own hook and can wedge the UI (see app_window.rs).

use tauri::AppHandle;

use crate::settings::Settings;

/// Turn the extras on or off (persisted). Off puts the plain title back and
/// removes the progress bar and the thumbnail buttons.
#[tauri::command]
pub fn set_taskbar_extras(enabled: bool, app_handle: AppHandle) {
    Settings::current_mut().player.taskbar_extras = enabled;
    let app = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        #[cfg(target_os = "windows")]
        imp::set_enabled(&app, enabled);
        #[cfg(not(target_os = "windows"))]
        let _ = (app, enabled);
    });
}

/// The player's once-a-second report: "Artist - Title", its state
/// ("playing" / "paused" / "stopped") and where it is. Ignored while off.
#[tauri::command]
pub fn taskbar_update(
    title: String,
    state: String,
    position_ms: u32,
    duration_ms: u32,
    app_handle: AppHandle,
) {
    if !Settings::current().player.taskbar_extras {
        return;
    }
    let app = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        #[cfg(target_os = "windows")]
        imp::update(&app, &title, &state, position_ms, duration_ms);
        #[cfg(not(target_os = "windows"))]
        let _ = (app, title, state, position_ms, duration_ms);
    });
}

#[cfg(target_os = "windows")]
mod imp {
    use std::cell::RefCell;
    use std::sync::OnceLock;

    use tauri::{AppHandle, Emitter, Manager};
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
        DeleteObject,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Shell::{
        DefSubclassProc, ITaskbarList3, SetWindowSubclass, TBPF_NOPROGRESS, TBPF_NORMAL,
        TBPF_PAUSED, TBPFLAG, THB_FLAGS, THB_ICON, THB_TOOLTIP, THBF_ENABLED, THBF_HIDDEN,
        THBN_CLICKED, THUMBBUTTON, THUMBBUTTONFLAGS, THUMBBUTTONMASK, TaskbarList,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, GetSystemMetrics, HICON, ICONINFO, RegisterWindowMessageW,
        SM_CXSMICON, WM_COMMAND,
    };

    const PLAIN_TITLE: &str = "Spotiamp+";
    const SUBCLASS_ID: usize = 0x5350_4150;
    const BTN_PREV: u32 = 1;
    const BTN_PLAY: u32 = 2;
    const BTN_NEXT: u32 = 3;

    struct Icons {
        prev: HICON,
        play: HICON,
        pause: HICON,
        next: HICON,
    }

    struct State {
        list: ITaskbarList3,
        hwnd: HWND,
        icons: Icons,
        enabled: bool,
        buttons_added: bool,
        /// What the play button shows now, to only touch it on a change.
        playing: Option<bool>,
        last_title: String,
        last_state: String,
    }

    // COM objects and window handles belong to the main thread; everything
    // here runs there, so a thread-local holds them.
    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }
    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// Explorer broadcasts this after it (re)creates our taskbar button — on
    /// a restart of Explorer the thumbnail buttons are gone and must be re-added.
    static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

    pub fn set_enabled(app: &AppHandle, enabled: bool) {
        if enabled {
            if ensure_init(app) {
                with_state(|s| {
                    s.enabled = true;
                    s.last_title.clear();
                    s.last_state.clear();
                    show_buttons(s, true);
                });
            }
            return;
        }
        with_state(|s| {
            s.enabled = false;
            unsafe {
                let _ = s.list.SetProgressState(s.hwnd, TBPF_NOPROGRESS);
            }
            show_buttons(s, false);
            s.last_title.clear();
            s.last_state.clear();
        });
        if let Some(player) = app.get_webview_window("player") {
            let _ = player.set_title(PLAIN_TITLE);
        }
    }

    pub fn update(app: &AppHandle, title: &str, state: &str, position_ms: u32, duration_ms: u32) {
        if !ensure_init(app) {
            return;
        }
        let mut new_title = None;
        with_state(|s| {
            if !s.enabled {
                s.enabled = true;
                show_buttons(s, true);
            }
            if !s.buttons_added {
                show_buttons(s, true);
            }
            // Winamp's style: the song, the app, and the state when not playing.
            let full = if title.is_empty() {
                PLAIN_TITLE.to_string()
            } else {
                match state {
                    "playing" => format!("{title} - {PLAIN_TITLE}"),
                    "paused" => format!("{title} - {PLAIN_TITLE} [Paused]"),
                    _ => format!("{title} - {PLAIN_TITLE} [Stopped]"),
                }
            };
            if full != s.last_title {
                s.last_title = full.clone();
                new_title = Some(full);
            }
            let flag: TBPFLAG = match state {
                "playing" => TBPF_NORMAL,
                "paused" => TBPF_PAUSED,
                _ => TBPF_NOPROGRESS,
            };
            unsafe {
                if state != s.last_state {
                    let _ = s.list.SetProgressState(s.hwnd, flag);
                }
                if flag != TBPF_NOPROGRESS && duration_ms > 0 {
                    let _ = s.list.SetProgressValue(
                        s.hwnd,
                        position_ms.min(duration_ms) as u64,
                        duration_ms as u64,
                    );
                }
            }
            s.last_state = state.to_string();
            let playing = state == "playing";
            if s.playing != Some(playing) {
                s.playing = Some(playing);
                if s.buttons_added {
                    let buttons = buttons(&s.icons, playing, true);
                    unsafe {
                        let _ = s.list.ThumbBarUpdateButtons(s.hwnd, &buttons);
                    }
                }
            }
        });
        // Outside the state borrow: setting the title sends WM_SETTEXT to the
        // window straight away.
        if let (Some(full), Some(player)) = (new_title, app.get_webview_window("player")) {
            let _ = player.set_title(&full);
        }
    }

    fn with_state(f: impl FnOnce(&mut State)) {
        STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut()
                && let Some(state) = guard.as_mut()
            {
                f(state);
            }
        });
    }

    /// Create the taskbar COM object, the icons and the click hook, once.
    fn ensure_init(app: &AppHandle) -> bool {
        let ready = STATE.with(|cell| cell.borrow().is_some());
        if ready {
            return true;
        }
        let Some(player) = app.get_webview_window("player") else {
            return false;
        };
        let Ok(hwnd) = player.hwnd() else {
            return false;
        };
        let _ = APP.set(app.clone());
        let init = || -> windows::core::Result<State> {
            unsafe {
                // Already initialised on this thread by the runtime; harmless.
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let list: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
                list.HrInit()?;
                let _ = TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarButtonCreated")));
                let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
                Ok(State {
                    list,
                    hwnd,
                    icons: make_icons(),
                    enabled: false,
                    buttons_added: false,
                    playing: None,
                    last_title: String::new(),
                    last_state: String::new(),
                })
            }
        };
        match init() {
            Ok(state) => {
                STATE.with(|cell| *cell.borrow_mut() = Some(state));
                true
            }
            Err(e) => {
                log::warn!("taskbar extras unavailable: {e}");
                false
            }
        }
    }

    fn buttons(icons: &Icons, playing: bool, visible: bool) -> [THUMBBUTTON; 3] {
        let flags: THUMBBUTTONFLAGS = if visible { THBF_ENABLED } else { THBF_HIDDEN };
        let make = |id: u32, icon: HICON, tip: &str| {
            let mut b = THUMBBUTTON {
                dwMask: THUMBBUTTONMASK(THB_ICON.0 | THB_TOOLTIP.0 | THB_FLAGS.0),
                iId: id,
                hIcon: icon,
                dwFlags: flags,
                ..Default::default()
            };
            for (i, c) in tip.encode_utf16().take(b.szTip.len() - 1).enumerate() {
                b.szTip[i] = c;
            }
            b
        };
        [
            make(BTN_PREV, icons.prev, "Previous"),
            if playing {
                make(BTN_PLAY, icons.pause, "Pause")
            } else {
                make(BTN_PLAY, icons.play, "Play")
            },
            make(BTN_NEXT, icons.next, "Next"),
        ]
    }

    /// Add the thumbnail buttons the first time (they can only be added once
    /// per taskbar button), then show or hide them.
    fn show_buttons(s: &mut State, visible: bool) {
        let playing = s.playing.unwrap_or(false);
        let buttons = buttons(&s.icons, playing, visible);
        unsafe {
            if !s.buttons_added {
                if !visible {
                    return;
                }
                if s.list.ThumbBarAddButtons(s.hwnd, &buttons).is_ok() {
                    s.buttons_added = true;
                }
            } else {
                let _ = s.list.ThumbBarUpdateButtons(s.hwnd, &buttons);
            }
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_COMMAND && ((wparam.0 >> 16) & 0xffff) as u32 == THBN_CLICKED {
            let action = match (wparam.0 & 0xffff) as u32 {
                BTN_PREV => Some("previous"),
                BTN_PLAY => Some("playpause"),
                BTN_NEXT => Some("next"),
                _ => None,
            };
            if let (Some(action), Some(app)) = (action, APP.get()) {
                // Same event the media keys use, so a click behaves exactly
                // like the key. Sent off the window procedure.
                if let Some(player) = app.get_webview_window("player") {
                    tauri::async_runtime::spawn(async move {
                        let _ = player.emit("mediaKey", action);
                    });
                }
                return LRESULT(0);
            }
        }
        if TASKBAR_CREATED.get() == Some(&msg) {
            with_state(|s| {
                s.buttons_added = false;
                s.last_state.clear();
                if s.enabled {
                    show_buttons(s, true);
                }
            });
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    // --- icons, drawn here so there are no image files to ship --------------

    enum Shape {
        /// x0, y0, x1, y1 on a 16x16 grid
        Rect(f32, f32, f32, f32),
        Tri([(f32, f32); 3]),
    }

    fn inside(shapes: &[Shape], x: f32, y: f32) -> bool {
        shapes.iter().any(|shape| match shape {
            Shape::Rect(x0, y0, x1, y1) => x >= *x0 && x < *x1 && y >= *y0 && y < *y1,
            Shape::Tri([a, b, c]) => {
                let side = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
                let (d1, d2, d3) = (side(*a, *b), side(*b, *c), side(*c, *a));
                let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
                let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
                !(neg && pos)
            }
        })
    }

    fn make_icons() -> Icons {
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64);
        Icons {
            prev: make_icon(size, &[
                Shape::Rect(3.0, 3.5, 5.0, 12.5),
                Shape::Tri([(13.0, 3.5), (13.0, 12.5), (5.5, 8.0)]),
            ]),
            play: make_icon(size, &[Shape::Tri([(4.5, 3.0), (4.5, 13.0), (12.5, 8.0)])]),
            pause: make_icon(size, &[
                Shape::Rect(4.0, 3.0, 7.0, 13.0),
                Shape::Rect(9.0, 3.0, 12.0, 13.0),
            ]),
            next: make_icon(size, &[
                Shape::Tri([(3.0, 3.5), (3.0, 12.5), (10.5, 8.0)]),
                Shape::Rect(11.0, 3.5, 13.0, 12.5),
            ]),
        }
    }

    /// A white glyph with smooth edges (4x4 samples a pixel) as a 32-bit icon.
    fn make_icon(size: i32, shapes: &[Shape]) -> HICON {
        let n = size as usize;
        let mut pixels = vec![0u8; n * n * 4];
        let scale = 16.0 / size as f32;
        for py in 0..n {
            for px in 0..n {
                let mut hits = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let x = (px as f32 + (sx as f32 + 0.5) / 4.0) * scale;
                        let y = (py as f32 + (sy as f32 + 0.5) / 4.0) * scale;
                        if inside(shapes, x, y) {
                            hits += 1;
                        }
                    }
                }
                let i = (py * n + px) * 4;
                pixels[i..i + 4].copy_from_slice(&[255, 255, 255, (hits * 255 / 16) as u8]);
            }
        }
        unsafe {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size,
                    biHeight: -size, // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(color) = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                return HICON::default();
            };
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
            // The alpha channel does the masking; the mask just has to exist.
            let mask_row = n.div_ceil(16) * 2;
            let mask_bits = vec![0u8; mask_row * n];
            let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr() as *const _));
            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: true.into(),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            })
            .unwrap_or_default();
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(mask.into());
            icon
        }
    }
}
