//! Winamp's global hotkeys (0.7.9): Ctrl+Alt with Insert, Home, End, Page Up
//! and Page Down for play, pause, stop, previous and next, and with the arrows
//! for the volume and a 5 s jump, whatever window is in front. Off unless
//! turned on (Windows menu): Ctrl+Alt with the arrows means something in some
//! editors, and a hotkey belongs to whichever program takes it first.

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VIRTUAL_KEY, VK_DOWN,
    VK_END, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT};

use crate::settings::Settings;

/// The key (with Ctrl+Alt), its name for the log, whether holding it repeats,
/// and what the player does: the `mediaKey` actions the media keys send.
const KEYS: [(VIRTUAL_KEY, &str, bool, &str); 9] = [
    (VK_INSERT, "Insert", false, "play"),
    (VK_HOME, "Home", false, "pause"),
    (VK_END, "End", false, "stop"),
    (VK_PRIOR, "Page Up", false, "previous"),
    (VK_NEXT, "Page Down", false, "next"),
    (VK_UP, "Up", true, "volumeup"),
    (VK_DOWN, "Down", true, "volumedown"),
    (VK_LEFT, "Left", true, "back5"),
    (VK_RIGHT, "Right", true, "forward5"),
];

/// The thread that holds the keys (0: none).
static THREAD: AtomicU32 = AtomicU32::new(0);

#[tauri::command]
pub fn set_global_hotkeys(enabled: bool, app_handle: AppHandle) {
    Settings::current_mut().player.global_hotkeys = enabled;
    apply(&app_handle, enabled);
}

/// Take the keys, or let them go.
pub fn apply(app: &AppHandle, enabled: bool) {
    let running = THREAD.load(Ordering::SeqCst);
    if !enabled {
        // (a thread that's only just starting has no message queue yet)
        for _ in 0..20 {
            if running == 0 || unsafe { PostThreadMessageW(running, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        return;
    }
    if running != 0 {
        return;
    }
    let app = app.clone();
    let _ = std::thread::Builder::new().name("global hotkeys".into()).spawn(move || run(app));
}

fn run(app: AppHandle) {
    THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    let mut taken = Vec::new();
    for (i, (vk, name, repeats, _)) in KEYS.iter().enumerate() {
        let id = i as i32 + 1;
        let mods = MOD_CONTROL | MOD_ALT | if *repeats { HOT_KEY_MODIFIERS(0) } else { MOD_NOREPEAT };
        if unsafe { RegisterHotKey(None, id, mods, vk.0 as u32) }.is_ok() {
            taken.push(id);
        } else {
            log::warn!("global hotkey Ctrl+Alt+{name}: another program has it");
        }
    }
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if msg.message != WM_HOTKEY {
            continue;
        }
        if let Some((_, _, _, action)) = KEYS.get(msg.wParam.0.wrapping_sub(1))
            && let Some(player) = app.get_webview_window("player")
        {
            let _ = player.emit("mediaKey", *action);
        }
    }
    for id in taken {
        let _ = unsafe { UnregisterHotKey(None, id) };
    }
    THREAD.store(0, Ordering::SeqCst);
}
