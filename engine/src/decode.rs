//! Recording decoders: WAV (PCM/float) and CSV, both tolerant of what
//! standard USB-accelerometer capture tools export.

use std::f64;

/// A decoded mono recording. Multi-channel inputs are averaged to mono;
/// vibration fault analysis only needs magnitude.
#[derive(Debug, Clone)]
pub struct Recording {
    /// Sample rate in Hz.
    pub sample_rate: f64,
    /// Per-sample acceleration in the file's units (g or m/s² — the
    /// analysis is unit-agnostic; ratio- and dB-based scores don't care).
    pub samples: Vec<f64>,
    /// Human-facing note about how the file was interpreted (channels
    /// averaged, sample rate inferred, …) — surfaced in the UI.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum DecodeError {
    /// Not a WAV file (bad RIFF/WAVE magic).
    NotWav,
    /// A RIFF file, but no usable `fmt `/`data` chunk pair.
    BadWav(String),
    /// Unsupported sample encoding in a WAV container.
    UnsupportedWavFormat(String),
    /// No numeric rows could be extracted from the CSV.
    BadCsv(String),
    /// Truncated file: a chunk length runs past the end of the buffer.
    Truncated,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::NotWav => write!(f, "not a WAV file"),
            DecodeError::BadWav(why) => write!(f, "unreadable WAV file: {why}"),
            DecodeError::UnsupportedWavFormat(why) => {
                write!(f, "unsupported WAV encoding: {why}")
            }
            DecodeError::BadCsv(why) => write!(f, "unreadable CSV file: {why}"),
            DecodeError::Truncated => write!(f, "file ends mid-chunk"),
        }
    }
}

// ---------------------------------------------------------------- WAV ----

/// Decodes a WAV file's first `data` chunk: 16/24/32-bit integer PCM and
/// 32/64-bit float, mono or multi-channel (channels averaged). Accepts both
/// plain `WAVE_FORMAT_PCM`/`IEEE_FLOAT` headers and
/// `WAVE_FORMAT_EXTENSIBLE` ones that declare the same subformats.
pub fn decode_wav(bytes: &[u8]) -> Result<Recording, DecodeError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(DecodeError::NotWav);
    }

    let mut pos = 12usize;
    let mut format_tag: u16 = 0;
    let mut channels: u16 = 0;
    let mut sample_rate: u32 = 0;
    let mut bits: u16 = 0;
    let mut data: Option<&[u8]> = None;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body_start = pos + 8;
        let body_end = body_start
            .checked_add(len)
            .filter(|&e| e <= bytes.len())
            .ok_or(DecodeError::Truncated)?;
        let body = &bytes[body_start..body_end];

        match id {
            b"fmt " => {
                if body.len() < 16 {
                    return Err(DecodeError::BadWav("fmt chunk shorter than 16 bytes".into()));
                }
                format_tag = u16::from_le_bytes([body[0], body[1]]);
                channels = u16::from_le_bytes([body[2], body[3]]);
                sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
                // WAVE_FORMAT_EXTENSIBLE: the real format code is in the
                // SubFormat GUID's first two bytes (fmt cbSize ≥ 22).
                if format_tag == 0xFFFE && body.len() >= 26 {
                    format_tag = u16::from_le_bytes([body[24], body[25]]);
                }
            }
            b"data" => data = Some(body),
            _ => {}
        }
        // RIFF chunks are word-aligned.
        pos = body_start + len + (len & 1);
    }

    if channels == 0 || sample_rate == 0 {
        return Err(DecodeError::BadWav("missing or empty fmt chunk".into()));
    }
    let Some(data) = data else {
        return Err(DecodeError::BadWav("no data chunk".into()));
    };

    let bytes_per_sample = (bits as usize + 7) / 8;
    if bytes_per_sample == 0 {
        return Err(DecodeError::UnsupportedWavFormat(format!("{bits}-bit")));
    }
    let frames = data.len() / (bytes_per_sample * channels as usize);
    if frames == 0 {
        return Err(DecodeError::BadWav("empty data chunk".into()));
    }

    // Read one frame's worth of channel samples, normalized to f64. Integer
    // PCM is scaled to a ±1.0 range so downstream dB math is encoding-agnostic.
    let read_channel = |frame: usize, ch: usize| -> Option<f64> {
        let off = (frame * channels as usize + ch) * bytes_per_sample;
        let raw = data.get(off..off + bytes_per_sample)?;
        Some(match (bits, format_tag) {
            (16, 1) => i16::from_le_bytes([raw[0], raw[1]]) as f64 / 32768.0,
            (24, 1) => {
                let v = ((raw[2] as i32) << 24 | (raw[1] as i32) << 16 | (raw[0] as i32) << 8) >> 8;
                v as f64 / 8388608.0
            }
            (32, 1) => i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as f64 / 2147483648.0,
            (32, 3) => f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as f64,
            (64, 3) => f64::from_le_bytes([raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7]]),
            (b, t) => unreachable!("format filtered before read: {b} bits tag {t}"),
        })
    };

    // Only the encodings above are supported; everything else is rejected
    // up front so `read_channel`'s unreachable arm stays unreachable.
    if !matches!(
        (bits, format_tag),
        (16, 1) | (24, 1) | (32, 1) | (32, 3) | (64, 3)
    ) {
        return Err(DecodeError::UnsupportedWavFormat(format!(
            "{bits}-bit, format tag {format_tag}"
        )));
    }

    let mut samples = Vec::with_capacity(frames);
    let mut notes = vec![format!("WAV: {channels}-channel, {bits}-bit, {sample_rate} Hz")];
    for frame in 0..frames {
        let mut sum = 0.0;
        for ch in 0..channels as usize {
            sum += read_channel(frame, ch).unwrap_or(0.0);
        }
        samples.push(sum / channels as f64);
    }
    if channels > 1 {
        notes.push("channels averaged to mono".to_string());
    }

    Ok(Recording {
        sample_rate: sample_rate as f64,
        samples,
        notes,
    })
}

