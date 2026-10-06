//! The MilkDrop visualizer: projectM (libprojectM 4, a MilkDrop engine) drawing
//! with OpenGL into a window of its own laid over the visualizer's canvas.
//!
//! - projectM-4.dll and glew32.dll ship with the app (scripts/build-projectm.mjs)
//!   and are loaded at runtime: without them MilkDrop is only unavailable.
//! - That window is OWNED by the visualizer window, not a child of it: the
//!   WebView (Chromium) composes its page in a layer of its own above anything
//!   drawn into the window's surface, so a child window's picture never showed.
//!   An owned window has its own surface and always sits above its owner; it
//!   follows the visualizer when that moves (app_window calls `follow`).
//! - It's made, moved and destroyed on the main thread; one render thread owns
//!   the GL context and the projectM instance.
//! - Audio: what the visualizer hears (Spotify and local files, or in Free
//!   Mode the system loopback), fed in only while MilkDrop is on.
//! - Clicks on it are passed to the page as events, so the visualizer's own
//!   click (next) and double-click (fullscreen) keep working.
//! - The bottom line (the preset's name, MILKDROP and PIN) is drawn over the
//!   picture here, as the page draws it over its own canvas.
#![cfg(target_os = "windows")]

use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetDC, HDC, ReleaseDC, ScreenToClient};
use windows::Win32::Graphics::OpenGL::{
    ChoosePixelFormat, GL_ALPHA, GL_BACK, GL_BLEND, GL_CULL_FACE, GL_DEPTH_TEST, GL_LINEAR, GL_MODELVIEW, GL_MODULATE,
    GL_ONE_MINUS_SRC_ALPHA, GL_PROJECTION, GL_QUADS, GL_RGBA, GL_SCISSOR_TEST, GL_SRC_ALPHA, GL_TEXTURE_2D,
    GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_TEXTURE_MAG_FILTER, GL_TEXTURE_MIN_FILTER, GL_TEXTURE_WRAP_S,
    GL_TEXTURE_WRAP_T, GL_UNPACK_ALIGNMENT, GL_UNSIGNED_BYTE, HGLRC, PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW,
    PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR, SetPixelFormat, SwapBuffers, glBegin,
    glBindTexture, glBlendFunc, glColor4f, glDeleteTextures, glDisable, glEnable, glEnd, glFinish, glGenTextures,
    glLoadIdentity, glMatrixMode, glOrtho, glPixelStorei, glReadBuffer, glReadPixels, glTexCoord2f, glTexEnvi,
    glTexImage2D, glTexParameteri, glVertex2f, glViewport, wglCreateContext, wglDeleteContext, wglGetProcAddress,
    wglMakeCurrent,
};
use windows::Win32::Storage::FileSystem::GetShortPathNameW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CS_OWNDC, CreateWindowExW, DefWindowProcW, DestroyWindow, GW_OWNER, GetCursorPos, GetForegroundWindow,
    GetWindow, HCURSOR, HWND_TOP, IDC_ARROW, IDC_HAND, IsIconic, IsWindow, IsWindowVisible, LoadCursorW, MA_NOACTIVATE, RegisterClassW,
    SW_HIDE, SW_SHOWNA, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursor, SetForegroundWindow, SetWindowPos, ShowWindow,
    WM_ERASEBKGND, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_RBUTTONUP, WM_SETCURSOR,
    WNDCLASSW, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, s, w};

// --- the engine, loaded at runtime ---------------------------------------------

type Handle = *mut c_void;
type SwitchRequested = unsafe extern "C" fn(bool, *mut c_void);
type SwitchFailed = unsafe extern "C" fn(*const c_char, *const c_char, *mut c_void);

struct Api {
    glew_init: unsafe extern "C" fn() -> u32,
    create: unsafe extern "C" fn() -> Handle,
    destroy: unsafe extern "C" fn(Handle),
    load_preset_data: unsafe extern "C" fn(Handle, *const c_char, bool),
    pcm_add_float: unsafe extern "C" fn(Handle, *const f32, u32, u32),
    pcm_max_samples: unsafe extern "C" fn() -> u32,
    render_frame: unsafe extern "C" fn(Handle),
    set_window_size: unsafe extern "C" fn(Handle, usize, usize),
    set_preset_duration: unsafe extern "C" fn(Handle, f64),
    set_soft_cut_duration: unsafe extern "C" fn(Handle, f64),
    set_hard_cut_enabled: unsafe extern "C" fn(Handle, bool),
    set_hard_cut_duration: unsafe extern "C" fn(Handle, f64),
    on_switch_requested: unsafe extern "C" fn(Handle, Option<SwitchRequested>, *mut c_void),
    on_switch_failed: unsafe extern "C" fn(Handle, Option<SwitchFailed>, *mut c_void),
    set_texture_search_paths: unsafe extern "C" fn(Handle, *const *const c_char, usize),
    // kept loaded for as long as the functions above are used: the process
    _glew: Library,
    _projectm: Library,
}

// the function pointers are plain C entry points, callable from any thread
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

fn load_api(dir: &Path) -> Result<Api, String> {
    // glew32 first, by full path: projectM-4.dll then finds it already loaded
    let open = |name: &str| unsafe {
        Library::load_with_flags(dir.join(name), LOAD_WITH_ALTERED_SEARCH_PATH)
            .map_err(|e| format!("{name}: {e}"))
    };
    let glew = open("glew32.dll")?;
    let projectm = open("projectM-4.dll")?;
    unsafe {
        macro_rules! sym {
            ($lib:expr, $name:literal) => {
                *$lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $name))?
            };
        }
        Ok(Api {
            glew_init: sym!(glew, "glewInit"),
            create: sym!(projectm, "projectm_create"),
            destroy: sym!(projectm, "projectm_destroy"),
            load_preset_data: sym!(projectm, "projectm_load_preset_data"),
            pcm_add_float: sym!(projectm, "projectm_pcm_add_float"),
            pcm_max_samples: sym!(projectm, "projectm_pcm_get_max_samples"),
            render_frame: sym!(projectm, "projectm_opengl_render_frame"),
            set_window_size: sym!(projectm, "projectm_set_window_size"),
            set_preset_duration: sym!(projectm, "projectm_set_preset_duration"),
            set_soft_cut_duration: sym!(projectm, "projectm_set_soft_cut_duration"),
            set_hard_cut_enabled: sym!(projectm, "projectm_set_hard_cut_enabled"),
            set_hard_cut_duration: sym!(projectm, "projectm_set_hard_cut_duration"),
            on_switch_requested: sym!(projectm, "projectm_set_preset_switch_requested_event_callback"),
            on_switch_failed: sym!(projectm, "projectm_set_preset_switch_failed_event_callback"),
            set_texture_search_paths: sym!(projectm, "projectm_set_texture_search_paths"),
            _glew: glew,
            _projectm: projectm,
        })
    }
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

fn api(app: &AppHandle) -> Result<&'static Api, String> {
    API.get_or_init(|| {
        let dir = app.path().resource_dir().map_err(|e| e.to_string())?;
        load_api(&dir)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

// --- audio ----------------------------------------------------------------------

/// Where the samples come from: the player (Spotify and local files) or, in
/// Free Mode, the system loopback, which hears local files too.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Source {
    Player = 1,
    Loopback = 2,
}

/// 0 while MilkDrop is off, else the `Source` being fed.
static FEEDING: AtomicU8 = AtomicU8::new(0);
/// Stereo samples (interleaved) not yet handed to projectM.
static FEED: Mutex<VecDeque<f32>> = Mutex::new(VecDeque::new());
/// Half a second of stereo: older audio is dropped if the renderer falls behind.
const FEED_MAX: usize = 44_100;

fn feeding(source: Source) -> bool {
    FEEDING.load(Ordering::Relaxed) == source as u8
}

/// Add samples (mono ones go to both sides), keeping only the newest FEED_MAX.
fn append(feed: &mut VecDeque<f32>, samples: &[f32], mono: bool) {
    if mono {
        feed.extend(samples.iter().flat_map(|&s| [s, s]));
    } else {
        feed.extend(samples);
    }
    if feed.len() > FEED_MAX {
        let excess = feed.len() - FEED_MAX;
        feed.drain(..excess);
    }
}

/// Interleaved stereo samples, as the visualizer gets them.
pub fn feed_stereo(source: Source, samples: &[f32]) {
    if feeding(source)
        && let Ok(mut feed) = FEED.lock()
    {
        append(&mut feed, samples, false);
    }
}

/// Mono samples (Free Mode's loopback): the same on both sides.
pub fn feed_mono(source: Source, samples: &[f32]) {
    if feeding(source)
        && let Ok(mut feed) = FEED.lock()
    {
        append(&mut feed, samples, true);
    }
}

fn take_feed(out: &mut Vec<f32>) {
    out.clear();
    if let Ok(mut feed) = FEED.lock() {
        out.extend(feed.drain(..));
    }
}

// --- presets ----------------------------------------------------------------------

/// The user's own presets (any folders inside are searched too), and
/// textures: their own, and the full pack when it's been fetched.
fn user_presets_dir() -> Option<PathBuf> {
    crate::settings::get_config_dir().map(|d| d.join("milkdrop"))
}

/// Where presets are found: the user's folder, then the bundled ones.
fn preset_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = user_presets_dir().into_iter().collect();
    if let Ok(dir) = app.path().resource_dir() {
        dirs.push(dir.join("presets"));
    }
    dirs
}

