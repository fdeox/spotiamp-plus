//! Local-file playback (0.7.0): play an MP3/FLAC/AAC/WAV from disk through the
//! same EQ + visualizer the Spotify path uses, so a local track and a Spotify
//! track behave the same in the player.
//!
//! Proven feasible first in an isolated probe: symphonia (already in the tree)
//! decodes the file, a background thread fills a ring buffer, and a cpal output
//! stream plays it gap-free while a tap in the callback carries the samples to
//! the EQ and the visualizer.
//!
//! Shape mirrors `SpotifyPlayer` (`load`/`play`/`pause`/`stop`/`seek`, a
//! position/state to poll, an event queue) so the rest of the app can drive a
//! local track the same way — the wiring into the playlist/UI is the remaining
//! step. Additive: nothing here touches the librespot path, so it cannot break
//! existing playback until it's deliberately hooked up.

#![allow(dead_code)] // UI wiring (drag-drop / picker) lands in a later step

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use crate::eq::{EqProcessor, EqState};
use crate::visualizer::Visualizer;

/// What the worker reports back, matching the shape of the librespot player's
/// events so the frontend can treat local and Spotify tracks alike.
#[derive(Debug, Clone, serde::Serialize)]
pub enum LocalEvent {
    Playing { position_ms: u32 },
    Paused { position_ms: u32 },
    Stopped,
    EndOfTrack,
    Position { position_ms: u32 },
    /// The file couldn't be opened or decoded.
    Failed { path: PathBuf, reason: String },
}

enum Command {
    Load(PathBuf),
    Play,
    Pause,
    Stop,
    Seek(u32),
}

/// Handle the app holds. Commands are queued to the worker thread; events and
/// the spectrum are read back through shared state.
pub struct LocalPlayer {
    cmd: mpsc::Sender<Command>,
    events: Arc<Mutex<VecDeque<LocalEvent>>>,
    position_ms: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    duration_ms: Arc<AtomicU64>,
}

impl LocalPlayer {
    /// Share the SAME `EqState` and `Visualizer` the Spotify sink uses, so EQ
    /// settings and the spectrum window carry over to local playback unchanged.
    pub fn new(eq: Arc<Mutex<EqState>>, visualizer: Arc<Mutex<Visualizer>>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let position_ms = Arc::new(AtomicU64::new(0));
        let playing = Arc::new(AtomicBool::new(false));
        let duration_ms = Arc::new(AtomicU64::new(0));

        let worker = Worker {
            eq,
            visualizer,
            events: events.clone(),
            position_ms: position_ms.clone(),
            playing: playing.clone(),
            duration_ms: duration_ms.clone(),
        };
        std::thread::Builder::new()
            .name("local-player".into())
            .spawn(move || worker.run(cmd_rx))
            .expect("spawn local-player thread");

        Self {
            cmd: cmd_tx,
            events,
            position_ms,
            playing,
            duration_ms,
        }
    }

    pub fn load(&self, path: PathBuf) {
        let _ = self.cmd.send(Command::Load(path));
    }
    pub fn play(&self) {
        let _ = self.cmd.send(Command::Play);
    }
    pub fn pause(&self) {
        let _ = self.cmd.send(Command::Pause);
    }
    pub fn stop(&self) {
        let _ = self.cmd.send(Command::Stop);
    }
    pub fn seek(&self, position_ms: u32) {
        let _ = self.cmd.send(Command::Seek(position_ms));
    }

    pub fn position_ms(&self) -> u32 {
        self.position_ms.load(Ordering::Relaxed) as u32
    }
    pub fn duration_ms(&self) -> u32 {
        self.duration_ms.load(Ordering::Relaxed) as u32
    }
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Drain queued events (the app forwards them to the player window, the way
    /// it forwards the librespot event channel).
    pub fn take_events(&self) -> Vec<LocalEvent> {
        self.events.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default()
    }
}

/// The output device to play through: the one the user picked for Spotify
/// (saved in settings), else the system default. Keeping local files on the
/// chosen device matters when the system default is a virtual/monitor output.
fn select_output_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    crate::settings::Settings::current()
        .player
        .audio_device
        .clone()
        .and_then(|name| {
            host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| d.name().map(|n| n == name).unwrap_or(false))
            })
        })
        .or_else(|| host.default_output_device())
}

