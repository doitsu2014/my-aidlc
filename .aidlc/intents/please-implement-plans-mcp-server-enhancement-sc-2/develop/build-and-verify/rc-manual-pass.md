# RC manual pass (headless)

## Purpose

The owner asked for a headless pass before the release-candidate gate (2026-11-28). Code under test: commit 483e65f. Two passes were run: a dashboard/accessibility pass and a signal/telemetry pass. Labels: **Observed** means seen in the headless browser or process; **Not checked** means not exercised.

## Environment

- macOS, headless Microsoft Edge 158, isolated profile, driven over CDP.
- Network: 127.0.0.1 only.
- Prebuilt binaries from 483e65f. Nothing rebuilt, no repo file edited.
- MCP driven over stdio with scripted frames. Crafted telemetry lines (1 corrupt, 1 partial tail, several with markup values) added to a scratch project store.
- Widths: 1280 px and 320 px (375 px in the signal pass). Emulated dark and forced-colors.

## Results

| AC | Check | Result | Label |
|---|---|---|---|
| AC-013 | SIGINT to monitor: stops, port closed, stdout empty, no extra stderr | Pass | Observed |
| AC-013 | SIGTERM to monitor: same | Pass | Observed |
| AC-013 | Wrapper without signal forwarding leaves monitor running (README claim) | Pass | Observed |
| AC-013 | Monitor writes nothing to the project (before/after file snapshot diff empty) | Pass | Observed |
| AC-013 | Ctrl+C in a real TTY; Windows .cmd wrapper prompt | Not checked | Not checked |
| AC-017 | Empty store: message and coverage text, 0 rows, both widths | Pass | Observed |
| AC-017 | Corrupt line and partial tail: coverage counters shown, rows render | Pass | Observed |
| AC-017 | Monitor stopped while page open: stale banner within one 5 s poll, polling stops, Retry once sends 1 GET | Pass | Observed |
| AC-017 | Token missing (bare / and malformed fragment): banner, 0 API calls, hash removed | Pass | Observed |
| AC-019 | Filter name=status, Apply: only matching rows, summary updates | Pass | Observed |
| AC-019 | Show this trace / Show this session apply the filter | Pass | Observed |
| AC-019 | Clear resets | Pass | Observed |
| AC-019 | Load older pauses auto-refresh; Back to newest resumes | Pass | Observed |
| AC-019 | Invalid session value blocked client-side, aria-invalid set, focus moved | Pass | Observed |
| AC-021 | Tab order at 1280 px matches design pack section 5.3 | Pass | Observed |
| AC-021 | Tab order at 320 px, all controls reachable, no horizontal scroll | Pass | Observed |
| AC-021 | Visible focus indicator at every stop (light, dark, forced-colors) | Pass | Observed |
| AC-021 | Enter on row opens detail and focuses heading; Escape/Close returns focus to same row | Pass | Observed |
| AC-021 | Row focus survives auto-refresh | Pass | Observed |
| AC-021 | Live region: role=status, auto-refresh silent, manual Refresh updates text | Pass (attributes and text only) | Observed |
| AC-021 | Forced-colors: selected row distinguishable from others | **Fail** | Observed (emulated palette) |
| AC-021 | Screen-reader speech output | Not checked | Not checked |
| XSS/CSP | No injected elements, 0 alerts, no `window.__pwn`, 0 row texts with `<` | Pass | Observed |
| XSS/CSP | No console errors, CSP or Trusted Types violations; strict response CSP; only expected requests, all API calls carry Authorization | Pass | Observed |
| XSS/CSP | Escaping of a markup string actually displayed | Not checked | Not checked |

Note on the XSS row: the server rejects every crafted markup line as a skipped line, so the page never received one. Escaping of an accepted hostile string was not exercised in a browser. Source review only suggests values are written with textContent and setAttribute; that is not a browser observation.

## Defects found

1. Severity **low**. **Blocks RC: yes under the stated rule, because the check is recorded as an AC-021 fail; the owner should confirm that AC-021 covers forced-colors, and re-check in real Windows High Contrast.**
   Verbatim: "Forced-colors (Edge emulation, light palette): .row-btn has 'border-left: 4px solid transparent' (app.css line 88), and forced colors force that border to rgb(0,0,0) on every row. All rows show a 4px black left bar, and the selected row (Highlight rgba(5,0,73,0.8) border plus 1px outline, app.css line 117) is barely distinguishable from the rest. This conflicts with the s5.3 checklist item 'selected-row border ... stay visible' in forced colors. Possible fix: in the forced-colors block, give non-selected .row-btn border-left-color: Canvas. Evidence: /private/tmp/rc-pass-a11y/shots/forced-1280-selected-row-unfocused.png. Severity is probably low; confirm in real Windows High Contrast."

2. Severity **low**. **Blocks RC: no** (focus and aria-invalid did change; no AC observed broken).
   Verbatim: "Minor, unverified: after an invalid Session filter, the #status live region kept its previous 'Updated ..., 6 events' text, so I saw no distinct validation message there. aria-invalid and focus did change. A screen reader may not announce the validation failure through the live region. Needs manual check."

3. Informational, not a defect, **does not block**.
   Verbatim: "Minor, informational only: the monitor has no signal handler, so SIGINT and SIGTERM end it by default signal disposition (exit status 130 and 143 in a shell). The README does not claim a graceful shutdown or a final message, so this is not a mismatch."

## What stays manual

- Real screen-reader speech output (VoiceOver and others), including whether an invalid-filter error is announced.
- Windows desktop prompts: Ctrl+C in a real TTY and the "Terminate batch job" prompt in a .cmd wrapper.
- Real Windows High Contrast palettes (only the Edge emulated palette was used).
- Display of a markup string the server accepts (needs a test server returning one).
- prefers-reduced-motion behaviour (rule exists, not exercised).
- 360 px error-state scenario (320 px passed with the stopped state shown).
- Focus surviving 3 or more auto-refreshes while paused (only 1 to 2 cycles seen).
- Long-running behaviour: many minutes of 5 s polling, 7-day retention, large-data paging.
- Dark-theme contrast measurement (emulated only, not measured).
- Not observed: workflow.stage_reported and workflow.gate_decided events (no stage was reported or gated); telemetry came from driven and crafted MCP calls, not a real agent session.

## Resolution (conductor, 2026-11-28)

- Defect 1 (forced colors, selected row not distinguishable): fixed. `rust-cli/monitor/assets/app.css` forced-colors block now sets `.row-btn { border-left-color: Canvas; }` for unselected rows, so only the selected row shows the Highlight border and outline (AC-021 "not rely on color alone"). A needle in `ts030_narrow_width_and_accessibility_css` failed before the fix (Observed, tests/monitor_assets.rs:193) and passes after. Full sweep after the fix: 657 passed, 0 failed, 1 ignored. Re-check in real Windows High Contrast stays manual.
- Defect 2 (invalid filter not announced through the status live region): accepted as a low limitation. The field gets `aria-invalid`, focus and an error referenced by `aria-describedby`, which screen readers announce on focus; real screen-reader behaviour stays manual.
- Rendering of a markup string actually displayed could not be exercised because the server rejects such lines (strict parse); the page writes values with `textContent` only (static test TS-030).