/// A preset as the page knows it (the list, favourites): its path in its
/// folder, the user's own under "mine/", so it holds across installs.
fn preset_key(path: &Path, dirs: &[PathBuf]) -> String {
    let user = user_presets_dir();
    for dir in dirs {
        if let Ok(rel) = path.strip_prefix(dir) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            return if user.as_deref() == Some(dir.as_path()) { format!("mine/{rel}") } else { rel };
        }
    }
    path.to_string_lossy().into_owned()
}

/// Which presets can come next: not broken, not too slow at this size, and
/// among the chosen ones (favourites) when there's a choice.
struct Pool {
    broken: Vec<bool>,
    /// too slow at this many pixels and more (0: never found so)
    slow_at: Vec<u64>,
    only: Option<Vec<bool>>,
}

impl Pool {
    fn new(n: usize) -> Pool {
        Pool { broken: vec![false; n], slow_at: vec![0; n], only: None }
    }

    /// The ones to pass over in a picture of `area` pixels. The chosen ones
    /// count only while one of them is left: none usable, any will do.
    fn skip(&self, area: u64) -> Vec<bool> {
        let skip: Vec<bool> = (0..self.broken.len())
            .map(|i| self.broken[i] || (self.slow_at[i] != 0 && area >= self.slow_at[i]))
            .collect();
        if let Some(only) = &self.only {
            let narrowed: Vec<bool> = skip.iter().zip(only).map(|(&s, &o)| s || !o).collect();
            if narrowed.contains(&false) {
                return narrowed;
            }
        }
        skip
    }
}

/// A preset that was loading when MilkDrop last stopped dead (projectM stuck
/// in it, or the app gone with it) and the ones found so before, skipped
/// from then on.
fn loading_marker() -> Option<PathBuf> {
    crate::settings::get_config_dir().map(|d| d.join("milkdrop-loading.txt"))
}

fn stuck_list() -> Option<PathBuf> {
    crate::settings::get_config_dir().map(|d| d.join("milkdrop-stuck.txt"))
}

/// The marker goes with a render thread that ends (not one stuck for good).
struct LoadingMarker(Option<PathBuf>);

impl Drop for LoadingMarker {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// A folder as projectM can open it: it reads paths in the ANSI code page,
/// so one with other letters in it (a user name like Ömer) goes in its short
/// 8.3 form, which is plain ASCII. None if it isn't there, or has no such form.
fn ansi_path(path: &Path) -> Option<CString> {
    let text = path.to_str()?;
    if text.is_ascii() {
        return path.exists().then(|| CString::new(text).ok()).flatten();
    }
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut short = vec![0u16; 1024];
    let n = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut short)) } as usize;
    if n == 0 || n >= short.len() {
        return None;
    }
    let short = String::from_utf16(&short[..n]).ok()?;
    short.is_ascii().then(|| CString::new(short).ok()).flatten()
}

fn find_presets(dirs: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 8 {
                    walk(&path, out, depth + 1);
                }
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("milk")) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for dir in dirs {
        walk(dir, &mut out, 0);
    }
    out
}

/// A cheap shuffle (no need for a random crate): xorshift from the clock.
fn shuffle(items: &mut [PathBuf]) {
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9)
        | 1;
    for i in (1..items.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        items.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// A preset's text, read here: projectM opens a path in the ANSI code page,
/// so a folder or user name with letters outside it (Masaüstü, Ömer) never
/// loaded. None if it can't be read.
fn read_preset(path: &Path) -> Option<CString> {
    let mut data = std::fs::read(path).ok()?;
    data.retain(|&b| b != 0);
    CString::new(data).ok()
}

/// The next preset after `from` (or before it) that isn't known to be bad:
/// round the list, `from` itself last. None when every one is bad.
fn next_index(bad: &[bool], from: usize, forward: bool) -> Option<usize> {
    let n = bad.len();
    (1..=n)
        .map(|k| if forward { (from + k) % n } else { (from + n - k % n) % n })
        .find(|&i| !bad[i])
}

fn preset_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

// --- the child window ---------------------------------------------------------------

static APP: OnceLock<AppHandle> = OnceLock::new();
/// The pointer hides over the picture when the page says so (fullscreen,
/// the mouse at rest), as it does over its own canvas.
static CURSOR_HIDDEN: AtomicBool = AtomicBool::new(false);
/// The picture put away while the page shows something in its place (the
/// preset list): it stays hidden through moves and resizes.
static PICTURE_HIDDEN: AtomicBool = AtomicBool::new(false);
/// When the last "move" went to the page (ms since the epoch): a few a second
/// are plenty to wake its idle timer.
static LAST_MOVE: AtomicU64 = AtomicU64::new(0);
/// Whether Windows will tell us when the pointer leaves the picture (asked
/// for on the first move over it, once per visit).
static TRACKING: AtomicBool = AtomicBool::new(false);
/// (the windows crate has it under Win32_UI_Controls, a feature for one number)
const WM_MOUSELEAVE: u32 = 0x02A3;
/// Where the bottom line's buttons are on the picture (px), while they show:
/// a click there is that button, not the next preset.
static BAR_HITS: Mutex<Option<BarHits>> = Mutex::new(None);

/// A box on the picture: left, top, right, bottom (px).
type Area = (i32, i32, i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
struct BarHits {
    milk: Area,
    pin: Area,
}

/// The page's event for the button at `(x, y)` on the picture, if one's there.
fn button_at(x: i32, y: i32) -> Option<&'static str> {
    let hits = (*BAR_HITS.lock().ok()?)?;
    let inside = |(l, t, r, b): Area| x >= l && x < r && y >= t && y < b;
    if inside(hits.milk) {
        Some("milkdropButton")
    } else if inside(hits.pin) {
        Some("pinButton")
    } else {
        None
    }
}

/// A mouse message's `(x, y)` (signed: off the left or top is negative).
fn point_of(lparam: LPARAM) -> (i32, i32) {
    ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

unsafe extern "system" fn child_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let event = match msg {
        // GL paints all of it; no flash of the window background
        WM_ERASEBKGND => return LRESULT(1),
        WM_SETCURSOR if CURSOR_HIDDEN.load(Ordering::Relaxed) => {
            unsafe { SetCursor(None) };
            return LRESULT(1);
        }
        // a hand over the buttons, as over the page's
        WM_SETCURSOR => {
            let mut at = POINT::default();
            let over = unsafe { GetCursorPos(&mut at).is_ok() && ScreenToClient(hwnd, &mut at).as_bool() }
                && button_at(at.x, at.y).is_some();
            if over && let Ok(hand) = unsafe { LoadCursorW(None, IDC_HAND) } {
                unsafe { SetCursor(Some(hand)) };
                return LRESULT(1);
            }
            None
        }
        // it never takes the focus itself...
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            // ...but a click brings the visualizer forward, so its keys
            // (Esc, M, F11) work after clicking the picture
            unsafe {
                if let Ok(owner) = GetWindow(hwnd, GW_OWNER)
                    && GetForegroundWindow() != owner
                {
                    let _ = SetForegroundWindow(owner);
                }
            }
            let (x, y) = point_of(lparam);
            Some(button_at(x, y).unwrap_or("click"))
        }
        // the second click of a quick two on a button is that button again
        WM_LBUTTONDBLCLK => {
            let (x, y) = point_of(lparam);
            Some(button_at(x, y).unwrap_or("dblclick"))
        }
        WM_RBUTTONUP => Some("contextmenu"),
        WM_MOUSEMOVE => {
            // a WM_MOUSELEAVE when the pointer is off it again: the page shows
            // its buttons while the pointer is over the visualizer
            if !TRACKING.swap(true, Ordering::Relaxed) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                if unsafe { TrackMouseEvent(&mut track) }.is_err() {
                    TRACKING.store(false, Ordering::Relaxed);
                }
                // the first move of a visit goes to the page at once
                LAST_MOVE.store(0, Ordering::Relaxed);
            }
            let now = now_ms();
            (now.saturating_sub(LAST_MOVE.load(Ordering::Relaxed)) > 150).then(|| {
                LAST_MOVE.store(now, Ordering::Relaxed);
                "move"
            })
        }
        WM_MOUSELEAVE => {
            TRACKING.store(false, Ordering::Relaxed);
            Some("leave")
        }
        _ => None,
    };
    if let (Some(event), Some(app)) = (event, APP.get()) {
        let _ = app.emit_to("visualizer", "milkdropMouse", event);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

const CLASS: PCWSTR = w!("SpotiampMilkDrop");

fn register_class() -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| unsafe {
            let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(child_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or(HCURSOR::default()),
                lpszClassName: CLASS,
                // OWNDC: GL needs the same device context every frame
                style: CS_OWNDC | CS_DBLCLKS,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err("couldn't register the MilkDrop window class".into());
            }
            Ok(())
        })
        .clone()
}

