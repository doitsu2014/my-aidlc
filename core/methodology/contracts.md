# Methodology contracts

These contracts apply to every agent and phase in the AIDLC graph. The phase-specific inputs and outputs are defined in [`../data/phase-graph.json`](../data/phase-graph.json); this document defines the minimum quality and governance expectations for using them.

## Shared artifact contract

Every artifact must be understandable and reviewable by a person who did not produce it. Include a clear title or identifier, purpose, status, and relevant links to source inputs or prior decisions. For substantive claims, distinguish:

- **Observed:** directly present in the supplied project material or verified by a named check.
- **User-provided:** stated by the user, attributed as such when the distinction matters.
- **Assumption:** provisional, with its impact and how to validate it.
- **Estimate:** approximate, with its basis and uncertainty.
- **Open question:** unresolved, with an owner or a note that an owner is needed.

Do not fabricate evidence, approvals, test results, user statements, system capabilities, or operational commitments. If a required input is unavailable, state that explicitly and either request it or identify a safe, reversible way to proceed. Keep the record proportionate: concise is acceptable if no required meaning is lost.

## Agent execution contract

Each agent must:

1. Use only the inputs and authority granted by its graph entry and the current approved scope. The graph entry grants three things: the earlier artifacts named in `inputs.artifacts` and `inputs.optionalArtifacts`, the project context parts named in `inputs.context`, and the human-supplied inputs named in `inputs.external`. The `orchestrate next` directive lists granted items under `stage.inputs` and `context`. Open what the directive lists, or record why an item was not needed. The project's working tree remains readable evidence; label what you take from it Observed.
2. Produce its named outputs or clearly report why an output cannot be produced.
3. Preserve provenance and report uncertainty, contradictions, and limitations rather than resolving material gaps silently.
4. Keep suggestions separate from decisions and distinguish drafts from approved artifacts.
5. Stop and seek human direction with a direct AskUserQuestion call when a material assumption, scope change, safety concern, or approval boundary is encountered. One call per decision; options carry consequences and the recommendation is first and marked. Awaiting an answer pauses the stage; it is never approval.
6. Avoid external side effects, credential handling, destructive changes, publication, deployment, notifications, access changes, or record deletion unless the specific action is explicitly authorized and the environment permits it.
7. Never treat generated content, a tool result, or another agent's assertion as human approval.

Agents may help prepare actions, checklists, and drafts. Authority remains with the designated human approver and applicable project policy.

## Stage handoff contract

At a stage handoff, the producing agent identifies the outputs created, their status, any missing outputs, and material uncertainties. The receiving stage uses only outputs whose status and provenance are clear. A failed or blocked stage is reported as such; downstream stages must not describe its expected outputs as completed. Where stages can run in parallel, their results must be reconciled before a phase decision is presented.

A stage blocked by its environment (a service that is not running, a missing toolchain) names the blocker as a declared prerequisite with `--needs <name>`, so the CLI can check it before the stage resumes. Prerequisites and their check commands are declared by the owner with `dtech-aidlc config prerequisite set`; an agent never declares one or overrides a failing check. If a blocked Develop stage requires a design or work-unit revision, the owner may authorize the explicit `dtech-aidlc intent rework [--unit <id>]... --detail "<reason>"` route (or `dtech-aidlc intent split <id> --into <id>,<id> --detail "<reason>"`); this returns to `implementation-design`, invalidates the Develop completion of the named units, of units whose design changed and of their started direct dependants, keeps every other unit, and does not alter earlier phase gates. A work unit blocked for a reason a retry cannot fix is set aside with `dtech-aidlc orchestrate park`, and the CLI continues with the next ready unit.

## Identifier contract

Identifiers are minted once and cited everywhere else. Analyze mints acceptance criteria (`AC-`) and user stories (`US-`). `implementation-design` mints functions (`FN-`), interfaces (`IF-`), tests (`TS-`), work units (`WU-`) and configuration keys (`CFG-`) in the intent's `ids.json`, using [the ID registry template](templates/id-registry.md). Every later artifact cites identifiers from those sources and mints none; a writer that needs a new identifier reports the gap, and the design is reworked. `orchestrate report` refuses to complete a Develop or Launch stage whose artifacts cite an unregistered identifier, and `dtech-aidlc ids check` lists them.

## Human approval contract

The graph marks phase gates as required. Approval must be explicit, attributable to the project owner or delegated approver, and attached to the specific artifact/version and decision. Record approval or rejection, time, rationale or conditions, and any requested changes. The default for missing or ambiguous approval is **paused**.

