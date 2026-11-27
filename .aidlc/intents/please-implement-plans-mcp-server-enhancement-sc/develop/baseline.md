# Existing suite baseline (intent please-implement-plans-mcp-server-enhancement-sc)

- Purpose: the existing test suite's result before any code change in this intent. Every regression report compares against this.
- Status: Recorded 2026-11-27 by the conductor, before WU-001 and WU-010 started.
- Commit: `d765edd` on `feature/please-implement-plans-mcp-server-enhancement-sc` (code identical to `69cab5a`).
- Environment (agreed definition, owner decision N5/Q14): macOS (Darwin 25.6.0, aarch64), rustc/cargo 1.100.0, `TMPDIR=/private/tmp/aidlc-test`, `target/debug` first on `PATH`.
- Command (Observed): `cargo build && mkdir -p /private/tmp/aidlc-test && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast`
- Prerequisites: `dtech-aidlc config prerequisite check` -> `{"ok": true, "results": []}` (none declared).

## Result (Observed)

- 28 test targets; **454 passed, 2 failed**, 0 ignored.
- Failures, both in the lib unit tests (`rust-cli/lib.rs`, 191 passed / 2 failed), and both listed in `PROJECT.md` "Known baseline failures":
  - `onboarding::tests::rejects_traversal_and_absolute_paths` (Unix-only test bug: `C:/escape` is not absolute on Unix).
  - `onboarding::projection::tests::refuses_symlink_targets` (setup never creates the file the test removes).
- These two are owned by WU-010 (owner decision 2026-11-27: fix them in this intent). After WU-010 the expected baseline is 456 passed, 0 failed.
- All other targets pass, matching PROJECT.md "All other targets pass".

## Not run here
- `cargo clippy --all-targets -- -D warnings` (known to fail on `tests/orchestration_golden.rs:1108`, PROJECT.md); `cargo fmt --check` was clean earlier on 2026-11-27.
- Windows: no local baseline. The first CI run (WU-008, after an owner-authorized push) establishes it.
