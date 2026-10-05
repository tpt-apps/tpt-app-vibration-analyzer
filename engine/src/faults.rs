//! Fault-signature detection (pro): bearing wear via envelope analysis,
//! imbalance and misalignment via shaft-order amplitudes.
//!
//! All scores are relative (ratios/dB against the recording's own floor), so
//! they work in g or m/s² and don't pretend to ISO machine classes — the
//! report says what dominates the spectrum and how strong each signature is
//! relative to this machine's own baseline. Severity cut-offs are the
//! documented heuristics below, meant for trend *changes* more than
//! absolute verdicts.

use crate::decode::Recording;
use crate::fft::fft_in_place;
use crate::spectrum::{self, Spectrum};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Watch,
    Alert,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Normal => "normal",
            Severity::Watch => "watch",
            Severity::Alert => "alert",
        }
    }
}

/// One scored fault signature.
#[derive(Debug, Clone)]
pub struct FaultScore {
    pub severity: Severity,
    /// The scored quantity (see each detector's doc for units).
    pub value: f64,
    /// Human-readable one-liner for the UI/report.
    pub detail: String,
}

/// Rolling-element bearing geometry (the inputs of the standard
/// characteristic-frequency formulas).
#[derive(Debug, Clone)]
pub struct BearingGeometry {
    /// Number of rolling elements.
    pub balls: u32,
    /// Ball diameter in mm.
    pub ball_dia_mm: f64,
    /// Pitch (cage) diameter in mm.
    pub pitch_dia_mm: f64,
    /// Contact angle in degrees (0 for radial ball bearings).
    pub contact_angle_deg: f64,
}

impl Default for BearingGeometry {
    /// A 6205-style deep-groove ball bearing — the common demo/test bearing,
    /// and a sensible default for "I don't know my bearing's numbers".
    fn default() -> Self {
        BearingGeometry {
            balls: 9,
            ball_dia_mm: 7.94,
            pitch_dia_mm: 39.04,
            contact_angle_deg: 0.0,
        }
    }
}

/// Bearing characteristic frequencies in Hz, all **absolute** (i.e. already
/// referenced to the shaft speed).
#[derive(Debug, Clone, Copy)]
pub struct BearingFreqs {
    /// Fundamental train (cage) frequency.
    pub ftf: f64,
    /// Ball-pass frequency, outer race.
    pub bpfo: f64,
    /// Ball-pass frequency, inner race.
    pub bpfi: f64,
    /// Ball-spin frequency.
    pub bsf: f64,
}

impl BearingGeometry {
    /// Characteristic frequencies at `shaft_hz` shaft rotation frequency.
    /// Standard formulas, e.g. for BPFO:
    /// `n/2 · fs · (1 − d/D · cos φ)`.
    pub fn freqs_at(&self, shaft_hz: f64) -> BearingFreqs {
        let phi = self.contact_angle_deg.to_radians();
        let ratio = (self.ball_dia_mm / self.pitch_dia_mm) * phi.cos();
        let n = self.balls as f64;
        BearingFreqs {
            ftf: shaft_hz * 0.5 * (1.0 - ratio),
            bpfo: shaft_hz * n * 0.5 * (1.0 - ratio),
            bpfi: shaft_hz * n * 0.5 * (1.0 + ratio),
            bsf: shaft_hz * (self.pitch_dia_mm / (2.0 * self.ball_dia_mm))
                * (1.0 - ratio * ratio),
        }
    }
}

/// One bearing-signature hit in the envelope spectrum.
#[derive(Debug, Clone)]
pub struct BearingHit {
    /// Which characteristic frequency matched ("FTF"/"BPFO"/"BPFI"/"BSF"/"2×BSF").
    pub component: &'static str,
    pub freq_hz: f64,
    /// Envelope-spectrum amplitude at the hit.
    pub envelope_amp: f64,
    /// Peak-to-local-floor prominence — the actual scored quantity.
    pub prominence: f64,
    pub severity: Severity,
}

