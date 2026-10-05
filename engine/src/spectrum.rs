//! Spectrum computation: Welch-averaged single-sided amplitude spectrum,
//! peak picking, and overall recording statistics.

use crate::decode::Recording;
use crate::fft::{fft_in_place, next_pow2};
use crate::window::{coherent_gain, hann};

/// Single-sided amplitude spectrum of a recording, in the recording's own
/// engineering units (g or m/s² — the analysis is unit-agnostic).
#[derive(Debug, Clone)]
pub struct Spectrum {
    /// Bin center frequencies in Hz, `0..=sample_rate/2`, `bins.len()` entries.
    pub freq_hz: Vec<f64>,
    /// Single-sided amplitude per bin (peak amplitude a sine at that bin
    /// would show), window-corrected.
    pub amp: Vec<f64>,
    pub sample_rate: f64,
    /// Samples per FFT segment (power of two).
    pub fft_size: usize,
    /// Number of Welch segments averaged.
    pub segments: usize,
}

/// A detected spectral peak.
#[derive(Debug, Clone)]
pub struct Peak {
    pub freq_hz: f64,
    pub amp: f64,
}

/// Overall time-domain statistics.
#[derive(Debug, Clone)]
pub struct Stats {
    /// Root-mean-square of the samples.
    pub rms: f64,
    /// Peak absolute sample value.
    pub peak: f64,
    /// `peak / rms` — a rough condition indicator (>~5 suggests impacts).
    pub crest_factor: f64,
    pub samples: usize,
    pub duration_s: f64,
}

pub fn stats(recording: &Recording) -> Stats {
    let n = recording.samples.len();
    let sum_sq: f64 = recording.samples.iter().map(|s| s * s).sum();
    let rms = if n > 0 { (sum_sq / n as f64).sqrt() } else { 0.0 };
    let peak = recording.samples.iter().fold(0.0f64, |a, s| a.max(s.abs()));
    Stats {
        rms,
        peak,
        crest_factor: if rms > 0.0 { peak / rms } else { 0.0 },
        samples: n,
        duration_s: if recording.sample_rate > 0.0 {
            n as f64 / recording.sample_rate
        } else {
            0.0
        },
    }
}

/// Welch-averaged spectrum: the recording is cut into 50%-overlapping
/// segments of `fft_size` samples, windowed with Hann, and the per-segment
/// power is averaged before conversion to single-sided amplitude. Averaging
/// is what keeps spectra readable on real (noisy) sensor data.
///
/// `fft_size` is rounded up to a power of two; recordings shorter than one
/// segment are zero-padded (single segment, marked as such via
/// [`Spectrum::segments`]).
pub fn analyze(recording: &Recording, fft_size: usize) -> Spectrum {
    let size = next_pow2(fft_size.max(16));
    let win = hann(size);
    let gain = coherent_gain(&win);
    let rate = recording.sample_rate.max(1.0);

    // Segment hop: 50% overlap.
    let hop = size / 2;
    // Cap the averaging at 240 segments — enough smoothing for even very
    // long recordings without quadratic rework on huge files. When more
    // segments exist, stride through them evenly (keeping coverage).
    let available = 1 + recording.samples.len().saturating_sub(size) / hop;
    let stride = if available > 240 {
        available.div_ceil(240) * hop
    } else {
        hop
    };

    let mut power = vec![0.0f64; size / 2 + 1];
    let mut used = 0usize;
    let mut start = 0usize;
    while start + size <= recording.samples.len() && used < 240 {
        acc_segment(&recording.samples[start..start + size], &win, gain, &mut power);
        used += 1;
        start += stride;
    }
    if used == 0 {
        // Zero-pad a short recording into one segment.
        let mut padded = recording.samples.clone();
        padded.resize(size, 0.0);
        acc_segment(&padded, &win, gain, &mut power);
        used = 1;
    }

    // Average power → single-sided amplitude. The ×2 recovers the negative
    // frequencies (DC and Nyquist are not doubled — power[0]/power[last]).
    let norm = used as f64;
    let bins = power.len();
    let mut amp: Vec<f64> = power
        .iter()
        .map(|p| (p / norm).sqrt())
        .collect();
    for a in amp.iter_mut().take(bins).skip(1) {
        *a *= 2.0;
    }
    if bins >= 2 {
        amp[0] = (power[0] / norm).sqrt();
        amp[bins - 1] = (power[bins - 1] / norm).sqrt();
    }

    let freq_hz = (0..bins)
        .map(|k| k as f64 * rate / size as f64)
        .collect();

    Spectrum {
        freq_hz,
        amp,
        sample_rate: rate,
        fft_size: size,
        segments: used,
    }
}

