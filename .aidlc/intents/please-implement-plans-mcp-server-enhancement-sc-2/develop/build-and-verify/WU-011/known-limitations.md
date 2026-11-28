# Known limitations: WU-011

- Windows: the `ping -n 6` change is unverified until windows-latest CI (Assumption).
- Loop B (one thread) was run before the fix only; Loops A and C cover the fix.
- Other crate `temp()` helpers (orchestrate.rs, state_migration.rs, onboarding/mod.rs, scope.rs) keep the pid + nanosecond pattern; none failed in the loops.
