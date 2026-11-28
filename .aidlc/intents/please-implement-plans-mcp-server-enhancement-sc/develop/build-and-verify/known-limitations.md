# Known limitations: release candidate

- Status: consolidated at the final sweep (2026-11-28). Limitations that were fixed or are cosmetic only are left in the per-unit files.
- Labels: Observed, Assumption, Open question. Cites AC-003, AC-006, AC-010, AC-011, AC-012, AC-013, AC-014, TS-012, TS-013, FN-022, FN-015, WU-007, WU-008, WU-009.

## Open items at the release-candidate gate

1. **TS-013 DeepSeek Harness launch-and-list not run (AC-003).** Observed: `dsh` is not installed on this machine, so DeepSeek Harness never launched `dtech-aidlc-mcp`. The owner decision requires BOTH Claude Code and DeepSeek Harness to launch and list the tools, so AC-003 is half evidenced. The README says plainly that DeepSeek Harness was never run against the server and that the `insert` entry shape comes from the dsh-mcp-client documentation. Assumption: `DTECH_AIDLC_HOME` survives the harness's scrubbed environment. Open question for the owner: accept the gate with this half open, or run it on a machine with `dsh`.
2. **TS-013 model-driven `tools/call` in a real Claude Code session not done.** Observed (partial): Claude Code 2.1.332 `claude mcp list` with an isolated `CLAUDE_CONFIG_DIR` and the real binary against an empty temp project reported "Connected"; the project directory stayed empty and `~/.claude.json` was not modified. Not done: a model asking for `tools/list` and `tools/call` in a live session.
3. **Windows host launch unverified.** Observed: CI built and tested on `windows-latest` (547 passed, 6 of 6 install, 29 of 29 MCP). Not done: a Windows host (Claude Code or DeepSeek Harness on Windows) launching `dtech-aidlc-mcp.exe`. The README says so.
4. **README platform sentence pending the owner.** Observed: the sentence near the install section (around line 191) still says macOS and Linux are unverified. By design it is updated by the owner only after CI is green on both OSes; that condition is now true (run 46090623346). Not edited by this sweep (no source edits). The owner should also update README.md line 421 ('A launch-and-list check in each host is a manual release check, not yet run'), because the Claude Code `claude mcp list` half is now partly done (item 2); the DeepSeek Harness half is still not run.
5. **AC-010 MCP test gap.** Observed: MCP tests cover only the omitted or blank label refusal and the human-mode gate with a named approver. Assumption (shared facade, AC-002): AI-mode gate approved with 'AI Orchestrator' and a valid recommendation file; AI-mode gate refused without a recommendation; human-mode gate refusing the 'AI Orchestrator' label; policy edited between calls. None has an MCP test.
6. **AC-011 unsafe-layout scenario untested over MCP.** Open question: state dir as an unsafe path or symlink (any tool call refused, nothing written) has no MCP-level test and test-strategy.md does not plan one. Assumption: the shared engine refusal applies.
7. **AC-012 mixed CLI and MCP writers untested.** Observed: the race tests are MCP-vs-MCP (two server processes) and in-process. Assumption: the CLI writer shares the same `with_state_lock` code path.
8. **AC-013 install smoke is narrower than the scenario.** Observed: `ts009_cargo_install_and_uninstall_both_binaries` does not assert that a settings file is unchanged and gives the MCP binary only initialize and tools/list, not help or version (same settings-bytes gap as AC-014).

## Limitations that still apply

- **Pre-existing timing flakes (not from this intent).** `prerequisites::tests::check_timeout_is_reported` (`sleep 1` against a 1 s timeout), `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. Observed only under heavy parallel load; none failed in the sweep or in CI. A follow-up should give them margin.
- **`deps add` gap (pre-existing, outside scope).** `deps add` saves intent state without the compare check or the state lock (FN-022). Not reachable over MCP.
- **Redaction value-space limitation (FN-015).** In values (not messages), an unregistered absolute path containing a space is redacted only up to the space; messages fall back to a generic text, so nothing leaks there. Paths after uncommon punctuation (`|`, `!`, `*`, `>`) or a non-ASCII letter are not token starts; URL schemes other than `file:` pass through.
- **State lock reopen ceiling (FN-022).** A waiter queued behind four or more successive holders that each retire the lock file gets the non-retryable "state.lock is stale" error (marked with a `ponytail:` comment). The open of the lock file checks `symlink_metadata` then opens without no-follow (a small race window; a symlinked `.aidlc` is already refused). An older build or manual edit could leave a marked file; the delete hint then applies.
- **State lock on Windows.** Observed: passed on CI. When another process holds `.aidlc` state open on Windows the result is a non-retryable `operation_failed` (documented in the README).
- **Concurrency test is timing-based.** `ts007_concurrent_reports_never_lose_an_update` observes a conflict through timing (4 rounds, 8 MiB padding) and asserts at least one conflict. If a runner ever reports zero conflicts, raise rounds or padding; do not drop the assertion. It passed on all three CI runners.
- **Prerequisites never run over MCP.** By decision, `next` and `report` do not execute configured prerequisite commands; `pendingPrerequisites` is a preview, computed with `work_units::peek` on ledger intents (ignores claim and parallel selection). Under skip, the hold warning keeps the CLI wording although nothing ran (cosmetic).
- **Approver label is self-asserted.** It is not authentication; the tool description and README say so.
- **Defence-in-depth resets not observable.** The scope, depth and `approved_by` resets in the MCP `next` path are never consulted because `intent` is forced to none; intent, mode, force-resume and skip resets are pinned by tests.
- **Wording gaps.** MCP `next` with no active intent returns `operation_failed` whose text suggests the CLI flag `--intent`, which MCP does not offer. The design document's "(id null)" list for wrong or missing `jsonrpc` differs from the code, which echoes a valid id (JSON-RPC allows it); align the design wording.
- **Validation details.** Length limits count characters (`needs` items by bytes); stage ids are validated by pattern, not against the live phase graph at call time; a final line of exactly 1 MiB plus a lone CR at EOF is accepted.
- **No golden for `orchestrate queue`.** Byte-compatibility was checked once by comparing pre- and post-change binaries; the new parity tests cover the MCP side.
- **CI.** The "Installer tests must not skip" step needs `sh` and `pwsh` on the runner (true on GitHub images; it would fail on a developer machine without `pwsh`). actionlint was not run (not installed; the YAML was parsed with Ruby). `actions/checkout@v4` targets the deprecated Node 20 runtime and is forced to Node 24; plan an upgrade. The Ubuntu leg is best effort by design.
- **Clippy.** `cargo clippy --all-targets -- -D warnings` still fails on the pre-existing warning at `tests/orchestration_golden.rs:1108` (not from this intent).

## Deferred to intent 2 (out of scope here)
- Plans telemetry events, workflow event instrumentation, the monitor dashboard, and hardening, README and release work (US-010..US-013). No CHANGELOG entry and no version bump (still 1.2.2); the MCP section in the README stays labelled proposed.

## Rollback
- See `implementation.md`: one commit per unit, revert in reverse order; the install writes nothing to any host.


## Owner decision at the gate (2026-11-28)

DeepSeek Harness verification (AC-003 second host) is deferred by the owner ("không cần deepseek harness giờ đâu"). Claude Code is the required host for this release. The README DeepSeek entry stays labelled proposed and not verified.
