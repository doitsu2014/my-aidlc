# Failing tests: WU-017 (Query engine)

- Purpose: list the tests written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-028, FN-032, IF-017, CFG-011, AC-003, AC-016..AC-019, US-003, US-006.
- Kind: code manifest. All in the `#[cfg(test)]` module of `rust-cli/telemetry/query.rs` (new).

| Test id | Tests |
|---|---|
| TS-028 (parameters, test-first) | `ts028_rejects_bad_parameters_with_invalid_filter`, `ts028_range_above_604800_is_range_too_large`, `ts028_accepts_valid_parameters_and_bounds` |
| TS-028 (cursor, test-first) | `ts028_cursor_pages_without_gap_or_repeat_and_freezes_to`, `ts028_cursor_decode_failures_are_409_cursor_stale`, `ts028_cursor_for_another_query_or_future_to_is_409` |
| TS-028 (shape and summary) | `ts028_shape_and_event_kinds_always_has_all_eight_keys`, `ts028_summary_counts_dedup_and_separate_populations`, `ts028_summary_covers_whole_range_and_filters_apply`, `ts028_range_window_excludes_older_events`, `ts028_latency_is_nearest_rank_at_20_samples`, `ts028_events_are_newest_first_with_event_id_tie_break` |
| TS-028 (store and tolerance) | `ts028_tolerance_counters`, `ts028_ignores_non_store_entries_and_symlinks`, `ts028_expired_files_are_counted_and_not_shown`, `ts028_unreadable_file_is_counted_and_others_still_read`, `ts028_absent_store_is_an_empty_200_and_is_not_created`, `ts028_unsafe_store_is_503_store_unavailable`, `ts028_reader_never_writes_creates_or_deletes` |
| TS-028 (caps and cache) | `ts028_file_cap_returns_partial_200_newest_first`, `ts028_byte_cap_returns_partial_200`, `ts028_event_cap_returns_partial_200`, `ts028_cache_reuses_unchanged_files_and_reparses_changed_ones` |
| measurement (ignored, Unix) | `ts028_measure_parse_cache_memory_at_caps` |
