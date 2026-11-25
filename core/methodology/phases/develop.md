---
id: develop
name: Develop
order: 3
agents:
  - implementation-planner
  - builder-verifier
stages:
  - implementation-design
  - red-tests
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
- The project's existing test suite was run after the change and compared with its baseline, or the reason it could not be run is recorded.

On rejection, address the findings and rerun the relevant verification. Do not enter Launch.

## Stages

1. **implementation-design** — `implementation-planner`. Produces an implementation plan, interfaces and contracts, a functional design, a test strategy, work units with their `work-units.json`, and the ID registry `ids.json`. Write the design with [the IEEE 1016 template](../templates/functional-design.md). Write the strategy with [the test-strategy template](../templates/test-strategy.md), the units with [the work-units template](../templates/work-units.md), and the registry with [the ID registry template](../templates/id-registry.md). Trace every function to an Analyze acceptance-criterion identifier. This stage is the only one that mints `FN-`, `IF-`, `TS-`, `WU-` and `CFG-` identifiers.
2. **red-tests** — `builder-verifier`. Check the environment first and record the existing suite baseline once per intent. For behavior where test-first materially reduces risk (for example a regression, security boundary, public contract, or complex business rule), write and run a focused failing test before implementation and record the observed failure. For other work, do not manufacture a red phase: record the focused checks and test approach planned for the slice; tests may be authored alongside its implementation in `build-and-verify`. Repeat once per work unit.
3. **build-and-verify** — `builder-verifier`. Complete one bounded slice: implement the approved behavior and add or update its automated tests together, unless the test was already authored in `red-tests`. Run focused tests and relevant checks, then re-run tests for dependent earlier slices. Produce the implementation, green verification report, regression report, and known limitations. The green report ends with a trace table. This stage is the phase gate. Repeat once per work unit; the release-candidate gate is presented once, after the last unit.

## Activities

- Turn the approved direction into tasks, interfaces, and a functional design before coding.
- Before the first stage that runs code, make sure the environment is ready: the commands and prerequisites in `PROJECT.md` work and `dtech-aidlc config prerequisite check` passes. A missing environment is reported blocked with `--needs <name>`, not discovered halfway through a work unit.
- Cite identifiers only from `ids.json` and the Analyze artifacts. `orchestrate report` refuses a Develop stage whose artifacts cite an unregistered identifier; a missing identifier sends the work back to `implementation-design`.
- Test-first is risk-based, not a ceremony. Prefer a recorded red run before code for regressions, security-sensitive behavior, public interfaces, complex business rules, and other high-risk changes. For low-risk, straightforward slices, tests may be written alongside implementation, but the slice is not complete until relevant tests exist and pass. Never claim red evidence unless a test was actually run and failed for the intended behavioral reason; compile, import, formatting, and setup errors are not behavioral red evidence.
- A work unit is a small, reviewable vertical slice with explicit acceptance criteria, files/components, focused checks, dependencies, and a done condition. Avoid splitting a simple behavior into separate test-only and implementation-only units. If a plan has more than the split threshold, recommend splitting the intent or reducing/recombining units before execution.
- For each slice, add/update tests with the implementation, run focused checks, record exact observed results, and re-run relevant tests from dependent earlier slices. Run the full existing suite once before the first change and at the phase-level regression check; record differences against the baseline. A regression blocks the release candidate.
- Fixes follow the same risk-based rule: add a failing test first when a behavior change or regression warrants it; otherwise add/update the test with the fix and document why a red-first test was not useful. Do not weaken or remove tests just to obtain a green run.
- If a blocked Develop stage shows that the approved design or work-unit split must change, stop work and ask the owner to authorize `dtech-aidlc intent rework --detail "<reason>"`. This supported route sends Develop back to `implementation-design` and resets downstream Develop stage/unit completion; it does not bypass Analyze or Ideate approval gates. If only implementation or a test is wrong and the design remains sound, correct the affected slice without reworking unrelated completed units.
- On an existing project, follow the conventions and structure recorded in the codebase map, PROJECT.md and the registered conventions sources.
- Implement only the approved scope. Record test coverage gaps, checks not run, limitations, and any reason test-first was not suitable; do not imply that unverified behavior passed.
- Present a release candidate with known limitations and a rollback note.

## Questions

- Does the implementation match the approved scope and contracts?
- Which checks were run, and what did they show?
- What remains unverified or out of scope?

## Tracking

Keep implementation notes and verification evidence with the intent. A stage is not complete in `state.json` until `orchestrate report` records it.
