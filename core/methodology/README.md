# AIDLC methodology

This directory defines a human-governed, phase-based workflow for taking a project from an initial request through delivery and learning. Begin with what the user actually asked for and make uncertainty visible.

Human-readable definitions live in [`phases/`](phases/) (one file per phase) and [`agents/`](agents/) (one file per agent). [`../data/phase-graph.json`](../data/phase-graph.json) is the machine index (schema 1.4.0): phase order, stage routing, gate IDs, outputs, scope membership, model aliases, and file paths. Phase purpose and entry/exit criteria, as well as agent responsibilities, stay in the Markdown files. This document explains how to use that graph and the contracts that govern its work.

## Principles

1. **Human ownership of intent and decisions.** Agents prepare analysis, options, plans, and evidence. They do not approve project gates, silently reinterpret the request, or claim a human decision was made.
2. **Evidence before confidence.** Separate observed facts, user-provided statements, assumptions, estimates, and unanswered questions. Trace important claims to their source where possible.
3. **Bounded, reviewable work.** Each stage has named inputs and outputs. Do only approved work, keep changes within the selected scope, and make limitations explicit.
4. **Approval before consequential transitions.** The project owner reviews and explicitly approves every phase gate. No approval means the workflow is paused at that gate.
5. **Traceability and reversibility.** Preserve decisions and verification evidence. Plan rollback before launch; do not perform destructive or externally visible actions without explicit authorization.
6. **Proportionate rigor.** Scope is coverage (which phases run). Depth is rigor (which stages and outputs of those phases are required). Apply only phases included by the selected scope and only stages included by the selected depth. A smaller scope or a faster depth is not a claim that omitted lifecycle concerns do not exist; note any important follow-up or residual risk.

## Phase and stage contracts

Phases run in this order: **Analyze → Ideate → Develop → Launch → Curate → Archived**. Solo, Team, and Enterprise all end at Archived. A phase can start only when its entry criteria are satisfied. Its agents produce the artifacts described in the graph's stages, then the project owner reviews the exit criteria and records an explicit decision. Rejection returns the work for revision; it does not permit the workflow to progress by default.

### Analyze

Establish the problem and its context, not a presumed solution. The brief analyst frames the request, stakeholders, success measures, acceptance criteria, and user stories. The context/risk analyst records relevant evidence, constraints, dependencies, assumptions, risks, open questions, and quality requirements. Label unknowns instead of filling gaps with invention.

**Approval gate — analysis baseline (`analysis-baseline`).** The project owner confirms that the framing represents their intent and that material uncertainties are visible. A change to the problem framing or success measures after approval requires renewed approval before downstream work relies on it.

### Ideate

Develop a bounded set of feasible, meaningfully distinct options. Describe trade-offs, dependencies, and risks in terms the project owner can assess. Make a recommendation, but do not present it as the selected direction until the owner chooses it. Record both the decision and what is out of scope or deferred.

**Approval gate — solution direction (`solution-direction`).** The project owner selects or rejects the proposed direction and approves the scope boundary. If rejected, return to option exploration. A material change to the chosen direction or scope returns to this gate.

### Develop

Plan and implement only the approved direction. Define interfaces and contracts, and trace them to the acceptance criteria approved in Analyze, before relying on implementation claims. The design stage mints every Develop identifier once, in a machine-readable `ids.json`, and writes the work units as `work-units.json`; later artifacts cite those identifiers and never mint new ones, and the CLI refuses a stage that cites an unregistered identifier. Stages that run code check the environment before writing tests, and a missing environment is reported as a named prerequisite. Verification reports identify checks actually performed, their results, and any coverage gaps; they do not substitute confidence or code inspection for an unrun test. Keep the implementation aligned to approved scope. Surface deviations for approval rather than making unilateral product decisions.

**Approval gate — release candidate (`release-candidate`).** The project owner reviews the implementation, verification evidence, and known limitations, including anything the owner must know about undoing the change. Approval authorizes preparation to launch, not necessarily deployment or any external action. Rejected candidates return for correction and re-verification.

### Launch

Prepare a controlled release with a readiness checklist, communications draft, monitoring/support ownership, and a rollback path. State what action is proposed and its impact. Obtain explicit approval for deployment, publication, notifications, or other externally visible, destructive, or irreversible operations; a general phase approval is not blanket authorization for each such action.

**Approval gate — launch and handoff (`launch-and-handoff`).** Confirm launch evidence, operational acceptance, and any rollback or follow-up required. If readiness is not established, pause or follow the approved rollback plan. Do not state that launch succeeded without observed evidence.

### Curate

Compare intended outcomes with available observations, distinguish correlation from demonstrated cause, retain useful decisions and lessons under the applicable access/retention rules, and assign follow-ups only with clear ownership. Curating does not authorize deleting, archiving, or broadening access to records.

**Approval gate — closure (`closure`).** The project owner approves the outcome record and closure, or keeps the work open with explicit next steps. Closure is a recorded human decision, not an inference from inactivity.

### Archived

Close the intent: record what it finished and what it left out, and write a knowledge record for later intents to `.aidlc/knowledge/<intent-id>.md`. Every scope ends here.

**Approval gate — knowledge write-back (`intent-closure`).** The project owner approves the write-back; until then the intent is not complete.

## Scopes

The graph defines exact phase lists:

<!-- generated:scopes -->
- **solo:** Analyze, Ideate, Develop, Archived.
- **team:** Analyze, Ideate, Develop, Launch, Archived.
- **enterprise:** Analyze, Ideate, Develop, Launch, Curate, Archived.
<!-- /generated:scopes -->

Select the scope with the project owner before execution. These labels determine workflow coverage only; they do not imply a particular team size, risk level, compliance status, or authority to execute actions. Depth (full, standard, or fast) is chosen per intent and recorded with an owner name. If omitted phases leave a material operational, retention, or outcome concern, flag it for explicit owner decision rather than implying it was addressed.

<!-- generated:depths -->
- **full:** every stage of the selected scope (the default).
- **standard:** problem-framing, context-and-risk, option-generation, direction-selection, implementation-design, red-tests, build-and-verify, release-readiness, operational-handoff, outcome-review, knowledge-maintenance, intent-record, knowledge-writeback.
- **fast:** problem-framing, implementation-design, red-tests, build-and-verify, knowledge-writeback, with a reduced output list.
<!-- /generated:depths -->

## Orchestration modes

An intent runs in one of two orchestration modes. The project default is set with `dtech-aidlc config mode <ai|human>` (default `human` when unset) and can be overridden per intent with `orchestrate next --mode <ai|human>`; the mode is fixed once the intent starts and is reported in every directive as `orchestrationMode`.

- **Human mode** is the default: every phase gate pauses for the project owner's explicit approval, as described below.
- **AI orchestrator mode** lets the agent approve gates itself after research: before reporting a gate, it writes a `gate-recommendation.md` artifact (researched options, evidence trace, rationale, boundary statement, and an `## Open items` section that tags every deferral `[written-approval]` or `[follow-up]`) and reports with `--approval approved --approved-by "AI Orchestrator"`. A recommendation that leaves a `[written-approval]` item open cannot be approved: the gate is reported blocked. The full contract is in [contracts.md](contracts.md). All AI approvals are audited with `mode: "ai"` so they stay distinguishable from human decisions.

Choose the mode with the project owner before execution. AI mode trades human review for speed; surface residual risks and follow-ups rather than implying they were reviewed. Settle open questions at the Ideate gate before relying on AI mode, or pin decision-heavy gates to people with `dtech-aidlc config gate-mode <gate> human`; the directive's `gateMode` shows who decides the current phase's gate.

## Agent roster and model recommendations

<!-- generated:agents -->
The graph contains 12 agents: Analyze (`brief-analyst`, `context-risk-analyst`); Ideate (`solution-explorer`, `decision-facilitator`); Develop (`implementation-planner`, `builder-verifier`); Launch (`release-coordinator`, `operations-handoff`); Curate (`outcome-curator`, `knowledge-steward`); Archived (`record-keeper`, `knowledge-archivist`).
<!-- /generated:agents -->

Agent responsibility, inputs, outputs, and guardrails are defined in the linked agent Markdown files; the graph stores each agent's phase, model alias, and file path. Model aliases are recommendations for the named work, not claims that a model is available in every environment or that it may bypass policy, permissions, or human review. Use the listed model only where supported and authorized; escalate difficult, consequential, or ambiguous reasoning to a stronger supported model. A model recommendation never changes an approval gate or grants authority to act.

## Approval and recordkeeping protocol

For each gate, record at minimum: gate, phase, stage, decision, result, by, bySource, mode, at, note, detail, skippedStages, artifacts. The CLI records these fields in the audit row `gate.decided` and in `state.gates.<gate id>` when the gate stage is reported. In human mode `mode` is `"human"` and `by` names the approver; in AI orchestrator mode `mode` is `"ai"` and `by` is `"AI Orchestrator"`. Approval must be affirmative and specific to the artifact and action under review, given through a direct AskUserQuestion call whose options state their consequences with the recommendation first and marked. Silence, a skipped question, a generated plan, a checkmark, or an agent's own assessment is not approval. If context or scope materially changes after approval, obtain renewed approval for affected decisions.

Use the stage outputs as contracts, not a requirement to create empty paperwork. Every output is a separate file at the path the CLI directive lists under `artifacts`. For small work the file can be a few lines, or say not applicable with the reason. Never claim an artifact, test, approval, deployment, or handoff exists until it is actually produced or observed.

## Fresh-project kickoff

1. Capture the user's request without assuming prior project state.
2. Confirm scope and identify the project owner / approval role.
3. Begin Analyze; ask only the questions needed to resolve material unknowns, recording the rest.
4. Produce the Analyze outputs and pause for the analysis-baseline approval.
5. Continue phase by phase, maintaining links between decisions, contracts, implementation, and evidence.

A request that is clear enough to start does not need an exhaustive interview. Make reasonable, low-impact working assumptions explicit and reversible; ask before committing to consequential assumptions or actions.

## Kickoff: existing project (brownfield)

1. Run `dtech-aidlc onboard --harness <claude|deepseek> --scope <scope>` in the project.
2. Review `aidlc.knowledge.json` and `PROJECT.md`, and correct them as needed.
3. Start the first intent; its `context-and-risk` stage writes the codebase map.
4. Review the map at the analysis-baseline gate.
5. Later intents read the map. The Develop stage that changes a component, entry point, dependency, or test command updates it in the same change.