#[derive(Debug, Clone)]
pub struct BearingReport {
    pub freqs: BearingFreqs,
    /// Demodulation band used (Hz), for the UI to show/reproduce.
    pub band: (f64, f64),
    /// Envelope spectrum (for charting).
    pub envelope: Spectrum,
    pub hits: Vec<BearingHit>,
    pub score: FaultScore,
}

/// Complete pro analysis of one recording.
#[derive(Debug, Clone)]
pub struct FaultReport {
    pub shaft_hz: f64,
    pub imbalance: FaultScore,
    pub misalignment: FaultScore,
    pub bearing: BearingReport,
}

/// Analysis inputs beyond the recording itself.
#[derive(Debug, Clone)]
pub struct FaultInput {
    /// Machine shaft speed in RPM (1× reference). Required.
    pub rpm: f64,
    pub bearing: BearingGeometry,
    /// Demodulation band for envelope analysis. `None` = auto-pick the
    /// strongest octave band above the shaft harmonics.
    pub demod_band: Option<(f64, f64)>,
}

/// Scores all three fault families on one recording.
pub fn analyze(recording: &Recording, input: &FaultInput) -> FaultReport {
    let spec = spectrum::analyze(recording, 8192);
    let shaft_hz = (input.rpm.max(0.0) / 60.0).max(0.0);
    let stats = spectrum::stats(recording);

    let imbalance = score_imbalance(&spec, shaft_hz, &stats);
    let misalignment = score_misalignment(&spec, shaft_hz);
    let bearing = score_bearing(recording, &spec, shaft_hz, &input.bearing, input.demod_band);

    FaultReport {
        shaft_hz,
        imbalance,
        misalignment,
        bearing,
    }
}

/// Amplitude of the spectrum at `freq` (nearest bin, ±1-bin tolerance is
/// handled by `spectrum::peaks` elsewhere; here we want a fixed lookup).
fn amp_at(spec: &Spectrum, freq: f64) -> f64 {
    if spec.freq_hz.is_empty() || freq <= 0.0 {
        return 0.0;
    }
    let bin_w = spec.sample_rate / spec.fft_size as f64;
    let k = ((freq / bin_w).round() as usize).min(spec.amp.len() - 1);
    spec.amp[k]
}

/// **Imbalance** — share of the recording's total vibration energy sitting
/// on the 1× shaft frequency: `(amp(1×)/√2)² / RMS²`. A pure 1× sine scores
/// 1.0; a healthy machine with several comparable components sits around
/// 0.2–0.4. Cut-offs: watch when 1× carries ≥ half the energy, alert at
/// ≥ 0.75 (the spectrum is essentially the shaft frequency alone).
fn score_imbalance(spec: &Spectrum, shaft_hz: f64, stats: &spectrum::Stats) -> FaultScore {
    if shaft_hz <= 0.0 || stats.rms <= 0.0 {
        return FaultScore {
            severity: Severity::Normal,
            value: 0.0,
            detail: "no shaft speed entered — imbalance needs the machine RPM".to_string(),
        };
    }
    let one_x = amp_at(spec, shaft_hz);
    let value = (one_x * one_x) / (2.0 * stats.rms * stats.rms);
    let severity = if value >= 0.75 {
        Severity::Alert
    } else if value >= 0.5 {
        Severity::Watch
    } else {
        Severity::Normal
    };
    FaultScore {
        severity,
        value,
        detail: format!(
            "1× amplitude {:.3} carries {:.0}% of total vibration energy ({})",
            one_x,
            value * 100.0,
            severity.label()
        ),
    }
}

