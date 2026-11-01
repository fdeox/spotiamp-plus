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
        Error, SpotifyId, SpotifyUri, authentication::Credentials, cache::Cache,
        config::SessionConfig, session::Session,
    },
    metadata::{Album, Lyrics, Metadata, Playlist, Track, lyrics::SyncType},
    playback::{
        config::{AudioFormat, Bitrate, NormalisationMethod, NormalisationType, PlayerConfig},
        dither::{TriangularDitherer, mk_ditherer},
        mixer::VolumeGetter,
        player::{Player, PlayerEventChannel, duration_to_coefficient},
    },
    protocol::{
        extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery},
        extension_kind::ExtensionKind,
        metadata::Track as TrackMessage,
    },
};
use oauth2::TokenResponse;
use protobuf::{EnumOrUnknown, Message};
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
    /// The folders it's in, outermost first, as in Spotify's own sidebar
    /// (empty: at the top).
    pub folder: Vec<String>,
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

        // librespot's own account gate kills the process shortly AFTER connect
        // returns (exit(1) from the packet handler, no dialog we could show).
        // Flag the attempt so the next startup can tell "died during connect"
        // apart from a clean run, and only clear it once the session has
        // demonstrably survived past that window. An ordinary error return
        // clears it immediately — the process being alive to return one is
        // itself the proof this wasn't the silent death.
        crate::settings::Settings::current_mut().pending_connect = true;
        let connected: Result<(), SessionError> = async {
            match self.inner.connect(credentials, true).await {
                Ok(_) => {
                    log::debug!("Successfully connected with cached credentials");
                    Ok(())
                }
                // No internet, Spotify down, a DNS hiccup: the saved login is
                // still good. It used to be thrown away here and a browser
                // sign-in started, so a moment offline signed you out and the
                // reconnect sat waiting for a login nobody asked for. Keep it;
                // the caller tries again.
                Err(e) if !is_login_refused(&e) => {
                    log::warn!("Couldn't reach Spotify ({e:?}); keeping the saved login");
                    Err(SessionError::ConnectError { e })
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
        .await;
        match &connected {
            Ok(()) => {
                crate::settings::Settings::current_mut().signed_out = false;
                let session = self.inner.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    {
                        let mut settings = crate::settings::Settings::current_mut();
                        settings.pending_connect = false;
                        settings.connect_deaths = 0;
                    }
                    log_account(&session);
                });
            }
            Err(_) => crate::settings::Settings::current_mut().pending_connect = false,
        }
        connected
    }

    /// Ask Spotify what kind of account this token belongs to.
    ///
    /// librespot's own catalogue check calls `exit(1)` the moment it learns the
    /// account isn't Premium — no error, no window, nothing the user can see. We
    /// therefore look it up ourselves *before* opening a session, so a free
    /// account gets a real explanation instead of a silent disappearance.
    ///
    /// A failure here is deliberately not fatal: if the profile call is blocked
    /// or the field is missing we carry on rather than lock anyone out.
    async fn check_account_is_premium(access_token: &str) -> Result<(), SessionError> {
        #[derive(serde::Deserialize)]
        struct Profile {
            product: Option<String>,
        }

        let client = match oauth2::reqwest::Client::builder().build() {
            Ok(client) => client,
            Err(_) => return Ok(()),
        };
        let response = client
            .get("https://api.spotify.com/v1/me")
            .bearer_auth(access_token)
            .send()
            .await;

        let product = match response {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(body) => serde_json::from_str::<Profile>(&body)
                    .ok()
                    .and_then(|profile| profile.product),
                Err(_) => None,
            },
            _ => None,
        };

        match product.as_deref() {
            // Only the values Spotify documents for a free account trigger the
            // fallback. Anything else — "premium", family/duo variants, or an
            // unknown future string — stays on the Premium path: mistakenly
            // sending a paying user into controller mode would leave them
            // stuck there, since retrying the sign-in reads the same field.
            Some(product @ ("free" | "open")) => Err(SessionError::NotPremium {
                product: product.to_string(),
            }),
            _ => Ok(()),
        }
    }

    async fn get_credentials_from_oauth(app: &AppHandle) -> Result<Credentials, SessionError> {
        let token = Self::sign_in(app).await?;

        // Checked here, before any session exists — librespot would otherwise
        // exit the process itself the instant it sees a non-Premium account.
        Self::check_account_is_premium(token.access_token().secret()).await?;

        Ok(Credentials::with_access_token(
            token.access_token().secret(),
        ))
    }

    /// Spotify's own sign-in page in a window of ours, and the token it
    /// gives: for Premium playback, and for Free Mode's queue (webapi.rs).
    pub(crate) async fn sign_in(app: &AppHandle) -> Result<crate::oauth::TokenType, SessionError> {
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
        // Logged out: Spotify's page would sign the same account straight
        // back in from its cookies, so they go first.
        #[cfg(target_os = "windows")]
        if crate::settings::Settings::current().signed_out {
            crate::app_window::delete_cookies(&window);
        }

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
        result.map_err(|e| SessionError::TokenExchangeFailure { e })
    }
}

