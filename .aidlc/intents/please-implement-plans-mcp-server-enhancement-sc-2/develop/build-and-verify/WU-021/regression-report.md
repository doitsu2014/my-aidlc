# Regression report: WU-021

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at 63852db (WU-019 committed) with WU-021 applied.
- Result: 657 passed, 0 failed (652 + 5 new). Golden changes limited to the seven version goldens (TS-035). regression.json `failures: []`.