/// **Misalignment** — 2× and 3× shaft amplitudes relative to 1×. Parallel
/// misalignment shows a strong 2×; angular shows 1×+2× together. Cut-offs:
/// watch when 2× (or half-weighted 3×) reaches half of 1×, alert at parity.
fn score_misalignment(spec: &Spectrum, shaft_hz: f64) -> FaultScore {
    if shaft_hz <= 0.0 {
        return FaultScore {
            severity: Severity::Normal,
            value: 0.0,
            detail: "no shaft speed entered — misalignment needs the machine RPM".to_string(),
        };
    }
    let one_x = amp_at(spec, shaft_hz);
    if one_x <= 0.0 {
        return FaultScore {
            severity: Severity::Normal,
            value: 0.0,
            detail: "no 1× component found".to_string(),
        };
    }
    let two_x = amp_at(spec, 2.0 * shaft_hz);
    let three_x = amp_at(spec, 3.0 * shaft_hz);
    let value = (two_x / one_x).max(0.5 * three_x / one_x);
    let severity = if value >= 1.0 {
        Severity::Alert
    } else if value >= 0.5 {
        Severity::Watch
    } else {
        Severity::Normal
    };
    FaultScore {
        severity,
        value,
        detail: format!(
            "2× = {:.2}×, 3× = {:.2}× (relative to 1×) — harmonic ratio {:.2} ({})",
            two_x / one_x,
            three_x / one_x,
            value,
            severity.label()
        ),
    }
}

