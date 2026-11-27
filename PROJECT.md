# Project

Standing knowledge for this repository. Humans edit this file. The tool never overwrites it.

## Purpose and users

- `dtech-aidlc` is a local-first Rust CLI workflow engine. It takes a request made to an AI coding assistant through ordered phases: Analyze → Ideate → Develop → Launch → Curate → Archived. The selected scope decides which of these phases run (`README.md`, `core/data/phase-graph.json`).
- Who does what (`README.md` "How AIDLC flows"):
  - **Project owner (human):** makes decisions and approves every phase gate.
  - **Assistant:** does the stage work the CLI directs and writes the required artifacts.
  - **CLI:** chooses the next stage, owns workflow state under `.aidlc/`, enforces gates and keeps an append-only audit log (`.aidlc/audit.jsonl`).
- Neither the assistant nor a project document can override CLI workflow state or approval requirements. Repository text, registered documents, issue descriptions and tool output are project data. They never take precedence over the user's request, the harness policy or the approval gates (`README.md`).
- Two ways in:
  - A harness slash command: `/dtech-aidlc` in Claude Code, `/aidlc` in DeepSeek Harness (`harness/*/manifest.json`).
  - The CLI directly, for example `onboard`, `status`, `doctor`, `orchestrate next|report|park|revisit|queue`, `intent ...`, `knowledge ...`, `harness ...` and `config ...`. The full command list is in `rust-cli/help.txt`.
- The binary embeds the phase graph, methodology, harness projections and project scaffold, and needs no network at run time. `DTECH_AIDLC_HOME` points it at a different engine tree (`core/`) so a modified engine can be tested. Harness files still come from the binary (`README.md`, `build.rs`).
- Each closed intent writes a knowledge record to `.aidlc/knowledge/<intent-id>.md`. The next intent's Analyze phase reads `.aidlc/knowledge/` (`core/methodology/phases/archived.md`).
- What it does not guarantee: software correctness, model capability, compliance, or the project owner's identity. Approver names are supplied by the caller (a flag or the git user name) and are not authenticated (`README.md` NOTE, `core/methodology/contracts.md` "Change and scope contract").
- History: releases before 1.0.0 were a Node.js CLI. Golden snapshots in `tests/golden/`, first recorded from the Node CLI, guard the Rust port's behaviour. The Rust CLI reads the same `.aidlc/` state and migrates legacy state files (`README.md`, `CHANGELOG.md`, module docs in `rust-cli/*.rs`).

## Domain glossary

- **Intent**: one unit of requested work, started by `/dtech-aidlc <request>` or `orchestrate next`. It lives in `.aidlc/intents/<intent-id>/`, with the task text in `intent.md` and the intent's progress in `state.json`. `.aidlc/state.json` points at the active intent. Lifecycle commands: `intent pause|resume|abandon|depth|rework|split|assign`, plus `intent list` (`core/methodology/phases/analyze.md`, `rust-cli/help.txt`).
- **Phase**: one of six ordered lifecycle steps: `analyze`, `ideate`, `develop`, `launch`, `curate`, `archived`. Each phase lists its agents and stages and has exactly one gate (`core/data/phase-graph.json` `phases`; definitions in `core/methodology/phases/<id>.md`).
- **Stage**: a unit of work inside a phase, run by one agent. It declares `inputs` (`artifacts`, `optionalArtifacts`, `context`, `external`) and the outputs it `produces` (`phase-graph.json` `stages`). The phase gate is decided when the phase's final stage is reported.
- **Agent**: a named methodology role. There are 12, two per phase, for example `brief-analyst` (Analyze) and `builder-verifier` (Develop). Each agent recommends a model alias. Agents currently use `opus`, `sonnet` or `haiku`; `fable` is also valid in `modelAliases`. An alias is only a preference: it never switches the model or changes a gate (`phase-graph.json` `agents`/`modelAliases`, `core/methodology/agents/`).
- **Gate**: the required approval at the end of a phase. Gate IDs are `analysis-baseline`, `solution-direction`, `release-candidate`, `launch-and-handoff`, `closure` and `intent-closure`. With no approval the workflow is **paused**. Each decision is recorded in `state.gates.<gate id>` and in a `gate.decided` audit row (`core/methodology/README.md`).
- **Scope**: lifecycle coverage, meaning which phases run. `solo` = analyze, ideate, develop, archived. `team` adds `launch`. `enterprise` adds `launch` and `curate`. Scope says nothing about team size, risk or authority (`phase-graph.json` `scopes`).
- **Depth**: rigor, meaning which stages and outputs are required. `full` is the default and runs every stage. `standard` and `fast` are defined in `phase-graph.json` `depths`. In the current graph `standard` lists all 13 stages, so only `fast` drops stages; it also trims the output list. Choosing `standard` or `fast` explicitly requires `--approved-by`. Changing it later with `intent depth` requires an approver and a rationale (`README.md`).
- **Mode / orchestration mode**: who approves gates, `human` (the default) or `ai`. Set it with `config mode` or `orchestrate next --mode`. It cannot change once the intent starts, and every directive shows it as `orchestrationMode`. In `ai` mode the approver must be `"AI Orchestrator"`, a `gate-recommendation.md` must exist first, and the audit row records `mode: "ai"` (`core/methodology/contracts.md`).
- **Gate pinning / gate mode**: `config gate-mode <gate> human` keeps that gate with a person even inside an AI-mode intent. A gate can be pinned to human approval, never to AI. Directives show the result as `gateMode` (`rust-cli/config_commands.rs`).
- **gate-recommendation.md**: the artifact an AI-mode gate approval requires, written in the gate stage's directory. It must have an `## Open items` section in which every item is tagged `[written-approval]` or `[follow-up]`, or reads `- None`. Any `[written-approval]` item blocks the gate (`core/methodology/contracts.md`).
- **Directive**: the JSON that `orchestrate next` returns for the selected stage. Fields include `stage.inputs`, `context`, `artifacts`, `orchestrationMode`, `gateMode` and, for work units, `currentUnit`, `unit`, `queue`, `parallel`, `verification` and `sweep`. Once the intent is finished, `orchestrate next` returns `complete: true` (`README.md`, `core/methodology/phases/develop.md`, `rust-cli/orchestrate.rs`).
- **Context envelope**: the read-only `context` block of a directive, built by `rust-cli/orchestration_context.rs`. It always holds `schemaVersion`, `projectRoot`, `granted` and `warnings`. The stage's `inputs.context` grants decide what else is added:
  - the `request` grant adds `request`;
  - the `project` and `sources` grants add `project`, `sources` and `sourceChanges`;
  - the `profile` grant adds `codebaseMap`;
  - the `knowledge` grant adds `knowledge`.
  - `repos` is a grant name with no matching key in the envelope.
  - Stages with `repeat: work-unit` also get `baseline`.