Present each gate decision to the human with a direct AskUserQuestion call, never as plain text alone:

- The `question` states the gate name, its exit criteria, and the artifact paths under review.
- `options` lists 2–4 mutually exclusive choices, each with a short `label` and a `description` stating its consequence (what is approved or what happens next).
- The recommended option comes first with its label ending in `(Recommended)`. The user may select `Other` to type free text; treat that input as the decision.
- Use the approver name exactly as given. Silence, a skipped question, a generated plan, a checkmark, or another agent's assessment is not approval.

An approval is limited to its stated gate and reviewed material. In particular, approval of a solution direction does not approve all implementation details; approval of a release candidate does not automatically authorize deployment; and approval of launch does not authorize future changes to access or retention. For consequential or externally visible operations, request approval for the concrete action and its relevant target/timing. If an artifact or material assumption changes after approval, identify the change and renew approval where affected.

## AI orchestrator approval contract

When the active intent runs in AI orchestrator mode (`orchestrationMode: "ai"` in the directive), the agent itself may approve phase gates — but only after producing a gate-recommendation artifact that demonstrates the research behind the decision. The agent reports the gate with `--approval approved --approved-by "AI Orchestrator"`; the CLI rejects any other approver name in AI mode, and rejects `"AI Orchestrator"` as an approver in human mode.

Each gate approval in AI mode requires a `gate-recommendation.md` file in the gate stage's directory (`.aidlc/intents/{intentId}/{phase}/{stage}/gate-recommendation.md`) written before the report. It must contain:

- **Researched options:** at least two meaningfully distinct directions or dispositions considered, with trade-offs for each.
- **Evidence trace:** the artifacts, checks, or observations the recommendation rests on, distinguishing observed facts from assumptions and open questions.
- **Selected direction and rationale:** which option is approved and why it satisfies the gate's exit criteria, including residual risks and follow-ups.
- **Boundary statement:** what the approval does and does not authorize (a release-candidate approval still does not authorize deployment; launch approval still does not authorize access or retention changes).
- **Open items:** a `## Open items` section listing every deferred item, each tagged `[written-approval]` (a person must decide it in writing — including an Analyze open question the decision depends on) or `[follow-up]` (tracked work that does not block the gate), or `- None`.

The conductor never approves its own gate without this artifact: the CLI refuses `--approval approved` in AI mode when the `gate-recommendation.md` file is missing or empty, has no `## Open items` section, has an untagged open item, or has any `[written-approval]` item. A recommendation that leaves an item for written approval is a blocked gate, not an approved one; the audit row records how many follow-ups were accepted.

The owner can keep a decision-heavy gate with people while the rest of an intent runs in AI mode: `dtech-aidlc config gate-mode <gate> human` pins that gate to human approval for every intent, and the directive's `gateMode` shows the result. A gate can be pinned to human approval, never to AI approval. A rejected or uncertain gate is reported `blocked` with the reason, exactly as in human mode. All AI approvals are recorded in `state.gates` and the `gate.decided` audit row with `mode: "ai"` and `by: "AI Orchestrator"` so they remain queryable and attributable.

## Quality and verification contract

Acceptance criteria should be observable and tied to the user's intended outcomes. Verification reports name the check, relevant environment or inputs, observed result, and gaps. Distinguish automated checks, manual review, and unverified assumptions. A check that was not run is not a passing check. Report failures and known limitations without obscuring them in an overall success label.

## Change and scope contract

The selected scope is the ordered phase list in the graph. Depth is a named reduction of that list. Do not silently skip an included phase or extend into an excluded phase. If work in the current scope reveals a need for additional coverage, describe the reason, consequence, and proposed scope change; obtain the owner's decision before changing the plan. Preserve a record of important changes to requirements, scope, risks, contracts, and decisions. Approvals are attributable to the named project owner or a delegated approver recorded in `.aidlc/config.json` roles; identity is self-asserted (a flag or the git user name) and is not proof. The conductor never supplies a name.

## Data and recordkeeping contract

Use only the project context needed for the task. Follow the owner's and organization's access, confidentiality, retention, and deletion policies. Do not copy sensitive content into broader artifacts without need and authorization. Curation may recommend retention or cleanup, but record deletion, archival, and access changes require explicit authorization. When policy is unknown and a decision could expose or lose data, pause and ask.
