use audioviz::spectrum::{
    config::{
        Interpolation, PositionNormalisation, ProcessorConfig, StreamConfig, VolumeNormalisation,
    },
    stream::Stream,
};
use librespot::playback::SAMPLE_RATE;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Samples per channel in a waveform frame: what MilkDrop (Butterchurn)
/// reads each frame, like a Web Audio analyser with an fftSize of 1024.
pub const WAVE_LEN: usize = 1024;

/// No new audio for this long (paused, stopped): the waveform reads as
/// silence instead of the last moment frozen.
const WAVE_STALE: Duration = Duration::from_millis(150);

pub struct Visualizer {
    stream: Stream,
    /// The last WAVE_LEN samples of each channel, for the waveform.
    wave_l: VecDeque<f32>,
    wave_r: VecDeque<f32>,
    wave_at: Option<Instant>,
}
pub fn stereo_to_mono(in_v: &[f32]) -> Vec<f32> {
    in_v.chunks_exact(2)
        .map(|chunk| (chunk[0] + chunk[1]) / 2.0)
        .collect()
}

impl Visualizer {
    pub fn new() -> Self {
        Self::with_sample_rate(SAMPLE_RATE)
    }

    /// Same spectrum config as `new()` but at a chosen sample rate — used by the
    /// loopback capture in Free Mode, which runs at the output device's rate
    /// (typically 48 kHz) rather than librespot's 44.1 kHz.
    pub fn with_sample_rate(sampling_rate: u32) -> Self {
        Self {
            wave_l: VecDeque::with_capacity(WAVE_LEN + 1),
            wave_r: VecDeque::with_capacity(WAVE_LEN + 1),
            wave_at: None,
            stream: Stream::new(StreamConfig {
                channel_count: 1,
                processor: ProcessorConfig {
                    sampling_rate,
                    frequency_bounds: [40, 20000],
                    resolution: Some(19),
                    volume: 0.8,
                    volume_normalisation: VolumeNormalisation::Mixture,
                    position_normalisation: PositionNormalisation::Harmonic,
                    manual_position_distribution: None,
                    interpolation: Interpolation::Cubic,
                },
                fft_resolution: 1024 * 2,
                refresh_rate: 60,
                gravity: Some(2.0),
            }),
        }
    }
    pub fn push_samples(&mut self, samples: &[f32]) {
        #[cfg(target_os = "windows")]
        crate::milkdrop::feed_stereo(crate::milkdrop::Source::Player, samples);
        for frame in samples.chunks_exact(2) {
            self.push_wave(frame[0], frame[1]);
        }
        self.stream.push_data(stereo_to_mono(samples));
        self.stream.update();
    }

    /// Push already-mono samples (the loopback path downmixes itself, since the
    /// output device can have any channel count). Its waveform is the same on
    /// both sides.
    pub fn push_mono(&mut self, mono: Vec<f32>) {
        #[cfg(target_os = "windows")]
        crate::milkdrop::feed_mono(crate::milkdrop::Source::Loopback, &mono);
        for &s in &mono {
            self.push_wave(s, s);
        }
        self.stream.push_data(mono);
        self.stream.update();
    }

    fn push_wave(&mut self, l: f32, r: f32) {
        self.wave_l.push_back(l);
        self.wave_r.push_back(r);
        if self.wave_l.len() > WAVE_LEN {
            self.wave_l.pop_front();
            self.wave_r.pop_front();
        }
        self.wave_at = Some(Instant::now());
    }

    /// The latest waveform as bytes, the way a Web Audio analyser gives it
    /// (getByteTimeDomainData: 128 is silence, 0 and 255 full scale): the mono
    /// mix, then left, then right, WAVE_LEN each. Before there's enough audio
    /// it's padded with silence at the start.
    pub fn waveform_bytes(&self) -> Vec<u8> {
        let mut out = vec![128u8; WAVE_LEN * 3];
        if self.wave_at.is_none_or(|at| at.elapsed() > WAVE_STALE) {
            return out;
        }
        let byte = |s: f32| (128.0 + s * 128.0).round().clamp(0.0, 255.0) as u8;
        let pad = WAVE_LEN - self.wave_l.len();
        for (i, (&l, &r)) in self.wave_l.iter().zip(&self.wave_r).enumerate() {
            out[pad + i] = byte((l + r) / 2.0);
            out[WAVE_LEN + pad + i] = byte(l);
            out[2 * WAVE_LEN + pad + i] = byte(r);
        }
        out
    }

    pub fn take_latest_spectrum(&mut self) -> Vec<(f32, f32)> {
        let freqs = self.stream.get_frequencies();
        freqs
            .first()
            .map(|data| data.iter().map(|d| (d.freq, d.volume)).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod wave_tests {
    use super::*;

    #[test]
    fn the_waveform_reads_like_a_web_audio_analyser() {
        let mut v = Visualizer::new();
        // nothing yet: silence
        assert!(v.waveform_bytes().iter().all(|&b| b == 128));

        // a little stereo: padded with silence in front, newest last
        v.push_samples(&[1.0, -1.0, 0.5, 0.0]);
        let w = v.waveform_bytes();
        assert_eq!(w.len(), WAVE_LEN * 3);
        assert_eq!(w[WAVE_LEN - 3], 128);
        assert_eq!(&w[WAVE_LEN - 2..WAVE_LEN], &[128, 160]); // mono: (1-1)/2, (0.5+0)/2
        assert_eq!(&w[2 * WAVE_LEN - 2..2 * WAVE_LEN], &[255, 192]); // left
        assert_eq!(&w[3 * WAVE_LEN - 2..], &[0, 128]); // right

        // only the last WAVE_LEN are kept
        let long: Vec<f32> = (0..WAVE_LEN * 4).map(|i| if i % 2 == 0 { 0.25 } else { -0.25 }).collect();
        v.push_samples(&long);
        let w = v.waveform_bytes();
        assert!(w[WAVE_LEN..2 * WAVE_LEN].iter().all(|&b| b == 160));
        assert!(w[2 * WAVE_LEN..].iter().all(|&b| b == 96));

        // the music stops: back to silence, not the last moment frozen
        v.wave_at = Some(Instant::now() - WAVE_STALE * 2);
        assert!(v.waveform_bytes().iter().all(|&b| b == 128));
    }
}
