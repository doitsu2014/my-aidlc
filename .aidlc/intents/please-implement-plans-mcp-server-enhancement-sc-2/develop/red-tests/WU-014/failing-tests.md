# Failing tests: WU-014 (MCP telemetry wiring)

- Purpose: list the tests written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-026, TS-027, FN-029, FN-030, FN-031, IF-015, IF-016, CFG-007, AC-001, AC-002, AC-004..AC-008, AC-010, AC-012, AC-028.
- Kind: code manifest.

| Test id | Tests | File |
|---|---|---|
| TS-026 | `ts026_on_error_table_maps_every_error_reply_to_a_fixed_code`, `ts026_on_error_is_silent_for_successes_and_notifications`, `ts026_on_error_is_not_called_when_the_write_fails`, `ts026_error_code_name_covers_the_ifs_table` | `rust-cli/mcp/protocol.rs` |
| TS-027 | `ts027_telemetry_off_writes_nothing`, `ts027_default_is_on_and_flags_are_order_independent`, `ts027_stdout_is_byte_identical_with_telemetry_on_and_off`, `ts027_event_set_trace_shape_and_session_ids`, `ts027_workflow_events_follow_a_durable_success`, `ts027_a_failed_operation_emits_no_workflow_event`, `ts027_sentinel_secrets_never_reach_the_store`, `ts027_protocol_errors_after_handshake_and_list_are_recorded`, `ts027_each_server_process_gets_its_own_session`, `ts027_a_failing_store_changes_nothing_on_stdout`, `ts027_concurrent_servers_write_separate_valid_stores` | `tests/mcp_telemetry.rs` (new) |
