# Existing suite baseline (intent please-implement-plans-mcp-server-enhancement-sc-2)

- Purpose: the existing test suite's result before any code change in this intent. Every regression report compares against this.
- Status: Recorded 2026-11-28 by the conductor, before any intent-2 work unit started.
- Commit: `27d8e91` on `feature/please-implement-plans-mcp-server-enhancement-sc` (code identical to intent-1 close `327b02a`).
- Environment (agreed definition, intent-1 owner decision N5/Q14): macOS (Darwin 25.6.0, aarch64), rustc/cargo 1.100.0, `TMPDIR=/private/tmp/aidlc-test-<x>`, `target/debug` first on `PATH`.
- Command (Observed): `cargo build && mkdir -p /private/tmp/aidlc-test-base2 && TMPDIR=/private/tmp/aidlc-test-base2 PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast`
- Prerequisites: `dtech-aidlc config prerequisite check` -> `{"ok": true, "results": []}` (none declared).

## Result (Observed)

- 33 test result lines; **545 passed, 0 failed**, 0 ignored. Lib unit tests 246 passed.
- Known intermittent tests (pass in this run, in scope for WU-011, owner decision Q12): `prerequisites` `check_timeout_is_reported` (failed once on CI ubuntu run 46093693872), `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. A failure of one of these before WU-011 is done is a known flake, owned by WU-011.
- Windows reference (Observed, intent-1 CI run 46090623346): windows-latest 547 passed / 0 failed; macos-latest 545 / 0; ubuntu 545 / 0.

## Not run here
- `cargo clippy --all-targets -- -D warnings` (known pre-existing failure, out of scope per Q12).
- Windows: no local run; windows-latest CI after an owner-authorized push.
