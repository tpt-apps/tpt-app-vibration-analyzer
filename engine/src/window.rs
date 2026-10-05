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

/// Coherent gain of a window: mean of its coefficients. Dividing an FFT
/// magnitude by this compensates the window's amplitude reduction so tone
/// amplitudes read in true engineering units.
pub fn coherent_gain(w: &[f64]) -> f64 {
    if w.is_empty() {
        return 1.0;
    }
    w.iter().sum::<f64>() / w.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_endpoints_and_gain() {
        let w = hann(8);
        assert!(w[0].abs() < 1e-12);
        let g = coherent_gain(&w);
        // Hann's coherent gain is 0.5.
        assert!((g - 0.5).abs() < 1e-12);
    }
}