pub struct SpotifyPlayer {
    player: Arc<Player>,
    pub session: SpotifySession,
    volume: Arc<AtomicU16>,
    eq: Arc<Mutex<EqState>>,

    visualizer: Arc<Mutex<Visualizer>>,
    /// Chosen audio output device name (None = system default). Read by the sink
    /// builder each time librespot opens a new sink, so a change takes effect on
    /// the next track (the frontend reloads the current track to apply it now).
    audio_device: Arc<Mutex<Option<String>>>,
}

impl SpotifyPlayer {
    #[allow(clippy::new_without_default)]
    pub fn new(session: SpotifySession) -> Self {
        let volume = Arc::new(AtomicU16::new(Settings::current().player.volume));
        let visualizer = Arc::new(Mutex::new(Visualizer::new()));
        let eq = crate::eq::shared();
        let audio_device = Arc::new(Mutex::new(Settings::current().player.audio_device.clone()));
        let player = Self::build_player(
            &session.inner,
            volume.clone(),
            visualizer.clone(),
            eq.clone(),
            audio_device.clone(),
        );

        Self {
            player,
            session,
            volume,
            eq,
            visualizer,
            audio_device,
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
        audio_device: Arc<Mutex<Option<String>>>,
    ) -> Arc<Player> {
        let player_config = PlayerConfig {
            // Emit a position update every second so the UI can re-sync its
            // playback clock instead of free-running and drifting from the
            // actual position (which left the seek bar short at end of track).
            position_update_interval: Some(Duration::from_secs(1)),
            bitrate: match Settings::current().player.quality_kbps() {
                96 => Bitrate::Bitrate96,
                160 => Bitrate::Bitrate160,
                _ => Bitrate::Bitrate320,
            },
            gapless: true,
            normalisation: Settings::current().player.normalize,
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
                // The volume curve, defined next to the visualizer tap that
                // inverts it, so the two never drift apart.
                crate::sink::volume_amplitude(
                    self.volume.load(std::sync::atomic::Ordering::Relaxed),
                )
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
                let audio_device = audio_device.clone();
                move || {
                    // A saved device may have been unplugged since it was chosen;
                    // librespot's rodio backend `.unwrap()`s on a missing named
                    // device, which would panic the player. Only pass the name if
                    // it's actually present right now — otherwise fall back to the
                    // system default (None).
                    let device = audio_device
                        .lock()
                        .unwrap()
                        .clone()
                        .filter(|name| list_output_devices().iter().any(|d| d == name));
                    Box::new(SpotiampSink::new(
                        device,
                        AudioFormat::F32,
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
        // A session Spotify stopped answering is still "valid"; close it rather
        // than leave its connection open beside the new one.
        if !self.session.inner.is_invalid() {
            self.session.inner.shutdown();
        }
        let session = SpotifySession::default();
        session.login(app).await?;
        self.replace_player(Self::build_player(
            &session.inner,
            self.volume.clone(),
            self.visualizer.clone(),
            self.eq.clone(),
            self.audio_device.clone(),
        ));
        self.session = session;
        Ok(self.player.get_player_event_channel())
    }

    /// Set the stereo balance (-1.0 left .. 0.0 centre .. +1.0 right).
    pub fn set_balance(&self, balance: f32) {
        let mut eq = self.eq.lock().unwrap();
        eq.balance = balance;
    }

    pub async fn load_track(&self, uri: &str, position_ms: u32, play: bool) -> Result<(), PlayError> {
        let uri = SpotifyUri::from_uri(uri).map_err(|e| PlayError::MetadataError { e })?;
        self.player.load(uri, play, position_ms);
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

    /// Switch the audio output device (None = system default). The librespot sink
    /// builder runs only once, at player creation, so we rebuild the player to
    /// open a fresh sink on the chosen device and return the new event channel
    /// for the UI forwarder to re-subscribe. The frontend reloads the current
    /// track afterwards so the change is heard immediately.
    pub fn set_audio_device(&mut self, device: Option<String>) -> PlayerEventChannel {
        *self.audio_device.lock().unwrap() = device.clone();
        Settings::current_mut().player.audio_device = device;
        self.rebuild_player()
    }

    /// Reopen playback on the system's new default output, asked for by
    /// name. Asked for "the default" while Windows is still switching (a
    /// Bluetooth headset connecting), the sink could get the old device and
    /// stay there unnoticed. The saved choice stays "system default".
    pub fn follow_default_output(&mut self, name: Option<String>) -> PlayerEventChannel {
        *self.audio_device.lock().unwrap() = name;
        self.rebuild_player()
    }

    /// Turn loudness normalisation on or off. librespot reads it from the
    /// PlayerConfig, fixed when the player is built, so this rebuilds the player
    /// the same way a device switch does (the frontend then reloads the track).
    pub fn set_normalisation(&mut self, enabled: bool) -> PlayerEventChannel {
        Settings::current_mut().player.normalize = enabled;
        self.rebuild_player()
    }

    /// Set the streaming quality (96, 160 or 320 kbps). Read when the player
    /// is built, so it's rebuilt like for normalize.
    pub fn set_quality(&mut self, kbps: u16) -> PlayerEventChannel {
        Settings::current_mut().player.quality = Some(kbps);
        self.rebuild_player()
    }

    fn rebuild_player(&mut self) -> PlayerEventChannel {
        crate::note_player_rebuilt();
        self.player.stop();
        self.replace_player(Self::build_player(
            &self.session.inner,
            self.volume.clone(),
            self.visualizer.clone(),
            self.eq.clone(),
            self.audio_device.clone(),
        ));
        self.player.get_player_event_channel()
    }

    /// Put a new player in place of the old one, and let the old one go on a
    /// thread of its own. Dropping a librespot player waits for its thread,
    /// and that thread can be stuck for good writing to an output that's gone
    /// (its rodio sink waits for a queue a vanished device never drains):
    /// waited for here, under the player lock, every command after it hung
    /// and the app showed "playing" with no sound (Bluetooth headphones
    /// dropping out, issue #9).
    ///
    /// It's given a moment to go, though. Each librespot player has a tokio
    /// runtime of its own, and the connections the session's HTTP client
    /// opened from it die with it but stay in the shared pool: a song loaded
    /// on the new player before the old one was gone drew a dead one and sat
    /// silent for 8 s (or was skipped). Gone, its connections are seen closed
    /// and left alone. A player stuck on a vanished device isn't waited for.
    fn replace_player(&mut self, player: Arc<Player>) {
        let old = std::mem::replace(&mut self.player, player);
        let (gone_tx, gone_rx) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new().name("old-player".into()).spawn(move || {
            drop(old);
            let _ = gone_tx.send(());
        });
        if spawned.is_ok() && gone_rx.recv_timeout(Duration::from_millis(1500)).is_err() {
            log::warn!("The old player is slow to go; carrying on without it");
        }
    }

    pub fn seek(&self, position_ms: u32) {
        self.player.seek(position_ms);
    }

    pub fn take_latest_spectrum(&mut self) -> Vec<(f32, f32)> {
        self.visualizer.lock().unwrap().take_latest_spectrum()
    }

    /// The latest waveform (see `Visualizer::waveform_bytes`).
    pub fn waveform_bytes(&self) -> Vec<u8> {
        self.visualizer.lock().unwrap().waveform_bytes()
    }

    pub fn get_player_event_channel(&self) -> PlayerEventChannel {
        self.player.get_player_event_channel()
    }

    /// Share the live EQ state and the visualizer so the local-file player runs
    /// through the exact same DSP and spectrum — a local track then gets the EQ
    /// and the visualizer just like a Spotify one.
    pub fn eq_shared(&self) -> Arc<Mutex<EqState>> {
        self.eq.clone()
    }
    pub fn visualizer_shared(&self) -> Arc<Mutex<Visualizer>> {
        self.visualizer.clone()
    }
}

/// Names of the available audio output devices, for the device picker. Uses the
/// same cpal default host as the rodio backend, so the names round-trip.
pub fn list_output_devices() -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    match cpal::default_host().output_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// What kind of Premium the account has, for diagnostic info: songs that
/// won't load can be the plan (some don't stream 320 kbps). Only the plan's
/// own fields: nothing about the person, not even the country (which
/// "financial-product" names, so it stays out).
fn log_account(session: &Session) {
    const KEYS: [&str; 5] = ["type", "name", "catalogue", "high-bitrate", "streaming"];
    let attributes = session.user_data().attributes;
    let known: Vec<String> = KEYS
        .iter()
        .filter_map(|k| attributes.get(*k).filter(|v| !v.is_empty()).map(|v| format!("{k}={v}")))
        .collect();
    log::info!("Spotify account: {} ({} attributes)", known.join(", "), attributes.len());
}

/// Spotify turned the saved login down (expired or revoked), as opposed to
/// not being reachable at all.
fn is_login_refused(e: &Error) -> bool {
    use librespot::core::error::ErrorKind;
    matches!(e.kind, ErrorKind::PermissionDenied | ErrorKind::Unauthenticated | ErrorKind::InvalidArgument)
}

impl SessionError {
    /// Couldn't reach Spotify (offline, DNS, Spotify down): worth another try.
    pub fn is_unreachable(&self) -> bool {
        matches!(self, SessionError::ConnectError { e } if !is_login_refused(e))
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

    /// The account can't stream. librespot reacts to this by calling exit()
    /// itself, so we have to catch it first to say anything useful.
    #[error("This Spotify account is {product}, and playback needs Premium")]
    NotPremium { product: String },
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

/// One line of lyrics sent to the frontend (start time + text).
#[derive(serde::Serialize, Clone)]
pub struct LyricsLine {
    pub time_ms: u32,
    pub text: String,
}

/// Lyrics for a track. `synced` = the provider gave per-line timestamps, so the
/// frontend can highlight/scroll in time with playback.
#[derive(serde::Serialize, Clone)]
pub struct LyricsData {
    pub synced: bool,
    pub provider: String,
    pub lines: Vec<LyricsLine>,
    /// How long the song the words were timed to is (LRCLIB says): a radio
    /// song's words stop there, should the station not name the next song.
    pub duration_ms: Option<u32>,
}

pub async fn fetch_lyrics(session: &Session, uri: &str) -> Result<LyricsData, String> {
    let sp_uri = SpotifyUri::from_uri(uri).map_err(|e| format!("Bad track uri ({e:?})"))?;
    let id = SpotifyId::try_from(&sp_uri).map_err(|e| format!("Bad track id ({e:?})"))?;
    let lyrics = Lyrics::get(session, &id)
        .await
        .map_err(|e| format!("No lyrics ({e:?})"))?;
    let inner = lyrics.lyrics;
    Ok(LyricsData {
        synced: matches!(inner.sync_type, SyncType::LineSynced),
        provider: inner.provider_display_name,
        lines: inner
            .lines
            .iter()
            .map(|line| LyricsLine {
                time_ms: line.start_time_ms.parse().unwrap_or(0),
                text: line.words.clone(),
            })
            .collect(),
        duration_ms: None,
    })
}

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

/// Track info for many tracks in one request: the same extended-metadata call
/// `Track::get` makes, asked for a whole batch of tracks at once. librespot
/// allows 300 requests per 30 s to spotify.com, shared with playback itself,
/// so filling in a long playlist one request per song used it all up and
/// songs then failed to load or play. Results are in `uris` order; `None`
/// where Spotify sent nothing usable for that track.
pub async fn fetch_tracks(
    session: &Session,
    uris: &[SpotifyUri],
) -> Result<Vec<Option<Track>>, Error> {
    let entity_request = uris
        .iter()
        .map(|uri| {
            Ok(EntityRequest {
                entity_uri: uri.to_uri()?,
                query: vec![ExtensionQuery {
                    extension_kind: EnumOrUnknown::new(ExtensionKind::TRACK_V4),
                    ..Default::default()
                }],
                ..Default::default()
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let response = session
        .spclient()
        .get_extended_metadata(BatchedEntityRequest {
            entity_request,
            ..Default::default()
        })
        .await?;

    let mut found = std::collections::HashMap::new();
    for data in response
        .extended_metadata
        .iter()
        .flat_map(|array| array.extension_data.iter())
    {
        let Some(any) = data.extension_data.as_ref() else {
            continue;
        };
        match TrackMessage::parse_from_bytes(&any.value)
            .map_err(Error::from)
            .and_then(|message| Track::try_from(&message))
        {
            Ok(track) => {
                found.insert(data.entity_uri.clone(), track);
            }
            Err(e) => log::debug!("Unreadable track info in a batch ({:?})", e.kind),
        }
    }
    Ok(uris
        .iter()
        .map(|uri| uri.to_uri().ok().and_then(|uri| found.remove(&uri)))
        .collect())
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

/// A track from a playlist/album together with when it was added to the list
/// (the playlist item's timestamp, ms since epoch). Albums and other contexts
/// have no per-track added date, so it's optional.
#[derive(serde::Serialize)]
pub struct TrackRef {
    pub uri: String,
    pub added_ms: Option<i64>,
}

/// Like `fetch_track_ids` but also carries each playlist item's "date added"
/// (`PlaylistItem.attributes.timestamp`). Used by the library's Date column.
pub async fn fetch_track_refs(
    session: &Session,
    playlist_uri: SpotifyUri,
) -> Result<Vec<TrackRef>, PlayError> {
    // Spotify predates ~2008; anything at/near the epoch is a missing date.
    const MIN_VALID_MS: i64 = 946_684_800_000; // 2000-01-01
    match playlist_uri {
        SpotifyUri::Playlist { .. } => Ok(Playlist::get(session, &playlist_uri)
            .await
            .map_err(|e| PlayError::MetadataError { e })?
            .contents
            .items
            .iter()
            .filter(|item| matches!(&item.id, SpotifyUri::Track { .. }))
            .map(|item| {
                let ms = item.attributes.timestamp.as_timestamp_ms();
                TrackRef {
                    uri: item.id.to_uri().expect("a valid uri"),
                    added_ms: (ms >= MIN_VALID_MS).then_some(ms),
                }
            })
            .collect()),
        SpotifyUri::Album { .. } => Ok(Album::get(session, &playlist_uri)
            .await
            .map_err(|e| PlayError::MetadataError { e })?
            .tracks()
            .map(|uri| TrackRef {
                uri: uri.to_uri().expect("a valid uri"),
                added_ms: None,
            })
            .collect()),
        _ => Ok(vec![]),
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
        .map_err(|e| {
            // Logged (the kind only: the full error can carry the request URL,
            // which names the user) so diagnostic info shows why the Library
            // was empty.
            log::warn!("Failed to fetch rootlist ({:?})", e.kind);
            format!("Failed to fetch rootlist: {e:?}")
        })?;

    // The rootlist lists playlists and the folders around them in order;
    // read as text if it ever won't parse (no folders then).
    let entries = rootlist_entries(bytes.as_ref()).unwrap_or_else(|| {
        log::warn!("Couldn't read the rootlist's folders; listing its playlists without them");
        rootlist_uris(bytes.as_ref()).into_iter().map(|uri| (uri, Vec::new())).collect()
    });

    // resolve names + lengths concurrently (bounded), keeping rootlist
    // order. buffered(N) runs up to N Playlist::get calls at once, which
    // turns a slow one-by-one wait into a few fast batches.
    const CONCURRENCY: usize = 16;
    let playlists: Vec<UserPlaylist> = stream::iter(entries)
        .map(|(uri, folder)| async move {
            let spuri = SpotifyUri::from_uri(&uri).ok()?;
            match Playlist::get(session, &spuri).await {
                Ok(pl) => Some(UserPlaylist {
                    name: pl.attributes.name.clone(),
                    uri,
                    track_count: pl.length.max(0) as u32,
                    image: None,
                    folder,
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

/// The rootlist's playlists in order, each with the folders it's in
/// (outermost first). A folder is a `spotify:start-group:<id>:<name>` item,
/// its name form-encoded, up to its `spotify:end-group:<id>`. None if the
/// bytes aren't the protobuf they should be.
fn rootlist_entries(bytes: &[u8]) -> Option<Vec<(String, Vec<String>)>> {
    use librespot::protocol::playlist4_external::SelectedListContent;
    let list = SelectedListContent::parse_from_bytes(bytes).ok()?;
    let contents = list.contents.as_ref()?;
    let mut folder: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in &contents.items {
        let uri = item.uri();
        if let Some(rest) = uri.strip_prefix("spotify:start-group:") {
            let name = rest.split_once(':').map_or(rest, |(_, name)| name);
            let name = url::form_urlencoded::parse(name.as_bytes()).next().map(|(n, _)| n.into_owned());
            folder.push(name.unwrap_or_default());
        } else if uri.starts_with("spotify:end-group:") {
            folder.pop();
        } else if uri.starts_with("spotify:playlist:") && seen.insert(uri.to_string()) {
            out.push((uri.to_string(), folder.clone()));
        }
    }
    Some(out)
}

/// The rootlist's playlist URIs, found in its bytes as text (the way it was
/// read before folders).
fn rootlist_uris(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
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
    uris
}

/// The query part of a `spotify:search:` URI: words joined by '+', each one
/// percent-encoded. Put in raw, "serdar ortaç" made an invalid request URL
/// and the search failed, while "ortac" worked.
fn search_uri_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            word.bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("+")
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
    let endpoint = format!("/context-resolve/v1/spotify:search:{}", search_uri_query(query));

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

/// Fetch the user's "Liked Songs" (the saved-tracks collection) as track URIs,
/// via the same internal context-resolve endpoint used for search. Newest first.
pub async fn fetch_liked_songs(session: &Session) -> Result<Vec<String>, String> {
    let bytes = session
        .spclient()
        .request_as_json(
            &http::Method::GET,
            "/context-resolve/v1/spotify:collection:tracks",
            None,
            None,
        )
        .await
        .map_err(|e| format!("Failed to fetch liked songs: {e:?}"))?;

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
    }
    Ok(uris)
}

/// Fetch a radio/recommendation list seeded from a track — used for autoplay when
/// the queue runs out (repeat off). The radio endpoint returns a generated
/// playlist uri, which we expand into concrete track uris (minus the seed).
pub async fn fetch_radio(session: &Session, seed_uri: &str) -> Result<Vec<String>, String> {
    let seed = SpotifyUri::from_uri(seed_uri).map_err(|e| format!("Bad seed uri: {e:?}"))?;
    let bytes = session
        .spclient()
        .get_radio_for_track(&seed)
        .await
        .map_err(|e| format!("Failed to fetch radio: {e:?}"))?;

    let text = String::from_utf8_lossy(bytes.as_ref());
    let pat = "spotify:playlist:";
    let playlist_uri = text
        .find(pat)
        .and_then(|rel| {
            let id_start = rel + pat.len();
            let id: String = text[id_start..].chars().take(22).collect();
            (id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()))
                .then(|| format!("{pat}{id}"))
        })
        .ok_or_else(|| "No radio playlist returned".to_string())?;

    let playlist_spuri =
        SpotifyUri::from_uri(&playlist_uri).map_err(|e| format!("Bad radio playlist uri: {e:?}"))?;
    let track_uris = fetch_track_ids(session, playlist_spuri)
        .await
        .map_err(|e| format!("Failed to expand radio: {e:?}"))?;

    Ok(track_uris
        .into_iter()
        .filter_map(|u| u.to_uri().ok())
        .filter(|u| u != seed_uri)
        .collect())
}

#[cfg(test)]
mod search_tests {
    #[test]
    fn non_ascii_and_symbols_are_encoded() {
        assert_eq!(super::search_uri_query("serdar  ortaç"), "serdar+orta%C3%A7");
        assert_eq!(super::search_uri_query("AC/DC #1?"), "AC%2FDC+%231%3F");
        assert_eq!(super::search_uri_query("  "), "");
    }
}

#[cfg(test)]
mod rootlist_tests {
    use librespot::protocol::playlist4_external::{Item, ListItems, SelectedListContent};
    use protobuf::Message;

    fn rootlist(uris: &[&str]) -> Vec<u8> {
        let mut contents = ListItems::new();
        contents.set_pos(0);
        contents.set_truncated(false);
        for uri in uris {
            let mut item = Item::new();
            item.set_uri(uri.to_string());
            contents.items.push(item);
        }
        let mut list = SelectedListContent::new();
        list.contents = protobuf::MessageField::some(contents);
        list.write_to_bytes().unwrap()
    }

    const A: &str = "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M";
    const B: &str = "spotify:playlist:37i9dQZF1DX0XUsuxWHRQd";
    const C: &str = "spotify:playlist:37i9dQZF1DWXRqgorJj26U";
    const D: &str = "spotify:playlist:37i9dQZF1DX4sWSpwq3LiO";

    #[test]
    fn playlists_come_with_their_folders() {
        let bytes = rootlist(&[
            A,
            "spotify:start-group:8212237ac7347bfe:Rock+%26+Roll",
            B,
            "spotify:start-group:1a2b3c4d5e6f7a8b:90s",
            C,
            "spotify:end-group:1a2b3c4d5e6f7a8b",
            "spotify:end-group:8212237ac7347bfe",
            D,
            A,
        ]);
        let entries = super::rootlist_entries(&bytes).expect("parses");
        let rock = || "Rock & Roll".to_string();
        assert_eq!(
            entries,
            vec![
                (A.to_string(), vec![]),
                (B.to_string(), vec![rock()]),
                (C.to_string(), vec![rock(), "90s".to_string()]),
                (D.to_string(), vec![]),
            ]
        );
        // read as text, the same playlists without folders
        assert_eq!(super::rootlist_uris(&bytes), vec![A, B, C, D]);
    }

    #[test]
    fn garbage_isnt_a_rootlist() {
        assert!(super::rootlist_entries(b"\xff\xff\xff not protobuf").is_none());
    }
}
