//! Spotify Soloist: Spotify's own headless player, as a second way to play
//! Spotify songs when Spotify refuses librespot the song keys of an account
//! (librespot #1649). Spotify doesn't let it be shipped with apps, so it's
//! downloaded from Spotify's own address when the user sets it up, run with
//! their own Soloist API key, and paired once from a Spotify app. It then runs
//! in the background with its local WebSocket API on, and while Soloist mode
//! is on the player's Spotify commands (load, play, pause, seek, volume) go to
//! it instead of librespot, and what it says comes back as the same player
//! events librespot's did. Its sound doesn't pass through Spotiamp+: the
//! visualizer listens to the system's output (loopback.rs), the EQ can't apply.
//!
//! Files: <config>/soloist/bin (soloist.exe as Spotify ships it), data (its
//! stored session and runtime files), cache, key (the API key, never logged).
//! Reference: developer.spotify.com/documentation/soloist

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use oauth2::reqwest;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{Settings, get_config_dir};

/// Where Spotify publishes the Windows build (no redistribution: users get it from here).
const DOWNLOAD_URL: &str = "https://soloist-builds.spotifycdn.com/soloist_release_windows_x86_64.zip";
/// Where a user makes their own key.
pub const KEY_PAGE: &str = "https://developer.spotify.com/dashboard/soloist";
/// A build stops working this long after it was built.
const BUILD_LIFETIME_DAYS: i64 = 90;
/// A new one is fetched when this little is left.
const RENEW_DAYS: i64 = 14;

fn root() -> Option<PathBuf> {
    get_config_dir().map(|d| d.join("soloist"))
}
fn bin_dir() -> Option<PathBuf> {
    root().map(|r| r.join("bin"))
}
fn exe_path() -> Option<PathBuf> {
    bin_dir().map(|b| b.join("soloist.exe"))
}
fn data_dir() -> Option<PathBuf> {
    root().map(|r| r.join("data"))
}
fn cache_dir() -> Option<PathBuf> {
    root().map(|r| r.join("cache"))
}
fn key_path() -> Option<PathBuf> {
    root().map(|r| r.join("key"))
}
/// Written once a pairing finished (Soloist keeps the session itself).
fn paired_marker() -> Option<PathBuf> {
    root().map(|r| r.join("paired"))
}
/// When the build in bin was downloaded (ms), for its expiry when --version can't be read.
fn installed_marker() -> Option<PathBuf> {
    root().map(|r| r.join("installed"))
}

fn read_key() -> Option<String> {
    let k = std::fs::read_to_string(key_path()?).ok()?;
    let k = k.trim().to_string();
    (!k.is_empty()).then_some(k)
}

