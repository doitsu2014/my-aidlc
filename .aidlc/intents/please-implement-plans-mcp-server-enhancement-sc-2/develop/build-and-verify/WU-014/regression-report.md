# Regression report: WU-014

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at db30d93 with WU-014 applied.
- Result: 591 passed, 0 failed (577 + 14 new). regression.json `failures: []`.
