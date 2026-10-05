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
#![cfg(target_os = "windows")]

use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetDC, HDC, ReleaseDC};
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
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CS_OWNDC, CreateWindowExW, DefWindowProcW, DestroyWindow, GW_OWNER, GetForegroundWindow, GetWindow,
    HCURSOR, HWND_TOP, IDC_ARROW, IsIconic, IsWindow, IsWindowVisible, LoadCursorW, MA_NOACTIVATE, RegisterClassW,
    SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursor, SetForegroundWindow, SetWindowPos, ShowWindow,
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
    on_switch_requested: unsafe extern "C" fn(Handle, Option<SwitchRequested>, *mut c_void),
    on_switch_failed: unsafe extern "C" fn(Handle, Option<SwitchFailed>, *mut c_void),
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
            on_switch_requested: sym!(projectm, "projectm_set_preset_switch_requested_event_callback"),
            on_switch_failed: sym!(projectm, "projectm_set_preset_switch_failed_event_callback"),
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

/// The user's own presets (any folders inside are searched too).
fn user_presets_dir() -> Option<PathBuf> {
    crate::settings::get_config_dir().map(|d| d.join("milkdrop"))
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
/// When the last "move" went to the page (ms since the epoch): a few a second
/// are plenty to wake its idle timer.
static LAST_MOVE: AtomicU64 = AtomicU64::new(0);

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
            Some("click")
        }
        WM_LBUTTONDBLCLK => Some("dblclick"),
        WM_RBUTTONUP => Some("contextmenu"),
        WM_MOUSEMOVE => {
            let now = now_ms();
            (now.saturating_sub(LAST_MOVE.load(Ordering::Relaxed)) > 150).then(|| {
                LAST_MOVE.store(now, Ordering::Relaxed);
                "move"
            })
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
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
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

/// White text as an 8-bit alpha mask (top row first) and its size; longer
/// than `max_width` ends in an ellipsis.
fn text_mask(text: &str, px: i32, bold: bool, max_width: i32) -> Option<(Vec<u8>, i32, i32)> {
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
            w!("Segoe UI"),
        );
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        let mut extent = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &wide, &mut extent);
        let pad = px / 4 + 2;
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

/// (Re)make an overlay's texture for the window's size.
fn build_overlay(o: &mut Overlay, width: u32, height: u32) {
    unsafe {
        if o.texture != 0 {
            glDeleteTextures(1, &o.texture);
            o.texture = 0;
        }
        let max_width = (width as f32 * 0.9) as i32;
        let Some((mask, w, h)) = text_mask(&o.text, o.kind.px(height), o.kind == OverlayKind::Title, max_width) else {
            return;
        };
        let mut texture = 0u32;
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
        o.texture = texture;
        o.size = (w, h);
        o.for_height = height;
    }
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
        glOrtho(0.0, w as f64, h as f64, 0.0, -1.0, 1.0);
        glMatrixMode(GL_MODELVIEW);
        glLoadIdentity();
        glEnable(GL_TEXTURE_2D);
        glEnable(GL_BLEND);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        glTexEnvi(GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_MODULATE as i32);
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
            glBindTexture(GL_TEXTURE_2D, o.texture);
            for (dx, color) in [(shadow, [0.0, 0.0, 0.0, 0.8 * fade]), (0.0, [1.0, 1.0, 1.0, fade])] {
                glColor4f(color[0], color[1], color[2], color[3]);
                glBegin(GL_QUADS);
                glTexCoord2f(0.0, 0.0);
                glVertex2f(x + dx, y + dx);
                glTexCoord2f(1.0, 0.0);
                glVertex2f(x + dx + tw, y + dx);
                glTexCoord2f(1.0, 1.0);
                glVertex2f(x + dx + tw, y + dx + th);
                glTexCoord2f(0.0, 1.0);
                glVertex2f(x + dx, y + dx + th);
                glEnd();
            }
        }
        // as projectM expects it
        glBindTexture(GL_TEXTURE_2D, 0);
        glDisable(GL_BLEND);
        glDisable(GL_TEXTURE_2D);
        glColor4f(1.0, 1.0, 1.0, 1.0);
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
    failed: Cell<bool>,
}

