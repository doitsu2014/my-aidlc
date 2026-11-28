---
id: please-implement-plans-mcp-server-enhancement-sc-2
title: MCP telemetry, loopback monitor dashboard and 1.3.0 release prep (intent 2 of 2)
origin: intent
scope: solo
closed: 2026-11-28T07:15:55Z
phases-run: analyze, ideate, develop, archived
supersedes:
tags: mcp, telemetry, monitor, dashboard, loopback-http, installers, release-1.3.0, threat-model
---

# MCP telemetry, loopback monitor dashboard and 1.3.0 release prep (intent 2 of 2)

## Outcome

- Observed: 12 work units done (WU-011..WU-022), commits c23b4a8..483e65f plus 372b60f on branch feature/please-implement-plans-mcp-server-enhancement-sc. Final sweep 657 passed, 0 failed, 1 ignored (baseline 545/0). CI green on windows-latest, macos-latest and ubuntu-latest for 6aec542 and 483e65f (runs 46117480020, 46118325770). Version 1.3.0 prepared, not tagged, published or merged.
- Telemetry: `dtech-aidlc-mcp` records metadata-only events to `.aidlc/telemetry/` (code in `rust-cli/telemetry/`), on by default, `--telemetry off` records nothing; the CLI records nothing; MCP replies and stdout are byte-identical with telemetry on or off; background writer never blocks the caller (drops disclosed on stderr, not counted); retention 5 MiB per file, 7 days, 50 MiB total (soft by about one part).
- Monitor: `dtech-aidlc monitor` (`rust-cli/monitor/`) serves a read-only dashboard on 127.0.0.1 only; std-only HTTP/1.1; per-run OS-random token in the URL fragment, printed once on stderr; exact Host `127.0.0.1:<port>`, Origin and Sec-Fetch-Site checks; CSP with Trusted Types; 16 connections, 5 s head deadline, 8 KiB head; strict `/api/events` query (`rust-cli/telemetry/query.rs`) with frozen-window cursor and read caps (512 files, 64 MiB, 250,000 events).
- Release and hardening: installers install, upgrade and uninstall both binaries (marker v2); `deps add` takes the state lock; three flaky tests fixed; intent-1 MCP follow-up tests (`tests/mcp_followups.rs`); README telemetry and monitor sections; README MCP section no longer "proposed" (green CI plus a live Claude Code `tools/call`).
- Reviews: AI dashboard design review accepted (AC-022); AI threat-model review 0 high, 0 medium, 23 low with required fixes done (AC-030); headless Edge pass for keyboard, states, drill-down and SIGINT.

## Decisions

- User-provided (owner, 2026-11-28): Q1 telemetry MCP-only, default on with opt-out; Q3 zero new crates; Q2/Q4 scoped golden re-bless (version, `monitor` help/completion/validation, managed `/telemetry/` ignore line) - confirmed to cover `tests/golden/routing_golden/unsafe.json` (eight goldens, not the seven ADR-033 listed); Q5 1.3.0 prepare only; Q10/Q21 drop "proposed" after green CI and a live Claude Code tools/call; Q11 installers for both binaries; Q12 scope; Q13 validated ids may be stored (intent ids are slugs of the intent text - disclosed); Q15 AI reviews the dashboard design; Studio mock title "dtech-aidlc MCP Studio"; direction Option A.
- User-provided: ADR-025 accepted - with telemetry on, a handshake or list error reply leads to one `mcp.protocol.error` write (amends intent-1 ADR-005 rule 5; reply bytes unchanged).
- Recorded ADRs: ADR-019..ADR-039 in `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/ideate/direction-selection/decision-record.md` (token from `/dev/urandom` or BCryptGenRandom with no crate; constant-time compare; status/error table; telemetry hook in the MCP adapter, not the facade).
- AI conductor decisions (recorded, not owner approvals): IF-018 deviations - OWS stripped around the Authorization value (token still exact) and a bounded drain before every error reply except 408/503; WU-021 edited earlier units' files for the AC-030 fixes.

## Limitations

- Not verified: DeepSeek Harness (owner deferral); an AI host launching the Windows `.exe`; Windows firewall and `.cmd` Ctrl+C prompts; real screen reader and real Windows High Contrast on the dashboard.
- Accepted residuals: same-OS-user processes can read telemetry and the token (T2-06); the token URL stays in browser history and terminal scrollback (T2-03); other OS users reach static assets, Windows telemetry files inherit project ACLs (T2-05); any local client can exhaust the 16 slots (T2-09); drops not counted (T2-15).
- Tool and operation events carry no stageId, so the dashboard stage filter matches workflow events only.
- The query event cap can be exceeded by one rotated file; dashboard is cramped at 960-1040 px with the detail open.
- 372b60f (one CSS rule plus one test needle) has the local sweep but no CI run.

## Follow-ups

- Threat-review low findings: T2-20 client-side event shape check; T2-22b/T2-22d install.ps1 rollback-in-finally and path handling; T2-18 store `.gitignore` content; query whole-file re-read bound; impossible timestamps not counted as skipped; writer closed-set and id charset check; installer ownership matching and housekeeping.
- Contract text: amend IF-018 and ADR-029 (OWS, drain), T2-03 to Observed, ADR-033 golden list to eight.
- Refresh PROJECT.md (1.3.0, CI exists, MSRV 1.100, baseline 657/0) and `.aidlc/knowledge/_codebase-map.md` (telemetry, monitor, new test binaries) - Q16/Q25.
- Measure the Estimates (queue 1024, flush 500 ms, retention, 16 slots, timeouts) in early use against the ADR reopen triggers.
- Out of scope (Q12): pre-existing `cargo clippy --all-targets -- -D warnings` failure; actions/checkout Node 20.
- Launch actions (tag v1.3.0, GitHub release, merge or PR) each need a separate owner approval.

## Links

- Intent folder: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/` (intent record in `archived/intent-record/`)
- Release-candidate gate: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/develop/build-and-verify/gate-recommendation.md`
- Threat review: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/develop/build-and-verify/WU-021/threat-model-review.md`
- Dashboard design: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/develop/build-and-verify/WU-016/`
- Previous intent: `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md`
- Next-intent notes: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/archived/knowledge-writeback/next-intent-notes.md`