/// Where the picture goes, in the visualizer window's client pixels.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Rect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Put the picture over `rect` of its owner's client area (on the screen).
fn place(picture: HWND, owner: HWND, rect: Rect) {
    unsafe {
        let mut at = POINT { x: rect.x, y: rect.y };
        if !ClientToScreen(owner, &mut at).as_bool() {
            return;
        }
        let _ = SetWindowPos(
            picture,
            Some(HWND_TOP),
            at.x,
            at.y,
            rect.width.max(1) as i32,
            rect.height.max(1) as i32,
            SWP_NOACTIVATE | if PICTURE_HIDDEN.load(Ordering::Relaxed) { SWP_HIDEWINDOW } else { SWP_SHOWWINDOW },
        );
    }
}

// --- text over the picture: the song title (and a hint) in fullscreen --------
// HTML can't draw over this window, so text is drawn in GL after projectM's
// frame: Windows renders it (GDI, grey antialiasing) into an alpha mask, and
// it's laid on as a texture, a shadow first, fading in and out.

/// The two texts there can be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayKind {
    Title,
    Hint,
}

struct Overlay {
    kind: OverlayKind,
    text: String,
    shown_at: Instant,
    texture: u32,
    size: (i32, i32),
    /// the window height its letters were sized for
    for_height: u32,
}

impl OverlayKind {
    fn seconds(self) -> f32 {
        match self {
            OverlayKind::Title => 6.0,
            OverlayKind::Hint => 3.0,
        }
    }
    /// letter height (px) for a window this tall
    fn px(self, height: u32) -> i32 {
        match self {
            OverlayKind::Title => (height as i32 / 16).clamp(16, 72),
            OverlayKind::Hint => (height as i32 / 50).clamp(11, 24),
        }
    }
}

/// The room around a text in its mask, each side (px).
fn text_pad(px: i32) -> i32 {
    px / 4 + 2
}

/// White text as an 8-bit alpha mask (top row first) and its size; longer
/// than `max_width` ends in an ellipsis. `text_pad` around it.
fn text_mask(text: &str, px: i32, bold: bool, max_width: i32, face: PCWSTR) -> Option<(Vec<u8>, i32, i32)> {
    use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC,
        CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS,
        DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FF_SWISS, FW_BOLD, FW_NORMAL,
        GdiFlush, GetTextExtentPoint32W, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor,
        TRANSPARENT,
    };
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return None;
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let weight = if bold { FW_BOLD } else { FW_NORMAL };
        let font = CreateFontW(
            -px,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
            face,
        );
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        let mut extent = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &wide, &mut extent);
        let pad = text_pad(px);
        let (w, h) = ((extent.cx + 2 * pad).clamp(1, max_width.max(1)), extent.cy + 2 * pad);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top row first
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let result = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok().map(|bitmap| {
            let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            let mut rect = RECT { left: pad, top: 0, right: w - pad, bottom: h };
            let mut text = wide.clone();
            DrawTextW(dc, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
            let _ = GdiFlush();
            // a fresh DIB is black: the brightest channel is how much ink
            let pixels = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
            let mask: Vec<u8> = pixels.chunks_exact(4).map(|p| p[0].max(p[1]).max(p[2])).collect();
            SelectObject(dc, old_bitmap);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            mask
        });
        SelectObject(dc, old_font);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteDC(dc);
        result.map(|mask| (mask, w, h))
    }
}

/// The GL functions past 1.1 the text needs, to set projectM's state aside.
struct GlCalls {
    use_program: unsafe extern "system" fn(u32),
    bind_vertex_array: unsafe extern "system" fn(u32),
    active_texture: unsafe extern "system" fn(u32),
    bind_framebuffer: unsafe extern "system" fn(u32, u32),
}

fn gl_calls() -> Option<GlCalls> {
    unsafe {
        macro_rules! get {
            ($name:literal, $ty:ty) => {
                std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(wglGetProcAddress(s!($name))?)
            };
        }
        Some(GlCalls {
            use_program: get!("glUseProgram", unsafe extern "system" fn(u32)),
            bind_vertex_array: get!("glBindVertexArray", unsafe extern "system" fn(u32)),
            active_texture: get!("glActiveTexture", unsafe extern "system" fn(u32)),
            bind_framebuffer: get!("glBindFramebuffer", unsafe extern "system" fn(u32, u32)),
        })
    }
}

const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;

/// An alpha mask as a texture.
fn mask_texture(mask: &[u8], w: i32, h: i32) -> u32 {
    let mut texture = 0u32;
    unsafe {
        glGenTextures(1, &mut texture);
        glBindTexture(GL_TEXTURE_2D, texture);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_ALPHA as i32, w, h, 0, GL_ALPHA, GL_UNSIGNED_BYTE, mask.as_ptr() as *const c_void);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 4);
        glBindTexture(GL_TEXTURE_2D, 0);
    }
    texture
}

/// (Re)make an overlay's texture for the window's size.
fn build_overlay(o: &mut Overlay, width: u32, height: u32) {
    if o.texture != 0 {
        unsafe { glDeleteTextures(1, &o.texture) };
        o.texture = 0;
    }
    let max_width = (width as f32 * 0.9) as i32;
    let bold = o.kind == OverlayKind::Title;
    let Some((mask, w, h)) = text_mask(&o.text, o.kind.px(height), bold, max_width, w!("Segoe UI")) else {
        return;
    };
    o.texture = mask_texture(&mask, w, h);
    o.size = (w, h);
    o.for_height = height;
}

fn drop_overlay(o: &Overlay) {
    if o.texture != 0 {
        unsafe { glDeleteTextures(1, &o.texture) };
    }
}

/// Lay the overlays over the frame projectM just drew; drops the finished.
fn draw_overlays(calls: &GlCalls, overlays: &mut Vec<Overlay>, width: u32, height: u32) {
    overlays.retain(|o| {
        let done = o.shown_at.elapsed().as_secs_f32() > o.kind.seconds();
        if done {
            drop_overlay(o);
        }
        !done
    });
    if overlays.is_empty() {
        return;
    }
    for o in overlays.iter_mut() {
        if o.for_height != height || o.texture == 0 {
            build_overlay(o, width, height);
        }
    }
    let (w, h) = (width as f32, height as f32);
    begin_2d(calls, width, height);
    for o in overlays.iter() {
        if o.texture == 0 {
            continue;
        }
        let t = o.shown_at.elapsed().as_secs_f32();
        let fade = (t / 0.4).min((o.kind.seconds() - t) / 0.8).clamp(0.0, 1.0);
        let (tw, th) = (o.size.0 as f32, o.size.1 as f32);
        let x = ((w - tw) / 2.0).round();
        let y = match o.kind {
            OverlayKind::Title => (h * 0.86 - th / 2.0).round(),
            OverlayKind::Hint => (h * 0.02).round(),
        };
        let shadow = (th / 24.0).max(1.0).round();
        unsafe { glBindTexture(GL_TEXTURE_2D, o.texture) };
        quad((x + shadow, y + shadow), (tw, th), [0.0, 0.0, 0.0, 0.8 * fade]);
        quad((x, y), (tw, th), [1.0, 1.0, 1.0, fade]);
    }
    end_2d();
}

