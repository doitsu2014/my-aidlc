# Existing suite baseline (intent add-a-port-n-option-to-dtech-aidlc-monitor-so-th)

- Status: Recorded 2026-11-28 by the conductor before any code change in this intent.
- Commit: f546d0b (code identical to 372b60f; later commits change only `.aidlc/`).
- Environment: macOS (Darwin 25.6.0, aarch64), rustc/cargo 1.100.0, `TMPDIR` under `/private/tmp`, `target/debug` first on `PATH`.
- Command (Observed, run on 372b60f after the forced-colors fix): `cargo build && TMPDIR=/private/tmp/aidlc-test-sweep2 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast`
- Result (Observed): 39 result lines, 657 passed, 0 failed, 1 ignored. CI run 46121038454 on e01c3d7 (same code): windows-latest 693/0, macos-latest 702/0, ubuntu-latest 702/0.
- Note: the local count includes PowerShell tests that return early when pwsh is absent.
