---
id: decision-facilitator
phase: ideate
---

# decision-facilitator

Present a reasoned recommendation and make the human decision, scope boundary, and deferred choices explicit. The human remains the decision-maker.

## Inputs

- Solution options
- Trade-off matrix
- Approved analysis from registered sources of kind `adr`, `architecture` and `api-spec`.

## Outputs

- Recommended direction
- Decision record
- Scope boundary
- Architecture design, using [the architecture-design template](../templates/architecture-design.md)
- Threat model, using [the threat-model template](../templates/threat-model.md)

## Must not

- Record a recommendation as approved without explicit user approval.
- Expand scope without consent.
- Contradict an accepted decision record without recording the superseding decision for the owner to approve.

## How to do the work

Recommend one direction and say why the alternatives lost. Write the scope boundary, deferred work, and the decision that still belongs to the project owner. A recommendation is not an approval. When the project already has decision records, continue their numbering and their location and format conventions: the next decision takes the number after the highest existing one, and a decision that replaces an accepted one names it and marks it superseded. Never restart at ADR-001 in a project that has ADRs. Describe the existing architecture by linking to the registered documents and the codebase map rather than restating them. Write each decision once, in the decision record; section 9 of the architecture design lists each decision by its identifier with a relative link to the decision record and does not restate it.

## Done when

The decision record distinguishes the recommendation from the approved choice and names what is out of scope.

## Handoff

This stage is the Ideate gate. Develop starts only after the owner approves the direction.

In AI orchestrator mode (`orchestrationMode: "ai"` in the directive), research the direction decision yourself, write `gate-recommendation.md` in this stage's directory, and approve with `--approved-by "AI Orchestrator"` as the [AI orchestrator approval contract](../contracts.md#ai-orchestrator-approval-contract) requires. This gate is where open questions from Analyze are settled: list every open question the chosen direction depends on under `## Open items` as `[written-approval]`, which blocks AI approval until the owner answers. When the directive's `gateMode` is `human`, present the gate to the owner instead.
