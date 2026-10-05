//! Maintenance-report export (pro): a self-contained HTML document and a
//! trend CSV, both plain strings the app turns into downloads. No
//! dependencies — the HTML is a template with inline CSS that opens
//! standalone in any browser or prints to PDF.

use crate::faults::{BearingReport, FaultReport};
use crate::spectrum::{Peak, Stats};

/// One row of the machine's trend log (one row per analyzed recording).
#[derive(Debug, Clone)]
pub struct TrendRow {
    /// ISO date the recording was taken/analyzed.
    pub date: String,
    /// Recording label (file name or note).
    pub label: String,
    /// Overall RMS in the recording's units.
    pub rms: f64,
    /// Imbalance score (1×/RMS ratio).
    pub imbalance: f64,
    /// Misalignment score (harmonic ratio).
    pub misalignment: f64,
    /// Worst bearing-envelope prominence.
    pub bearing: f64,
}

/// Renders the standalone maintenance report.
///
/// `unit` is the recordings' engineering-unit label ("g", "m/s²", …) — the
/// engine is unit-agnostic, so the report just carries the label through.
pub fn maintenance_report_html(
    machine: &str,
    date: &str,
    notes: &[String],
    stats: &Stats,
    top_peaks: &[Peak],
    shaft_hz: f64,
    report: &FaultReport,
    bearing: &BearingReport,
    unit: &str,
    trend: &[TrendRow],
) -> String {
    let peaks_rows: String = top_peaks
        .iter()
        .take(10)
        .map(|p| {
            format!(
                "<tr><td>{:.1}</td><td>{:.4}</td></tr>",
                p.freq_hz, p.amp
            )
        })
        .collect();

    let fault_row = |name: &str, detail: &str, sev: &str| {
        let class = match sev {
            "alert" => "alert",
            "watch" => "watch",
            _ => "normal",
        };
        format!(
            "<tr><td>{name}</td><td><span class=\"pill {class}\">{sev}</span></td><td>{detail}</td></tr>"
        )
    };
    let faults_rows = format!(
        "{}{}{}",
        fault_row(
            "Imbalance",
            &report.imbalance.detail,
            report.imbalance.severity.label()
        ),
        fault_row(
            "Misalignment",
            &report.misalignment.detail,
            report.misalignment.severity.label()
        ),
        fault_row(
            "Bearing wear",
            &bearing.score.detail,
            bearing.score.severity.label()
        ),
    );

    let bearing_freqs = format!(
        "<table><tr><th>FTF</th><th>BPFO</th><th>BPFI</th><th>BSF</th></tr><tr><td>{:.1} Hz</td><td>{:.1} Hz</td><td>{:.1} Hz</td><td>{:.1} Hz</td></tr></table><p class=\"muted\">Demodulation band {:.0}–{:.0} Hz.</p>",
        bearing.freqs.ftf,
        bearing.freqs.bpfo,
        bearing.freqs.bpfi,
        bearing.freqs.bsf,
        bearing.band.0,
        bearing.band.1
    );

    let bearing_hits: String = if bearing.hits.is_empty() {
        "<p>No bearing tones in the envelope spectrum.</p>".to_string()
    } else {
        let rows: String = bearing
            .hits
            .iter()
            .map(|h| {
                format!(
                    "<tr><td>{}</td><td>{:.1}</td><td>{:.5}</td><td>{:.0}×</td><td>{}</td></tr>",
                    h.component,
                    h.freq_hz,
                    h.envelope_amp,
                    h.prominence,
                    h.severity.label()
                )
            })
            .collect();
        format!(
            "<table><tr><th>Component</th><th>Hz</th><th>Envelope amp ({unit})</th><th>Prominence</th><th>Status</th></tr>{rows}</table>"
        )
    };

    let trend_section = if trend.is_empty() {
        String::new()
    } else {
        let rows: String = trend
            .iter()
            .map(|r| {
                format!(
                    "<tr><td>{}</td><td>{}</td><td>{:.4}</td><td>{:.2}</td><td>{:.2}</td><td>{:.0}×</td></tr>",
                    r.date, r.label, r.rms, r.imbalance, r.misalignment, r.bearing
                )
            })
            .collect();
        format!(
            "<h2>Trend history</h2><table><tr><th>Date</th><th>Recording</th><th>RMS ({unit})</th><th>Imbalance</th><th>Misalignment</th><th>Bearing</th></tr>{rows}</table>"
        )
    };

    let notes_html = if notes.is_empty() {
        String::new()
    } else {
        let items: String = notes
            .iter()
            .map(|n| format!("<li>{}</li>", html_escape(n)))
            .collect();
        format!("<h2>Recording notes</h2><ul>{items}</ul>")
    };

    format!(
        include_str!("report_template.html"),
        machine = html_escape(machine),
        date = html_escape(date),
        unit = html_escape(unit),
        rms = stats.rms,
        peak = stats.peak,
        crest = stats.crest_factor,
        samples = stats.samples,
        duration = stats.duration_s,
        shaft = shaft_hz,
        notes = notes_html,
        peaks = peaks_rows,
        faults = faults_rows,
        bearing_freqs = bearing_freqs,
        bearing_hits = bearing_hits,
        trend = trend_section,
    )
}

