//! TPT Vibration Engine — signal-processing adapter for the vibration
//! analyzer app.
//!
//! Scope (apps.md §T1-11): raw accelerometer time series in → FFT spectrum
//! and (pro) fault-signature scores out, as plain structs. No UI deps;
//! host-unit-testable; the wasm lib and the native desktop build both call
//! this same crate.
//!
//! Note on `tpt-dsp`: the spec names `tpt-dsp` as the base foundation, but
//! that foundation is still design-only (tpt-foundations/17-tpt-dsp.md — no
//! implementation repo exists yet; `tpt-audio`'s dsp module has no FFT). The
//! FFT/windowing primitives this engine needs are small and self-contained,
//! so the private `fft`/`window` modules below are that swap surface: when
//! `tpt-dsp` lands, its FFT/windowing replaces them behind the same public
//! API.

mod fft;
mod window;

pub mod decode;
pub mod spectrum;
pub mod synth;

#[cfg(feature = "pro")]
pub mod faults;
#[cfg(feature = "pro")]
pub mod report;
