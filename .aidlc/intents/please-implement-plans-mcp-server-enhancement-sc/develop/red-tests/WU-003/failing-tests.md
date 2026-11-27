# Failing tests: WU-003 (Facade and CLI routing)

- Status: Complete (red-tests, 2026-11-28). Cites TS-010, TS-011, FN-001..FN-006, FN-009, IF-001, AC-001, AC-002, AC-008, AC-009, AC-010.
- Kind: code manifest. All tests in `rust-cli/application.rs` (`#[cfg(test)]`).

| Test | Test id | Red observed |
|---|---|---|
| `ts010_blank_approver_refused_over_mcp_before_any_lookup` | TS-010 | yes: with the Q18 guard stubbed off, left "No active workflow state", right "approved_by is required for a gate decision over MCP (no git or OS fallback)" |
| `ts010_cli_caller_is_not_subject_to_the_approver_guard` | TS-010 | guard (CLI keeps its own approver resolution) |
| `ts010_mcp_next_cannot_create_an_intent` (rewritten in review) | TS-010 | yes, by mutation: with `options.intent = None` removed it fails (MCP next created/continued an intent instead of "No active intent") |
| `ts010_mcp_next_skips_prerequisite_checks_and_cli_runs_them` (review) | TS-010 | yes, by mutation: dropping `skip_prerequisite_checks = true`, the `mode` reset or the `force_resume` reset each make it fail (marker present / executed true) |
| `ts011_external_engine_from_honours_the_injected_home` (review) | TS-011 | new; covers None, empty, a home without engine files, and a temp home with `core/methodology` |
| TS-011 parity | golden suites | regression gate (build-and-verify) |