/// GL set up for laying text over projectM's frame (its state set aside);
/// `end_2d` puts it back.
fn begin_2d(calls: &GlCalls, width: u32, height: u32) {
    unsafe {
        (calls.bind_framebuffer)(GL_FRAMEBUFFER, 0);
        (calls.use_program)(0);
        (calls.bind_vertex_array)(0);
        (calls.active_texture)(GL_TEXTURE0);
        glViewport(0, 0, width as i32, height as i32);
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_SCISSOR_TEST);
        glDisable(GL_CULL_FACE);
        glMatrixMode(GL_PROJECTION);
        glLoadIdentity();
        glOrtho(0.0, width as f64, height as f64, 0.0, -1.0, 1.0);
        glMatrixMode(GL_MODELVIEW);
        glLoadIdentity();
        glEnable(GL_TEXTURE_2D);
        glEnable(GL_BLEND);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        glTexEnvi(GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_MODULATE as i32);
    }
}

/// As projectM expects it.
fn end_2d() {
    unsafe {
        glBindTexture(GL_TEXTURE_2D, 0);
        glDisable(GL_BLEND);
        glDisable(GL_TEXTURE_2D);
        glColor4f(1.0, 1.0, 1.0, 1.0);
    }
}

/// The bound text mask at `(x, y)` (its top left), `size` big, in `color`.
fn quad((x, y): (f32, f32), (w, h): (f32, f32), color: [f32; 4]) {
    unsafe {
        glColor4f(color[0], color[1], color[2], color[3]);
        glBegin(GL_QUADS);
        glTexCoord2f(0.0, 0.0);
        glVertex2f(x, y);
        glTexCoord2f(1.0, 0.0);
        glVertex2f(x + w, y);
        glTexCoord2f(1.0, 1.0);
        glVertex2f(x + w, y + h);
        glTexCoord2f(0.0, 1.0);
        glVertex2f(x, y + h);
        glEnd();
    }
}

// --- the bottom line: the preset's name, MILKDROP and PIN ---------------------
// Laid over the picture as the page lays them over its own canvas (for them
// it used to leave a black strip under the picture). They show while the
// pointer's over the visualizer, fading like the page's; a note shows alone.

/// What the page wants on the bottom line.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct Bar {
    /// the preset's name, or a note
    text: String,
    /// `text` is a note: amber, and shown with the rest hidden too
    note: bool,
    /// the name and the buttons show: the pointer's over the visualizer
    shown: bool,
    pinned: bool,
    /// the letters' size and the gap to the picture's edges, in px
    px: i32,
    inset: i32,
}

impl Bar {
    /// the same letters (only the showing differs)
    fn same_text(&self, other: &Bar) -> bool {
        (&self.text, self.note, self.pinned, self.px, self.inset)
            == (&other.text, other.note, other.pinned, other.px, other.inset)
    }
}

/// The page's colours there: its green, its amber.
const GREEN: [f32; 3] = [0.431, 1.0, 0.627];
const AMBER: [f32; 3] = [1.0, 0.824, 0.29];
const MILK_LABEL: &str = "◆ MILKDROP";
/// The pin's room is PINNED's either way, so MILKDROP stays put.
const PINNED_LABEL: &str = "● PINNED";

/// A text made into a texture.
struct Label {
    texture: u32,
    size: (i32, i32),
    /// the text's own width in it (the mask has `text_pad` either side)
    width: i32,
}

impl Label {
    fn new(text: &str, px: i32, max_width: i32) -> Option<Label> {
        let (mask, w, h) = text_mask(text, px, false, max_width, w!("Consolas"))?;
        Some(Label { texture: mask_texture(&mask, w, h), size: (w, h), width: w - 2 * text_pad(px) })
    }
}

impl Drop for Label {
    fn drop(&mut self) {
        unsafe { glDeleteTextures(1, &self.texture) };
    }
}

/// Where the bottom line's texts go, as boxes around the letters.
#[derive(Debug, PartialEq)]
struct BarLayout {
    /// the room the name has, from the left inset
    name_room: i32,
    milk: Area,
    pin: Area,
}

/// The bottom line in a `width` x `height` picture: PIN at the right (in
/// PINNED's room), MILKDROP left of it, the name up to MILKDROP.
fn bar_layout(width: i32, height: i32, inset: i32, line: i32, milk_width: i32, pin_room: i32) -> BarLayout {
    let gap = line;
    let (bottom, top) = (height - inset, height - inset - line);
    let pin_right = width - inset;
    let milk_right = pin_right - pin_room - gap;
    let milk_left = milk_right - milk_width;
    BarLayout {
        name_room: (milk_left - gap - inset).max(0),
        milk: (milk_left, top, milk_right, bottom),
        pin: (pin_right - pin_room, top, pin_right, bottom),
    }
}

/// The bottom line's state on the render thread.
struct BarState {
    bar: Bar,
    /// how much the name and the buttons show, fading towards `bar.shown`
    alpha: f32,
    last: Instant,
    /// name, MILKDROP, PIN and PINNED's room, made for `made_for`
    labels: Option<(Option<Label>, Label, Label, i32)>,
    made_for: (u32, u32),
}

impl BarState {
    fn new() -> BarState {
        BarState { bar: Bar::default(), alpha: 0.0, last: Instant::now(), labels: None, made_for: (0, 0) }
    }

    fn set(&mut self, bar: Bar) {
        if !self.bar.same_text(&bar) {
            self.labels = None;
        }
        self.bar = bar;
    }
}

fn draw_bar(calls: &GlCalls, state: &mut BarState, width: u32, height: u32) {
    let step = state.last.elapsed().as_secs_f32() / 0.2;
    state.last = Instant::now();
    state.alpha = if state.bar.shown { (state.alpha + step).min(1.0) } else { (state.alpha - step).max(0.0) };
    let note = state.bar.note && !state.bar.text.is_empty();
    if (state.alpha == 0.0 && !note) || state.bar.px <= 0 {
        set_hits(None);
        return;
    }
    let px = state.bar.px;
    let pad = text_pad(px);
    if state.labels.is_none() || state.made_for != (width, height) {
        state.labels = None;
        let pin_label = if state.bar.pinned { PINNED_LABEL } else { "○ PIN" };
        let Some((milk, pin)) = Label::new(MILK_LABEL, px, i32::MAX).zip(Label::new(pin_label, px, i32::MAX)) else {
            set_hits(None);
            return;
        };
        let pin_room = Label::new(PINNED_LABEL, px, i32::MAX).map_or(pin.width, |l| l.width).max(pin.width);
        let line = milk.size.1 - 2 * pad;
        let layout = bar_layout(width as i32, height as i32, state.bar.inset, line, milk.width, pin_room);
        let name = Label::new(&state.bar.text, px, layout.name_room + 2 * pad);
        state.labels = Some((name, milk, pin, pin_room));
        state.made_for = (width, height);
    }
    let Some((name, milk, pin, pin_room)) = &state.labels else {
        return;
    };
    let line = milk.size.1 - 2 * pad;
    let layout = bar_layout(width as i32, height as i32, state.bar.inset, line, milk.width, *pin_room);
    // the buttons answer clicks only while they can be seen
    set_hits((state.alpha > 0.5).then(|| {
        let grow = |(l, t, r, b): Area| (l - pad, t - pad, r + pad, b + pad);
        BarHits { milk: grow(layout.milk), pin: grow(layout.pin) }
    }));
    let a = state.alpha;
    let (name_alpha, name_color) = if note { (0.95, AMBER) } else { (0.75 * a, GREEN) };
    let (pin_alpha, pin_color) = if state.bar.pinned { (0.95 * a, AMBER) } else { (0.5 * a, GREEN) };
    // each text's mask with its top left where its letters' box goes
    let at = |left: i32, top: i32| ((left - pad) as f32, (top - pad) as f32);
    let mut parts = vec![
        (milk, at(layout.milk.0, layout.milk.1), 0.95 * a, AMBER),
        (pin, at(layout.pin.2 - pin.width, layout.pin.1), pin_alpha, pin_color),
    ];
    if let Some(name) = name {
        parts.push((name, at(state.bar.inset, layout.milk.1), name_alpha, name_color));
    }
    // dark around the letters, as the page's text-shadow, so they read on
    // a bright picture
    let edge = (px as f32 / 12.0).round().max(1.0);
    begin_2d(calls, width, height);
    for (label, (x, y), alpha, color) in parts {
        if alpha <= 0.0 {
            continue;
        }
        let size = (label.size.0 as f32, label.size.1 as f32);
        unsafe { glBindTexture(GL_TEXTURE_2D, label.texture) };
        for (dx, dy) in AROUND {
            quad((x + dx * edge, y + dy * edge), size, [0.0, 0.0, 0.0, 0.7 * alpha]);
        }
        quad((x, y), size, [color[0], color[1], color[2], alpha]);
    }
    end_2d();
}

/// The eight steps around a pixel.
const AROUND: [(f32, f32); 8] =
    [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)];

fn set_hits(hits: Option<BarHits>) {
    if let Ok(mut h) = BAR_HITS.lock() {
        *h = hits;
    }
}

