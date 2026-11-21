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
- `work-units` — the ordered units of work for this intent.

## Outputs

- Failing tests
- Red evidence
- Implementation
- Green verification report
- Regression report
- Known limitations

## Must not

- Write production code before the new test has been run and seen to fail for the intended reason.
- Claim a check passed unless it was run and its result observed.
- Change the test to match an implementation that was written first.
- Perform destructive, external, or irreversible actions without explicit approval.
- Delete, skip or weaken an existing test to obtain a green run without recording it and getting the owner's approval.

## How to do the work

On a project that already has tests, first run the existing test suite unchanged and record the result as the baseline in the red evidence: the command, the counts, and every test that already fails. Red first. Write red tests at the levels the test strategy names. For each acceptance criterion, add a test that expresses the Given/When/Then behavior, run it, and save the failing output as red evidence. Then write the smallest production change that makes that test pass. Run the tests again and save the green output. A test that passed on the first run does not count as red. State any criterion that still has no test. The green verification report gives a result per test level, with 'not run' stated where true, states which quality requirements were verified, and ends with a trace table that has one row per acceptance criterion and the columns story identifier, criterion identifier, function identifier, test name, and result (passed, failed or not run). After the new tests are green, run the whole existing suite again and write the regression report: the command, the baseline result, the result after the change, and every difference. A test that passed in the baseline and fails now means the stage is blocked: report it as blocked, not complete. If the suite cannot be run (no command known, environment missing), say so in the regression report as unverified and let the owner decide at the gate. If the project has no existing tests, the regression report says so in one line. When the change adds, removes or renames a component, entry point, dependency or test command recorded in the codebase map, update the map in the same change and say so in the green verification report. Run red-tests and build-and-verify once per work unit, in dependency order; for a unit, re-run the tests of earlier units. If test-strategy is unavailable, proceed without it.

## Done when

The release candidate has a failing run followed by a passing run for the same tests, the green report matches the observed output, every acceptance criterion has a row in the trace table, and deviations from scope are explicit. The regression report shows the existing suite's result before and after the change, or states why it could not be run.

## Handoff

After red-tests, hand the failing run to build-and-verify. The build-and-verify stage closes the Develop phase: Launch preparation waits for approval of the release candidate.

In AI orchestrator mode (`orchestrationMode: "ai"` in the directive), the build-and-verify stage researches the release-candidate decision itself, writes `gate-recommendation.md` in that stage's directory, and approves with `--approved-by "AI Orchestrator"` as the [AI orchestrator approval contract](../contracts.md#ai-orchestrator-approval-contract) requires.
