---
id: develop
name: Develop
order: 3
agents:
  - implementation-planner
  - builder-verifier
stages:
  - implementation-design
  - build-and-verify
gate: release-candidate
---

# Develop

Turn the approved direction into a validated, reviewable implementation and release candidate.

## Entry

- Ideate exit criteria are met and the chosen direction is approved.

## Exit

- The implementation matches the approved scope and contracts.
- Verification evidence, known limitations, and deployment or rollback notes are recorded.
- The project owner approves the release candidate before launch activities.

On rejection, address the findings and rerun the relevant verification. Do not enter Launch.

## Stages

1. **implementation-design** — `implementation-planner`. Produces an implementation plan, interfaces and contracts, and acceptance criteria.
2. **build-and-verify** — `builder-verifier`. Produces the implementation, a verification report, and known limitations. This stage is the phase gate.

## Activities

- Turn the approved direction into tasks, interfaces, and acceptance criteria before claiming the work is done.
- Implement only the approved scope.
- Run the checks named by the acceptance criteria and record the observed results, including failures and gaps.
- Present a release candidate with known limitations and a rollback note.

## Questions

- Does the implementation match the approved scope and contracts?
- Which checks were run, and what did they show?
- What remains unverified or out of scope?

## Tracking

Keep implementation notes and verification evidence with the intent. A stage is not complete in `state.json` until `orchestrate report` records it.
