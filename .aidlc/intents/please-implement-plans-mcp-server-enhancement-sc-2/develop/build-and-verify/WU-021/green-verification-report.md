# Green verification: WU-021

- Status: Complete (2026-11-28). Cites TS-034, TS-035, TS-036, AC-020, AC-023..AC-027, AC-029, AC-030.
- Observed (conductor, main tree at 63852db + WU-021): `cargo fmt --check` clean; `cargo build --locked` ok; full suite 657 passed / 0 failed; `ts034_posix*` 6 passed in 3 of 3 runs; the seven version goldens each `cmp`-equal to `sed s/1.2.2/1.3.0/` of HEAD and no other golden changed; Cargo.lock diff is the single version line; LOCK_SHA256 matches; README keeps `(proposed)`.
- Threat review: 0 unresolved high findings (AC-030 release rule met on macOS); Windows items pending CI.
- Trace: US-008 AC-023 AC-024 FN-040 TS-035 PASS; US-011 AC-020 AC-025 AC-030 FN-041 TS-036 + threat review PASS; US-012 AC-026 FN-040 ts034_installed_cli_starts_monitor_from_its_own_location PASS; AC-027 TS-035 golden procedure PASS; AC-029 macOS PASS, Windows pending CI.
