//! Minimal iterative radix-2 complex FFT (power-of-two sizes, f64).
//!
//! Swap surface for `tpt-dsp`: when that foundation ships an FFT, replace
//! this module and keep the `fft_in_place` signature.

/// In-place complex FFT. `re`/`im` must be the same length and a power of
/// two (panics otherwise — callers size the buffers via `next_pow2`).
///
/// Forward transform, unnormalized: X[k] = Σ x[n]·e^(−2πikn/N). The inverse
/// is the same loop with a sign flip plus a 1/N scale, which only
/// `hilbert_envelope` (faults) needs.
pub fn fft_in_place(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    assert_eq!(n, im.len(), "re/im length mismatch");
    assert!(n.is_power_of_two(), "radix-2 FFT needs a power-of-two size");
    if n <= 1 {
        return;
    }

    // Bit-reversal permutation.
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = (i.reverse_bits() >> (usize::BITS - bits)) as usize;
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterflies bottom-up. Twiddles are computed per stage from the
    // incremental angle — exact enough for spectral analysis and avoids a
    // precomputed table sized for one N.
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let step = std::f64::consts::TAU * sign / len as f64;
        let half = len / 2;
        let mut base = 0;
        while base < n {
            // w = e^(step·k), advanced by repeated rotation.
            let (mut wr, mut wi) = (1.0f64, 0.0f64);
            let (sin, cos) = step.sin_cos();
            for k in 0..half {
                let i = base + k;
                let j = i + half;
                let (xr, xi) = (re[j], im[j]);
                let (tr, ti) = (xr * wr - xi * wi, xr * wi + xi * wr);
                re[j] = re[i] - tr;
                im[j] = im[i] - ti;
                re[i] += tr;
                im[i] += ti;
                let (nr, ni) = (wr * cos - wi * sin, wr * sin + wi * cos);
                wr = nr;
                wi = ni;
            }
            base += len;
        }
        len *= 2;
    }

    if inverse {
        let scale = 1.0 / n as f64;
        for v in re.iter_mut().chain(im.iter_mut()) {
            *v *= scale;
        }
    }
}

/// Smallest power of two ≥ `n` (minimum 1).
pub fn next_pow2(n: usize) -> usize {
    n.next_power_of_two().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Naive DFT reference.
    fn dft(re: &[f64], im: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let n = re.len();
        let mut or = vec![0.0; n];
        let mut oi = vec![0.0; n];
        for k in 0..n {
            for t in 0..n {
                let a = -std::f64::consts::TAU * (k * t % n) as f64 / n as f64;
                let (s, c) = a.sin_cos();
                or[k] += re[t] * c - im[t] * s;
                oi[k] += re[t] * s + im[t] * c;
            }
        }
        (or, oi)
    }

    #[test]
    fn matches_naive_dft() {
        let n = 64;
        let re: Vec<f64> = (0..n).map(|i| (i as f64 * 0.37).sin() + 0.2 * (i as f64 * 2.1).cos()).collect();
        let im: Vec<f64> = (0..n).map(|i| (i as f64 * 0.11).cos()).collect();
        let (mut fr, mut fi) = (re.clone(), im.clone());
        fft_in_place(&mut fr, &mut fi, false);
        let (er, ei) = dft(&re, &im);
        for k in 0..n {
            assert!((fr[k] - er[k]).abs() < 1e-9, "re bin {k}");
            assert!((fi[k] - ei[k]).abs() < 1e-9, "im bin {k}");
        }
    }

    #[test]
    fn inverse_round_trips() {
        let n = 128;
        let re: Vec<f64> = (0..n).map(|i| ((i * 7 % n) as f64).sin()).collect();
        let im: Vec<f64> = vec![0.0; n];
        let (mut fr, mut fi) = (re.clone(), im.clone());
        fft_in_place(&mut fr, &mut fi, false);
        fft_in_place(&mut fr, &mut fi, true);
        for i in 0..n {
            assert!((fr[i] - re[i]).abs() < 1e-12);
            assert!((fi[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn pure_tone_lands_in_expected_bin() {
        // 16 Hz tone sampled at 256 Hz, N=256 → bin 16 (and its mirror at
        // N−16; real spectra only ever look at the first half).
        let n = 256;
        let mut re: Vec<f64> = (0..n)
            .map(|i| (std::f64::consts::TAU * 16.0 * i as f64 / 256.0).sin())
            .collect();
        let mut im = vec![0.0; n];
        fft_in_place(&mut re, &mut im, false);
        let mag: Vec<f64> = (0..n).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        let peak = (0..=n / 2).fold(0usize, |a, k| if mag[k] > mag[a] { k } else { a });
        assert_eq!(peak, 16);
    }

    #[test]
    fn next_pow2_basics() {
        assert_eq!(next_pow2(1), 1);
        assert_eq!(next_pow2(1000), 1024);
        assert_eq!(next_pow2(4096), 4096);
    }
}
