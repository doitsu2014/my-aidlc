---
id: analyze
name: Analyze
order: 1
agents:
  - brief-analyst
  - context-risk-analyst
stages:
  - problem-framing
  - context-and-risk
gate: analysis-baseline
---

# Analyze

Establish the problem, context, constraints, stakeholders, and evidence before selecting a direction.

## Entry

- A project brief or explicit request exists.

## Exit

- The problem and desired outcomes are stated.
- Constraints, assumptions, risks, and open questions are recorded.
- The project owner approves the problem framing and analysis baseline.
- For an existing project, the codebase map exists and the project owner has reviewed it.

On rejection, revise the analysis artifacts and resubmit. Do not enter Ideate.

## Stages

1. **problem-framing** — `brief-analyst`. Produces a problem statement, success measures, a stakeholder map, and acceptance criteria. Write the criteria in Given/When/Then using [the acceptance-criteria template](../templates/acceptance-criteria.md).
2. **context-and-risk** — `context-risk-analyst`. Produces constraints and assumptions, an evidence log, a risk register, and open questions. On an existing project with no codebase map, it also writes the map (see Existing projects). This stage is the phase gate.

## Activities

- Read the project request, `PROJECT.md`, registered sources in `aidlc.knowledge.json`, and `.aidlc/knowledge/` records listed in the directive `context` before proposing a solution.
- Name the people affected, the current behavior, and the outcome that would count as success.
- Separate evidence, assumptions, constraints, and open questions.
- Record the task in `.aidlc/intents/<intent-id>/` and keep progress in that folder's `state.json`.
- Start from the registered sources in the directive `context`: requirements documents first, then architecture notes, decision records, API specifications and runbooks that bear on the request. Open the documents; do not rely on their titles.
- When a document and the code disagree, or two documents disagree, record both statements with their paths and raise an open question. Do not pick one silently.

## Existing projects (brownfield)

`context.profile.kind` in the directive gives the project kind; anything other than `greenfield` is an existing project. When `context.codebaseMap.required` is true and `exists` is false, `context-risk-analyst` writes the map at `context.codebaseMap.path` (`.aidlc/knowledge/_codebase-map.md`) using [the codebase-map template](../templates/codebase-map.md) during `context-and-risk`, before the evidence log. The map is a project-level file that is not in the stage's `produces` list; write it at that path, not under the intent folder, and list it among the artifacts at the analysis-baseline gate. When the map exists, both Analyze agents read it before anything else and do not re-map the codebase; they correct a section only when they observe that it is wrong and say so in the evidence log. The map is written once per project, not once per intent. When `required` is null, the project kind is not known on this machine: show the user the scan command from `context.warnings` and proceed with what can be observed; never treat 'not known' as greenfield.

## Questions

- What problem is being solved, for whom, and why now?
- Which constraints and risks are known, and which claims still lack evidence?
- What must be true before a solution direction is allowed?

## Tracking

The active project is the directory where `dtech-aidlc` is run. The task text is `intent.md`. Stage status, the current stage, and approval progress are `state.json` in the same intent folder. `.aidlc/state.json` points at that active task.
