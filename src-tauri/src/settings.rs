use std::{
    collections::hash_map::DefaultHasher,
    fs::{File, create_dir_all},
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter},
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tauri::LogicalPosition;

pub fn get_config_dir() -> Option<PathBuf> {
    let path =
        ProjectDirs::from("org.darkbits", "", "spotiamp").map(|pd| pd.config_dir().to_path_buf());
    if let Some(path) = path.clone()
        && let Err(e) = create_dir_all(path)
    {
        log::error!("Could not create path: {:?}", e);
    }
    path
}
fn get_settings_file_path() -> PathBuf {
    get_config_dir()
        .expect("a config directory")
        .join("settings.yaml")
}

pub struct AutoSavingSettings<'a> {
    inner: RwLockWriteGuard<'a, Settings>,
    hash_before: u64,
}

impl<'a> AutoSavingSettings<'a> {
    fn new(inner: &'a RwLock<Settings>) -> Self {
        let inner = inner.write().unwrap();
        AutoSavingSettings {
            hash_before: inner.get_hash(),
            inner,
        }
    }
}

impl Deref for AutoSavingSettings<'_> {
    type Target = Settings;

    fn deref(&self) -> &Settings {
        &self.inner
    }
}

impl DerefMut for AutoSavingSettings<'_> {
    fn deref_mut(&mut self) -> &mut Settings {
        &mut self.inner
    }
}

impl Drop for AutoSavingSettings<'_> {
    fn drop(&mut self) {
        if self.hash_before != self.inner.get_hash() {
            self.inner.save()
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Hash)]
pub struct OuterWindowPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct InnerWindowSize {
    pub width: u32,
    pub height: u32,
}

impl Default for InnerWindowSize {
    fn default() -> Self {
        Self {
            width: 275,
            height: 116,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Hash)]
pub struct WindowState {
    pub outer_position: Option<OuterWindowPosition>,
    pub inner_size: Option<InnerWindowSize>,
    /// Whether this on-demand window was open when last used, so it can be
    /// reopened on the next launch. `serde(default)` keeps older files loading.
    #[serde(default)]
    pub visible: bool,
}

impl WindowState {
    pub fn get_position(&self) -> Option<LogicalPosition<i32>> {
        self.outer_position
            .as_ref()
            .map(|pos| LogicalPosition::new(pos.x, pos.y))
    }

