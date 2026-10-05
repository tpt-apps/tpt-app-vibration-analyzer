# Gumroad listing

Everything needed to create/update the Gumroad product once this app is
built. Source copy is `apps.md` §T1-11 in the `tpt-electrician-nz-2` repo — if
that changes, update here too (and vice versa).

## Step-by-step: publish this listing

1. Build the Pro edition (`./build.ps1 -Pro`, once the build pipeline exists
   — see `todo.md` and mirror `tpt-app-fea-lite`/`tpt-app-cutlist-optimizer`'s
   pattern) — produces the packaged exe/zip in `release/`.
2. Log in to Gumroad → **New product** → **Digital product**.
3. Set the product name, price, and upload the packaged file (all below).
4. Paste the short description, bullets, and full description (below) into
   their respective fields.
5. Set category/tags (below) and add a cover image (see note below).
6. Publish, then copy the resulting product URL (`https://gum.co/...`).
7. Paste that URL into the `vibration-analyzer` entry's `gumroadUrl` field in
   `tpt-electrician-nz-2\src\lib\tpt-apps-registry.ts`, and set `price: '$199'`
   on the same entry.
8. Flip that entry's `status` to `'live'` once both the free demo and this
   Gumroad listing exist.
9. Commit and push the registry repo.

## File to upload

Not built yet. Once packaged, this will be the Pro exe/zip produced by the
app's `build.ps1 -Pro` (or equivalent) — see `todo.md`.

## Product name

```
TPT Vibration Analyzer — Predictive Maintenance Without a $5,000 Analyzer
```

## Price

**$199 USD, one-time purchase** (no subscription).

## Short description / summary field

```
FFT-based vibration analysis for bearing wear, imbalance, and misalignment detection — works with a standard USB accelerometer, no dedicated hardware analyzer required.
```

## Bullets / feature list

```
Fault-signature detection
Trend logging over time
Exportable maintenance reports
Works with standard sensors
One-time purchase
```

## Full description (long-form field)

```
TPT Vibration Analyzer performs FFT/spectral analysis of vibration sensor
data to detect bearing wear, imbalance, and misalignment signatures — using
a standard USB accelerometer instead of a dedicated hardware analyzer.

Who it's for: predictive-maintenance technicians at small manufacturing
plants doing bearing/motor fault detection, priced out of SKF/Fluke-class
vibration analyzers costing thousands of dollars per unit.

Pro edition (this download):
- Fault-signature detection (bearing/imbalance/misalignment)
- Trend logging over time
- Exportable maintenance reports

Try the free basic-FFT-view edition first, right in your browser, no
install: tptsolutions.co.nz/tools/vibration-analyzer

Requirements: Windows 10 (20H2+) or Windows 11. Uses the WebView2 runtime,
preinstalled on both — nothing extra to download.
```

## Category / tags

- Category: **Software > Design & Engineering** (or Gumroad's closest equivalent)
- Tags: `vibration-analysis`, `predictive-maintenance`, `condition-monitoring`,
  `industrial-maintenance`, `engineering-software`, `windows`

## Cover image / thumbnail

Not created yet. A screenshot of an FFT spectrum with a flagged fault
signature is the most convincing cover once built. No stock/AI art.

## After publishing

1. Copy the resulting product URL (`https://gum.co/...`).
2. Paste it into the `gumroadUrl` field of the `vibration-analyzer` entry in
   `tpt-electrician-nz-2\src\lib\tpt-apps-registry.ts`, and set `price: '$199'`.
3. Set `status` to `'live'`.
4. Commit and push the registry repo.
