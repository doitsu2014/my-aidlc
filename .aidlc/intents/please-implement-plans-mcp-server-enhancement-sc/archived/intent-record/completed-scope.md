# Completed scope: please-implement-plans-mcp-server-enhancement-sc

- Purpose: list what intent 1 of 2 finished. This covers the ten work units, the acceptance-criteria status from the final sweep, and the follow-ups still open.
- Status: Draft for owner review (intent-record stage, 2026-11-28). Not an approved close.
- Sources: `state.json` (work-unit status and steps); `git log 69cab5a..HEAD` (commits); `develop/build-and-verify/green-verification-report.md` (acceptance-criteria statuses, copied as written); `develop/build-and-verify/gate-recommendation.md` (open items); `develop/build-and-verify/known-limitations.md`; `intent-summary.md`.
- Provenance labels: **Observed** = from the intent folder, git history or a named check. **User-provided** = stated by the project owner.

## 1. Work units

Observed. All ten are `done` in `state.json`. Each unit completed its `red-tests` and `build-and-verify` steps. No unit was removed or split.

| Unit | Title | Commit | Status |
|---|---|---|---|
| WU-001 | Scaffold and packaging | `9697529` | done |
| WU-002 | Prerequisite guard, error constant and pending preview | `4bc3ec2` | done |
| WU-003 | Facade and CLI routing | `e720b5f` | done |
| WU-004 | MCP protocol layer | `6b60591` | done |
| WU-005 | MCP tool adapter | `bbd9635` | done |
| WU-006 | Path redaction | `a986c73` | done |
| WU-007 | Integration, parity, install tests and state write lock | `bab236e` (design decision in `0487f49`) | done |
| WU-008 | CI workflow | `ca30a85` | done |
| WU-009 | README docs | `b2c4166` | done |
| WU-010 | Fix pre-existing Unix test bugs | `919e0b9` | done |

WU-007 was blocked once at red-tests (2026-11-28 00:24:35 UTC) because the concurrency test exposed a lost update. The owner's decision to add an advisory file lock is in `intent-summary.md`, section 5.

Record commits outside the units: `d765edd` (onboarding and the intent 1 analysis and design) and `38ebf90` (the final sweep and release-candidate approval).

## 2. Acceptance criteria: final status

Copied from `develop/build-and-verify/green-verification-report.md`, trace table, "Result" column. The status text is unchanged except that bold markers are removed. Summary: 12 of 14 passed (AC-011 to AC-014 with labelled caveats). AC-003 and AC-010 are partial. None failed.

| AC | Criterion | Final status |
|---|---|---|
| AC-001 | CLI behaviour unchanged | passed |
| AC-002 | One shared behaviour path | passed |
| AC-003 | Server launches and exposes tools | partial: handshake, discovery and Claude Code launch passed; DeepSeek Harness launch-and-list NOT RUN (required by the owner decision); model-driven `tools/call` NOT RUN; the restart scenario has no dedicated test named in the unit reports (Open question) |
| AC-004 | Stdout carries only protocol frames | passed |
| AC-005 | Requests validated before project access | passed |
| AC-006 | Status is read-only and path-safe | passed |
| AC-007 | Queue is a read-only preview | passed |
| AC-008 | Next-stage guard | passed |
| AC-009 | Report keeps validation, audit and concurrency | passed |
| AC-010 | Approvals over MCP follow the CLI gate policy | partial. Observed: the omitted or blank label refusal and the human-mode named-approver gate are tested. Assumption (covered by the shared facade, AC-002, no MCP test): AI-mode gate approved with 'AI Orchestrator' and a valid recommendation file; AI-mode gate refused without a recommendation; human-mode gate refusing the 'AI Orchestrator' label; policy edited between calls. See `known-limitations.md` |
| AC-011 | Errors are safe and not retried | passed for the cited scenarios. Open question: the 'unsafe project layout' scenario (state dir is an unsafe path or symlink, so any tool call is refused and nothing is written) has no MCP-level test and test-strategy.md does not plan one; Assumption: the shared engine refusal covers it. See `known-limitations.md` |
| AC-012 | Root fixed at launch; concurrent use safe | passed for MCP-vs-MCP and in-process races. Observed: `ts007_concurrent_reports_never_lose_an_update` races two MCP server processes (racer helper, `tests/mcp_parity.rs:384-410`); `ts020_racing_persist_active_calls_land_exactly_one_write` is an in-process race. No test mixes a CLI writer with an MCP writer; Assumption: the same `with_state_lock` code path covers the mixed case. See `known-limitations.md` |
| AC-013 | CLI optional and binaries install cleanly | passed, with caveats. Observed: `ts009_*` gives the MCP binary only initialize and tools/list, not help or version, and does not assert that a settings file is unchanged (same gap as AC-014); no background process or user-level assistant settings change is evidenced by the code only |
| AC-014 | Manual host configuration and honest labels | passed (docs present and labelled proposed; the install-writes-nothing scenario is evidenced by the code and the partial TS-013 check, not by a dedicated settings-bytes test; DeepSeek Harness entry is documentation only and was never exercised) |

