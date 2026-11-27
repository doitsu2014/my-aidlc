# Regression report: WU-003

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 494 passed / 0 failed (commit a986c73).
- Final: 499 passed / 0 failed, 30 targets.
- Differences: +5 facade tests. No existing test changed state; every golden unchanged.
