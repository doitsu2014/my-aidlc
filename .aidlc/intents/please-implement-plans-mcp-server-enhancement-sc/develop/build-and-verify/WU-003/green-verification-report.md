# Green verification report: WU-003

- Status: Complete (build-and-verify, 2026-11-28). Integrated main tree, base `a986c73` (WU-001/010/002/004/006 integrated).

## Results per test level (Observed)
- Focused: `cargo test --lib application` -> 5 passed, 0 failed.
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 30 targets, 499 passed, 0 failed (baseline 494/0; +5).
- CLI parity (TS-011), golden and orchestration suites in that run, all unchanged and passing: cli_golden 4, onboard_golden 4, orchestration_golden 14, routing_golden 11, scope_golden 1, status_golden 1, utility_golden 2, rust_orchestrate_engine 6, rust_orchestration_report_validation 4, rust_orchestration_policy 5. No golden re-blessed.
- `orchestrate queue`: byte-identical stdout, stderr and exit code between the pre-change and patched binaries (re-reviewer, separate builds).
- `cargo fmt --check` clean; clippy: only the known pre-existing warning.
- Not run: Windows (WU-008 CI).

## Quality requirements verified
- Gate integrity (Q6/Q18, TH-01/TH-04): an MCP approval without an explicit approver is refused before any identity lookup; nothing written.
- Security (Q5): MCP next cannot run prerequisite commands or create intents (mutation-proven tests).
- CLI compatibility (AC-001): goldens unchanged.

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-001 | AC-001 | FN-009 | golden suites listed above (TS-011) | passed |
| US-002 | AC-002 | FN-001, FN-003..FN-006 | CLI routed through the facade + goldens (TS-011); `ts010_mcp_next_skips_prerequisite_checks_and_cli_runs_them` (CLI half) | passed |
| US-006 | AC-008 | FN-005 | `ts010_mcp_next_cannot_create_an_intent`, `ts010_mcp_next_skips_prerequisite_checks_and_cli_runs_them` (TS-010) | passed |
| US-007 | AC-009 | FN-006 | `ts010_cli_caller_is_not_subject_to_the_approver_guard` (TS-010); `pendingPrerequisites` exercised end to end in WU-005/WU-007 | passed (partial; see limitations) |
| US-008 | AC-010 | FN-006 | `ts010_blank_approver_refused_over_mcp_before_any_lookup` (TS-010) | passed |
| US-001 | AC-001 | FN-002 | `ts011_external_engine_from_honours_the_injected_home` (TS-011) | passed |
