# Regression report: WU-001

- Status: Complete (2026-11-27).
- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (agreed environment: macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline: intent baseline 454 / 2 (`develop/baseline.md`); after WU-010 (commit 919e0b9) 456 passed / 0 failed.
- Final: 461 passed, 0 failed, 30 targets (28 + `mcp_packaging` + the new `dtech-aidlc-mcp` bin's empty test target).
- Differences: +5 new passing tests in `tests/mcp_packaging.rs`. No existing test changed state. No failures.
