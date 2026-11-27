# Regression report: WU-002

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 461 passed / 0 failed (commit 9697529).
- Final: 467 passed / 0 failed, 30 targets.
- Differences: +6 new passing lib tests. No existing test changed state; no golden changed.
