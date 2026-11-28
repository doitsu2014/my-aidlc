# Green verification: monitor --port (intent 3)

- Status: Complete (2026-11-28).
- Observed (worker, worktree at 31d0fef): red run against stubs (see red-tests); after implementation the `port_` tests passed 5 of 5 loops; full suite 670 passed / 0 failed / 1 ignored.
- Observed (independent review on a clean export): fmt, build and full suite 670/0; golden word-diff limited to monitor/--port words in three files; mutations checked for validation order, Host check and messages.
- Observed (conductor, main tree e319bac): `monitor_http` 29 passed in 3 of 3 runs; fallback mutation now fails in 10.02 s; full suite 670 passed / 0 failed / 1 ignored; Cargo.toml/Cargo.lock unchanged.
- CI (Observed): run 46124668134 on e319bac (owner-authorised push) success - windows-latest 706/0, macos-latest 715/0, ubuntu-latest 715/0.

| AC | Tests | Result |
|---|---|---|
| AC-001 | port_flag_binds_the_chosen_loopback_port_in_both_forms_and_orders | passed |
| AC-002 | ts029_two_monitors_get_distinct_ports_and_tokens | passed |
| AC-003 | parse_port_* ; port_flag_refuses_invalid_values_with_the_fixed_line_and_writes_nothing ; port_flag_accepts_leading_zeros ; port_flag_boundary_65535_passes_validation ; port_flag_missing_empty_and_option_like_values_are_refused ; port_flag_duplicate_is_refused_and_nothing_is_bound ; port_flag_validation_order_follows_the_decision | passed |
| AC-004 | port_in_use_is_refused_without_fallback | passed |
| AC-005 | port_flag_host_check_uses_the_chosen_port | passed |
| AC-006 | port_flag_token_is_new_on_every_run | passed |
| AC-007 | command_table_matches_cli_spec ; help_completion_and_the_command_table_stay_in_step ; golden word-diff | passed |
| AC-008 | ts036_readme_telemetry_and_monitor_sections | passed |
| AC-009 | all of the above; CI run 46124668134 on windows, macos, ubuntu | passed (listed OS limits documented, not tested) |
| AC-010 | status_port_effects_are_the_disclosed_ones ; Cargo files unchanged ; full suite | passed |
