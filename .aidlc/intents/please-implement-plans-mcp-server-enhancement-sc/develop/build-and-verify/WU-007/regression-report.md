# Regression report: WU-007

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 506 passed / 0 failed.
- Final: 544 passed / 0 failed, 33 targets (+3 new integration test targets).
- Differences: +38 tests (8 ts020 lock tests, 29 MCP integration tests, 1 install test). No existing test changed state; no golden changed. CLI behaviour without contention is unchanged (goldens); with contention a losing writer now gets STATE_CHANGED instead of silently losing its update.
- Pre-existing timing flakes seen only under heavy parallel load by the reviewer (not in this run): `prerequisites::tests::check_timeout_is_reported`, `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. Not attributed to any unit; flagged as CI risk.
