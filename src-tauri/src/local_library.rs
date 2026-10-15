//! Local Media (0.7.8): the music folders added in the Library, listed there
//! like Winamp's own Local Media. Each file is read once (its tags and length)
//! and kept in local-library.json; it's read again only when its size or date
//! changes, so even a big folder opens at once. Reading runs on a thread of its
//! own and tells the Library how far it got.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use crate::settings::{get_config_dir, Settings};

/// One audio file of the folders.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LocalItem {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub sample_rate: u32,
    pub kbps: u32,
    /// The file's date (last modified), epoch ms: the Library's Date column.
    pub date_ms: i64,
    /// Size when it was read: with the date, how a changed file is noticed.
    pub size: u64,
}

/// What the Library shows: the folders, their files, and whether they're
/// being read right now.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalLibrary {
    pub folders: Vec<String>,
    pub items: Vec<LocalItem>,
    pub scanning: bool,
}

/// How far a reading has got, for the Library's "reading 120 of 900…".
#[derive(Debug, Clone, serde::Serialize)]
struct ScanProgress {
    done: usize,
    total: usize,
    scanning: bool,
}

#[derive(Default)]
struct State {
    /// None until local-library.json is first needed.
    items: Option<Vec<LocalItem>>,
    scanning: bool,
    /// Asked again while reading (a folder added or taken out): once more after.
    again: bool,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn index_path() -> Option<PathBuf> {
    get_config_dir().map(|d| d.join("local-library.json"))
}

fn load_index() -> Vec<LocalItem> {
    index_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_index(items: &[LocalItem]) {
    let Some(path) = index_path() else { return };
    // written next to it first, so a crash mid-write never leaves half a file
    let tmp = path.with_extension("json.tmp");
    let saved = serde_json::to_vec(items)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&tmp, bytes).map_err(|e| e.to_string()))
        .and_then(|()| std::fs::rename(&tmp, &path).map_err(|e| e.to_string()));
    if let Err(e) = saved {
        log::warn!("Could not save the local library ({e})");
    }
}

/// The files known so far (read from disk the first time).
fn items() -> Vec<LocalItem> {
    let mut s = state();
    s.items.get_or_insert_with(load_index).clone()
}

/// A file's size and date (epoch ms), or None when it can't be looked at.
fn stamp(path: &Path) -> Option<(u64, i64)> {
    let md = std::fs::metadata(path).ok()?;
    let date_ms = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some((md.len(), date_ms))
}

fn read_item(path: &str, size: u64, date_ms: i64) -> LocalItem {
    let meta = crate::local_player::read_local_meta(Path::new(path)).unwrap_or_default();
    LocalItem {
        path: path.to_string(),
        title: meta.title,
        artist: meta.artist,
        album: meta.album,
        duration_ms: meta.duration_ms,
        sample_rate: meta.sample_rate,
        kbps: meta.kbps,
        date_ms,
        size,
    }
}

/// Every audio file under `folders`, in path order: the known ones as they
/// were when unchanged, the new and changed ones read now (on a few threads).
/// `progress(done, total)` is called as files are read.
fn scan(folders: &[String], known: Vec<LocalItem>, progress: &(dyn Fn(usize, usize) + Sync)) -> Vec<LocalItem> {
    let mut files = Vec::new();
    for folder in folders {
        crate::local_player::collect_audio_files(Path::new(folder), &mut files, 0);
    }
    // a folder inside another one that's also added: its files once
    files.sort();
    files.dedup();

    let mut known: HashMap<String, LocalItem> = known.into_iter().map(|i| (i.path.clone(), i)).collect();
    let mut out: Vec<Option<LocalItem>> = Vec::with_capacity(files.len());
    let mut to_read = Vec::new();
    for path in &files {
        let Some((size, date_ms)) = stamp(Path::new(path)) else {
            out.push(None);
            continue;
        };
        match known.remove(path) {
            Some(item) if item.size == size && item.date_ms == date_ms => out.push(Some(item)),
            _ => {
                to_read.push((out.len(), path.clone(), size, date_ms));
                out.push(None);
            }
        }
    }

    let total = to_read.len();
    if total > 0 {
        let done = AtomicUsize::new(0);
        let next = AtomicUsize::new(0);
        let read = Mutex::new(Vec::with_capacity(total));
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 4));
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    while let Some((slot, path, size, date_ms)) = to_read.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let item = read_item(path, *size, *date_ms);
                        read.lock().unwrap_or_else(|e| e.into_inner()).push((*slot, item));
                        progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
                    }
                });
            }
        });
        for (slot, item) in read.into_inner().unwrap_or_else(|e| e.into_inner()) {
            out[slot] = Some(item);
        }
    }
    out.into_iter().flatten().collect()
}

