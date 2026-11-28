# Regression report: final sweep (unit "sweep")

- Status: Sweep green, no failures (Observed by the conductor, 2026-11-28). `regression.json` in this folder: `unit: sweep`, `scope: full`, `failures: []`.
- Command (Observed): `cargo build && TMPDIR=/private/tmp/aidlc-test-sweep2 PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast` on the integrated tree, HEAD 483e65f. `cargo fmt --check` clean. Log: `/private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/i2-sweep-suite.log`.
- Baseline (`develop/baseline.md`): 545 passed, 0 failed, 0 ignored (33 result lines) at 2ef7fa6 (code equal to intent-1 close 327b02a), same environment definition (macOS, `TMPDIR=/private/tmp/aidlc-test-<x>`, `target/debug` first on `PATH`). The baseline flags (`check_timeout_is_reported`, `verifies_empty_missing_and_not_applicable_reason`, `keeps_user_edits_unless_force_then_backups`) passed in the sweep; WU-011 owns them.
- Sweep: 39 result lines, 657 passed, 0 failed, 1 ignored.
- No failure outside the baseline; no baseline failure existed.

## Every difference from the baseline

1. Tests: +112 passed (657 - 545), +1 ignored (`ts028_measure_parse_cache_memory_at_caps`, the measurement test, Unix-only by design). Per unit (Observed in unit regression reports): WU-012 +1 (TS-031), WU-013 +24, WU-020 +7, WU-016 0, WU-014 +14 (net of the counting chain 577 to 591), WU-017 +24 (one ignored), WU-015 +8, WU-018 +22, WU-019 +7, WU-021 +5, WU-022 0, WU-011 0 (test-module fixes). The unit counts sum to 112 along the recorded chain 545, 546, 570, 577, 591, 615, 623, 645, 652, 657. Result lines grew from 33 to 39 (new test binaries: `deps_lock`, `telemetry_guards`, `mcp_telemetry`, `mcp_followups`, `monitor_http`, `monitor_assets`).
2. Golden files (Observed, `git diff --name-only 327b02a..HEAD -- tests/golden`): 8 files changed, not 9. The conductor note counted 9; the diff lists 8 (see Open question below). Every change falls under an owner-scoped reason (Q2/Q4):
   - `/telemetry/` managed ignore line (WU-013, TS-025): `routing_golden/lifecycle.json`, `lifecycle-embedded.json`, one line each (the `.aidlc/.gitignore` value).
   - `monitor` in help, completion and validation (WU-018, TS-037): `utility_golden/help-and-completion.json`, `routing_golden/validation.json` (help line plus the two monitor validation steps), and `routing_golden/unsafe.json` (one completion line; the eighth golden that ADR-033 missed; AI conductor decision recorded in WU-018, not an owner approval).
   - Version 1.2.2 to 1.3.0 (WU-021, TS-035): seven goldens, each `cmp`-equal to `sed s/1.2.2/1.3.0/` of its previous content: `cli_golden/version.json`, `routing_golden/harness.json`, `lifecycle.json`, `lifecycle-embedded.json`, `read-only.json`, `validation.json`, `utility_golden/help-and-completion.json`. (`unsafe.json` carries no version string; it changed only for the monitor line.)
   - Per-file diff counts (Observed): `unsafe.json` has no version or telemetry change and one monitor line; the lifecycle pair has one telemetry line each plus version lines.
3. Cargo.lock (Observed): the only difference since 327b02a is the `dtech-aidlc` package `version` line, 1.2.2 to 1.3.0. No dependency added or changed (AC-029).
4. Other files: 49 non-`.aidlc`, non-`plans` files changed since 327b02a (7750 insertions, 222 deletions), listed per unit in `implementation.md`. No existing test was deleted or weakened (Assumption: from the unit records; not re-diffed per test here).
5. Test-environment changes in existing tests: existing MCP snapshot spawns now pass `--telemetry off` (WU-014) so recorded transcripts stay byte-identical; the two `cargo install --path .` tests are serialised by a mutex (WU-020); temp-dir naming in test modules (WU-011).

## CI per OS (Observed, run 46117480020 on commit 6aec542; owner authorised the push; no PR, merge or tag)

| OS | Result | Tests |
|---|---|---|
| windows-latest | success | 693 passed, 0 failed (real PowerShell installer tests, the extended in-use test, BCryptGenRandom TS-022, monitor tests) |
| macos-latest | success | 702 passed, 0 failed (PowerShell tests run under pwsh) |
| ubuntu-latest | success | 702 passed, 0 failed |

- Counts are sums over all `test result` lines of each job log, including the separate MCP transcript step (Observed by the conductor in WU-022).
- Commit 483e65f (WU-022) was pushed with owner authorisation; CI run 46118325770 succeeded (windows-latest 693/0, macos-latest 702/0, ubuntu-latest 702/0). After the headless release-candidate pass a one-line forced-colors CSS fix and one test needle were added; the sweep was re-run on that tree: 657 passed, 0 failed, 1 ignored (log i2-sweep2-suite.log). The difference between 6aec542 and HEAD is documentation text and the test needles for it (Observed in WU-022).
- Windows reference from intent 1 (run 46090623346): 547 / 0.

## Not run

- `cargo clippy --all-targets -- -D warnings`: known pre-existing failure, out of scope (Q12).
- Windows local run: none; Windows evidence is CI only, and only on 6aec542.

## Open question

- Golden count: the conductor note says 9 golden files; `git diff` from 327b02a to HEAD lists 8. The reasons given account for exactly these 8 (2 telemetry, 3 monitor, 7 version, with overlaps). The owner may confirm that no ninth file was expected.
