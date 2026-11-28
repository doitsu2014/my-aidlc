# Regression report: WU-015

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at d3f0312 (WU-017 committed) with WU-015 applied.
- Result: 623 passed, 0 failed (615 + 8 new). regression.json `failures: []`.
