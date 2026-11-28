# Failing tests: WU-020 (Installers for both binaries)

- Purpose: list the tests written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-034, FN-039, IF-020, AC-026, US-009.
- Kind: code manifest. All in `tests/rust_cargo_install.rs`.

| Test | Platform | State before the fix (Observed) |
|---|---|---|
| `ts034_posix_installer_round_trips_both_binaries` | POSIX | FAILED at :548 `f.lib().join("dtech-aidlc-mcp").is_file()` |
| `ts034_posix_upgrade_from_cli_only_v1_adds_mcp_and_keeps_unrelated_files` | POSIX | FAILED at :585, same assertion |
| `ts034_posix_installer_refuses_missing_mcp_binary_and_foreign_mcp_launcher` | POSIX | FAILED at :609 `!out.status.success()` |
| `ts034_posix_uninstall_refuses_foreign_mcp_launcher` (review fix) | POSIX | FAILED at :748 against an installer variant that deletes the foreign launcher |
| `ts034_powershell_installer_round_trips_both_binaries`, `ts034_powershell_upgrade_from_cli_only_v1_adds_mcp_and_keeps_unrelated_files`, `ts034_powershell_uninstall_refuses_foreign_mcp_exe` | pwsh (macOS and Windows CI) | not run locally (pwsh absent); they assert instead of skipping on Windows |
| `ts034_powershell_upgrade_refuses_when_installed_files_are_in_use` | Windows only | not compiled on macOS |
