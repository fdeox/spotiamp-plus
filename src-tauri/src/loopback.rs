//! System-audio loopback capture for the Free (controller) mode visualizer.
//!
//! In controller mode there's no librespot sink to tap for a spectrum, so the
//! audio never passes through us. Instead we open the default OUTPUT device as
//! a WASAPI loopback INPUT stream (cpal enables loopback when you build an
//! input stream on a render device) and feed those samples into the same
//! audioviz spectrum the normal visualizer uses.
//!
//! The cpal stream is `!Send`, so it's built and parked on a dedicated thread;
//! the spectrum is shared through an `Arc<Mutex<Visualizer>>` the command reads.

use std::sync::{Arc, Mutex, OnceLock};

use crate::visualizer::Visualizer;

fn shared() -> &'static Arc<Mutex<Option<Arc<Mutex<Visualizer>>>>> {
    static VIZ: OnceLock<Arc<Mutex<Option<Arc<Mutex<Visualizer>>>>>> = OnceLock::new();
    VIZ.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Start the loopback capture once. Safe to call repeatedly — after the first
/// success it does nothing.
#[cfg(target_os = "windows")]
fn start_capture() {
    // Already running?
    if shared().lock().map(|g| g.is_some()).unwrap_or(true) {
        return;
    }

    std::thread::spawn(|| {
        // Follow the default output device instead of binding to whichever one
        // happened to be default at startup: plugging in headphones changes it,
        // and a stream left on the old device just captures silence — which
        // looked exactly like "the visualizer doesn't work".
        let mut open: Option<(cpal::Stream, String)> = None;
        loop {
            let current = default_device_name();
            let changed = match (&open, &current) {
                (Some((_, name)), Some(now)) => name != now,
                (None, Some(_)) => true,
                (Some(_), None) => true,
                (None, None) => false,
            };
            if changed {
                // Drop the previous stream first so the old device is released.
                open = None;
                if let Ok(mut guard) = shared().lock() {
                    *guard = None;
                }
                if let Some((stream, viz, name, rate)) = open_default_device() {
                    if let Ok(mut guard) = shared().lock() {
                        *guard = Some(viz);
                    }
                    log::info!("loopback: capturing '{name}' at {rate} Hz");
                    open = Some((stream, name));
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

#[cfg(target_os = "windows")]
fn default_device_name() -> Option<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
}

/// Open a loopback capture on the current default output device.
#[cfg(target_os = "windows")]
fn open_default_device() -> Option<(cpal::Stream, Arc<Mutex<Visualizer>>, String, u32)> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let device = cpal::default_host().default_output_device()?;
    let name = device.name().unwrap_or_else(|_| "?".to_string());
    // cpal rejects default_input_config() on a render device, but building an
    // input stream on one still enables loopback — so take the format from the
    // output config.
    let config = match device.default_output_config() {
        Ok(config) => config,
        Err(e) => {
            log::warn!("loopback: no output config for '{name}' ({e:?})");
            return None;
        }
    };
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let sample_format = config.sample_format();

    let viz = Arc::new(Mutex::new(Visualizer::with_sample_rate(sample_rate)));
    let viz_cb = viz.clone();

    // Average the interleaved channels down to mono before pushing.
    let push = move |mono: Vec<f32>| {
        if let Ok(mut v) = viz_cb.lock() {
            v.push_mono(mono);
        }
    };
    let to_mono = move |data: &[f32]| -> Vec<f32> {
        if channels <= 1 {
            return data.to_vec();
        }
        data.chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    let err_fn = |e| log::warn!("loopback stream error: {e:?}");
    let cfg: cpal::StreamConfig = config.into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &cfg,
            move |data: &[f32], _| push(to_mono(data)),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &cfg,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                push(to_mono(&f));
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &cfg,
            move |data: &[u16], _| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|&s| (s as f32 - 32768.0) / 32768.0)
                    .collect();
                push(to_mono(&f));
            },
            err_fn,
            None,
        ),
        other => {
            log::warn!("loopback: unsupported sample format {other:?}");
            return None;
        }
    };

    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            log::warn!("loopback: could not build stream on '{name}' ({e:?})");
            return None;
        }
    };
    if let Err(e) = stream.play() {
        log::warn!("loopback: could not start stream on '{name}' ({e:?})");
        return None;
    }
    Some((stream, viz, name, sample_rate))
}

#[cfg(not(target_os = "windows"))]
fn start_capture() {}

#[tauri::command]
pub fn start_loopback() {
    start_capture();
}

/// The current spectrum, in the same `(freq, volume)` shape the player's
/// `take_latest_spectrum` returns, so the visualizer surfaces can poll either
/// one interchangeably. Empty until capture is running (or while nothing plays,
/// which loopback reports as no data — the visualizer idles, as it should).
#[tauri::command]
pub fn loopback_spectrum() -> Vec<(f32, f32)> {
    let handle = shared().lock().ok().and_then(|g| g.clone());
    match handle {
        Some(viz) => viz
            .lock()
            .map(|mut v| v.take_latest_spectrum())
            .unwrap_or_default(),
        None => Vec::new(),
    }
}
