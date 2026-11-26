# Changelog

Each release lists the behaviour changes a user or a script can notice.

## 1.0.0

- The `dtech-aidlc` CLI is a native Rust executable installed with `cargo install --path . --locked --bin dtech-aidlc`. Every command (onboard/init, orchestrate next/report, status, scope, intent, config, harness, knowledge, ids, repos, deps, doctor, completion) runs natively, and the graph, methodology, harness projections and scaffold are embedded in the binary.
- The Node.js CLI (`core/tools/`), the npm package (`package.json`, `package-lock.json`), the Node test suite (`tests/*.mjs`, `scripts/lint.mjs`) and the Node installers (`scripts/install.sh`, `scripts/install.ps1`) are removed; Node is no longer needed to build, test or run dtech-aidlc. Upgrade and uninstall with `cargo install --force` and `cargo uninstall dtech-aidlc`, and delete an old Node installation by hand (see the README). Parity with earlier releases is guarded by golden snapshots under `tests/golden/`, recorded from the Node CLI with 0.7.2-equivalent behaviour; the methodology lint and repository checks are Rust tests in `tests/repo_content.rs`.
- Without `DTECH_AIDLC_HOME`, the embedded methodology is written to `.aidlc/engine/` (git-ignored, refreshed when the binary changes), and directive paths point there.
- Commands that write state now check that `.aidlc/audit.jsonl` can be written before changing anything.
- Known differences from Node: harness files always come from the embedded copies; some symlinked or dangling state paths are refused where Node writes through them; a prerequisite without a check command reports a readable error. Parity was verified on Windows only.

## 0.7.2

- Develop uses test-alongside vertical slices, with test-first focused on behavior where risk justifies it; rework guidance emphasizes local recovery and stopping repeated retries.

## 0.7.1

- `dtech-aidlc intent rework --detail "<reason>"` returns a blocked Develop stage to `implementation-design`. It resets downstream Develop stage and work-unit completion, preserves earlier lifecycle gates, and records the owner-provided reason in workflow state and the audit log.
- Platform-portable path and filesystem assertions in the test suite.

## 0.7.0

Changes from the backup-restore pilot review.

- The directive now returns `contractsFile` (the skills already told agents to read it, but it was never returned), a `template` path on each artifact that has one, `suggestedBranch` (`feature/<intentId>`), and `gateMode`.
- `implementation-design` produces two data outputs listed in the directive's `artifacts`: `work-units.json` and `ids.json`. Reporting the stage fails when either is missing or invalid: unit ids must be `WU-NN` (legacy `U-NN` is still read), unique, with `dependsOn` naming known units and no cycles. Previously a missing `work-units.json` quietly ran Develop as one implicit unit. Graph schema is 1.5.0, with a `data` output kind and a `template` key on outputs.
- ID registry: `ids.json` registers `FN-`, `IF-`, `TS-`, `WU-` and `CFG-` identifiers. `orchestrate report` refuses a Develop or Launch stage whose Markdown cites an identifier that is not registered, or an `AC-`/`US-` identifier that Analyze did not define. New `dtech-aidlc ids check [--stage ID]` lists them, plus registered functions no green verification report cites.
- Named prerequisites: `dtech-aidlc config prerequisite set NAME --check CMD [--hint TEXT] [--applies-to STAGE|PHASE]...`, `remove`, `check`. `orchestrate report --result blocked --needs NAME` records the blocker; `orchestrate next` re-runs its check and, while it fails, returns `blocked: true` with the `needs` list and exits with status 3 instead of re-selecting the stage. `--applies-to` makes a check a preflight before matching stages. The owner can override with `orchestrate next --force-resume --detail TEXT`, which is audited. `doctor` reports each prerequisite.
- AI-mode gates count deferrals: `gate-recommendation.md` needs an `## Open items` section whose items are tagged `[written-approval]` or `[follow-up]`, or `- None`. Any `[written-approval]` item, an untagged item, or a missing section refuses the AI approval. `gate.decided` records the counts.
- `dtech-aidlc config gate-mode GATE human|default` pins a gate to human approval inside AI-mode intents. A gate can never be pinned to AI approval.
- The `PROJECT.md` scaffold gains Commands, Environment prerequisites, and Known baseline failures sections. The directive warns while `PROJECT.md` is still the unedited scaffold, and in Develop when those sections are empty.
- Repeating Develop directives carry `context.baseline` (one baseline file per intent), and the first `red-tests` directive warns when the plan has more work units than `unitSplitThreshold` (default 6).
- Skills: one dynamic workflow per stage directive in AI mode (was one per phase); delegates are briefed with the directive JSON and the phase, agent, contracts and template files rather than a summary; writers cite registered identifiers and never mint them; environment check on the first turn of any stage that runs code; red first for fixes; depth proposal recommends `standard` for pilots and splitting unrelated work.
- Agents: `builder-verifier` checks the environment first, records the baseline once per intent, writes compiling stubs in compiled languages (a compile error is not red evidence), formats new tests before the red run, and applies red first to rework. `implementation-planner` mints and registers identifiers and recommends splitting oversized intents.