// --------------------------------------------------------------- CSV ----

/// Decodes a CSV/TSV export of a vibration recording. Tolerant about
/// delimiters (`,` `;` tab) and headers:
///
/// - 1 numeric column: values, `sample_rate_hz` must be supplied;
/// - 2+ numeric columns: first column is time (monotonic seconds), remaining
///   columns are channels (averaged); the sample rate is inferred from the
///   median time step, and `sample_rate_hz` — when given — is used only as a
///   fallback for constant/blank time columns.
pub fn decode_csv(bytes: &[u8], sample_rate_hz: Option<f64>) -> Result<Recording, DecodeError> {
    let text = String::from_utf8_lossy(bytes);
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let delim = pick_delimiter(line);
        let fields: Vec<&str> = line.split(delim).collect();
        let mut parsed = Vec::with_capacity(fields.len());
        let mut all_numeric = true;
        for f in fields {
            match parse_number(f) {
                Some(v) => parsed.push(v),
                None => {
                    all_numeric = false;
                    break;
                }
            }
        }
        // A header row (or any line with junk in it) is skipped — recordings
        // are wall-to-wall numbers, so one skipped line can't lose data.
        if all_numeric && !parsed.is_empty() {
            rows.push(parsed);
        }
    }
    if rows.is_empty() {
        return Err(DecodeError::BadCsv("no numeric data rows found".into()));
    }

    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut notes = Vec::new();

    if width == 1 {
        let rate = sample_rate_hz
            .filter(|r| *r > 0.0)
            .ok_or_else(|| DecodeError::BadCsv(
                "single-column CSV needs a sample rate — enter it next to the file picker".into(),
            ))?;
        return Ok(Recording {
            sample_rate: rate,
            samples: rows.into_iter().map(|r| r[0]).collect(),
            notes: vec![format!("CSV: single value column, rate taken from input ({rate} Hz)")],
        });
    }

    // Time column + 1..n value channels. Require the time column to actually
    // increase — otherwise fall back to the supplied rate (or 1 kHz) and
    // treat column 0 as data too.
    let t0 = rows[0][0];
    let tlast = rows[rows.len() - 1][0];
    if tlast > t0 {
        let mut dts: Vec<f64> = rows.windows(2).map(|w| w[1][0] - w[0][0]).collect();
        dts.retain(|dt| *dt > 0.0);
        let rate = if dts.is_empty() {
            sample_rate_hz.unwrap_or(1000.0)
        } else {
            dts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            1.0 / dts[dts.len() / 2]
        };
        let n_channels = width - 1;
        if n_channels > 1 {
            notes.push(format!("{n_channels} channels averaged to mono"));
        }
        let samples: Vec<f64> = rows
            .iter()
            .map(|r| {
                let mut sum = 0.0;
                for v in &r[1..] {
                    sum += *v;
                }
                sum / n_channels as f64
            })
            .collect();
        notes.push(format!(
            "CSV: time+value, rate inferred from timestamps ({rate:.1} Hz)"
        ));
        Ok(Recording {
            sample_rate: rate,
            samples,
            notes,
        })
    } else {
        // No usable time axis: treat every column as data, average them.
        let rate = sample_rate_hz
            .filter(|r| *r > 0.0)
            .ok_or_else(|| {
                DecodeError::BadCsv(
                    "time column is not increasing — enter the sample rate manually".into(),
                )
            })?;
        let samples: Vec<f64> = rows
            .iter()
            .map(|r| r.iter().sum::<f64>() / r.len() as f64)
            .collect();
        notes.push(format!(
            "CSV: no usable time column, averaged {width} columns, rate from input ({rate} Hz)"
        ));
        Ok(Recording {
            sample_rate: rate,
            samples,
            notes,
        })
    }
}

