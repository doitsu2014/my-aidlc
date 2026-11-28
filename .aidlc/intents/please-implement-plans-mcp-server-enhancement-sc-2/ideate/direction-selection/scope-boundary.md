# Scope boundary: telemetry, monitor and release (intent 2 of 2)

- Purpose: state what intent 2 delivers under the selected direction (Option A, owner-decided), what it leaves out, what is deferred and under which trigger, and which decisions still belong to the project owner.
- Status: Draft - pending solution-direction gate.
- Stage: Ideate / direction-selection (decision-facilitator). Companions: [recommended-direction.md](recommended-direction.md), [decision-record.md](decision-record.md) (ADR-019..ADR-039).
- Sources: intent scope (User-provided: plan phases 3-5); `../../analyze/problem-framing/{user-stories,acceptance-criteria}.md`; `../../analyze/context-and-risk/{constraints-and-assumptions,open-questions}.md`; [solution-options.md](../option-generation/solution-options.md) s1, s4; owner decisions of 2026-11-28 (User-provided).
- Provenance labels: Observed, User-provided, Assumption, Estimate, Open question. No FN-/IF-/TS-/WU-/CFG- id is minted here.

## 1. In scope (intent 2)

| Item | Criteria / stories | Decision |
|---|---|---|
| MCP telemetry writer: background thread, bounded queue, failure isolation, drop disclosure | AC-004, AC-010, AC-018, AC-020, AC-028; US-002, US-004, US-010 | ADR-021 |
| Storage `.aidlc/telemetry/`, per-process files, permissions, IGNORE_BLOCK line plus self-ignore, rotation and retention | AC-008, AC-009, AC-012; US-003 | ADR-020 |
| Hook in the MCP adapter; test spawns pass `--telemetry off` | AC-006, AC-007, AC-027; US-002, US-010 | ADR-022 |
| `--telemetry on\|off` flag, default on | AC-001; US-001 | ADR-023 |
| Event schema v1 (eight kinds, AC-002 fields, validated ids), outcome/error mapping, protocol-error hook, ids | AC-002..AC-005; US-001, US-002 | ADR-024, ADR-025, ADR-026 |
| `dtech-aidlc monitor` (in-process, foreground) with the std loopback HTTP server, token, query/cursor API, embedded dashboard | AC-011, AC-013..AC-019; US-005, US-006 | ADR-027..ADR-032 |
| Dashboard states, metrics, filters, privacy disclosure, accessibility basics | AC-017..AC-021; US-006, US-007 | ADR-030, ADR-031, ADR-039 |
| AI dashboard design review before dashboard build; AI threat-model review before release | AC-022, AC-030; US-007, US-011 | Dashboard review: owner-decided (Q15). Threat-model review: Analyze gate [follow-up] under the AI orchestrator approval contract, not an owner decision. ADR-039 |
| `monitor` in help, completion and validation (scoped re-bless) | AC-025, AC-027 | ADR-033 (Q2, Q4) |
| Version 1.3.0 prepared (no tag), 7 goldens re-blessed with filtered-diff review, LOCK_SHA256, one CHANGELOG entry for intents 1 and 2 | AC-023, AC-024, AC-027; US-008 | ADR-033 |
| README telemetry and monitor docs; "proposed" label removed only with the Q10 evidence | AC-025; US-008 | ADR-034 |
| Installers install, upgrade and uninstall both binaries (sh and pwsh, tested in `cargo test`) | AC-026; US-009 | ADR-035 |
| Flaky-test fixes; missing intent-1 MCP tests | AC-027, AC-029; US-013 (Q12 items) | ADR-036 |
| `deps add` under `with_state_lock` | AC-027; US-013 (Q12 items) | ADR-037 |
| `mcp-studio-server.html` updated to the design | AC-011, AC-022 | ADR-038 |
| Windows and macOS CI green (Ubuntu best-effort), zero new crates | AC-029; US-010 | ADR-027, ADR-029 |

## 2. Out of scope

