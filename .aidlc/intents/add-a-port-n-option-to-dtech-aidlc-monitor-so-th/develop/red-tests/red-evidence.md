# Red evidence: monitor --port

- Status: Complete (red-tests, 2026-11-28).
- Test-first: yes (isolated worktree at 31d0fef; stubs: `parse_port` returned Ok(0), `run` ignored the port, flag table unchanged).
- Observed: the two `parse_port` unit tests and `command_table_matches_cli_spec` FAILED; `monitor_http` `port_` tests 10 failed / 0 passed (e.g. `Unknown option --port for 'monitor'. Allowed options: --project.` instead of the expected message; spawn tests had no startup line); `ts036_readme_telemetry_and_monitor_sections`, `help_completion_and_the_command_table_stay_in_step` and `status_port_effects_are_the_disclosed_ones` FAILED. All behavioural.
- Review round (Observed): with a fallback mutation (`or_else` bind to port 0) `port_in_use_is_refused_without_fallback` used to hang; after the conductor added a 10 s deadline to `run_refused` it fails with `monitor did not exit within 10 s: it started serving` (10.02 s).
