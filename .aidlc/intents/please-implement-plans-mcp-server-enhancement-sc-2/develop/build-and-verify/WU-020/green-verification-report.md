# Green verification: WU-020

- Status: Complete (2026-11-28). Cites TS-034, FN-039, AC-026.
- Observed (re-review, clean apply): `cargo test --test rust_cargo_install` 13/13; `sh -n install.sh` ok; each review item verified, with mutants for the settings isolation and the uninstall refusal.
- Observed (conductor, main tree at 2f9a33e + WU-020): `cargo test --locked --test rust_cargo_install` 13 passed in 4 of 4 consecutive runs after the mutex; full suite 577 passed / 0 failed; `cargo fmt --check` clean; Cargo.lock unchanged.
- Trace: US-009 | AC-026 | FN-039 | ts034_posix_* (4) | PASS on macOS. ts034_powershell_* (4) | not run locally (pwsh absent) - CI.
