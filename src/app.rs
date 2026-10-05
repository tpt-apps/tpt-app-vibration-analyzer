//! Free WASM edition UI (tpt-appfront-dom), with the pro edition's
//! diagnostics compiled in behind `--features pro`.
//!
//! Rendering architecture (mirrors fea-lite's proven split):
//! - the **input panel is mounted once** (`tpt_appfront_dom::mount`) — typing
//!   never loses focus; field values are read straight from the DOM when a
//!   recording is analyzed;
//! - the **results card** is shown/hidden and its canvases redrawn by an
//!   effect that watches the `outcome` + view signals;
//! - the **charts** are raw web-sys `<canvas>` elements (the DOM backend has
//!   no canvas node kind) appended into placeholder containers;
//! - buttons backed by engine work (pick/demo/export) are wired as direct
//!   DOM listeners after mount, like the file input — the appfront dispatch
//!   only carries the tree's `VisitSite` messages.

use std::rc::Rc;

use tpt_appfront_core::{create_effect, Signal, UITree};
use tpt_vibration_engine::decode::{self, Recording};
use tpt_vibration_engine::spectrum::{self, Peak, Spectrum, Stats};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[cfg(feature = "pro")]
use tpt_vibration_engine::faults::{self, FaultInput, FaultReport, Severity};
#[cfg(feature = "pro")]
use tpt_vibration_engine::report::{self, TrendRow};

use crate::chart;

/// Where the upsell/edition links point. The hub page hosts the free demo
/// and the Pro buy button.
const TOOLS_URL: &str = "https://tptsolutions.co.nz/tools/vibration-analyzer";

/// The results section's current content.
#[derive(Clone)]
enum Outcome {
    Idle,
    Error(String),
    Analyzed(Rc<Analysis>),
}

struct Analysis {
    name: String,
    notes: Vec<String>,
    stats: Stats,
    spectrum: Rc<Spectrum>,
    peaks: Vec<Peak>,
    /// (pro) fault-signature scores for this recording.
    #[cfg(feature = "pro")]
    faults: Rc<FaultReport>,
    /// (pro) the machine's trend log including this recording.
    #[cfg(feature = "pro")]
    trend: Rc<Vec<TrendRow>>,
    /// (pro) machine label the trend was stored under.
    #[cfg(feature = "pro")]
    machine: String,
}

#[derive(Debug, Clone)]
enum Msg {
    /// Open a TPT Solutions URL — in a new browser tab for the in-browser
    /// free edition, or via the desktop host's system-browser IPC action for
    /// the Pro desktop shell (whose webview would otherwise navigate itself
    /// away from the local `app://` bundle with no way back). See
    /// [`visit_site`].
    VisitSite(String),
}

