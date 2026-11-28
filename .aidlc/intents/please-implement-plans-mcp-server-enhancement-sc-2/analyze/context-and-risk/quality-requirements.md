# Quality requirements

Purpose: state measurable targets for the five quality areas and how each is verified.
Status: Draft - pending analysis-baseline gate


## Performance
- Measure: Telemetry overhead per MCP tool call (one bounded append, no state lock); monitor query bounds (range, page size, files and bytes read); suite wall time. Targets are Estimates, no figures exist in the plans.
- Target: Estimate: typical overhead well under 1 ms and not a contract; every monitor query bounded by a stated maximum range, page size, file count and byte budget; slowest test target stays near the baseline (rust_unit_scheduling about 20 s).
- Verified by: unit tests of bounds and `query_limit_reached`; telemetry on/off timing sanity check without tight assertions (no timing assertion tighter than seconds); baseline test run (AC-010, AC-016, AC-018).
## Availability
- Measure: Operation results, exit codes and stdout with telemetry on, off, corrupt, full or unwritable; monitor start, interrupt and port-in-use behaviour; dashboard states on missing or corrupt data.
- Target: Identical results in all five telemetry conditions; monitor starts in the foreground, prints its address, exits on interrupt with no orphan, and reports a clear error when it cannot start; all nine dashboard states render. Liveness is shown as "unknown" (no heartbeat).
- Verified by: fault-injection tests (read-only dir, dir in place of file, oversized, corrupt), CLI+MCP interleaving test, monitor start/stop tests, dashboard state matrix in the AI-review artifact (AC-010, AC-013, AC-017, AC-028).
## Privacy
- Measure: Fields stored per event; presence of payload, free text, `detail`, `approved_by`, credentials, env values or unredacted absolute paths in stored files and in monitor responses; outbound connections and remote assets.
- Target: Zero such content (explicit field allowlist; validated ids only); loopback-only, GET-only monitor with exact Host/Origin and no CORS; no outbound connection or remote font/script; telemetry directory git-ignored by the managed block and a self-ignore; Unix files 0o600; dashboard discloses what is retained; telemetry is not audit and approver names are self-asserted.
- Verified by: hostile-input tests asserting absence in files, forged Host/Origin tests over a raw socket, offline run, asset scan of the dashboard, golden review of the one .gitignore line, AI threat-model review before release (AC-002, AC-008, AC-011, AC-014, AC-015, AC-020, AC-030).
## Accessibility
- Measure: Dashboard keyboard operability, labels and landmarks, focus visibility, non-colour status cues, reduced-motion handling, narrow-width layout.
- Target: Keyboard-only completion of range, filter, row, trace, refresh and close flows; no colour-only status; no horizontal scroll at 320 px; prefers-reduced-motion respected; basics only, no formal WCAG conformance claim.
- Verified by: static checks of the embedded HTML/CSS plus a manual checklist recorded in the AI design-review artifact (no human design gate, owner decision Q15) (AC-021, AC-022).
## Operability
- Measure: Release and install consistency; test suite and CI results; documentation accuracy; cross-platform behaviour.
- Target: Version 1.3.0 agrees across Cargo.toml, Cargo.lock, badge, CHANGELOG heading and lock-hash pin; suite at or above 545 passed, 0 failed, 0 ignored (the 545/0 baseline is not stable until the three timing flakes are fixed: a reviewer run at 327b02a saw `artifacts::tests::verifies_empty_missing_and_not_applicable_reason` fail in 2 of 4 runs); Windows and macOS CI green; zero new crates and MSRV 1.100; installers add, upgrade and uninstall both binaries in sh and PowerShell without skipping; README and CHANGELOG accurate, "proposed" removed only after the Q10 evidence; goldens re-blessed only for the three owner-allowed reasons.
- Verified by: `cargo test --no-fail-fast` (baseline 545/0 at commit 327b02a), CI runs on Windows, macOS and Ubuntu (reference run 46090623346), repo_content and mcp_packaging tests, installer tests, reviewed golden diffs (AC-023, AC-024, AC-025, AC-026, AC-027, AC-029).
