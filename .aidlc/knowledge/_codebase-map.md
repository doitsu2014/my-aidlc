# Codebase map

- Status: draft
- Mapped on: 2026-11-27
- Mapped at commit: 69cab5a (working tree has only untracked `.claude/`, `PROJECT.md`, `aidlc.knowledge.json`)
- Mapped by: AIDLC Analyze, context-and-risk stage (context-risk-analyst), intent please-implement-plans-mcp-server-enhancement-sc
- Sources consulted: `project` (`PROJECT.md`), `README.md`; code: `Cargo.toml`, `build.rs`, `rust-cli/{lib,main,args,orchestrate,status,doctor,work_units,workflow/mod,onboarding/*,harness_commands}.rs`, `core/data/phase-graph.json`, `harness/*`, `tests/*`, `plans/mcp-server-enhancement/` (names only)

Intent scope note (User-provided): this map serves intent 1 of 2 (shared application facade over the CLI + MCP stdio server binary). Telemetry/monitor are out of scope.

## 1. System summary

- Observed (`Cargo.toml`, `rust-cli/lib.rs`): single Cargo package `dtech-aidlc` v1.2.2, edition 2021, `publish = false`. One library crate (`rust-cli/lib.rs`, 28 `pub mod`s) plus one binary (`rust-cli/main.rs`). ~27.5k lines of Rust in `rust-cli/`.
- Observed (`README.md`, `PROJECT.md` Purpose): a local-first CLI workflow engine. A request moves through Analyze, Ideate, Develop, Launch, Curate, Archived; the CLI owns state under `.aidlc/` and an append-only `audit.jsonl`; the assistant does stage work; the human approves gates.
- Observed (`build.rs`): the binary embeds `core/data/phase-graph.json`, `core/methodology/**`, `core/data/scaffold/**` and `harness/**` via generated `include_bytes!` (`ASSETS`). No network at run time.
- Observed (`Cargo.toml`): only two dependencies, `serde_json` (preserve_order) and `sha2`. No async runtime, no MCP/JSON-RPC library, no CLI-parser crate (hand-written `rust-cli/args.rs`). Adding an MCP server is therefore a new-dependency or hand-rolled-protocol decision (Open question, owner: solution design).
- Observed (`plans/mcp-server-enhancement/`): design docs for the MCP server exist (architecture, detailed design, observability plan, HTML mockup). No MCP code exists: `grep -i mcp rust-cli/*.rs` finds nothing.

## 2. Components

| Name | Path | Responsibility | Depends on |
|---|---|---|---|
| Binary/router | `rust-cli/main.rs` (602 lines) | Parses argv, validates flags/arg counts, resolves `--project`, safety-checks `.aidlc`, dispatches by command word, prints JSON/text, maps results to exit codes. Also holds real logic: `run_onboard`, onboard validation, `run_orchestrate`, `run_unit_queue`, `intent_text`, `external_engine`, stdout writing. Observed. | lib modules |
| Arg parsing | `rust-cli/args.rs`, `help.txt`, `completion.zsh` | Flag tables per command (`command_flags`, `command_max_args`), `parse_args`. Observed. | none |
| Orchestration | `rust-cli/orchestrate.rs` (4735 lines) | `orchestrate_next`, `orchestrate_report_with_engine`, `finish_directive`, `finish_report`, `Engine` (embedded vs external engine root), `materialize_embedded_engine`. Observed. | workflow, work_units, regression, artifacts, gate, state_migration |
| Orchestration helpers | `orchestration_context.rs`, `orchestration_policy.rs`, `orchestration_report_validation.rs`, `orchestration_assets.rs`, `prerequisites.rs` | Directive context building, policy, report validation, writing methodology assets to `.aidlc/engine`, stage preflight. Observed. | orchestrate, knowledge |
| Work units | `rust-cli/work_units.rs` (4483 lines), `unit_commands.rs` | Develop-phase unit ledger, scheduling/claims, park/revisit/queue. Observed. | workflow |
| Workflow state | `rust-cli/workflow/mod.rs`, `unicode_tables.rs`, `state_migration.rs` | Load/save pointer (`.aidlc/state.json`) and per-intent `state.json`, intent ids/slugs, stage ordering, legacy v1 migration. Observed. | audit |
| Audit and safety | `rust-cli/audit.rs` | Append-only `audit.jsonl`, `assert_safe_state_dir` (symlink refusal). Observed. | none |
| Scope/graph/config | `rust-cli/scope.rs`, `config_commands.rs`, `config_read.rs` | Read embedded or external phase graph, scope resolution, `.aidlc/config.json` read/validate/edit. Observed. | assets |
| Status and doctor | `rust-cli/status.rs`, `doctor.rs` | Read-only status report; health checks (includes "dtech-aidlc on PATH" warning). Observed. | workflow, scope |
| Intents, gates, artifacts | `intent_commands.rs`, `gate.rs`, `artifacts.rs`, `regression.rs`, `identity.rs`, `ids.rs` | Intent lifecycle commands, gate decisions, artifact verification, regression handling, approver identity, stable-id checks. Observed. | workflow |
| Knowledge | `knowledge.rs`, `knowledge_commands.rs` | `.aidlc/knowledge/` records and preflight, `aidlc.knowledge.json` source registry. Observed. | none |
| Repos/deps | `dependencies.rs` | `repos` and `deps` commands (git clone/ref handling). Observed. | std::process (git) |
| Onboarding | `rust-cli/onboarding/mod.rs`, `projection.rs` | Project scaffold, harness projection into `.claude/` etc., source path normalization. Observed. | embedded assets |
| Harness commands | `rust-cli/harness_commands.rs` | `harness` subcommands (list/add/remove/refresh style) over `harness/*`. Observed (names only; not read in depth). | onboarding |
| Methodology/data | `core/data/phase-graph.json`, `core/methodology/{agents,phases,templates,contracts.md}`, `core/data/scaffold/` | Engine content, embedded in the binary. Observed. | none |
| Harness projections | `harness/claude/`, `harness/deepseek/` (`manifest.json`, `onboarding.md`, `skills/`) | Files projected into user projects. Observed. | none |
| Install scripts | `scripts/rust-install/` | Install helpers (exercised by `tests/rust_cargo_install.rs`). Observed (not read). | none |

