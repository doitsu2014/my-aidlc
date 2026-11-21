# Changelog

Each release lists the behaviour changes a user or a script can notice.

## 0.6.0

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