    pub fn set_position(&mut self, position: LogicalPosition<i32>) {
        self.outer_position = Some(OuterWindowPosition {
            x: position.x,
            y: position.y,
        });
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Hash)]
pub struct PlaylistSettings {
    pub window_state: WindowState,
    pub uris: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct PlayerSettings {
    pub window_state: WindowState,
    pub double_size_active: bool,
    pub volume: u16,
    pub show_playlist: bool,
    /// Name of the chosen audio output device, or None for the system default.
    /// `serde(default)` keeps older settings files (without this field) loading.
    #[serde(default)]
    pub audio_device: Option<String>,
    /// Classic Winamp windowshade: the player collapses to a single title bar.
    /// `serde(default)` keeps older settings files loading (defaults to off).
    #[serde(default)]
    pub windowshade_active: bool,
    /// Keep every Spotiamp+ window above other applications.
    #[serde(default)]
    pub always_on_top: bool,
    /// Player scale as a percentage (100 = 1x, 150, 200 = double size, 300).
    /// `Option`/`serde(default)` so old files load; `None` means "fall back to
    /// `double_size_active`". Stored as an int because `f32` isn't `Hash` (and
    /// this struct derives `Hash` for change-detection).
    #[serde(default)]
    pub player_zoom_pct: Option<u16>,
    /// Whether the EQ window was open, so it reopens on launch (and the docked
    /// playlist keeps its position). `serde(default)` keeps old files loading.
    #[serde(default)]
    pub show_eq: bool,
    /// Where playback was at the last exit, so the next launch can cue it.
    #[serde(default)]
    pub resume: Option<ResumePoint>,
    /// Scale for every window as a percentage (100, 150, 200, 300), None = 100.
    /// A new field on purpose: `player_zoom_pct` may still hold a value from
    /// the pulled 0.7.1 player-only zoom, which shouldn't suddenly apply.
    #[serde(default)]
    pub ui_scale_pct: Option<u16>,
    /// Even out loudness between tracks (librespot's normalisation, the same
    /// as Spotify's own "Normalize volume" setting). Off by default.
    #[serde(default)]
    pub normalize: bool,
    /// Taskbar extras: song title on the taskbar button, progress across it,
    /// and prev/play/next under its thumbnail (taskbar.rs). Opt-in.
    #[serde(default)]
    pub taskbar_extras: bool,
    /// On-screen display: the song, with its cover, for a few seconds in the
    /// screen's corner when it changes (osd.rs). Opt-in.
    #[serde(default)]
    pub osd: bool,
    /// The llama sitting on the player (mascot.rs). None = never switched,
    /// which counts as on.
    #[serde(default)]
    pub mascot: Option<bool>,
    /// The llama's height in px at 1x (48, 56 or 64); None = 56.
    #[serde(default)]
    pub mascot_size: Option<u16>,
    /// Where on the player's top edge it sits: px (at 1x) from the right end,
    /// set by dragging it; None = the default spot.
    #[serde(default)]
    pub mascot_inset: Option<u16>,
    /// The equalizer: it used to start flat on every launch.
    #[serde(default)]
    pub eq: EqSettings,
}

/// An EQ curve in whole dB (the sliders move in 1 dB steps), -12..+12.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Hash)]
pub struct EqCurve {
    pub preamp: i8,
    pub bands: [i8; 10],
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct EqSettings {
    pub enabled: bool,
    /// The curve set by hand: what plays when no auto-load preset applies.
    pub curve: EqCurve,
    /// AUTO (Winamp's): a song's own preset, else its artist's, loads when it
    /// starts.
    #[serde(default)]
    pub auto: bool,
    /// Auto-load presets, keyed "song:<uri or local path>" / "artist:<name>".
    /// A Vec, not a map, so the settings stay `Hash`.
    #[serde(default)]
    pub presets: Vec<(String, EqCurve)>,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self { enabled: true, curve: EqCurve::default(), auto: false, presets: Vec::new() }
    }
}

/// A Spotify track and position to pick up from on the next launch.
#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct ResumePoint {
    pub uri: String,
    /// 1-based playlist row, to find the right one when a track is listed twice.
    pub index: u32,
    pub position_ms: u32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            window_state: Default::default(),
            double_size_active: Default::default(),
            volume: 80,
            show_playlist: true,
            audio_device: None,
            windowshade_active: false,
            always_on_top: false,
            player_zoom_pct: None,
            show_eq: false,
            resume: None,
            ui_scale_pct: None,
            normalize: false,
            taskbar_extras: false,
            osd: false,
            mascot: None,
            mascot_size: None,
            mascot_inset: None,
            eq: EqSettings::default(),
        }
    }
}

/// A user-named list of track URIs, saved inside Spotiamp+ (not on Spotify).
#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct SavedList {
    pub name: String,
    pub uris: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Hash)]
pub struct Settings {
    pub player: PlayerSettings,
    pub playlist: PlaylistSettings,
    /// Active skin name ("" / "classic" = the default base-2.91 set).
    #[serde(default)]
    pub skin: String,
    /// App-local named lists (Winamp-style playlists kept in the app).
    #[serde(default)]
    pub saved_lists: Vec<SavedList>,
    /// Spotify playlists pinned to the top of the Library (uris, in order).
    #[serde(default)]
    pub pinned_playlists: Vec<String>,
    /// Songs loved in Spotiamp+ (♡, the F key): Spotify track uris, oldest
    /// first. Kept in the app; Spotify's own Liked Songs can't be written to.
    #[serde(default)]
    pub loved: Vec<String>,
    /// Skins starred (★) in the Skin Museum, oldest first, kept whole so the
    /// Favorites tab shows them without asking the museum again.
    #[serde(default)]
    pub favorite_skins: Vec<crate::museum::MuseumSkin>,
    /// Controller ("free") mode: instead of streaming through librespot —
    /// which a non-Premium account can't do — Spotiamp+ mirrors and drives
    /// the official Spotify app through the Windows media session. Persisted
    /// so a free account isn't sent through OAuth again on every launch.
    #[serde(default)]
    pub controller_mode: bool,
    /// Crash backstop: set just before a librespot connect and cleared a
    /// little after it survives. librespot kills the whole process (exit(1),
    /// no dialog) when it learns mid-session that an account can't stream,
    /// so if this flag is still set at the next startup, the last run died
    /// during connect — almost always a non-Premium account that slipped
    /// past the profile check — and the user gets offered Free Mode instead
    /// of an endless silent-crash loop.
    #[serde(default)]
    pub pending_connect: bool,
    /// How many runs in a row ended with `pending_connect` still set. One can
    /// be anything (closed or restarted within seconds of connecting); only
    /// two in a row mean the account gate, so only then is Free Mode offered.
    #[serde(default)]
    pub connect_deaths: u8,
    /// Remembered geometry for the on-demand windows (library / visualizer /
    /// lyrics / eq), keyed by window label — so reopening one brings it back
    /// where and how it was left. Player and playlist keep their own fields
    /// above; this covers the rest without a struct each. A Vec, not a HashMap,
    /// because Settings derives Hash (used to detect changes for auto-save) and
    /// HashMap isn't Hash; there are only a handful of windows.
    #[serde(default)]
    pub windows: Vec<(String, WindowState)>,
    /// The version whose "What's new" was last shown, so it opens once after
    /// each update (whatsnew_window.rs).
    #[serde(default)]
    pub last_seen_version: Option<String>,
}