/// Dispatch a file by sniffing its head: RIFF....WAVE → WAV, else CSV.
pub fn decode_auto(bytes: &[u8], sample_rate_hz: Option<f64>) -> Result<Recording, DecodeError> {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        decode_wav(bytes)
    } else {
        decode_csv(bytes, sample_rate_hz)
    }
}

// ---------------------------------------------------------- wav_encode ----

/// Minimal WAV writers — used by tests for round-trip validation and handy
/// for tooling that wants to synthesize recordings.
pub mod wav_encode {
    fn container(fmt: &[u8], data: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(12 + 8 + fmt.len() + 8 + data.len());
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&((4 + 8 + fmt.len() + 8 + data.len()) as u32).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        v.extend_from_slice(fmt);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn fmt_chunk(tag: u16, channels: u16, rate: f64, bits: u16) -> [u8; 16] {
        let rate = rate as u32;
        let mut f = [0u8; 16];
        f[0..2].copy_from_slice(&tag.to_le_bytes());
        f[2..4].copy_from_slice(&channels.to_le_bytes());
        f[4..8].copy_from_slice(&rate.to_le_bytes());
        f[8..12].copy_from_slice(&(rate * channels as u32 * bits as u32 / 8).to_le_bytes());
        f[12..14].copy_from_slice(&((channels * bits / 8) as u16).to_le_bytes());
        f[14..16].copy_from_slice(&bits.to_le_bytes());
        f
    }

    /// Mono 16-bit PCM WAV.
    pub fn pcm16(rec: &super::Recording) -> Result<Vec<u8>, super::DecodeError> {
        let mut data = Vec::with_capacity(rec.samples.len() * 2);
        for &s in &rec.samples {
            let clamped = s.clamp(-1.0, 1.0);
            data.extend_from_slice(&((clamped * 32767.0) as i16).to_le_bytes());
        }
        Ok(container(
            &fmt_chunk(1, 1, rec.sample_rate, 16),
            &data,
        ))
    }

    /// Mono 32-bit float WAV (lossless round-trip of synthesized samples).
    pub fn float32(rec: &super::Recording) -> Result<Vec<u8>, super::DecodeError> {
        let mut data = Vec::with_capacity(rec.samples.len() * 4);
        for &s in &rec.samples {
            data.extend_from_slice(&(s as f32).to_le_bytes());
        }
        Ok(container(
            &fmt_chunk(3, 1, rec.sample_rate, 32),
            &data,
        ))
    }
}

fn pick_delimiter(line: &str) -> char {
    for c in ['\t', ';', ','] {
        if line.contains(c) {
            return c;
        }
    }
    // Whitespace-separated (space or multiple spaces).
    ' '
}

/// Parses `f64` tolerantly: trims whitespace, quotes and a UTF-8 BOM,
/// accepts both `.` and `,` decimal separators (a `,` is a decimal separator
/// only when the field has no other delimiter in it — CSV dispatch above
/// runs first, so by the time we see a comma here it's decimal, as in
/// European exports like `0,0123`).
fn parse_number(field: &str) -> Option<f64> {
    let mut s = field.trim().trim_matches('"').trim().to_string();
    if s.starts_with('\u{feff}') {
        s = s[3..].to_string();
    }
    if s.is_empty() {
        return None;
    }
    if !s.contains('.') && s.matches(',').count() == 1 {
        s = s.replacen(',', ".", 1);
    }
    s.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_bytes(fmt_body: &[u8], data_body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        let riff_len = (4 + 8 + fmt_body.len() + 8 + data_body.len()) as u32;
        v.extend_from_slice(&riff_len.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&(fmt_body.len() as u32).to_le_bytes());
        v.extend_from_slice(fmt_body);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data_body.len() as u32).to_le_bytes());
        v.extend_from_slice(data_body);
        v
    }

    fn fmt_chunk(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&tag.to_le_bytes());
        f.extend_from_slice(&channels.to_le_bytes());
        f.extend_from_slice(&rate.to_le_bytes());
        let byte_rate = rate * channels as u32 * bits as u32 / 8;
        f.extend_from_slice(&byte_rate.to_le_bytes());
        let align = channels * bits / 8;
        f.extend_from_slice(&(align as u16).to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f
    }

    #[test]
    fn wav_pcm16_mono_roundtrip() {
        let samples: Vec<i16> = [0, 16384, -16384, 32767, -32768].to_vec();
        let mut data = Vec::new();
        for s in &samples {
            data.extend_from_slice(&s.to_le_bytes());
        }
        let rec = decode_wav(&wav_bytes(&fmt_chunk(1, 1, 48000, 16), &data)).unwrap();
        assert_eq!(rec.sample_rate, 48000.0);
        assert_eq!(rec.samples.len(), 5);
        assert!((rec.samples[3] - 32767.0 / 32768.0).abs() < 1e-9);
        assert!((rec.samples[4] - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn wav_float32_stereo_averages() {
        // L = [1.0, -1.0], R = [0.0, 1.0] → avg [0.5, 0.0]
        let mut data = Vec::new();
        for pair in [(1.0f32, 0.0f32), (-1.0, 1.0)] {
            data.extend_from_slice(&pair.0.to_le_bytes());
            data.extend_from_slice(&pair.1.to_le_bytes());
        }
        let rec = decode_wav(&wav_bytes(&fmt_chunk(3, 2, 44100, 32), &data)).unwrap();
        assert_eq!(rec.samples.len(), 2);
        assert!((rec.samples[0] - 0.5).abs() < 1e-12);
        assert!((rec.samples[1] - 0.0).abs() < 1e-12);
        assert!(rec.notes.iter().any(|n| n.contains("averaged")));
    }

    #[test]
    fn wav_24bit_reads() {
        // 24-bit little-endian: value 1_000_000 (of 8_388_608 full scale).
        let raw: i32 = 1_000_000;
        let b = [raw as u8, (raw >> 8) as u8, (raw >> 16) as u8];
        let rec = decode_wav(&wav_bytes(&fmt_chunk(1, 1, 8000, 24), &b)).unwrap();
        assert!((rec.samples[0] - 1_000_000.0 / 8_388_608.0).abs() < 1e-9);
    }

    #[test]
    fn wav_extensible_float_maps_tag() {
        // WAVE_FORMAT_EXTENSIBLE fmt chunk (40 bytes): standard 16, then
        // cbSize(2), wValidBitsPerSample(2), dwChannelMask(4), and the
        // SubFormat GUID (16) whose first two bytes carry the format code.
        let mut f = fmt_chunk(0xFFFE, 1, 48000, 32);
        f.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        f.extend_from_slice(&32u16.to_le_bytes()); // wValidBitsPerSample
        f.extend_from_slice(&4u32.to_le_bytes()); // dwChannelMask (mono)
        f.extend_from_slice(&3u16.to_le_bytes()); // SubFormat: IEEE float
        f.extend_from_slice(&[0u8; 14]); // GUID remainder
        let mut data = Vec::new();
        for s in [0f32, -2.5] {
            data.extend_from_slice(&s.to_le_bytes());
        }
        let rec = decode_wav(&wav_bytes(&f, &data)).unwrap();
        assert_eq!(rec.samples.len(), 2);
        assert!((rec.samples[1] - (-2.5)).abs() < 1e-12);
    }

    #[test]
    fn rejects_non_wav() {
        assert!(matches!(decode_wav(b"hello there!"), Err(DecodeError::NotWav)));
    }

    #[test]
    fn csv_two_columns_infers_rate() {
        // 10 ms steps → 100 Hz.
        let csv = "time,accel\n0,0.1\n0.01,0.2\n0.02,0.3\n0.03,0.4\n";
        let rec = decode_csv(csv.as_bytes(), None).unwrap();
        assert!((rec.sample_rate - 100.0).abs() < 1e-9);
        assert_eq!(rec.samples, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn csv_single_column_needs_rate() {
        assert!(decode_csv(b"0.1\n0.2\n", None).is_err());
        let rec = decode_csv(b"0.1\n0.2\n", Some(2048.0)).unwrap();
        assert_eq!(rec.sample_rate, 2048.0);
    }

    #[test]
    fn csv_semicolon_decimal_comma() {
        // European export: semicolon delimiter, comma decimals, header.
        let csv = "Zeit;Beschleunigung\n0;0,5\n0,01;1,5\n0,02;2,5\n";
        let rec = decode_csv(csv.as_bytes(), None).unwrap();
        assert!((rec.sample_rate - 100.0).abs() < 1e-9);
        assert_eq!(rec.samples, vec![0.5, 1.5, 2.5]);
    }

    #[test]
    fn csv_skips_junk_lines() {
        let csv = "some header text\n\n0.01,1\n0.02,2\n0.03,3\n";
        let rec = decode_csv(csv.as_bytes(), None).unwrap();
        assert_eq!(rec.samples, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn auto_dispatch() {
        assert!(decode_auto(b"not a wav", None).is_err()); // falls to CSV, no rows
        let wav = wav_bytes(&fmt_chunk(1, 1, 16000, 16), &1i16.to_le_bytes());
        assert!(decode_auto(&wav, None).is_ok());
    }
}
