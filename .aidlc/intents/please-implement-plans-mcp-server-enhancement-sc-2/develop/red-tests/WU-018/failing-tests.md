# Failing tests: WU-018 (Monitor server and CLI wiring)

- Purpose: list the tests written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-029, TS-037, FN-033, FN-034, FN-035, IF-018, IF-019, CFG-010, AC-013..AC-016, AC-025, AC-027, AC-030, US-005.
- Kind: code manifest.

| Test id | Tests | File |
|---|---|---|
| TS-029 (raw TcpStream, test-first) | `ts029_startup_line_is_one_stderr_line_and_stdout_stays_empty`, `ts029_two_monitors_get_distinct_ports_and_tokens`, `ts029_static_routes_need_no_token_and_carry_the_fixed_headers`, `ts029_api_needs_the_exact_bearer_token`, `ts029_api_returns_stored_events`, `ts029_filter_errors_map_to_their_http_status`, `ts029_store_unavailable_when_the_telemetry_dir_turns_unsafe`, `ts029_start_refuses_an_unsafe_telemetry_path_without_naming_it`, `ts029_unknown_targets_are_404_and_traversal_never_reads_files`, `ts029_malformed_requests_are_400_bad_request`, `ts029_only_get_is_allowed_and_the_method_is_checked_before_the_host`, `ts029_a_large_post_body_still_receives_its_405_and_the_drain_is_bounded`, `ts029_host_must_be_exactly_loopback_and_port`, `ts029_origin_and_sec_fetch_site_are_exact`, `ts029_an_oversized_head_is_431`, `ts029_a_slow_head_gets_408_after_about_five_seconds`, `ts029_the_seventeenth_connection_gets_503_busy_and_slots_are_released`, `ts029_a_closed_connection_frees_its_slot`, `ts029_no_response_ever_carries_access_control_headers` | `tests/monitor_http.rs` (new) |
| TS-029 (unit) | `ts029_ct_eq_equal_short_and_one_bit`, `ts029_a_handler_panic_answers_500_internal_error` (ADR-032 row 14), `ts029_the_slot_is_released_when_a_handler_panics` | `rust-cli/monitor/http.rs` |
| TS-037 | golden diff procedure: `routing_golden` validation and unsafe goldens, `utility_golden` help-and-completion (monitor lines only) | `tests/routing_golden.rs`, `tests/utility_golden.rs` |