## 3. Entry points

- Observed: binary `dtech-aidlc` (`Cargo.toml` `[[bin]]`, `rust-cli/main.rs` `main` -> `run`). Commands routed in `run()`: `onboard|init`, `orchestrate next|report|park|queue|revisit`, `scope`, `status`, `intent ...`, `harness ...`, `config ...`, `doctor`, `ids ...`, `knowledge ...`, `repos ...`, `deps ...`, `completion zsh`, `version`, `help`. Full list in `rust-cli/help.txt`.
- Observed main paths:
  - `orchestrate next`: `run_orchestrate` (main.rs) loads graph (`scope::read_graph`, external engine via `DTECH_AIDLC_HOME` or embedded), `knowledge_preflight`, then `orchestrate::orchestrate_next`, then `finish_directive`, prints JSON.
  - `orchestrate report`: `run_orchestrate` builds `ReportOptions` from flags, calls `orchestrate_report_with_engine`, `finish_report`, prints JSON.
  - `park|queue|revisit`: `run_unit_queue` -> `unit_commands`.
  - `status|scope|intent list`: `run_read_only` -> `status::status` / scope.
  - `doctor`, `config`, `harness`, `intent`, `knowledge`, `repos`, `deps`, `ids`: `lib` module `run(key, root, rest, flags)` returning `(Value, exit_code)`.
- Observed: there is no public facade. The command handlers split into two shapes: (a) lib `run(...)` functions that take parsed flags (`BTreeMap<String, FlagValue>` from `args`), and (b) logic that lives only in the binary (`run_onboard`, `run_orchestrate`, `run_unit_queue`, validation helpers, printing). A shared facade for an MCP server must lift (b) into the library. Observed in `rust-cli/main.rs`.
- Observed: the Rust lib modules do not print to stdout (only test-skip `eprintln!` and one in `dependencies.rs:1204`); all stdout is in `main.rs`. This matters for an MCP stdio server, where stdout is the protocol channel.
- Observed: errors are `String`s; `main` prints `dtech-aidlc: <error>` to stderr and exits 1. Exit code 3 (`BLOCKED_EXIT_CODE`, `orchestrate.rs`) is a blocked `next`; `doctor` and `ids check` print a report and exit 1.
- Observed: harness entry (user-facing): slash command `/dtech-aidlc` (Claude) / `/aidlc` (DeepSeek) from `harness/*/manifest.json`; skills call the bare `dtech-aidlc` command.

## 4. External dependencies and integrations

