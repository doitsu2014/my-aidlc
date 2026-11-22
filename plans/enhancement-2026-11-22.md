Implementation plan — pilot feedback (2026-11-22)

Status: implemented in 0.7.0 (all steps in one release). Deviations from the text below:
- Function prefix is FN- (the existing functional-design template already used FN-001), not F-.
- Directive carries artifacts[].template rather than a separate stage.templates map.
- B2: the CLI does not run the baseline command. Repeating Develop directives carry
  context.baseline (one file per intent) and builder-verifier records it once and reuses it.
- B1: PROJECT.md staleness is a directive warning only; it does not block the analysis-baseline gate.
- A6: the Ideate open-questions rule is enforced through the Open items tags in
  gate-recommendation.md, not by parsing open-questions.md.
- A2: per-unit `criteria` in work-units.json is validated when present but not required.

Source: "How to improve" review of the backup-restore pilot (U1). 15 points in three groups.
Each point is mapped to what the repo does today (checked in code) and the change.

Release target: 0.7.0 (graph schema bump to 1.5.0 for the ID registry and prerequisites).

---

Verified gaps in the current code

- G1 `contractsFile` is cited by both SKILL.md files (line 27) and asserted by tests/skill-parity.test.mjs,
  but `buildDirective` (core/tools/workflow.mjs ~545) never returns it. Delegates are told to read a path
  that does not exist.
- G2 A missing `work-units.json` is silent twice: the `work-units` validator (core/tools/context.mjs)
  returns [] when the file is absent, and `applyWorkUnits` (workflow.mjs ~392) falls back to one implicit
  unit. It is not in `artifacts.expected`, so `orchestrate report` cannot catch it.
- G3 No template mints F / IF / TS / WU / CFG IDs. Templates only show US-001, AC-001, U-01, ADR-001.
  Writers invented ID schemes per artifact, so traces drift (U-01 vs WU-01).
- G4 `--result blocked` takes free-text `--detail` only. On resume, `rework` replays the detail; nothing
  checks that the blocker is gone.
- G5 AI-mode approval checks only that gate-recommendation.md is non-empty
  (`gateRecommendationExists`). A recommendation that defers open items still approves.
- G6 SKILL.md (claude) says "Run one workflow for each such phase". The pilot needed one per stage
  directive, because stages arrive one `orchestrate next` at a time.
- G7 The scaffold PROJECT.md has no section for environment prerequisites, test/format commands or known
  baseline failures, and nothing warns when it is still the unedited scaffold.

---

Phase A — CLI correctness (dtech-aidlc column, highs first)

A1. Return `contractsFile` in the directive  [fixes G1, part of "Tighten the skill text"]
  - workflow.mjs buildDirective: add `contractsFile: join(engineRoot(), 'methodology/contracts.md')`.
  - Also return `stage.templates`: absolute paths of the templates the phase file links for this stage's
    outputs (map output -> template in phase-graph.json `outputs`, e.g. "work-units": {template: ...}).
    This is what C1 needs.
  - Tests: directive.test.mjs asserts both paths exist on disk.

A2. List `work-units.json` in artifacts.expected and fail on report  [fixes G2]
  - phase-graph.json `outputs`: add "work-units-json": {kind: "data", path:
    ".aidlc/intents/{intentId}/{phase}/{stage}/work-units.json"}; add it to implementation-design
    `produces` (full + standard; fast depth stays without it, one implicit unit is correct there).
  - context.mjs validator: missing file -> problem 'work-units.json is missing'; validate schema:
    unique ids, `dependsOn` refers to known ids, no cycles, each unit lists >=1 AC id.
  - applyWorkUnits: keep the fallback only when depth is fast; otherwise surface a warning in the directive.
  - Tests: work-units.test.mjs — report complete without the JSON fails; bad dependsOn fails; cycle fails.

A3. Machine-readable ID registry  [high; fixes G3; enables C2]
  - New output `id-registry` (ids.json) produced by implementation-design, path beside work-units.json.
    Shape: { "schemaVersion": 1, "ids": { "F": [{id, title, traces:[AC-..]}], "IF": [...], "TS": [...],
    "WU": [...], "CFG": [...] } }. AC and US IDs are read from Analyze artifacts, not re-minted.
  - Pick one unit prefix: adopt WU-NN everywhere (update work-units template, validator, docs; accept
    legacy U-NN in work-units.json for state migration).
  - New module core/tools/ids.mjs: extract cited IDs from an artifact (regex per prefix), compare with the
    registry + Analyze AC/US IDs.
  - Report-time check for every Develop/Launch stage: an ID cited but not registered -> problem
    "cites unregistered F-007". Registry entries never cited by build-and-verify trace table -> warning.
  - New command `dtech-aidlc ids check [--stage ID]` so the conductor can run it before reporting.
  - Templates: functional-design (F-, IF-), test-strategy (TS-), architecture-design (CFG-) get ID columns.
  - Tests: ids.test.mjs + one brownfield-e2e case.

A4. Named prerequisites for blocked stages  [med; fixes G4]
  - `orchestrate report --result blocked --needs <name> [--needs ...] --detail "..."`.
  - Prerequisites are declared in config (`.aidlc/config.json` "prerequisites": {"docker": {"check":
    "docker info", "hint": "Start Docker Desktop"}}) — check commands come from the owner's config,
    never from artifacts.
  - On `orchestrate next`, for a stage blocked with `needs`, run each check (timeout, no shell
    expansion of untrusted text). Any still failing -> return {blocked: true, needs:[{name, hint, ok}]}
    instead of re-selecting the stage. `--force-resume --detail` overrides with an audit row.
  - `dtech-aidlc doctor` lists configured prerequisites and their current result.
  - Tests: new prerequisites.test.mjs (pass/fail checks, unknown name rejected, audit rows).

