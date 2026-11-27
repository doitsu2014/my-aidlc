# Regression report: WU-010

- Status: Complete (2026-11-27).
- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (agreed environment: macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline (`develop/baseline.md`): 454 passed, 2 failed (`onboarding::tests::rejects_traversal_and_absolute_paths`, `onboarding::projection::tests::refuses_symlink_targets`).
- Final: 456 passed, 0 failed, 28 targets.
- Differences: exactly the two baseline failures now pass (fixed by this unit). No other test changed state. No new failures.
- From now on the macOS baseline for later units is 0 failures.
