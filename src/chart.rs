//! Canvas chart helpers for the spectrum (and pro trend/envelope) views.
//! Raw web-sys canvas — the appfront DOM backend has no canvas node kind.

use wasm_bindgen::JsCast;

/// Colors pulled from the app's CSS custom properties (so the canvas matches
/// light/dark scheme without duplicating the palette here).
#[derive(Debug, Clone)]
pub struct Theme {
    pub fg: String,
    pub muted: String,
    pub border: String,
    pub accent: String,
}

pub fn read_theme(container: &web_sys::Element) -> Theme {
    Theme {
        fg: read_css(container, "--vib-fg", "#181d27"),
        muted: read_css(container, "--vib-muted", "#667085"),
        border: read_css(container, "--vib-border", "#e3e7ee"),
        accent: read_css(container, "--vib-accent", "#0e7c6b"),
    }
}

pub fn read_css(container: &web_sys::Element, name: &str, fallback: &str) -> String {
    let Some(window) = web_sys::window() else {
        return fallback.to_string();
    };
    let Ok(Some(style)) = window.get_computed_style(container) else {
        return fallback.to_string();
    };
    style
        .get_property_value(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// Brings a canvas to `css_w × css_h` CSS pixels at device-pixel resolution
/// and returns the CSS size (the drawing code scales by DPR itself).
pub fn size_canvas(canvas: &web_sys::HtmlCanvasElement) -> (f64, f64) {
    let rect = canvas.get_bounding_client_rect();
    let css_w = rect.width().max(10.0);
    let css_h = rect.height().max(10.0);
    let dpr = web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0);
    let _ = canvas.set_attribute("width", &((css_w * dpr).round() as usize).to_string());
    let _ = canvas.set_attribute("height", &((css_h * dpr).round() as usize).to_string());
    (css_w, css_h)
}

fn ctx(canvas: &web_sys::HtmlCanvasElement) -> Option<web_sys::CanvasRenderingContext2d> {
    canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok())
}

/// "Nice" tick positions covering `[min, max]` (1/2/5 × 10ⁿ steps).
pub fn ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if max <= min || target == 0 {
        return Vec::new();
    }
    let span = max - min;
    let raw_step = span / target as f64;
    let mag = 10f64.powf(raw_step.log10().floor());
    let norm = raw_step / mag;
    let step = if norm < 1.5 {
        mag
    } else if norm < 3.5 {
        2.0 * mag
    } else if norm < 7.5 {
        5.0 * mag
    } else {
        10.0 * mag
    };
    let first = (min / step).ceil() * step;
    let mut out = Vec::new();
    let mut v = first;
    while v <= max * (1.0 + 1e-9) {
        out.push((v * 1e6).round() / 1e6);
        v += step;
    }
    out
}

pub struct PlotSetup {
    pub ctx: web_sys::CanvasRenderingContext2d,
    pub w: f64,
    pub h: f64,
    /// Left margin (y labels).
    pub ml: f64,
    /// Bottom margin (x labels).
    pub mb: f64,
    pub mt: f64,
    pub mr: f64,
}

/// Clears the canvas at DPR resolution and returns the plot geometry.
pub fn begin_plot(canvas: &web_sys::HtmlCanvasElement) -> Option<PlotSetup> {
    let (w, h) = size_canvas(canvas);
    let dpr = web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0);
    let ctx = ctx(canvas)?;
    ctx.clear_rect(0.0, 0.0, w * dpr, h * dpr);
    let _ = ctx.scale(dpr, dpr);
    Some(PlotSetup {
        ctx,
        w,
        h,
        ml: 52.0,
        mb: 30.0,
        mt: 14.0,
        mr: 14.0,
    })
}

/// Draws axis + grid from `ticks_x`/`ticks_y` value ranges, with a labeling
/// closure deciding each tick's text. Returns the plot rect in CSS px.
pub fn draw_axes(
    p: &PlotSetup,
    theme: &Theme,
    xr: (f64, f64),
    yr: (f64, f64),
    ticks_x: &[f64],
    ticks_y: &[f64],
    label_x: &dyn Fn(f64) -> String,
    label_y: &dyn Fn(f64) -> String,
) -> (f64, f64, f64, f64) {
    let (px0, py0) = (p.ml, p.mt);
    let (pw, ph) = (p.w - p.ml - p.mr, p.h - p.mt - p.mb);
    let px1 = px0 + pw;
    let py1 = py0 + ph;
    let sx = |v: f64| px0 + (v - xr.0) / (xr.1 - xr.0).max(1e-12) * pw;
    let sy = |v: f64| py1 - (v - yr.0) / (yr.1 - yr.0).max(1e-12) * ph;

    p.ctx.set_font("11px sans-serif");
    p.ctx.set_text_align("center");
    p.ctx.set_text_baseline("top");
    p.ctx.set_stroke_style_str(&theme.border);
    p.ctx.set_fill_style_str(&theme.muted);
    p.ctx.set_line_width(1.0);
    for &t in ticks_x {
        let x = sx(t).round() + 0.5;
        let _ = p.ctx.begin_path();
        let _ = p.ctx.move_to(x, py0);
        let _ = p.ctx.line_to(x, py1);
        let _ = p.ctx.stroke();
        let _ = p.ctx.fill_text(&label_x(t), x, py1 + 6.0);
    }
    p.ctx.set_text_align("right");
    p.ctx.set_text_baseline("middle");
    for &t in ticks_y {
        let y = sy(t).round() + 0.5;
        let _ = p.ctx.begin_path();
        let _ = p.ctx.move_to(px0, y);
        let _ = p.ctx.line_to(px1, y);
        let _ = p.ctx.stroke();
        let _ = p.ctx.fill_text(&label_y(t), px0 - 6.0, y);
    }

    // Axis frame.
    p.ctx.set_stroke_style_str(&theme.muted);
    let _ = p.ctx.stroke_rect(px0. into(), py0.into(), pw.into(), ph.into());
    (px0, py0, pw, ph)
}

