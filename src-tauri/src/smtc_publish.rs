//! Publish Spotiamp+ as a Windows media session (SMTC).
//!
//! This is what makes the headset's transport buttons work, and it puts the
//! track in the panel Windows shows when you press a volume or media key.
//!
//! It replaces an earlier attempt that registered the media keys as global
//! hotkeys. That approach took the keys away from *every* other program: with
//! Spotiamp+ running, pausing a video in the browser started our playback
//! instead. Windows already solves this — a registered session receives the
//! keys only while it is the one that played most recently — so the fix was to
//! stop seizing them and register properly instead. Verified in an isolated
//! probe first: Spotify kept receiving its own keys throughout, and its session
//! (the one controller mode reads) was never disturbed.
//!
//! Two rules this module exists to keep:
//!
//!   * Nothing is published in controller mode. There the official Spotify app
//!     owns playback and already answers these keys.
//!   * The status we report has to be the truth. Claiming "playing" while idle
//!     would pull the keys onto us anyway — the same bug through a politer door.

#![cfg(target_os = "windows")]

use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Emitter, Manager};
use windows::{
    core::{factory, HSTRING},
    Foundation::TypedEventHandler,
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
        SystemMediaTransportControlsButton as Button,
        SystemMediaTransportControlsButtonPressedEventArgs,
    },
    Win32::System::WinRT::ISystemMediaTransportControlsInterop,
};

fn controls() -> &'static Mutex<Option<SystemMediaTransportControls>> {
    static CONTROLS: OnceLock<Mutex<Option<SystemMediaTransportControls>>> = OnceLock::new();
    CONTROLS.get_or_init(|| Mutex::new(None))
}

/// Attach a media session to the player window.
///
/// Best-effort throughout: a machine that refuses the session should lose the
/// headset buttons, not the ability to play music.
pub fn init(app: &AppHandle) {
    if crate::settings::Settings::current().controller_mode {
        log::info!("smtc: not publishing — Spotify owns the session in controller mode");
        return;
    }
    let Some(window) = app.get_webview_window("player") else {
        log::warn!("smtc: no player window to attach to");
        return;
    };
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            log::warn!("smtc: couldn't get the player window handle ({e})");
            return;
        }
    };

    let interop =
        match factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>() {
            Ok(interop) => interop,
            Err(e) => {
                log::warn!("smtc: interop factory unavailable ({e:?})");
                return;
            }
        };
    // GetForWindow is generic over what it returns, so the type has to be named.
    let smtc: SystemMediaTransportControls = match unsafe { interop.GetForWindow(hwnd) } {
        Ok(smtc) => smtc,
        Err(e) => {
            log::warn!("smtc: GetForWindow failed ({e:?})");
            return;
        }
    };

    let setup = || -> windows::core::Result<()> {
        smtc.SetIsEnabled(true)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsStopEnabled(true)?;
        smtc.SetIsNextEnabled(true)?;
        smtc.SetIsPreviousEnabled(true)?;
        // Start closed, not playing: nothing is loaded yet, and announcing
        // "playing" here would make Windows route media keys to us before we
        // have anything to play.
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;

        let app = app.clone();
        smtc.ButtonPressed(&TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            let Some(args) = args.as_ref() else { return Ok(()) };
            // Reuses the `mediaKey` event the player window already handles, so
            // a press behaves exactly like the in-window shortcut for it.
            let action = match args.Button()? {
                Button::Play | Button::Pause => "playpause",
                Button::Stop => "stop",
                Button::Next => "next",
                Button::Previous => "previous",
                _ => return Ok(()),
            };
            if let Some(window) = app.get_webview_window("player") {
                let _ = window.emit("mediaKey", action);
            }
            Ok(())
        }))?;
        Ok(())
    };
    if let Err(e) = setup() {
        log::warn!("smtc: couldn't configure the session ({e:?})");
        return;
    }

    if let Ok(mut guard) = controls().lock() {
        *guard = Some(smtc);
    }
    log::info!("smtc: publishing a media session for the player window");
}

/// Report what is playing. Called on every track change so the Windows panel
/// and the lock screen stay in step with the player.
#[tauri::command]
pub fn smtc_set_track(title: String, artist: String, album: String) {
    let Ok(guard) = controls().lock() else { return };
    let Some(smtc) = guard.as_ref() else { return };
    let write = || -> windows::core::Result<()> {
        let updater = smtc.DisplayUpdater()?;
        updater.SetType(MediaPlaybackType::Music)?;
        let music = updater.MusicProperties()?;
        music.SetTitle(&HSTRING::from(&title))?;
        music.SetArtist(&HSTRING::from(&artist))?;
        music.SetAlbumTitle(&HSTRING::from(&album))?;
        updater.Update()?;
        Ok(())
    };
    if let Err(e) = write() {
        log::debug!("smtc: metadata update failed ({e:?})");
    }
}

/// Report whether we are actually playing. This must stay honest — Windows
/// hands the media keys to whoever most recently reported playback, so a stale
/// "playing" would silently take them from whatever the user is really using.
#[tauri::command]
pub fn smtc_set_playing(playing: bool) {
    let Ok(guard) = controls().lock() else { return };
    let Some(smtc) = guard.as_ref() else { return };
    let status = if playing {
        MediaPlaybackStatus::Playing
    } else {
        MediaPlaybackStatus::Paused
    };
    if let Err(e) = smtc.SetPlaybackStatus(status) {
        log::debug!("smtc: status update failed ({e:?})");
    }
}

/// Nothing loaded — drop out of the running order for media keys entirely.
#[tauri::command]
pub fn smtc_set_stopped() {
    let Ok(guard) = controls().lock() else { return };
    let Some(smtc) = guard.as_ref() else { return };
    let _ = smtc.SetPlaybackStatus(MediaPlaybackStatus::Stopped);
}