/// The name it shows in Spotify's device lists.
fn device_name() -> String {
    match std::env::var("COMPUTERNAME") {
        Ok(pc) if !pc.trim().is_empty() => format!("Spotiamp+ ({})", pc.trim()),
        _ => "Spotiamp+".to_string(),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A command line run without a console window popping up.
fn command(exe: &Path) -> std::process::Command {
    let mut c = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c.stdin(std::process::Stdio::null());
    c
}

// ------------------------------------------------------------------ status

#[derive(Serialize, Clone, Default)]
pub struct SoloistStatus {
    /// soloist.exe is there
    installed: bool,
    /// what `soloist --version` said
    version: String,
    /// days until the build stops working (None: unknown)
    days_left: Option<i64>,
    has_key: bool,
    paired: bool,
    /// the background player is running and its API answers
    running: bool,
    /// Soloist mode: Spotify songs play through it
    on: bool,
    device_name: String,
    /// where the user makes their key
    key_page: &'static str,
}

/// The build date in `soloist --version`'s words (the first YYYY-MM-DD, or a
/// YYYYMMDD run of digits), as days since 1970.
fn build_day(version: &str) -> Option<i64> {
    let b = version.as_bytes();
    let num = |s: &[u8]| -> Option<i64> { s.iter().all(u8::is_ascii_digit).then(|| s.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0'))) };
    let day = |y: i64, m: i64, d: i64| ((2020..2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d));
    let digit_at = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    for i in 0..b.len() {
        // YYYY-MM-DD
        if let Some(w) = b.get(i..i + 10)
            && w[4] == b'-'
            && w[7] == b'-'
            && let (Some(y), Some(m), Some(d)) = (num(&w[..4]), num(&w[5..7]), num(&w[8..]))
            && let Some(found) = day(y, m, d)
        {
            return Some(found);
        }
        // YYYYMMDD, a run of exactly eight digits
        if let Some(w) = b.get(i..i + 8)
            && (i == 0 || !digit_at(i - 1))
            && !digit_at(i + 8)
            && let (Some(y), Some(m), Some(d)) = (num(&w[..4]), num(&w[4..6]), num(&w[6..]))
            && let Some(found) = day(y, m, d)
        {
            return Some(found);
        }
    }
    None
}

/// Days since 1970-01-01 (Howard Hinnant's civil-from-days, the other way).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn read_version() -> Option<String> {
    let exe = exe_path()?;
    if !exe.exists() {
        return None;
    }
    let out = command(&exe).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let text = if text.is_empty() { String::from_utf8_lossy(&out.stderr).trim().to_string() } else { text };
    Some(text)
}

fn days_left(version: &str) -> Option<i64> {
    let today = (now_ms() / 86_400_000) as i64;
    let built = build_day(version).or_else(|| {
        let ms: u64 = std::fs::read_to_string(installed_marker()?).ok()?.trim().parse().ok()?;
        Some((ms / 86_400_000) as i64)
    })?;
    Some(built + BUILD_LIFETIME_DAYS - today)
}

pub fn status() -> SoloistStatus {
    let installed = exe_path().is_some_and(|p| p.exists());
    let version = if installed { read_version().unwrap_or_default() } else { String::new() };
    SoloistStatus {
        installed,
        days_left: if installed { days_left(&version) } else { None },
        version,
        has_key: read_key().is_some(),
        paired: paired_marker().is_some_and(|p| p.exists()),
        running: engine().lock().map(|e| e.as_ref().is_some_and(|e| e.connected.load(Ordering::SeqCst))).unwrap_or(false),
        on: Settings::current().soloist_mode,
        device_name: device_name(),
        key_page: KEY_PAGE,
    }
}

#[tauri::command(async)]
pub fn soloist_status() -> SoloistStatus {
    status()
}

// ------------------------------------------------------------------ setup

/// Downloads Soloist from Spotify and puts soloist.exe (and whatever comes
/// with it) in bin, replacing an older build. Returns its --version.
#[tauri::command]
pub async fn soloist_install(app: AppHandle) -> Result<String, String> {
    let bin = bin_dir().ok_or("no config folder")?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("Spotiamp+/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client.get(DOWNLOAD_URL).send().await.map_err(|e| format!("download: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut bytes: Vec<u8> = Vec::with_capacity(total as usize);
    let mut last = Instant::now();
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("download: {e}"))? {
        bytes.extend_from_slice(&chunk);
        if last.elapsed() > Duration::from_millis(200) {
            last = Instant::now();
            let _ = app.emit("soloistDownload", serde_json::json!({ "done": bytes.len(), "total": total }));
        }
    }
    let _ = app.emit("soloistDownload", serde_json::json!({ "done": bytes.len(), "total": total.max(bytes.len() as u64) }));
    // a running Soloist holds the old exe: stop it first
    stop_engine();
    kill_strays();
    let staging = bin.with_extension("new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("archive: {e}"))?;
    let mut found = false;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // flattened: the files by their own names (no paths from the archive)
        let Some(name) = entry.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else { continue };
        if name.to_string_lossy().eq_ignore_ascii_case("soloist.exe") {
            found = true;
        }
        let mut out = std::fs::File::create(staging.join(&name)).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    if !found {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the archive has no soloist.exe".into());
    }
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::rename(&staging, &bin).map_err(|e| e.to_string())?;
    if let Some(m) = installed_marker() {
        let _ = std::fs::write(m, now_ms().to_string());
    }
    let version = read_version().unwrap_or_default();
    log::info!("Soloist installed: {version}");
    // (Soloist mode on: it plays on with the new build, also after an expired one stopped)
    if Settings::current().soloist_mode && read_key().is_some() {
        start_engine(&app);
    }
    Ok(version)
}

/// The user's own Soloist API key (kept in the config folder, never logged).
#[tauri::command(async)]
pub fn soloist_set_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.len() < 8 || key.chars().any(|c| c.is_whitespace()) {
        return Err("that doesn't look like a Soloist key".into());
    }
    let path = key_path().ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, key).map_err(|e| e.to_string())
}

static PAIRING: Mutex<Option<u32>> = Mutex::new(None);

/// Pairs it with the user's Spotify account: Soloist shows up in Spotify's
/// device list until it's picked there once, then keeps the session and
/// exits. Resolves true when paired (false: cancelled or timed out).
#[tauri::command]
pub async fn soloist_pair() -> Result<bool, String> {
    let exe = exe_path().filter(|p| p.exists()).ok_or("Soloist isn't installed")?;
    let key = read_key().ok_or("no Soloist key")?;
    let (data, cache) = (data_dir().ok_or("no folder")?, cache_dir().ok_or("no folder")?);
    let was_running = stop_engine();
    kill_strays();
    let mut child = command(&exe)
        .args(["--pair", "--device-name", &device_name(), "--api-key", &key])
        .arg("--data-dir")
        .arg(&data)
        .arg("--cache-dir")
        .arg(&cache)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Soloist didn't start: {e}"))?;
    if let Ok(mut p) = PAIRING.lock() {
        *p = Some(child.id());
    }
    let started = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(code)) => break Ok(code.code()),
            Ok(None) if started.elapsed() > Duration::from_secs(300) => {
                let _ = child.kill();
                break Ok(None);
            }
            Ok(None) => tokio::time::sleep(Duration::from_millis(300)).await,
            Err(e) => break Err(e.to_string()),
        }
    };
    if let Ok(mut p) = PAIRING.lock() {
        *p = None;
    }
    let paired = match result? {
        Some(0) => true,
        Some(10) => return Err("this Soloist build has expired: set it up again for a new one".into()),
        _ => false,
    };
    if paired && let Some(m) = paired_marker() {
        let _ = std::fs::write(m, now_ms().to_string());
    }
    log::info!("Soloist pairing {}", if paired { "done" } else { "stopped" });
    if was_running && let Some(app) = APP.get() {
        start_engine(app);
    }
    Ok(paired)
}

