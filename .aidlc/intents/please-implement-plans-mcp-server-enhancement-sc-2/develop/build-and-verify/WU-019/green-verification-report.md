# Green verification: WU-019

- Status: Complete (2026-11-28). Cites TS-030, FN-036, AC-011, AC-016..AC-021.
- Observed (fix worktree): `monitor_assets` 7 passed, `monitor_http` 19 passed; full suite 652 / 0.
- Observed (conductor, main tree at ed1b131 + WU-019): `cargo fmt --check` clean; full suite 652 passed / 0 failed.
- Browser evidence (Observed, headless Microsoft Edge with an isolated profile, CDP device-metrics, real `dtech-aidlc monitor` with seeded events; screenshots in `browser-shots/` in this folder, conductor viewed ok-320 and ok-1280):
  - With data at 320, 360, 960, 992, 1040 and 1280 px: no horizontal page scroll (scrollWidth equals clientWidth), no row child past the row edge, outcome text inside the row; at 960-1040 px also with the detail panel open (re-reviewer).
  - Bare `/` at 320, 360, 992, 1280 px: Token missing banner, no API call, no horizontal scroll.
  - Error state (unsafe store) at 320, 360, 992 px: `Telemetry store unavailable ... Retrying in 10 s` with Retry now, single-bordered banner, no horizontal scroll.
  - Behavioural retry (re-reviewer): Load older, store made unavailable, Refresh -> 503 banner, automatic retry GET about 10 s later, back to the newest 50 rows after restore.
  - 9 of 12 seeded events shown because 3 seeded lines were malformed; the coverage line reports `skipped lines 3`.
- Trace: US-006 AC-011/AC-016..AC-019 FN-036 TS-030 PASS; US-007 AC-020 FN-036 ts030_privacy_text_drops_line_and_generic_label PASS; US-007 AC-021 FN-036 ts030_narrow_width_and_accessibility_css + headless widths PASS.
