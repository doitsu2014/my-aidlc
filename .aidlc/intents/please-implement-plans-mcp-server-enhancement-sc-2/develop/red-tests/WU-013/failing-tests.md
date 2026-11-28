# Failing tests: WU-013 (Telemetry core plus managed ignore line)

- Purpose: list the tests written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-021, TS-022, TS-023, TS-024, TS-025, FN-023..FN-028, AC-002, AC-003, AC-005, AC-008, AC-009, AC-010, AC-012, AC-027, AC-028.
- Kind: code manifest. New files: `rust-cli/telemetry/mod.rs`, `rust-cli/telemetry/sink.rs` (unit tests in `#[cfg(test)]` modules), `tests/telemetry_guards.rs`.

| Test id | Tests | File |
|---|---|---|
| TS-021 | `ts021_line_round_trips_with_keys_in_contract_order`, `ts021_parse_rejects_unknown_extra_missing_and_bad_values`, `ts021_invalid_optional_ids_are_omitted_not_stored`, `ts021_started_events_have_null_outcome_and_duration`, `ts021_completed_accepts_null_duration_and_started_rejects_a_duration` | `rust-cli/telemetry/mod.rs` |
| TS-022 | `ts022_hex_is_lowercase_and_os_random_fills_distinct_buffers` | `rust-cli/telemetry/mod.rs` |
| TS-023 | `ts023_stalled_writer_never_blocks_the_producer`, `ts023_panicking_build_is_contained_and_disables`, `ts023_panicking_consumer_is_contained_and_disables`, `ts023_next_id_is_session_prefixed_and_unique`, `ts023_sink_start_writes_through_the_real_store_and_finish_drains`; Q1 guards `ts023_sink_is_built_only_under_mcp`, `ts023_no_stdout_writes_outside_main_and_mcp_handoff`, `ts023_cli_run_creates_no_telemetry_dir` | `rust-cli/telemetry/*.rs`, `tests/telemetry_guards.rs` |
| TS-024 | `ts024_file_name_pattern_is_strict`, `ts024_store_state_distinguishes_absent_dir_and_unsafe`, `ts024_writer_never_creates_aidlc_and_drops_silently`, `ts024_writer_creates_store_gitignore_and_part_files`, `ts024_rotates_when_the_next_line_would_exceed_the_part_limit`, `ts024_symlinked_store_is_refused_and_nothing_is_written_through_it`, `ts024_prune_by_age_then_size_never_touches_current_or_unrelated_files`, `ts024_prune_skips_symlinks_named_like_store_files`, `ts024_ordinary_eacces_is_a_write_failure_not_an_unsafe_location`, `ts024_unsafe_location_is_marked_and_chooses_the_unsafe_message` | `rust-cli/telemetry/*.rs` |
| TS-025 | golden diff procedure on `end_to_end_lifecycle_matches_the_golden` and `end_to_end_lifecycle_with_the_embedded_engine_matches_the_golden` (only the `/telemetry/` managed ignore line) | `tests/routing_golden.rs` |
