# Failing tests: WU-015 (Missing intent-1 MCP tests)

- Purpose: list the characterisation tests this unit adds.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-033, FN-044, AC-027, AC-029, US-013, US-010.
- Kind: code manifest. New file `tests/mcp_followups.rs`. Characterisation tests are expected green on arrival (work-units.md), so this list is the planned checks, not failing tests.

| Test | Case |
|---|---|
| `ts033_ai_mode_gate_follows_the_policy_edited_between_calls` | AI-mode gate; recommendation and gate-mode pin edited between calls of one session |
| `ts033_unsafe_layout_at_launch_is_refused_and_writes_nothing` | unsafe state layout at launch (portable: file instead of directory) |
| `ts033_symlinked_state_directory_at_launch_is_refused` | symlinked `.aidlc` at launch (Unix only) |
| `ts033_unsafe_layout_mid_session_is_refused_and_the_session_recovers` | unsafe layout mid-session, recovery after restore |
| `ts033_cli_and_mcp_interleave_sequentially_on_one_state` | CLI and MCP interleaving, sequential |
| `ts033_cli_and_mcp_race_never_loses_an_update` | CLI and MCP interleaving, concurrent |
| `ts033_restart_after_a_killed_session_resumes_from_the_recorded_state` | restart after a killed session |
| `ts033_shutdown_mid_session_is_clean_and_answers_what_was_sent` | shutdown mid-session, including EOF mid-line |
