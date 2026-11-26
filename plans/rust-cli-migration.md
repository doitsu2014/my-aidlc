# Rust CLI migration

## Goal and release boundary

Replace the Node.js CLI in one production cutover with a native Rust executable, while retaining compatibility with existing commands, project state, artifacts, harness projections, and installations.

**Status: cutover done (1.0.0).** The Rust binary `dtech-aidlc` is the only CLI. It routes every former Node command to a native handler (see the status below) and is installed with `cargo install --path . --locked --bin dtech-aidlc`. The Node CLI (`core/tools/*.mjs`), `package.json`/`package-lock.json`, the Node test suite (`tests/*.mjs`), `scripts/lint.mjs` and the Node installers (`scripts/install.sh`, `scripts/install.ps1`) have been removed. Parity with the last Node version is now guarded by golden snapshots under `tests/golden/`, recorded from the Node CLI (0.7.2-equivalent behaviour) before it was deleted; the methodology lint and repository-content checks were ported to `tests/repo_content.rs`. The sections below are kept as the record of how the migration was done.

## Compatibility requirements

- Preserve command names, arguments, validation, JSON shapes, stdout/stderr and exit statuses, including prerequisite-blocked exit status 3.
- Read and write existing `.aidlc` state, audit logs, work-unit state, gates, prerequisites and identifier registries; preserve legacy state migration and recovery semantics. Never silently reset an existing project.
- Preserve graph, methodology, harness and projection behavior, including protection of user-modified files and safe paths/symlinks.
- Preserve dependency and knowledge manifests, Git safety checks, onboarding, and shell completion.
- Provide runtime access to graph, methodology and harness assets; a standalone executable alone is not sufficient unless those assets are embedded or installed alongside it.
- Support upgrades from the current Windows and POSIX installers without deleting or overwriting unrelated files. Document build, install, uninstall and distribution requirements.

## Implementation sequence

1. Establish binary-level parity tests using isolated project fixtures and the existing Node CLI as oracle. Compare structured JSON semantically, normalize only nondeterministic fields, and check side effects, failure behavior and exit codes. Preserve existing Node tests until cutover.
2. Port argument parsing, command dispatch, graph loading, safe filesystem operations and project/config state. Exercise legacy state migration and error cases before enabling mutation.
3. Port workflow directives, work-unit sequencing, reporting, gates, prerequisites, IDs and intent lifecycle; compare entire intent runs and resume/rework behavior against the oracle.
4. Port onboarding, projections, knowledge, dependency/repository operations, completion and diagnostics, including path and security behavior across supported platforms.
5. Implement asset packaging and Windows/POSIX install, upgrade and uninstall. Test a real upgrade from a Node installation and verify that existing project data and user-modified projection files survive.
6. Run the full parity matrix and Rust tests on Windows and POSIX. Only then switch the shipped entry point and documentation to Rust, and remove Node runtime dependencies in the same cutover.

## Current implementation status

**Routing.** `rust-cli/main.rs` routes every command of the former `core/tools/dtech-aidlc.mjs` to a native handler; there is no Node fallback and no "not yet implemented" refusal. The router follows Node's order: version/help, unknown-option check (reporting the first disallowed option in the order given), positional-argument limits (`args.rs` now matches `COMMAND_FLAGS` and `COMMAND_MAX_ARGS` of `cli-spec.mjs`, checked by a unit test; `repos remove` and `knowledge index` limits were missing before), `--project` resolution like `path.resolve`, the `.aidlc` safety check for every command except `completion`, then dispatch. Unknown commands and actions print Node's messages. Output follows Node `out()` (strings raw, values as two-space JSON plus newline) and handler exit codes are kept (1 for failing `doctor`, `ids check`, `config prerequisite check` and refused `harness refresh` reports, 3 for prerequisite-blocked `orchestrate next`).

| Command | Handler | Binary-level Node parity (`tests/routing_golden.rs`) |
| --- | --- | --- |
| `onboard`, `init` (incl. `--rescan`) | `onboarding` + router validation | Validation errors: exact. Success: output and files match Node except `methodology` (embedded engine path) |
| `scope`, `status`, `intent list` | `scope`, `status` | Exact, incl. new, missing and broken-config projects |
| `orchestrate next`, `orchestrate report` | `orchestrate` | Exact through a gate, block, rework and second intent |
| `intent pause/resume/abandon/depth/rework/assign` | `intent_commands` | Exact (success and error cases) |
| `config` (`scope`, `mode`, `gate-mode`, `prerequisite`, `agents`) | `config_commands` | Exact (claude and deepseek) |
| `harness add/status/refresh` | `harness_commands` | Exact, incl. kept-modified and `--force` backups |
| `knowledge init/sources/list/show/check/index` | `knowledge_commands` | Exact |
| `repos add/list/pull/remove` | `dependencies::run_repos` | Exact, incl. a local clone, the stderr note and a dirty checkout |
| `deps add/list/pull` | `dependencies::run_deps` | Exact |
| `ids check` | `ids::run` | Exact |
| `doctor` | `doctor` | Exact (healthy and failing reports) |
| `help`, `version`, `completion zsh` | router | Exact |