const BASE_CSS: &str = r#"
.vib-app{--vib-bg:#f7f8fa;--vib-surface:#ffffff;--vib-panel:#f2f4f7;--vib-fg:#181d27;--vib-muted:#667085;--vib-border:#e3e7ee;--vib-accent:#0e7c6b;--vib-accent-2:#0a5f52;--vib-accent-soft:#e6f5f2;--vib-accent-fg:#ffffff;--vib-warn:#b25e09;--vib-warn-bg:#fef4e6;--vib-err:#b3261e;--vib-err-bg:#fbeeec;--vib-shadow:0 1px 2px rgba(16,24,40,.04),0 1px 3px rgba(16,24,40,.06);max-width:1160px;margin:0 auto;background:var(--vib-bg);color:var(--vib-fg);font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:14.5px;line-height:1.55;padding:0 4px 32px}
@media(prefers-color-scheme:dark){.vib-app{--vib-bg:#0b0f16;--vib-surface:#141a24;--vib-panel:#161d2a;--vib-fg:#e8ebf0;--vib-muted:#98a2b3;--vib-border:#262f3d;--vib-accent:#34b39a;--vib-accent-2:#4fd6ba;--vib-accent-soft:#122824;--vib-accent-fg:#04140f;--vib-warn:#e8a765;--vib-warn-bg:#2c2113;--vib-err:#f2b8b5;--vib-err-bg:#2c1a1c;--vib-shadow:0 1px 2px rgba(0,0,0,.3)}}
.vib-app *{box-sizing:border-box}
.vib-app .vib-header{display:flex;align-items:center;gap:10px;padding:18px 2px 10px}
.vib-app .vib-brand{display:inline-flex;align-items:center;justify-content:center;width:34px;height:34px;border-radius:8px;background:var(--vib-accent);color:var(--vib-accent-fg);font-weight:700;font-size:13px;letter-spacing:.5px;flex:none}
.vib-app .vib-title-group{min-width:0}
.vib-app .vib-title-row{display:flex;align-items:center;gap:8px}
.vib-app h2{margin:0;font-size:19px;line-height:1.25}
.vib-app .vib-chip{display:inline-block;padding:1px 8px;border-radius:999px;background:var(--vib-accent-soft);color:var(--vib-accent-2);font-size:11.5px;font-weight:700;text-transform:uppercase;letter-spacing:.4px}
.vib-app .vib-sub{color:var(--vib-muted);font-size:13px;margin-top:1px}
.vib-app .vib-card{background:var(--vib-surface);border:1px solid var(--vib-border);border-radius:10px;box-shadow:var(--vib-shadow);padding:14px 16px;margin-top:12px}
.vib-app h3{margin:0 0 10px;font-size:14px;text-transform:uppercase;letter-spacing:.6px;color:var(--vib-muted)}
.vib-app .vib-row{display:flex;flex-wrap:wrap;gap:10px;align-items:flex-end}
.vib-app .vib-field{display:flex;flex-direction:column;gap:3px;min-width:0}
.vib-app .vib-label{font-size:12px;color:var(--vib-muted)}
.vib-app input{font:inherit;color:var(--vib-fg);background:var(--vib-panel);border:1px solid var(--vib-border);border-radius:6px;padding:7px 9px;width:110px}
.vib-app input:focus{outline:2px solid var(--vib-accent);outline-offset:-1px}
.vib-app select{font:inherit;color:var(--vib-fg);background:var(--vib-panel);border:1px solid var(--vib-border);border-radius:6px;padding:7px 9px}
.vib-app button{font:inherit;font-weight:600;color:var(--vib-fg);background:var(--vib-panel);border:1px solid var(--vib-border);border-radius:6px;padding:7px 12px;cursor:pointer}
.vib-app button:hover{border-color:var(--vib-accent)}
.vib-app button.vib-primary{background:var(--vib-accent);border-color:var(--vib-accent);color:var(--vib-accent-fg)}
.vib-app button.vib-primary:hover{background:var(--vib-accent-2)}
.vib-app button.vib-link{background:none;border:none;color:var(--vib-accent);padding:0;font-weight:600}
.vib-app .vib-hint{color:var(--vib-muted);font-size:12.5px;margin-top:8px}
.vib-app .vib-status{margin-top:10px;font-size:13px}
.vib-app .vib-status.vib-err{color:var(--vib-err)}
.vib-app .vib-status ul{margin:4px 0 0 18px;padding:0;color:var(--vib-muted)}
.vib-app .vib-stats{display:flex;flex-wrap:wrap;gap:8px;margin:2px 0 12px}
.vib-app .vib-stat{background:var(--vib-panel);border:1px solid var(--vib-border);border-radius:8px;padding:6px 10px;min-width:96px}
.vib-app .vib-stat b{display:block;font-size:15px;max-width:260px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.vib-app .vib-stat span{font-size:11.5px;color:var(--vib-muted)}
.vib-app canvas{display:block;width:100%;height:300px}
.vib-app canvas.vib-envelope{height:200px;margin-top:10px}
.vib-app canvas.vib-trend{height:170px;margin-top:10px}
.vib-app .vib-slot{margin-top:6px}
.vib-app table{border-collapse:collapse;width:100%;font-size:13px;margin-top:10px}
.vib-app th,.vib-app td{text-align:left;padding:5px 8px;border-bottom:1px solid var(--vib-border)}
.vib-app th{color:var(--vib-muted);font-weight:600;font-size:12px}
.vib-app td.num,.vib-app th.num{text-align:right;font-variant-numeric:tabular-nums}
.vib-app .vib-pill{display:inline-block;padding:1px 9px;border-radius:999px;font-size:11.5px;font-weight:700}
.vib-app .vib-pill.normal{background:var(--vib-accent-soft);color:var(--vib-accent-2)}
.vib-app .vib-pill.watch{background:var(--vib-warn-bg);color:var(--vib-warn)}
.vib-app .vib-pill.alert{background:var(--vib-err-bg);color:var(--vib-err)}
.vib-app .vib-fault{display:flex;flex-wrap:wrap;gap:8px;align-items:baseline;padding:8px 0;border-bottom:1px solid var(--vib-border)}
.vib-app .vib-fault:last-child{border-bottom:none}
.vib-app .vib-fault b{width:120px;flex:none}
.vib-app .vib-fault .vib-detail{color:var(--vib-muted);font-size:12.5px;flex:1;min-width:200px}
.vib-app .vib-upsell{display:flex;flex-wrap:wrap;gap:14px;align-items:center;justify-content:space-between;background:var(--vib-accent-soft);border:1px solid var(--vib-accent);border-radius:10px;padding:14px 16px;margin-top:12px}
.vib-app .vib-upsell h3{color:var(--vib-accent-2);margin-bottom:4px;text-transform:none;font-size:15px;letter-spacing:0}
.vib-app .vib-upsell ul{margin:0;padding-left:18px;color:var(--vib-fg);font-size:13px}
.vib-app .vib-li{font-size:13px;margin:2px 0}
.vib-app .vib-footer{display:flex;gap:6px;align-items:center;margin-top:16px;color:var(--vib-muted);font-size:12.5px}
.vib-app .vib-export{display:flex;gap:8px;margin-top:12px;flex-wrap:wrap}
"#;

pub fn mount_app(container: &web_sys::Element) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window.document().ok_or_else(|| JsValue::from_str("no document"))?;

    super::upsert_style(&document, "vib-styles", BASE_CSS);
    let existing_class = container.get_attribute("class").unwrap_or_default();
    if !existing_class.split_whitespace().any(|c| c == "vib-app") {
        let combined = if existing_class.is_empty() {
            "vib-app".to_string()
        } else {
            format!("{existing_class} vib-app")
        };
        let _ = container.set_attribute("class", &combined);
    }

    let outcome: Signal<Outcome> = Signal::new(Outcome::Idle);
    let view_max_hz: Signal<f64> = Signal::new(0.0); // 0 = full range (Nyquist)

    // 1. The mounted-once shell: header, input card, status, results card,
    //    upsell (free) / diagnostics + exports (pro), footer.
    let ui: UITree<Msg> = shell_tree();
    tpt_appfront_dom::mount(
        container,
        &ui,
        Rc::new(|msg: Msg| match msg {
            Msg::VisitSite(url) => visit_site(&url),
        }),
    )?;

    // 2. Post-mount queries (everything the closures below need).
    let pick_btn: web_sys::HtmlElement = query(container, ".vib-pick")?;
    let demo_btn: web_sys::HtmlElement = query(container, ".vib-demo")?;
    let rate_input: web_sys::HtmlInputElement = query(container, ".vib-rate")?;
    let view_select: web_sys::HtmlSelectElement = query(container, ".vib-view")?;
    let results_card: web_sys::HtmlElement = query(container, ".vib-results")?;
    let status_box: web_sys::HtmlElement = query(container, ".vib-status")?;
    let stats_box: web_sys::HtmlElement = query(container, ".vib-stats")?;
    let peaks_box: web_sys::HtmlElement = query(container, ".vib-peaks")?;
    let spectrum_slot: web_sys::HtmlElement = query(container, ".vib-spectrum-slot")?;

    // (pro) diagnostics inputs and panels.
    #[cfg(feature = "pro")]
    let machine_input: web_sys::HtmlInputElement = query(container, ".vib-machine")?;
    #[cfg(feature = "pro")]
    let rpm_input: web_sys::HtmlInputElement = query(container, ".vib-rpm")?;
    #[cfg(feature = "pro")]
    let balls_input: web_sys::HtmlInputElement = query(container, ".vib-balls")?;
    #[cfg(feature = "pro")]
    let ball_dia_input: web_sys::HtmlInputElement = query(container, ".vib-ball-dia")?;
    #[cfg(feature = "pro")]
    let pitch_dia_input: web_sys::HtmlInputElement = query(container, ".vib-pitch-dia")?;
    #[cfg(feature = "pro")]
    let angle_input: web_sys::HtmlInputElement = query(container, ".vib-angle")?;
    #[cfg(feature = "pro")]
    let fault_box: web_sys::HtmlElement = query(container, ".vib-faults")?;
    #[cfg(feature = "pro")]
    let bearing_box: web_sys::HtmlElement = query(container, ".vib-bearing")?;
    #[cfg(feature = "pro")]
    let envelope_slot: web_sys::HtmlElement = query(container, ".vib-envelope-slot")?;
    #[cfg(feature = "pro")]
    let trend_box: web_sys::HtmlElement = query(container, ".vib-trend")?;

    // 3. Hidden file input backing the visible "Choose recording" button.
    let file_input: web_sys::HtmlInputElement = document
        .create_element("input")?
        .dyn_into()
        .map_err(|_| JsValue::from_str("file input cast failed"))?;
    file_input.set_attribute("type", "file")?;
    file_input.set_attribute("accept", ".wav,.csv,.txt,audio/wav,text/csv")?;
    file_input.set_attribute("style", "display:none")?;
    container.append_child(&file_input)?;

    // 4. Charts: created once, appended into their slots, redrawn by the
    //    effect below.
    let spectrum_canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")?
        .dyn_into()
        .map_err(|_| JsValue::from_str("canvas cast failed"))?;
    spectrum_slot.append_child(&spectrum_canvas)?;
    #[cfg(feature = "pro")]
    let envelope_canvas: web_sys::HtmlCanvasElement = {
        let c: web_sys::HtmlCanvasElement = document
            .create_element("canvas")?
            .dyn_into()
            .map_err(|_| JsValue::from_str("canvas cast failed"))?;
        let _ = c.set_attribute("class", "vib-envelope");
        envelope_slot.append_child(&c)?;
        c
    };
    #[cfg(feature = "pro")]
    let trend_canvas: web_sys::HtmlCanvasElement = {
        let c: web_sys::HtmlCanvasElement = document
            .create_element("canvas")?
            .dyn_into()
            .map_err(|_| JsValue::from_str("canvas cast failed"))?;
        let _ = c.set_attribute("class", "vib-trend");
        trend_box.append_child(&c)?;
        c
    };

    // 5. "Choose recording" opens the hidden file input.
    {
        let input_for_pick = file_input.clone();
        add_click(&pick_btn, move || {
            let _ = input_for_pick.click();
        });
    }

    // 6. "Load demo recording" synthesizes + analyzes without a file.
    {
        let outcome_for_demo = outcome.clone();
        #[cfg(feature = "pro")]
        let document_for_demo = document.clone();
        #[cfg(feature = "pro")]
        let (machine_for_demo, rpm_for_demo, balls_for_demo, ball_dia_for_demo, pitch_dia_for_demo, angle_for_demo) = (
            machine_input.clone(),
            rpm_input.clone(),
            balls_input.clone(),
            ball_dia_input.clone(),
            pitch_dia_input.clone(),
            angle_input.clone(),
        );
        add_click(&demo_btn, move || {
            let rec = demo_recording();
            outcome_for_demo.set(analyze_recording(
                "demo-motor-1797rpm (synthetic)",
                &rec,
                #[cfg(feature = "pro")]
                &machine_for_demo,
                #[cfg(feature = "pro")]
                &rpm_for_demo,
                #[cfg(feature = "pro")]
                &balls_for_demo,
                #[cfg(feature = "pro")]
                &ball_dia_for_demo,
                #[cfg(feature = "pro")]
                &pitch_dia_for_demo,
                #[cfg(feature = "pro")]
                &angle_for_demo,
                #[cfg(feature = "pro")]
                &document_for_demo,
            ));
        });
    }

    // 7. File input change: read bytes, decode, analyze. The outer closure
    //    borrows its captures (cloning per call); the onload closure is
    //    constructed fresh per file and owns its own clones.
    {
        let outcome_for_change = outcome.clone();
        let input_for_change = file_input.clone();
        let rate_for_change = rate_input.clone();
        #[cfg(feature = "pro")]
        let document_for_change = document.clone();
        #[cfg(feature = "pro")]
        let (machine_for_change, rpm_for_change, balls_for_change, ball_dia_for_change, pitch_dia_for_change, angle_for_change) = (
            machine_input.clone(),
            rpm_input.clone(),
            balls_input.clone(),
            ball_dia_input.clone(),
            pitch_dia_input.clone(),
            angle_input.clone(),
        );
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
            let Some(files) = input_for_change.files() else { return };
            let Some(file) = files.get(0) else { return };
            let name = file.name();
            let Ok(reader) = web_sys::FileReader::new() else { return };
            let reader_for_load = reader.clone();
            let outcome_for_load = outcome_for_change.clone();
            let rate_for_load = rate_for_change.clone();
            #[cfg(feature = "pro")]
            let document_for_load = document_for_change.clone();
            #[cfg(feature = "pro")]
            let (machine_for_load, rpm_for_load, balls_for_load, ball_dia_for_load, pitch_dia_for_load, angle_for_load) = (
                machine_for_change.clone(),
                rpm_for_change.clone(),
                balls_for_change.clone(),
                ball_dia_for_change.clone(),
                pitch_dia_for_change.clone(),
                angle_for_change.clone(),
            );
            let onload =
                Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
                    let loaded = (|| -> Result<Vec<u8>, JsValue> {
                        let buffer = reader_for_load
                            .result()?
                            .dyn_into::<js_sys::ArrayBuffer>()?;
                        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
                    })();
                    match loaded.and_then(|bytes| {
                        decode::decode_auto(&bytes, read_rate(&rate_for_load))
                            .map_err(|e| JsValue::from_str(&e.to_string()))
                    }) {
                        Ok(rec) => outcome_for_load.set(analyze_recording(
                            &name,
                            &rec,
                            #[cfg(feature = "pro")]
                            &machine_for_load,
                            #[cfg(feature = "pro")]
                            &rpm_for_load,
                            #[cfg(feature = "pro")]
                            &balls_for_load,
                            #[cfg(feature = "pro")]
                            &ball_dia_for_load,
                            #[cfg(feature = "pro")]
                            &pitch_dia_for_load,
                            #[cfg(feature = "pro")]
                            &angle_for_load,
                            #[cfg(feature = "pro")]
                            &document_for_load,
                        )),
                        Err(err) => outcome_for_load.set(Outcome::Error(
                            err.as_string().unwrap_or_else(|| "unreadable file".to_string()),
                        )),
                    }
                });
            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
            onload.forget();
            if let Err(err) = reader.read_as_array_buffer(&file) {
                outcome_for_change.set(Outcome::Error(format!("could not open file: {err:?}")));
            }
        });
        file_input
            .add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())
            .map_err(|e| e)?;
        closure.forget();
    }

    // 8. (pro) Export buttons: report and trend CSV from the last analysis.
    #[cfg(feature = "pro")]
    {
        let outcome_for_report = outcome.clone();
        let document_for_report = document.clone();
        add_click(&query::<web_sys::HtmlElement>(container, ".vib-export-report")?, move || {
            export_report(&outcome_for_report.get(), &document_for_report);
        });
        let outcome_for_trend = outcome.clone();
        add_click(&query::<web_sys::HtmlElement>(container, ".vib-export-trend")?, move || {
            export_trend(&outcome_for_trend.get());
        });
    }

    // 9. View select -> view signal (0 = full Nyquist).
    {
        let view = view_max_hz.clone();
        let select_for_change = view_select.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
            view.set(select_for_change.value().parse::<f64>().unwrap_or(0.0));
        });
        view_select
            .add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())
            .map_err(|e| e)?;
        closure.forget();
    }

    // 10. Redraw effect: outcome or view changes repaint everything.
    {
        let outcome = outcome.clone();
        let view = view_max_hz.clone();
        let container = container.clone();
        let results_card = results_card.clone();
        let status_box = status_box.clone();
        let stats_box = stats_box.clone();
        let peaks_box = peaks_box.clone();
        let spectrum_canvas = spectrum_canvas.clone();
        #[cfg(feature = "pro")]
        let (envelope_canvas, trend_canvas, fault_box, bearing_box) =
            (envelope_canvas.clone(), trend_canvas.clone(), fault_box.clone(), bearing_box.clone());
        let handle = create_effect(move || {
            let view_max = view.get();
            let state = outcome.get();
            let theme = chart::read_theme(&container);

            match &state {
                Outcome::Idle => {
                    results_card.style().set_property("display", "none").ok();
                    status_box.style().set_property("display", "block").ok();
                    status_box.set_class_name("vib-status");
                    status_box.set_text_content(Some("Load a recording to see its spectrum."));
                }
                Outcome::Error(message) => {
                    results_card.style().set_property("display", "none").ok();
                    status_box.style().set_property("display", "block").ok();
                    status_box.set_class_name("vib-status vib-err");
                    status_box.set_text_content(Some(&format!("Could not analyze: {message}")));
                }
                Outcome::Analyzed(a) => {
                    status_box.style().set_property("display", "none").ok();
                    results_card.style().set_property("display", "block").ok();

                    render_stats(&stats_box, a);
                    render_notes(&status_box, a);
                    render_peaks(&peaks_box, a);

                    let nyquist = a.spectrum.sample_rate / 2.0;
                    let max_hz = if view_max > 0.0 {
                        view_max.min(nyquist)
                    } else {
                        nyquist
                    };
                    let marks: Vec<(f64, String)> = a
                        .peaks
                        .iter()
                        .take(6)
                        .map(|p| {
                            (
                                p.freq_hz,
                                if p.freq_hz >= 1000.0 {
                                    format!("{:.2} kHz", p.freq_hz / 1000.0)
                                } else {
                                    format!("{:.1} Hz", p.freq_hz)
                                },
                            )
                        })
                        .collect();
                    chart::draw_spectrum(
                        &spectrum_canvas,
                        &theme,
                        &a.spectrum.freq_hz,
                        &a.spectrum.amp,
                        max_hz,
                        &marks,
                    );

                    #[cfg(feature = "pro")]
                    {
                        render_faults(&fault_box, a);
                        render_bearing(&bearing_box, &envelope_canvas, a, &theme);
                        render_trend(&trend_canvas, a, &theme);
                    }
                }
            }
        });
        std::mem::forget(handle);
    }

    Ok(())
}

