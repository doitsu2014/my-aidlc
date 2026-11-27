# Problem statement: MCP server enhancement (intent 1 of 2)

- Purpose: frame the problem, scope and open decisions for the shared application facade plus MCP stdio server.
- Status: Draft - pending analysis-baseline gate
- Sources: intent.md; plans/mcp-server-enhancement/{mcp-server-architecture.md, mcp-server-detailed-design.md, mcp-server-observability-implementation-plan.md}; rust-cli/{main.rs, orchestrate.rs, status.rs, unit_commands.rs, identity.rs, gate.rs}; Cargo.toml; PROJECT.md

## Problem
- Observed (architecture.md s1): dtech-aidlc is a local-first Rust workflow engine. The CLI owns stage selection, state, report validation, gates and audit. Assistant harnesses reach it only through projected slash-command skills that shell out to the CLI.
- Observed (plan header): MCP-capable assistants have no structured way to discover and call a few AIDLC operations without running the CLI and parsing its output.
- Observed (code): behaviour is reachable only through the binary entry point. Argument validation, root resolution, engine selection (DTECH_AIDLC_HOME), preflights and finalizers live in rust-cli/main.rs, not in a reusable layer.
- Assumption: no measured pain point or baseline exists in the docs (the "why" is qualitative). Impact: success measures are capability and safety based, not numeric. Validate: owner confirms.

## Desired outcome (User-provided scope, plans as proposal)
1. The CLI and an MCP server call the same application operations, with CLI behaviour unchanged.
2. A local stdio MCP server exposes four tools (status, queue, next, report) with strict schemas.
3. The MCP path preserves path safety, gate rules, concurrency checks and audit semantics, and never becomes a second workflow engine or an authorization authority.

## In scope (this intent)
Plan phases 1-2: shared application facade (CLI refactor, incremental), MCP stdio server binary, four tools, manual host-config docs, install/package smoke tests, and the related architecture/design parts.

## Out of scope (intent 2, starts after this intent closes)
Telemetry schema/sink/retention/redaction, workflow event instrumentation, the monitor command and dashboard (including mcp-studio-server.html), plan phases 3-5 (hardening, README/changelog, release). See out-of-scope stories in user-stories.md.

## Non-goals carried from the plans (Observed, plan s1)
No CLI replacement; no remote/HTTP transport; no arbitrary shell/Git/filesystem tools; no second state store; no phase-graph, state or audit schema change; no auto-editing of host config; tools only (no MCP resources, prompts, subscriptions, sampling); no authenticated-approval flow.

## Constraints (Observed)
- All MCP pieces are Proposed and must be labelled so until implemented and released (plan s6).
- Project root is fixed at launch, never per call (design s3.1).
- Existing goldens stay unchanged (plan Phase 1 exit). Baseline per PROJECT.md:310-332 has 17 macOS failures under default TMPDIR and 1 clippy warning, so "unchanged" needs a defined environment.

## Where plans pre-commit to choices that remain open
The plans describe an SDK-based server, an application.rs module and named flags. This framing treats these as the owner's proposal, not decided solutions.

## Contradictions found (documents vs documents, documents vs code)
1. Facade operation names: design s3.2 `status/queue/next/report` vs plan s3.1 `status/orchestrate_next/orchestrate_report/orchestrate_queue`.
2. Telemetry flag: plan s3.2 states `--telemetry on|off` default on; plan s3.7, s8.1 and design s4.1, s8 call flag and default undecided.
3. Paths in output: plan s3.3 says include relevant artifact paths; design s4.3 requires redacting or relativizing every absolute path. Path form undefined.
4. Ordering: plan s2.3 fixes validate -> preflight audit -> persistence -> audit; architecture s6.3 says ordering is per command.
5. Open-decision lists differ: design s10.2/10.3 (prerequisites, AI-mode gate UX) absent from plan s8.
6. Prerequisites: plan s3.3 and design s4.3 require aidlc_next not to run prerequisite shell checks, but code runs the checks on the next path (run_prerequisites at orchestrate.rs:1470, 1544, 1694, all inside orchestrate_next, which starts at line 1142; main.rs:475-505). The --force-resume flag (NextOptions.force_resume, orchestrate.rs:240) does not gate execution; it only overrides the resulting hold, so it is not a no-execute mode, and the design gap stands. Observed: the CLI report path (orchestrate_report, finish_report; main.rs:437-457) runs no prerequisite checks and only validates that needs names are known (require_known_prerequisites, orchestrate.rs:2986). Plan schedules no task for the next-path guard.
7. Facade return type: design s3.2 `next -> Result<Value, AppError>` cannot carry CLI exit code 3 for Blocked (main.rs:102-127) without extra outcome data.
8. Design s3.1 "canonicalized" root vs code resolve_like_node (orchestrate.rs:381) which only normalizes lexically.
9. Design s3.1 "handlers consume graph.value()" vs status::status taking &Graph (status.rs:233).
10. Idempotency: design s5 mentions operation IDs; no tool schema defines one.
11. SDK decision timing: "before implementation" (arch s9, design s4.2) vs "Phase 0 exit" (plan) vs "before release implementation" (arch s11 AC8).
12. Line references drift slightly (e.g. run() at main.rs:28, not 22-31). No impact on requirements.

