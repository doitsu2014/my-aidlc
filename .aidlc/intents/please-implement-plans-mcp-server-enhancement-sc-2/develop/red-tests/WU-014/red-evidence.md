# Red evidence: WU-014

- Status: Complete (red-tests step, 2026-11-28). Cites TS-026, TS-027, FN-030, FN-031.
- Test-first: yes (isolated worktree at 2f9a33e; stubs with real signatures first).

## Observed
- TS-026 against stubs (on_error never called, `error_code_name` returning `protocol_error`): `cargo test --locked --lib mcp::protocol` 16 passed, 2 FAILED - `ts026_error_code_name_covers_the_ifs_table` (left "protocol_error", right "parse_error") and `ts026_on_error_table_maps_every_error_reply_to_a_fixed_code` (on_error never called). Behavioural.
- TS-027 first run: all 9 failed because the binary refused the unknown `--telemetry` flag (setup-level, not counted as behavioural red).
- TS-027 behavioural reds (review round): (1) `ts027_workflow_events_follow_a_durable_success` failed at tests/mcp_telemetry.rs:260 (`left: None, right: Some("context-and-risk")`) while stageId came from the optional call argument; (2) a temporary mutation storing the raw tool name made `ts027_sentinel_secrets_never_reach_the_store` fail (sentinel secret found in the raw store), then was reverted. Re-reviewer mutants also turned `ts027_concurrent_servers_write_separate_valid_stores` (constant session id) and `ts027_a_failing_store_changes_nothing_on_stdout` (disable line on stdout) red.
