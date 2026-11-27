# Open questions

- Purpose: every unresolved question for intent 1 of 2, classified, with owner, proposed safe default and dependent criteria.
- Status: Draft - pending analysis-baseline gate
- Sources: problem-statement.md (15 questions, 12 contradictions); evidence-log.md; risk-register.md. Provenance: Open question unless marked answered. Defaults are proposals for the owner, not decisions. Owner is the project owner unless stated.
- Classes: OD = owner decision before Ideate (none remain); Decided = owner decision recorded 2026-11-27; follow-up = confirmed at the Ideate gate; IR = Ideate research decision; AE = answered by evidence; LP = later phase.

| # | Question | Class | Owner | Proposed safe default | Depends (AC / US) |
|---|----------|-------|-------|-----------------------|-------------------|
| Q1 | Which MCP hosts and minimum protocol version count as supported? | Decided (User-provided) | Project owner, 2026-11-27 | Decision: Claude Code and DeepSeek Harness; both must pass launch-and-discover. Ideate pins the protocol revision | AC-003 / US-003 |
| Q2 | Can Phase 1 start before SDK spike and design approval? Is the design approved? When must the SDK decision land (arch s9, plan, arch s11 disagree)? | AE | Methodology (confirm at Ideate gate) | Design is proposal-only; SDK decision at the Ideate gate before Phase 2 code; Phase 1 may follow Ideate approval | AC-001, AC-002, AC-003 / US-001..US-003 |
| Q3 | Dependency policy: may an SDK, async runtime and transitive crates be added? Must offline and --locked builds work? Declare an MSRV? | Decided (User-provided) | Project owner, 2026-11-27 | Decision "MCP binary only": at most one reviewed SDK tree, used only by the MCP binary; CLI binary keeps only serde_json and sha2; Cargo.lock committed; --locked and offline cargo install keep working; MSRV = current stable; hand-rolled JSON-RPC still allowed via an ADR (Ideate) | AC-003, AC-013 / US-003, US-009 |
| Q3b | Which SDK (or hand-rolled with ADR), runtime, and separate binary versus cargo feature? | IR | Ideate (architect) | Compare official SDK with minimal stdio JSON-RPC; prefer a synchronous facade | AC-003, AC-013 / US-003 |
| Q4 | Approve extraction boundaries and exact tool schemas and side effects | IR | Ideate; owner approves at gate | Thin application module over existing lib functions; one command per step; deny_unknown_fields; root fixed at launch | AC-002, AC-005 / US-002, US-004 |
| Q5 | May MCP next and report run prerequisite checks? May the guard change shared code? | Decided (User-provided) | Project owner, 2026-11-27 | Decision "Never from MCP": next and report never run owner-configured prerequisite shell checks; they return pending prerequisites marked not executed; the guard is a facade option that is off for the CLI, so CLI behaviour is byte-identical | AC-008, AC-009 / US-006, US-007 |
| Q6 | Expose approved_by in v1? AI-mode gate UX? Human-mode refusal error code? | Decided in part (User-provided) | Project owner, 2026-11-27 | Decision "Accept approver label": MCP report takes an approver field like CLI --approved-by (self-asserted), with the CLI gate policy (AI-mode needs "AI Orchestrator" plus gate-recommendation checks; human-mode refuses "AI Orchestrator"). Chosen over the recommended AI-mode-only option; owner accepts the name is self-asserted. Refusal error codes stay with Ideate (Q18) | AC-009, AC-010 / US-008 |
| Q7 | How does the facade carry outcome kind and exit code 3? | IR | Ideate | Outcome struct with value plus kind; CLI maps to exit code; MCP maps Blocked to a structured result | AC-001, AC-008 / US-001, US-006 |
| Q8 | Path form in MCP output and which directive fields carry paths | IR | Ideate | Project-relative forward-slash paths; never the absolute root; omit what cannot be relativized | AC-006, AC-008 / US-005, US-006 |
| Q9 | Canonical facade and tool names | IR | Ideate | status, queue, next, report; tools aidlc_status, aidlc_queue, aidlc_next, aidlc_report | AC-002, AC-003 / US-002, US-003 |
| Q10 | Is optimistic concurrency enough with a long-lived process, or is an operation ID needed? | IR | Ideate | No operation ID in v1; typed retryable conflict error; document the residual compare-then-write window | AC-011, AC-012 / US-004 |
| Q11 | Telemetry toggle in this intent | LP | Intent 2 | Omit flag and any telemetry field in intent 1 | none (US-010..US-012 out of scope) |
| Q12 | Do host-config docs ship in Phase 2 or Phase 5? Exact flag names? | AE (docs timing); flag names IR | Ideate | Docs ship with Phase 2 (User-provided scope lists manual host-config docs); minimal flags (project root only) | AC-014 / US-009 |
| Q13 | Does aidlc_status accept an intent selector? | AE | none | Observed: status::status takes Option<&str> intent id (status.rs:233); expose optional intent_id | AC-006 / US-005 |
| Q14 | Is "tests unchanged" measured on the macOS baseline or a controlled environment? Does CI cover Windows plus POSIX? | LP (environment definition, Develop); CI half AE | Project owner | CI half answered: no in-repo workflows (E15). Define unchanged as identical pass/fail set before and after with TMPDIR under /private/tmp and target/debug on PATH (PROJECT.md:330); do not re-bless | AC-001, AC-013 / US-001, US-009 |
| Q15 | Move DTECH_AIDLC_HOME resolution into the library? | IR | Ideate | One library function used by the facade; leave existing call sites alone | AC-002 / US-002 |
| Q16 | (Contradiction 8, 9) Root "canonicalized" versus lexical resolve_like_node; handlers consume graph.value() versus status taking &Graph | IR | Ideate | Reuse resolve_like_node plus assert_safe_state_dir; no new canonicalization; correct design wording (code settles the facts) | AC-006, AC-011, AC-012 / US-004, US-005 |
| Q17 | (Contradiction 4, 5) Fixed versus per-command ordering; differing open-decision lists | LP | Develop / Ideate | Preserve existing per-command ordering; reconcile lists in the design | AC-009, AC-011 / US-007 |