// ------------------------------------------------------------- shell ----

/// The mounted-once shell: header, input card, status line, results card
/// (re-filled by the redraw effect), upsell (free) / exports (pro), footer.
fn shell_tree() -> UITree<Msg> {
    UITree::container(|c| {
        c.container(|header| {
            header.text("VA").class("vib-brand");
            header.container(|group| {
                group.container(|row| {
                    row.heading(2, "TPT Vibration Analyzer");
                    #[cfg(not(feature = "pro"))]
                    row.text("Free edition").class("vib-chip");
                    #[cfg(feature = "pro")]
                    row.text("Pro").class("vib-chip");
                })
                .class("vib-title-row");
                group.text("FFT spectrum of an uploaded vibration recording — analyzed locally, nothing is uploaded.")
                    .class("vib-sub");
            })
            .class("vib-title-group");
        })
        .class("vib-header");

        // Input card.
        c.container(|card| {
            card.heading(3, "Recording");
            card.container(|row| {
                row.button("Choose recording\u{2026}")
                    .class("vib-pick");
                row.button("Load demo recording").class("vib-demo");
                row.container(|field| {
                    field.container(|f| { f.text("Sample rate, Hz (CSV without time column)"); }).class("vib-label");
                    f_input(field, "8192", "vib-rate");
                })
                .class("vib-field");
                row.container(|field| {
                    field.container(|f| { f.text("Spectrum view"); }).class("vib-label");
                    field
                        .select(
                            [
                                ("0", "Full range"),
                                ("1000", "0-1 kHz"),
                                ("2000", "0-2 kHz"),
                                ("4000", "0-4 kHz"),
                            ],
                            "0",
                        )
                        .class("vib-view");
                })
                .class("vib-field");
            })
            .class("vib-row");

            // (pro) machine + bearing inputs for fault detection.
            #[cfg(feature = "pro")]
            {
                card.container(|row| {
                    row.container(|field| {
                        field.container(|f| { f.text("Machine / asset tag"); }).class("vib-label");
                        f_input(field, "", "vib-machine");
                    })
                    .class("vib-field");
                    row.container(|field| {
                        field.container(|f| { f.text("Shaft speed (RPM)"); }).class("vib-label");
                        f_input(field, "1797", "vib-rpm");
                    })
                    .class("vib-field");
                })
                .class("vib-row");
                card.container(|row| {
                    row.container(|field| {
                        field.container(|f| { f.text("Bearing: balls"); }).class("vib-label");
                        f_input(field, "9", "vib-balls");
                    })
                    .class("vib-field");
                    row.container(|field| {
                        field.container(|f| { f.text("ball dia (mm)"); }).class("vib-label");
                        f_input(field, "7.94", "vib-ball-dia");
                    })
                    .class("vib-field");
                    row.container(|field| {
                        field.container(|f| { f.text("pitch dia (mm)"); }).class("vib-label");
                        f_input(field, "39.04", "vib-pitch-dia");
                    })
                    .class("vib-field");
                    row.container(|field| {
                        field.container(|f| { f.text("contact angle (deg)"); }).class("vib-label");
                        f_input(field, "0", "vib-angle");
                    })
                    .class("vib-field");
                })
                .class("vib-row");
                card.text("Fault detection, trend logging and reports are Pro features — this build has them compiled in. Bearing defaults describe a 6205 deep-groove ball bearing.")
                    .class("vib-hint");
            }
            #[cfg(not(feature = "pro"))]
            card.text("WAV (16/24/32-bit PCM or float) or CSV (time,value per row — or a single value column plus the sample rate). Multi-channel files are averaged to mono. All analysis runs locally in your browser.")
                .class("vib-hint");
        })
        .class("vib-card");

        // Status line (decoding notes / errors), then results card.
        c.container(|s| {
            s.text("Load a recording to see its spectrum.");
        })
        .class("vib-status");

        c.container(|card| {
            card.heading(3, "Spectrum");
            card.container(|_stats| {}).class("vib-stats");
            card.container(|_slot| {}).class("vib-slot vib-spectrum-slot");
            card.container(|_peaks| {}).class("vib-peaks");

            // (pro) diagnostics sections, shown with the results.
            #[cfg(feature = "pro")]
            {
                card.container(|f| {
                    f.heading(3, "Fault signatures");
                    f.container(|_b| {}).class("vib-faults");
                })
                .class("vib-card");
                card.container(|f| {
                    f.heading(3, "Bearing envelope");
                    f.container(|_b| {}).class("vib-bearing");
                    f.container(|_s| {}).class("vib-slot vib-envelope-slot");
                })
                .class("vib-card");
                card.container(|f| {
                    f.heading(3, "Trend over recordings");
                    f.container(|_t| {}).class("vib-trend");
                })
                .class("vib-card");
                card.container(|ex| {
                    ex.button("Download maintenance report (.html)").class("vib-export-report");
                    ex.button("Download trend log (.csv)").class("vib-export-trend");
                })
                .class("vib-export");
            }
        })
        .class("vib-card vib-results");

        // Free-tier upsell (Pro builds get the real panels instead).
        #[cfg(not(feature = "pro"))]
        c.container(|up| {
            up.container(|copy| {
                copy.heading(3, "Go Pro: from spectrum to diagnosis");
                for bullet in [
                    "Fault-signature detection — bearing wear, imbalance, misalignment",
                    "Trend logging — track every recording per machine over time",
                    "Exportable maintenance reports (print/PDF-ready HTML + CSV)",
                ] {
                    copy.container(|li| {
                        li.text(format!("\u{2022} {bullet}"));
                    })
                    .class("vib-li");
                }
            })
            .class("vib-upsell-copy");
            up.button("Get the Pro edition — $199 one-time")
                .class("vib-primary")
                .on_click(Msg::VisitSite(TOOLS_URL.to_string()));
        })
        .class("vib-upsell");

        c.container(|footer| {
            footer.text("TPT Vibration Analyzer \u{b7} free edition \u{b7} ");
            footer
                .button("tptsolutions.co.nz")
                .class("vib-link")
                .on_click(Msg::VisitSite(TOOLS_URL.to_string()));
        })
        .class("vib-footer");
    })
}