/// Renders the single-sided amplitude spectrum with the top peaks marked.
pub fn draw_spectrum(
    canvas: &web_sys::HtmlCanvasElement,
    theme: &Theme,
    freq_hz: &[f64],
    amp: &[f64],
    max_hz: f64,
    peak_marks: &[(f64, String)],
) {
    let Some(p) = begin_plot(canvas) else { return };
    let n = freq_hz.len().min(amp.len());
    if n < 2 {
        return;
    }
    // Decimate for the plot: one point per pixel column at most.
    let plot_px = (p.w - p.ml - p.mr) as usize;
    let stride = ((n - 1) / plot_px.max(1)).max(1);
    let mut max_amp = 0.0f64;
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(n / stride + 2);
    let mut i = 0;
    while i < n {
        if freq_hz[i] <= max_hz {
            max_amp = max_amp.max(amp[i]);
            pts.push((freq_hz[i], amp[i]));
        }
        i += stride;
    }
    if pts.len() < 2 {
        return;
    }
    let ymax = max_amp * 1.12;

    let tx = ticks(0.0, max_hz, 8);
    let ty = ticks(0.0, ymax, 5);
    let (px0, py0, pw, ph) = draw_axes(
        &p,
        theme,
        (0.0, max_hz),
        (0.0, ymax),
        &tx,
        &ty,
        &|v| if max_hz >= 10000.0 { format!("{:.0}k", v / 1000.0) } else { format!("{v:.0}") },
        &|v| {
            if ymax >= 100.0 { format!("{v:.0}") }
            else if ymax >= 1.0 { format!("{v:.2}") }
            else { format!("{v:.3}") }
        },
    );
    let sx = |v: f64| px0 + v / max_hz * pw;
    let sy = |v: f64| py0 + ph - v / ymax * ph;

    // Filled area + line.
    p.ctx.save();
    let _ = p.ctx.begin_path();
    let _ = p.ctx.move_to(sx(pts[0].0).into(), sy(0.0).into());
    for (f, a) in &pts {
        let _ = p.ctx.line_to(sx(*f).into(), sy(*a).into());
    }
    let _ = p.ctx.line_to(sx(pts[pts.len() - 1].0).into(), sy(0.0).into());
    let _ = p.ctx.close_path();
    p.ctx.set_fill_style_str(&theme.accent);
    p.ctx.set_global_alpha(0.18);
    let _ = p.ctx.fill();
    p.ctx.restore();

    p.ctx.save();
    let _ = p.ctx.begin_path();
    let _ = p.ctx.move_to(sx(pts[0].0).into(), sy(pts[0].1).into());
    for (f, a) in &pts {
        let _ = p.ctx.line_to(sx(*f).into(), sy(*a).into());
    }
    p.ctx.set_stroke_style_str(&theme.accent);
    p.ctx.set_line_width(1.4);
    let _ = p.ctx.stroke();
    p.ctx.restore();

    // Peak markers: dot + label, alternating label heights to avoid overlap.
    p.ctx.set_font("10px sans-serif");
    p.ctx.set_text_align("center");
    p.ctx.set_text_baseline("bottom");
    for (idx, (f, label)) in peak_marks.iter().enumerate() {
        if *f > max_hz {
            continue;
        }
        let x = sx(*f);
        let a = amp_at_y(freq_hz, amp, *f);
        let y = sy(a);
        p.ctx.set_fill_style_str(&theme.fg);
        let _ = p.ctx.begin_path();
        let _ = p.ctx.arc(x.into(), y.into(), 2.5.into(), 0.0, std::f64::consts::TAU);
        let _ = p.ctx.fill();
        let dy = if idx % 2 == 0 { 8.0 } else { 20.0 };
        let _ = p.ctx.fill_text(label, x.into(), (y - dy).into());
    }
}

/// Nearest-bin amplitude at `freq` (peak labels sit at the interpolated
/// frequency but the drawn marker should sit on the curve).
fn amp_at_y(freq_hz: &[f64], amp: &[f64], freq: f64) -> f64 {
    if freq_hz.len() < 2 {
        return 0.0;
    }
    let bin_w = freq_hz[1] - freq_hz[0];
    let k = ((freq / bin_w).round() as usize).min(amp.len() - 1);
    amp[k]
}
