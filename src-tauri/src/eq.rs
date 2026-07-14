//! A functional 10-band graphic equaliser, applied to the decoded audio in the
//! sink. Classic Winamp band centres, ±12 dB per band + preamp.

/// Classic Winamp 10-band centre frequencies (Hz).
pub const EQ_FREQS: [f32; 10] = [
    60.0, 170.0, 310.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0,
];
const SAMPLE_RATE: f32 = 44_100.0;
const Q: f32 = 1.0;

/// Shared EQ configuration (set from the EQ window, read by the sink).
#[derive(Clone)]
pub struct EqState {
    pub enabled: bool,
    /// -12..+12 dB overall pre-gain.
    pub preamp_db: f32,
    /// -12..+12 dB per band.
    pub bands_db: [f32; 10],
    /// Stereo balance, -1.0 (full left) .. 0.0 (centre) .. +1.0 (full right).
    /// Applied independently of `enabled`.
    pub balance: f32,
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            enabled: false,
            preamp_db: 0.0,
            bands_db: [0.0; 10],
            balance: 0.0,
        }
    }
}

/// One peaking-EQ biquad (RBJ cookbook), transposed direct form II.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn peaking(freq: f32, gain_db: f32, q: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / SAMPLE_RATE;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos_w0) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha / a) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Stateful EQ processor: 10 cascaded biquads per channel (stereo).
pub struct EqProcessor {
    filters: [[Biquad; 10]; 2],
    last_gains: [f32; 10],
    preamp_lin: f32,
}

impl EqProcessor {
    pub fn new() -> Self {
        Self {
            filters: [[Biquad::default(); 10]; 2],
            last_gains: [f32::NAN; 10],
            preamp_lin: 1.0,
        }
    }

    /// Recompute coefficients only when the band gains actually change.
    fn refresh(&mut self, state: &EqState) {
        if self.last_gains != state.bands_db {
            for b in 0..10 {
                let c = Biquad::peaking(EQ_FREQS[b], state.bands_db[b], Q);
                for ch in 0..2 {
                    let f = &mut self.filters[ch][b];
                    // keep the running state (z1/z2), swap in new coefficients
                    f.b0 = c.b0;
                    f.b1 = c.b1;
                    f.b2 = c.b2;
                    f.a1 = c.a1;
                    f.a2 = c.a2;
                }
            }
            self.last_gains = state.bands_db;
        }
        self.preamp_lin = 10f32.powf(state.preamp_db / 20.0);
    }

    /// Apply the EQ and stereo balance in place to interleaved stereo f64
    /// samples. Balance is applied even when the EQ itself is disabled.
    pub fn process(&mut self, state: &EqState, samples: &mut [f64]) {
        let apply_eq = state.enabled;
        let bal = state.balance.clamp(-1.0, 1.0);
        let left_gain = if bal > 0.0 { 1.0 - bal } else { 1.0 };
        let right_gain = if bal < 0.0 { 1.0 + bal } else { 1.0 };
        if apply_eq {
            self.refresh(state);
        }
        // nothing to do: EQ off and balance centred
        if !apply_eq && bal == 0.0 {
            return;
        }
        for frame in samples.chunks_mut(2) {
            let channels = frame.len().min(2);
            for ch in 0..channels {
                let mut x = frame[ch] as f32;
                if apply_eq {
                    x *= self.preamp_lin;
                    for b in 0..10 {
                        x = self.filters[ch][b].process(x);
                    }
                }
                if channels >= 2 {
                    x *= if ch == 0 { left_gain } else { right_gain };
                }
                frame[ch] = (x as f64).clamp(-1.0, 1.0);
            }
        }
    }
}