#[tauri::command(async)]
pub fn soloist_cancel_pair() {
    let pid = PAIRING.lock().ok().and_then(|p| *p);
    if let Some(pid) = pid {
        kill_pid(pid);
    }
}

/// Soloist mode on or off: on starts the background player (Spotify songs
/// then play through it), off stops it (librespot again).
#[tauri::command]
pub async fn soloist_set_mode(on: bool, app: AppHandle) -> Result<SoloistStatus, String> {
    if on {
        let s = status();
        if !s.installed || !s.has_key || !s.paired {
            return Err("Soloist isn't set up yet".into());
        }
    }
    Settings::current_mut().soloist_mode = on;
    if on {
        start_engine(&app);
        // its API up before the window sends anything
        for _ in 0..60 {
            if status().running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    } else {
        stop_engine();
    }
    Ok(status())
}

// ------------------------------------------------------------------ the engine

static APP: OnceLock<AppHandle> = OnceLock::new();

/// What the window asked for last, to tell our song's end from Soloist going on by itself.
#[derive(Default)]
struct Wanted {
    uri: String,
    /// start there once it's on (ms)
    position_ms: u32,
    /// start paused
    paused: bool,
    /// the position/pause still has to be applied
    pending: bool,
    /// it has played (so a change away from it is its end)
    started: bool,
    /// stopped by the window: Soloist's pause shows as Stopped
    stopped: bool,
}

struct Engine {
    pid: u32,
    /// commands for the socket
    tx: tokio::sync::mpsc::UnboundedSender<String>,
    connected: Arc<AtomicBool>,
    generation: u64,
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

fn engine() -> &'static Mutex<Option<Engine>> {
    static E: Mutex<Option<Engine>> = Mutex::new(None);
    &E
}
fn wanted() -> &'static Mutex<Wanted> {
    static W: OnceLock<Mutex<Wanted>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(Wanted::default()))
}