fn f_input(c: &mut tpt_appfront_core::ContainerBuilder<Msg>, value: &str, class: &str) {
    c.input(value).class(format!("vib-in {class}"));
}

fn query<T: JsCast>(container: &web_sys::Element, selector: &str) -> Result<T, JsValue> {
    container
        .query_selector(selector)
        .map_err(|e| e)?
        .and_then(|el| el.dyn_into::<T>().ok())
        .ok_or_else(|| JsValue::from_str(&format!("missing element {selector}")))
}

/// Wires a plain click listener that borrows nothing but its own closure —
/// the escape hatch for buttons whose work needs engine access rather than
/// a tree message.
fn add_click<F: FnMut() + 'static>(target: &web_sys::HtmlElement, mut f: F) {
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
        f();
    });
    let _ = target.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn read_rate(input: &web_sys::HtmlInputElement) -> Option<f64> {
    input.value().trim().parse::<f64>().ok().filter(|r| *r > 0.0)
}

// ----------------------------------------------------------- analyze ----

/// Analyzes one recording and returns the outcome for the results card.
/// The pro build additionally runs fault detection and appends to the
/// machine's persisted trend log.
fn analyze_recording(
    name: &str,
    rec: &Recording,
    #[cfg(feature = "pro")] machine_input: &web_sys::HtmlInputElement,
    #[cfg(feature = "pro")] rpm_input: &web_sys::HtmlInputElement,
    #[cfg(feature = "pro")] balls_input: &web_sys::HtmlInputElement,
    #[cfg(feature = "pro")] ball_dia_input: &web_sys::HtmlInputElement,
    #[cfg(feature = "pro")] pitch_dia_input: &web_sys::HtmlInputElement,
    #[cfg(feature = "pro")] angle_input: &web_sys::HtmlInputElement,
    #[cfg(feature = "pro")] document: &web_sys::Document,
) -> Outcome {
    let spec = spectrum::analyze(rec, 8192);
    let nyquist = spec.sample_rate / 2.0;
    let peaks = spectrum::peaks(&spec, 1.0, nyquist * 0.98, 8, 0.02);
    let stats = spectrum::stats(rec);

    #[cfg(feature = "pro")]
    {
        let machine = machine_input.value().trim().to_string();
        let rpm = rpm_input.value().trim().parse::<f64>().ok().unwrap_or(0.0);
        let geometry = faults::BearingGeometry {
            balls: balls_input.value().trim().parse().ok().unwrap_or(9).max(1),
            ball_dia_mm: ball_dia_input.value().trim().parse().ok().unwrap_or(7.94_f64).max(0.1),
            pitch_dia_mm: pitch_dia_input.value().trim().parse().ok().unwrap_or(39.04_f64).max(0.1),
            contact_angle_deg: angle_input.value().trim().parse().ok().unwrap_or(0.0),
        };
        let report = faults::analyze(
            rec,
            &FaultInput {
                rpm,
                bearing: geometry,
                demod_band: None,
            },
        );

        let row = TrendRow {
            date: today(&document),
            label: name.to_string(),
            rms: stats.rms,
            imbalance: report.imbalance.value,
            misalignment: report.misalignment.value,
            bearing: report.bearing.score.value,
        };
        let mut trend = load_trend(&document, &machine);
        trend.push(row);
        trend.truncate(200);
        save_trend(&document, &machine, &trend);

        return Outcome::Analyzed(Rc::new(Analysis {
            name: name.to_string(),
            notes: rec.notes.clone(),
            stats,
            spectrum: Rc::new(spec),
            peaks,
            faults: Rc::new(report),
            trend: Rc::new(trend),
            machine,
        }));
    }

    #[cfg(not(feature = "pro"))]
    Outcome::Analyzed(Rc::new(Analysis {
        name: name.to_string(),
        notes: rec.notes.clone(),
        stats,
        spectrum: Rc::new(spec),
        peaks,
    }))
}