impl Settings {
    fn window(&self, label: &str) -> Option<&WindowState> {
        self.windows.iter().find(|(l, _)| l == label).map(|(_, w)| w)
    }
    fn window_mut(&mut self, label: &str) -> &mut WindowState {
        if let Some(i) = self.windows.iter().position(|(l, _)| l == label) {
            &mut self.windows[i].1
        } else {
            self.windows.push((label.to_string(), WindowState::default()));
            &mut self.windows.last_mut().unwrap().1
        }
    }
    /// Restore a remembered position for `label`, or None if never saved.
    pub fn window_position(&self, label: &str) -> Option<LogicalPosition<i32>> {
        self.window(label).and_then(|w| w.get_position())
    }
    /// Restore a remembered inner size for `label`, or None if never saved.
    pub fn window_inner_size(&self, label: &str) -> Option<InnerWindowSize> {
        self.window(label).and_then(|w| w.inner_size.clone())
    }
    pub fn set_window_position(&mut self, label: &str, position: LogicalPosition<i32>) {
        self.window_mut(label).set_position(position);
    }
    pub fn set_window_inner_size(&mut self, label: &str, size: InnerWindowSize) {
        self.window_mut(label).inner_size = Some(size);
    }
    /// Remember whether an on-demand window is open, so launch can reopen it.
    pub fn set_window_visible(&mut self, label: &str, visible: bool) {
        self.window_mut(label).visible = visible;
    }
    /// Labels of the on-demand windows that were open when last used.
    pub fn open_windows(&self) -> Vec<String> {
        self.windows
            .iter()
            .filter(|(_, w)| w.visible)
            .map(|(label, _)| label.clone())
            .collect()
    }
    fn _current() -> &'static RwLock<Settings> {
        static MEM: OnceLock<RwLock<Settings>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(Settings::load()))
    }

    pub fn current_mut<'a>() -> AutoSavingSettings<'a> {
        AutoSavingSettings::new(Self::_current())
    }

    pub fn current<'a>() -> RwLockReadGuard<'a, Settings> {
        Self::_current().read().unwrap()
    }

    fn load() -> Settings {
        let settings_file_path = get_settings_file_path();
        log::info!("Loading settings from '{settings_file_path:?}'");
        let mut settings: Result<Settings, String> = File::open(settings_file_path.clone())
            .map_err(|e| format!("Could not open file ({e:?}"))
            .and_then(|f| {
                serde_yaml::from_reader(BufReader::new(f))
                    .map_err(|e| format!("Could not deserialize file ({e:?})"))
            });

        if let Err(e) = &mut settings {
            log::warn!("Could not load settings ({settings_file_path:?}): {e:}")
        }
        //TODO: Check if the error is something else than file not found and log
        //eprintln!("Failed to load config ({err}), falling back to default settings");
        settings.unwrap_or_else(|e| {
            log::info!("Could not load a settings file ({e:?}, creating a new one");
            // start with an empty playlist (was a hardcoded "One More Time" demo track)
            Settings::default()
        })
    }

    fn save(&self) {
        let settings_file_path = get_settings_file_path();
        if let Err(e) = File::create(settings_file_path.clone())
            .map_err(|e| format!("Could not create file ({e:?})"))
            .and_then(|file| {
                serde_yaml::to_writer(BufWriter::new(file), self)
                    .map_err(|e| format!("Could not serialize ({e:?})"))
            })
        {
            log::error!("Failed to save settings: {:?}", e);
        } else {
            log::debug!("Settings saved");
        }
    }

    fn get_hash(&self) -> u64 {
        let hasher = &mut DefaultHasher::new();
        self.hash(hasher);
        hasher.finish()
    }
}