## 0.6.4

- The CLI starts again: 0.6.3 removed `model:` from agent Markdown while graph validation still required it, so every command failed with "agent ... id, phase, or model disagrees with Markdown". `phase-graph.json` is now the only source of an agent's model alias, and agent Markdown that declares `model:` is rejected.
- `red-tests` and `build-and-verify` now run once per work unit for every unit in `work-units.json`, in order. Previously only the first unit ran and the intent was reported complete without the rest.
- The release-candidate gate is presented once, on the last work unit's `build-and-verify`. Earlier units report without `--approval`, and their directives carry `stage.phaseGate: false`; passing `--approval` on them is refused.

## 0.6.3

- Agent Markdown files no longer carry a `model:` field; the directive's `stage.model` is the model preference for delegation. (This release fails graph validation on every command; use 0.6.4.)

## 0.6.2

- Claude stage delegation treats the directive's configured model preference as authoritative over methodology agent-file recommendations and verifies the requested model is passed to the Agent tool.

## 0.6.1

- Claude harness delegates stages using the configured model preference when available, and uses dynamic workflows for each lifecycle phase included in AI mode while preserving stage ordering, artifact ownership, and approval gates.

## 0.6.0

- Human gates and material unknowns are presented with direct AskUserQuestion calls: the question names the gate, criteria, and artifacts; options state consequences with the recommendation first and marked `(Recommended)`; `Other` free text is treated as the decision. Skipping a question is never approval.
- Intents run in an orchestration mode: `human` (default, the owner approves every phase gate) or `ai` (the AI orchestrator researches each gate, records a `gate-recommendation.md`, and approves as `"AI Orchestrator"`). Set the project default with `dtech-aidlc config mode ai|human`; override per intent with `orchestrate next --mode`. Directives carry `mode` and `orchestrationMode`; `intent list` and `status` show the mode; the `gate.decided` audit row and `state.gates` record `mode` alongside the approver. The CLI rejects human approver names in AI mode, rejects `"AI Orchestrator"` in human mode, and refuses AI gate approval without a non-empty `gate-recommendation.md`.

## 0.5.0

- Analyze produces user stories; Develop produces a test strategy and work units; Analyze and Ideate produce quality requirements and a threat model.
- Intents can run at depth `full`, `standard`, or `fast`. Choose with `orchestrate next --depth` or `intent depth`.
- `red-tests` and `build-and-verify` repeat per work unit when `work-units.json` is present.
- Optional `.aidlc/config.json` `roles` restrict gate approvers; `intent assign` records an assignee. Identity is self-asserted.
- `repos add` and `repos remove` edit `aidlc.deps.json` when it exists; `repos pull` prints a note on stderr when runtime-only entries are dropped; a stale `.aidlc/deps.lock` is recovered; `repos` and `config agents` write audit rows.
- `npm run lint` syntax-checks `.mjs` files under `core/tools`, `scripts` and `tests`.
- On an existing project that has been scanned, `context-and-risk` cannot be reported complete until `.aidlc/knowledge/_codebase-map.md` exists.
- `install.ps1` matches `install.sh`: layout `lib\dtech-aidlc`, `-Upgrade`, `-Uninstall`, `DTECH_AIDLC_PREFIX` / `DTECH_AIDLC_SOURCE`, `.cmd` and Git Bash launchers.

