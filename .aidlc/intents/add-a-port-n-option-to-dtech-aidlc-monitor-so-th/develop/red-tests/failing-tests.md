# Failing tests: monitor --port (intent 3, single unit)

- Status: Complete (red-tests, 2026-11-28). Cites AC-001..AC-010.
- Kind: code manifest.

| Test | File | AC |
|---|---|---|
| `parse_port_accepts_the_range_and_leading_zeros`, `parse_port_refuses_everything_else` | `rust-cli/monitor/mod.rs` | AC-003 |
| `port_flag_binds_the_chosen_loopback_port_in_both_forms_and_orders`, `port_flag_host_check_uses_the_chosen_port` | `tests/monitor_http.rs` | AC-001, AC-005 |
| `port_flag_accepts_leading_zeros`, `port_flag_refuses_invalid_values_with_the_fixed_line_and_writes_nothing`, `port_flag_boundary_65535_passes_validation`, `port_flag_missing_empty_and_option_like_values_are_refused`, `port_flag_duplicate_is_refused_and_nothing_is_bound`, `port_flag_validation_order_follows_the_decision` | `tests/monitor_http.rs` | AC-003 |
| `port_in_use_is_refused_without_fallback` (held listener; plain and leading-zero spelling) | `tests/monitor_http.rs` | AC-004 |
| `port_flag_token_is_new_on_every_run` | `tests/monitor_http.rs` | AC-006 |
| `args::tests::command_table_matches_cli_spec` (monitor gains `port`), `help_completion_and_the_command_table_stay_in_step` | `rust-cli/args.rs`, `tests/repo_content.rs` | AC-007 |
| `ts036_readme_telemetry_and_monitor_sections` (README and CHANGELOG needles) | `tests/repo_content.rs` | AC-008 |
| `status_port_effects_are_the_disclosed_ones` (D5 rows) | `tests/rust_cli.rs` | AC-010 |
| existing `ts029_two_monitors_get_distinct_ports_and_tokens` (no flag) | `tests/monitor_http.rs` | AC-002 |