/// Soloist mode is on and its player answers: the player's Spotify commands come here.
pub fn active() -> bool {
    Settings::current().soloist_mode && engine().lock().map(|e| e.is_some()).unwrap_or(false)
}

fn send(cmd: serde_json::Value) {
    if let Ok(guard) = engine().lock()
        && let Some(e) = guard.as_ref()
    {
        let _ = e.tx.send(cmd.to_string());
    }
}
fn command_msg(name: &str) -> serde_json::Value {
    serde_json::json!({ "type": "command", "command": name })
}

/// At the app's start: the background player, if Soloist mode was left on.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    if !Settings::current().soloist_mode {
        return;
    }
    let s = status();
    if s.installed && s.has_key && s.paired {
        start_engine(app);
    }
    // a build near its end (they stop working after 90 days): a new one, quietly
    if s.installed && s.days_left.is_some_and(|d| d < RENEW_DAYS) {
        renew(app);
    }
}

/// Downloads a new build in the background (it takes over when it's there).
fn renew(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match soloist_install(app.clone()).await {
            Ok(v) => log::info!("Soloist renewed: {v}"),
            Err(e) => {
                log::warn!("Soloist couldn't be renewed: {e}");
                let _ = app.emit("soloistRenewFailed", e);
            }
        }
    });
}

/// Any soloist.exe of ours still running (an earlier run's): the data folder takes one at a time.
fn kill_strays() {
    let Some(exe) = exe_path() else { return };
    let exe = exe.to_string_lossy().to_lowercase();
    let mut sys = sysinfo_lite::processes();
    sys.retain(|(_, path)| path.to_lowercase() == exe);
    for (pid, _) in sys {
        kill_pid(pid);
    }
}

fn kill_pid(pid: u32) {
    let _ = command(Path::new("taskkill")).args(["/PID", &pid.to_string(), "/T", "/F"]).output();
}

/// Stops the background player. True if one was running.
fn stop_engine() -> bool {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let old = engine().lock().ok().and_then(|mut e| e.take());
    if let Some(e) = old {
        kill_pid(e.pid);
        true
    } else {
        false
    }
}

