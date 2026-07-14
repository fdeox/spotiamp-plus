use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use librespot::playback::audio_backend::{self, Sink, SinkResult};
use librespot::playback::config::AudioFormat;
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;

use crate::eq::{EqProcessor, EqState};
use crate::visualizer::Visualizer;

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
                let volume = 100.0 / self.volume.load(Ordering::Relaxed) as f32;
                if volume.is_finite() && volume > 0.0 {
                    let mut visualizer = self.visualizer.lock().unwrap();
                    for (idx, s) in samples.iter().enumerate() {
                        self.scratch[idx] = *s as f32 * volume;
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