/// **Bearing wear** — band-pass around the structural resonance, amplitude-
/// demodulate (Hilbert envelope), then look for the characteristic
/// frequencies (and 2× BSF) as prominent envelope-spectrum peaks. A healthy
/// bearing's envelope spectrum is floor; a defective one spikes at BPFO/
/// BPFI/BSF/FTF depending on which race is damaged.
///
/// Prominence cut-offs: watch ≥ 8× the local envelope floor, alert ≥ 20×.
fn score_bearing(
    recording: &Recording,
    spec: &Spectrum,
    shaft_hz: f64,
    geometry: &BearingGeometry,
    demod_band: Option<(f64, f64)>,
) -> BearingReport {
    let freqs = geometry.freqs_at(shaft_hz);
    let band = demod_band.unwrap_or_else(|| auto_demod_band(spec, shaft_hz));
    let envelope = envelope_spectrum(recording, band);
    let mut hits = Vec::new();

    if shaft_hz > 0.0 {
        let candidates: [(&'static str, f64); 5] = [
            ("FTF", freqs.ftf),
            ("BPFO", freqs.bpfo),
            ("BPFI", freqs.bpfi),
            ("BSF", freqs.bsf),
            ("2×BSF", 2.0 * freqs.bsf),
        ];
        for (name, cf) in candidates {
            if cf < 2.0 || cf >= envelope.sample_rate / 2.0 {
                continue;
            }
            if let Some((amp, prominence)) = envelope_prominence(&envelope, cf) {
                let severity = if prominence >= 20.0 {
                    Severity::Alert
                } else if prominence >= 8.0 {
                    Severity::Watch
                } else {
                    Severity::Normal
                };
                hits.push(BearingHit {
                    component: name,
                    freq_hz: cf,
                    envelope_amp: amp,
                    prominence,
                    severity,
                });
            }
        }
        // Strongest hit drives the headline score.
        hits.sort_by(|a, b| {
            b.prominence
                .partial_cmp(&a.prominence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let score = match hits.first() {
        Some(top) if top.severity != Severity::Normal => FaultScore {
            severity: top.severity,
            value: top.prominence,
            detail: format!(
                "{} envelope peak at {:.0} Hz — {:.0}× local floor ({})",
                top.component,
                top.freq_hz,
                top.prominence,
                top.severity.label()
            ),
        },
        _ => FaultScore {
            severity: Severity::Normal,
            value: hits.first().map(|h| h.prominence).unwrap_or(0.0),
            detail: "no prominent bearing tones in the envelope spectrum".to_string(),
        },
    };

    BearingReport {
        freqs,
        band,
        envelope,
        hits,
        score,
    }
}

/// Auto demodulation band: the strongest octave band starting at ≥ 5× shaft
/// (keeping well clear of the shaft harmonics, which would leak into the
/// envelope as false tones), capped below Nyquist.
fn auto_demod_band(spec: &Spectrum, shaft_hz: f64) -> (f64, f64) {
    let nyquist = spec.sample_rate / 2.0;
    let lo_min = (5.0 * shaft_hz).max(100.0);
    let mut best = (lo_min, (lo_min * 2.0).min(nyquist * 0.95));
    let mut best_amp = -1.0;
    let mut lo = lo_min;
    while lo < nyquist * 0.9 {
        let hi = (lo * 2.0).min(nyquist * 0.95);
        if hi <= lo {
            break;
        }
        let amp = spectrum::band_amplitude(spec, lo, hi);
        if amp > best_amp {
            best_amp = amp;
            best = (lo, hi);
        }
        lo *= 2.0;
    }
    best
}

/// Magnitude of the analytic signal (Hilbert transform via FFT: zero the
/// negative frequencies, double the positive ones, invert, take |·|), then
/// remove its DC trend — the envelope the bearing tones should sit on.
fn envelope_signal(recording: &Recording, band: (f64, f64)) -> Vec<f64> {
    // FFT the whole recording in one power-of-two block (zero-padding the
    // tail is fine for envelope work; the analysis tones are far from the
    // padding edge).
    let n = crate::fft::next_pow2(recording.samples.len().max(16));
    let rate = recording.sample_rate.max(1.0);
    let mut re = vec![0.0f64; n];
    let mut im = vec![0.0f64; n];
    re[..recording.samples.len()].copy_from_slice(&recording.samples);
    fft_in_place(&mut re, &mut im, false);

    // Band-pass in the frequency domain (brick wall — fine for demodulation;
    // the envelope's shape, not the carrier's phase, is the signal).
    let bin_w = rate / n as f64;
    let lo_bin = ((band.0 / bin_w).ceil() as usize).min(n / 2);
    let hi_bin = ((band.1 / bin_w).floor() as usize).min(n / 2);
    for k in 0..n {
        let keep = k >= lo_bin && k <= hi_bin;
        let mirror = k > 0 && n - k >= lo_bin && n - k <= hi_bin;
        if !keep && !mirror {
            re[k] = 0.0;
            im[k] = 0.0;
        }
    }
    // Analytic signal: keep only positive frequencies (times 2), zero the rest.
    for k in 0..n {
        let positive = k >= lo_bin && k <= hi_bin;
        let negative = k > 0 && n - k >= lo_bin && n - k <= hi_bin;
        if negative {
            re[k] = 0.0;
            im[k] = 0.0;
        } else if positive {
            re[k] *= 2.0;
            im[k] *= 2.0;
        } else {
            re[k] = 0.0;
            im[k] = 0.0;
        }
    }
    fft_in_place(&mut re, &mut im, true);

    let mut out: Vec<f64> = (0..recording.samples.len())
        .map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt())
        .collect();
    // Detrend (remove mean) so the envelope spectrum isn't dominated by DC.
    let mean = out.iter().sum::<f64>() / out.len().max(1) as f64;
    for v in out.iter_mut() {
        *v -= mean;
    }
    out
}

/// Welch spectrum of the envelope signal.
fn envelope_spectrum(recording: &Recording, band: (f64, f64)) -> Spectrum {
    let enveloped = Recording {
        sample_rate: recording.sample_rate,
        samples: envelope_signal(recording, band),
        notes: Vec::new(),
    };
    spectrum::analyze(&enveloped, 4096)
}

/// Prominence of the envelope-spectrum peak at `cf` against its shoulders:
/// the nearest local minimum on each side (searched within ±25% of `cf`),
/// with the *higher* shoulder as the floor. Shoulder-based prominence is
/// immune to smoothly-decaying envelope spectra (where a wide median floor
/// under-reads and flags noise) — a real defect tone rises from its own
/// floor; a chance noise spike does not. No special-casing of the shaft
/// frequency is needed: the walk stops at the first climb either way, and
/// the shaft peak (if reached) reads as a climb.
fn envelope_prominence(env: &Spectrum, cf: f64) -> Option<(f64, f64)> {
    let bin_w = env.sample_rate / env.fft_size as f64;
    let k = ((cf / bin_w).round() as usize).min(env.amp.len() - 1);
    if k < 2 || k + 2 >= env.amp.len() {
        return None;
    }
    let a = env.amp[k];
    if !(a > env.amp[k - 1] && a > env.amp[k - 2] && a >= env.amp[k + 1] && a >= env.amp[k + 2]) {
        return None;
    }

    // Walk each way from the peak (past its own flank) until the amplitude
    // starts climbing — the bin before the climb is the shoulder minimum.
    // Capped at ±25% of cf; if still descending at the cap, the cap value
    // (the lowest seen) is the shoulder.
    let w = ((cf * 0.25) / bin_w).round() as usize;
    let walk = |dir: isize| -> f64 {
        let mut lowest = a;
        let mut prev = a;
        let mut i = k as isize;
        for _ in 0..w {
            i += dir;
            if i < 0 || i as usize >= env.amp.len() {
                break;
            }
            let v = env.amp[i as usize];
            if v > prev {
                break;
            }
            prev = v;
            lowest = lowest.min(v);
        }
        lowest
    };
    let floor = walk(-1).max(walk(1)).max(1e-12);
    Some((a, a / floor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{self, Spec, Tone};

    fn input(rpm: f64) -> FaultInput {
        FaultInput {
            rpm,
            bearing: BearingGeometry::default(),
            demod_band: None,
        }
    }

    /// 6205 bearing at 1797 rpm (the classic Case Western test rig speed):
    /// shaft 29.95 Hz, BPFO ≈ 107.4 Hz, BPFI ≈ 162.2 Hz, BSF ≈ 141.2 Hz.
    const RPM: f64 = 1797.0;

    #[test]
    fn bearing_formulas_match_reference() {
        let f = BearingGeometry::default().freqs_at(RPM / 60.0);
        assert!((f.bpfo - 107.36).abs() < 0.2, "bpfo {}", f.bpfo);
        assert!((f.bpfi - 162.18).abs() < 0.2, "bpfi {}", f.bpfi);
        // Standard ball-spin formula (D/2d)·fs·(1−(d/D·cosφ)²) — 2.357× shaft.
        // (Some references quote 2× this value; the detector's "2×BSF"
        // candidate covers that convention.)
        assert!((f.bsf - 70.58).abs() < 0.3, "bsf {}", f.bsf);
        assert!((f.ftf - 11.93).abs() < 0.2, "ftf {}", f.ftf);
    }

    #[test]
    fn imbalance_detected_misalignment_not() {
        let rec = synth::recording(Spec {
            sample_rate: 8192.0,
            seconds: 2.0,
            tones: vec![
                Tone { freq: RPM / 60.0, amp: 1.0, phase: 0.0 },
                Tone { freq: 1200.0, amp: 0.05, phase: 0.0 },
            ],
            noise: 0.05,
            noise_seed: 11,
            ..Default::default()
        });
        let report = analyze(&rec, &input(RPM));
        assert_eq!(report.imbalance.severity, Severity::Alert, "imbalance: {report:?}");
        assert_eq!(
            report.misalignment.severity,
            Severity::Normal,
            "misalignment should stay quiet: {}",
            report.misalignment.detail
        );
        assert_eq!(
            report.bearing.score.severity,
            Severity::Normal,
            "no bearing tones present: {}",
            report.bearing.score.detail
        );
    }

    #[test]
    fn misalignment_detected() {
        let shaft = RPM / 60.0;
        // 2× at 0.9 of 1× — clearly watch territory, not yet "2× ≥ 1×".
        let rec = synth::recording(Spec {
            sample_rate: 8192.0,
            seconds: 2.0,
            tones: vec![
                Tone { freq: shaft, amp: 1.0, phase: 0.0 },
                Tone { freq: 2.0 * shaft, amp: 0.9, phase: 1.0 },
                Tone { freq: 3.0 * shaft, amp: 0.4, phase: 0.5 },
            ],
            noise: 0.03,
            noise_seed: 13,
            ..Default::default()
        });
        let report = analyze(&rec, &input(RPM));
        assert_eq!(report.misalignment.severity, Severity::Watch, "{}", report.misalignment.detail);
        // And with 2× at parity the score escalates to alert.
        let rec = synth::recording(Spec {
            sample_rate: 8192.0,
            seconds: 2.0,
            tones: vec![
                Tone { freq: shaft, amp: 1.0, phase: 0.0 },
                Tone { freq: 2.0 * shaft, amp: 1.1, phase: 1.0 },
            ],
            noise: 0.03,
            noise_seed: 13,
            ..Default::default()
        });
        let report = analyze(&rec, &input(RPM));
        assert_eq!(report.misalignment.severity, Severity::Alert, "{}", report.misalignment.detail);
    }

    #[test]
    fn bearing_bpfo_detected_in_envelope() {
        // Outer-race defect: resonance carrier (auto-picked band) amplitude-
        // modulated at BPFO ≈ 107.4 Hz.
        let bpfo = BearingGeometry::default().freqs_at(RPM / 60.0).bpfo;
        let rec = synth::recording(Spec {
            sample_rate: 8192.0,
            seconds: 2.0,
            tones: vec![
                Tone { freq: 1900.0, amp: 0.5, phase: 0.0 }, // resonance carrier
                Tone { freq: RPM / 60.0, amp: 0.15, phase: 0.0 }, // some shaft
            ],
            am_freq: bpfo,
            am_depth: 0.9,
            noise: 0.04,
            noise_seed: 17,
            ..Default::default()
        });
        let report = analyze(&rec, &input(RPM));
        let bpfo_hit = report
            .bearing
            .hits
            .iter()
            .find(|h| h.component == "BPFO")
            .expect("BPFO hit missing");
        assert!(
            bpfo_hit.severity != Severity::Normal,
            "BPFO prominence {:.1} — {}",
            bpfo_hit.prominence,
            report.bearing.score.detail
        );
    }

    #[test]
    fn healthy_recording_scores_normal() {
        // A plausible healthy machine: several comparable components spread
        // across the spectrum plus broadband noise — no single signature
        // dominating the energy.
        let shaft = RPM / 60.0;
        let rec = synth::recording(Spec {
            sample_rate: 8192.0,
            seconds: 2.0,
            tones: vec![
                Tone { freq: shaft, amp: 0.2, phase: 0.0 },
                Tone { freq: 2.0 * shaft, amp: 0.08, phase: 0.3 },
                Tone { freq: 120.0, amp: 0.15, phase: 0.0 },
                Tone { freq: 400.0, amp: 0.1, phase: 0.0 },
                Tone { freq: 1900.0, amp: 0.08, phase: 0.0 },
            ],
            noise: 0.15,
            noise_seed: 23,
            ..Default::default()
        });
        let report = analyze(&rec, &input(RPM));
        assert!(
            report.imbalance.severity == Severity::Normal,
            "imbalance: {}",
            report.imbalance.detail
        );
        assert!(
            report.misalignment.severity == Severity::Normal,
            "misalignment: {}",
            report.misalignment.detail
        );
        assert_eq!(
            report.bearing.score.severity,
            Severity::Normal,
            "healthy should not flag bearing: {}",
            report.bearing.score.detail
        );
    }

    #[test]
    fn zero_rpm_scores_gracefully() {
        let rec = synth::recording(Spec {
            sample_rate: 8192.0,
            seconds: 0.5,
            noise: 0.05,
            noise_seed: 5,
            ..Default::default()
        });
        let report = analyze(&rec, &input(0.0));
        assert_eq!(report.imbalance.severity, Severity::Normal);
        assert_eq!(report.misalignment.severity, Severity::Normal);
        assert!(report.bearing.hits.is_empty());
    }
}
