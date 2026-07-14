use std::{
    sync::{Arc, Mutex, atomic::AtomicU16},
    time::Duration,
};

use crate::{
    eq::EqState,
    oauth::{OAuthError, OAuthFlow},
    settings::Settings,
    sink::SpotiampSink,
    visualizer::Visualizer,
};
use futures_util::{StreamExt, stream};
use librespot::{
    core::{
        Error, SpotifyUri, authentication::Credentials, cache::Cache, config::SessionConfig,
        session::Session,
    },
    metadata::{Album, Metadata, Playlist, Track},
    playback::{
        config::{AudioFormat, Bitrate, NormalisationMethod, NormalisationType, PlayerConfig},
        dither::{TriangularDitherer, mk_ditherer},
        mixer::VolumeGetter,
        player::{Player, PlayerEventChannel, duration_to_coefficient},
    },
};
use oauth2::TokenResponse;
use tauri::AppHandle;
use thiserror::Error;

use crate::settings::get_config_dir;
pub type SharedPlayer = Arc<tokio::sync::Mutex<SpotifyPlayer>>;

/// A playlist from the current user's library (sent to the frontend).
#[derive(serde::Serialize, Clone)]
pub struct UserPlaylist {
    pub name: String,
    pub uri: String,
    pub track_count: u32,
    pub image: Option<String>,
}

use std::sync::OnceLock;
static PENDING_AUTH_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
pub fn pending_auth_url() -> &'static Mutex<Option<String>> {
    PENDING_AUTH_URL.get_or_init(|| Mutex::new(None))
}
pub struct SpotifySession {
    inner: Session,
    cache: Cache,
}

impl Default for SpotifySession {
    fn default() -> Self {
        let cache = get_config_dir()
            .and_then(|config_dir| {
                Cache::new(Some(config_dir.clone()), None, Some(config_dir), None).ok()
            })
            .expect("a cache to be created");
        let session = Session::new(SessionConfig::default(), Some(cache.clone()));
        Self {
            inner: session,
            cache,
        }
    }
}

impl SpotifySession {
    pub async fn login(&self, app: &AppHandle) -> Result<(), SessionError> {
        log::debug!("Getting credentials");
        let credentials = match self.cache.credentials() {
            Some(credentials) => credentials,
            None => {
                log::debug!("No credentials in cache, starting OAuth flow...");
                Self::get_credentials_from_oauth(app).await?
            }
        };

        match self.inner.connect(credentials, true).await {
            Ok(_) => {
                log::debug!("Successfully connected with cached credentials");
                Ok(())
            }
            Err(e) => {
                log::warn!(
                    "Failed to connect with cached credentials ({e:?}), re-authenticating..."
                );
                if let Some(config_dir) = get_config_dir() {
                    let _ = std::fs::remove_file(config_dir.join("credentials.json"));
                }
                let new_credentials = Self::get_credentials_from_oauth(app).await?;
                self.inner
                    .connect(new_credentials, true)
                    .await
                    .map_err(|e| SessionError::ConnectError { e })?;
                log::debug!("Successfully connected after re-authentication");
                Ok(())
            }
        }
    }

    async fn get_credentials_from_oauth(app: &AppHandle) -> Result<Credentials, SessionError> {
        let oauth_flow = OAuthFlow::new(
            "https://accounts.spotify.com/authorize",
            "https://accounts.spotify.com/api/token",
            "65b708073fc0480ea92a077233ca87bd",
        )
        .map_err(|e| SessionError::OauthError { e })?;

        let auth_url = oauth_flow.get_auth_url();
        log::debug!("Opening URL: {auth_url}");

        *pending_auth_url().lock().unwrap() = Some(auth_url.clone());

        let token_received = Arc::new(Mutex::new(false));
        let (abort_tx, abort_rx) = tokio::sync::watch::channel(false);

        let window = tauri::WebviewWindowBuilder::new(
            app,
            "login",
            tauri::WebviewUrl::App("login-proxy".into()),
        )
        .title("Login")
        .inner_size(600.0, 800.0)
        .closable(true)
        .maximizable(false)
        .resizable(false)
        .build()
        .map_err(|e| SessionError::OpenURLFailed {
            url: auth_url.clone(),
            e,
        })?;

        window.on_window_event({
            let token_received = token_received.clone();
            let abort_tx = abort_tx.clone();
            move |e| {
                if let tauri::WindowEvent::CloseRequested { .. } = &e
                    && !*token_received.lock().unwrap()
                {
                    log::info!("No token received when closing login window. Aborting.");
                    let _ = abort_tx.send(true);
                }
            }
        });

        let result = oauth_flow.start(abort_rx).await;
        *token_received.lock().unwrap() = true;
        let _ = window.close();
        let token = result.map_err(|e| SessionError::TokenExchangeFailure { e })?;

        Ok(Credentials::with_access_token(
            token.access_token().secret(),
        ))
    }
}

