# Regression report: WU-019

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at ed1b131 (WU-018 committed) with WU-019 applied.
- Result: 652 passed, 0 failed (645 + 7 new). regression.json `failures: []`.
