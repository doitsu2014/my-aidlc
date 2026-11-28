# Green verification: WU-017

- Status: Complete (2026-11-28). Cites TS-028, FN-032, AC-003, AC-016..AC-019.
- Observed (worktree): `cargo test --lib telemetry::query` 23 passed, 1 ignored; `cargo fmt --check` clean; clippy clean on query.rs; Cargo.lock unchanged; full suite 600 / 0 at db30d93 + patch.
- Observed (independent review): ok, no blocking issue; span matching for `incomplete` verified against WU-014's `traced_call`.
- Observed (conductor, main tree at 164f09c + WU-017 + nit fixes): `cargo test --locked --lib telemetry::query` 23 passed, 1 ignored; full suite 615 passed / 0 failed; `cargo fmt --check` clean.
- Measurement (Observed, release build): 12 files, 62.9 MB, 132,574 events: RSS +79 MiB, parse cache about 62.5 MiB, cold 345 ms, warm 24 ms; the 64 MiB byte cap is reached before the 250,000-event cap for events of this size.
