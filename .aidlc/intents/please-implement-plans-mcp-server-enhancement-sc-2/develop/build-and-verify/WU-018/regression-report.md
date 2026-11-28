# Regression report: WU-018

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at ddf97ca (WU-015 committed) with WU-018 applied.
- Result: 645 passed, 0 failed (623 + 22 new). Golden changes limited to monitor lines (TS-037). regression.json `failures: []`.