## New questions raised by this stage
| # | Question | Class | Owner | Proposed safe default | Depends |
|---|----------|-------|-------|-----------------------|---------|
| N1 | Do prerequisite child processes inherit stdin? (Observed: no stdin setting in prerequisites.rs; effect unverified) | IR | Ideate / Develop | Treat as confirmed risk: set stdin null for children, with a test | AC-004, AC-008 / US-003, US-006 |
| N2 | Do the install scripts (scripts/rust-install) install the MCP binary, or only the CLI, with MCP installed via cargo? | LP (follow-up, confirm at Ideate gate) | Project owner | Scripts keep installing the CLI only; document `cargo install --path . --locked` for both binaries; revisit later | AC-013 / US-009 |
| N3 | Does external CI exist for Windows and one POSIX target? | LP (follow-up, confirm at Ideate gate) | Project owner | Assume none; limit platform claims to what local tests verify | AC-013 / US-009 |
| N4 | Trace the resolve_approver call sites so an MCP approval with no label is refused before resolution and the git/OS fallback in identity.rs:28 is never reached. (Not traced by this stage; a human-mode gate with a named label is accepted as the CLI does, per Q6.) | IR | Ideate | Q18 proposed default: refuse an MCP approval with no label before approver resolution | AC-010 / US-008 |
| N5 | Are the pre-existing baseline failures (2 real, 4 environmental) to be fixed in this intent or left? | LP (follow-up, confirm at Ideate gate) | Project owner | Leave unchanged; record and compare sets | AC-001 / US-001 |
| N6 | What per-call millisecond bound applies to MCP read calls? (quality-requirements.md Performance; docs give no baseline) | IR | Ideate / decision-facilitator | Measure CLI read-command latency in Ideate and set the bound relative to it; no numeric bound until then | AC-002, AC-004 / US-002 |

## New questions raised by the 2026-11-27 owner decisions
| # | Question | Class | Owner | Proposed safe default | Depends |
|---|----------|-------|-------|-----------------------|---------|
| Q18 | What happens when an MCP approval omits the approver label? (CLI falls back to git user.name, then OS user, rust-cli/identity.rs:28.) The owner did not decide this. Also: refusal error codes, and can the audit record that the approval came via MCP? | IR | Ideate / decision-facilitator; owner confirms at the Ideate gate | Conservative: an MCP gate approval without an explicit label is refused; no implicit git or OS identity fallback over MCP. Ideate researches whether audit provenance (via MCP) is feasible without a schema change | AC-010 / US-008 |

## Answered by evidence
- Q13 (status.rs:233 accepts an intent id).
- Q14 CI half: no .github/workflows (E15).
- Q12 docs timing: User-provided scope puts manual host-config docs in intent 1.
- New: README sh-block matcher does not parse `dtech-aidlc-mcp` as a CLI command (repo_content.rs:1436-1460, requires whitespace after `dtech-aidlc`); a `dtech-aidlc mcp` line would fail.
- Q15 correction: DTECH_AIDLC_HOME is already read in several lib modules (evidence-log.md).

## Owner decisions needed before Ideate
None remaining. Q1, Q3, Q5 and Q6 were decided by the owner on 2026-11-27 (User-provided, conductor AskUserQuestion).

## Follow-ups confirmed at the Ideate gate
- Q2 (settled by the methodology: Ideate precedes Develop; plans are proposal-only).
- N2 (default: scripts stay CLI-only), N3 (default: assume no external CI), N5 (default: leave baseline failures, compare pass/fail sets), Q14 environment definition.
- Owner review of .aidlc/knowledge/_codebase-map.md (Status: draft).
- IR: Q3b, Q4, Q7-Q10, Q15, Q16, N1, N4, N6, Q18. Ideate also reads plans/mcp-server-enhancement/* first-hand.

## Revision note
- 2026-11-27 (attempt 2): Q1, Q3, Q5, Q6 recorded as decided per owner decisions 2026-11-27 (User-provided, project owner, via the conductor AskUserQuestion after the first analysis-baseline submission was blocked); Q18 added for the omitted approver label; Q2, N2, N3, N5 reclassified as follow-ups; N6 owner fixed. Q4, Q7-Q10 reference unchanged.
