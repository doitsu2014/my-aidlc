# Green verification report: WU-007

- Status: Complete (build-and-verify, 2026-11-28). Integrated main tree, base `b2c4166` (all other units except WU-008 integrated).

## Results per test level (Observed)
- Lock unit tests: `cargo test --lib ts020` -> 8 passed.
- `ts007_concurrent_reports_never_lose_an_update` run alone 3 times on the integrated tree: 3/3 passed (the worker ran it 6/6; before the lock it failed 12/12).
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 33 targets, **544 passed, 0 failed** (baseline 506/0; +38).
- Suites in that run: cli_golden 4, onboard_golden 4, orchestration_golden 14, routing_golden 11, scope_golden 1, status_golden 1, utility_golden 2, rust_intent_commands 18, rust_doctor 10, rust_config_commands 7, rust_harness_commands 5, rust_knowledge_commands 8, mcp_stdio 13, mcp_parity 8, mcp_redaction 8, rust_cargo_install 6. No golden file changed.
- `.aidlc/state.lock` absent after the run (also asserted by `ts020_lock_file_never_survives_a_command`). Cargo.toml and Cargo.lock unchanged; `cargo fmt --check` clean; clippy: only the known pre-existing warning.
- Independent review: implementation matches FN-022/IF-011 at all 6 wrap sites; trace-row mapping corrected here; two nits recorded (reopen ceiling, symlink TOCTOU). Under heavy parallel load the reviewer saw pre-existing timing flakes only (`check_timeout_is_reported`, `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`); no `ts020_` test ever failed.
- Not run: Windows (WU-008 CI is the first evidence for LockFileEx and delete-pending behaviour).

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-001 | AC-001 | FN-022 | golden suites above; `ts020_root_without_aidlc_runs_unlocked_and_creates_nothing`; `ts020_lock_file_never_survives_a_command` | passed |
| US-002 | AC-002 | FN-005, FN-006 | `ts006_lifecycle_through_mcp_matches_the_cli_state_and_audit`, `ts006_status_matches_the_cli` (TS-006) | passed |
| US-003 | AC-003 | FN-016, FN-017 | `ts005_legacy_session_is_stdout_pure_and_answers_every_request`, `ts005_modern_session_needs_no_handshake_and_stamps_results_not_errors`, `ts005_eras_mix_per_request_on_one_connection` (TS-005) | passed |
| US-003 | AC-004 | FN-012, FN-017 | `ts005_empty_stdin_exits_zero_and_writes_nothing`, `ts005_crlf_framing_is_accepted_and_replies_are_lf_only`, `ts005_startup_failure_exits_one_before_reading_stdin`; `ts014_hostile_protocol_input_table_keeps_the_session_alive` (TS-005, TS-014) | passed |
| US-004 | AC-005 | FN-013 | `ts014_hostile_tool_arguments_are_rejected_before_the_project_is_opened`, `ts014_boundary_values_pass_validation`, `ts014_line_of_exactly_the_cap_is_accepted` (TS-014) | passed |
| US-005 | AC-006 | FN-015 | `ts008_status_and_queue_on_the_default_temp_base_carry_no_absolute_path`, `ts008_macos_private_var_twin_is_registered_for_a_var_root` (TS-008) | passed |
| US-005 | AC-007 | FN-003, FN-014 | `ts006_queue_is_a_read_only_preview_and_matches_the_cli` (state.json and audit.jsonl bytes unchanged) (TS-006) | passed |
| US-006 | AC-008 | FN-005, FN-007 | `ts006_no_prerequisite_command_runs_over_mcp_but_the_cli_runs_it`, `ts006_an_intent_cannot_be_created_over_mcp`, `ts008_next_and_report_results_are_project_relative` (TS-006, TS-008) | passed |
| US-007 | AC-009 | FN-006, FN-020 | `ts006_no_prerequisite_command_runs_over_mcp_but_the_cli_runs_it` (pendingPrerequisites after an MCP report) (TS-006) | passed |
| US-008 | AC-010 | FN-006 | `ts006_lifecycle_through_mcp_matches_the_cli_state_and_audit` (approver_required for missing/blank label; labelled approval equals the CLI) (TS-006) | passed |
| US-004 | AC-011 | FN-014, FN-015 | `ts008_error_messages_never_carry_a_path`, `ts008_unusable_engine_home_warns_on_stderr_without_the_path_and_stdout_stays_pure` (TS-008) | passed |
| US-004 | AC-012 | FN-008, FN-019, FN-022 | `ts007_concurrent_reports_never_lose_an_update` (TS-007); `ts020_racing_persist_active_calls_land_exactly_one_write`, `ts020_held_lock_times_out_with_state_changed_without_running_f` (TS-020); `ts005_root_is_fixed_at_launch_and_never_taken_from_a_call` (TS-005) | passed |
| US-009 | AC-013 | FN-016 | `ts009_cargo_install_and_uninstall_both_binaries` (TS-009) | passed |
