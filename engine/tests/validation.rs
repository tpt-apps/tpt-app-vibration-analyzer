//! End-to-end validation against known synthetic signals: a WAV file is
//! synthesized, encoded, decoded through the real file path, and analyzed —
//! the same pipeline a user's upload takes. Fault detection is pro-gated,
//! so this suite compiles only under `--features pro`.

#![cfg(feature = "pro")]

use tpt_vibration_engine::decode::{self, wav_encode};
use tpt_vibration_engine::faults::{analyze, BearingGeometry, FaultInput, Severity};
use tpt_vibration_engine::synth::{self, Spec, Tone};
use tpt_vibration_engine::spectrum;

const RPM: f64 = 1797.0;

#[test]
fn pure_tone_survives_the_wav_roundtrip() {
    let rec = synth::recording(Spec {
        sample_rate: 8192.0,
        seconds: 1.0,
        tones: vec![Tone { freq: 250.0, amp: 0.7, phase: 0.3 }],
        ..Default::default()
    });
    let bytes = wav_encode::pcm16(&rec).expect("encode");
    let decoded = decode::decode_auto(&bytes, None).expect("decode");
    assert!((decoded.sample_rate - 8192.0).abs() < 1e-9);

    let spec = spectrum::analyze(&decoded, 8192);
    let peaks = spectrum::peaks(&spec, 50.0, 4000.0, 3, 0.05);
    assert!(!peaks.is_empty());
    assert!((peaks[0].freq_hz - 250.0).abs() < 1.5, "peak {}", peaks[0].freq_hz);
    assert!((peaks[0].amp - 0.7).abs() < 0.05, "amp {}", peaks[0].amp);
}

#[test]
fn csv_pipeline_finds_injected_fault_frequency() {
    // Misalignment signature: 1× with 2× at parity, exported as time,value CSV.
    let shaft = RPM / 60.0;
    let rec = synth::recording(Spec {
        sample_rate: 8192.0,
        seconds: 2.0,
        tones: vec![
            Tone { freq: shaft, amp: 1.0, phase: 0.0 },
            Tone { freq: 2.0 * shaft, amp: 1.15, phase: 0.7 },
        ],
        noise: 0.04,
        noise_seed: 29,
        ..Default::default()
    });
    let mut csv = String::from("time_s,accel_g\n");
    for (i, s) in rec.samples.iter().enumerate() {
        // 9 decimals so 1/8192 s steps survive rounding (6 would quantize
        // the inferred rate to ~8197 Hz).
        csv.push_str(&format!("{:.9},{:.6}\n", i as f64 / rec.sample_rate, s));
    }
    let decoded = decode::decode_auto(csv.as_bytes(), None).expect("decode");
    assert!((decoded.sample_rate - 8192.0).abs() < 1.0);

    let report = analyze(&decoded, &FaultInput {
        rpm: RPM,
        bearing: BearingGeometry::default(),
        demod_band: None,
    });
    assert_eq!(report.misalignment.severity, Severity::Alert);
    // With 2× carrying comparable energy to 1×, no single 1× dominance —
    // the imbalance share stays normal while misalignment flags.
    assert_eq!(report.imbalance.severity, Severity::Normal);
}

#[test]
fn full_synthetic_bearing_case_end_to_end() {
    // Outer-race fault riding on noise + shaft, through a WAV roundtrip with
    // the automatic demodulation band.
    let bpfo = BearingGeometry::default().freqs_at(RPM / 60.0).bpfo;
    let rec = synth::recording(Spec {
        sample_rate: 8192.0,
        seconds: 3.0,
        tones: vec![
            Tone { freq: 2100.0, amp: 0.4, phase: 0.0 },
            Tone { freq: RPM / 60.0, amp: 0.2, phase: 0.0 },
            Tone { freq: 900.0, amp: 0.08, phase: 1.0 },
        ],
        am_freq: bpfo,
        am_depth: 0.8,
        noise: 0.06,
        noise_seed: 31,
        ..Default::default()
    });
    let bytes = wav_encode::pcm16(&rec).expect("encode");
    let decoded = decode::decode_auto(&bytes, None).expect("decode");

    let report = analyze(&decoded, &FaultInput {
        rpm: RPM,
        bearing: BearingGeometry::default(),
        demod_band: None,
    });
    let hit = report
        .bearing
        .hits
        .iter()
        .find(|h| h.component == "BPFO")
        .expect("BPFO not found in envelope hits");
    assert!(
        hit.severity != Severity::Normal,
        "BPFO prominence {:.1}",
        hit.prominence
    );
}
