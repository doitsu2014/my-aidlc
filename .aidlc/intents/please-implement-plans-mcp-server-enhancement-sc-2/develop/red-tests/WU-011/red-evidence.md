# Red evidence: WU-011

- Purpose: the observed failing runs that confirm each flake's root cause before the fix (Q23 follow-up).
- Status: Complete (red-tests step, 2026-11-28). Cites TS-032, FN-038, AC-027, AC-029.
- Test-first: yes, as repeated-loop evidence on the unmodified tree (isolated worktree at 2ef7fa6). Environment: `TMPDIR=/private/tmp/aidlc-test-flake`, `target/debug` on `PATH`.

## Observed loops before the fix

- Loop A (40 runs, parallel): `cargo test --locked --lib -- artifacts::tests onboarding::projection::tests scope::tests prerequisites::tests::check_timeout_is_reported`. Failures: artifacts 2 (two tests, 1 each); projection 26 (creates_missing_projection_and_stamp 12, keeps_user_edits_unless_force_then_backups 10, refuses_symlink_targets 2, repeated_refresh_keeps_modified_obsolete_file 2); check_timeout_is_reported 1; scope::tests 0.
- Loop B (40 runs, `--test-threads=1`, control): check_timeout_is_reported 7; no temp-dir test failed.
- Loop C (40 runs of check_timeout_is_reported, `yes` on every core): 1 failure.

## Root causes (confirmed)

1. Temp-dir names built from pid + nanoseconds collide across parallel tests in the same process (fails only with parallelism: Loop A yes, Loop B no).
2. `check_timeout_is_reported` runs `sleep 1` against a 1 s timeout, a race that does not need parallelism (Loop B 7 of 40).

## Planned fix (handed to build-and-verify)
- Add a per-module `AtomicUsize` sequence to `temp()` in `artifacts.rs` and `onboarding/projection.rs`.
- Sleep 5 s (Windows `ping -n 6`) against the 1 s timeout; the runner kills the child at the timeout, so the test still takes about 1 s.
- `scope.rs` `temp()` is not changed: 0 failures in Loops A and B (work-units.md rule).