Owner note (User-provided, 2026-11-28): the AC-003 row stays "partial" as observed. For this release, AC-003 is judged on Claude Code (Connected with the real binary), the protocol tests and CI.

## 3. What is complete

Observed, summarised from the sweep and CI evidence:

- Behaviour of the existing CLI is unchanged: 545 passed, 0 failed locally, against the baseline 454 passed and 2 failed (both fixed by WU-010). The golden suites are unchanged.
- The MCP stdio server exposes `status`, `queue`, `next` and `report`, validated before any project access, with redacted paths and no prerequisite command run over MCP.
- CI is green on `windows-latest` (547/0) and `macos-latest` (545/0). Installer tests and MCP transcript tests (29 of 29) pass on each OS.

## 4. Open follow-ups

Copied from `develop/build-and-verify/gate-recommendation.md`, "Open items" section. All are tagged [follow-up]. The gate has no [written-approval] item. The file records 14 follow-ups, which matches the approved gate in `state.json`.

1. **AC-003 / TS-013, DeepSeek Harness half.** Launch-and-list of `dtech-aidlc-mcp` in DeepSeek Harness is deferred by the owner ("không cần deepseek harness giờ đâu", 2026-11-28, User-provided). Claude Code is the required host for this release. The README keeps the DeepSeek Harness entry labelled proposed and not verified.
2. **README platform sentence** (`README.md:191`, "macOS and Linux builds have not been verified yet"). The owner updates it now that CI is green on Windows and macOS. Linux stays best effort. Re-run `cargo test --test repo_content` afterwards.
3. **README `launch-and-list` sentence** (`README.md:421`). The owner updates it to match the outcome of the AC-003 item. Keep the DeepSeek Harness "not verified" wording unless that check is actually run.
4. **TS-013 live session and Windows host.** A model-driven `tools/list` and `tools/call` in a live Claude Code session, and a Windows host launch of `dtech-aidlc-mcp.exe`. The CI evidence on Windows was agreed on 2026-11-27.
5. **AC-003 restart and shutdown.** Add a test that `state.json` and `audit.jsonl` are byte-identical across a server shutdown and restart. Currently bounded only by `ts005_empty_stdin_exits_zero_and_writes_nothing`, the ts006 read-only checks and the per-call project open.
6. **AC-010 MCP tests.** Add MCP-level tests for the AI-mode gate scenarios, the human-mode refusal of "AI Orchestrator", and policy edits between calls. Until then these rely on the shared facade (AC-002, passed).
7. **AC-011 unsafe layout.** An MCP-level test for an unsafe state directory or symlink layout (refused, nothing written). It is neither planned in `test-strategy.md` nor present.
8. **AC-012 mixed writers.** A test that mixes a CLI writer and an MCP writer under `with_state_lock` (FN-022). The current races are MCP against MCP and in-process.
9. **AC-013 and AC-014 install checks.** Extend `ts009_cargo_install_and_uninstall_both_binaries` (TS-009) to check MCP `--help` and `--version`, and to assert that host settings files are byte-unchanged.
10. **Performance.** The CLI and MCP latency bound was never measured (N6). Record a measurement or accept the estimate.
11. **Pre-existing timing flakes** (not from this intent): `prerequisites::tests::check_timeout_is_reported`, `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. Give them timing margin.
12. **CI hygiene** (CFG-003, WU-008). Move `actions/checkout` off the deprecated Node 20 runtime and run actionlint. The pre-existing clippy warning at `tests/orchestration_golden.rs:1108` is a separate item.
13. **Limitations:** redaction value-space limit (FN-015); the state lock reopen ceiling (FN-022, marked with a ponytail comment); the `deps add` path without the lock (pre-existing, not reachable over MCP); and the wording gaps in `known-limitations.md`.
14. **Release work** stays in intent 2: CHANGELOG entry, version bump from 1.2.2, removing the "proposed" label, and install-script support (N2).

Other open items from the earlier gates (solution-direction, 16 follow-ups; analysis-baseline, 8 follow-ups) are in their `gate-recommendation.md` files and are not repeated here.
