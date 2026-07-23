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
/// A cubic curve, not the straight `vol/100` it used to be: loudness is
/// perceived roughly logarithmically, so a linear multiplier makes the bottom
/// fifth of the slider cover almost the whole *audible* range and everything
/// above it barely change — which is exactly the "I can't go past 20 %" report.
/// Cubic is one of librespot's own volume curves; it maps a comfortable low
/// listening level to around the middle of the slider and leaves the top at
/// unity, so nothing gets quieter than before at max.
///
/// The sink's visualizer tap divides by this same value to stay volume-
/// independent, so both callers MUST use this one function — otherwise the
/// spectrum would react to the wrong amount at low volume.
pub fn volume_amplitude(volume_percent: u16) -> f64 {
    let v = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
    v * v * v
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
