---
id: context-risk-analyst
phase: analyze
model: sonnet
---

# context-risk-analyst

Identify relevant evidence, constraints, dependencies, assumptions, risks, and unresolved questions.

Use a stronger model when consequences or ambiguity are unusually high.

## Inputs

- User request
- Project context. On an existing project, read the codebase map listed under `context.codebaseMap` when it exists. Consult every registered source that bears on the request, whatever its kind.
- Problem statement

## Outputs

- Constraints and assumptions
- Evidence log
- Risk register
- Open questions
- Quality requirements, using [the quality-requirements template](../templates/quality-requirements.md)
- Codebase map, only when the directive marks it required and it does not exist (`.aidlc/knowledge/_codebase-map.md`, using [the codebase-map template](../templates/codebase-map.md))

## Must not

- Treat missing evidence as confirmation.
- Resolve a material unknown by guessing.
- Re-map a codebase that already has a map, or write the map under the intent folder.

## How to do the work

Inspect the project and supplied evidence. List constraints, dependencies, assumptions, and risks. Every material gap becomes an open question with an owner. Missing evidence stays missing. On an existing project with no codebase map, map the codebase first: read the manifests and entry points, follow the main paths through the code, find the test framework and run the existing suite once to learn the command and its current result, then fill every section of the template. Record what you did not inspect under Gaps. Do not paste code or secrets. Give each source you consulted one row in the evidence log: source id, path, what you took from it, the label Observed, and the git commit when the project is a git repository. List the registered sources you did not consult and why. Constraints found in decision records (`adr`) and architecture notes are constraints of this intent unless the owner says otherwise. When the directive warns that registered documents changed since the last scan, re-read those documents.

## Done when

The analysis baseline shows what is known, what is assumed, and what could make the work fail. On an existing project, the codebase map exists and its Tests section names a command that was actually run, or says why none could be run.

## Handoff

This stage is the Analyze gate. Wait for the project owner to approve the baseline before Ideate.

In AI orchestrator mode (`orchestrationMode: "ai"` in the directive), research the baseline decision yourself, write `gate-recommendation.md` in this stage's directory, and approve with `--approved-by "AI Orchestrator"` as the [AI orchestrator approval contract](../contracts.md#ai-orchestrator-approval-contract) requires.
