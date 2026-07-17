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