/// Streaming linear resampler + channel remap, from the file's rate/channels to
/// the output device's. It exists because a file whose sample rate the device
/// can't play natively used to fail to open the stream at all (silent stop);
/// now it's converted to a rate the device does support. Linear is modest
/// quality but reliable and dependency-free, and it only kicks in on a mismatch.
struct Resampler {
    /// Input frames advanced per output frame (in_rate / out_rate).
    step: f64,
    in_ch: usize,
    out_ch: usize,
    /// Interleaved input frames not yet fully consumed.
    buf: VecDeque<f32>,
    /// Fractional read position within `buf`, in frames.
    pos: f64,
}

impl Resampler {
    fn new(in_rate: u32, out_rate: u32, in_ch: usize, out_ch: usize) -> Self {
        Self {
            step: in_rate as f64 / out_rate.max(1) as f64,
            in_ch: in_ch.max(1),
            out_ch: out_ch.max(1),
            buf: VecDeque::new(),
            pos: 0.0,
        }
    }

    /// Feed interleaved input frames (`in_ch` each) and append interleaved output
    /// frames (`out_ch` each) to `out`.
    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.buf.extend(input.iter().copied());
        let frames = self.buf.len() / self.in_ch;
        // Need frame i+1 to interpolate toward, so stop one short of the end.
        while (self.pos as usize) + 1 < frames {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            for c in 0..self.out_ch {
                let sc = c.min(self.in_ch - 1); // map/duplicate channels
                let a = self.buf[i * self.in_ch + sc];
                let b = self.buf[(i + 1) * self.in_ch + sc];
                out.push(a + (b - a) * frac);
            }
            self.pos += self.step;
        }
        // Drop the input frames we've moved past, keep the fractional remainder.
        let consumed = self.pos as usize;
        if consumed > 0 {
            self.buf.drain(0..consumed * self.in_ch);
            self.pos -= consumed as f64;
        }
    }
}

/// A file opened and ready to decode, plus the cpal stream playing it.
struct Active {
    /// Format reader + decoder, kept together so we can seek and pull packets.
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    /// Output stream rate/channels (what the ring holds and the callback plays).
    sample_rate: u32,
    channels: usize,
    ring: Arc<Mutex<VecDeque<f32>>>,
    frames_played: Arc<AtomicU64>,
    finished_ring: Arc<AtomicBool>,
    _stream: cpal::Stream,
    ended_sent: bool,
    sample_buf: Option<SampleBuffer<f32>>,
    /// Set when the file's rate/channels differ from the output device's.
    resampler: Option<Resampler>,
    /// Scratch for resampled output, reused across packets.
    resample_out: Vec<f32>,
}

struct Worker {
    eq: Arc<Mutex<EqState>>,
    visualizer: Arc<Mutex<Visualizer>>,
    events: Arc<Mutex<VecDeque<LocalEvent>>>,
    position_ms: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    duration_ms: Arc<AtomicU64>,
}

impl Worker {
    fn emit(&self, event: LocalEvent) {
        if let Ok(mut q) = self.events.lock() {
            q.push_back(event);
        }
    }

