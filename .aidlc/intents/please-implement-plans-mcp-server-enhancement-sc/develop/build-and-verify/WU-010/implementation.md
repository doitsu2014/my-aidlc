# Implementation: WU-010 (Fix pre-existing Unix test bugs)

- Purpose: the code change that completes WU-010.
- Status: Complete (build-and-verify, 2026-11-27). Cites WU-010, TS-019, AC-001, US-001.
- Kind: code manifest. Test code only; no product code changed (owner decision 2026-11-27).

| File | Change |
|---|---|
| `rust-cli/onboarding/mod.rs` (`#[cfg(test)] mod tests`, `rejects_traversal_and_absolute_paths`) | +4 lines: all-platform `assert!(normalize_source_path("/escape").is_err())`; the `C:/escape` `is_err()` assertion now under `#[cfg(windows)]` (unchanged text); `#[cfg(not(windows))]` asserts `Ok("C:/escape")` |
| `rust-cli/onboarding/projection.rs` (`#[cfg(test)] mod tests`, `refuses_symlink_targets`) | -1 line: removed the stray `fs::remove_file(.../SKILL.md).unwrap()`; the symlink is now created over an absent path and the existing `projection_status(...).is_err()` assertion runs |

`git diff --stat` for the unit: 2 files, 4 insertions, 1 deletion. No golden, Cargo.toml or Cargo.lock change. Codebase map: no component, entry point or test command changed, so no map update.