A5. Count deferrals at AI gates  [med; fixes G5]
  - contracts.md: gate-recommendation.md must contain a "## Open items" section; each line marked
    `[written-approval]`, `[follow-up]` or `none`.
  - workflow.mjs: in AI mode, refuse `--approval approved` when the section is missing or has any
    `[written-approval]` item; message tells the conductor to report blocked or switch the gate to human.
  - Audit row records the counts of each kind.
  - Tests: orchestration-mode.test.mjs cases for missing section, deferral present, clean.

A6. Per-gate mode override  ["Decide before AI mode"]
  - config `gateModes: {"solution-direction": "human"}` overrides the intent's mode for that gate only.
    The directive returns the effective `gateMode`. Lets decision-heavy gates (Ideate) stay human while
    the rest runs in AI mode.
  - In AI mode at the Ideate gate, an open-questions artifact with unanswered items counts as a
    `[written-approval]` deferral (feeds A5).

---

Phase B — Project setup support ("In this project" column)

These are actions in the pilot repo. dtech-aidlc should make them hard to skip.

B1. PROJECT.md scaffold + staleness warning  [high: "Fill in PROJECT.md"; fixes G7]
  - core/data/scaffold/PROJECT.md: add sections "Environment prerequisites" (Docker, toolchains),
    "Commands" (build, test, lint, format, baseline), "Known baseline failures".
  - context.mjs: warn in the directive when PROJECT.md still matches the scaffold hash or a required
    section is empty. Escalate to a gate-blocking problem at the analysis-baseline gate for brownfield.

B2. Environment preflight before Develop  [high: "Fix the environment before Develop"]
  - Reuse A4 prerequisites: before the first red-tests directive, run configured checks and the
    `baseline` command from PROJECT.md/config; if any fail, return the failures rather than the stage.
  - Record the baseline result (counts + known failures) once per intent so red-tests does not rerun
    and rediscover the 23 tsc errors per unit.

B3. Right-size the depth  [high]
  - SKILL.md depth proposal: recommend `standard` for pilots and first intents in a repo; full only
    with a stated reason.
  - After implementation-design, if work-units.json has more than N units (config, default 6) or units
    span unrelated components, the directive adds a warning suggesting a split into separate intents
    (e.g. backend vs API/UI). Advisory only.

B4. One branch per intent  [med]
  - Directive returns `suggestedBranch: feature/<intentId>`; skill asks once at intent start whether to
    create it. Never creates or switches branches without the owner's answer.

---

Phase C — How the conductor briefs agents (skill + agent files)

C1. Brief from the templates and the CLI  [high]
  - SKILL.md "Model preferences" delegation paragraph: the delegate prompt must include the directive
    JSON verbatim plus the absolute phaseFile, agentFile, contractsFile and stage.templates paths, and
    instruct the delegate to read them. The conductor's own summary is allowed only as an addition.

C2. Writers get a fixed ID registry  [high]
  - For stages after implementation-design, the conductor passes ids.json path and the rule: cite IDs
    from the registry only; a needed new ID is reported back to the conductor, who returns the stage to
    implementation-design (rework) rather than minting.
  - Agent files builder-verifier, release-coordinator: add the same "never mint" line.

C3. Red first for fixers too  [med]
  - builder-verifier.md + SKILL.md: on a rework directive (`stage.rework` present) or any fix, no new
    behaviour without a failing test first; a fix that changes behaviour adds a red test and records it
    in red evidence under the rework attempt number.

C4. Format before the red snapshot  [med; "Run rustfmt" generalised]
  - builder-verifier.md: run the project's formatter (PROJECT.md "Commands: format") on new test files
    before capturing red evidence, so the green diff is not a formatting-only exception later.

C5. Check the environment on turn one  [med]
  - SKILL.md + builder-verifier.md: for any stage that runs code (red-tests, build-and-verify, and
    brownfield context-and-risk), the first action is the preflight (B2 / `dtech-aidlc doctor`). On
    failure report blocked with `--needs`, not after writing tests.

C6. Tighten the skill text  [med]
  - AI mode: "one workflow per stage directive", not per phase (fixes G6). Keep the rule that it must
    not pre-create later stages' artifacts.
  - Remove the contractsFile wording issue (resolved by A1).
  - Compiled languages: red tests must compile against stubs (signatures from interfaces-and-contracts
    that panic/throw "not implemented") so the red run fails at the assertion, not at the compiler.
    A compile error is not valid red evidence. Add to builder-verifier.md and develop.md.
  - Mirror the shared changes into harness/deepseek/skills/aidlc/SKILL.md (Workflow-specific text stays
    Claude-only). skill-parity.test.mjs keeps enforcing it.

---

Order of work

1. A1, A2, C6 — small, fix verified bugs, no schema change. Ship as 0.6.5.
2. A3 + C1 + C2 — ID registry and briefing (schema 1.5.0).
3. A4 + B2 + C5 — prerequisites and environment preflight (one mechanism).
4. A5 + A6 — AI gate deferrals and per-gate mode.
5. B1, B3, B4, C3, C4 — scaffold, advisory warnings, agent text.
Release 0.7.0 after step 5; CHANGELOG and README "How AIDLC flows" updated per step.

Open decisions (defaults chosen above, change if needed)

- Unit ID prefix: WU-NN (matches the review) vs keeping U-NN (matches the current template).
- Prerequisite checks run shell commands from config on `orchestrate next`; alternative is to only
  run them from `doctor` and have `next` read the last result.
- Unit-count threshold for the split warning (default 6).
