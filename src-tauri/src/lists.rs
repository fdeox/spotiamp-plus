//! App-local named track lists (Winamp-style playlists kept inside Spotiamp+,
//! not written to the user's Spotify account). Backed by `Settings.saved_lists`.

use crate::settings::{SavedList, Settings};

#[tauri::command]
pub fn get_saved_lists() -> Vec<SavedList> {
    Settings::current().saved_lists.clone()
}

/// Create or overwrite a list with the given track uris.
#[tauri::command]
pub fn save_list(name: String, uris: Vec<String>) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let mut settings = Settings::current_mut();
    if let Some(list) = settings.saved_lists.iter_mut().find(|l| l.name == name) {
        list.uris = uris;
    } else {
        settings.saved_lists.push(SavedList { name, uris });
    }
}

#[tauri::command]
pub fn delete_list(name: String) {
    Settings::current_mut().saved_lists.retain(|l| l.name != name);
}

/// Spotify playlists pinned to the top of the Library (their uris, in order).
#[tauri::command]
pub fn get_pinned_playlists() -> Vec<String> {
    Settings::current().pinned_playlists.clone()
}

#[tauri::command]
pub fn set_playlist_pinned(uri: String, pinned: bool) {
    let mut settings = Settings::current_mut();
    settings.pinned_playlists.retain(|u| u != &uri);
    if pinned && !uri.is_empty() {
        settings.pinned_playlists.push(uri);
    }
}

/// Append a track to a list (creating the list if it doesn't exist). Ignores
/// duplicates so the same track isn't added twice.
#[tauri::command]
pub fn add_to_list(name: String, uri: String) {
    let name = name.trim().to_string();
    if name.is_empty() || uri.is_empty() {
        return;
    }
    let mut settings = Settings::current_mut();
    if let Some(list) = settings.saved_lists.iter_mut().find(|l| l.name == name) {
        if !list.uris.contains(&uri) {
            list.uris.push(uri);
        }
    } else {
        settings.saved_lists.push(SavedList {
            name,
            uris: vec![uri],
        });
    }
}

/// Loved songs (♡), newest first.
#[tauri::command]
pub fn get_loved() -> Vec<String> {
    Settings::current().loved.iter().rev().cloned().collect()
}

/// Love (or unlove) these tracks. Only Spotify tracks can be loved. Tells
/// every window, so an open Library or player follows along.
#[tauri::command]
pub fn set_loved(uris: Vec<String>, loved: bool, app: tauri::AppHandle) -> usize {
    use tauri::Emitter;
    let mut changed = 0;
    {
        let mut settings = Settings::current_mut();
        for uri in uris.iter().filter(|u| u.starts_with("spotify:track:")) {
            let has = settings.loved.contains(uri);
            if loved && !has {
                settings.loved.push(uri.clone());
                changed += 1;
            } else if !loved && has {
                settings.loved.retain(|u| u != uri);
                changed += 1;
            }
        }
    }
    if changed > 0 {
        // true when songs were loved (the llama shows a heart), false unloved
        let _ = app.emit("lovedChanged", loved);
    }
    changed
}
