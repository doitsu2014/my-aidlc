# Regression report: WU-009

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 506 passed / 0 failed.
- Final: 506 passed / 0 failed, 30 targets. No differences (README only).