fn start_engine(app: &AppHandle) {
    stop_engine();
    kill_strays();
    let (Some(exe), Some(key), Some(data), Some(cache)) = (exe_path().filter(|p| p.exists()), read_key(), data_dir(), cache_dir()) else {
        log::warn!("Soloist: not set up, not started");
        return;
    };
    let _ = std::fs::create_dir_all(&data);
    let _ = std::fs::remove_file(data.join("ws.port"));
    let volume = Settings::current().player.volume.min(100);
    let child = command(&exe)
        // (its cache kept to 300 MB: unlimited, it would only grow)
        .args(["--device-name", &device_name(), "--api-key", &key, "--ws", "127.0.0.1:0", "--initial-volume", &volume.to_string(), "--cache-size", "300"])
        .arg("--data-dir")
        .arg(&data)
        .arg("--cache-dir")
        .arg(&cache)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Soloist didn't start: {e}");
            return;
        }
    };
    job::adopt(&child);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let connected = Arc::new(AtomicBool::new(false));
    if let Ok(mut e) = engine().lock() {
        *e = Some(Engine { pid: child.id(), tx, connected: connected.clone(), generation });
    }
    log::info!("Soloist started (pid {})", child.id());
    // its last words, for the log when it stops by itself (the key edited out)
    let stderr = child.stderr.take();
    let app2 = app.clone();
    std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        if let Some(err) = stderr {
            use std::io::BufRead;
            for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
                tail.push(line);
                if tail.len() > 20 {
                    tail.remove(0);
                }
            }
        }
        let code = child.wait().ok().and_then(|s| s.code());
        if GENERATION.load(Ordering::SeqCst) != generation {
            return; // stopped by us
        }
        let key = read_key().unwrap_or_default();
        let said = tail.join(" | ");
        let said = if key.is_empty() { said } else { said.replace(&key, "<key>") };
        log::warn!("Soloist stopped by itself (exit {code:?}): {said}");
        if let Ok(mut e) = engine().lock()
            && e.as_ref().is_some_and(|e| e.generation == generation)
        {
            *e = None;
        }
        let _ = app2.emit("soloistStopped", serde_json::json!({ "expired": code == Some(10), "code": code }));
        // an expired build: the new one, which starts again once it's there
        if code == Some(10) {
            renew(&app2);
        }
    });
    // the API's port, once it's open; then the socket
    let app3 = app.clone();
    tauri::async_runtime::spawn(async move {
        let port_file = data.join("ws.port");
        let mut port = None;
        for _ in 0..80 {
            if GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            if let Ok(s) = std::fs::read_to_string(&port_file)
                && let Ok(p) = s.trim().parse::<u16>()
            {
                port = Some(p);
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let Some(port) = port else {
            log::warn!("Soloist: its API didn't open");
            return;
        };
        run_socket(app3, port, rx, connected, generation).await;
    });
}

async fn run_socket(app: AppHandle, port: u16, mut rx: tokio::sync::mpsc::UnboundedReceiver<String>, connected: Arc<AtomicBool>, generation: u64) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let url = format!("ws://127.0.0.1:{port}");
    let ws = match tokio_tungstenite::connect_async(&url).await {
        Ok((ws, _)) => ws,
        Err(e) => {
            log::warn!("Soloist: no connection to its API ({e})");
            return;
        }
    };
    connected.store(true, Ordering::SeqCst);
    log::info!("Soloist: connected to its API");
    let _ = app.emit("soloistReady", ());
    let (mut sink, mut stream) = ws.split();
    let _ = sink.send(Message::Text(command_msg("get_auth_state").to_string().into())).await;
    let mut state = Live::default();
    loop {
        if GENERATION.load(Ordering::SeqCst) != generation {
            break;
        }
        tokio::select! {
            out = rx.recv() => {
                let Some(text) = out else { break };
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            msg = stream.next() => {
                let Some(Ok(msg)) = msg else { break };
                if let Message::Text(text) = msg
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
                {
                    state.handle(&app, &v);
                }
            }
        }
    }
    connected.store(false, Ordering::SeqCst);
    log::info!("Soloist: its API connection closed");
}

/// What Soloist plays now, from its events.
#[derive(Default)]
struct Live {
    uri: String,
    status: String,
    position_ms: u64,
    anchor_ms: u64,
    speed: f64,
}

impl Live {
    fn position(&self) -> u32 {
        if self.status == "playing" && self.anchor_ms > 0 {
            let gone = now_ms().saturating_sub(self.anchor_ms) as f64 * self.speed.max(0.0);
            (self.position_ms as f64 + gone) as u32
        } else {
            self.position_ms as u32
        }
    }

    fn handle(&mut self, app: &AppHandle, v: &serde_json::Value) {
        let kind = v["type"].as_str().unwrap_or("");
        match kind {
            // (right after it starts it says not yet: it's still opening its stored session)
            "auth_state" => log::info!("Soloist: {}", if v["logged_in"].as_bool().unwrap_or(false) { "logged in" } else { "not logged in yet" }),
            "playback_state" => {
                if let Some(uri) = v["item"]["uri"].as_str() {
                    self.track(app, uri);
                }
                self.anchor(&v["position"]);
                if let Some(s) = v["status"].as_str() {
                    self.set_status(app, s);
                }
            }
            "track_changed" => {
                if let Some(uri) = v["item"]["uri"].as_str() {
                    self.track(app, uri);
                }
            }
            "playback_changed" => {
                if let Some(s) = v["status"].as_str() {
                    self.set_status(app, s);
                }
            }
            "position_sync" => {
                self.anchor(&v["position"]);
                if self.ours() {
                    emit(app, "PositionCorrection", &self.uri, self.position());
                }
            }
            "error" => {
                let message = v["message"].as_str().unwrap_or("?");
                log::warn!("Soloist said: {message}");
                // its session is gone (revoked, another account): it needs pairing again
                if message.contains("authentication") {
                    let _ = app.emit("soloistNeedsPairing", ());
                }
            }
            _ => {}
        }
    }

    fn anchor(&mut self, p: &serde_json::Value) {
        if let Some(ms) = p["position_ms"].as_u64() {
            self.position_ms = ms;
            self.anchor_ms = p["timestamp_ms"].as_u64().unwrap_or_else(now_ms);
            self.speed = p["speed"].as_f64().unwrap_or(1.0);
        }
    }

    /// the track it's on is the one the window asked for
    fn ours(&self) -> bool {
        wanted().lock().map(|w| !w.uri.is_empty() && w.uri == self.uri).unwrap_or(false)
    }

    fn track(&mut self, app: &AppHandle, uri: &str) {
        if uri == self.uri {
            return;
        }
        let was_ours = self.ours();
        let (wanted_uri, started) = wanted().lock().map(|w| (w.uri.clone(), w.started)).unwrap_or_default();
        self.uri = uri.to_string();
        // it went on to a song of its own (autoplay, the context's next):
        // our song is over; the window loads the next one of its playlist
        if was_ours && started && uri != wanted_uri {
            send(command_msg("pause"));
            emit(app, "EndOfTrack", &wanted_uri, 0);
        }
    }

    fn set_status(&mut self, app: &AppHandle, status: &str) {
        let before = std::mem::replace(&mut self.status, status.to_string());
        if !self.ours() {
            return;
        }
        let uri = self.uri.clone();
        match status {
            "playing" => {
                let mut apply: Option<(u32, bool)> = None;
                if let Ok(mut w) = wanted().lock() {
                    w.started = true;
                    w.stopped = false;
                    if w.pending {
                        w.pending = false;
                        apply = Some((w.position_ms, w.paused));
                    }
                }
                if let Some((pos, paused)) = apply {
                    if pos > 1000 {
                        send(serde_json::json!({ "type": "command", "command": "seek", "position_ms": pos }));
                    }
                    if paused {
                        send(command_msg("pause"));
                        return;
                    }
                }
                emit(app, "Playing", &uri, self.position());
            }
            "paused" => {
                let stopped = wanted().lock().map(|w| w.stopped).unwrap_or(false);
                if stopped {
                    emit(app, "Stopped", &uri, 0);
                } else {
                    emit(app, "Paused", &uri, self.position());
                }
            }
            // done with it and nothing after: the end of our song
            "idle" if before == "playing" || before == "paused" => {
                let started = wanted().lock().map(|w| w.started).unwrap_or(false);
                if started {
                    emit(app, "EndOfTrack", &uri, 0);
                }
            }
            _ => {}
        }
    }
}

/// The player's own event, as librespot's are sent (lib.rs SpotiampPlayerEvent).
fn emit(app: &AppHandle, kind: &str, uri: &str, position_ms: u32) {
    let payload = match kind {
        "Stopped" | "EndOfTrack" | "Unavailable" => serde_json::json!({ kind: { "uri": uri } }),
        _ => serde_json::json!({ kind: { "uri": uri, "position_ms": position_ms } }),
    };
    if let Some(w) = app.get_webview_window("player") {
        let _ = w.emit("player", payload);
    }
}

// ------------------------------------------------------------------ the player's commands

pub fn load(uri: &str, position_ms: u32, play: bool) {
    if let Ok(mut w) = wanted().lock() {
        *w = Wanted { uri: uri.to_string(), position_ms, paused: !play, pending: position_ms > 1000 || !play, started: false, stopped: false };
    }
    send(serde_json::json!({ "type": "command", "command": "play", "uri": uri }));
}
pub fn play() {
    // (a stopped song was put back to its start by stop(), as librespot's is)
    if let Ok(mut w) = wanted().lock() {
        w.stopped = false;
    }
    send(command_msg("play"));
}
pub fn pause() {
    send(command_msg("pause"));
}
pub fn stop() {
    if let Ok(mut w) = wanted().lock() {
        w.stopped = true;
    }
    send(command_msg("pause"));
    send(serde_json::json!({ "type": "command", "command": "seek", "position_ms": 0 }));
}
pub fn seek(position_ms: u32) {
    send(serde_json::json!({ "type": "command", "command": "seek", "position_ms": position_ms }));
}
pub fn set_volume(volume: u16) {
    send(serde_json::json!({ "type": "command", "command": "set_volume", "volume": volume.min(100) }));
}

// ------------------------------------------------------------------ the setup window

const SETUP_SIZE: crate::settings::InnerWindowSize = crate::settings::InnerWindowSize { width: 330, height: 440 };

/// The window that sets Soloist up (download, key, pairing) and turns it on.
#[tauri::command]
pub async fn show_soloist_setup(app_handle: AppHandle) -> Result<(), ()> {
    let window = match app_handle.get_webview_window("soloist") {
        Some(window) => window,
        None => {
            let window = crate::app_window::build_frameless_window(&app_handle, "soloist", "Spotify Soloist - Spotiamp+", "soloist", SETUP_SIZE).map_err(|_| ())?;
            let _ = window.center();
            crate::app_window::own_by_player(&app_handle, &window);
            window
        }
    };
    window.show().map_err(|_| ())?;
    window.set_focus().map_err(|_| ())?;
    Ok(())
}

#[tauri::command]
pub fn close_soloist_setup(app_handle: AppHandle) {
    if let Some(window) = app_handle.get_webview_window("soloist") {
        let _ = window.destroy();
    }
}

/// Stops it with the app.
pub fn shutdown() {
    stop_engine();
}

// ------------------------------------------------------------------ helpers

/// The processes running and their exe paths (only what kill_strays needs).
mod sysinfo_lite {
    #[cfg(windows)]
    pub fn processes() -> Vec<(u32, String)> {
        use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
        use windows::Win32::System::ProcessStatus::EnumProcesses;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
        let mut pids = vec![0u32; 4096];
        let mut needed = 0u32;
        unsafe {
            if EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut needed).is_err() {
                return Vec::new();
            }
        }
        pids.truncate(needed as usize / 4);
        let mut out = Vec::new();
        for pid in pids {
            if pid == 0 {
                continue;
            }
            unsafe {
                let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { continue };
                let mut buf = [0u16; MAX_PATH as usize * 2];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                    out.push((pid, String::from_utf16_lossy(&buf[..len as usize])));
                }
                let _ = CloseHandle(h);
            }
        }
        out
    }
    #[cfg(not(windows))]
    pub fn processes() -> Vec<(u32, String)> {
        Vec::new()
    }
}

/// A job object the background player is put in, so it ends with Spotiamp+
/// even when Spotiamp+ doesn't get to stop it (a crash, the Task Manager).
mod job {
    #[cfg(windows)]
    pub fn adopt(child: &std::process::Child) {
        use std::os::windows::io::AsRawHandle;
        use std::sync::OnceLock;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        static JOB: OnceLock<isize> = OnceLock::new();
        let job = *JOB.get_or_init(|| unsafe {
            let Ok(job) = CreateJobObjectW(None, None) else { return 0 };
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let _ = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            job.0 as isize
        });
        if job != 0 {
            unsafe {
                let _ = AssignProcessToJobObject(HANDLE(job as *mut core::ffi::c_void), HANDLE(child.as_raw_handle()));
            }
        }
    }
    #[cfg(not(windows))]
    pub fn adopt(_child: &std::process::Child) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_date_is_read_from_its_version_line() {
        assert_eq!(build_day("soloist 1.3.10.15 (built 2026-11-10)"), Some(days_from_civil(2026, 11, 10)));
        assert_eq!(build_day("soloist 1.3.10 20261110"), Some(days_from_civil(2026, 11, 10)));
        assert_eq!(build_day("soloist 1.3.10.15"), None);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
    }
}
