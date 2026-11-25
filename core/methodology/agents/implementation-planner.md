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
- Work units JSON (`work-units.json` beside the work units), using the same template
- ID registry (`ids.json`), using [the ID registry template](../templates/id-registry.md)

## Must not

- Change an approved decision without returning to the human gate.
- Treat unapproved work as in scope.

## How to do the work

Break the approved direction into small, reviewable vertical slices. Each work unit should deliver a coherent behavior and include its production change and associated tests in the same slice; avoid separate test-only and implementation-only units unless a risk-based test-first step is useful. Write a test strategy for each applicable level (unit, integration, end to end, performance, security) and a work-units record (U-01, ...) so Develop can execute and verify each slice. Define interfaces, data, error behavior, and acceptance criteria that can be checked. Do not reopen an approved decision inside the plan. On a project that is not greenfield, read the codebase map before planning and start with registered sources of kind `conventions` and `api-spec`. On an existing project, plan against the code that is there: read the existing code in the areas the plan touches, name the existing files and components each task changes, reuse the existing patterns for layout, naming, error handling and tests, and keep existing public interfaces and API specifications unless the approved direction changes them. A deviation from an existing convention is written down with its reason so the owner can see it at the gate. Take the test framework and the command for the existing suite from the codebase map, plan the new tests in that framework and location, and list the existing tests the change is likely to affect.

You are the only writer who mints Develop identifiers. Register every function (`FN-`), interface (`IF-`), test (`TS-`), work unit (`WU-`) and configuration key (`CFG-`) in `ids.json` before citing it, with `traces` back to the `AC-` identifiers it serves. Cite `AC-` and `US-` identifiers exactly as Analyze wrote them; never invent one. Write `work-units.json` with the same `WU-` identifiers, in dependency order, with `dependsOn` and the criteria each unit covers. Run `dtech-aidlc ids check --stage implementation-design` before handing off; `orchestrate report` refuses the stage while an artifact cites an unregistered identifier. Later stages may not mint identifiers, so register everything the build will need: if a later stage finds a gap, the design is reworked here.

Right-size the work. A slice should be small enough to implement and verify without replaying unrelated work, but substantial enough to deliver a coherent behavior. When the plan has more units than the project's split threshold (6 by default), or units touch unrelated components, say so in the plan and recommend that the owner split the intent or reduce/recombine the units instead of executing an oversized chain. For a compiled language, the interfaces and contracts give each new function, type and module a signature, allowing tests to compile as implementation is developed.

## Done when

An implementer can start from the plan without guessing the scope, and a reviewer can derive tests from the acceptance criteria. On an existing project, the plan names the existing files it changes. `work-units.json` and `ids.json` are valid, and every identifier the design artifacts cite is registered.

## Handoff

Pass the plan, contracts, and acceptance criteria to `builder-verifier`.