/// Windows one segment, FFTs it, and adds its power into `power`.
fn acc_segment(seg: &[f64], win: &[f64], gain: f64, power: &mut [f64]) {
    let n = win.len();
    let mut re = vec![0.0f64; n];
    let mut im = vec![0.0f64; n];
    for i in 0..n {
        re[i] = seg[i] * win[i];
    }
    fft_in_place(&mut re, &mut im, false);
    // Normalize by (N·coherent-gain) so tones read in true amplitude after
    // the single-sided ×2 applied by the caller.
    let norm = n as f64 * gain;
    for k in 0..power.len() {
        let m = (re[k] * re[k] + im[k] * im[k]).sqrt() / norm;
        power[k] += m * m;
    }
}

/// Local-maximum peaks with parabolic interpolation, strongest-first, within
/// a frequency range. `min_prominence` is relative to the max amplitude in
/// the range (0..1] — a plain noise-floor gate.
pub fn peaks(spectrum: &Spectrum, min_hz: f64, max_hz: f64, top_n: usize, min_prominence: f64) -> Vec<Peak> {
    let bins = spectrum.freq_hz.len();
    if bins < 3 {
        return Vec::new();
    }
    let lo = spectrum.freq_hz.partition_point(|&f| f < min_hz).max(1);
    let hi = spectrum
        .freq_hz
        .partition_point(|&f| f <= max_hz)
        .min(bins - 1);
    if hi <= lo + 1 {
        return Vec::new();
    }
    let range_max = spectrum.amp[lo..hi]
        .iter()
        .fold(0.0f64, |a, &v| a.max(v));
    if range_max <= 0.0 {
        return Vec::new();
    }
    let gate = range_max * min_prominence;

    let mut out: Vec<Peak> = Vec::new();
    for k in lo..hi {
        let a = spectrum.amp[k];
        if a >= gate && a > spectrum.amp[k - 1] && a >= spectrum.amp[k + 1] {
            // Parabolic interpolation on log-amplitude for sub-bin accuracy.
            let (al, ac, ar) = (spectrum.amp[k - 1], a, spectrum.amp[k + 1]);
            let denom = al - 2.0 * ac + ar;
            let shift = if denom.abs() > f64::EPSILON {
                0.5 * (al - ar) / denom
            } else {
                0.0
            };
            let bin_hz = spectrum.sample_rate / spectrum.fft_size as f64;
            let freq = spectrum.freq_hz[k] + shift.clamp(-0.5, 0.5) * bin_hz;
            out.push(Peak {
                freq_hz: freq,
                amp: ac,
            });
        }
    }
    out.sort_by(|x, y| y.amp.partial_cmp(&x.amp).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(top_n);
    out
}

/// Average amplitude across `[lo, hi]` Hz — the building block for fault
/// band comparisons.
pub fn band_amplitude(spectrum: &Spectrum, lo_hz: f64, hi_hz: f64) -> f64 {
    let bins = spectrum.freq_hz.len();
    if bins < 2 {
        return 0.0;
    }
    let lo = spectrum.freq_hz.partition_point(|&f| f < lo_hz).max(1);
    let hi = spectrum.freq_hz.partition_point(|&f| f <= hi_hz).min(bins - 1);
    if hi <= lo {
        return 0.0;
    }
    let sum: f64 = spectrum.amp[lo..=hi].iter().sum();
    sum / (hi - lo + 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth;

    #[test]
    fn pure_tone_amplitude_and_frequency() {
        // 50 Hz tone at 1.0 unit amplitude, 4096 Hz, ~1 s.
        let rec = synth::recording(synth::Spec {
            sample_rate: 4096.0,
            seconds: 1.0,
            tones: vec![synth::Tone { freq: 50.0, amp: 1.0, phase: 0.0 }],
            noise: 0.0,
            ..Default::default()
        });
        let spec = analyze(&rec, 4096);
        let ps = peaks(&spec, 10.0, 2000.0, 5, 0.05);
        assert!(!ps.is_empty(), "tone peak not found");
        assert!((ps[0].freq_hz - 50.0).abs() < 1.0, "peak at {} Hz", ps[0].freq_hz);
        assert!(
            (ps[0].amp - 1.0).abs() < 0.05,
            "amplitude {} vs 1.0",
            ps[0].amp
        );
    }

    #[test]
    fn welch_averaging_still_finds_tone_in_noise() {
        // Deterministic noise (fixed seed) at 30% — the tone must dominate.
        let rec = synth::recording(synth::Spec {
            sample_rate: 4096.0,
            seconds: 2.0,
            tones: vec![synth::Tone { freq: 120.0, amp: 1.0, phase: 0.0 }],
            noise: 0.3,
            noise_seed: 7,
            ..Default::default()
        });
        let spec = analyze(&rec, 2048);
        assert!(spec.segments > 1, "expected multiple Welch segments");
        let ps = peaks(&spec, 50.0, 300.0, 3, 0.02);
        assert!(!ps.is_empty());
        assert!((ps[0].freq_hz - 120.0).abs() < 2.0, "peak at {}", ps[0].freq_hz);
    }

    #[test]
    fn short_recording_zero_pads() {
        let rec = synth::recording(synth::Spec {
            sample_rate: 1024.0,
            seconds: 0.1,
            tones: vec![synth::Tone { freq: 100.0, amp: 1.0, phase: 0.0 }],
            noise: 0.0,
            ..Default::default()
        });
        let spec = analyze(&rec, 4096);
        assert_eq!(spec.segments, 1);
        assert_eq!(spec.amp.len(), spec.fft_size / 2 + 1);
    }

    #[test]
    fn band_amplitude_concentrates_where_tones_live() {
        let rec = synth::recording(synth::Spec {
            sample_rate: 1024.0,
            seconds: 1.0,
            tones: vec![
                synth::Tone { freq: 50.0, amp: 1.0, phase: 0.0 },
                synth::Tone { freq: 400.0, amp: 1.0, phase: 0.0 },
            ],
            noise: 0.0,
            ..Default::default()
        });
        let spec = analyze(&rec, 1024);
        // Mean-amplitude semantics: a tone in a wide band lifts the band's
        // average well above a silent band, but doesn't saturate it.
        let low = band_amplitude(&spec, 30.0, 80.0);
        let high = band_amplitude(&spec, 380.0, 420.0);
        let empty = band_amplitude(&spec, 700.0, 720.0);
        assert!(empty < 1e-6, "empty band amplitude {empty}");
        assert!(low > 0.01 && low > empty * 100.0, "50 Hz band amplitude {low} vs empty {empty}");
        assert!(high > 0.01 && high > empty * 100.0, "400 Hz band amplitude {high}");
        // A narrow band around the tone averages much higher than the wide one.
        let narrow = band_amplitude(&spec, 48.0, 52.0);
        assert!(narrow > low * 3.0, "narrow {narrow} should beat wide {low}");
    }

    #[test]
    fn stats_crest_factor() {
        // Pure sine: crest ≈ √2.
        let rec = synth::recording(synth::Spec {
            sample_rate: 1024.0,
            seconds: 1.0,
            tones: vec![synth::Tone { freq: 50.0, amp: 1.0, phase: 0.0 }],
            noise: 0.0,
            ..Default::default()
        });
        let st = stats(&rec);
        assert!((st.rms - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.01, "rms {}", st.rms);
        assert!((st.crest_factor - std::f64::consts::SQRT_2).abs() < 0.05);
        assert!((st.duration_s - 1.0).abs() < 1e-9);
    }
}