// --- the render thread ----------------------------------------------------------------

enum Command {
    Resize(u32, u32),
    Next,
    Previous,
    /// pinned: no changes by time (a slow or broken one still goes)
    Lock(bool),
    /// show (Some) or take away (None) a text over the picture
    Overlay(OverlayKind, Option<String>),
    /// take every text away
    ClearOverlays,
    /// the bottom line
    Bar(Bar),
    /// how long a preset stays, and whether a strong beat may cut to the next
    Timing(f64, bool),
    /// this one now (its key)
    Play(String),
    /// cycle only through these (keys), or all
    Only(Option<Vec<String>>),
    Snapshot(PathBuf, Sender<Result<(), String>>),
}

#[derive(Clone, Default, serde::Serialize)]
pub struct Status {
    running: bool,
    preset: String,
    presets: usize,
    fps: f32,
    frames: u64,
    skipped: u32,
    error: Option<String>,
    /// the preset's key (the page's list marks it)
    key: String,
    /// where the bottom line's MILKDROP and PIN are while they show
    buttons: Option<[Area; 2]>,
}

/// How long a preset stays before the next blends in.
const PRESET_SECONDS: f64 = 30.0;
/// The blend between two presets.
const BLEND_SECONDS: f64 = 2.5;
/// A preset slower than this (ms a frame, at the window's size) is skipped:
/// it would stutter, and on slower PCs far worse.
const SLOW_MS: f64 = 25.0;
/// 60 frames a second with music, 30 after a few quiet seconds: paced here,
/// since vsync doesn't hold in a child window everywhere (it drew at 400+).
const FRAME_ACTIVE: Duration = Duration::from_micros(16_667);
const FRAME_QUIET: Duration = Duration::from_micros(33_333);
const QUIET_AFTER: Duration = Duration::from_secs(3);

struct Running {
    child: isize,
    owner: isize,
    /// where it goes in the owner, to follow it when that moves
    rect: Rect,
    commands: Sender<Command>,
    thread: std::thread::JoinHandle<()>,
    status: Arc<Mutex<Status>>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// The GL side: released in every way out of the render thread.
struct Gl {
    hwnd: HWND,
    hdc: HDC,
    ctx: HGLRC,
}

impl Drop for Gl {
    fn drop(&mut self) {
        unsafe {
            let _ = wglMakeCurrent(HDC::default(), HGLRC::default());
            let _ = wglDeleteContext(self.ctx);
            ReleaseDC(Some(self.hwnd), self.hdc);
        }
    }
}

struct Engine {
    api: &'static Api,
    handle: Handle,
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            (self.api.on_switch_requested)(self.handle, None, std::ptr::null_mut());
            (self.api.on_switch_failed)(self.handle, None, std::ptr::null_mut());
            (self.api.destroy)(self.handle);
        }
    }
}

/// What projectM's callbacks tell the render loop (called on its own thread,
/// during render_frame).
#[derive(Default)]
struct Signals {
    switch: Cell<bool>,
    /// the switch is a cut on the beat: no blend
    hard: Cell<bool>,
    failed: Cell<bool>,
}

unsafe extern "C" fn switch_requested(hard_cut: bool, data: *mut c_void) {
    if let Some(signals) = unsafe { (data as *const Signals).as_ref() } {
        signals.switch.set(true);
        signals.hard.set(hard_cut);
    }
}

