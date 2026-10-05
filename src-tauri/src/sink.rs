use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use librespot::playback::audio_backend::{self, Sink, SinkResult};
use librespot::playback::config::AudioFormat;
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;

use crate::eq::{EqProcessor, EqState};
use crate::visualizer::Visualizer;

/// Map a 0..100 slider position to an amplitude multiplier.
///
/// A square curve: 50 % is -12 dB, 25 % -24 dB, 10 % -40 dB. Loudness is
/// heard roughly logarithmically, so the straight `vol/100` it once was put
/// the whole audible range in the slider's bottom fifth (the "I can't go past
/// 20 %" report). The cubic curve that followed went too far the other way:
/// 25 % was -36 dB, and with normalisation taking a few dB more, the bottom
/// quarter was next to silent. The top stays at unity.
///
/// The sink's visualizer tap divides by this same value to stay volume-
/// independent, so both callers MUST use this one function — otherwise the
/// spectrum would react to the wrong amount at low volume.
pub fn volume_amplitude(volume_percent: u16) -> f64 {
    let v = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
    v * v
}

/// Where a volume set under the old cubic curve sounds the same now (50 % ->
/// 35 %), so an update doesn't change how loud anything plays.
pub fn volume_from_cubic(volume_percent: u16) -> u16 {
    if volume_percent == 0 {
        return 0;
    }
    let v = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
    // v^3 = w^2; never down to 0, which would read as muted
    ((v.powf(1.5) * 100.0).round() as u16).max(1)
}

pub struct SpotiampSink {
    backend_delegate: Box<dyn Sink>,
    visualizer: Arc<Mutex<Visualizer>>,
    volume: Arc<AtomicU16>,
    eq_config: Arc<Mutex<EqState>>,
    eq: EqProcessor,
    scratch: Vec<f32>,
}

impl SpotiampSink {
    pub fn new(
        file: Option<String>,
        format: AudioFormat,
        visualizer: Arc<Mutex<Visualizer>>,
        volume: Arc<AtomicU16>,
        eq_config: Arc<Mutex<EqState>>,
    ) -> Self {
        Self {
            backend_delegate: audio_backend::find(None).unwrap()(file, format),
            visualizer,
            volume,
            eq_config,
            eq: EqProcessor::new(),
            scratch: Vec::new(),
        }
    }
}

impl Sink for SpotiampSink {
    fn start(&mut self) -> SinkResult<()> {
        self.backend_delegate.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.backend_delegate.stop()
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        // We own the packet, so for a samples packet we can equalise in place,
        // feed the (post-EQ) audio to the visualizer, then repackage for output.
        let packet = match packet {
            AudioPacket::Samples(mut samples) => {
                {
                    let state = self.eq_config.lock().unwrap();
                    self.eq.process(&state, &mut samples);
                }

                if samples.len() > self.scratch.len() {
                    self.scratch.resize(samples.len().next_power_of_two(), 0.0);
                }
                // Undo the volume attenuation for the visualizer so the bars
                // read the same at any level. Must invert the exact curve the
                // player applied (volume_amplitude), not a plain linear ratio.
                let amplitude = volume_amplitude(self.volume.load(Ordering::Relaxed)) as f32;
                if amplitude > 0.0 {
                    let compensate = 1.0 / amplitude;
                    let mut visualizer = self.visualizer.lock().unwrap();
                    for (idx, s) in samples.iter().enumerate() {
                        self.scratch[idx] = *s as f32 * compensate;
                    }
                    visualizer.push_samples(&self.scratch[..samples.len()]);
                }

                AudioPacket::Samples(samples)
            }
            other => other,
        };

        self.backend_delegate.write(packet, converter)
    }
}

#[cfg(test)]
mod volume_tests {
    use super::{volume_amplitude, volume_from_cubic};

    #[test]
    fn the_curve_is_square_and_unity_at_the_top() {
        assert_eq!(volume_amplitude(0), 0.0);
        assert_eq!(volume_amplitude(50), 0.25);
        assert_eq!(volume_amplitude(100), 1.0);
        assert_eq!(volume_amplitude(250), 1.0);
    }

    #[test]
    fn an_old_cubic_volume_sounds_the_same_after_converting() {
        assert_eq!(volume_from_cubic(0), 0);
        assert_eq!(volume_from_cubic(100), 100);
        assert_eq!(volume_from_cubic(50), 35);
        assert_eq!(volume_from_cubic(1), 1);
        // within about a dB (whole percents can't do better); below 15 %, the
        // old curve's -49 dB and quieter, they can't follow closely at all
        for old in 15..=100u16 {
            let before = (old as f64 / 100.0).powi(3);
            let after = volume_amplitude(volume_from_cubic(old));
            let db = 20.0 * (after / before).log10();
            assert!(db.abs() < 1.2, "{old}% -> {}%: {db:.2} dB", volume_from_cubic(old));
        }
    }
}
