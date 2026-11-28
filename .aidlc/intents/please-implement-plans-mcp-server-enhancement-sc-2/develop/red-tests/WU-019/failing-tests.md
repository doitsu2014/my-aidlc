# Failing tests: WU-019 (Dashboard build)

- Purpose: list the tests written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-030, FN-036, CFG-012, AC-011, AC-016..AC-021, US-006, US-007.
- Kind: code manifest. New file `tests/monitor_assets.rs`.

| Test | Checks |
|---|---|
| `ts030_forbidden_apis_are_absent` | no innerHTML/outerHTML/insertAdjacentHTML/document.write/eval/new Function/localStorage/sessionStorage/document.cookie/indexedDB |
| `ts030_no_remote_urls_no_inline_script_or_style` | no http(s) URL, no inline script body, no style attribute |
| `ts030_token_handling_and_request_shape` | replaceState before the first fetch, Authorization header, GET only, AbortController, no console |
| `ts030_privacy_text_drops_line_and_generic_label` | AC-020 disclosure text, `Dropped events: not recorded`, generic `local project` label, state strings |
| `ts030_narrow_width_and_accessibility_css` | 60rem breakpoint, min-width 0, overflow-wrap, focus-visible, forced-colors, reduced motion, dark tokens, 44 px targets |
| `ts030_monitor_serves_the_assets_with_the_csp` | real monitor serves `/`, `/app.js`, `/app.css` with the CSP and no Access-Control headers |
| `ts030_review_fixes_retry_hint_banner_and_columns` (review round) | retry unpauses, describedby only for existing hints, no nested banner class, flexible event column |
