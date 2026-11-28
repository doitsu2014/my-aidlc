# Red evidence: WU-017

- Status: Complete (red-tests step, 2026-11-28). Cites TS-028, FN-032.
- Test-first: yes (isolated worktree at db30d93). Stubs with the real signatures (`query`/`query_with` returning `Ok({})`, `Cache::len`/`approx_bytes` returning 0) and the full test module first.
- Observed: `TMPDIR=/private/tmp/aidlc-test-WU-017 cargo test --lib telemetry::query` -> `test result: FAILED. 0 passed; 23 failed; 1 ignored`. Behavioural, e.g. `ts028_rejects_bad_parameters_with_invalid_filter` and `ts028_cursor_decode_failures_are_409_cursor_stale` panicked at `called Result::unwrap_err() on an Ok value: Object {}`. The parameter-rejection and cursor tests were red before `query.rs` had behaviour, as WU-017 requires.
