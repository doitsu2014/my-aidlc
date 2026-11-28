# Gate recommendation: analysis-baseline (intent 2)

- Intent: please-implement-plans-mcp-server-enhancement-sc-2 (intent 2 of 2: telemetry, monitor dashboard, release 1.3.0 prep)
- Gate: analysis-baseline (Analyze), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-28
- Owner decisions folded in before the gate (User-provided, project owner, 2026-11-28): Q1 telemetry MCP-only, default on with opt-out; Q3 zero new crates; Q2/Q4 scoped re-bless (version bump, `monitor` in help/completion/validation, managed .gitignore line) with `monitor` listed in help; Q5 1.3.0 prepare only; Q10 remove 'proposed' after CI green + a live Claude Code tools/call; Q11 installers install both binaries; Q12 flaky tests, missing intent-1 MCP tests and `deps add` lock in scope (CI/clippy cleanup out); Q13 validated ids may be stored; Q15 update mcp-studio-server.html, AI reviews the dashboard design. DeepSeek Harness stays deferred.

## Researched options

### A. Approve the baseline as written

Write gate-recommendation.md now and report context-and-risk complete with --approval approved --approved-by "AI Orchestrator". Everything still open goes in as [follow-up]. This includes the problem-framing text that predates the owner decisions: AC-006, AC-012, the AC-001 note, AC-022, AC-025 and US-013.

Trade-offs: For: quickest, and nothing more changes in .aidlc. Against: the approved baseline would contain an acceptance criterion that contradicts an owner decision. AC-006 says each shared operation produces start and completion events naming the CLI or MCP caller, and its 'Both callers' scenario expects a CLI-run status to record caller cli. AC-012 says concurrent CLI and MCP processes write independent files. Both conflict with Q1, where the CLI writes no telemetry. No context-and-risk artifact says which side wins: CON4 cites AC-006 but does not mark it superseded. A later technical-spec or test writer could take AC-006 literally and build CLI telemetry, which would break the CLI goldens the owner protected.

### B. Revise this stage's own artifacts, then approve (recommended)

Before writing gate-recommendation.md, make small edits to the context-and-risk outputs only, leaving problem-framing alone. (1) Add an 'AC and story reconciliation with owner decisions' table to open-questions.md. AC-006: operation events carry caller mcp only, and the CLI-caller scenario becomes 'a CLI run writes no telemetry' (Q1). AC-012: concurrent MCP processes write independent files and concurrent CLI runs write none (Q1). AC-001 note and AC-025 'blocked pending Q10': superseded by Q1 and Q10. AC-022 'named reviewer': a written AI design-review artifact (Q15). US-013: in scope for the Q12 items, traced to AC-027, AC-028 and AC-029, with the CI runtime upgrade out. (2) Give Q16, Q21, Q24 and Q25 owners and dispositions that match the gate's Open items. Q24 goes to Ideate with a default that needs no owner decision. (3) Fix CON2 to cite tests/mcp_packaging.rs:6, not :8, for LOCK_SHA256. Then write gate-recommendation.md and approve.

Trade-offs: For: the approved baseline is internally consistent, and the owner decisions visibly win over the earlier AC text. It mints no ids, adds no further drift to problem-framing (problem-statement.md already differs from its recorded hash, CON17), and needs no owner input because every reconciliation applies a decision the owner already made. Against: one more edit pass, and Ideate must read open-questions.md to see how the ACs are reconciled, since acceptance-criteria.md still holds the old text.

### C. Rework problem-framing and resubmit

Edit acceptance-criteria.md (AC-001, AC-006, AC-012, AC-022, AC-025) and user-stories.md (US-013) directly, then reconcile and resubmit the gate.

Trade-offs: For: the AC source text itself becomes correct. Against: those files would no longer match the hashes state.json recorded when problem-framing was reported (acceptance-criteria 74146544..., user-stories 2b443372...), on top of the problem-statement drift already disclosed in CON17. There is no observed CLI route to re-report a completed stage short of intent rework, so this costs more and gains little over B.

### D. Block the gate for owner written approval

Report context-and-risk blocked, with the owner-labelled questions (Q16, Q21, Q24, Q25) as [written-approval] items.

Trade-offs: For: the most conservative reading of the 'Owner' labels in the artifacts. Against: the direction depends on none of them. Q21 is an action at release time whose fallback the owner already decided (Q10: the label stays). Q16 and Q25 are documentation refreshes, and the owner accepted the conductor's map review in intent 1. Q24 has defaults that keep every golden and comparison unchanged. Blocking would stall Ideate with no decision actually pending.

## Selected direction and rationale