- Observed (`Cargo.toml`, `Cargo.lock`): crates `serde_json`, `sha2`. Std only otherwise.
- Observed (`rust-cli/dependencies.rs`): shells out to `git` for `repos`.
- Observed (`README.md`, `PROJECT.md`): env var `DTECH_AIDLC_HOME` (alternate engine tree), `DTECH_AIDLC_BLESS=1` (golden rewrite, tests). Harness integrations are file projections (`.claude/`), not API calls.
- Observed: no HTTP, no network, no async runtime.
- Assumption (impact: dependency choice for MCP; validate: solution design/ADR): a JSON-RPC stdio loop can be hand-written on `serde_json` alone, or an MCP SDK crate can be added; the plans may state the choice (not read in this stage).

## 5. Data

- Observed (`.aidlc/` in this repo; `PROJECT.md`): state is plain files. `.aidlc/state.json` (pointer to active intent), `.aidlc/config.json`, `.aidlc/audit.jsonl` (append-only), `.aidlc/intents/<id>/{intent.md,state.json,<phase>/<stage>/*.md}`, `.aidlc/knowledge/*.md`, `.aidlc/engine/` (materialized methodology), `.aidlc/shared/`, `aidlc.knowledge.json` at project root (source registry).
- Observed (`rust-cli/orchestrate.rs` header): writes are checked for concurrent change (pointer and state bytes must be unchanged since read), and nothing is written until validation passes. Concurrent processes (a server plus a CLI) rely on this check, not a lock (Assumption: no file lock exists; validate by grepping for locking in `workflow/mod.rs`, not done).
- Observed: state schema is versioned with legacy migration (`state_migration.rs`).
- No database.

## 6. Conventions

- Link, do not repeat: `PROJECT.md` "Conventions" and "Constraints and non-negotiables" are authoritative.
- Observed in code: `Result<_, String>` errors; handlers return `(serde_json::Value, i32)`; JSON object key order preserved (`preserve_order`); module comments state ported-from-Node behaviour; goldens compare byte-level JSON output.
- Observed: `cargo fmt --check` is clean per `PROJECT.md` (not re-run here).

## 7. Tests

- Observed: framework is Rust built-in `cargo test` (no external test crates). Unit tests inline in `rust-cli/**` (`#[cfg(test)]`, in the lib target); integration tests in `tests/*.rs` (25 top-level files), shared helper `tests/common/golden.rs`, golden JSON in `tests/golden/<suite>/`, fixtures in `tests/fixtures/unit_scheduling`.
- Observed commands (README/PROJECT.md): whole suite `cargo test --no-fail-fast`; one file `cargo test --test <name>`; lib only `cargo test --lib`; one test `cargo test --test rust_doctor started_intent_is_healthy`; rewrite goldens `DTECH_AIDLC_BLESS=1 cargo test --test routing_golden` (not run).
- Observed (run for this map): `cargo test --no-fail-fast` from the project root, 2026-11-27, rustc 1.100.0, macOS, `dtech-aidlc` present on PATH (`~/.cargo/bin`), default macOS `TMPDIR` (`/var/folders/...`). Wall time about 1 minute incremental (the `rust_unit_scheduling` target alone takes ~20 s). Result: 28 targets, 450 passed, 6 failed, 0 ignored; exit nonzero (3 targets failed: `--lib`, `--test routing_golden`, `--test rust_orchestration_assets`).
- Observed per target (passed/failed): lib 191/2; main 0; cli_golden 4; onboard_golden 4; orchestration_golden 14; repo_content 35; routing_golden 10/1; rust_cargo_install 5; rust_cli 6; rust_config_commands 7; rust_doctor 10; rust_embedded_harness 1; rust_harness_commands 5; rust_intent_commands 18; rust_knowledge_commands 8; rust_onboard_safety 1; rust_orchestrate_engine 6; rust_orchestration_assets 4/3; rust_orchestration_context 18; rust_orchestration_policy 5; rust_orchestration_report_validation 4; rust_repos_deps_ids 5; rust_review_regressions 8; rust_unit_scheduling 77; scope_golden 1; status_golden 1; utility_golden 2; doc-tests 0.
- Observed failures (6), all matching `PROJECT.md` "Known baseline failures":
  1. `onboarding::tests::rejects_traversal_and_absolute_paths` (real, `C:/escape` not absolute on Unix).
  2. `onboarding::projection::tests::refuses_symlink_targets` (real, test setup bug).
  3. `routing_golden::repos_commands_match_the_golden` (env: `/private/var` canonicalization).
  4-6. `rust_orchestration_assets`: `materializes_assets_and_is_idempotent`, `refuses_conflicting_existing_file_without_overwriting`, `refuses_symlinked_target_and_parent` (env: `/var` symlink in `TMPDIR`).
