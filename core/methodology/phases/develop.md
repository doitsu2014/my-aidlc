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
2. **red-tests** — `builder-verifier`. Check the environment first and record the existing suite baseline once per intent. For behavior where test-first materially reduces risk (for example a regression, security boundary, public contract, or complex business rule), write and run a focused failing test before implementation and record the observed failure. For other work, do not manufacture a red phase: record the focused checks and test approach planned for the slice; tests may be authored alongside its implementation in `build-and-verify`. Repeat once per work unit, in the order the CLI selects (dependency order; parked units are skipped and revisited).
3. **build-and-verify** — `builder-verifier`. Complete one bounded slice: implement the approved behavior and add or update its automated tests together, unless the test was already authored in `red-tests`. Run focused tests and relevant checks, then re-run tests for dependent earlier slices. Produce the implementation, green verification report, regression report, known limitations, and the machine-readable `regression.json` beside the regression report. The green report ends with a trace table. Repeat once per work unit, in the order the CLI selects. Once every work unit is done, the CLI selects `build-and-verify` one more time without a work unit: the final regression sweep, where the release-candidate gate is presented.

## Activities

- Turn the approved direction into tasks, interfaces, and a functional design before coding.
- Before the first stage that runs code, make sure the environment is ready: the commands and prerequisites in `PROJECT.md` work and `dtech-aidlc config prerequisite check` passes. A missing environment is reported blocked with `--needs <name>`, not discovered halfway through a work unit.
- Cite identifiers only from `ids.json` and the Analyze artifacts. `orchestrate report` refuses a Develop stage whose artifacts cite an unregistered identifier; a missing identifier sends the work back to `implementation-design`.
- Test-first is risk-based, not a ceremony. Prefer a recorded red run before code for regressions, security-sensitive behavior, public interfaces, complex business rules, and other high-risk changes. For low-risk, straightforward slices, tests may be written alongside implementation, but the slice is not complete until relevant tests exist and pass. Never claim red evidence unless a test was actually run and failed for the intended behavioral reason; compile, import, formatting, and setup errors are not behavioral red evidence.
- A work unit is a small, reviewable vertical slice with explicit acceptance criteria, files/components, focused checks, dependencies, and a done condition. Avoid splitting a simple behavior into separate test-only and implementation-only units. If a plan has more than the split threshold, recommend splitting the intent or reducing/recombining units before execution.
- For each slice, add/update tests with the implementation, run focused checks, record exact observed results, and re-run relevant tests from dependent earlier slices. Run the full existing suite once before the first change and record it as the baseline; record differences against the baseline in every regression report. Failures are attributed to the work unit that owns them (see Work-unit scheduling): a failure owned by the unit being built blocks it, a failure owned by an unfinished unit is deferred to that unit, and a failure without an owner blocks the final sweep. A regression outside the baseline blocks the release candidate.
- Fixes follow the same risk-based rule: add a failing test first when a behavior change or regression warrants it; otherwise add/update the test with the fix and document why a red-first test was not useful. Do not weaken or remove tests just to obtain a green run.
- If a blocked work unit shows that the approved design or work-unit split must change, park it as `design-defect` and ask the owner to authorize `dtech-aidlc intent rework --unit <id> --detail "<reason>"` (or `dtech-aidlc intent split <id> --into <id>,<id> --detail "<reason>"` to replace one unit with several). This supported route sends Develop back to `implementation-design` for the named units only; units whose design did not change keep their status and are not replayed. It does not bypass Analyze or Ideate approval gates. If only implementation or a test is wrong and the design remains sound, correct the affected slice without reworking unrelated completed units.
- On an existing project, follow the conventions and structure recorded in the codebase map, PROJECT.md and the registered conventions sources.
- Implement only the approved scope. Record test coverage gaps, checks not run, limitations, and any reason test-first was not suitable; do not imply that unverified behavior passed.
- Present a release candidate with known limitations and a rollback note.

## Work-unit scheduling

When `work-units.json` lists work units, the CLI schedules `red-tests` and `build-and-verify` per unit and keeps a per-unit record in the intent's state (`orchestrate queue` shows it; `dtech-aidlc status` summarizes it).

- **Order.** The next unit is the first one in `work-units.json` order whose `dependsOn` units are done. Dependencies, not array order, decide what can run. Each directive names its unit (`currentUnit`, `unit`) and the queue around it (`queue`). Report the step with `--unit <id>`; only the current unit can be reported.
- **Blocked kinds.** A unit that cannot finish is reported `blocked` or set aside with `dtech-aidlc orchestrate park --unit <id> --kind <kind> [--blocked-by <id>]... --detail "<reason>"`, so the CLI moves on to the next ready unit instead of retrying it:
  - environment prerequisite (Docker, a toolchain): `orchestrate report --result blocked --needs <name>`, as before; not a park.
  - `dep-unit`: the unit waits for another unit (`--blocked-by WU-16`). It comes back on its own once that unit is done.
  - `foreign-regression`: tests owned by another unit fail (`--blocked-by TS-20` or the unit). It comes back once the owning units are done. With attributed regression (below) such failures usually need no park.
  - `owner-decision`: the unit needs a decision only the owner can make (`--blocked-by CFG-17`). It comes back only with `dtech-aidlc orchestrate revisit --unit <id> --detail "<decision>"`.
  - `design-defect`: the unit's design is wrong. It is cleared only by `intent rework --unit` or `intent split`.
- The CLI refuses a park that waits on a unit depending on the parked unit, on units that are already done, or in a cycle. After `autoParkThreshold` (default 3) consecutive blocked reports of the same unit and step, the CLI parks the unit as an `owner-decision` itself. When no unit can run, `orchestrate next` exits with status 3 and `reason: "queue-stalled"`, listing the parked units and what each one needs.
- **Attributed regression.** Completing a unit's `build-and-verify` needs `regression.json` beside its regression report, at the path the directive's `verification` object gives, written after the step was selected: `schemaVersion: 1`, `unit`, `scope` (`full` for the whole existing suite, or `affected`), the `commands` run, and every failing `test` with optional `suite`, `ts` (its `TS-` ids), `owner` (its work unit) and `baseline: true` for a failure already in the baseline. The CLI attributes each failure: the reporter's own failures and failures of an already finished unit refuse the report; failures owned by an unfinished unit are deferred to it; failures with no owner are recorded and block only the final sweep. Declare baseline failures in the first unit run; the sweep refuses a baseline claim no earlier run made.
- **Final sweep.** After every unit is done and none is parked, the CLI selects the final regression sweep: run the whole existing suite once more and write `regression.json` with `"unit": "sweep"` and `scope: "full"` at the path the directive's `sweep` object gives. Only a sweep without failures outside the baseline can pass the release-candidate gate, and it cannot be skipped. A blocked sweep report with failing tests reopens the units that own them; those run again before the sweep is offered again.
- **Scoped rework.** `intent rework --unit <id>` is available when one of the named units is blocked or parked, or after a blocked sweep. Only the named units, units whose design changed, and their started direct dependants are reset.

## Questions

- Does the implementation match the approved scope and contracts?
- Which checks were run, and what did they show?
- What remains unverified or out of scope?

## Tracking

Keep implementation notes and verification evidence with the intent. A stage is not complete in `state.json` until `orchestrate report` records it.