- **Report**: `orchestrate report --result complete|blocked|skipped`, with `--approval approved --approved-by <name>` at a gate. A stage is not complete in `state.json` until it is reported (`core/methodology/phases/develop.md`). The report is refused when:
  - a Develop or Launch artifact cites an unregistered ID;
  - a `build-and-verify` completion or the sweep lacks a valid, fresh `regression.json`;
  - an AI-mode gate fails the `gate-recommendation.md` checks.
- **Artifact / output**: a named stage output, written as its own file at the path the directive lists under `artifacts`. The default path is `.aidlc/intents/{intentId}/{phase}/{stage}/{output}.md`, with a `{unitId}/` subfolder for per-unit stages (`rust-cli/artifacts.rs`). Output kinds other than the default document are `data` (`work-units.json`, `ids.json`) and `code-manifest` (`failing-tests`, `implementation`) (`phase-graph.json` `outputs`).
- **Work unit (`WU-`)**: a bounded vertical slice that `implementation-design` defines in `work-units.json`. `red-tests` and `build-and-verify` repeat once per unit (`"repeat": "work-unit"`). The next unit is the first in file order whose `dependsOn` units are done. Up to `unitWorkers` units (default 3) can be in flight using `orchestrate next --claim`.
- **Park / parking kinds**: `orchestrate park --unit <id> --kind <kind>` sets aside a unit that a retry will not fix. The four kinds:
  - `dep-unit`: waits on another unit. It comes back by itself once that unit is done.
  - `foreign-regression`: another unit's tests fail. It comes back once the owning units are done.
  - `owner-decision`: needs the owner to decide. It comes back only with `orchestrate revisit`.
  - `design-defect`: the design is wrong. Only `intent rework --unit` or `intent split` clears it.
  - After `autoParkThreshold` (default 3) consecutive blocked reports of the same unit and step, the CLI parks the unit as `owner-decision` itself.
  - An environment problem is not a park. Report it with `--result blocked --needs <name>` (`core/methodology/phases/develop.md`).
- **Prerequisite**: a named environment requirement with a check command. Only the owner declares one, via `config prerequisite set ... [--applies-to STAGE]`. `orchestrate next` runs the check before a matching stage. Agents never declare a prerequisite or override a failing check (`core/methodology/contracts.md`).
- **Attributed regression / `regression.json`**: the machine-readable test result written beside the regression report. Each failure is attributed to a unit:
  - a failure owned by the reporting unit, or by a unit that is already done, refuses the report;
  - a failure owned by an unfinished unit is deferred to that unit;
  - a failure with no owner blocks only the final sweep.
- **Final sweep**: one more `build-and-verify` run without a unit, once every unit is done and none is parked. It writes `"unit": "sweep"` and `scope: "full"`. The `release-candidate` gate is presented here and passes only with no failures outside the baseline. The sweep cannot be skipped.
- **Queue stall**: no unit can run. `orchestrate next` exits with status 3 and `reason: "queue-stalled"`, listing the parked units (`rust-cli/work_units.rs`, `BLOCKED_EXIT_CODE` in `rust-cli/orchestrate.rs`).
- **ID registry (`ids.json`)**: identifiers are minted once and cited everywhere else. Analyze mints `AC-` and `US-` in its artifacts. `implementation-design` mints `FN-`, `IF-`, `TS-`, `WU-` and `CFG-` in `ids.json`. `orchestrate report` refuses a Develop or Launch stage that cites an unregistered ID, and `ids check` lists such IDs (`core/methodology/contracts.md`).
- **Rework**: `intent rework [--unit <id>]...` or `intent split <id> --into <id>,<id>` returns Develop to `implementation-design`. It resets:
  - the named units;
  - units whose design changed;
  - their started direct dependants.
  - Other units and earlier phase gates are unchanged (`core/methodology/contracts.md`).
- **Engine**: the graph and methodology (`core/`) that the CLI runs on. They are embedded in the binary and written to `.aidlc/engine/`, or read from `DTECH_AIDLC_HOME`. Directive paths point into the engine.
- **Harness**: a supported assistant integration (`claude` or `deepseek`), defined by `harness/<name>/manifest.json`. Key fields: `skillName`, `invoke`, `skillDir`, `files`, `modelAliases`.
- **Projection**: the harness skill files copied into a project's `skillDir` (`.claude/skills/dtech-aidlc` or `.dsh/skills/aidlc`). A `.projection.json` stamp records the engine version. Managed with `harness status|refresh|add`. `refresh` keeps local edits unless `--force` is given; `--force` makes backups first (`rust-cli/onboarding/projection.rs`, `rust-cli/harness_commands.rs`).
- **Knowledge source**: an entry in `aidlc.knowledge.json` with `id`, `path`, `kind` and an optional `description`. The file is read in place, never copied. Kinds: `project`, `docs`, `requirements`, `architecture`, `adr`, `api-spec`, `runbook`, `conventions`, `readme` (`rust-cli/knowledge.rs` `SOURCE_KINDS`).
- **Knowledge record**: the per-intent write-back at `.aidlc/knowledge/<intent-id>.md`, produced by `knowledge-writeback`. `knowledge index` writes `.aidlc/knowledge/_index.md`. Later Analyze stages read `.aidlc/knowledge/`.
- **Codebase map**: `.aidlc/knowledge/_codebase-map.md`. The first `context-and-risk` stage on an existing project creates it, and later intents read it. The Develop stage that changes a component, entry point, dependency or test command updates it.
- **PROJECT.md**: standing project context owned by humans. Onboarding creates it from a template only if it is missing and never overwrites it.
- **Audit log**: `.aidlc/audit.jsonl`, append-only. Do not edit it or `.aidlc/state.json` by hand (`README.md`).

