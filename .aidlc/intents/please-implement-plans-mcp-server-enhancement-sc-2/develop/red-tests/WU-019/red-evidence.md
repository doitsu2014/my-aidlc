# Red evidence: WU-019

- Status: Complete (red-tests step, 2026-11-28). Cites TS-030, FN-036.
- Test-first: yes for the static checks, against the WU-018 stub assets (isolated worktree at ed1b131).
- Observed: `cargo test --test monitor_assets` -> `3 passed; 3 failed`: `ts030_narrow_width_and_accessibility_css` (app.css lacks the 60rem breakpoint), `ts030_privacy_text_drops_line_and_generic_label` (index.html lacks `Dropped events: not recorded`), `ts030_token_handling_and_request_shape` (no `history.replaceState(null, '', '/')`). The other three pass trivially on stubs (absence checks).
- Review round: `ts030_review_fixes_retry_hint_banner_and_columns` failed at tests/monitor_assets.rs:123 (retry branch did not unpause) before the fix. The original defects were measured in headless Edge by the reviewer (outcome text spilling past the row at 992 px; retry never firing while paused).