unsafe extern "C" fn switch_failed(file: *const c_char, message: *const c_char, data: *mut c_void) {
    let text = |p: *const c_char| {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    log::warn!("MilkDrop: couldn't load {}: {}", text(file), text(message));
    if let Some(signals) = unsafe { (data as *const Signals).as_ref() } {
        signals.failed.set(true);
    }
}

fn open_gl(hwnd: HWND) -> Result<Gl, String> {
    unsafe {
        let hdc = GetDC(Some(hwnd));
        if hdc.is_invalid() {
            return Err("no device context".into());
        }
        let pfd = PIXELFORMATDESCRIPTOR {
            nSize: size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
            iPixelType: PFD_TYPE_RGBA,
            cColorBits: 32,
            cDepthBits: 24,
            cStencilBits: 8,
            iLayerType: PFD_MAIN_PLANE.0 as u8,
            ..Default::default()
        };
        let format = ChoosePixelFormat(hdc, &pfd);
        let ctx = (format != 0)
            .then(|| SetPixelFormat(hdc, format, &pfd).ok())
            .flatten()
            .and_then(|()| wglCreateContext(hdc).ok());
        let Some(ctx) = ctx else {
            ReleaseDC(Some(hwnd), hdc);
            return Err("OpenGL isn't available".into());
        };
        let gl = Gl { hwnd, hdc, ctx };
        wglMakeCurrent(hdc, ctx).map_err(|e| format!("OpenGL: {e}"))?;
        // vsync: a frame per screen refresh, no more
        if let Some(swap_interval) = wglGetProcAddress(s!("wglSwapIntervalEXT")) {
            let swap_interval: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(swap_interval);
            swap_interval(1);
        }
        Ok(gl)
    }
}

fn set_status(status: &Mutex<Status>, f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = status.lock() {
        f(&mut s);
    }
}

#[allow(clippy::too_many_arguments)]
fn render(
    app: AppHandle,
    api: &'static Api,
    child: isize,
    (mut width, mut height): (u32, u32),
    preset_dirs: Vec<PathBuf>,
    texture_dirs: Vec<PathBuf>,
    commands: Receiver<Command>,
    status: Arc<Mutex<Status>>,
) -> Result<(), String> {
    let hwnd = HWND(child as *mut c_void);
    let gl = open_gl(hwnd)?;
    unsafe {
        // projectM 4.1 reaches OpenGL through GLEW, started by its host
        let glew = (api.glew_init)();
        if glew != 0 {
            return Err(format!("GLEW couldn't start ({glew})"));
        }
    }
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return Err("projectM couldn't start (OpenGL 3.3 needed)".into());
    }
    let signals = Box::new(Signals::default());
    let engine = Engine { api, handle };
    unsafe {
        let data = &*signals as *const Signals as *mut c_void;
        (api.on_switch_requested)(handle, Some(switch_requested), data);
        (api.on_switch_failed)(handle, Some(switch_failed), data);
        (api.set_window_size)(handle, width as usize, height as usize);
        (api.set_preset_duration)(handle, PRESET_SECONDS);
        // the pictures presets draw with (clouds, lichen, the random ones)
        let paths: Vec<CString> = texture_dirs.iter().filter_map(|d| ansi_path(d)).collect();
        let pointers: Vec<*const c_char> = paths.iter().map(|p| p.as_ptr()).collect();
        (api.set_texture_search_paths)(handle, pointers.as_ptr(), pointers.len());
        (api.set_soft_cut_duration)(handle, BLEND_SECONDS);
    }

    // here, not on the main thread: a big collection takes a moment to list
    let mut presets = find_presets(&preset_dirs);
    shuffle(&mut presets);
    let keys: Vec<String> = presets.iter().map(|p| preset_key(p, &preset_dirs)).collect();
    set_status(&status, |s| {
        s.running = true;
        s.presets = presets.len();
    });
    let mut pool = Pool::new(presets.len());
    // one that held projectM last time: skipped from now on (and the user told)
    let marker = loading_marker();
    let mut stuck: Vec<String> = stuck_list()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    if let Some(m) = &marker
        && let Ok(text) = std::fs::read_to_string(m)
    {
        let path = text.trim().to_string();
        if !path.is_empty() && !stuck.contains(&path) {
            log::warn!("MilkDrop: {path} held projectM last time; skipped from now on");
            if let Some(list) = stuck_list() {
                let _ = std::fs::write(&list, format!("{}{path}\n", stuck.iter().map(|p| format!("{p}\n")).collect::<String>()));
            }
            let _ = app.emit_to("visualizer", "milkdropStuck", preset_name(Path::new(&path)));
            stuck.push(path);
        }
        let _ = std::fs::remove_file(m);
    }
    for (i, p) in presets.iter().enumerate() {
        pool.broken[i] = stuck.iter().any(|s| Path::new(s) == p);
    }
    let _marker = LoadingMarker(marker.clone());
    // a preset loading and not drawn a frame yet
    let loading = Cell::new(false);
    let mut loaded_at = Instant::now();
    let mut measured = (Duration::ZERO, 0u32);
    // put preset `i` on: false if it couldn't be read
    let show = |i: usize, smooth: bool| -> bool {
        let Some(text) = presets.get(i).and_then(|p| read_preset(p)) else {
            return false;
        };
        if let Some(m) = &marker {
            let _ = std::fs::write(m, presets[i].to_string_lossy().as_bytes());
            loading.set(true);
        }
        unsafe { (api.load_preset_data)(engine.handle, text.as_ptr(), smooth) };
        let name = preset_name(&presets[i]);
        set_status(&status, |s| {
            s.preset = name.clone();
            s.key = keys[i].clone();
        });
        let _ = app.emit_to("visualizer", "milkdropPreset", name);
        true
    };
    // on to the next usable preset from `from`, skipping unreadable ones; if
    // none is left, say so and stay (no retrying the same one every frame)
    let go = |from: usize, forward: bool, smooth: bool, pool: &mut Pool, area: u64, index: &mut usize| {
        let mut skip = pool.skip(area);
        let mut from = from;
        loop {
            let Some(i) = next_index(&skip, from, forward) else {
                set_status(&status, |s| s.preset = String::new());
                let _ = app.emit_to("visualizer", "milkdropPreset", "none of the presets would load");
                return;
            };
            if show(i, smooth) {
                *index = i;
                return;
            }
            log::warn!("MilkDrop: couldn't read {}", presets[i].display());
            pool.broken[i] = true;
            skip[i] = true;
            set_status(&status, |s| s.skipped += 1);
            from = i;
        }
    };
    let area = |w: u32, h: u32| w as u64 * h as u64;
    let mut index = presets.len().saturating_sub(1);
    if !presets.is_empty() {
        // from the last one round to the first
        go(index, true, false, &mut pool, area(width, height), &mut index);
    }

    let max_chunk = unsafe { (api.pcm_max_samples)() }.max(1) as usize;
    let mut pcm = Vec::new();
    let mut frames = 0u64;
    let mut second = (Instant::now(), 0u32);
    let mut locked = false;
    let calls = gl_calls();
    let mut overlays: Vec<Overlay> = Vec::new();
    let mut bar = BarState::new();
    let mut next_frame = Instant::now();
    // when something was last heard
    let mut heard = Instant::now();
    loop {
        // the window went away (its parent closed): done
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return Ok(());
        }
        let mut snapshot = None;
        loop {
            match commands.try_recv() {
                Ok(Command::Resize(w, h)) => {
                    (width, height) = (w.max(1), h.max(1));
                    unsafe { (api.set_window_size)(handle, width as usize, height as usize) };
                }
                Ok(Command::Next) if !presets.is_empty() => {
                    go(index, true, true, &mut pool, area(width, height), &mut index);
                    loaded_at = Instant::now();
                }
                Ok(Command::Previous) if !presets.is_empty() => {
                    go(index, false, true, &mut pool, area(width, height), &mut index);
                    loaded_at = Instant::now();
                }
                Ok(Command::Lock(on)) => locked = on,
                Ok(Command::Timing(seconds, beat_cuts)) => unsafe {
                    (api.set_preset_duration)(handle, seconds);
                    // a beat may cut once half its time is up
                    (api.set_hard_cut_duration)(handle, seconds / 2.0);
                    (api.set_hard_cut_enabled)(handle, beat_cuts);
                },
                Ok(Command::Play(key)) => {
                    if let Some(i) = keys.iter().position(|k| *k == key) {
                        if show(i, true) {
                            index = i;
                            loaded_at = Instant::now();
                        } else {
                            pool.broken[i] = true;
                        }
                    }
                }
                Ok(Command::Only(chosen)) => {
                    pool.only = chosen.map(|chosen| {
                        let chosen: HashSet<String> = chosen.into_iter().collect();
                        keys.iter().map(|k| chosen.contains(k)).collect()
                    });
                }
                Ok(Command::Overlay(kind, text)) => {
                    overlays.retain(|o| {
                        let same = o.kind == kind;
                        if same {
                            drop_overlay(o);
                        }
                        !same
                    });
                    if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
                        overlays.push(Overlay {
                            kind,
                            text,
                            shown_at: Instant::now(),
                            texture: 0,
                            size: (0, 0),
                            for_height: 0,
                        });
                    }
                }
                Ok(Command::ClearOverlays) => {
                    overlays.iter().for_each(drop_overlay);
                    overlays.clear();
                }
                Ok(Command::Bar(b)) => bar.set(b),
                Ok(Command::Snapshot(path, reply)) => snapshot = Some((path, reply)),
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        // hidden (minimized, or the page asked): no drawing, and the audio
        // waiting for it is dropped
        let shown = unsafe {
            IsWindowVisible(hwnd).as_bool()
                && !GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| IsIconic(owner).as_bool())
        };
        if !shown {
            take_feed(&mut pcm);
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        take_feed(&mut pcm);
        if pcm.iter().any(|s| s.abs() > 1e-4) {
            heard = Instant::now();
        }
        for chunk in pcm.chunks(max_chunk * 2) {
            unsafe { (api.pcm_add_float)(handle, chunk.as_ptr(), (chunk.len() / 2) as u32, 2) };
        }
        // the next preset, if one's due: Some(blend it in)
        let mut advance = None;
        // is this preset too slow? measured once its blend is over
        let since = loaded_at.elapsed();
        let measuring = since > Duration::from_secs(4) && since < Duration::from_secs(6);
        let started = Instant::now();
        unsafe { (api.render_frame)(handle) };
        if let Some(calls) = &calls {
            draw_overlays(calls, &mut overlays, width, height);
            draw_bar(calls, &mut bar, width, height);
        }
        if measuring {
            unsafe { glFinish() };
            measured.0 += started.elapsed();
            measured.1 += 1;
        } else if measured.1 > 0 {
            let avg_ms = measured.0.as_secs_f64() * 1000.0 / measured.1 as f64;
            measured = (Duration::ZERO, 0);
            if avg_ms > SLOW_MS && !presets.is_empty() {
                log::info!(
                    "MilkDrop: {} is too slow at {width}x{height} ({avg_ms:.1} ms a frame), skipped at that size",
                    preset_name(&presets[index])
                );
                // a smaller picture (out of fullscreen) may take it again
                pool.slow_at[index] = area(width, height);
                set_status(&status, |s| s.skipped += 1);
                advance = Some(false);
            }
        }
        if let Some((path, reply)) = snapshot.take() {
            let _ = reply.send(save_frame(&path, width, height));
        }
        unsafe {
            let _ = SwapBuffers(gl.hdc);
        }
        // the new preset drew a frame: it didn't hold projectM
        if loading.replace(false)
            && let Some(m) = &marker
        {
            let _ = std::fs::remove_file(m);
        }
        let period = if heard.elapsed() > QUIET_AFTER { FRAME_QUIET } else { FRAME_ACTIVE };
        next_frame += period;
        let now = Instant::now();
        if next_frame > now {
            std::thread::sleep(next_frame - now);
        } else if now - next_frame > period {
            // fell behind (a slow frame): carry on from now, no catching up
            next_frame = now;
        }
        frames += 1;
        second.1 += 1;
        if second.0.elapsed() >= Duration::from_secs(1) {
            let fps = second.1 as f32 / second.0.elapsed().as_secs_f32();
            set_status(&status, |s| {
                s.fps = fps;
                s.frames = frames;
            });
            second = (Instant::now(), 0);
        }
        // projectM's word: this one wouldn't load, or its time is up
        if signals.failed.get() && !presets.is_empty() && !pool.broken[index] {
            pool.broken[index] = true;
            set_status(&status, |s| s.skipped += 1);
            advance = Some(false);
        }
        if signals.switch.get() && !locked {
            // blended, unless it's a cut on the beat
            advance = advance.or(Some(!signals.hard.get()));
        }
        signals.switch.set(false);
        signals.hard.set(false);
        signals.failed.set(false);
        if let Some(smooth) = advance
            && !presets.is_empty()
        {
            go(index, true, smooth, &mut pool, area(width, height), &mut index);
            loaded_at = Instant::now();
        }
    }
}

fn save_frame(path: &Path, width: u32, height: u32) -> Result<(), String> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe {
        glReadBuffer(GL_BACK);
        glReadPixels(
            0,
            0,
            width as i32,
            height as i32,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            pixels.as_mut_ptr() as *mut c_void,
        );
    }
    let mut img = image::RgbaImage::from_raw(width, height, pixels).ok_or("bad frame size")?;
    image::imageops::flip_vertical_in_place(&mut img);
    img.pixels_mut().for_each(|p| p.0[3] = 255);
    img.save(path).map_err(|e| e.to_string())
}

// --- commands -----------------------------------------------------------------------