/// The synthetic demo: a 1797 rpm motor with a 1×-dominant (imbalance-style)
/// spectrum plus a line-frequency component and a resonance carrier band.
fn demo_recording() -> Recording {
    let shaft = 1797.0 / 60.0;
    tpt_vibration_engine::synth::recording(tpt_vibration_engine::synth::Spec {
        sample_rate: 8192.0,
        seconds: 2.0,
        tones: vec![
            tpt_vibration_engine::synth::Tone { freq: shaft, amp: 1.0, phase: 0.0 },
            tpt_vibration_engine::synth::Tone { freq: 2.0 * shaft, amp: 0.12, phase: 0.7 },
            tpt_vibration_engine::synth::Tone { freq: 3.0 * shaft, amp: 0.05, phase: 0.2 },
            tpt_vibration_engine::synth::Tone { freq: 120.0, amp: 0.15, phase: 0.0 },
            tpt_vibration_engine::synth::Tone { freq: 1900.0, amp: 0.08, phase: 0.0 },
        ],
        noise: 0.05,
        noise_seed: 42,
        ..Default::default()
    })
}

// ---------------------------------------------------------- rendering ----

fn render_stats(box_: &web_sys::HtmlElement, a: &Analysis) {
    let s = &a.stats;
    box_.set_inner_html(&format!(
        "<div class=\"vib-stat\"><b>{}</b><span>recording</span></div>\
         <div class=\"vib-stat\"><b>{:.0} Hz</b><span>sample rate</span></div>\
         <div class=\"vib-stat\"><b>{:.2} s</b><span>duration</span></div>\
         <div class=\"vib-stat\"><b>{:.4}</b><span>RMS</span></div>\
         <div class=\"vib-stat\"><b>{:.4}</b><span>peak</span></div>\
         <div class=\"vib-stat\"><b>{:.2}</b><span>crest factor</span></div>",
        html_escape(&a.name),
        a.spectrum.sample_rate,
        s.duration_s,
        s.rms,
        s.peak,
        s.crest_factor,
    ));
}

