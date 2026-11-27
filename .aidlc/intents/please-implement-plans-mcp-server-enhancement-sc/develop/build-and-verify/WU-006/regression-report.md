# Regression report: WU-006

- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` (macOS, rustc 1.100.0, TMPDIR under /private/tmp, target/debug on PATH).
- Baseline before this unit: 483 passed / 0 failed (commit 6b60591).
- Final: 494 passed / 0 failed, 30 targets (second run).
- Differences: +11 redact tests. One intermittent failure in the first run, `prerequisites::tests::check_timeout_is_reported`, is a pre-existing timing race (commit 613c7e46, 2026-11-25; `sleep 1` vs `timeoutSeconds: 1`) unrelated to this unit; it passed 5/5 alone and in the full re-run. Recorded as a CI flakiness risk for WU-008, not as a regression and not attributed to any unit.