/// Whether MilkDrop can run here (the engine's DLLs load), and how many
/// presets there are. Async: the first call loads the DLLs from disk.
#[tauri::command(async)]
pub fn milkdrop_available(app: AppHandle) -> Result<usize, String> {
    api(&app)?;
    Ok(find_presets(&preset_dirs(&app)).len())
}

/// A preset in the page's list.
#[derive(serde::Serialize)]
pub struct PresetEntry {
    key: String,
    name: String,
    /// the folder it's in ("" at the top)
    folder: String,
    /// in the user's own folder
    mine: bool,
}

/// Every preset there is, the user's own first, by folder and name.
#[tauri::command(async)]
pub fn milkdrop_list(app: AppHandle) -> Vec<PresetEntry> {
    let dirs = preset_dirs(&app);
    let mut list: Vec<PresetEntry> = find_presets(&dirs)
        .iter()
        .map(|p| {
            let key = preset_key(p, &dirs);
            let mine = key.starts_with("mine/");
            let rel = key.strip_prefix("mine/").unwrap_or(&key);
            let folder = rel.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
            PresetEntry { name: preset_name(p), folder, mine, key }
        })
        .collect();
    list.sort_by_cached_key(|e| (!e.mine, e.folder.to_lowercase(), e.name.to_lowercase()));
    list
}

/// Put this preset on now (its key from `milkdrop_list`).
#[tauri::command]
pub fn milkdrop_play(key: String) {
    send(Command::Play(key));
}

/// Cycle only through these presets (keys), or through all (None).
#[tauri::command]
pub fn milkdrop_only(keys: Option<Vec<String>>) {
    send(Command::Only(keys));
}

/// How long a preset stays, and whether a strong beat may cut to the next.
#[tauri::command]
pub fn milkdrop_timing(seconds: f64, beat_cuts: bool) {
    send(Command::Timing(seconds.clamp(5.0, 600.0), beat_cuts));
}

fn send(command: Command) {
    if let Ok(running) = RUNNING.lock()
        && let Some(r) = running.as_ref()
    {
        let _ = r.commands.send(command);
    }
}

/// Start drawing over `rect` of the calling window (or just move there, if
/// it's drawing already). On the main thread: it makes the child window.
#[tauri::command]
pub fn milkdrop_start(app: AppHandle, window: WebviewWindow, rect: Rect) -> Result<(), String> {
    let api = api(&app)?;
    let _ = APP.set(app.clone());
    let mut running = RUNNING.lock().map_err(|_| "busy")?;
    if let Some(r) = running.as_mut() {
        if !r.thread.is_finished() {
            r.rect = rect;
            place(HWND(r.child as *mut c_void), HWND(r.owner as *mut c_void), rect);
            let _ = r.commands.send(Command::Resize(rect.width, rect.height));
            return Ok(());
        }
        // it stopped by itself (an error): clean up and start over
        stop_locked(&mut running);
    }

    register_class()?;
    // a new window: nothing tracked over it yet, no buttons on it, shown
    TRACKING.store(false, Ordering::Relaxed);
    PICTURE_HIDDEN.store(false, Ordering::Relaxed);
    set_hits(None);
    let owner = window.hwnd().map_err(|e| e.to_string())?;
    let child = unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        // owned by the visualizer (the "parent" of a popup is its owner): kept
        // above it, hidden with it when it's minimized; no taskbar button, no
        // Alt+Tab entry, never activated by itself
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS,
            w!("MilkDrop"),
            WS_POPUP | WS_CLIPSIBLINGS,
            0,
            0,
            rect.width.max(1) as i32,
            rect.height.max(1) as i32,
            Some(owner),
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?
    };
    place(child, owner, rect);

    if let Some(dir) = user_presets_dir() {
        let _ = std::fs::create_dir_all(dir);
    }
    let dirs = preset_dirs(&app);
    // the bundled textures, then the user's folder (their own)
    let mut textures: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        textures.push(dir.join("textures"));
    }
    textures.extend(user_presets_dir());

    let source = if crate::settings::Settings::current().controller_mode { Source::Loopback } else { Source::Player };
    if let Ok(mut feed) = FEED.lock() {
        feed.clear();
    }
    FEEDING.store(source as u8, Ordering::Relaxed);

    let status = Arc::new(Mutex::new(Status::default()));
    let (commands, receiver) = mpsc::channel();
    let child_id = child.0 as isize;
    let thread = {
        let status = status.clone();
        let app = app.clone();
        std::thread::Builder::new()
            .name("milkdrop".into())
            .spawn(move || {
                let result = render(app.clone(), api, child_id, (rect.width, rect.height), dirs, textures, receiver, status.clone());
                if let Err(e) = &result {
                    log::warn!("MilkDrop stopped: {e}");
                    let _ = app.emit_to("visualizer", "milkdropError", e.clone());
                }
                set_status(&status, |s| {
                    s.running = false;
                    s.error = result.err();
                });
            })
            .map_err(|e| e.to_string())?
    };
    *running = Some(Running { child: child_id, owner: owner.0 as isize, rect, commands, thread, status });
    Ok(())
}

/// Move it with the canvas (a resize, fullscreen, a zoom change).
#[tauri::command]
pub fn milkdrop_resize(rect: Rect) {
    if let Ok(mut running) = RUNNING.lock()
        && let Some(r) = running.as_mut()
    {
        r.rect = rect;
        place(HWND(r.child as *mut c_void), HWND(r.owner as *mut c_void), rect);
        let _ = r.commands.send(Command::Resize(rect.width, rect.height));
    }
}

/// The visualizer window moved (or resized): the picture goes with it. From
/// its window events, on the main thread.
pub fn follow() {
    // (try: it's busy only while starting or stopping, and then it's placed anyway)
    if let Ok(running) = RUNNING.try_lock()
        && let Some(r) = running.as_ref()
    {
        place(HWND(r.child as *mut c_void), HWND(r.owner as *mut c_void), r.rect);
    }
}

fn stop_locked(running: &mut Option<Running>) {
    FEEDING.store(0, Ordering::Relaxed);
    if let Some(r) = running.take() {
        let child = HWND(r.child as *mut c_void);
        unsafe {
            let _ = ShowWindow(child, SW_HIDE);
        }
        // dropping the sender ends its loop; it lets go of the GL context
        // before the window it draws in is destroyed
        drop(r.commands);
        // A preset can hold projectM for good: then the thread is left to it
        // (hidden, its window kept for it) rather than the app hanging too;
        // that preset is skipped from the next start on.
        let until = Instant::now() + Duration::from_secs(3);
        while !r.thread.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        if r.thread.is_finished() {
            let _ = r.thread.join();
            unsafe {
                let _ = DestroyWindow(child);
            }
        } else {
            log::warn!("MilkDrop: its thread is stuck in a preset; left behind");
        }
    }
    if let Ok(mut feed) = FEED.lock() {
        feed.clear();
    }
}

/// Stop and put the window away (MilkDrop off, or the visualizer closing).
/// On the main thread: only the thread that made a window may destroy it.
#[tauri::command]
pub fn milkdrop_stop() {
    if let Ok(mut running) = RUNNING.lock() {
        stop_locked(&mut running);
    }
    CURSOR_HIDDEN.store(false, Ordering::Relaxed);
}

/// From anywhere: stop it on the main thread (the visualizer is closing).
/// Queued before the window's own destruction, so it goes first.
pub fn stop_on_main(app: &AppHandle) {
    let _ = app.run_on_main_thread(milkdrop_stop);
}

/// Hide the pointer over the picture (or show it again).
#[tauri::command]
pub fn milkdrop_cursor(hidden: bool) {
    CURSOR_HIDDEN.store(hidden, Ordering::Relaxed);
}

/// Put the picture away (the page shows the preset list there) or back.
/// While it's away nothing is drawn; a preset picked meanwhile is on when
/// it's back.
#[tauri::command]
pub fn milkdrop_hide(hidden: bool) {
    PICTURE_HIDDEN.store(hidden, Ordering::Relaxed);
    if let Ok(running) = RUNNING.lock()
        && let Some(r) = running.as_ref()
    {
        unsafe {
            let _ = ShowWindow(HWND(r.child as *mut c_void), if hidden { SW_HIDE } else { SW_SHOWNA });
        }
    }
}

/// Next (or previous) preset, blended in.
#[tauri::command]
pub fn milkdrop_step(forward: bool) {
    if let Ok(running) = RUNNING.lock()
        && let Some(r) = running.as_ref()
    {
        let _ = r.commands.send(if forward { Command::Next } else { Command::Previous });
    }
}

