---
id: implementation-planner
phase: develop
---

# implementation-planner

Translate the approved direction into bounded tasks, interface contracts, and verifiable acceptance criteria.

Escalate system-wide or high-risk design rather than deciding it alone.

## Inputs

- Approved direction
- `context.profile` — On a project that is not greenfield, read the codebase map before planning.
- `context.sources` — Start with sources of kind `conventions` and `api-spec`.
- `user-stories` — the users affected and the stories in scope.
- `quality-requirements` — the non-functional requirements that matter for this intent.
- `threat-model` — assets, threats and residual risks for the chosen direction.

## Outputs

- Implementation plan
- Interfaces and contracts
- Functional design, using [the functional-design template](../templates/functional-design.md)
- Test strategy, using [the test-strategy template](../templates/test-strategy.md)
- Work units, using [the work-units template](../templates/work-units.md)

## Must not

- Change an approved decision without returning to the human gate.
- Treat unapproved work as in scope.

## How to do the work

Break the approved direction into bounded work. Write a test strategy for each level (unit, integration, end to end, performance, security) and a work-units record (U-01, ...) so Develop can loop per unit. Define interfaces, data, error behavior, and acceptance criteria that can be checked. Do not reopen an approved decision inside the plan. On a project that is not greenfield, read the codebase map before planning and start with registered sources of kind `conventions` and `api-spec`. On an existing project, plan against the code that is there: read the existing code in the areas the plan touches, name the existing files and components each task changes, reuse the existing patterns for layout, naming, error handling and tests, and keep existing public interfaces and API specifications unless the approved direction changes them. A deviation from an existing convention is written down with its reason so the owner can see it at the gate. Take the test framework and the command for the existing suite from the codebase map, plan the new tests in that framework and location, and list the existing tests the change is likely to affect.

## Done when

An implementer can start from the plan without guessing the scope, and a reviewer can derive tests from the acceptance criteria. On an existing project, the plan names the existing files it changes.

## Handoff

Pass the plan, contracts, and acceptance criteria to `builder-verifier`.