/// Renders the trend log as CSV.
pub fn trend_csv(trend: &[TrendRow]) -> String {
    let mut out = String::from("date,recording,rms,imbalance,misalignment,bearing_prominence\n");
    for r in trend {
        out.push_str(&format!(
            "{},{},{:.6},{:.4},{:.4},{:.2}\n",
            csv_field(&r.date),
            csv_field(&r.label),
            r.rms,
            r.imbalance,
            r.misalignment,
            r.bearing
        ));
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Recording;
    use crate::faults::{analyze, BearingGeometry, FaultInput};
    use crate::spectrum;

    #[test]
    fn report_contains_sections_and_escapes() {
        let rec = Recording {
            sample_rate: 8192.0,
            samples: vec![0.1; 4096],
            notes: vec!["<script>alert(1)</script> & 6205 bearing".to_string()],
        };
        let stats = spectrum::stats(&rec);
        let spec = spectrum::analyze(&rec, 4096);
        let peaks = spectrum::peaks(&spec, 1.0, 4000.0, 10, 0.05);
        let input = FaultInput {
            rpm: 1797.0,
            bearing: BearingGeometry::default(),
            demod_band: None,
        };
        let report = analyze(&rec, &input);
        let html = maintenance_report_html(
            "Pump <A> \"line 3\"",
            "2026-10-06",
            &rec.notes,
            &stats,
            &peaks,
            input.rpm / 60.0,
            &report,
            &report.bearing,
            "g",
            &[TrendRow {
                date: "2026-10-01".into(),
                label: "run1.csv".into(),
                rms: 0.1,
                imbalance: 0.2,
                misalignment: 0.3,
                bearing: 2.0,
            }],
        );
        assert!(html.contains("Pump &lt;A&gt; &quot;line 3&quot;"));
        assert!(!html.contains("<script>alert(1)"));
        assert!(html.contains("Trend history"));
        assert!(html.contains("BPFO"));
        assert!(html.contains("Imbalance"));
    }

    #[test]
    fn csv_quotes_fields() {
        let csv = trend_csv(&[TrendRow {
            date: "2026-10-06".into(),
            label: "run, 2\".csv".into(),
            rms: 1.0,
            imbalance: 0.0,
            misalignment: 0.0,
            bearing: 0.0,
        }]);
        assert_eq!(
            csv,
            "date,recording,rms,imbalance,misalignment,bearing_prominence\n2026-10-06,\"run, 2\"\".csv\",1.000000,0.0000,0.0000,0.00\n"
        );
    }
}