## 0.4.0

- `install.sh` adds `--upgrade` (replace in place, repair a partial installation, roll back on failure) and `--uninstall`. A plain re-install fails with one line that names all three installed paths. The launcher resolves symbolic links and fails with one line (`installation is incomplete`) when the library is missing. Files that are not dtech-aidlc files are never replaced or removed.
- A project that uses only the DeepSeek harness no longer gets `stage.model` and `stage.modelSource` in `orchestrate next` or a `model` in `scope NAME`; `config agents` shows source `unsupported`, and `config agents set` is refused (claude harness only). Harness manifests declare `modelAliases`.
- `repos pull` follows the remote's default branch for a dependency registered without `--ref`, instead of whichever branch was fetched first.
- `npm run lint` checks that the methodology README's scope lists, agent roster, phase sections and gate ids agree with the graph. The README now has an Archived section and gate ids, and its approval-record fields match the CLI's audit row.
- The root README describes onboarding, project knowledge, what to commit, harnesses, upgrade and uninstall, and environment variables.

## 0.3.0

- Existing documentation is registered by reference in `aidlc.knowledge.json`; BRD files are no longer copied into `.aidlc/shared/brd`, and the `brd` key is removed from `onboard` output and the project profile. Existing copies are left in place and may be deleted.
- `onboard` adds `--docs`, `--no-detect` and `--rescan`; it creates `aidlc.knowledge.json` only when it registers at least one document.
- `init` is an alias of `onboard`; it scans the project, writes `.aidlc/shared/`, and creates a root `aidlc.knowledge.json` when documentation is detected or supplied with `--docs`.
- The project profile is schema version 2, marked generated, and supports the `unknown` project kind. Folders such as `vendor`, `target`, `out`, `obj` and `venv` are excluded from source evidence, and `.kt` files are reported as Kotlin.
- `orchestrate next` directives add `context.profile.kind`, `context.sourceChanges` and `context.codebaseMap`, with warnings for missing or changed registered sources. `doctor` adds a `Knowledge sources:` check.
- The methodology adds the one-time codebase map at `.aidlc/knowledge/_codebase-map.md` and brownfield guidance for Analyze, Ideate and Develop.
- `build-and-verify` requires a fourth output, `regression-report`; intents that have not completed that stage must write it.
- A new project must choose `--harness` on its first `init` or `onboard`; existing projects can add a harness with `harness add`, and `config scope` shows or changes the default scope for new intents.
- Decisions are written once in the decision record; the architecture design only lists them.
- State migrates by itself; run `dtech-aidlc harness refresh` for the skill files and `dtech-aidlc onboard` for PROJECT.md, the manifest and the profile.

## 0.2.0

- State version 2: `.aidlc/state.json` is a pointer and each intent has its own state file. Intents run one after another, with `intent list`, `pause`, `resume` and `abandon`.
- A phase gate cannot be passed without a recorded approval that names the approver, and a stage cannot be reported complete until every file it produces exists at the path the directive lists.
- Project knowledge: `PROJECT.md`, `aidlc.knowledge.json`, the knowledge house `.aidlc/knowledge/` with one record per closed intent, the `knowledge` commands, and a `context` object in the `orchestrate next` directive.
- Harness manifests are JSON data files and projected skill files carry a `.projection.json` stamp. `harness status`, `harness refresh` and `harness add` are new, and `doctor` checks the projection.
- The methodology is no longer copied into projects; the CLI reads it from the installation (see `DTECH_AIDLC_HOME`).
- `--intent-file` and standard input supply the intent text.

## 0.1.3

- Unknown options, options without a value, and unexpected extra arguments now make a command fail before anything is written; the error lists the allowed options.
- `dtech-aidlc --version` prints only the version line.