## Architecture overview

### Crate and binary
- Single Cargo package `dtech-aidlc` (`Cargo.toml`, edition 2021, `publish = false`). Library at `rust-cli/lib.rs`, binary `dtech-aidlc` at `rust-cli/main.rs`. Runtime deps: `serde_json` (`preserve_order`) and `sha2` only.
- The Rust CLI is a port of a former Node CLI. Module docs still cite `core/tools/*.mjs` (`dtech-aidlc.mjs`, `workflow.mjs`, …) and `scripts/lint.mjs`. Those files are no longer in the repo (`scripts/` holds only `rust-install/install.sh|ps1`). "Node" in comments means the reference behaviour the Rust code reproduces: error texts, JSON key order, audit rows.
- `rust-cli/main.rs` routes commands. In order: `--version`/`--help`, a per-command flag allow-list and max positional count (`args::command_flags`, `command_max_args`), `completion zsh`, `--project` resolution (`orchestrate::resolve_like_node`), then `audit::assert_safe_state_dir`. After that it dispatches. Handlers return `(serde_json::Value, exit code)`. A string value is printed raw, anything else as pretty JSON. Errors print `dtech-aidlc: <msg>` and exit 1. A `next` blocked by a failing prerequisite exits `orchestrate::BLOCKED_EXIT_CODE` (3).
- `rust-cli/help.txt` and `rust-cli/completion.zsh` are compiled in with `include_str!`.

### Module map (`rust-cli/`)
- `args`: argv parser, command keys (`"orchestrate next"`, …), per-command flag specs.
- `orchestrate`: `orchestrate next` / `orchestrate report` engine (largest module), the `Engine` type (embedded vs external methodology root), `materialize_embedded_engine`, `finish_directive`, `finish_report`, projection notices.
- `workflow/`: state primitives (pointer read/write, intent load/save, status derivation, intent ids, atomic JSON writes). Also the older `Workflow` type; its doc says "new code should use the free functions". Its only callers are `tests/rust_orchestration_assets.rs` and `orchestrate::next`, which nothing calls. `unicode_tables.rs` is generated Unicode data for intent slugs (do not edit by hand).
- `work_units`: unit-level scheduling for Develop's repeating stages (`red-tests`, `build-and-verify`). It keeps the `workUnits` ledger inside intent `state.json`.
- `unit_commands`: `orchestrate park|queue|revisit`.
- `regression`: parses and attributes the agent-written `regression.json` for `build-and-verify`. The CLI never runs tests.
- `artifacts`: expected-artifact paths and verification (size and sha256). Used by `orchestrate`, `work_units`, `ids`, `regression` and `workflow`. Its header comment saying "not yet wired into the CLI" is out of date.
- `orchestration_context`: read-only directive `context` envelope (sources, source changes, codebase map, baseline hint, knowledge).
- `state_migration`: explicit v1 to v2 `.aidlc/state.json` migration, with a `state.v1.bak.json` backup.
- `scope`: loads and validates the methodology graph (`read_graph`), validates model config, holds the embedded asset table (`embedded_asset`, `embedded_methodology_assets`), and serves the `scope` command.
- `status`: `status` and `intent list` (read-only).
- `intent_commands`: `intent depth|rework|split|assign|pause|resume|abandon`.
- `config_commands` / `config_read`: `config …` writes and read-only queries.
- `harness_commands`: `harness add|status|refresh`, harness manifests, projection stamp/status/refresh, cleanup of the legacy `.aidlc/methodology`.
- `onboarding/`: `onboard`/`init` (`--rescan`). Writes config, projects the harness, scaffolds `PROJECT.md`, registers docs in `aidlc.knowledge.json`, writes the project profile and sources snapshot. Exposes `safe_join`. `onboarding/projection.rs` is a separate data-driven projection helper; a comment in `doctor.rs` notes its `projection_status` lacks projection history and legacy skill directories.
- `knowledge` / `knowledge_commands`: knowledge manifest (`aidlc.knowledge.json`), records, `_index.md`, the managed `.aidlc/.gitignore` block, and the `knowledge …` commands.
- `dependencies`: `repos …` / `deps …` (`.aidlc/deps.json`, `deps-lock.json`, `deps.lock` process lock, `.aidlc/repos/`).
- `ids`: ID registry and citation checks (registry prefixes `FN IF TS WU CFG`, analyze prefixes `AC US`), plus `ids …` commands.
- `doctor`: read-only health report, exits 1 if any check fails.
- `prerequisites`: named prerequisite config and execution of owner-authored check commands through the platform shell.
- `identity`: approver resolution, in order: `--approved-by`, then `git config --get user.name`, then OS user, then `unknown`.
- `audit`: appends one JSON line per event to `.aidlc/audit.jsonl`; `assert_safe_state_dir`.
- Not routed from `main.rs`: `orchestration_assets` (`materialize_methodology_assets`, called only from `tests/rust_orchestration_assets.rs`), `orchestration_report_validation` (its doc says "not routed from the CLI yet"), `orchestration_policy`, and `gate` (used only by those last two, plus its inline tests).

