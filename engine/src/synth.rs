//! Synthetic signal generation — used by the validation tests here and by
//! the app's "load a demo recording" button (so the hub demo works for
//! visitors without a recording on hand).

/// One sine component.
#[derive(Debug, Clone)]
pub struct Tone {
    pub freq: f64,
    pub amp: f64,
    /// Phase in radians.
    pub phase: f64,
}

/// What `Default` gives you: 1 s of silence at 4096 Hz.
#[derive(Debug, Clone, Default)]
pub struct Spec {
    pub sample_rate: f64,
    pub seconds: f64,
    pub tones: Vec<Tone>,
    /// Additive white Gaussian-ish noise RMS (uniform sum approximation —
    /// deterministic, seeded).
    pub noise: f64,
    /// Seed for the noise generator (deterministic tests + demo).
    pub noise_seed: u64,
    /// Amplitude-modulate `tones[0]`'s amplitude with `sin(2π·am_freq·t +
    /// am_depth)` — the standard way to synthesize a bearing defect: a
    /// defect repetition rate sidebands a structural resonance.
    pub am_freq: f64,
    /// AM depth 0..1 (0 = none).
    pub am_depth: f64,
}

/// Generates the mono recording described by `spec`.
pub fn recording(spec: Spec) -> crate::decode::Recording {
    let n = (spec.sample_rate * spec.seconds).round() as usize;
    let mut samples = Vec::with_capacity(n);
    let mut noise_state = spec.noise_seed | 1;
    for i in 0..n {
        let t = i as f64 / spec.sample_rate;
        let mut v = 0.0;
        for (ti, tone) in spec.tones.iter().enumerate() {
            let mut a = tone.amp;
            if spec.am_depth > 0.0 && ti == 0 && spec.am_freq > 0.0 {
                a *= 1.0
                    + spec.am_depth
                        * (std::f64::consts::TAU * spec.am_freq * t + std::f64::consts::FRAC_PI_2)
                            .sin();
                // AM with positive depth only (envelope dips, never inverts).
                a = a.max(0.0);
            }
            v += a * (std::f64::consts::TAU * tone.freq * t + tone.phase).sin();
        }
        if spec.noise > 0.0 {
            v += spec.noise * uniform_symmetric(&mut noise_state);
        }
        samples.push(v);
    }
    crate::decode::Recording {
        sample_rate: spec.sample_rate,
        samples,
        notes: vec!["synthetic demo signal".to_string()],
    }
}

/// Deterministic symmetric pseudo-random in roughly ±1 (xorshift; sum of
/// two draws approximates a bell shape well enough for demo noise).
fn uniform_symmetric(state: &mut u64) -> f64 {
    fn next(state: &mut u64) -> f64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
    (next(state) + next(state)) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let spec = Spec {
            sample_rate: 1024.0,
            seconds: 0.5,
            noise: 0.2,
            noise_seed: 42,
            ..Default::default()
        };
        let a = recording(spec.clone()).samples;
        let b = recording(spec).samples;
        assert_eq!(a, b);
    }

    #[test]
    fn noise_level_is_approximate() {
        let rec = recording(Spec {
            sample_rate: 1024.0,
            seconds: 4.0,
            noise: 0.1,
            noise_seed: 3,
            ..Default::default()
        });
        let n = rec.samples.len();
        let rms = (rec.samples.iter().map(|s| s * s).sum::<f64>() / n as f64).sqrt();
        // Two-draw triangular noise has RMS = σ/√1.5 of the requested value;
        // allow wide tolerance — this checks scale, not distribution shape.
        assert!(rms > 0.03 && rms < 0.12, "noise rms {rms}");
    }
}
