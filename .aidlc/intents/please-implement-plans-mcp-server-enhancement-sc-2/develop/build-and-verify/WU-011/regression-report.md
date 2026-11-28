# Regression report: WU-011

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree with WU-011 and WU-012 applied on 2ef7fa6.
- Result: 546 passed, 0 failed (baseline 545 / 0 plus TS-031). No failures; regression.json `failures: []`.
