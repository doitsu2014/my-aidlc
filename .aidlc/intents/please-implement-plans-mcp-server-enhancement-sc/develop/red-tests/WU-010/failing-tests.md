# Failing tests: WU-010 (Fix pre-existing Unix test bugs)

- Purpose: list the tests that were failing before this unit and are the subject of its fix.
- Status: Complete (red-tests step, 2026-11-27). Cites TS-019, AC-001, US-001.
- Kind: code manifest. No new test files; both tests are existing unit tests fixed in place, as WU-010 specifies.

| Test | File | Test id | State before the fix (Observed) |
|---|---|---|---|
| `onboarding::tests::rejects_traversal_and_absolute_paths` | `rust-cli/onboarding/mod.rs` (`#[cfg(test)] mod tests`) | TS-019 | FAILED on macOS: `assertion failed: normalize_source_path("C:/escape").is_err()` (mod.rs:1597) |
| `onboarding::projection::tests::refuses_symlink_targets` | `rust-cli/onboarding/projection.rs` (`#[cfg(test)] mod tests`, `#[cfg(unix)]` block) | TS-019 | FAILED on macOS: `unwrap()` on `Err(Os { code: 2, kind: NotFound })` at projection.rs:463 |

Both failures are recorded in the intent baseline (`develop/baseline.md`) and in `PROJECT.md` "Known baseline failures".
