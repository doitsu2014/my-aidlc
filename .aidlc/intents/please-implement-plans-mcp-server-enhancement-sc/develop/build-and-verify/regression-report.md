# Regression report: final sweep (no unit)

- Status: Complete (2026-11-28). Machine-readable result: `regression.json` beside this file (`unit: sweep`, `scope: full`, `failures: []`).
- Cites AC-001, AC-013, TS-011, TS-012, TS-017, TS-019, WU-008, WU-010.

## Command and environment (Observed)
- Command: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast`
- Environment: macOS (Darwin 25.6.0, aarch64), rustc/cargo 1.100.0, `TMPDIR=/private/tmp/aidlc-test`, `target/debug` first on `PATH` (the agreed definition recorded in `develop/baseline.md`).
- Tree: integrated HEAD `ca30a85`.

## Comparison

| Run | Targets | Passed | Failed | Source |
|---|---|---|---|---|
| Intent baseline (commit `d765edd`, code identical to `69cab5a`) | 28 | 454 | 2 | `develop/baseline.md` (TS-017) |
| After WU-010 (`919e0b9`) | 28 | 456 | 0 | WU-010 regression report |
| Final sweep, local macOS | 33 | 545 | 0 | `sweep-suite.log` (conductor, Observed) |

## Every difference against the baseline (Observed)
- +91 passed in total (545 versus 454), made of:
  - 2 baseline failures now passing, both fixed by WU-010 (owner decision 2026-11-27): `onboarding::tests::rejects_traversal_and_absolute_paths` and `onboarding::projection::tests::refuses_symlink_targets` (TS-019). Both were already listed as known baseline failures.
  - 89 new passing tests (by unit: WU-001 +5, WU-002 +6, WU-004 +16, WU-006 +11, WU-003 +5, WU-005 +7, WU-007 +38, WU-008 +1; WU-009 and WU-010 add none beyond the two fixes). This sums to 89 and matches 545 - 456.
- Targets: 28 to 33 = +5: `mcp_packaging`, `mcp_stdio`, `mcp_parity`, `mcp_redaction`, and the new `dtech-aidlc-mcp` binary's test target. (Assumption on the exact split: `rust_cargo_install` already existed; the unit reports show 30 targets after WU-001 and 33 after WU-007.)
- No existing test changed from passed to failed. No test was removed or skipped.
- No golden changed: `git diff 69cab5a..HEAD -- Cargo.lock tests/golden` is empty (TS-011). Every golden suite passed unchanged in the sweep and in every unit run.
- One existing test was edited outside WU-010: `prerequisites::tests::ts010_skip_runs_no_command` (WU-005; the millisecond `startedAt` field is excluded from the comparison to stop a flake; test-only, still passing).
- Quality gates: `cargo fmt --check` clean. `cargo clippy --all-targets`: only the known pre-existing warning at `tests/orchestration_golden.rs:1108`; nothing new.

## CI per OS (Observed): GitHub run 46090623346, conclusion success
Pushed after the owner authorised it (no PR, no merge). Log: `ci-46090623346.log` in the conductor scratchpad.

| Runner | Test step | "Installer tests must not skip" | "MCP transcript tests" | Skips |
|---|---|---|---|---|
| windows-latest (required) | 547 passed, 0 failed (33 result lines) | 6 of 6 | 29 of 29 | 0 |
| macos-latest (required) | 545 passed, 0 failed | 6 of 6 | 29 of 29 | 0 |
| ubuntu-latest (best effort) | 545 passed, 0 failed | 6 of 6 | 29 of 29 | not reported |

- Windows runs 2 more tests than macOS (547 versus 545). Assumption: platform-gated tests (for example `#[cfg(windows)]` branches); not investigated.
- Only warning: `actions/checkout@v4` targets the deprecated Node 20 runtime (forced to Node 24).
- This is the first Windows evidence: Windows goldens, the state lock and the MCP suites all passed. TS-012 is satisfied for both required OSes.

## Flaky tests (not regressions)
- Observed in unit runs under heavy parallel load only, never in the sweep or in CI: `prerequisites::tests::check_timeout_is_reported` (failed once during WU-006, passed 5 of 5 alone and on re-run), `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. All pre-existing and not attributed to any unit.

## Verdict
- No regression outside the baseline. `regression.json` has `failures: []`, so the sweep can pass the release-candidate gate.
