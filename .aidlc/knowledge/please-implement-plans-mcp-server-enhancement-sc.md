---
id: please-implement-plans-mcp-server-enhancement-sc
title: Shared application facade and dtech-aidlc-mcp stdio server (intent 1 of 2)
origin: intent
scope: solo
closed: 2026-11-28T01:15:54Z
phases-run: analyze, ideate, develop, archived
supersedes:
tags: mcp, stdio, application-facade, json-rpc, state-lock, redaction, ci
---

# Shared application facade and dtech-aidlc-mcp stdio server (intent 1 of 2)

## Outcome

Intent 1 of 2 for `plans/mcp-server-enhancement/` (plan phases 1-2). Launch and Curate did not run (solo scope). Telemetry, the monitor dashboard and plan phases 3-5 are intent 2.

What exists now:

- A second binary, `dtech-aidlc-mcp`, in the same package (`rust-cli/mcp_main.rs`, `rust-cli/mcp/`). It speaks hand-rolled JSON-RPC 2.0 over stdio, supports both protocol eras (legacy `initialize` and the stateless `server/discover` flow), and exposes exactly four tools: `status`, `queue`, `next`, `report`.
- A shared application facade, `rust-cli/application.rs`. The CLI routes `status` and `orchestrate queue|next|report` through it. `Caller::Mcp` forces no intent creation, no prerequisite execution and an explicit approver label.
- An advisory state write lock around the state check-and-write, so a losing concurrent writer gets the retryable `state_conflict`.
- Redaction of every MCP result and error (`rust-cli/mcp/redact.rs`).
- CI in `.github/workflows/ci.yml` on `windows-latest` and `macos-latest` (required) and `ubuntu-latest` (best effort).
- A README MCP section that labels the server "proposed" and marks the DeepSeek Harness entry as not verified.

Evidence (Observed, from the release-candidate gate):

- Final sweep 545 passed, 0 failed, against the intent baseline of 454 passed, 2 failed. Both baseline failures were fixed by WU-010.
- CI run 46090623346: windows-latest 547/0, macos-latest 545/0, ubuntu-latest 545/0. Installer 6/6 with no skips and MCP transcript tests 29/29 on each.
- `Cargo.lock` and `tests/golden` have no diff against 69cab5a. The version stays 1.2.2.
- Claude Code 2.1.332 `claude mcp list` against the real binary reported "Connected". No model-driven `tools/call` was run.
- Acceptance: 12 of 14 criteria passed (AC-011 to AC-014 with caveats). AC-003 and AC-010 are partial. None failed.

## Decisions

Full text: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/decision-record.md` (ADR-001 to ADR-018). Later intents must respect these.

- **Hand-rolled JSON-RPC, zero new crates (ADR-001, ADR-002).** The CLI and MCP binary depend only on `serde_json` and `sha2`. Adding any crate needs a new owner decision.
- **MSRV 1.100 (ADR-003).**
- **Dual-era protocol in v1 (ADR-004, ADR-005).** This overrides the earlier legacy-only recommendation. Stdout carries only protocol frames, written by `serve`. Lines are capped at 1 MiB and CRLF is stripped.
- **Shared facade (ADR-006).** CLI and MCP go through `application`. Goldens must stay byte-identical. ADR-016 covers engine resolution sharing.
- **Tool names and schemas (ADR-009).** Exactly `status`, `queue`, `next`, `report`, with strict argument schemas. Renaming or adding tools is a contract change. ADR-007 and ADR-008 cover outcome and error shape, and ADR-010 the path form in output.
- **No prerequisite execution over MCP (ADR-011, ADR-012).** `next` and `report` return `pendingPrerequisites` marked not executed. The option is off for the CLI.
- **Approver label is required and self-asserted (ADR-013).** An approval without an explicit label is refused. There is no git or OS identity fallback. It is not authentication.
- **MCP `next` only continues an existing active intent (ADR-014).**
- **Ephemeral state lock replaces the ADR-015 "no lock" decision.** On 2026-11-28 the owner reopened Q10 after a lost-update test failure and chose an advisory file lock. This supersedes ADR-015's compare-then-rename-only position; the amendment is recorded in the decision record.
- **Supported hosts.** Claude Code is the required host for this release. DeepSeek Harness is deferred by the owner (2026-11-28). Windows and macOS are supported through CI (scope-boundary amendment, 2026-11-27).
- **No version bump and no CHANGELOG edit in intent 1 (ADR-018).** Install scripts stay CLI-only (N2). Host configuration docs live in the README (ADR-017).

## Limitations

Source: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/develop/build-and-verify/known-limitations.md`. Those that still apply:

- DeepSeek Harness never launched the server (`dsh` was not installed). A Windows host launch and a live-session Claude Code `tools/call` were also not run.
- Test gaps over MCP: AI-mode gate approval (AC-010), unsafe state layout (AC-011), mixed CLI and MCP writers (AC-012), and install smoke breadth (AC-013, AC-014). Behaviour rests on the shared facade and lock code paths.
- Redaction (FN-015): in values, an unregistered absolute path containing a space is cut at the space. Uncommon punctuation, non-ASCII starts and URL schemes other than `file:` pass through.
- State lock (FN-022): a waiter behind four or more successive holders that retire the lock file gets a non-retryable stale error. On Windows an externally held state file gives a non-retryable `operation_failed`. `deps add` saves without the lock or the compare check (pre-existing, not reachable over MCP).
- The concurrency test `ts007_concurrent_reports_never_lose_an_update` is timing-based. If a runner reports zero conflicts, raise rounds or padding. Do not drop the assertion.
- Pre-existing timing flakes: `prerequisites::tests::check_timeout_is_reported`, `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. Clippy `-D warnings` fails on the old warning at `tests/orchestration_golden.rs:1108`.
- `orchestrate queue` has no golden. Wording gaps: MCP `next` with no active intent suggests the CLI flag `--intent`, and the design doc's "(id null)" note differs from the code.
- CI: `actions/checkout@v4` runs on a deprecated Node 20 runtime. actionlint was not run. The installer-skip step needs `sh` and `pwsh` on the runner.

## Follow-ups

Open items from the release-candidate gate (14 follow-ups, none written-approval):

- Verify DeepSeek Harness launch-and-list (AC-003 second host, TS-013), and a live Claude Code `tools/call` and a Windows host launch.
- Owner edits README.md: the platform sentence near the install section (CI is now green on both OSes) and the "launch-and-list check ... not yet run" sentence.
- Add the missing MCP tests: AC-010 AI-mode and policy-edit cases, AC-011 unsafe layout, AC-012 mixed writers, AC-013 and AC-014 install smoke (`--help`, `--version`, settings bytes unchanged), and a restart/shutdown scenario for AC-003.
- Give the three timing flakes margin. Upgrade `actions/checkout`.
- Align the design wording on "(id null)" and the MCP `next` no-intent message.
- Optional owner call: tighten label character policy for both CLI and MCP (scope-boundary F2).
- Intent 2: telemetry, monitor dashboard, CHANGELOG entry, version bump from 1.2.2, removing the "proposed" label, install-script support. See `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/archived/knowledge-writeback/next-intent-notes.md`.

## Links

- Intent request: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/intent.md`
- Intent summary, completed scope, artifact index: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/archived/intent-record/`
- Decision record: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/decision-record.md`
- Scope boundary and amendments: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/scope-boundary.md`
- Release-candidate gate: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/develop/build-and-verify/gate-recommendation.md`
- Known limitations and verification: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/develop/build-and-verify/known-limitations.md`, `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/develop/build-and-verify/green-verification-report.md`
- Codebase map: `.aidlc/knowledge/_codebase-map.md`
- Source plans: `plans/mcp-server-enhancement/`
- Code: `rust-cli/application.rs`, `rust-cli/mcp/`, `.github/workflows/ci.yml`