/// Text over the picture: `kind` "title" or "hint" with its text (empty
/// takes it away), or "clear" for none. The page sends these in fullscreen.
#[tauri::command]
pub fn milkdrop_overlay(kind: String, text: String) {
    let command = match kind.as_str() {
        "title" => Command::Overlay(OverlayKind::Title, Some(text)),
        "hint" => Command::Overlay(OverlayKind::Hint, Some(text)),
        _ => Command::ClearOverlays,
    };
    if let Ok(running) = RUNNING.lock()
        && let Some(r) = running.as_ref()
    {
        let _ = r.commands.send(command);
    }
}

/// Open the user's presets folder (made first if needed) in Explorer.
#[tauri::command(async)]
pub fn milkdrop_open_folder() -> Result<(), String> {
    let dir = user_presets_dir().ok_or("no config folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer").arg(&dir).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Pin the preset on screen (no changes by time), or let them change again.
#[tauri::command]
pub fn milkdrop_lock(locked: bool) {
    if let Ok(running) = RUNNING.lock()
        && let Some(r) = running.as_ref()
    {
        let _ = r.commands.send(Command::Lock(locked));
    }
}

/// The bottom line over the picture: the preset's name, MILKDROP and PIN.
#[tauri::command]
pub fn milkdrop_bar(bar: Bar) {
    if let Ok(running) = RUNNING.lock()
        && let Some(r) = running.as_ref()
    {
        let _ = r.commands.send(Command::Bar(bar));
    }
}

#[tauri::command]
pub fn milkdrop_status() -> Status {
    let mut status: Status = RUNNING
        .lock()
        .ok()
        .and_then(|r| r.as_ref().and_then(|r| r.status.lock().ok().map(|s| s.clone())))
        .unwrap_or_default();
    status.buttons = BAR_HITS.lock().ok().and_then(|h| *h).map(|h| [h.milk, h.pin]);
    status
}

/// The next frame, saved as a PNG (for the smoke test).
#[tauri::command(async)]
pub fn milkdrop_snapshot(path: String) -> Result<(), String> {
    let (reply, answer) = mpsc::channel();
    {
        let running = RUNNING.lock().map_err(|_| "busy")?;
        let r = running.as_ref().ok_or("MilkDrop isn't running")?;
        r.commands
            .send(Command::Snapshot(PathBuf::from(path), reply))
            .map_err(|_| "MilkDrop stopped")?;
    }
    answer.recv_timeout(Duration::from_secs(3)).map_err(|_| "no frame".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    // (on a local queue: the shared one is fed by the other tests' audio)
    #[test]
    fn the_feed_keeps_stereo_doubles_mono_and_stays_bounded() {
        let mut feed = VecDeque::new();
        append(&mut feed, &[0.1, -0.1, 0.2, -0.2], false);
        append(&mut feed, &[0.3, -0.3], true);
        assert_eq!(feed.iter().copied().collect::<Vec<_>>(), [0.1, -0.1, 0.2, -0.2, 0.3, 0.3, -0.3, -0.3]);

        // the renderer fell behind: only the newest half second is kept,
        // the newest last
        let mut lots = vec![0.25f32; FEED_MAX * 3];
        *lots.last_mut().unwrap() = 1.0;
        append(&mut feed, &lots, false);
        assert_eq!(feed.len(), FEED_MAX);
        assert_eq!(feed.back(), Some(&1.0));
    }

    #[test]
    fn titles_are_drawn_into_a_mask_and_cut_short_when_too_long() {
        let (mask, w, h) = text_mask("Şebnem Ferah - Sigara", 40, true, 2000, w!("Segoe UI")).expect("a mask");
        assert_eq!(mask.len(), (w * h) as usize);
        assert!(h >= 40 && w > 200);
        // some ink, but mostly the clear ground
        let inked = mask.iter().filter(|&&a| a > 128).count();
        assert!(inked > 100 && inked < mask.len() / 2);
        // a long one stops at the width it's given
        let (_, w, _) = text_mask(&"long title ".repeat(40), 40, true, 600, w!("Segoe UI")).expect("a mask");
        assert_eq!(w, 600);
        assert!(text_mask("", 40, true, 600, w!("Segoe UI")).is_none());
    }

    #[test]
    fn a_folder_with_any_letters_reaches_projectm_as_plain_ascii() {
        let dir = std::env::temp_dir().join(format!("spotiamp-doku-Ömer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // a drive without short names has no such form: then it's left out
        if let Some(path) = ansi_path(&dir) {
            let path = path.to_str().unwrap().to_string();
            assert!(path.is_ascii(), "{path}");
            assert!(Path::new(&path).is_dir());
        }
        assert!(ansi_path(&dir.join("not there")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ansi_path(&std::env::temp_dir()).is_some());
    }

    #[test]
    fn the_pool_skips_slow_ones_only_at_their_size_and_falls_back_from_favourites() {
        let mut pool = Pool::new(4);
        pool.broken[0] = true;
        pool.slow_at[1] = 1920 * 1080;
        assert_eq!(pool.skip(1920 * 1080), [true, true, false, false]);
        // out of fullscreen, a smaller picture: back in
        assert_eq!(pool.skip(320 * 240), [true, false, false, false]);
        pool.only = Some(vec![false, false, true, false]);
        assert_eq!(pool.skip(320 * 240), [true, true, false, true]);
        // the only favourite broken: any will do
        pool.only = Some(vec![true, false, false, false]);
        assert_eq!(pool.skip(320 * 240), [true, false, false, false]);
    }

    #[test]
    fn a_preset_is_known_by_its_place_in_its_folder() {
        let dirs = vec![PathBuf::from(r"C:\bundled\presets")];
        assert_eq!(preset_key(Path::new(r"C:\bundled\presets\Dancer\x y.milk"), &dirs), "Dancer/x y.milk");
        assert_eq!(preset_key(Path::new(r"C:\elsewhere\z.milk"), &dirs), r"C:\elsewhere\z.milk");
    }

    #[test]
    fn the_bottom_line_keeps_the_name_off_the_buttons() {
        // 320 x 240, 4 px in, 11 px letters; MILKDROP 60 wide, PINNED 46
        let l = bar_layout(320, 240, 4, 11, 60, 46);
        assert_eq!(l.pin, (270, 225, 316, 236));
        assert_eq!(l.milk, (199, 225, 259, 236));
        assert_eq!(l.name_room, 199 - 11 - 4);
        // too narrow for a name: no room, never less
        assert_eq!(bar_layout(100, 50, 4, 11, 60, 46).name_room, 0);
    }

    #[test]
    fn a_click_on_a_shown_button_is_that_button() {
        set_hits(Some(BarHits { milk: (10, 10, 20, 20), pin: (30, 10, 40, 20) }));
        assert_eq!(button_at(15, 15), Some("milkdropButton"));
        assert_eq!(button_at(39, 10), Some("pinButton"));
        assert_eq!(button_at(25, 15), None);
        assert_eq!(button_at(20, 15), None);
        set_hits(None);
        assert_eq!(button_at(15, 15), None);
        // off the left edge (a drag) reads as negative
        assert_eq!(point_of(LPARAM((0xFFFB | (7 << 16)) as isize)), (-5, 7));
    }

    #[test]
    fn the_next_preset_skips_bad_ones_and_gives_up_when_all_are() {
        let bad = [false, true, false, true];
        assert_eq!(next_index(&bad, 0, true), Some(2));
        assert_eq!(next_index(&bad, 2, true), Some(0));
        assert_eq!(next_index(&bad, 0, false), Some(2));
        // only one left: that one, even from itself
        assert_eq!(next_index(&[true, false, true], 1, true), Some(1));
        assert_eq!(next_index(&[true, true], 0, true), None);
    }

    #[test]
    fn a_preset_is_read_from_a_path_with_any_letters() {
        let dir = std::env::temp_dir().join(format!("spotiamp-masaüstü-Ömer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("şarkı ğ - deneme.milk");
        std::fs::write(&path, b"[preset00]\nzoom=1.0\0\n").unwrap();
        let text = read_preset(&path).expect("read");
        assert_eq!(text.as_bytes(), b"[preset00]\nzoom=1.0\n");
        assert!(read_preset(&dir.join("missing.milk")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn presets_are_found_in_folders_and_shuffled_without_loss() {
        let dir = std::env::temp_dir().join(format!("spotiamp-milk-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Pack A")).unwrap();
        for name in ["one.milk", "Pack A/two.MILK", "Pack A/notes.txt"] {
            std::fs::write(dir.join(name), b"[preset00]").unwrap();
        }
        let mut found = find_presets(std::slice::from_ref(&dir));
        assert_eq!(found.len(), 2);
        shuffle(&mut found);
        let mut names: Vec<String> = found.iter().map(|p| preset_name(p)).collect();
        names.sort();
        assert_eq!(names, ["one", "two"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
