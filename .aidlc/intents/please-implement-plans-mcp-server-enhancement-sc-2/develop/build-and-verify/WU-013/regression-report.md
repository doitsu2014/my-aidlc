# Regression report: WU-013

- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-w1 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` in the main tree at 695d62f (WU-011, WU-012 committed) with WU-013 applied.
- Result: 570 passed, 0 failed (546 + 24 new). Golden diff limited to the scoped `/telemetry/` lines. regression.json `failures: []`.