- Observed: `rust_doctor` passes 10/10 and the other 3 `routing_golden` baseline-env failures did not occur because `dtech-aidlc` is on PATH, as `PROJECT.md` predicts. Baseline for this intent: 450 pass / 6 fail in this environment; 2 are real, 4 environmental. No new failures.
- Assumption (impact: MCP work needs a green baseline to attribute regressions; validate: run with `TMPDIR=/private/tmp/...`, per `PROJECT.md` which reports 454/2 that way; not re-run here).
- Observed: no test exists for MCP behaviour. Goldens pin CLI stdout/exit codes, which are the regression net for a facade refactor (AC-001..AC-014 and US-001..US-013 are defined in the problem-framing artifacts, not here).

## 8. Build and run

- Observed (the test run built the debug profile cleanly): `cargo build`. `PROJECT.md` reports `cargo build --release --locked` clean (not re-run here).
- Run without install: `cargo run --locked -- --help` (README, not run). Install: `cargo install --path . --locked --bin dtech-aidlc` (README, not run).
- Observed: a second `[[bin]]` for an MCP server would be declared in `Cargo.toml`; `build.rs` embeds assets for every target of the package.
- Observed: no CI workflows found (`.github/` contains only `badges/`); Assumption: CI lives elsewhere or is absent (owner: project owner).

## 9. Known risks and technical debt

1. Observed: logic that a facade must reuse is stranded in `rust-cli/main.rs` (onboard, orchestrate wiring, queue, printing), and `orchestrate.rs` (4.7k lines) and `work_units.rs` (4.5k lines) are very large. Refactor risk is high, offset by byte-level goldens.
2. Observed: stale references. `rust-cli/main.rs`, `orchestrate.rs` and `tests/common/golden.rs` comments cite the former Node implementation (`core/tools/dtech-aidlc.mjs`, `workflow.mjs`), but `core/tools/` does not exist. Documentation-only drift; no contradiction with `PROJECT.md`.
3. Observed: help text and flag validation live in `args.rs`/`help.txt`, so a new command surface (MCP tools) would duplicate the flag tables unless the facade owns a typed request model. Open question (owner: solution design).
4. Observed: stdout hygiene, errors as plain strings, and `process`-level exit codes in the router mean MCP error mapping (blocked exit 3, doctor exit 1) must be defined explicitly.
5. Observed: baseline test failures (section 7) will mask regressions in onboarding and assets code unless fixed or scoped.
6. Observed (`PROJECT.md`): `cargo clippy --all-targets -- -D warnings` fails on one `needless_borrow` at `tests/orchestration_golden.rs:1108`; not re-run here.
7. Assumption (no lock found, section 5): an MCP server running beside a CLI user can race on state writes; the only guard is compare-before-write.

## 10. Gaps

- Not inspected in depth: `dependencies.rs`, `knowledge.rs`, `ids.rs`, `regression.rs`, `intent_commands.rs`, `config_commands.rs`, `harness_commands.rs`, `state_migration.rs`, `work_units.rs` internals, `scripts/rust-install/`, `harness/*/skills/` content, `core/methodology/*` content, `tests/golden/` and `tests/fixtures/` content, `CHANGELOG.md`.
- Not read: `plans/mcp-server-enhancement/*` (design intent for MCP; read by problem-framing, cite from there).
- Not run: `cargo build --release`, `cargo fmt --check`, `cargo clippy`, a run with a `/private` `TMPDIR`, Windows paths. Those figures are from `PROJECT.md` only.
- Not verified: absence of file locking, concurrency behaviour, Windows/`pwsh` install tests (2 of 5 in `rust_cargo_install` are no-ops without `pwsh` per `PROJECT.md`).
- Discrepancy with `PROJECT.md`: none found in test results (all 6 observed failures are listed there). Minor: `PROJECT.md` records branch `feature/orca-mcp-safety-alignment` at `69cab5a`; the current branch is `feature/please-implement-plans-mcp-server-enhancement-sc` at the same commit. `PROJECT.md` totals (439/17 default env, 454/2 fixed env) were not reproduced exactly because this environment has `dtech-aidlc` on PATH (observed 450/6, within the stated model).
