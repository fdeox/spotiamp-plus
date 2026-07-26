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

/// A file opened and ready to decode, plus the cpal stream playing it.
struct Active {
    /// Format reader + decoder, kept together so we can seek and pull packets.
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    ring: Arc<Mutex<VecDeque<f32>>>,
    frames_played: Arc<AtomicU64>,
    finished_ring: Arc<AtomicBool>,
    _stream: cpal::Stream,
    ended_sent: bool,
    sample_buf: Option<SampleBuffer<f32>>,
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

        let stream = self.build_stream(sample_rate, channels, &ring, &frames_played)?;

        Ok(Active {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            ring,
            frames_played,
            finished_ring,
            _stream: stream,
            ended_sent: false,
            sample_buf: None,
        })
    }

    /// cpal output stream at the file's native rate. The callback pulls decoded
    /// samples, runs the EQ in place (reusing `EqProcessor`, live settings), and
    /// pushes the result to the visualizer — the same order the Spotify sink
    /// uses, so both paths sound and look identical.
    fn build_stream(
        &self,
        sample_rate: u32,
        channels: usize,
        ring: &Arc<Mutex<VecDeque<f32>>>,
        frames_played: &Arc<AtomicU64>,
    ) -> Result<cpal::Stream, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no output device")?;
        let config = cpal::StreamConfig {
            channels: channels as u16,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let ring = ring.clone();
        let frames_played = frames_played.clone();
        let eq = self.eq.clone();
        let visualizer = self.visualizer.clone();
        let playing = self.playing.clone();

        let mut eq_proc = EqProcessor::new();
        let mut scratch: Vec<f64> = Vec::new();
        let ch = channels;

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
                            a.ring.lock().unwrap().extend(buf.samples().iter().copied());
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