| Item | Why |
|---|---|
| DeepSeek Harness verification | Owner deferred (2026-11-28); US-012 out of scope. README keeps it "not verified". |
| Git tag, publish, crates.io, GitHub release | Q5: prepare only. |
| CI and lint cleanup: `actions/checkout` Node 20 upgrade, actionlint, clippy warning at `tests/orchestration_golden.rs:1108` | Q12 "Out". |
| CLI telemetry (operation events with caller `cli`) | Q1: the CLI writes none (Infeasible, options s4). |
| Telemetry default off / opt-in, or an env-var opt-out | Q1 default on; flag only (ADR-023). |
| Any new crate (HTTP, random, constant-time compare) | Q3. |
| A monitor supervisor or hidden child mode; a third binary; a monitor inside the MCP process | ADR-028; options s4 (OD4). |
| Monitor flags, config override, custom bind address, `/shutdown` route, any write route | ADR-028, ADR-032; AC-014. |
| Server push, heartbeat, process liveness | Plan s3.5 excludes process events; liveness stays "unknown" (AC-017). |
| Counted drop field; approve/reject value on `gate_decided` | Both need an AC-002 change (Analyze-owned) (ADR-021, ADR-024). |
| Park, revisit and intent pause/resume/abandon events | Not reachable over MCP (ADR-009); removed from v1 (ADR-024). |
| Snapshot helper skipping `.aidlc/telemetry` (Q24) | Not needed; test spawns pass `--telemetry off` (ADR-022). |
| Replacing the Windows CLI `.cmd` launcher; dropping `DTECH_AIDLC_ASSETS` | Beyond Q11 (ADR-035). |
| Chart click-to-filter by time bucket | Simplification of plan s3.6 for the AI design review; summary counts link to filters. |
| New MCP tools or tool-name changes | ADR-009, CON10. |
| Edits to `harness/` skill files | CON14. |
| PROJECT.md and codebase-map edits | Not editable by this workflow (Q16, Q25). |

## 3. Deferred, with triggers

| Item | Trigger to pick it up |
|---|---|
| DeepSeek Harness launch-and-list | Owner reopens US-012. |
| Dropped-event count on the dashboard | Drops observed in real use, or Analyze amends AC-002. |
| Gate approve/reject counts | AI design review or owner asks; needs an AC-002 `decision` field. |
| "Configured off" display | A trusted launch input appears (none exists under Q1). |
| Server push "shutting down", heartbeat | A separate bounded heartbeat design with its own review (design s7.5). |
| Replacing `BCryptGenRandom` or `/dev/urandom` with `std::random` | `std::random` stabilises within the MSRV. |
| Digest-based token compare | A reviewer distrusts `black_box` (ADR-029). |
| Separate p95 sample threshold, different refresh or cap values | Real sessions or the AI design review (ADR-030). |
| Real exe for the Windows CLI | Users report the batch prompt as confusing (ADR-028, ADR-035). |
| Monitor stdin-EOF stop | A supervisor or wrapper needs it (ADR-028). |
| README "proposed" removal | CI green on both OSes and a live Claude Code `tools/call` observed (ADR-034). |

## 4. Decisions that still belong to the owner (non-blocking follow-ups)

None of these blocks implementation-design.

| Item | Proposed default | Effect until answered |
|---|---|---|
| Studio mock title "Orca MCP Studio" (`mcp-studio-server.html` lines 2 and 130) | "dtech-aidlc MCP Studio" (ADR-038) | The Studio unit keeps the current title. |
| Q21: who supplies the live Claude Code `tools/call` observation, and when | The owner runs it once CI is green on both OSes | The "proposed" label stays; release notes say which phases are released (ADR-034). |
| Q16: refresh PROJECT.md (1.2.2, "no CI", MSRV, test baseline) | Owner edits after the release unit | PROJECT.md stays stale (R26). |
| Q25: refresh `.aidlc/knowledge/_codebase-map.md` | Owner assigns after intent 2 | Map stays stale. |
| Any AC amendment (drop counter, gate decision field) | Not proposed for v1 | ACs stay as written; ADR-039 readings apply. |

Residual risks in [threat-model.md](threat-model.md) (T2-03, T2-05, T2-06, T2-15, T2-22) are not in this table: none needs the owner's written approval at the solution-direction gate. The AC-030 AI threat review accepts each one or requires a change before release, and records the outcome. T2-10 (intent ids are slugs of user text) is already covered by the owner's Q13 decision. The owner may still raise any residual at the gate.

## 5. Plan statements superseded or amended

Recorded in [ADR-019](decision-record.md#adr-019-direction-option-a-and-the-owner-decisions-it-carries) and summarised in [recommended-direction.md](recommended-direction.md) section 3: the supervisor and hidden child (plan:305, :409, :456, :500; design s7.7), the no-token-in-hash rule for the token only (plan:278, :309; design:312), the facade as operation producer (design s6.3), the `cli`/park/revisit enumeration values (design s6.2), monitor config and time-window flags (plan s3.7, design s8), and plan:119 `--help`/`--version` (already superseded in intent 1).