### `orchestrate next` / `orchestrate report` data flow
- `main.rs::run_orchestrate` resolves the root and checks the state dir. It picks the engine: `external_engine()` from `DTECH_AIDLC_HOME`, otherwise `Engine::embedded(root)` = `<project>/.aidlc/engine`. It then loads the graph with `scope::read_graph`.
- **next**: `knowledge::knowledge_preflight` (when the root exists) → intent text from at most one of `--intent` | `--description` | `--intent-file` (a path or `-` for stdin, 100000-byte cap) → flag validation (`--mode human|ai`, `--force-resume` requires `--detail`, `--claim`/`--unit` are mutually exclusive and can't start an intent) → `scope::validate_model_config` → `orchestrate::orchestrate_next`.
  - `orchestrate_next` loads the workflow (migrating v1 state), selects a stage or work unit (ledger, parallel claims, prerequisite holds, design-rework requests) and builds the directive with its `context`.
  - It writes only after every check passes, and only if the pointer (and the active intent's `state.json`) still holds the bytes read at the start. Write order for a new intent: intent folder, `intent.md`, `state.json`, pointer, audit rows. Updating an active intent rewrites only its `state.json`.
  - Returns `NextOutcome::{Directive, Blocked, Complete, Busy}`. For a `Directive` with a stage, `finish_directive` ensures `.aidlc/knowledge/` and `.aidlc/.gitignore` exist and appends harness projection notices to `warnings`.
- **report**: reads config leniently (`scope::load_config_unvalidated`) → `orchestrate_report_with_engine` with `--stage --result --approval --detail --approved-by --approval-note --needs --unit` → validates artifacts, gates and regression, persists state and audit → `finish_report`. When `knowledge-writeback` reports `complete`, `finish_report` rewrites the knowledge index and sets `knowledgeRecord`.
- The methodology graph is `core/data/phase-graph.json`. Phases in order: analyze, ideate, develop, launch, curate, archived, each with a gate. Scopes `solo|team|enterprise`; depths `standard|fast`.

### Workflow state on disk (`<project>/.aidlc/`)
- `config.json`: `harness`/`harnesses`, `scope`, `mode`, `gateModes`, `agents`, `prerequisites`.
- `state.json`: pointer `{"version": 2, "activeIntentId", "updatedAt"}` (`workflow::save_pointer`). `intent resume` quarantines a corrupt pointer as `state.json.corrupt-<timestamp>`. The v1 backup is `state.v1.bak.json`.
- `intents/<id>/state.json` (per-intent state, version 2, optional `workUnits` ledger) and `intents/<id>/intent.md`.
- Stage artifacts default to `intents/<id>/<phase>/<stage>/<output>.md`, or `…/<stage>/<unitId>/<output>.md` for work units (`artifacts.rs`). `regression.json` sits under `develop/build-and-verify/[<WU>/]`.
- `audit.jsonl` (append-only), `knowledge/` (`<id>.md`, `_index.md`, `_codebase-map.md`), `shared/` (`project-profile.json`, `sources-snapshot.json`), `deps.json`, `deps-lock.json`, `repos/`. `shared/brd/` is a legacy copy from earlier versions. Onboarding only reports that it exists.
- `engine/`: CLI-owned copy of the embedded methodology. It has its own `.gitignore` of `*` and an `.engine-version` holding the CLI version plus a sha256 digest of the methodology.
- `.aidlc/.gitignore`: managed block that ignores `/state.json`, `/state.v1.bak.json`, `/audit.jsonl`, `/config.json`, `/shared/`, `/repos/`, `*.tmp` (`knowledge.rs` `IGNORE_BLOCK`).
- At the project root: `PROJECT.md` (created from the embedded `core/data/scaffold/PROJECT.md` only if missing, never overwritten) and `aidlc.knowledge.json`.
- Writes refuse symlinked components and go through temp file + rename.

### Embedded assets and `DTECH_AIDLC_HOME`
- `build.rs` walks `core/data/phase-graph.json`, `core/methodology/**`, `core/data/scaffold/**` and `harness/**`. It generates `$OUT_DIR/embedded_assets.rs` (`ASSETS: &[(&str, &[u8])]` via `include_bytes!`). Harness files are keyed `harness/<name>/…`.
- `rust-cli/scope.rs` includes that table. `core/data/projection-history.json` is compiled in separately with `include_str!` (`orchestrate.rs`, `doctor.rs`, `harness_commands.rs`). Without `DTECH_AIDLC_HOME`, the binary does not read `core/` or `harness/` at runtime.
- `DTECH_AIDLC_HOME`: `<home>/core` or `<home>` is used if it contains `data/phase-graph.json` or `methodology/`. The graph is then read from disk, and directive paths (`contractsFile`, `phaseFile`, templates) point there. Otherwise the embedded graph is used and methodology is materialized to `<project>/.aidlc/engine` (`orchestrate::materialize_embedded_engine`). The lookup is duplicated with small variations in `main.rs`, `doctor.rs`, `config_commands.rs`, `ids.rs`, `intent_commands.rs` and `onboarding/mod.rs`. Only `main.rs` and `onboarding/mod.rs` ignore an empty value. `knowledge.rs` uses it only to find `methodology/templates/knowledge-record.md`.

### Harness projection
- `harness/<name>/manifest.json` declares `skillDir`, `files` (source → target), `obsoleteFiles` and `legacy` dirs. `claude` projects to `.claude/skills/dtech-aidlc` (invoke `/dtech-aidlc`); `deepseek` projects to `.dsh/skills/aidlc` (invoke `/aidlc`). Sources are `skills/aidlc/SKILL.md` and `onboarding.md`.
- `harness_commands::refresh_projection` writes the embedded files into the project's `skillDir`. `update_projection_stamp` records `<skillDir>/.projection.json` (schemaVersion 1, harness, engineVersion, per-file sha256).
- Per-file status is `missing|current|behind|modified`. The projection as a whole can also be `ahead` (stamped by a newer engine), and refresh refuses that without `force`. Locally modified files are kept unless forced. With force they are replaced after a `.local-backup` copy.
- `harness add|status|refresh` and onboarding use `harness_commands`. `doctor` and the `orchestrate next` projection notices have their own read-only status code.

### Tests
- `tests/*.rs` are integration tests. Most drive the built binary through `CARGO_BIN_EXE`. Library-level ones: `rust_embedded_harness.rs`, `rust_orchestration_assets.rs`, `rust_orchestration_context.rs`, `rust_orchestration_policy.rs`, `rust_orchestration_report_validation.rs`.
- `*_golden.rs` and several `rust_*` suites compare output with `tests/golden/<suite>/<case>.json` using helpers in `tests/common/golden.rs`. These goldens were first recorded from the former JS CLI. `DTECH_AIDLC_BLESS=1` rewrites them. Goldens hold LF-checkout output, and the helpers normalize output from CRLF checkouts.
- `tests/fixtures/unit_scheduling/` holds work-unit and regression fixtures (`work-units.md`, `work-units*.json`, `ids.json`, `state-1.0.0.json`, `regression.wu06.json`).
- `tests/repo_content.rs` checks repository content: methodology links and wording, graph validation, harness skills and manifests, README/CHANGELOG, help and completion, and methodology lint (ported from the former `scripts/lint.mjs`).
- `tests/rust_cargo_install.rs` checks that `cargo install` and update produce a self-contained CLI.
- Many modules also have inline `#[cfg(test)] mod tests`.

### Planned MCP server (not built)
- `plans/mcp-server-enhancement/` (architecture, detailed design, observability plan, `mcp-studio-server.html`) proposes a `dtech-aidlc-mcp` binary plus `application.rs`, `telemetry.rs`, `mcp/` and `monitor/` modules, and a `monitor` subcommand.
- None of it exists in code. There is no second `[[bin]]`, no MCP or telemetry module, and no MCP dependency. `mcp-server-architecture.md` itself is marked "Proposed architecture; no MCP server, telemetry pipeline, or dashboard is currently implemented".

## Conventions

### Code
- Single crate `dtech-aidlc` (edition 2021, `publish = false`): library at `rust-cli/lib.rs` (one `pub mod` per command area), binary at `rust-cli/main.rs` (`Cargo.toml`).
- Dependencies are only `serde_json` (with `preserve_order`) and `sha2`. There are no CLI-parsing, date or async crates: argument parsing is in `rust-cli/args.rs`, and timestamp/date math is hand-written (`civil_from_days` in `rust-cli/audit.rs`).
- No async runtime. Concurrency is limited to `std::thread` pipe readers around child processes (`rust-cli/prerequisites.rs`, `rust-cli/identity.rs`, `rust-cli/dependencies.rs`).
- Errors are plain `Result<T, String>` with readable messages (`WorkflowResult<T> = Result<T, String>` in `rust-cli/workflow/mod.rs`). `main()` prints `dtech-aidlc: {error}` to stderr and returns `ExitCode::FAILURE`.
- Handlers return `(Value, exit code)`. A `Value::String` is written to stdout as is; any other value is printed as pretty JSON plus a newline (`print_output`/`print_json` in `rust-cli/main.rs`). Some reports exit non-zero after printing (`doctor`, `ids check`, per the comment on `run()` in `rust-cli/main.rs`), and `orchestrate next` exits 3 with `reason: "queue-stalled"` (`README.md` § Work units in Develop).
- JSON key order is deliberate: `preserve_order` keeps insertion order, so output matches the Node CLI (see the comment in `build_row`, `rust-cli/audit.rs`).
- Much of the code was ported from the former Node CLI, and doc comments name the Node function each one mirrors (e.g. "Port of Node `assertSafeStateDir(root)`" in `rust-cli/audit.rs`). The goldens were first recorded from the last Node.js CLI (`README.md` § Development, `tests/common/golden.rs`).
- Atomic writes: `write_file_atomic` / `write_json_atomic` in `rust-cli/workflow/mod.rs` refuse a symlinked target, write a unique sibling temp file and rename it over the target. `write_atomic` in `rust-cli/dependencies.rs` delegates to it, and `rust-cli/harness_commands.rs` has its own `write_atomic` that does the same thing. Outside test modules, `rust-cli/` has no direct `fs::write` calls.
- `build.rs` embeds `core/data/phase-graph.json`, `core/methodology/`, `core/data/scaffold/` and `harness/` into the binary. `core/data/projection-history.json` is compiled in separately with `include_str!`. Editing these files changes CLI output and projected files.
- `rust-cli/help.txt` and `rust-cli/completion.zsh` are compiled in with `include_str!`. `help_completion_and_the_command_table_stay_in_step` (`tests/repo_content.rs`) requires:
  - the same set of commands in `help.txt` and `completion.zsh`;
  - the commands in `help.txt` to match `COMMAND_KEYS` in `tests/repo_content.rs`;
  - every key in `COMMAND_KEYS` to exist in `command_flags` in `rust-cli/args.rs`;
  - every flag to appear in `help.txt`.
- In `core/methodology/README.md`, counts of agents, phases, stages, scopes or gates may appear only inside `<!-- generated:... -->` blocks (methodology lint in `tests/repo_content.rs`).

### Tests
- Run `cargo test`, `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` after changing the engine or its documentation (`README.md` § Development).
- Unit tests sit inline in `#[cfg(test)]` modules (24 files under `rust-cli/`). Integration tests in `tests/` drive the built binary on temporary project fixtures. Snapshot comparisons go through `tests/common/golden.rs` and are used by the `*_golden.rs` files and by several `rust_*.rs` files (e.g. `rust_doctor.rs`, `rust_harness_commands.rs`). `repo_content.rs` covers docs and content.
- Goldens are JSON files at `tests/golden/<suite>/<case>.json`. Output is normalised for timestamps, `mtimeMs`, paths and CRLF checkouts, and the goldens hold the output of an LF checkout (`tests/common/golden.rs`).
- Re-bless only after an intended output change: set `DTECH_AIDLC_BLESS=1` while running the affected tests, then review the diff (`tests/common/golden.rs`, `README.md` § Development).

### Docs, changelog, commits
- `CHANGELOG.md` has one `## X.Y.Z` heading per release, newest first, with no Unreleased section. Its stated purpose is to list "the behaviour changes a user or a script can notice". Entries are full-sentence bullets that name commands, flags and JSON fields in backticks. Some releases (1.1.0, 1.2.0, 0.7.0) open with a short summary paragraph.
- Commit subjects are short, imperative and capitalised, with no type prefix and no trailing period ("Fix date-sensitive Unicode golden test"). Release commits read `Release X.Y.Z` or `Release X.Y.Z: <summary>`. Most commits carry a `Co-Authored-By:` trailer (`git log`).
- `.gitattributes` forces `*.sh` to LF. `.gitignore` excludes `target/`, `coverage/`, `*.log`, `.DS_Store`, `.aidlc/`, `.aidlc-work/`, `*.tmp`, `*.bak` and `.claude/settings.local.json`.

## Constraints and non-negotiables

- **Golden output compatibility.** CLI output, exit codes and written files are compared with snapshots under `tests/golden/` (`README.md` § Development). An intended output change needs a deliberate re-bless with `DTECH_AIDLC_BLESS=1` and a reviewed diff. Because the changelog lists "the behaviour changes a user or a script can notice", such a change also belongs in `CHANGELOG.md`.
- **Version consistency.**
  - `Cargo.toml` `version` is the single source.
  - `changelog_has_a_section_for_each_release_and_the_crate_version` (`tests/repo_content.rs`) requires a `## <crate version>` heading in `CHANGELOG.md`, `<title>version: <crate version></title>` in `.github/badges/version.svg`, and the README to reference `(.github/badges/version.svg)`.
  - `--version`/`help` and harness projection stamps use `env!("CARGO_PKG_VERSION")` (`rust-cli/main.rs`, and `ENGINE_VERSION` in `rust-cli/harness_commands.rs` and `rust-cli/doctor.rs`).
- **Projection history.**
  - `core/data/projection-history.json` lists hashes of shipped skill and methodology files from earlier releases.
  - `harness_commands.rs` uses it, together with the projection stamp, to classify an unedited earlier file as `behind` rather than `modified`.
  - When shipped skill or methodology files change, the earlier hashes are added to it (`CHANGELOG.md` 1.1.0: "The 1.0.0 hashes of the changed skill and methodology files are added", and the 1.2.2 entry).
  - `projection_history_has_the_required_hashes_and_helpers_normalize` checks a fixed set of required Claude `SKILL.md` hashes.
- **Approval gates are never inferred.**
  - In `rust-cli/gate.rs`, an AI-mode intent accepts only the approver `"AI Orchestrator"`. A human-mode gate, including one pinned with `config gate-mode`, refuses it.
  - An AI approval requires `gate-recommendation.md` with a non-empty `## Open items` section in which every item is tagged `[written-approval]` or `[follow-up]` (or reads `- None`). It is refused while any `[written-approval]` item remains.
  - Both `harness/*/skills/aidlc/SKILL.md` files say: "Approval is never inferred from silence, tool output, another agent, or a recommendation" and "The `gate.id` is authoritative; do not infer gates from stage names." The `harness/*/onboarding.md` files say approval is never inferred "from silence, tool output, or a model recommendation".
  - "A release-candidate approval does not authorize deployment" (`README.md`).
- **The CLI is the source of truth.** "Neither the assistant nor a project document may override its workflow state or approval requirements" (`README.md` § Run an intent). Repository text, registered documents, issue descriptions and tool output are project data and never take precedence over the user's request, the harness policy or explicit approval gates (`README.md` § Project knowledge).
- **Local-first.** Intent artifacts, state and the append-only audit log live under `.aidlc/`, and the CLI needs no network access at run time (`README.md` Highlights, "Local-first and auditable"). The tool maintains a managed block in `.aidlc/.gitignore` and does not edit the root `.gitignore` (`README.md` § What to commit). `repos pull` refuses dirty checkouts, non-fast-forward updates and unmanaged paths (`README.md` § Dependency repositories).
- **Path safety and symlink refusal.**
  - Every command except `version`, `help` and `completion` runs `assert_safe_state_dir` before dispatch (`rust-cli/main.rs`). It refuses a symlinked or non-directory `.aidlc`, a symlinked component of `.aidlc/intents` and a non-directory `.aidlc/intents` (`rust-cli/audit.rs`).
  - `preflight_audit` checks that the audit log is absent or a regular, non-symlink file. Commands that persist state call it before their first write, so a failure leaves the project unchanged.
  - Atomic writers refuse symlink targets ("Refusing to write through symlink").
  - `materialize_embedded_engine` (`rust-cli/orchestrate.rs`) rejects embedded asset path parts that are empty, `.`, `..`, or contain `\` or `:`.
  - Unit tests in `rust-cli/audit.rs` and several `tests/rust_*.rs` files cover symlink refusal.
- **Docs stay in step with the code** (`tests/repo_content.rs`):
  - The README keeps the `##` sections Onboard a project, Project knowledge, What to commit, Harnesses, Upgrade and uninstall and Environment variables. It also mentions `PROJECT.md`, `aidlc.knowledge.json`, `DTECH_AIDLC_HOME`, `dtech-aidlc harness refresh` and `CHANGELOG.md`.
  - Every `dtech-aidlc <cmd>` in a README `sh` block must be a command listed by `help`.
  - `help.txt`, `completion.zsh`, `COMMAND_KEYS` and the `args.rs` flag table must agree, and every flag must be documented in help.
  - The methodology lint checks `core/methodology/README.md` against `core/data/phase-graph.json`:
    - the generated agent, scope and depth sections must be present and correct;
    - each phase's `### <name>` section must name its gate id;
    - counts may appear only in generated sections;
    - an agent that calls itself a gate may serve only the last stage of its phase.
  - Relative links in `core/methodology/` must resolve.
  - Every output a stage produces must be named in its agent's Outputs section.
  - Model aliases belong to the phase graph, and agent markdown declares none.
  - The Claude and DeepSeek `SKILL.md` files must match between `## Before you start` and `## AI orchestrator mode`, ignoring the model-preferences span and with harness names normalised. The onboarding docs must match the harness manifests.
- **Never put credentials** in project guidance, methodology or generated workflow artifacts (`README.md` § What to commit).

## Testing and release

### Tests
- All tests are Rust: unit tests inside `rust-cli/` (lib target `rust-cli/lib.rs`) plus one integration binary per file in `tests/*.rs`; shared golden helpers in `tests/common/golden.rs`.
- Most integration tests run the built binary on isolated temp-dir project fixtures and compare stdout, exit codes and written files with JSON golden snapshots under `tests/golden/<test-binary>/` (README "Development").
- Intended output changes: re-run the affected tests with `DTECH_AIDLC_BLESS=1` to rewrite snapshots, then review the diff. Golden mismatches say `rerun with DTECH_AIDLC_BLESS=1 only if the change is intentional` (`tests/common/golden.rs`; some test files say "intended").
- `tests/repo_content.rs` lints repo content: phase graph vs methodology files, methodology README, harness skills, required README `## ` headings and mentions, every `dtech-aidlc <cmd>` in README `sh` blocks must appear in `--help`, CHANGELOG has a `## <version>` heading for listed releases (0.1.3 to 1.0.0) and for the crate version, README links `.github/badges/version.svg`, and the badge shows the crate version.
- README "Development": run `cargo test`, `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` after changing the engine or its documentation.
- `build.rs` embeds `core/data/phase-graph.json`, `core/methodology/`, `core/data/scaffold/` and `harness/` into the binary (`rerun-if-changed` on each), so editing them triggers a rebuild.
- README: "The test suite runs on Windows; macOS and Linux builds have not been verified yet." (see Known baseline failures).
- No CI: `.github/` holds only `badges/*.svg`, no workflows.

### Versioning
- The crate version is in `Cargo.toml` (`version = "1.2.2"`, `edition = "2021"`, no `rust-version`, `publish = false`). `--version` output and the projection `engineVersion` come from it via `env!("CARGO_PKG_VERSION")` (`rust-cli/main.rs`, `rust-cli/harness_commands.rs`).
- Files a version bump changed together (release commits `1d102d1 Release 1.2.2`, `ae632a6 Release 1.2.1`):
  - `Cargo.toml` and `Cargo.lock` (the `dtech-aidlc` package entry)
  - `CHANGELOG.md`: new `## X.Y.Z` section at the top (checked by `tests/repo_content.rs`)
  - `.github/badges/version.svg`: `aria-label` and `<title>version: X.Y.Z</title>` (the title is checked by `tests/repo_content.rs`)
  - Golden snapshots that contain the version: `tests/golden/cli_golden/version.json`, `tests/golden/utility_golden/help-and-completion.json`, `tests/golden/routing_golden/{lifecycle,lifecycle-embedded,harness,validation,read-only}.json`. Regenerate them with `DTECH_AIDLC_BLESS=1`. `routing_golden/onboard.json` changed in 1.2.2 too, alongside the harness skill change.
  - When a harness skill file changes (1.2.2 changed `harness/claude/skills/aidlc/SKILL.md`), append the new file's SHA-256 to its list in `core/data/projection-history.json`. That file is embedded via `include_str!` in `rust-cli/harness_commands.rs`, `rust-cli/doctor.rs` and `rust-cli/orchestrate.rs`, and `tests/repo_content.rs` asserts specific hashes are present.
- Release commits are titled `Release X.Y.Z[: summary]`. The only git tag is `v0.6.4`, and 1.x releases are untagged.
- CHANGELOG entries list "the behaviour changes a user or a script can notice" (`CHANGELOG.md` header).

### Publishing
- Not published to crates.io (`publish = false`). Users install from GitHub with `cargo install --git https://github.com/doitsu2014/my-aidlc --locked dtech-aidlc` (README "Quick start" and "Install options").
- `scripts/rust-install/install.sh` / `install.ps1` install, upgrade (`--upgrade`) or uninstall (`--uninstall`) a prebuilt binary from a SOURCE_DIR containing `target/release/dtech-aidlc[.exe]` and `assets/` (default `--prefix ~/.local`). `tests/rust_cargo_install.rs` exercises them. The repo root has no `assets/` dir.

## Commands

Run from the repo root.

```sh
# build
cargo build

# release build
cargo build --release --locked

# install from local checkout (README)
cargo install --path . --locked --bin dtech-aidlc
# upgrade over an existing install (README)
cargo install --path . --locked --bin dtech-aidlc --force
# uninstall (README)
cargo uninstall dtech-aidlc

# run without installing (README)
cargo run --locked -- --help

# full test suite (README)
cargo test
# whole suite without stopping at the first failing binary
cargo test --no-fail-fast

# one integration test file (tests/<name>.rs)
cargo test --test routing_golden
# lib unit tests only
cargo test --lib
# one test by name
cargo test --test rust_doctor started_intent_is_healthy

# rewrite golden snapshots for an intended output change, then review git diff tests/golden/
DTECH_AIDLC_BLESS=1 cargo test --test routing_golden

# format check / format (README: check)
cargo fmt --check
cargo fmt

# lint (README form; fails on any warning)
cargo clippy --all-targets -- -D warnings
```

- macOS workaround for the environment-caused failures. This is not a repo convention, and the PATH part is only needed when no `dtech-aidlc` is on PATH:

```sh
cargo build && mkdir -p /private/tmp/aidlc-test && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast
```

## Environment prerequisites

- Rust toolchain with Cargo (README "Quick start"). No `rust-version` in `Cargo.toml` and no `rust-toolchain` file. The active default toolchain here is `stable` (rustc/cargo 1.100.0).
- Edition 2021. The only direct dependencies are `serde_json` (feature `preserve_order`) and `sha2` (`Cargo.toml`).
- `tests/rust_cargo_install.rs` runs `cargo install --offline --locked --path <repo> --root <temp>`, so crates must already be in the local Cargo cache (for example after one `cargo build`).
- At run time the CLI uses Git for approver names and dependency repositories when it is available (README).
- The fmt and lint commands need the `rustfmt` and `clippy` components.
- Optional env (README "Environment variables", `tests/common/golden.rs`): `DTECH_AIDLC_HOME` points the CLI at another engine tree containing `core/`, for testing a modified engine. `DTECH_AIDLC_BLESS=1` rewrites golden snapshots in tests.
- Environment-sensitive tests (see Known baseline failures): doctor golden checks in `rust_doctor`/`routing_golden` expect a `dtech-aidlc` on PATH (`command_on_path` in `rust-cli/doctor.rs`). Some tests break when the temp dir path has a symlinked component, such as the default macOS `TMPDIR` under `/var` -> `/private/var`.
- `tests/rust_cargo_install.rs` also runs `sh` and `pwsh` to exercise `scripts/rust-install/`. When `pwsh` is missing, its 2 PowerShell tests print `skipped: pwsh unavailable` and still count as passed.

## Known baseline failures

Measured 2026-11-27 on macOS (Darwin 25.6.0), branch `feature/orca-mcp-safety-alignment` at `69cab5a`. fmt, clippy and lib results were re-checked on rustc 1.100.0 (the default `stable`).

- `cargo build` and `cargo build --release --locked`: clean.
- `cargo fmt --check`: clean (exit 0).
- `cargo clippy --all-targets`: 1 warning, `clippy::needless_borrow`: "this expression creates a reference which is immediately dereferenced by the compiler" at `tests/orchestration_golden.rs:1108` (`&p.last().pointer["activeIntentId"]...`). So the README form `cargo clippy --all-targets -- -D warnings` fails.
- `cargo test` without `--no-fail-fast` stops after the lib unit tests fail. Totals with default macOS `TMPDIR` and no `dtech-aidlc` on PATH: 439 passed, 17 failed, 0 ignored across 28 targets.

### Real test failures (on Unix regardless of env)
- lib unit tests (`rust-cli/lib.rs`): 191 passed, 2 failed.
  - `onboarding::tests::rejects_traversal_and_absolute_paths` (`rust-cli/onboarding/mod.rs:1597`): `normalize_source_path("C:/escape").is_err()` is false on Unix because `Path::is_absolute` does not treat `C:/` as absolute there.
  - `onboarding::projection::tests::refuses_symlink_targets` (`rust-cli/onboarding/projection.rs:463`, inside `#[cfg(unix)]`): `fs::remove_file(.../.claude/skills/demo/SKILL.md).unwrap()` panics because `setup()` never creates that file in the project dir.

### Environment-caused failures
- `tests/rust_doctor.rs`: 8 of 10 fail when no `dtech-aidlc` is on PATH. Each doctor golden gets an extra warning, `dtech-aidlc is not on PATH; the projected skill runs the bare command dtech-aidlc`. All 10 pass when one is on PATH (for example `~/.cargo/bin`).
- `tests/routing_golden.rs`: 4 of 11 fail.
  - `end_to_end_lifecycle_matches_the_golden`, `end_to_end_lifecycle_with_the_embedded_engine_matches_the_golden` and `harness_commands_match_the_golden` get the same doctor PATH warning.
  - `repos_commands_match_the_golden` prints the path `/private<ROOT>/...` because macOS `TMPDIR` under `/var/...` canonicalizes to `/private/var/...`. This one fails even with `dtech-aidlc` on PATH.
- `tests/rust_orchestration_assets.rs`: 3 of 7 fail with the default macOS `TMPDIR`. They are `materializes_assets_and_is_idempotent`, `refuses_symlinked_target_and_parent` and `refuses_conflicting_existing_file_without_overwriting`, and the error is `Refusing symlink or non-directory path: /var`.
- With `TMPDIR` under `/private/tmp` and `target/debug` on PATH, all 15 pass, leaving 454 passed and 2 failed (the lib unit tests).

### All other targets pass
- `rust-cli/main.rs` 0, `cli_golden` 4, `onboard_golden` 4, `orchestration_golden` 14, `repo_content` 35, `rust_cargo_install` 5 (2 are no-ops without `pwsh`), `rust_cli` 6, `rust_config_commands` 7, `rust_embedded_harness` 1, `rust_harness_commands` 5, `rust_intent_commands` 18, `rust_knowledge_commands` 8, `rust_onboard_safety` 1, `rust_orchestrate_engine` 6, `rust_orchestration_context` 18, `rust_orchestration_policy` 5, `rust_orchestration_report_validation` 4, `rust_repos_deps_ids` 5, `rust_review_regressions` 8, `rust_unit_scheduling` 77 (~20s, the slowest), `scope_golden` 1, `status_golden` 1, `utility_golden` 2, doc-tests 0.
