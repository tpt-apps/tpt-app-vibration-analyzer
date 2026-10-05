# TPT Vibration Analyzer — Build Todo

> Hub listing: `tptsolutions.co.nz/tools/vibration-analyzer` (registry slug
> `vibration-analyzer`, category `engineering`). Spec: `apps.md` §T1-11 in the
> tpt-electrician-nz-2 repo. Free WASM edition: basic FFT view of an uploaded
> vibration recording. Paid offline edition ($99 USD, Gumroad): fault-signature
> detection (bearing/imbalance/misalignment), trend logging over time, and
> exportable maintenance reports. Solver wraps `tpt-dsp`. UI on `tpt-appfront`.
>
> Hub contract: `wasm-bindgen --target web` glue exposes `mount(container)`
> (called by `WasmAppRunner` after `default()` init). Standalone `trunk serve`
> mounts into the `#tpt-appfront-root` marker div instead — never
> `document.body` — so the two hosts never double-mount.

## Phase 0 — Scaffold & pipeline proof
- [ ] `tpt-appfront init tpt-app-vibration-analyzer --target dom` (path deps on the tpt-appfront checkout)
- [ ] Restructure scaffold to hub contract: `#[wasm_bindgen] pub fn mount(el)` + `#[wasm_bindgen(start)]` no-op unless `#tpt-appfront-root` present
- [ ] `index.html` carries the `#tpt-appfront-root` marker div
- [ ] Build pipeline script mirroring fea-lite/cutlist-optimizer's (`--target web` glue → web repo `public/apps/vibration-analyzer/`)
- [ ] Hello-world build loads through the standalone dev page (`trunk serve`)
- [ ] git init + first commit

## Phase 1 — Solver integration (wrap tpt-dsp)
- [ ] Survey `tpt-dsp` for FFT/spectral-analysis primitives and any existing
      windowing/filtering support needed for clean spectra from noisy sensor data
- [ ] Adapter crate `engine/`: raw accelerometer time series in → FFT spectrum,
      fault-signature scores (bearing/imbalance/misalignment frequency bands)
      out as plain structs — no UI deps, host-unit-testable
- [ ] Validation against known synthetic signals (pure tone, known fault
      frequency injected into noise) to confirm detection accuracy
- [ ] Decide input format(s) supported (WAV/CSV from standard USB
      accelerometer capture tools)

## Phase 2 — Free WASM UI
- [ ] File upload for a vibration recording + basic FFT/spectrum view
- [ ] Free-tier upsell copy pointing at fault-signature detection, trend
      logging, exportable reports
- [ ] Browser-verified: mount, upload, FFT renders correctly

## Phase 3 — Paid desktop edition (cargo feature `pro`)
- [ ] `pro` gates: fault-signature detection (bearing wear, imbalance,
      misalignment), trend logging over time (multiple recordings per
      machine), exportable maintenance reports
- [ ] Pro UI: fault-detection results panel, trend chart, machine log,
      export button
- [ ] Desktop shell via `tpt-appfront-webview`; Windows exe build + smoke test
- [ ] Gumroad product from apps.md listing copy ($99), exe upload, URL into
      registry — manual step, listing copy at apps.md §T1-11

## Phase 4 — Ship & measure
- [ ] Registry entry (`engineering` category, `wasm: { entry: '/apps/vibration-analyzer/vibration-analyzer.js' }`)
      per the "Adding a finished app to the hub" runbook at the bottom of apps.md
- [ ] Verify hub card/detail/sitemap/runner in dev, then prod after deploy
- [ ] WASM size budget check on the shipped build
