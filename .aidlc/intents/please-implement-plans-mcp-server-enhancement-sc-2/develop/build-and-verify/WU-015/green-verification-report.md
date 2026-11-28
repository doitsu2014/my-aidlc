# Green verification: WU-015

- Status: Complete (2026-11-28). Cites TS-033, FN-044.
- Observed (worktree): `cargo test --test mcp_followups` 8 passed, 4 runs; clippy `-D warnings` clean on the target; full suite 600 / 0 at 164f09c + patch.
- Observed (conductor, main tree at d3f0312 + WU-015 + review fixes): `cargo test --locked --test mcp_followups` 8 passed in 3 of 3 runs; full suite 623 passed / 0 failed; `cargo fmt --check` clean.
- Trace: US-013 AC-027 FN-044 ts033_ai_mode_gate_follows_the_policy_edited_between_calls, ts033_cli_and_mcp_interleave_sequentially_on_one_state, ts033_cli_and_mcp_race_never_loses_an_update, ts033_restart_after_a_killed_session_resumes_from_the_recorded_state, ts033_shutdown_mid_session_is_clean_and_answers_what_was_sent PASS; US-010 AC-029 FN-044 ts033_unsafe_layout_at_launch_is_refused_and_writes_nothing, ts033_symlinked_state_directory_at_launch_is_refused (Unix), ts033_unsafe_layout_mid_session_is_refused_and_the_session_recovers PASS.
