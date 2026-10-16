//! Music files dropped from Windows Explorer (0.7.8). The windows are built
//! without Tauri's own drop handler, which would stop drags between our own
//! windows (a song from the Library onto the playlist), so a dropped file
//! reaches the page as a browser File, which has no path. The page passes
//! those to WebView2 (`postMessageWithAdditionalObjects`), and here each
//! one's path is read and the audio files among them (a folder: its audio
//! files) go back to the window as "filesDropped". The message is the text
//! "spotiamp-files": WebView2 stops at Tauri's handler for anything that
//! isn't text, so an object never got here (Tauri, which can't read it as a
//! call, notes it in the page's console, nothing more).

use tauri::{Emitter, WebviewWindow};

#[cfg(windows)]
pub fn listen(window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs2,
    };
    use webview2_com::{take_pwstr, WebMessageReceivedEventHandler};
    use windows::core::{Interface, PWSTR};

    let label = window.label().to_string();
    let app = tauri::Manager::app_handle(window).clone();
    let _ = window.with_webview(move |webview| unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut text = PWSTR::null();
            if args.TryGetWebMessageAsString(&mut text).is_err() || take_pwstr(text) != "spotiamp-files" {
                return Ok(());
            }
            let objects = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>()?.AdditionalObjects()?;
            let mut count = 0u32;
            objects.Count(&mut count)?;
            let mut dropped = Vec::new();
            for i in 0..count {
                if let Ok(file) = objects.GetValueAtIndex(i).and_then(|o| o.cast::<ICoreWebView2File>()) {
                    let mut path = PWSTR::null();
                    if file.Path(&mut path).is_ok() {
                        dropped.push(take_pwstr(path));
                    }
                }
            }
            let paths = audio_files(dropped);
            log::info!("{} file(s) dropped on the {label} window", paths.len());
            if !paths.is_empty() {
                let _ = app.emit_to(label.as_str(), "filesDropped", paths);
            }
            Ok(())
        }));
        let mut token = Default::default();
        if let Err(e) = core.add_WebMessageReceived(&handler, &mut token) {
            log::warn!("couldn't listen for dropped files ({e})");
        }
    });
}

#[cfg(not(windows))]
pub fn listen(_window: &WebviewWindow) {}

/// The audio files among what was dropped, a folder's (and its folders') in
/// path order, in the order they were dropped.
fn audio_files(dropped: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for path in dropped {
        let p = std::path::Path::new(&path);
        if p.is_dir() {
            let mut found = Vec::new();
            crate::local_player::collect_audio_files(p, &mut found, 0);
            found.sort();
            out.extend(found);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| crate::local_player::AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_files_and_folders_but_nothing_else() {
        let dir = std::env::temp_dir().join(format!("spotiamp-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("album")).unwrap();
        for f in ["album/02 b.flac", "album/01 a.mp3", "album/cover.jpg", "song.MP3", "notes.txt"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let p = |f: &str| f.split('/').fold(dir.clone(), |d, part| d.join(part)).to_string_lossy().into_owned();
        let got = audio_files(vec![p("song.MP3"), p("notes.txt"), p("album")]);
        assert_eq!(got, vec![p("song.MP3"), p("album/01 a.mp3"), p("album/02 b.flac")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
