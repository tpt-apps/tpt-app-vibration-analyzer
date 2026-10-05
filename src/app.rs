//! Free WASM edition UI (tpt-appfront-dom).
//!
//! Phase 0 scaffold proof: the hub contract works end to end (mount into a
//! provided container; standalone `#tpt-appfront-root` mount). Phase 2
//! replaces this with the real upload/FFT interface.

use tpt_appfront_core::UITree;

const BASE_CSS: &str = r#"
.vib-app{--vib-fg:#181d27;--vib-muted:#667085;--vib-accent:#0e7c6b;max-width:1100px;margin:0 auto;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;color:var(--vib-fg);padding:16px 4px 32px}
.vib-app .vib-sub{color:var(--vib-muted);margin-top:2px}
.vib-app .vib-placeholder{margin-top:16px;padding:16px;border:1px dashed #cbd2dc;border-radius:8px;color:var(--vib-muted)}
"#;

pub fn mount_app(container: &web_sys::Element) -> Result<(), wasm_bindgen::JsValue> {
    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    super::upsert_style(&document, "vib-styles", BASE_CSS);
    let _ = container.set_attribute("class", "vib-app");

    let ui: UITree<Msg> = UITree::container(|c| {
        c.heading(1, "TPT Vibration Analyzer");
        c.text("FFT spectrum view of an uploaded vibration recording — scaffold build.");
    });

    let dispatch: std::rc::Rc<dyn Fn(Msg)> = std::rc::Rc::new(|_msg: Msg| {});
    tpt_appfront_dom::mount(container, &ui, dispatch)?;
    Ok(())
}

#[derive(Debug, Clone)]
enum Msg {}