    fn run(self, cmd_rx: mpsc::Receiver<Command>) {
        let mut active: Option<Active> = None;

        loop {
            // Drain any pending commands without blocking, so decoding stays
            // responsive; block only when there's nothing to do.
            let cmd = if active.is_some() {
                cmd_rx.try_recv().ok()
            } else {
                cmd_rx.recv().ok()
            };

            match cmd {
                Some(Command::Load(path)) => match self.open(&path) {
                    Ok(a) => {
                        self.duration_ms.store(0, Ordering::Relaxed);
                        self.position_ms.store(0, Ordering::Relaxed);
                        self.playing.store(true, Ordering::Relaxed);
                        active = Some(a);
                        self.emit(LocalEvent::Playing { position_ms: 0 });
                    }
                    Err(reason) => {
                        log::warn!("local open failed for '{}': {}", path.display(), reason);
                        active = None;
                        self.playing.store(false, Ordering::Relaxed);
                        self.emit(LocalEvent::Failed { path, reason });
                    }
                },
                Some(Command::Play) => {
                    if active.is_some() {
                        self.playing.store(true, Ordering::Relaxed);
                        self.emit(LocalEvent::Playing {
                            position_ms: self.position_ms.load(Ordering::Relaxed) as u32,
                        });
                    }
                }
                Some(Command::Pause) => {
                    if active.is_some() {
                        self.playing.store(false, Ordering::Relaxed);
                        self.emit(LocalEvent::Paused {
                            position_ms: self.position_ms.load(Ordering::Relaxed) as u32,
                        });
                    }
                }
                Some(Command::Stop) => {
                    active = None;
                    self.playing.store(false, Ordering::Relaxed);
                    self.position_ms.store(0, Ordering::Relaxed);
                    self.emit(LocalEvent::Stopped);
                }
                Some(Command::Seek(ms)) => {
                    if let Some(a) = active.as_mut() {
                        self.seek(a, ms);
                    }
                }
                None if active.is_none() => break, // sender dropped
                None => {}
            }

            // Keep the current file's ring topped up and advance the clock.
            if let Some(a) = active.as_mut() {
                let ended = self.pump(a);
                // Position from frames actually played out (not decoded ahead).
                let played = a.frames_played.load(Ordering::Relaxed);
                let pos = (played * 1000 / a.sample_rate.max(1) as u64) as u32;
                self.position_ms.store(pos as u64, Ordering::Relaxed);
                if self.playing.load(Ordering::Relaxed) {
                    self.emit(LocalEvent::Position { position_ms: pos });
                }
                if ended && !a.ended_sent {
                    a.ended_sent = true;
                    self.playing.store(false, Ordering::Relaxed);
                    self.emit(LocalEvent::EndOfTrack);
                }
                // ~60 Hz service loop; the cpal callback runs independently.
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }

    /// Open a file, build its cpal stream, and return the active state. The
    /// stream's callback is where EQ runs and the visualizer is fed.
    fn open(&self, path: &PathBuf) -> Result<Active, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| format!("unsupported: {e}"))?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.sample_rate.is_some())
            .ok_or("no audio track")?
            .clone();
        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.ok_or("no sample rate")?;
        let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2).max(1);
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("no decoder: {e}"))?;

        // Report duration if the container knows it (for the seek bar).
        if let (Some(n), tb) = (track.codec_params.n_frames, track.codec_params.time_base) {
            if let Some(tb) = tb {
                let t = tb.calc_time(n);
                let ms = (t.seconds as f64 + t.frac) * 1000.0;
                self.duration_ms.store(ms as u64, Ordering::Relaxed);
            }
        }

        let ring: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(1 << 18)));
        let frames_played = Arc::new(AtomicU64::new(0));
        let finished_ring = Arc::new(AtomicBool::new(false));

        // Pick the output device (the one chosen for Spotify, else the system
        // default) and use its OWN default config. Forcing the file's exact rate
        // on a device that can't do it is what made some files fail to open (a
        // silent stop with no log). Anything that doesn't match is resampled.
        let device = select_output_device().ok_or("no output device")?;
        let out_cfg = device
            .default_output_config()
            .map_err(|e| format!("no output config: {e}"))?;
        let out_rate = out_cfg.sample_rate().0;
        let out_channels = (out_cfg.channels() as usize).max(1);
        let resampler = if out_rate != sample_rate || out_channels != channels {
            Some(Resampler::new(sample_rate, out_rate, channels, out_channels))
        } else {
            None
        };

        // Log the format decision so a "plays nothing" report is diagnosable
        // from spotiamp.log (device chosen, file vs output rate/channels).
        log::info!(
            "local open: '{}' {} Hz {} ch -> device '{}' {} Hz {} ch ({} fmt){}",
            path.display(),
            sample_rate,
            channels,
            device.name().unwrap_or_else(|_| "?".into()),
            out_rate,
            out_channels,
            out_cfg.sample_format(),
            if resampler.is_some() { ", resampling" } else { "" },
        );

        let stream = self.build_stream(&device, out_rate, out_channels, &ring, &frames_played)?;

        Ok(Active {
            format,
            decoder,
            track_id,
            sample_rate: out_rate,
            channels: out_channels,
            ring,
            frames_played,
            finished_ring,
            _stream: stream,
            ended_sent: false,
            sample_buf: None,
            resampler,
            resample_out: Vec::new(),
        })
    }

    /// cpal output stream at the device's own supported rate/channels. The
    /// callback pulls decoded (and, when rates differ, resampled) samples, runs
    /// the EQ in place (reusing `EqProcessor`, live settings), and pushes the
    /// result to the visualizer — the same order the Spotify sink uses.
    fn build_stream(
        &self,
        device: &cpal::Device,
        out_rate: u32,
        out_channels: usize,
        ring: &Arc<Mutex<VecDeque<f32>>>,
        frames_played: &Arc<AtomicU64>,
    ) -> Result<cpal::Stream, String> {
        let config = cpal::StreamConfig {
            channels: out_channels as u16,
            sample_rate: cpal::SampleRate(out_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let ring = ring.clone();
        let frames_played = frames_played.clone();
        let eq = self.eq.clone();
        let visualizer = self.visualizer.clone();
        let playing = self.playing.clone();

        let mut eq_proc = EqProcessor::new();
        let mut scratch: Vec<f64> = Vec::new();
        let ch = out_channels;

        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    // Paused: output silence, don't consume the ring, freeze the
                    // clock. (Play resumes exactly where it left off.)
                    if !playing.load(Ordering::Relaxed) {
                        out.iter_mut().for_each(|s| *s = 0.0);
                        return;
                    }

                    let mut got = 0usize;
                    {
                        let mut buf = ring.lock().unwrap();
                        for s in out.iter_mut() {
                            if let Some(v) = buf.pop_front() {
                                *s = v;
                                got += 1;
                            } else {
                                *s = 0.0; // underrun / end: silence
                            }
                        }
                    }

                    // EQ in place (f64), matching the Spotify sink. Only over the
                    // frames we actually pulled, so trailing silence stays clean.
                    if got > 0 {
                        if scratch.len() < got {
                            scratch.resize(got, 0.0);
                        }
                        for i in 0..got {
                            scratch[i] = out[i] as f64;
                        }
                        if let Ok(state) = eq.lock() {
                            eq_proc.process(&state, &mut scratch[..got]);
                        }
                        for i in 0..got {
                            out[i] = scratch[i] as f32;
                        }
                        if let Ok(mut v) = visualizer.lock() {
                            v.push_samples(&out[..got]);
                        }
                        frames_played.fetch_add((got / ch.max(1)) as u64, Ordering::Relaxed);
                    }
                },
                |e| log::warn!("local-player stream error: {e}"),
                None,
            )
            .map_err(|e| format!("build stream: {e}"))?;
        stream.play().map_err(|e| format!("play stream: {e}"))?;
        Ok(stream)
    }

    /// Decode a little further into the ring; return true when the file is fully
    /// decoded AND played out (so the caller can emit EndOfTrack).
    fn pump(&self, a: &mut Active) -> bool {
        // Keep a modest lead over playback so live EQ/seek stay responsive.
        const TARGET: usize = 1 << 16; // ~0.3–0.7 s depending on rate/channels
        loop {
            let len = a.ring.lock().unwrap().len();
            if len >= TARGET {
                break;
            }
            match a.format.next_packet() {
                Ok(packet) => {
                    if packet.track_id() != a.track_id {
                        continue;
                    }
                    match a.decoder.decode(&packet) {
                        Ok(decoded) => {
                            let spec = *decoded.spec();
                            let buf = a
                                .sample_buf
                                .get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
                            buf.copy_interleaved_ref(decoded);
                            if let Some(rs) = a.resampler.as_mut() {
                                a.resample_out.clear();
                                rs.process(buf.samples(), &mut a.resample_out);
                                a.ring.lock().unwrap().extend(a.resample_out.iter().copied());
                            } else {
                                a.ring.lock().unwrap().extend(buf.samples().iter().copied());
                            }
                        }
                        Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                        Err(_) => {
                            a.finished_ring.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
                Err(_) => {
                    a.finished_ring.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
        // Ended once decoding is done and the ring has drained to the speaker.
        a.finished_ring.load(Ordering::Relaxed) && a.ring.lock().unwrap().is_empty()
    }

    /// Seek the reader to `ms`, clear the buffered audio, and reset the clock so
    /// position tracks from the new point.
    fn seek(&self, a: &mut Active, ms: u32) {
        let time = Time::from(ms as f64 / 1000.0);
        let _ = a.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(a.track_id),
            },
        );
        a.decoder.reset();
        a.ring.lock().unwrap().clear();
        if let Some(rs) = a.resampler.as_mut() {
            rs.buf.clear();
            rs.pos = 0.0;
        }
        a.finished_ring.store(false, Ordering::Relaxed);
        a.ended_sent = false;
        let frames = (ms as u64) * a.sample_rate as u64 / 1000;
        a.frames_played.store(frames, Ordering::Relaxed);
        self.position_ms.store(ms as u64, Ordering::Relaxed);
    }
}

// --- tauri commands --------------------------------------------------------
// The player is managed as `Mutex<LocalPlayer>` (its command Sender isn't Sync).
// Each call just queues a message to the worker, so the lock is held briefly.

type State<'a> = tauri::State<'a, Mutex<LocalPlayer>>;

#[tauri::command]
pub fn local_load(path: String, player: State) {
    if let Ok(p) = player.lock() {
        p.load(PathBuf::from(path));
    }
}
#[tauri::command]
pub fn local_play(player: State) {
    if let Ok(p) = player.lock() {
        p.play();
    }
}
#[tauri::command]
pub fn local_pause(player: State) {
    if let Ok(p) = player.lock() {
        p.pause();
    }
}
#[tauri::command]
pub fn local_stop(player: State) {
    if let Ok(p) = player.lock() {
        p.stop();
    }
}
#[tauri::command]
pub fn local_seek(position_ms: u32, player: State) {
    if let Ok(p) = player.lock() {
        p.seek(position_ms);
    }
}
#[tauri::command]
pub fn local_position(player: State) -> u32 {
    player.lock().map(|p| p.position_ms()).unwrap_or(0)
}
#[tauri::command]
pub fn local_duration(player: State) -> u32 {
    player.lock().map(|p| p.duration_ms()).unwrap_or(0)
}
#[tauri::command]
pub fn local_is_playing(player: State) -> bool {
    player.lock().map(|p| p.is_playing()).unwrap_or(false)
}
/// Drain queued events so the frontend can react (position, EndOfTrack, …).
#[tauri::command]
pub fn local_take_events(player: State) -> Vec<LocalEvent> {
    player.lock().map(|p| p.take_events()).unwrap_or_default()
}

/// Audio extensions Spotiamp+ can decode via symphonia. Kept in sync with the
/// player window's drag-drop filter.
const AUDIO_EXTS: [&str; 9] = [
    "mp3", "flac", "m4a", "aac", "wav", "ogg", "oga", "opus", "wma",
];

/// Show a picker for one or more audio files and return their absolute paths
/// (empty when cancelled). A discoverable entry point alongside drag-drop; the
/// frontend plays the first and queues the rest.
#[tauri::command]
pub async fn local_pick_files(app_handle: tauri::AppHandle) -> Vec<String> {
    use tauri_plugin_dialog::DialogExt;
    app_handle
        .dialog()
        .file()
        .add_filter("Audio", &AUDIO_EXTS)
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Pick a folder and return every audio file inside it (walks subfolders, since
/// real music libraries are nested), sorted for a stable play order.
#[tauri::command]
pub async fn local_pick_folder(app_handle: tauri::AppHandle) -> Vec<String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(dir) = app_handle
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_audio_files(&dir, &mut out, 0);
    out.sort();
    out
}

/// Recursively gather audio files under `dir`. Depth-bounded so an accidental
/// pick of a drive root can't walk the whole disk forever.
fn collect_audio_files(dir: &std::path::Path, out: &mut Vec<String>, depth: u32) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio_files(&path, out, depth + 1);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
        {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// Title / artist / album / duration read from a file's tags, so a local track
/// shows real names instead of just its filename.
#[derive(Debug, Default, serde::Serialize)]
pub struct LocalMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
}

/// Probe a single file for its tags + duration. Separate from the playback
/// path (no stream is built), so it's cheap to call the moment a file loads.
#[tauri::command]
pub fn local_metadata(path: String) -> LocalMeta {
    read_local_meta(&PathBuf::from(path)).unwrap_or_default()
}

fn apply_tags(rev: &symphonia::core::meta::MetadataRevision, meta: &mut LocalMeta) {
    use symphonia::core::meta::StandardTagKey;
    for tag in rev.tags() {
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) if meta.title.is_empty() => {
                meta.title = tag.value.to_string();
            }
            Some(StandardTagKey::Artist) if meta.artist.is_empty() => {
                meta.artist = tag.value.to_string();
            }
            Some(StandardTagKey::AlbumArtist) if meta.artist.is_empty() => {
                meta.artist = tag.value.to_string();
            }
            Some(StandardTagKey::Album) if meta.album.is_empty() => {
                meta.album = tag.value.to_string();
            }
            _ => {}
        }
    }
}

fn read_local_meta(path: &std::path::Path) -> Option<LocalMeta> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut meta = LocalMeta::default();

    if let Some(track) = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        && let (Some(n), Some(tb)) =
            (track.codec_params.n_frames, track.codec_params.time_base)
    {
        let t = tb.calc_time(n);
        meta.duration_ms = ((t.seconds as f64 + t.frac) * 1000.0) as u32;
    }

    // Tags the probe collected up front (ID3v2 on MP3 usually lands here)...
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        apply_tags(rev, &mut meta);
    }
    // ...and tags the format reader exposes (Vorbis comments, etc.).
    if let Some(rev) = probed.format.metadata().current() {
        apply_tags(rev, &mut meta);
    }
    Some(meta)
}
