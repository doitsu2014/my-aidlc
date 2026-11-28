# Failing tests: WU-021 (Release candidate: docs, AI threat review, 1.3.0 prep)

- Purpose: list the tests this unit adds or extends.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-034, TS-035, TS-036, FN-040, FN-041, CFG-013, AC-020, AC-023..AC-027, AC-029, AC-030.
- Kind: code manifest.

| Test id | Test | File | Before (Observed) |
|---|---|---|---|
| TS-036 | `ts036_readme_telemetry_and_monitor_sections` | `tests/repo_content.rs` | FAILED against the README at 63852db (panic at repo_content.rs:1418, section missing) |
| TS-034 | `ts034_installed_cli_starts_monitor_from_its_own_location` (startup line plus GET / 200) | `tests/rust_cargo_install.rs` | characterisation of WU-018 behaviour through an installed binary |
| TS-034 | `ts034_posix_term_between_swaps_exits_nonzero_and_keeps_the_previous_install` | `tests/rust_cargo_install.rs` | FAILED with the old install.sh (exit 0 instead of 143) |
| TS-034 | `ts034_posix_failed_fresh_install_leaves_an_empty_prefix` | `tests/rust_cargo_install.rs` | FAILED (lib/dtech-aidlc left behind) before the `had_root` fix |
| TS-034 | `ts034_powershell_upgrade_refuses_when_installed_files_are_in_use` (extended, Windows only) | `tests/rust_cargo_install.rs` | not run locally |
| TS-024 (threat fix T2-13) | `t2_13_store_swapped_for_a_symlink_between_writes_is_refused` | `rust-cli/telemetry/sink.rs` | FAILED with the rollover re-check and prune guard removed |
| TS-035 | golden procedure (seven version goldens), LOCK_SHA256 in `tests/mcp_packaging.rs`, CHANGELOG and badge checks in `tests/repo_content.rs` | various | version consistency |
