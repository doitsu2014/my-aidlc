# Regression report: WU-020

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at 2f9a33e with WU-020 applied (the uncommitted WU-016 mock edit in plans/ is not code).
- Result: 577 passed, 0 failed (570 + 7 new). regression.json `failures: []`.
