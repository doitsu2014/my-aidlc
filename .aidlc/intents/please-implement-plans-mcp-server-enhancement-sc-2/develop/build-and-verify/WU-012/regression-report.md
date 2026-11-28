# Regression report: WU-012

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at c23b4a8 (WU-011 committed) with WU-012 applied; re-run after WU-011 was recorded.
- Result: 546 passed, 0 failed (baseline 545 / 0 plus TS-031). No failures; regression.json `failures: []`.
