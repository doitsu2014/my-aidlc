# Regression report: WU-004

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 467 passed / 0 failed (commit 4bc3ec2).
- Final: 483 passed / 0 failed, 30 targets.
- Differences: +16 new passing tests in `rust-cli/mcp`. No existing test changed state. No CLI file touched.
