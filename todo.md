# TPT Vibration Analyzer — Build Todo

> Hub listing: `tptsolutions.co.nz/tools/vibration-analyzer` (registry slug
> `vibration-analyzer`, category `engineering`). Spec: `apps.md` §T1-11 in the
> tpt-electrician-nz-2 repo. Free WASM edition: basic FFT view of an uploaded
> vibration recording. Paid offline edition ($199 USD, Gumroad): fault-signature
> detection (bearing/imbalance/misalignment), trend logging over time, and
> exportable maintenance reports. Solver wraps `tpt-dsp`. UI on `tpt-appfront`.
>
> Hub contract: `wasm-bindgen --target web` glue exposes `mount(container)`
> (called by `WasmAppRunner` after `default()` init). Standalone `trunk serve`
> mounts into the `#tpt-appfront-root` marker div instead — never
> `document.body` — so the two hosts never double-mount.
>
> **tpt-dsp note:** the foundation is still design-only
> (tpt-foundations/17-tpt-dsp.md — no implementation repo exists; tpt-audio's
> dsp module has no FFT), so the FFT/windowing primitives live in
> `engine/src/fft.rs` + `engine/src/window.rs` as the swap surface. When
> tpt-dsp lands, they are replaced behind the same public API. Price is $199
> per apps.md §T1-11 and GUMROAD.md (the $99 in earlier revisions of this
> file was stale).

## Phase 0 — Scaffold & pipeline proof
- [x] `tpt-appfront init tpt-app-vibration-analyzer --target dom` (path deps on the tpt-appfront checkout)
- [x] Restructure scaffold to hub contract: `#[wasm_bindgen] pub fn mount(el)` + `#[wasm_bindgen(start)]` no-op unless `#tpt-appfront-root` present
- [x] `index.html` carries the `#tpt-appfront-root` marker div
- [x] Build pipeline script mirroring fea-lite/cutlist-optimizer's (`--target web` glue → web repo `public/apps/vibration-analyzer/`)
- [x] Hello-world build loads through the standalone dev page (`trunk serve`)
- [x] git init + first commit

## Phase 1 — Solver integration (wrap tpt-dsp)
- [x] Survey `tpt-dsp` — **outcome: design-only**, primitives self-contained in the engine (see note above)
- [x] Adapter crate `engine/`: raw accelerometer time series in → FFT spectrum,
      fault-signature scores (bearing/imbalance/misalignment frequency bands)
      out as plain structs — no UI deps, host-unit-testable
      (Welch-averaged Hann spectrum, parabolic peak picking; envelope
      analysis with auto demod band; bearing freq formulas validated
      against 6205 reference values)
- [x] Validation against known synthetic signals — 33 tests green: pure
      tone freq/amplitude, tone-in-noise under Welch averaging, CSV
      misalignment end-to-end, WAV bearing-BPFO roundtrip, healthy
      multi-component signal scores normal on all three detectors
- [x] Input formats: WAV (16/24/32-bit int PCM, 32/64-bit float, extensible;
      multi-channel averaged) + tolerant CSV (time,value / single-column +
      manual rate; `,` `;` tab delimiters; comma decimals)

## Phase 2 — Free WASM UI
- [x] File upload for a vibration recording + basic FFT/spectrum view
      (canvas chart, DPR-aware, peak markers; stats chips; peaks table;
      view range select; demo-recording button)
- [x] Free-tier upsell copy pointing at fault-signature detection, trend
      logging, exportable reports
- [x] Browser-verified: mount, upload (real WAV through the file input),
      FFT renders correctly — found + fixed a real bug (zero-padded short
      recordings read half amplitude)

## Phase 3 — Paid desktop edition (cargo feature `pro`)
- [x] `pro` gates: fault-signature detection (bearing wear, imbalance,
      misalignment), trend logging over time (localStorage per machine tag,
      survives desktop restarts), exportable maintenance reports (HTML +
      trend CSV)
- [x] Pro UI: fault-detection results panel, envelope + trend charts,
      machine log (trend), export buttons
- [x] Desktop shell via `tpt-appfront-webview`; Windows exe build + smoke
      test (launch from a bare cwd → embedded dist extracted, WebView2
      window stable, clean shutdown)
- [ ] Gumroad product from apps.md listing copy ($199), exe upload, URL into
      registry — **manual step**; listing copy in `GUMROAD.md`. Cover
      image: screenshot of the free demo's spectrum, or the pro UI showing
      a flagged fault — no stock/AI art.

## Phase 4 — Ship & measure
- [x] Registry entry (`engineering` category, `wasm: { entry: '/apps/vibration-analyzer/vibration-analyzer.js' }`)
      — added as `coming-soon`; flip to `live` after the hub deploy verifies
      (see below)
- [x] WASM size budget: free build 234 KB raw / **94.9 KB gzipped** (budget
      ~2 MB gz) — pro bundle (1.4 MB raw) never ships to the hub by design
- [ ] Verify hub card/detail/sitemap/runner in dev (`npm run dev` in
      tpt-electrician-nz-2), then prod after deploy — **manual step**; the
      bundle is in `public/apps/vibration-analyzer/` and the runner loads
      `vibration-analyzer.js` exactly like fea-lite's
