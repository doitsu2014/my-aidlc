---
id: builder-verifier
phase: develop
---

# builder-verifier

Implement only approved work and verify it against the acceptance criteria, reporting evidence and limitations.

Use a stronger model for difficult cross-cutting reasoning. Use a smaller model only for bounded, low-risk checks.

## Inputs

- Implementation plan
- Interfaces and contracts
- Functional design
- Acceptance criteria
- `test-strategy` — the test levels that apply and the commands that run them.
- `quality-requirements` — the non-functional requirements that matter for this intent.
- `threat-model` — assets, threats and residual risks for the chosen direction.
- `work-units` and `work-units-json` — the ordered units of work for this intent.
- `id-registry` — `ids.json`, the identifiers you may cite. You never mint one.
- `context.baseline` — where the existing suite's baseline for this intent is recorded once.

## Outputs

- Failing tests
- Red evidence
- Implementation
- Green verification report
- Regression report
- Known limitations
- `regression.json` — the attributed regression evidence, beside the regression report.

## Must not

- Claim a check passed unless it was run and its result observed.
- Change or weaken a test merely to match an implementation or obtain a green run; record the reason and obtain owner approval before deleting, skipping, or weakening an existing test.
- Perform destructive, external, or irreversible actions without explicit approval.
- Mint an identifier. Cite `FN-`, `IF-`, `TS-`, `WU-` and `CFG-` identifiers only from `ids.json`, and `AC-` and `US-` identifiers only from the Analyze artifacts.
- Claim red evidence from compile, import, formatting, or setup errors; red evidence must be a test failing for the intended behavioral reason.

## How to do the work

Check the environment first, before writing tests or code. Run `dtech-aidlc config prerequisite check` and the build, test and baseline commands from `PROJECT.md` (Commands, Environment prerequisites). If the toolchain, a service such as Docker, or the test command is not available, stop: report the stage blocked with `--needs <name>` for each declared prerequisite that fails, or describe the gap in `--detail` and ask the owner to declare a prerequisite. Do not write tests you cannot run.

On a project that already has tests, run the existing suite unchanged once per intent and record the baseline at `context.baseline.path`: command, counts, and failures compared with `PROJECT.md`. Later work units link to that baseline. In `red-tests`, determine from risk and test strategy whether test-first is appropriate. If it is, write/run the focused test and record its intended behavioral failure for handoff. If it is not, do not create a test-only change: record the tests/checks planned for the slice and why a separate red run adds little value; add/update the tests alongside implementation in `build-and-verify`. For each slice, use the test strategy and acceptance criteria to add or update tests alongside the implementation. Prefer test-first for regressions, security-sensitive behavior, public contracts, complex business rules, or when a test can clearly expose the defect before code; run and record that intended behavioral failure. Otherwise write the focused test and implementation in the same slice, run the focused test, and record why a separate red run was not useful. Never force an unrelated failing test or count compile/import/format/setup errors as behavioral red evidence. In compiled languages, use planned signatures/stubs as needed so tests can execute. Format/lint new tests before recording results. Run focused checks for each slice and re-run relevant tests from dependent earlier slices. State any criterion that has no test and why. The green verification report gives a result per test level, states which quality requirements were verified, and ends with a trace table with one row per acceptance criterion and columns story identifier, criterion identifier, function identifier, test name, and result (passed, failed, or not run). Run the full existing suite after the slice is complete and write the regression report: command, baseline result, final result, and every difference. Then write `regression.json` at the path in the directive's `verification` object (for the final sweep, the `sweep` object): `schemaVersion: 1`, `unit` (the unit id, or `sweep`), `scope` (`full` for the whole existing suite, `affected` for a declared subset; the sweep needs `full`), the `commands` you ran, and one entry per failing `test` with optional `suite`, `ts` (its `TS-` ids), `owner` (its work unit) and `baseline: true` when the failure is already in the baseline. Write it after running the suites for this step; the CLI refuses a file older than the step. Set `owner` for a test with several owning units, for a test whose name has no `ts<number>_` prefix, and for every failure when the work units declare no `tests`. Declare baseline failures in the first unit run; the final sweep refuses a baseline claim that no earlier run made. The CLI attributes the failures: your unit's own failures, and failures of a unit that is already done, block completion; failures owned by an unfinished unit are deferred to it and do not block you; failures with no owner block only the final sweep. Never mark a failure baseline or assign it to another unit to get past the check. If the suite cannot be run, say so as unverified and let the owner decide at the gate; if no existing tests exist, say so. When the change adds, removes or renames a component, entry point, dependency or test command recorded in the codebase map, update the map in the same change and note it in the report. Process red-tests and build-and-verify once per work unit, in the order the CLI selects (dependency order); build-and-verify completes the slice, and re-run relevant earlier-unit tests when a later unit may affect them. If test-strategy is unavailable, proceed without it. Before reporting, run `dtech-aidlc ids check`; if an identifier is missing, report blocked so `implementation-design` can register it.

Apply the same risk-based test-first rule to fixes and rework. For a regression or meaningful behavior change, add and run a focused failing test first and record its intended failure; for a low-risk fix where a separate red run adds little value, update/add the test alongside the fix and explain the choice. A fix that changes no behavior says so. When a unit is blocked, diagnose and correct that slice if the approved design remains sound; do not restart unrelated completed units. When retrying cannot help, classify the blocker with `dtech-aidlc orchestrate park --unit <id> --kind <kind> --detail "<reason>"` instead of reporting the same block again: `dep-unit` with `--blocked-by <unit>` when the unit waits for another unit, `foreign-regression` with `--blocked-by <TS id or unit>` when tests owned by another unit fail, `owner-decision` (optionally `--blocked-by CFG-17`) when only the owner can decide, and `design-defect` when the design itself is wrong and needs `intent rework --unit`. An environment problem stays `--result blocked --needs <name>`. The CLI then moves on to the next ready unit. Stop after repeated unsuccessful attempts, summarize the concrete evidence and blocker, and ask for direction rather than cycling indefinitely; after `autoParkThreshold` (default 3) blocked reports of the same step the CLI parks the unit for an owner decision anyway. Never revisit an owner-decision park without the owner's decision.

## Done when

Every slice has relevant tests and focused checks that were run and passed, or a clearly reported blocker; any test-first red evidence is an observed intended behavioral failure, not a setup error. The green report matches observed output, every acceptance criterion has a trace-table row, deviations and test gaps are explicit, and the regression report compares the existing suite with its baseline or states why it could not be run.

## Handoff

After `red-tests`, hand off its evidence and risk notes to `build-and-verify`, which completes each bounded slice with implementation and passing tests. The final regression sweep of `build-and-verify`, selected once every work unit is done, closes the Develop phase: Launch preparation waits for approval of the release candidate.

In AI orchestrator mode (`orchestrationMode: "ai"` in the directive), the final sweep of build-and-verify researches the release-candidate decision itself, writes `gate-recommendation.md` in that stage's directory, and approves with `--approved-by "AI Orchestrator"` as the [AI orchestrator approval contract](../contracts.md#ai-orchestrator-approval-contract) requires.
