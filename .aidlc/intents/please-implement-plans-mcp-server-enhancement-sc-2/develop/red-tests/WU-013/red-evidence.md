# Red evidence: WU-013

- Purpose: observed failing runs before the behaviour existed.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-021..TS-024, FN-023..FN-027.
- Test-first: yes (isolated worktree at 2ef7fa6). Stubs with the real signatures compiled first, so the tests executed and failed on behaviour, not on compilation.

## Observed red runs
- `TMPDIR=/private/tmp/aidlc-test-WU-013 cargo test --locked --lib telemetry:: -- --skip stalled` against the stubs: `test result: FAILED. 2 passed; 14 failed` (behavioural panics in TS-021, TS-022, TS-023 and TS-024 tests). The two that passed against stubs were later strengthened (review fix): `ts024_writer_never_creates_aidlc_and_drops_silently` now fails against a no-op store (Observed by the re-reviewer), and `ts024_prune_skips_symlinks_named_like_store_files`.
- `ts023_stalled_writer_never_blocks_the_producer` with the blocking stub hung for over 60 s and was killed: the behavioural red for the stall rule.
- Review-fix reds (Observed, mutants): build-panic and consumer-panic tests failed with `assertion failed: sink.disabled.load(Ordering::SeqCst)` when the disable arm was a no-op or `catch_unwind` was removed; `ts024_ordinary_eacces_is_a_write_failure_not_an_unsafe_location` failed while the unsafe check keyed only on PermissionDenied; `ts021_completed_accepts_null_duration_and_started_rejects_a_duration` failed before the parser was relaxed to IF-012.
- Q1 guards: green on the unchanged tree by design (they guard the current tree). Red shown by inserting `println!("x")` and `Sink::start(..)` into `rust-cli/knowledge.rs`: both guards failed with the expected messages; the file was restored.