/// Read the folders again on a thread of its own (only what changed is read
/// for real). Asked while one runs, it runs once more after it.
fn start_scan(app: &AppHandle) {
    {
        let mut s = state();
        if s.scanning {
            s.again = true;
            return;
        }
        s.scanning = true;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        let folders = Settings::current().local_folders.clone();
        let known = items();
        let last = Mutex::new(Instant::now());
        let progress = |done: usize, total: usize| {
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            if done == total || last.elapsed() >= Duration::from_millis(250) {
                *last = Instant::now();
                let _ = app.emit("localLibraryScan", ScanProgress { done, total, scanning: true });
            }
        };
        let found = scan(&folders, known, &progress);
        let changed = {
            let mut s = state();
            let changed = s.items.as_ref() != Some(&found);
            s.items = Some(found.clone());
            changed
        };
        if changed {
            save_index(&found);
            log::info!("Local library: {} files in {} folders", found.len(), folders.len());
            let _ = app.emit("localLibraryChanged", ());
        }
        let mut s = state();
        if s.again {
            s.again = false;
            continue;
        }
        s.scanning = false;
        drop(s);
        let _ = app.emit("localLibraryScan", ScanProgress { done: 0, total: 0, scanning: false });
        break;
    });
}

fn library() -> LocalLibrary {
    let items = items();
    LocalLibrary {
        folders: Settings::current().local_folders.clone(),
        items,
        scanning: state().scanning,
    }
}

/// The folders and what's known of their files; `rescan` also checks them
/// for new, changed and removed files (the answer comes as an event).
#[tauri::command(async)]
pub fn local_library(rescan: bool, app_handle: AppHandle) -> LocalLibrary {
    if rescan && !Settings::current().local_folders.is_empty() {
        start_scan(&app_handle);
    }
    library()
}

/// Pick a folder and add it (if it isn't there already), then read it.
#[tauri::command]
pub async fn local_library_add_folder(app_handle: AppHandle) -> LocalLibrary {
    use tauri_plugin_dialog::DialogExt;
    let picked = app_handle
        .dialog()
        .file()
        .set_title("Add a music folder")
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok());
    if let Some(dir) = picked {
        let dir = dir.to_string_lossy().into_owned();
        let added = {
            let mut settings = Settings::current_mut();
            let have = settings.local_folders.iter().any(|f| f.eq_ignore_ascii_case(&dir));
            if !have {
                settings.local_folders.push(dir.clone());
            }
            !have
        };
        if added {
            log::info!("Local library: a folder added");
            start_scan(&app_handle);
        }
    }
    library()
}

/// Take a folder out (its files leave the list; nothing on disk is touched).
#[tauri::command(async)]
pub fn local_library_remove_folder(folder: String, app_handle: AppHandle) -> LocalLibrary {
    Settings::current_mut().local_folders.retain(|f| f != &folder);
    start_scan(&app_handle);
    library()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, seconds: u32) {
        let data = 8000 * 2 * seconds;
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&8000u32.to_le_bytes());
        w.extend_from_slice(&16000u32.to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data.to_le_bytes());
        w.resize(w.len() + data as usize, 0);
        std::fs::write(path, w).unwrap();
    }

    #[test]
    fn a_folder_is_read_once_then_only_what_changed() {
        let dir = std::env::temp_dir().join(format!("spotiamp-locallib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        wav(&dir.join("a.wav"), 1);
        wav(&dir.join("inner").join("b.wav"), 2);
        std::fs::write(dir.join("notes.txt"), "not music").unwrap();
        let root = dir.to_string_lossy().into_owned();
        let inner = dir.join("inner").to_string_lossy().into_owned();

        // the folder and one inside it: each file once
        let first = scan(&[root.clone(), inner.clone()], Vec::new(), &|_, _| {});
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].duration_ms, 1000);
        assert_eq!(first[1].duration_ms, 2000);

        // nothing changed: nothing is read again
        let reads = AtomicUsize::new(0);
        let again = scan(std::slice::from_ref(&root), first.clone(), &|_, _| {
            reads.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(again, first);
        assert_eq!(reads.load(Ordering::Relaxed), 0);

        // one file changed, one gone: that one read again, the other left out
        wav(&dir.join("a.wav"), 3);
        std::fs::remove_file(dir.join("inner").join("b.wav")).unwrap();
        let reads = AtomicUsize::new(0);
        let after = scan(&[root], first, &|_, _| {
            reads.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].duration_ms, 3000);
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