pub struct SpotifyPlayer {
    player: Arc<Player>,
    pub session: SpotifySession,
    volume: Arc<AtomicU16>,
    eq: Arc<Mutex<EqState>>,

    visualizer: Arc<Mutex<Visualizer>>,
}

impl SpotifyPlayer {
    #[allow(clippy::new_without_default)]
    pub fn new(session: SpotifySession) -> Self {
        let volume = Arc::new(AtomicU16::new(Settings::current().player.volume));
        let visualizer = Arc::new(Mutex::new(Visualizer::new()));
        let eq = Arc::new(Mutex::new(EqState::default()));
        let player =
            Self::build_player(&session.inner, volume.clone(), visualizer.clone(), eq.clone());

        Self {
            player,
            session,
            volume,
            eq,
            visualizer,
        }
    }

    /// Build a librespot Player bound to `session`. Split out of `new` so that
    /// `reconnect` can rebuild the player on a fresh session while keeping the
    /// same volume + visualizer.
    fn build_player(
        session: &Session,
        volume: Arc<AtomicU16>,
        visualizer: Arc<Mutex<Visualizer>>,
        eq: Arc<Mutex<EqState>>,
    ) -> Arc<Player> {
        let player_config = PlayerConfig {
            // Emit a position update every second so the UI can re-sync its
            // playback clock instead of free-running and drifting from the
            // actual position (which left the seek bar short at end of track).
            position_update_interval: Some(Duration::from_secs(1)),
            bitrate: Bitrate::Bitrate320,
            gapless: true,
            normalisation: false,
            normalisation_type: NormalisationType::default(),
            normalisation_method: NormalisationMethod::Dynamic,
            normalisation_pregain_db: 0.0,
            normalisation_threshold_dbfs: -2.0,
            normalisation_attack_cf: duration_to_coefficient(Duration::from_millis(5)),
            normalisation_release_cf: duration_to_coefficient(Duration::from_millis(100)),
            normalisation_knee_db: 5.0,
            local_file_directories: Vec::new(),
            passthrough: false,
            ditherer: Some(mk_ditherer::<TriangularDitherer>),
        };

        struct SpotiampVolumeGetter {
            volume: Arc<AtomicU16>,
        }

        impl VolumeGetter for SpotiampVolumeGetter {
            fn attenuation_factor(&self) -> f64 {
                self.volume.load(std::sync::atomic::Ordering::Relaxed) as f64 / 100.0
            }
        }

        Player::new(
            player_config,
            session.clone(),
            Box::new(SpotiampVolumeGetter {
                volume: volume.clone(),
            }),
            {
                let visualizer = visualizer.clone();
                let volume = volume.clone();
                let eq = eq.clone();
                move || {
                    let audio_format = AudioFormat::F32;
                    Box::new(SpotiampSink::new(
                        None,
                        audio_format,
                        visualizer,
                        volume,
                        eq,
                    ))
                }
            },
        )
    }

    /// Whether Spotify has dropped our session (so playback needs a reconnect).
    pub fn is_session_invalid(&self) -> bool {
        self.session.inner.is_invalid()
    }

