# Failing tests: WU-011 (Flaky-test fixes)

- Purpose: list the intermittent tests this unit fixes.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-032, FN-038, AC-027, AC-029, US-013, US-010.
- Kind: code manifest. No new test files; TS-032 is an evidence record (repeated loops before and after), and the fixes are in the existing `#[cfg(test)]` modules.

| Test | File | Test id | State before the fix (Observed, 40-run loops at 2ef7fa6) |
|---|---|---|---|
| `artifacts::tests::verifies_empty_missing_and_not_applicable_reason` (and siblings `resolves_standard_and_unit_paths_and_depth_subset`) | `rust-cli/artifacts.rs` | TS-032 | intermittent: 1 of 40 each (Loop A, parallel) |
| `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups` (and siblings `creates_missing_projection_and_stamp`, `refuses_symlink_targets`, `repeated_refresh_keeps_modified_obsolete_file`) | `rust-cli/onboarding/projection.rs` | TS-032 | intermittent: 10, 12, 2, 2 of 40 (Loop A) |
| `prerequisites::tests::check_timeout_is_reported` | `rust-cli/prerequisites.rs` | TS-032 | intermittent: 1 of 40 (Loop A), 7 of 40 (Loop B, one thread), 1 of 40 (Loop C, CPU load) |