Option B, executed: the conductor revised this stage's own artifact before approving. constraints-and-assumptions.md now records CON18 (owner decision Q1 supersedes the CLI parts of AC-006 and AC-012: MCP-caller events only, concurrent MCP processes write independent files) and corrects the CON2 line reference to tests/mcp_packaging.rs:6 (LOCK_SHA256). With that, every Analyze exit criterion is met and no owner-only question the direction depends on remains open.

I checked the Analyze exit criteria one by one.

(1) Problem and outcomes are stated. Observed in problem-statement.md and success-measures.md, which cite AC-001..AC-030 and US-001..US-013.

(2) Constraints, assumptions, risks and open questions are recorded. Observed: CON1-CON17, ASM1-ASM9, R1-R26 and Q1-Q25, and quality-requirements.md fills all five template sections with a measure, a target and how each is verified.

(3) Approval of problem framing and baseline. Observed: the directive has orchestrationMode ai and gateMode ai, so the AI Orchestrator may approve once gate-recommendation.md exists.

(4) The codebase map exists and was reviewed. Observed: .aidlc/knowledge/_codebase-map.md exists, and the intent-1 intent-summary.md line 40 records that the owner accepted the conductor's review in place of an owner review. The 11 corrections in evidence-log.md are follow-ups, as the task instructs.

The owner answered Q1-Q5, Q10-Q13 and Q15 (User-provided, 2026-11-28). What remains is technical (Q6, Q7, Q8, Q9, Q14, Q17-Q20, Q22, Q23) or not something the direction depends on (Q16, Q21, Q24, Q25). So no [written-approval] item is needed.

The one real defect in the baseline: AC-006, which expects a CLI-caller event, and AC-012, which describes CLI processes writing telemetry files, contradict Q1 (the CLI writes no telemetry), and no artifact records which side wins. The methodology says not to resolve such a contradiction silently. B settles it by writing down that the owner's decision wins, edits only this stage's outputs, mints no ids, and needs no owner input. A small one-line correction (the LOCK_SHA256 line number in CON2) goes in the same pass.

Q24 is reclassified from Owner to Ideate on purpose, for this reason. The owner's rule (Q2/Q4) limits golden re-blesses. Two Ideate defaults change no golden and no comparison: the sink writes only when .aidlc already exists, which keeps tests/mcp_stdio.rs:548-549 true; and the shared MCP test fixture passes the Q1 opt-out flag, with separate tests for telemetry on. Only the third alternative, letting tests/fixtures/mcp/common.rs:244 skip .aidlc/telemetry, changes what the parity tests compare. That alternative escalates to the owner only if Ideate picks it.

Residual risks are listed below. None of them blocks entering Ideate.

## Evidence trace