    /// Rebuild the session + player after Spotify closed the connection
    /// (os error 10054 etc.). A librespot Session can't be reused once
    /// invalidated, so we spin up a fresh one (reusing the cached credentials,
    /// no re-login prompt) and swap in a new Player, keeping the same volume and
    /// visualizer. Returns the new event channel for the UI forwarder.
    pub async fn reconnect(
        &mut self,
        app: &AppHandle,
    ) -> Result<PlayerEventChannel, SessionError> {
        self.player.stop();
        let session = SpotifySession::default();
        session.login(app).await?;
        self.player = Self::build_player(
            &session.inner,
            self.volume.clone(),
            self.visualizer.clone(),
            self.eq.clone(),
        );
        self.session = session;
        Ok(self.player.get_player_event_channel())
    }

    /// Update the equaliser (applied live by the sink).
    pub fn set_eq(&self, enabled: bool, preamp_db: f32, bands_db: [f32; 10]) {
        let mut eq = self.eq.lock().unwrap();
        eq.enabled = enabled;
        eq.preamp_db = preamp_db;
        eq.bands_db = bands_db;
    }

    /// Set the stereo balance (-1.0 left .. 0.0 centre .. +1.0 right).
    pub fn set_balance(&self, balance: f32) {
        let mut eq = self.eq.lock().unwrap();
        eq.balance = balance;
    }

    pub async fn load_track(&self, uri: &str) -> Result<(), PlayError> {
        let uri = SpotifyUri::from_uri(uri).map_err(|e| PlayError::MetadataError { e })?;
        self.player.load(uri, true, 0);
        Ok(())
    }

    pub fn play(&mut self) {
        log::debug!("Play!");
        self.player.play();
    }

    pub async fn pause(&mut self) -> Result<(), PlayError> {
        log::debug!("Pause!");
        self.player.pause();
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), PlayError> {
        log::debug!("Stop!");
        self.player.stop();
        Ok(())
    }

    /// A cheap clone of the underlying librespot session handle. Metadata
    /// fetches (track names, playlists, search) run over this WITHOUT holding
    /// the player lock — otherwise a playlist load would starve play/pause/stop
    /// for its whole duration.
    pub fn session_handle(&self) -> Session {
        self.session.inner.clone()
    }

    pub fn set_volume(&mut self, volume: u16) {
        self.volume
            .store(volume, std::sync::atomic::Ordering::Relaxed);
        self.session.cache.save_volume(volume);
    }

