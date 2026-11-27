# Red evidence: WU-010

- Purpose: the observed failing run that this unit fixes.
- Status: Complete (red-tests step, 2026-11-27). Cites TS-019, AC-001.
- Test-first: yes, in the sense that the failing tests already existed. The defect is in the tests themselves, not in product code, so the red run is the existing failure on the unmodified tree.

## Observed red run (isolated worktree at d765edd, before any change)

- Command: `TMPDIR=/private/tmp/aidlc-test-WU-010 cargo test --lib onboarding`
- Result: `test result: FAILED. 14 passed; 2 failed; 177 filtered out`
  - `onboarding::tests::rejects_traversal_and_absolute_paths ... FAILED`, panicked at `rust-cli/onboarding/mod.rs:1597:9`: `assertion failed: normalize_source_path("C:/escape").is_err()`. On Unix `C:` is an ordinary path component, so `Path::is_absolute` is false and the call returns `Ok("C:/escape")`. The assertion is only true on Windows.
  - `onboarding::projection::tests::refuses_symlink_targets ... FAILED`, panicked at `rust-cli/onboarding/projection.rs:463:69`: `fs::remove_file(...SKILL.md).unwrap()` on `NotFound`. `setup()` never creates that file.
- Both are behavioural assertion/panic failures at the intended lines, not compile or setup errors in the harness. The same two failures appear in the intent baseline full-suite run (454 passed / 2 failed).

## Planned fix (handed to build-and-verify)
- `rejects_traversal_and_absolute_paths`: keep the `C:/escape` `is_err()` assertion under `#[cfg(windows)]` (not weakened), assert `Ok("C:/escape")` under `#[cfg(not(windows))]`, and add an all-platform `normalize_source_path("/escape").is_err()` so Unix absolute paths stay covered.
- `refuses_symlink_targets`: delete the stray `remove_file` line inside the test (not in the shared `setup()`), so the symlink is created over an absent path and `projection_status(...).is_err()` is asserted as before.
