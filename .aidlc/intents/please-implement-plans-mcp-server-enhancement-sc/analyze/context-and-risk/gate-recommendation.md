# Gate recommendation: analysis-baseline (attempt 2)

- Intent: please-implement-plans-mcp-server-enhancement-sc (intent 1 of 2: application facade + MCP stdio server)
- Gate: analysis-baseline (Analyze), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-27
- History: attempt 1 was blocked on 2026-11-27 pending the owner's written decisions on Q1, Q3, Q5 and Q6. The owner answered all four that day (User-provided; recorded in context-and-risk/evidence-log.md E18 and open-questions.md). The Analyze artifacts were revised and re-reviewed (11 review issues; 10 fixed, 1 ruled on by the conductor below).

## Researched options

### A. Approve the analysis baseline as AI Orchestrator and leave only [follow-up] items (recommended)

Replace the stale gate-recommendation.md (attempt 1, which has four [written-approval] items) with one where Q1, Q3, Q5 and Q6 are recorded as owner decisions, User-provided 2026-11-27. Tag everything still open [follow-up]: Q18 (what happens when an MCP approval has no label; the owner delegated this to Ideate and the proposed default is refusal), the IR questions, N2/N3/N5/Q14 confirmations, owner review of the codebase map, and reading the plans first-hand in Ideate. Then report --approval approved --approved-by "AI Orchestrator" and enter Ideate.

Trade-offs: For: the four owner-only policy questions that blocked attempt 1 now have written owner answers, and those answers are carried consistently through every Analyze artifact: AC-003/008/009/010, US-003/US-008, success measures 3 and 6, the stakeholder map, constraints 16-19, R3/R5/R7/R18, quality requirements and E18. No open question that only the owner can decide, and that the direction depends on, is left. The one undecided part of Q6 (no label given) was explicitly handed to Ideate by the owner with a conservative default. Against: the owner has not explicitly reviewed the codebase map, so the conductor's review stands in for that exit criterion (an Assumption). The owner's decisions reach the record only through the conductor; nothing about them appears in audit.jsonl.

### B. Block once more and ask the owner only to confirm the codebase map review

Read analyze.md exit criterion 4 ('the project owner has reviewed it') literally. Treat it as owner-only, list it as [written-approval], report the gate blocked, and ask one AskUserQuestion to confirm the map review before approving.

Trade-offs: For: meets the brownfield criterion word for word and gives the map an attributable human review. Against: another owner round-trip for a review that is not a decision. The contract's [written-approval] trigger is a person-decided item, such as an Analyze open question the direction depends on. The map is not an open question, and no direction choice depends on it. analyze.md already lets later agents correct the map when they see it is wrong. In AI mode the conductor already stands in for 'owner approves the baseline', which is the same kind of criterion. The owner saw map review tagged [follow-up] in the blocked attempt-1 recommendation and did not pin the gate to human (gateMode is still ai). That is context, not consent.

### C. Block for more artifact revision before approval