unsafe extern "C" fn switch_requested(_hard_cut: bool, data: *mut c_void) {
    if let Some(signals) = unsafe { (data as *const Signals).as_ref() } {
        signals.switch.set(true);
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
        (api.set_soft_cut_duration)(handle, BLEND_SECONDS);
    }

    // here, not on the main thread: a big collection takes a moment to list
    let mut presets = find_presets(&preset_dirs);
    shuffle(&mut presets);
    set_status(&status, |s| {
        s.running = true;
        s.presets = presets.len();
    });
    // presets that are too slow here, or wouldn't load: skipped from now on
    let mut bad = vec![false; presets.len()];
    let mut loaded_at = Instant::now();
    let mut measured = (Duration::ZERO, 0u32);
    // put preset `i` on: false if it couldn't be read
    let show = |i: usize, smooth: bool| -> bool {
        let Some(text) = presets.get(i).and_then(|p| read_preset(p)) else {
            return false;
        };
        unsafe { (api.load_preset_data)(engine.handle, text.as_ptr(), smooth) };
        let name = preset_name(&presets[i]);
        set_status(&status, |s| s.preset = name.clone());
        let _ = app.emit_to("visualizer", "milkdropPreset", name);
        true
    };
    // on to the next usable preset from `from`, skipping unreadable ones; if
    // none is left, say so and stay (no retrying the same one every frame)
    let go = |from: usize, forward: bool, smooth: bool, bad: &mut Vec<bool>, index: &mut usize| {
        let mut from = from;
        loop {
            let Some(i) = next_index(bad, from, forward) else {
                set_status(&status, |s| s.preset = String::new());
                let _ = app.emit_to("visualizer", "milkdropPreset", "none of the presets would load");
                return;
            };
            if show(i, smooth) {
                *index = i;
                return;
            }
            log::warn!("MilkDrop: couldn't read {}", presets[i].display());
            bad[i] = true;
            set_status(&status, |s| s.skipped += 1);
            from = i;
        }
    };
    let mut index = presets.len().saturating_sub(1);
    if !presets.is_empty() {
        // from the last one round to the first
        go(index, true, false, &mut bad, &mut index);
    }

    let max_chunk = unsafe { (api.pcm_max_samples)() }.max(1) as usize;
    let mut pcm = Vec::new();
    let mut frames = 0u64;
    let mut second = (Instant::now(), 0u32);
    let mut locked = false;
    let calls = gl_calls();
    let mut overlays: Vec<Overlay> = Vec::new();
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
                    go(index, true, true, &mut bad, &mut index);
                    loaded_at = Instant::now();
                }
                Ok(Command::Previous) if !presets.is_empty() => {
                    go(index, false, true, &mut bad, &mut index);
                    loaded_at = Instant::now();
                }
                Ok(Command::Lock(on)) => locked = on,
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
        }
        if measuring {
            unsafe { glFinish() };
            measured.0 += started.elapsed();
            measured.1 += 1;
        } else if measured.1 > 0 {
            let avg_ms = measured.0.as_secs_f64() * 1000.0 / measured.1 as f64;
            measured = (Duration::ZERO, 0);
            if avg_ms > SLOW_MS && !presets.is_empty() {
                log::info!("MilkDrop: {} is too slow here ({avg_ms:.1} ms a frame), skipped", preset_name(&presets[index]));
                bad[index] = true;
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
        if signals.failed.get() && !presets.is_empty() && !bad[index] {
            bad[index] = true;
            set_status(&status, |s| s.skipped += 1);
            advance = Some(false);
        }
        if signals.switch.get() && !locked {
            advance = advance.or(Some(true));
        }
        signals.switch.set(false);
        signals.failed.set(false);
        if let Some(smooth) = advance
            && !presets.is_empty()
        {
            go(index, true, smooth, &mut bad, &mut index);
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
    let mut dirs: Vec<PathBuf> = user_presets_dir().into_iter().collect();
    if let Ok(dir) = app.path().resource_dir() {
        dirs.push(dir.join("presets"));
    }
    Ok(find_presets(&dirs).len())
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

    let mut dirs: Vec<PathBuf> = user_presets_dir().into_iter().collect();
    if let Some(dir) = dirs.first() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(dir) = app.path().resource_dir() {
        dirs.push(dir.join("presets"));
    }

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
                let result = render(app.clone(), api, child_id, (rect.width, rect.height), dirs, receiver, status.clone());
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
        let _ = r.thread.join();
        unsafe {
            let _ = DestroyWindow(child);
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

#[tauri::command]
pub fn milkdrop_status() -> Status {
    RUNNING
        .lock()
        .ok()
        .and_then(|r| r.as_ref().and_then(|r| r.status.lock().ok().map(|s| s.clone())))
        .unwrap_or_default()
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
        let (mask, w, h) = text_mask("Şebnem Ferah - Sigara", 40, true, 2000).expect("a mask");
        assert_eq!(mask.len(), (w * h) as usize);
        assert!(h >= 40 && w > 200);
        // some ink, but mostly the clear ground
        let inked = mask.iter().filter(|&&a| a > 128).count();
        assert!(inked > 100 && inked < mask.len() / 2);
        // a long one stops at the width it's given
        let (_, w, _) = text_mask(&"long title ".repeat(40), 40, true, 600).expect("a mask");
        assert_eq!(w, 600);
        assert!(text_mask("", 40, true, 600).is_none());
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