## Open questions (questions 1, 3, 5, 6 decided 2026-11-27, question 2 answered by the methodology; see open-questions.md for the rest)
1. DECIDED (User-provided, project owner, 2026-11-27): supported hosts are Claude Code and DeepSeek Harness; both must pass launch-and-discover (AC-003). Ideate pins the protocol revision.
2. ANSWERED BY THE METHODOLOGY (confirm at the Ideate gate): Ideate precedes Develop and the plans are proposal-only, so Phase 1 may follow Ideate approval and the SDK decision lands at the Ideate gate. Original question: can Phase 1 start before the SDK spike (Phase 0) and design approval (Phase 0b) finish? Is the existing design considered approved?
3. DECIDED (User-provided, project owner, 2026-11-27) policy "MCP binary only": at most one reviewed SDK tree, used only by the MCP binary; the CLI binary keeps only serde_json and sha2; Cargo.lock committed; --locked and offline cargo install keep working; MSRV = current stable. Hand-rolled JSON-RPC remains allowed with an ADR; which SDK is still Ideate research.
4. Approve incremental extraction boundaries and exact tool schemas/side effects (arch s9).
5. DECIDED (User-provided, project owner, 2026-11-27) "Never from MCP": MCP next and report never run owner-configured prerequisite shell checks; they return pending prerequisites marked not executed. The guard lives in the shared facade as an option that is off for the CLI, so CLI behaviour stays byte-identical. Scheduling of the guard task remains Ideate/Develop.
6. DECIDED in part (User-provided, project owner, 2026-11-27) "Accept approver label": MCP report takes an approver field like the CLI --approved-by, self-asserted by the caller, with the same gate policy as the CLI (AI-mode needs "AI Orchestrator" plus gate-recommendation checks; human-mode refuses "AI Orchestrator"). Still open for Ideate: behaviour when the label is omitted (proposed default: refused, no git/OS identity fallback), refusal error codes, audit provenance (approval came via MCP).
7. Facade return shape: how outcome kind/exit code (Blocked = 3) reaches the CLI so output stays byte-identical.
8. Path form for MCP output (project-relative or omitted) and which directive fields carry paths.
9. Canonical facade operation names.
10. Retry affordance: is existing optimistic concurrency (compare-then-write, no lock: orchestrate.rs:1071-1088) sufficient between a long-lived MCP process and concurrent CLI use, or is an operation ID needed?
11. Telemetry toggle in this intent: flag stub, explicit deferral, or omitted; does ProjectContext carry an always-empty telemetry field now?
12. Do host-config docs ship in Phase 2 (plan Phase 2 step 5) or Phase 5 README work? Exact public flag names need owner review (design s8).
13. Does aidlc_status accept an intent selector (status::status takes an optional intent, Observed status.rs:233)?
14. Is "existing tests pass unchanged" measured against the macOS baseline or a controlled environment (TMPDIR under /private/tmp, binary on PATH)? Does CI cover Windows plus one POSIX target today (not verified)?
15. Should DTECH_AIDLC_HOME engine resolution (private to the binary) move into the library for reuse?

## Limits of this framing
Observed claims from plans were checked against named code locations by a parallel reader; code behaviour at the cited lines was not re-run by this writer. The codebase map (.aidlc/knowledge/_codebase-map.md) now exists (Status: draft, mapped at 69cab5a) and includes a real test run; its Gaps section says the plans were not read first-hand, and owner review of the map is a follow-up.

## Revision note
- 2026-11-27 (attempt 2): open questions 1, 3, 5 and 6 marked decided per owner decisions 2026-11-27 (User-provided, project owner, via the conductor AskUserQuestion after the first analysis-baseline submission was blocked); the stale "codebase map does not exist yet" line fixed. Contradiction 6 (prerequisites) is resolved for MCP by decision 5; the CLI behaviour is unchanged.
- 2026-11-27 (review fixes): contradiction 6 corrected to the observed behaviour (report runs no prerequisite checks; force-resume does not gate execution); open question 2 marked answered by the methodology.
