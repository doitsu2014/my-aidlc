# Regression report: WU-008

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 544 passed / 0 failed.
- Final: 545 passed / 0 failed, 33 targets. Difference: +1 (ts015). No existing test changed state.