/// The status line doubles as the decoding-notes list once a recording is
/// analyzed (channel averaging, rate inference, ...).
fn render_notes(box_: &web_sys::HtmlElement, a: &Analysis) {
    if a.notes.is_empty() {
        box_.style().set_property("display", "none").ok();
        return;
    }
    box_.style().set_property("display", "block").ok();
    box_.set_class_name("vib-status");
    let items: String = a
        .notes
        .iter()
        .map(|n| format!("<li>{}</li>", html_escape(n)))
        .collect();
    box_.set_inner_html(&format!("<ul>{items}</ul>"));
}

fn render_peaks(box_: &web_sys::HtmlElement, a: &Analysis) {
    let rows: String = a
        .peaks
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "<tr><td>{}</td><td class=\"num\">{:.1}</td><td class=\"num\">{:.4}</td></tr>",
                i + 1,
                p.freq_hz,
                p.amp
            )
        })
        .collect();
    box_.set_inner_html(&format!(
        "<table><tr><th>#</th><th>Frequency (Hz)</th><th>Amplitude</th></tr>{rows}</table>\
         <p class=\"vib-hint\">Amplitudes are in the recording's own units (g or m/s\u{b2}) — the analysis is unit-agnostic.</p>"
    ));
}

// ------------------------------------------------- pro rendering/exports ----

#[cfg(feature = "pro")]
fn severity_pill(sev: Severity) -> String {
    format!("<span class=\"vib-pill {}\">{}</span>", sev.label(), sev.label())
}

