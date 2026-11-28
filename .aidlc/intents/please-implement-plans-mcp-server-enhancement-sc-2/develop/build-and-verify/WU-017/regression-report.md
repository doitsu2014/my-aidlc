# Regression report: WU-017

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at 164f09c (WU-014 committed) with WU-017 applied.
- Result: 615 passed, 0 failed (591 + 24 new; 1 ignored measurement test). regression.json `failures: []`.
