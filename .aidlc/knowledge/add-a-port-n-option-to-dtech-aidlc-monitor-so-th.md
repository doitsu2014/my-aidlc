---
id: add-a-port-n-option-to-dtech-aidlc-monitor-so-th
title: dtech-aidlc monitor --port (fixed loopback port, folded into 1.3.0)
origin: intent
scope: solo
closed: 2026-11-28T08:16:46Z
phases-run: analyze, develop, archived
supersedes:
tags: monitor, cli-flag, loopback, release-1.3.0
---

# dtech-aidlc monitor --port (fixed loopback port, folded into 1.3.0)

## Outcome

- Observed: `dtech-aidlc monitor --port <n>` (also `--port=<n>`) binds 127.0.0.1:<n> for n in 1..65535 (leading zeros allowed); without the flag the monitor still picks a free port. Invalid values exit 1 with `Option --port requires a whole number from 1 to 65535`; a busy or forbidden port exits 1 with `Cannot bind 127.0.0.1:<n> (port in use or not permitted)` and never falls back. The token is still new on every run. Commit e319bac; suite 670/0/1 ignored (baseline 657/0); CI run 46124668134 green on windows-latest (706/0), macos-latest (715/0), ubuntu-latest (715/0).
- Fast depth: Analyze (problem-framing) and Develop ran; Ideate, intent-record, Launch and Curate did not.

## Decisions

- User-provided (owner, 2026-11-28): fast depth; fold into the unreleased 1.3.0 (no version bump, one CHANGELOG line); same branch; scoped golden re-bless for monitor/--port help, completion and validation lines (routing_golden/validation.json, routing_golden/unsafe.json, utility_golden/help-and-completion.json).
- AI conductor decision D5: `--port` joins the merged flag table like `--depth`, so invalid invocations of other commands (`status --port`, `status --port=`, `status --intent --port`, `status --port 1 --port 2`) print a different stderr line; exit codes unchanged; pinned by `status_port_effects_are_the_disclosed_ones` in tests/rust_cli.rs.
- Validation order (D4): parser errors, unknown option, positional limit, project safety, port validation, telemetry check, token, bind.
- Windows: SO_EXCLUSIVEADDRUSE not set (would need untested Windows-only unsafe code); same-user address sharing recorded next to T2-06.

## Limitations

- A fixed port does not make a bookmark work on its own: without `#token=` the page shows Token missing.
- Low-port privileges and Windows excluded port ranges are documented in the README, not tested.
- Another local user can pre-bind a well-known port; the monitor then refuses to start and prints no token.
- No test proves the bind is loopback-only (review only).

## Follow-ups

- Owner may ask to keep `--port` out of the merged flag table if the D5 stderr change matters (parser rewrite).
- A persistent or pasted token for bookmarks would be a separate security decision.
- Loopback-only bind test on Linux/macOS.

## Links

- Intent folder: `.aidlc/intents/add-a-port-n-option-to-dtech-aidlc-monitor-so-th/`
- Plan: `.aidlc/intents/add-a-port-n-option-to-dtech-aidlc-monitor-so-th/develop/implementation-design/implementation-plan.md`
- Release-candidate gate: `.aidlc/intents/add-a-port-n-option-to-dtech-aidlc-monitor-so-th/develop/build-and-verify/gate-recommendation.md`
- Monitor and telemetry record: `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc-2.md`