Until the cutover, each parity step ran the built binary and `node core/tools/dtech-aidlc.mjs` on identical fresh fixtures and compared stdout, stderr, exit code and every project file (`.git` excepted). Before Node was removed, every such comparison was recorded as a golden snapshot under `tests/golden/<test>/` (`tests/common/golden.rs` and per-file helpers); the tests now compare the binary against those recordings, and blessing them again from Rust (`DTECH_AIDLC_BLESS=1`) reproduced the Node recordings byte for byte. Only nondeterministic or machine-specific values are normalised: project and engine roots, ISO timestamps, `mtimeMs`, commit ids, lock-file PIDs and, in some suites, the engine version and line endings of written files. Every command key also has its unknown-option and extra-argument errors compared, and an unsafe `.aidlc` is refused by every command but `completion`. The end-to-end lifecycle (onboard, orchestrate next/report through the Analyze gate, a blocked Develop stage and design rework, deps, pause/resume/abandon, knowledge index, doctor) runs twice: with `DTECH_AIDLC_HOME` naming this repository, and with the embedded engine (engine paths compared from the engine directory on; `.aidlc/engine/` not compared). Per-module parity suites (`rust_config_commands`, `rust_harness_commands`, `rust_intent_commands`, `rust_knowledge_commands`, `rust_repos_deps_ids`, `rust_doctor`, `rust_orchestration_*`) cover more edge cases in-process. All of this ran on Windows only.

**Known gaps.**

- **Onboarding reports a different `methodology` path.** Without `DTECH_AIDLC_HOME`, Rust reports the embedded engine path `<project>/.aidlc/engine/methodology` instead of a Node checkout. Output and written files otherwise matched Node (checked by direct comparison on Windows). Rust onboarding output does not match Node byte for byte, so the routing parity fixtures for other commands restore Node's recorded onboarded files (`tests/golden/routing_golden/fixture-onboarded-*.json`).
- **Deliberate differences, safer than Node:** state and the audit log are checked before any write (`orchestrate`, `intent depth`), symlinked or dangling state files are refused where Node writes through them, and changed state is re-read before writing. These have module-level tests, not Node comparisons.
- **Engine assets:** `DTECH_AIDLC_HOME` is honoured for the graph and methodology, but harness manifests and skill sources always come from the copies embedded in the binary.
- **Not tested here:** POSIX platforms, real symlinks (they need privileges on this Windows machine), installer upgrades from a real Node installation, the Git timeout, legacy v1 state through `deps add`, and `--intent-file -` at binary level. Some suites that used Node to onboard a project and then ran Rust on it (onboard rerun/rescan, status/intent list) now build the fixture with Rust, so Rust on a Node-made project is covered only by the routing fixtures.
- **Golden snapshots depend on the checkout's line endings.** With `core.autocrlf=true`, `core/methodology`, `harness/` and `rust-cli/completion.zsh` are CRLF in the working copy, and that reaches CLI output and recorded file hashes. Roughly ten routing/utility goldens (for example `help-and-completion`, `onboard`, `harness`, `init`, `knowledge-init` and both onboarding fixtures) were recorded on such a checkout and fail on an LF checkout until a `.gitattributes` rule fixes those paths' line endings and the goldens are re-blessed.
- **Node behaviours without a Rust test.** Found by keyword search when the Node suite was ported, so some may be covered indirectly by whole-output comparisons: dependency fast-forward of default branch and pinned refs, a ref-less dependency staying on the remote default branch, Git hooks/templates and fsmonitor config not executing during checkout, divergent updates and clone errors, `file:///` URLs and `../`/symlink paths in dependency names; walking each scope to completion with a gate per phase; work units at binary level (only the `red-tests` split-threshold warning and baseline path are covered, in `tests/repo_content.rs`); completing a `fast`-depth intent; oversized intent files ("larger than 100000 bytes"); Dart sources classifying as brownfield; `scopeSource` on new intents.
- **Packaging:** `cargo install` is the supported installation. `scripts/rust-install/install.{sh,ps1}` remain as installer scripts for a prebuilt binary (tested by `tests/rust_cargo_install.rs`), but they expect an `assets/` directory beside `target/release/` that this repository does not produce, and the binary does not read the `DTECH_AIDLC_ASSETS` variable their launchers set.

## Verification and release criteria

- Existing representative projects continue from their current state without manual edits, including legacy state fixtures and blocked/resumed workflows.
- Every supported command has success, validation and failure parity coverage; workflow and gate decisions match; errors never fabricate completion.
- Rust tests, formatting and lint pass; cross-platform installer and safety tests pass. Record any intentional behavior differences for owner approval before release.
- The production installer invokes Rust and works without Node after cutover. No production command silently falls back to Node.