#[cfg(feature = "pro")]
fn render_faults(box_: &web_sys::HtmlElement, a: &Analysis) {
    let f = &a.faults;
    let row = |name: &str, score: &tpt_vibration_engine::faults::FaultScore| {
        format!(
            "<div class=\"vib-fault\"><b>{name}</b>{}<span class=\"vib-detail\">{}</span></div>",
            severity_pill(score.severity),
            html_escape(&score.detail)
        )
    };
    box_.set_inner_html(&format!(
        "{}{}{}",
        row("Imbalance", &f.imbalance),
        row("Misalignment", &f.misalignment),
        row("Bearing wear", &f.bearing.score),
    ));
}

#[cfg(feature = "pro")]
fn render_bearing(
    box_: &web_sys::HtmlElement,
    canvas: &web_sys::HtmlCanvasElement,
    a: &Analysis,
    theme: &chart::Theme,
) {
    let b = &a.faults.bearing;
    let freqs = format!(
        "<p class=\"vib-hint\">Characteristic frequencies at {:.1} Hz shaft: FTF {:.1} \u{b7} BPFO {:.1} \u{b7} BPFI {:.1} \u{b7} BSF {:.1} Hz — demodulated over {:.0}-{:.0} Hz.</p>",
        a.faults.shaft_hz,
        b.freqs.ftf,
        b.freqs.bpfo,
        b.freqs.bpfi,
        b.freqs.bsf,
        b.band.0,
        b.band.1
    );
    let hits = if b.hits.is_empty() {
        "<p class=\"vib-hint\">No prominent bearing tones in the envelope spectrum.</p>".to_string()
    } else {
        let rows: String = b
            .hits
            .iter()
            .map(|h| {
                format!(
                    "<tr><td>{}</td><td class=\"num\">{:.1}</td><td class=\"num\">{:.0}\u{d7}</td><td>{}</td></tr>",
                    h.component,
                    h.freq_hz,
                    h.prominence,
                    severity_pill(h.severity)
                )
            })
            .collect();
        format!(
            "<table><tr><th>Component</th><th class=\"num\">Hz</th><th class=\"num\">Prominence</th><th>Status</th></tr>{rows}</table>"
        )
    };
    box_.set_inner_html(&format!("{freqs}{hits}"));

    let env = &b.envelope;
    let max_cf = b
        .freqs
        .bpfi
        .max(b.freqs.bsf * 2.0)
        .max(b.freqs.bpfo)
        .max(100.0);
    let max_hz = (max_cf * 1.6).min(env.sample_rate / 2.0);
    let marks: Vec<(f64, String)> = b
        .hits
        .iter()
        .map(|h| (h.freq_hz, h.component.to_string()))
        .collect();
    chart::draw_spectrum(canvas, theme, &env.freq_hz, &env.amp, max_hz, &marks);
}

#[cfg(feature = "pro")]
fn render_trend(canvas: &web_sys::HtmlCanvasElement, a: &Analysis, theme: &chart::Theme) {
    if a.trend.len() < 2 {
        if let Some(p) = chart::begin_plot(canvas) {
            p.ctx.set_font("12px sans-serif");
            p.ctx.set_fill_style_str(&theme.muted);
            let _ = p
                .ctx
                .fill_text("Analyze a second recording (same machine tag) to start the trend.", 12.0, 24.0);
        }
        return;
    }
    // RMS line over recording index; newest recording annotated with its date.
    let Some(p) = chart::begin_plot(canvas) else { return };
    let n = a.trend.len();
    let max_rms = a.trend.iter().map(|r| r.rms).fold(0.0f64, |acc, v| acc.max(v)).max(1e-9);
    let ymax = max_rms * 1.1;
    let ty = chart::ticks(0.0, ymax, 4);
    let ticks_x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let (px0, py0, pw, ph) = chart::draw_axes(
        &p,
        theme,
        (0.0, (n - 1) as f64),
        (0.0, ymax),
        &ticks_x,
        &ty,
        &|v| format!("{}", v.round() as usize + 1),
        &|v| format!("{v:.3}"),
    );
    let sx = |i: f64| px0 + i / (n - 1).max(1) as f64 * pw;
    let sy = |v: f64| py0 + ph - v / ymax * ph;
    p.ctx.set_stroke_style_str(&theme.accent);
    p.ctx.set_line_width(1.6);
    let _ = p.ctx.begin_path();
    for (i, r) in a.trend.iter().enumerate() {
        let (x, y) = (sx(i as f64), sy(r.rms));
        if i == 0 {
            let _ = p.ctx.move_to(x.into(), y.into());
        } else {
            let _ = p.ctx.line_to(x.into(), y.into());
        }
    }
    let _ = p.ctx.stroke();
    p.ctx.set_fill_style_str(&theme.accent);
    for (i, r) in a.trend.iter().enumerate() {
        let _ = p.ctx.begin_path();
        let _ = p.ctx.arc(sx(i as f64).into(), sy(r.rms).into(), 2.5.into(), 0.0, std::f64::consts::TAU);
        let _ = p.ctx.fill();
    }
    let last = a.trend.last().expect("checked non-empty");
    p.ctx.set_font("10px sans-serif");
    p.ctx.set_text_align("right");
    p.ctx.set_text_baseline("top");
    let _ = p
        .ctx
        .fill_text(&html_escape(&last.date), (sx((n - 1) as f64) - 4.0).into(), (py0 + 2.0).into());
}

#[cfg(feature = "pro")]
fn export_report(state: &Outcome, document: &web_sys::Document) {
    let Outcome::Analyzed(a) = state else { return };
    let date = today(document);
    let machine = if a.machine.is_empty() { a.name.clone() } else { a.machine.clone() };
    let html = report::maintenance_report_html(
        &machine,
        &date,
        &a.notes,
        &a.stats,
        &a.peaks,
        a.faults.shaft_hz,
        &a.faults,
        &a.faults.bearing,
        "units",
        &a.trend,
    );
    download(
        &format!("vibration-report-{}-{date}.html", slug(&machine)),
        "text/html",
        html.as_bytes(),
    );
}

