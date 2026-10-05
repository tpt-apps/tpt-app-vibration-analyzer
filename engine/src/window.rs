//! Analysis windows and their coherent-gain corrections.
//!
//! Swap surface for `tpt-dsp`: when that foundation ships windowing, replace
//! this module and keep the public functions' signatures.

/// Hann window (periodic form, the convention for spectral analysis).
pub fn hann(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let two_pi = std::f64::consts::TAU;
            0.5 * (1.0 - (two_pi * i as f64 / n as f64).cos())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_endpoints_and_gain() {
        let w = hann(8);
        assert!(w[0].abs() < 1e-12);
        // Hann's coherent gain (mean coefficient) is 0.5 — the value the
        // spectrum normalization divides by (via the window-coefficient sum).
        let g = w.iter().sum::<f64>() / w.len() as f64;
        assert!((g - 0.5).abs() < 1e-12);
    }
}