Send context-and-risk back to read plans/mcp-server-enhancement/* first-hand (evidence-log E5 and the map's Gaps say plan sections were cited second-hand), trace the resolve_approver call sites (N4/E9), and check DeepSeek Harness MCP host behaviour (R18) before the gate.

Trade-offs: For: a more thorough baseline and fewer surprise contradictions in Ideate (R14). Against: each of these is Ideate research the artifacts already track (Q3b, N4, R18, the first-hand plan reading follow-up). None is an exit-criterion gap or an owner decision. Doing it in Analyze delays the gate with little gain, and attempt 1 already rejected this path (its option D).

## Selected direction and rationale

A. Approve the analysis baseline as AI Orchestrator. First replace gate-recommendation.md: none of the remaining items should be tagged [written-approval]; every one is a [follow-up].

I checked each Analyze exit criterion (phases/analyze.md) and the AI orchestrator approval contract (contracts.md lines 54-68):

(1) The problem and desired outcomes are stated. MET (problem-statement.md, success-measures.md; scope is User-provided as intent 1 of 2).

(2) Constraints, assumptions, risks and open questions are recorded. MET:
- constraints 1-19, A1-A9 and Dependencies 1-6;
- R1-R18 with evidence, owner and related criteria;
- Q1-Q18 and N1-N6, each with a class, owner, default and AC/US;
- quality-requirements.md has exactly the template's five sections, and Accessibility is n/a with a reason.

(3) Owner approval of the baseline. In AI mode with gateMode ai, the conductor may approve as long as gate-recommendation.md has no [written-approval] item. The attempt-1 block named four questions only the owner can decide (Q3, Q5, Q6, Q1). All four now have written owner decisions (User-provided, 2026-11-27, through AskUserQuestion), and they are applied consistently:
- AC-003: both hosts must launch the server and list the tools.
- AC-008 and AC-009: no prerequisite execution from MCP; pending prerequisites are returned marked not executed.
- AC-009 and AC-010: the approver label is accepted under the CLI gate policy.
- Constraints 16-19, R3/R5/R7/R18, QR Availability/Operability, and evidence-log E18.

The attempt-1 artifact defects are fixed:
- The AC-009 versus Q5/Q6 contradiction is resolved, and AC-009 now matches the Observed CLI report path, which runs no checks (orchestrate.rs: run_prerequisites only at 1470/1544/1694, all inside orchestrate_next at 1142; I re-checked this).
- The stale map line in problem-statement.md is updated.
- Evidence-log ordering is fixed.
- N6 owner is fixed.
- N2/N3/N5 are moved to follow-ups.

The only part of Q6 the owner did not decide is what happens when an MCP approval has no label (Q18). The owner explicitly made that an Ideate decision with a conservative default (refuse; no git or OS fallback). The artifacts record it that way: open-questions Q18, AC-010 with its scenario marked pending, US-008, and constraint 18. By the owner's own instruction that makes it a [follow-up], not [written-approval]. Q18 error codes and audit provenance are technical choices Ideate can research, so [follow-up]. N2/N3/N5/Q14 each have safe, reversible defaults that do not change direction or scope, so [follow-up] confirmed at the Ideate gate.

(4) For an existing project, the codebase map exists and the owner has reviewed it. The map exists (Observed: .aidlc/knowledge/_codebase-map.md, Status: draft, mapped at 69cab5a, with a real cargo test run of 450 passed and 6 failed). No owner review is recorded. In AI mode I treat the conductor's gate review as standing in for owner review, in the same way it stands in for 'owner approves the baseline' (Assumption).
- Why this holds: the contract's [written-approval] trigger is an item a person must decide. The map is evidence, not a decision. analyze.md lets later agents correct it when they see it is wrong. The owner can still pin analysis-baseline to human with config gate-mode, and has not.
- What I checked: the map's claims I spot-checked match the code (identity.rs resolve_approver with git/OS fallback; orchestrate.rs call sites; one harness/deepseek directory whose productName is "DeepSeek Harness").
- Follow-up: owner review of the map is tagged [follow-up], and the gate recommendation must say openly that it was substituted. Option B covers the stricter reading.

Approval is still conditional on one step. The gate-recommendation.md on disk is the attempt-1 BLOCKED version with four [written-approval] items, so the CLI would refuse approval as it stands. The conductor must replace it with:
- at least two options (A/B/C above);
- an evidence trace that separates Observed, User-provided and Assumption;
- the selected direction and residual risks;
- a boundary statement: approval permits entering Ideate only. It does not pick an SDK, approve the plans as the design, or authorize code, dependency, release or host-config changes;
- a '## Open items' section where every item starts with [follow-up], or '- None'.

## Evidence trace

- Observed (.aidlc/engine/methodology/contracts.md, AI orchestrator approval contract lines 54-68): AI-mode approval needs gate-recommendation.md with at least 2 options, an evidence trace, rationale, a boundary statement and '## Open items' with every item tagged. The CLI refuses approval if any item is [written-approval]. An Analyze open question the decision depends on must be [written-approval].
- Observed (.aidlc/engine/methodology/phases/analyze.md Exit, lines 104-107): four criteria, including 'the project owner approves' and, for an existing project, 'codebase map exists and the project owner has reviewed it'.
- User-provided (project owner, 2026-11-27, conductor AskUserQuestion, relayed in the task): Q3 'MCP binary only', Q5 'Never from MCP', Q6 'Accept approver label' (omitted label left to Ideate), Q1 'Claude Code + DeepSeek'.
- Observed (analyze/context-and-risk/open-questions.md): Q1/Q3/Q5 are Decided, Q6 is 'Decided in part', and Q18 is IR with the conservative refuse default. 'Owner decisions needed before Ideate: None remaining.' The follow-ups list includes map review, N2/N3/N5/Q14 and the IR questions.
- Observed (analyze/problem-framing/acceptance-criteria.md): AC-003 names Claude Code and DeepSeek Harness. AC-008 and AC-009 say prerequisites are never executed and come back pending, not executed. AC-010 applies the CLI gate policy to a self-asserted label. The omitted-label scenario is marked 'pending the Ideate decision'. The revision history records that the AC-009 versus Q5/Q6 contradiction is resolved.
- Observed (constraints-and-assumptions.md constraints 16-19; risk-register.md R3 lowered to Low, R5 reframed, R7 bounded, R18 added; quality-requirements.md Availability and Operability; evidence-log.md E18): the owner decisions are carried with User-provided labels.
- Observed (problem-statement.md 'Limits of this framing', line 67): the stale 'map does not exist yet' line is fixed. Open questions 1, 3, 5 and 6 are marked decided.
- Observed (success-measures.md measures 3 and 6, user-stories.md US-003 and US-008, stakeholder-map.md line 15): consistent with Q1 and Q6. US-008 states that refusing a label-less approval is a proposed Ideate default, not an owner decision.
- Observed (rust-cli/orchestrate.rs at 69cab5a): orchestrate_next at 1142 and run_prerequisites at 1470, 1544 and 1694 only; orchestrate_report at 2795. This confirms the corrected constraint 10 and AC-009.
- Observed (rust-cli/identity.rs:28): resolve_approver(root, flag) falls back to git user.name, then the OS user, when no flag is given. This is the fallback that Q18 proposes to block over MCP.
- Observed (harness/deepseek/manifest.json): productName 'DeepSeek Harness', so the second supported host is a real harness in this repo. Observed (.aidlc/config.json): mode 'ai'; harnesses lists only 'claude'; no gate pinned to human.
- Observed (.aidlc/knowledge/_codebase-map.md): Status draft, mapped at 69cab5a, a real test run recorded. Gaps: plans not read first-hand, locking not verified.
- Observed (analyze/context-and-risk/gate-recommendation.md): it is still the attempt-1 BLOCKED version with four [written-approval] items (Q3, Q5, Q6, Q1). The CLI would refuse approval until it is replaced.
- Assumption: in AI mode the conductor's review stands in for the owner's review of the codebase map. Impact if wrong: the gate needs one owner confirmation (option B). To validate, ask the owner, or the owner pins the gate to human.
- Assumption: the owner decisions were captured accurately by the conductor's AskUserQuestion. No audit.jsonl row records them (Observed: the audit has stage.selected and stage.reported rows only).
- Observed (conductor check, rust-cli/*.rs at 69cab5a): run_prerequisites is called only in orchestrate_next (orchestrate.rs:1470, 1544, 1694), in config prerequisite check (config_commands.rs:364) and in doctor (doctor.rs:1182). orchestrate report runs no prerequisite checks. This confirms the corrected AC-008/AC-009 Sources and constraint 10.
- Conductor ruling (review issue 10): the Q-, R-, E- and N- labels and the numbered constraints are document-local row labels, not contract identifiers. The identifier contract governs AC-/US- (Analyze) and FN-/IF-/TS-/WU-/CFG- (implementation-design). They are allowed.

## Residual risks

- Codebase map owner-review criterion: met only through conductor substitution in AI mode (Assumption). Someone who reads the criterion literally would consider it unmet until the owner reviews.
- R5: the approver label is self-asserted over MCP. The owner accepted this for explicit labels. The label-less fallback is closed only by a proposed Ideate default, not an owner decision.
- R7: Cargo resolves the whole package graph, so having an SDK in the same package could break offline --locked install of the CLI. The Q3 policy may force a separate package or hand-rolled JSON-RPC.
- R18: DeepSeek Harness MCP host behaviour was not observed. AC-003 could fail for one of the two required hosts.
- R1/R12: the facade refactor could change CLI bytes. The goldens only protect against this once the baseline environment is fixed (6 observed failures, 17 in the default environment).
- R6/A1: there is no file lock. A long-lived server running next to the CLI relies on compare-then-write.
- R9/A4: absolute-path redaction done only in the adapter is unverified (/var versus /private/var, Windows forms).
- R4/A3: prerequisite child processes may inherit stdin. This matters for the shared code path even though MCP never runs prerequisites.
- R14: plan sections were cited second-hand, so more document-versus-code contradictions may surface in Ideate.
- Provenance: the owner decisions exist only in the conductor's record and the artifacts. No audit row captures them.

## Boundary statement

This approval covers only the Analyze baseline: the problem framing, success measures, stakeholder map, acceptance criteria AC-001..AC-014, user stories US-001..US-013, constraints and assumptions, evidence log, risk register, open questions, quality requirements, and the codebase map at .aidlc/knowledge/_codebase-map.md. It permits entering Ideate. It does not choose an MCP SDK or runtime, approve the plans in plans/mcp-server-enhancement as the design, decide Q18 (an MCP approval with no approver label), authorize any code change, dependency addition, release, deployment, or change to host configuration, and it does not stand in for the owner's own review of the codebase map.

## Open items

- [follow-up] Q18 (owner delegated it to Ideate): what happens when an MCP approval omits the approver label. Proposed default: refuse before approver resolution, with no git/OS fallback (identity.rs:28). Ideate also decides the refusal error codes and whether the audit can record MCP provenance without an audit schema change. Confirm at the Ideate gate (AC-010, US-008, R5, N4).
- [follow-up] Owner review of .aidlc/knowledge/_codebase-map.md (Status: draft). In AI mode the conductor's review was substituted; the owner can review at the Ideate gate or pin analysis-baseline to human.
- [follow-up] Ideate research: Q3b SDK or hand-rolled JSON-RPC with an ADR, runtime, separate package/binary versus feature; check that offline --locked CLI install still works with the SDK in the package graph (R7). Q4 schemas and boundaries. Q7 outcome kind and exit code 3. Q8 path form. Q9 names. Q10 concurrency. Q15 engine resolution. Q16 resolve_like_node wording. N1 stdin null for prerequisite children. N4 resolve_approver trace. N6 latency bound.
- [follow-up] Q1: Ideate pins the MCP protocol revision against both Claude Code and DeepSeek Harness, and sets up a way to verify DeepSeek Harness (config.json lists only the claude harness on this machine; R18).
- [follow-up] Q3: turn 'MSRV = current stable' into a concrete rust-version value (rustc 1.100.0 observed; de-facto floor at least 1.79).
- [follow-up] Q5: the facade no-exec option must cover all three run_prerequisites call sites in orchestrate_next (1470, 1544, 1694), stay off for the CLI, and come with tests and unchanged goldens.
- [follow-up] Confirm at the Ideate gate: N2 (install scripts stay CLI-only), N3 (assume no external CI), N5 (leave baseline failures), Q14 (definition of 'unchanged': same pass/fail set, TMPDIR under /private/tmp, target/debug on PATH, no re-bless), Q2 (the plans are proposal-only).
- [follow-up] Ideate reads plans/mcp-server-enhancement/* first-hand (evidence-log E5 and the map's Gaps), and reconciles Q17 ordering and decision lists.