- Observed (.aidlc/engine/methodology/contracts.md, AI orchestrator approval contract): gate-recommendation.md needs researched options, an evidence trace, the selected direction, a boundary statement and '## Open items'. Any [written-approval] item blocks AI approval.
- Observed (rust-cli/gate.rs:109-153, commit 327b02a): every list item under '## Open items' counts, including indented sub-bullets, and must start with [written-approval] or [follow-up]. Prose lines without a list marker are ignored. Write each item on one line, with no nested bullets.
- Observed (.aidlc/engine/methodology/phases/analyze.md, Exit): four criteria. The fourth, for an existing project, is that the codebase map exists and the owner reviewed it.
- Observed (.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/archived/intent-record/intent-summary.md:40): 'Codebase-map review: the conductor's review is accepted in place of an owner review' (User-provided in intent 1). The map header still says commit 69cab5a and intent 1 of 2.
- Observed (current sha256 prefix and bytes at 327b02a working tree): problem-statement.md 34fab0280f76eae8, 12629 bytes. state.json still records f220d3bc..., 12262 bytes (disclosed as CON17). acceptance-criteria 74146544a3c764cb, user-stories 2b443372ffa3f47f, success-measures 470f005c880f03e0 and stakeholder-map 760246d25bcabfd8 all match state.json. constraints-and-assumptions 4ae328f406c54c1d, evidence-log 745eb22e36176420, risk-register 94a354826f9145d3, open-questions 438de95a3df75830, quality-requirements 869aadbb51ca6892.
- Observed (grep of rust-cli/ at 327b02a): I found no comparison of a recorded artifact sha256 when an earlier stage is reported, so the CON17 drift appears to be a provenance issue, not a CLI blocker. Only rust-cli/ was searched, with medium confidence.
- Observed (acceptance-criteria.md AC-006 vs open-questions.md Q1): AC-006 'Both callers' expects a CLI status to record caller cli. Q1 (User-provided) says the CLI writes no telemetry. AC-012's statement also has CLI processes writing files. CON4 cites AC-006 without marking it superseded.
- Observed (user-stories.md US-013): still 'out-of-scope unless the owner adds it', with no criteria, and it mentions the CI runtime upgrade. The Q12 decision brings the flakes, the missing MCP tests and the deps-add lock in scope and leaves the CI runtime upgrade out. open-questions.md Q12 notes the change, but no AC traceability is recorded.
- Observed (acceptance-criteria.md): the AC-001 note ('blocked pending Q1'), the AC-025 scenario ('blocked pending Q10') and AC-022 ('named reviewer') are worded before the decisions. Q1, Q10 and Q15 now settle them.
- Observed, spot-checked at 327b02a: tests/mcp_stdio.rs:548-549 asserts .aidlc is absent after calls on an empty project. tests/fixtures/mcp/common.rs:244 defines snapshot(). tests/mcp_parity.rs:96-99 snapshots before an MCP status call. rust-cli/mcp/mod.rs:13-22 rejects unknown args, and parse_args_accepts_only_project is at :86. rust-cli/knowledge.rs:22 has managed block v1. tests/repo_content.rs:1941 requires '## MCP server (proposed)'. tests/mcp_packaging.rs:6 holds LOCK_SHA256, while CON2 says :8 (a wrong citation). rust-cli/dependencies.rs:1264-1270 has no with_state_lock call.
- Observed (grep at 327b02a): CI run id 46093693872 appears only in the intent-2 context-and-risk artifacts. Its source is the owner decision text (User-provided), as E17 already states. Not checked on GitHub.
- Observed in evidence-log E15 and the reviewer note (author's record, not re-run by me): baseline 545 passed, 0 failed, but a reviewer saw artifacts::tests::verifies_empty_missing_and_not_applicable_reason fail in 2 of 4 runs. The baseline is not stable.
- User-provided (owner, 2026-11-28, conductor AskUserQuestion, relayed in the task): Q1, Q2/Q4, Q3, Q5, Q10, Q11, Q12, Q13 and Q15 are decided, and DeepSeek stays deferred.
- Observed (intent-summary.md, intent 1): N2, 'install scripts stay CLI-only', is superseded for intent 2 by Q11 (User-provided). That is allowed because intent.md says to keep intent-1 decisions 'unless the owner changes them'.
- Observed (quality-requirements.md vs AC-021): quality-requirements targets no horizontal scroll at 320 px, while the AC-021 scenario uses 360 px. 320 is stricter, so this is consistent but should be aligned.
- Assumption: the B-revision edits to open-questions.md and constraints-and-assumptions.md fall within this stage's write authority (they are its own outputs). The conductor, not this subagent, makes them.
- Not run by me: cargo test, clippy, CI logs, or any dtech-aidlc command. Per my instructions I wrote no files.

## Residual risks

- If option A is chosen instead of B, AC-006 and AC-012 stay literally in conflict with Q1, and a downstream writer may add CLI telemetry that breaks the CLI goldens the owner protected (AC-027).
- Reclassifying Q24 from Owner to Ideate is a conductor judgment. If the conductor keeps Q24 owner-owned, it becomes a [written-approval] item and the CLI will refuse AI approval (rust-cli/gate.rs:73).
- Default-ON MCP telemetry is still likely to change test snapshots (R1, High/High) until Ideate fixes how the sink and fixtures behave. Telemetry has not been run against any golden yet.
- The test baseline is unstable: one reviewer saw a flake in 2 of 4 runs. Regressions could be hidden, or false failures appear, until the Q12 flake fixes land (R18).
- The hand-rolled HTTP parser and DNS-rebinding defences are new attack surface with no crate support (R9, R10). The Q7 token entropy has no std cryptographic guarantee (ASM2).
- Removing the 'proposed' label depends on a human observation that may not happen (R25). Docs drift in PROJECT.md and the codebase map continues until Q16 and Q25 are done (R26).
- CI evidence (runs 46090623346 and 46093693872) is taken from records and owner text, not checked on GitHub.
- Intent size is an estimate of 8-12 work units (R24). It is one intent, by owner decision.

## Boundary statement

This approval accepts the intent-2 problem framing and analysis baseline as the basis for entering Ideate. That covers problem-statement.md (34fab028..., 12629 bytes), success-measures.md, stakeholder-map.md, acceptance-criteria.md and user-stories.md (all as recorded in state.json), and the five context-and-risk artifacts as revised under option B, read together with the AC and US reconciliation in open-questions.md. It accepts AC-001..AC-030 and US-001..US-013 as the scope to design against, with Q1 superseding the CLI-caller text in AC-006 and AC-012.

It does not choose a solution direction, approve any ADR (including the std-only HTTP, id and token ADR), approve the telemetry flag name, retention values or token mechanism, or approve the event set or error mapping. It does not authorize editing source, tests, goldens, README, CHANGELOG, Cargo.toml or Cargo.lock, scripts/rust-install, plans/, PROJECT.md or the codebase map. It does not authorize any golden re-bless beyond the owner's three scoped reasons (Q2/Q4), and it does not approve Q24's snapshot-skip alternative.

It does not authorize a version tag, publication, release, push, CI run, or removing the README '(proposed)' label (that needs the Q10 evidence). It does not authorize collecting telemetry on anyone's machine, or any access, retention or deletion change.

The owner decisions (Q1-Q5, Q10-Q13, Q15, DeepSeek deferral) remain User-provided. This AI approval neither re-approves nor alters them. Approving the analysis-baseline gate authorizes nothing in later gates: solution-direction, design or release candidate.

## Open items

- [follow-up] Before approval (option B): add an 'AC and story reconciliation with owner decisions' table to context-and-risk/open-questions.md. AC-006: caller mcp only, and a CLI run writes no telemetry (Q1). AC-012: concurrent MCP processes write independent files, CLI writes none (Q1). AC-001 note and AC-025 'blocked pending Q10' are superseded by Q1/Q10. AC-022: the reviewer is a written AI design-review artifact (Q15). US-013: in scope for the Q12 items, traced to AC-027/AC-028/AC-029, CI runtime upgrade out.
- [follow-up] Before approval (option B): set consistent owners in open-questions.md and risk-register R1. Q24 goes to Ideate, escalating to the owner only if the snapshot-skip alternative is chosen. Q21 is an owner action at release. Q16 and Q25 are documentation refreshes. Also correct CON2 to cite tests/mcp_packaging.rs:6 for LOCK_SHA256.
- [follow-up] Ideate: record an ADR for the zero-crate, std-only loopback HTTP server, id generation and token (Q3, CON1). Weigh RandomState plus sha2 against an OS RNG read for the token (ASM2, Q7).
- [follow-up] Ideate: settle Q6 retention numbers and rotation, Q7 token mechanism and threat review, Q8 v1 event set (default: drop park/revisit/pause), Q9 outcome and error mapping (no typed-error work, ADR-008), and Q14 sample minimum, liveness, shutdown and maximum query range. Each has a safe default in open-questions.md.
- [follow-up] Ideate: give values to the AC scenarios marked 'not testable as written': AC-009 file-cap behaviour (Q6), AC-016 maximum range, AC-017 stale threshold, AC-018 minimum sample (Q14), and the AC-028 write-stall bound.
- [follow-up] Ideate: Q24 default keeps every golden and comparison unchanged. The sink writes only when .aidlc exists, and the shared MCP fixture passes the Q1 opt-out flag, with dedicated telemetry-on tests. Choosing to let common.rs:244 skip .aidlc/telemetry requires owner written approval at the solution-direction gate.
- [follow-up] Ideate: answer Q17 (exact missing intent-1 MCP tests), Q18 (DTECH_AIDLC_ASSETS wrapper and .cmd launcher), Q19 (in-process monitor vs supervisor), Q20 (how a trusted off setting reaches the monitor, and the opt-out flag name), and Q22 (hook at the MCP adapter or a Caller::Mcp-only branch, not the shared facade for the CLI).
- [follow-up] Develop: Q23, confirm the flake root causes (ASM4) with repeated local loops and, if available, the logs of CI run 46093693872 before fixing. Until then, treat the 545/0 baseline as unstable.
- [follow-up] Release: Q21, the owner supplies a live Claude Code tools/call observation. Without it and green Windows and macOS CI, the '(proposed)' label and tests/repo_content.rs:1941 stay unchanged and the release notes say so. DeepSeek stays 'not verified'.
- [follow-up] Q25: refresh .aidlc/knowledge/_codebase-map.md with the 11 corrections in evidence-log.md (stale header and commit, 'No MCP code exists', 'no file lock exists', 450/6 baseline, missing application.rs and mcp/* rows, loopback listener). The owner accepted the conductor's map review in intent 1.
- [follow-up] Q16: refresh PROJECT.md (version 1.2.2, 'no CI', no MSRV, test baseline). This workflow cannot edit it, so the owner or a later authorised unit does.
- [follow-up] Before release: an AI threat-model review covering the six AC-030 topics, and an AI dashboard design-review artifact (AC-022, Q15) accepted before dashboard implementation starts.
- [follow-up] Align the narrow-width target between quality-requirements.md (320 px) and the AC-021 scenario (360 px) in the design pack.
- [follow-up] Provenance: problem-statement.md differs from the hash state.json recorded for it (CON17). Cite the current hashes in gate-recommendation.md so the approval attaches to the reviewed versions.
