# Failing tests: WU-007 (Integration, parity, install tests and state write lock)

- Status: Complete (red-tests, 2026-11-28, after the owner-authorised rework). Cites TS-005, TS-006, TS-007, TS-008, TS-009, TS-014, TS-020, FN-022, IF-011, AC-001..AC-013.
- Kind: code manifest.

| Test file / tests | Test id | Red observed |
|---|---|---|
| `tests/mcp_parity.rs` `ts007_concurrent_reports_never_lose_an_update` | TS-007 | yes: "round 0: 2 replies recorded a report but state holds 1" (left 1, right 2), 12/12 probe rounds, before the lock |
| `rust-cli/orchestrate.rs` `ts020_racing_persist_active_calls_land_exactly_one_write` | TS-020 | yes (pass-through stub): "round 0: [(a, Ok), (b, Ok)]" left 2, right 1 |
| `ts020_held_lock_times_out_with_state_changed_without_running_f` | TS-020 | yes: unwrap_err on Ok |
| `ts020_still_marked_file_gives_the_stale_hint_not_state_changed` | TS-020 | yes: unwrap_err on Ok |
| `ts020_permission_denied_on_open_is_retried_within_the_budget` | TS-020 | yes: attempts left 0, right 3 |
| `ts020_lock_file_never_survives_a_command`, `ts020_retired_marker_is_not_a_held_lock_and_is_reopened`, `ts020_failed_unlink_leaves_no_marker_so_the_next_writer_succeeds`, `ts020_root_without_aidlc_runs_unlocked_and_creates_nothing` | TS-020 | passed against the stub (guards: properties that must hold with or without the lock) |
| `tests/mcp_stdio.rs` (TS-005, TS-014), `tests/mcp_parity.rs` (TS-006, TS-007), `tests/mcp_redaction.rs` (TS-008), `tests/rust_cargo_install.rs` `ts009_cargo_install_and_uninstall_both_binaries` (TS-009), fixtures `tests/fixtures/mcp/*` | TS-005..TS-009, TS-014 | written alongside (test-strategy: the tests are the deliverable); 28 passed against the integrated product before the lock, ts007 failed (above) |