#[cfg(feature = "pro")]
fn export_trend(state: &Outcome) {
    let Outcome::Analyzed(a) = state else { return };
    let csv = report::trend_csv(&a.trend);
    download(
        &format!("vibration-trend-{}.csv", slug(&a.machine)),
        "text/csv",
        csv.as_bytes(),
    );
}

/// Hands `bytes` to the user as a file download (Blob → object URL →
/// programmatic anchor click), working in both the browser tab and the
/// desktop webview.
#[cfg_attr(not(feature = "pro"), allow(dead_code))]
pub fn download(filename: &str, mime: &str, bytes: &[u8]) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes));
    let bag = {
        let b = web_sys::BlobPropertyBag::new();
        b.set_type(mime);
        b
    };
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &bag) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(anchor) = document.create_element("a") {
        let _ = anchor.set_attribute("href", &url);
        let _ = anchor.set_attribute("download", filename);
        if let Ok(a) = anchor.dyn_into::<web_sys::HtmlAnchorElement>() {
            if let Some(body) = document.body() {
                let _ = body.append_child(&a);
                a.click();
                let _ = body.remove_child(&a);
            }
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

#[cfg(feature = "pro")]
fn today(document: &web_sys::Document) -> String {
    // ISO date via JS Date (wasm has no local-time-zone database); the
    // document is only here to make the caller's context explicit.
    let _ = document;
    let iso = js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default();
    if iso.len() >= 10 {
        iso[0..10].to_string()
    } else {
        iso
    }
}

#[cfg(feature = "pro")]
fn slug(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "machine".to_string()
    } else {
        trimmed.to_string()
    }
}

// -------------------------------------------------------- trend store ----

/// The machine's persisted trend log, kept in the host's localStorage
/// (persists across desktop-webview restarts; the free edition never runs
/// this code). Entries: `{"d":date,"l":label,"r":rms,"i":…,"m":…,"b":…}`.
#[cfg(feature = "pro")]
fn trend_key(machine: &str) -> String {
    format!("vib-trend-{}", slug(machine))
}

#[cfg(feature = "pro")]
fn load_trend(document: &web_sys::Document, machine: &str) -> Vec<TrendRow> {
    if machine.is_empty() {
        return Vec::new();
    }
    let Some(window) = document.default_view() else { return Vec::new() };
    let Ok(Some(storage)) = window.local_storage() else { return Vec::new() };
    let Ok(Some(text)) = storage.get_item(&trend_key(machine)) else { return Vec::new() };
    let Ok(value) = js_sys::JSON::parse(&text) else { return Vec::new() };
    let Ok(array) = value.dyn_into::<js_sys::Array>() else { return Vec::new() };
    array
        .iter()
        .filter_map(|entry| {
            let obj = entry.dyn_into::<js_sys::Object>().ok()?;
            let get = |key: &str| js_sys::Reflect::get(&obj, &JsValue::from_str(key)).ok();
            Some(TrendRow {
                date: get("d")?.as_string().unwrap_or_default(),
                label: get("l")?.as_string().unwrap_or_default(),
                rms: get("r")?.as_f64().unwrap_or(0.0),
                imbalance: get("i")?.as_f64().unwrap_or(0.0),
                misalignment: get("m")?.as_f64().unwrap_or(0.0),
                bearing: get("b")?.as_f64().unwrap_or(0.0),
            })
        })
        .collect()
}

#[cfg(feature = "pro")]
fn save_trend(document: &web_sys::Document, machine: &str, trend: &[TrendRow]) {
    if machine.is_empty() {
        return; // no machine tag → nothing persisted
    }
    let Some(window) = document.default_view() else { return };
    let Ok(Some(storage)) = window.local_storage() else { return };
    let entries = js_sys::Array::new();
    for r in trend {
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("d"), &JsValue::from_str(&r.date));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("l"), &JsValue::from_str(&r.label));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("r"), &JsValue::from_f64(r.rms));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("i"), &JsValue::from_f64(r.imbalance));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("m"), &JsValue::from_f64(r.misalignment));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("b"), &JsValue::from_f64(r.bearing));
        entries.push(&obj);
    }
    let text = js_sys::JSON::stringify(&entries)
        .ok()
        .and_then(|j| j.as_string())
        .unwrap_or_default();
    let _ = storage.set_item(&trend_key(machine), &text);
}

// -------------------------------------------------------------- misc ----

/// Opens a TPT Solutions URL from wherever the app happens to be running.
///
/// The free edition's wasm runs directly in a browser tab, where a normal
/// new-tab open is correct. The Pro edition's *identical* wasm bundle is
/// instead served over the desktop shell's `app://localhost` custom protocol
/// (see `desktop/src/main.rs`) — there, `window.open`/navigation would
/// replace the app inside the native window with no browser chrome and no
/// way back. Runtime protocol sniffing (rather than `#[cfg(feature =
/// "pro")]`) is what actually distinguishes the two, since it's the hosting
/// context that matters, not the build.
fn visit_site(url: &str) {
    let Some(window) = web_sys::window() else { return };
    let in_desktop_shell = window
        .location()
        .protocol()
        .map(|p| p == "app:")
        .unwrap_or(false);

    if in_desktop_shell {
        // Routed through the desktop host's allowlisted IPC bridge
        // (`window.__appfront.post`, injected by tpt-appfront-webview) so the
        // *native* process opens the OS default browser, leaving the app
        // window untouched.
        if let Ok(appfront) = js_sys::Reflect::get(&window, &JsValue::from_str("__appfront")) {
            if let Ok(post) = js_sys::Reflect::get(&appfront, &JsValue::from_str("post")) {
                if let Ok(post_fn) = post.dyn_into::<js_sys::Function>() {
                    let params = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &params,
                        &JsValue::from_str("url"),
                        &JsValue::from_str(url),
                    );
                    let _ = post_fn.call2(&appfront, &JsValue::from_str("open_external"), &params);
                }
            }
        }
    } else {
        let _ = window.open_with_url_and_target(url, "_blank");
    }
}

#[cfg_attr(not(feature = "pro"), allow(dead_code))]
fn html_escape(s: &str) -> String {    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
