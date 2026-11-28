# Red evidence: WU-007

- Status: Complete (red-tests, 2026-11-28). Test-first: yes for the state write lock (FN-022/TS-020, owner decision 2026-11-28) and for the lost-update regression (TS-007); the other integration tests were written alongside, as test-strategy.md specifies.

## Lost update (TS-007), before any lock
- First worker run (base bbd9635): `TMPDIR=/private/tmp/aidlc-test-WU-007 cargo test --test mcp_parity` -> 7 passed, 1 failed: `ts007_concurrent_reports_never_lose_an_update`, "round 0: 2 replies recorded a report but state holds 1" (left 1, right 2). A probe reproduced it in 12/12 rounds (two `dtech-aidlc-mcp` processes, 8 MiB padded intent state): both replies `recorded: true`, two `stage.reported` audit rows, one report in `state.json`. Cause: `ensure_unchanged` then `persist_active` with no lock (orchestrate.rs). This led to the owner decision and the rework (WU-007 parked as design-defect, `intent rework --unit WU-007`).
- Re-run on the reworked base (0487f49) with the tests applied: same failure.

## Lock tests (TS-020), written before the implementation
- Against a pass-through `with_state_lock_for` stub: `cargo test --lib ts020` -> 4 failed, 4 passed. Failures: `ts020_racing_persist_active_calls_land_exactly_one_write` (both racers Ok: "left 2 right 1"), `ts020_held_lock_times_out_with_state_changed_without_running_f` (unwrap_err on Ok), `ts020_still_marked_file_gives_the_stale_hint_not_state_changed` (unwrap_err on Ok), `ts020_permission_denied_on_open_is_retried_within_the_budget` (attempts 0 vs 3). The 4 passes are guards that hold either way.
- All are assertion failures in executing tests, not compile or setup errors.