    pub fn get_volume(&self) -> u16 {
        self.volume.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn seek(&self, position_ms: u32) {
        self.player.seek(position_ms);
    }

    pub fn take_latest_spectrum(&mut self) -> Vec<(f32, f32)> {
        self.visualizer.lock().unwrap().take_latest_spectrum()
    }

    pub fn get_player_event_channel(&self) -> PlayerEventChannel {
        self.player.get_player_event_channel()
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Failed to connect ({e:?}")]
    ConnectError { e: Error },

    #[error("OAuth error ({e:?}")]
    OauthError { e: OAuthError },

    #[error("Could not open URL {url} ({e:?})")]
    OpenURLFailed { url: String, e: tauri::Error },

    #[error("Could not get token ({e:?}")]
    TokenExchangeFailure { e: OAuthError },
}

#[derive(Debug, Error)]
pub enum PlayError {
    #[error("Failed to fetch metadata ({e:?})")]
    MetadataError { e: Error },
    #[error("Cannot get track for non track id ({_0:?})")]
    GettingTrackForNonTrackUri(SpotifyUri),
}

// ---------------------------------------------------------------------------
// Metadata fetches over a bare session handle. These are free functions (not
// SpotifyPlayer methods) on purpose: the tauri commands clone the session with
// a brief lock and run the network I/O here WITHOUT the player mutex, so
// play/pause/stop stay responsive while a playlist or search is loading.
// ---------------------------------------------------------------------------

pub async fn fetch_track(session: &Session, track_uri: SpotifyUri) -> Result<Track, PlayError> {
    match track_uri {
        SpotifyUri::Track { .. } => {
            log::debug!("Getting track data: {:?}", track_uri);
            //TODO: Check why we get `TrackMetadataError { e: Error { kind: Internal, error: ErrorMessage("channel closed") } }` here after leaving the mac in standby for a while.
            Track::get(session, &track_uri)
                .await
                .map_err(|e| PlayError::MetadataError { e })
        }
        _ => Err(PlayError::GettingTrackForNonTrackUri(track_uri)),
    }
}

pub async fn fetch_track_ids(
    session: &Session,
    playlist_uri: SpotifyUri,
) -> Result<Vec<SpotifyUri>, PlayError> {
    match playlist_uri {
        SpotifyUri::Playlist { .. } => Ok(Playlist::get(session, &playlist_uri)
            .await
            .map_err(|e| PlayError::MetadataError { e })?
            .contents
            .items
            .iter()
            .filter(|item| {
                let is_track = matches!(&item.id, SpotifyUri::Track { .. });

                is_track
            })
            .map(|item| &item.id)
            .cloned()
            .collect()),
        SpotifyUri::Album { .. } => Ok(Album::get(session, &playlist_uri)
            .await
            .map_err(|e| PlayError::MetadataError { e })?
            .tracks()
            .cloned()
            .collect()),
        _ => {
            log::warn!("Trying to get playlist tracks from an id that is not a playlist");
            Ok(vec![])
        }
    }
}

/// Fetch the current user's playlists via librespot's internal rootlist
/// (uses the same session auth that plays music — no Web API scope needed,
/// which the keymaster refuses to grant). We pull the raw rootlist, scan it
/// for playlist URIs, then resolve each playlist's name + length via the
/// metadata API (Playlist::get, the same call fetch_track_ids uses).
pub async fn fetch_user_playlists(session: &Session) -> Result<Vec<UserPlaylist>, String> {
    let bytes = session
        .spclient()
        .get_rootlist(0, Some(500))
        .await
        .map_err(|e| format!("Failed to fetch rootlist: {e:?}"))?;

    // rootlist is protobuf; the playlist URIs appear as literal strings.
    let text = String::from_utf8_lossy(bytes.as_ref());
    let pat = "spotify:playlist:";
    let mut uris: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(pat) {
        let id_start = search_from + rel + pat.len();
        let id: String = text[id_start..].chars().take(22).collect();
        search_from = id_start;
        if id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
            let uri = format!("{pat}{id}");
            if seen.insert(uri.clone()) {
                uris.push(uri);
            }
        }
    }

    // resolve names + lengths concurrently (bounded), keeping rootlist
    // order. buffered(N) runs up to N Playlist::get calls at once, which
    // turns a slow one-by-one wait into a few fast batches.
    const CONCURRENCY: usize = 16;
    let playlists: Vec<UserPlaylist> = stream::iter(uris)
        .map(|uri| async move {
            let spuri = SpotifyUri::from_uri(&uri).ok()?;
            match Playlist::get(session, &spuri).await {
                Ok(pl) => Some(UserPlaylist {
                    name: pl.attributes.name.clone(),
                    uri,
                    track_count: pl.length.max(0) as u32,
                    image: None,
                }),
                Err(e) => {
                    log::debug!("Skipping playlist {uri}: {e:?}");
                    None
                }
            }
        })
        .buffered(CONCURRENCY)
        .filter_map(|maybe| async move { maybe })
        .collect()
        .await;
    Ok(playlists)
}

/// Search the Spotify catalogue. Uses the internal context-resolve endpoint
/// (the same one the desktop client uses for `spotify:search:<query>`),
/// which returns JSON we scan for track URIs — no extra protobuf deps.
/// Returns track URIs; the frontend resolves names via get_track_metadata.
pub async fn fetch_search(session: &Session, query: &str) -> Result<Vec<String>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    // spotify:search:<query> expects '+' between words
    let encoded = query.split_whitespace().collect::<Vec<_>>().join("+");
    let endpoint = format!("/context-resolve/v1/spotify:search:{encoded}");

    let bytes = session
        .spclient()
        .request_as_json(&http::Method::GET, &endpoint, None, None)
        .await
        .map_err(|e| format!("Search failed: {e:?}"))?;

    let text = String::from_utf8_lossy(bytes.as_ref());
    let pat = "spotify:track:";
    let mut uris: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(pat) {
        let id_start = search_from + rel + pat.len();
        let id: String = text[id_start..].chars().take(22).collect();
        search_from = id_start;
        if id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
            let uri = format!("{pat}{id}");
            if seen.insert(uri.clone()) {
                uris.push(uri);
            }
        }
        if uris.len() >= 50 {
            break;
        }
    }
    Ok(uris)
}
