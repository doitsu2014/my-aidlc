# Regression report: WU-005

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 499 passed / 0 failed (commit e720b5f).
- Final: 506 passed / 0 failed, 30 targets.
- Differences: +7 tool-adapter tests; `prerequisites::tests::ts010_skip_runs_no_command` made deterministic (test-only). No existing test changed state; no CLI file touched.
