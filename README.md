# dtech-aidlc

A local-first AIDLC CLI and methodology distribution for Node.js 20 and newer. The command-line entry point is `core/tools/dtech-aidlc.mjs`; the reusable methodology and harness integrations live under `core/` and `harness/`. Release notes are in [CHANGELOG.md](CHANGELOG.md).

## Install

macOS and Linux, from the repository root:

```sh
sh scripts/install.sh
```

Windows PowerShell (tested with PowerShell 7 `pwsh`; PowerShell 5.1 is supported but checked by hand):

```powershell
pwsh -File scripts/install.ps1
```

Options: `-From`, `-Prefix`, `-Upgrade`, `-Uninstall`. Defaults: `-From` from `DTECH_AIDLC_SOURCE` or the repository root; `-Prefix` from `DTECH_AIDLC_PREFIX` or `%LOCALAPPDATA%\dtech-aidlc`. The install writes `bin\dtech-aidlc.cmd` and `bin\dtech-aidlc` (LF, no BOM, for Git Bash). Layout: `<Prefix>\lib\dtech-aidlc\{core,harness,package.json}`.

Node.js 20 or newer is required. The POSIX installer uses `~/.local` and installs three paths:

- program files: `~/.local/lib/dtech-aidlc/`
- launcher: `~/.local/bin/dtech-aidlc`
- zsh completion: `~/.local/share/zsh/site-functions/_dtech-aidlc`

Add `~/.local/bin` to `PATH` if necessary (`dtech-aidlc doctor` warns when the command is not on `PATH`). To choose another prefix or install from a separate local source tree:

```sh
sh scripts/install.sh --prefix "$HOME/.local" --from /path/to/dtech-aidlc
```

The Windows installer uses `%LOCALAPPDATA%\dtech-aidlc` and writes `bin\dtech-aidlc.cmd`. No installer network access is required; it copies only the runtime `core/`, `harness/` and `package.json`. The installer and launcher use POSIX `/bin/sh` syntax. Add the completion directory to `fpath` before `compinit`; `dtech-aidlc completion zsh` prints the same function.

To run without installing:

```sh
node core/tools/dtech-aidlc.mjs --help
```

## Upgrade and uninstall

```sh
sh scripts/install.sh --upgrade
sh scripts/install.sh --uninstall
```

A plain `install.sh` refuses to replace an existing installation and names the paths it found. `--upgrade` replaces the library, launcher and completion (it also repairs a partial installation) and rolls back if a step fails. `--uninstall` removes only the three paths above, leaves other files in `bin/`, `lib/` and `share/`, and never touches a project. Files that are not recognisably from dtech-aidlc are never changed. The installed tree does not contain `install.sh`, so to remove an installation without a checkout, delete the three paths by hand.

Workflow state migrates by itself. In each onboarded project run `dtech-aidlc harness refresh` for the skill files and `dtech-aidlc onboard` for PROJECT.md, the knowledge manifest and the profile. See [CHANGELOG.md](CHANGELOG.md) for what changed in each release.

## Onboard a project

From the project directory, greenfield or brownfield:

```sh
dtech-aidlc onboard --harness claude --scope solo
```

`init` is an alias of `onboard`. `--harness` is required the first time; afterwards both `--harness` and `--scope` default to the configured values. Onboarding scans the project and classifies it as brownfield, greenfield or unknown. It registers the documentation it finds by reference in `aidlc.knowledge.json` (nothing is copied into `.aidlc`), and prints what it registered and a `registerAllCommand` for what it only proposes. It also creates `PROJECT.md` from a template when absent.

- `--docs <path>` registers a document or folder (repeatable); `--no-detect` skips name-based detection; `--rescan` re-scans an initialized project and reports documentation changes.
- Generated local files are written under `.aidlc/shared/` (the project profile and the source snapshot).
- On an existing project, the first intent writes a one-time codebase map at `.aidlc/knowledge/_codebase-map.md` during context-and-risk.

## Daily use

Start the workflow with the slash command of your harness (`/dtech-aidlc` for Claude Code, `/aidlc` for the DeepSeek harness). The agent runs `dtech-aidlc orchestrate next`, does the stage, and reports with `dtech-aidlc orchestrate report`.

- A stage cannot be reported complete until every file it produces exists at the path the directive lists under `artifacts`.
- Record an approval with `--approval approved --approved-by "<name>"` and a rejection with `--result blocked --approval rejected`. The approver name is self-asserted: it is a flag passed by whoever runs the CLI, so it is attributable but not unforgeable.
- After an intent completes, `orchestrate next --intent "<text>"` starts the next one. `dtech-aidlc intent list`, `intent pause`, `intent resume <id>` and `intent abandon` manage them; paused means: not the active intent of this checkout.
- `dtech-aidlc status` shows progress and `dtech-aidlc doctor` validates the graph, state, configuration, projection, knowledge and audit log.

## Project knowledge

`PROJECT.md` is written by people. It is created from a template only when absent and never overwritten.

`aidlc.knowledge.json` at the project root registers other sources by path. Files are read in place:

```json
{ "schemaVersion": 1, "sources": [ { "id": "arch", "path": "docs/architecture.md", "kind": "architecture", "description": "Optional" } ] }
```

The kinds are `project`, `docs`, `requirements`, `architecture`, `adr`, `api-spec`, `runbook`, `conventions` and `readme`; paths are relative to the project root. Commands:

```sh
dtech-aidlc knowledge init
dtech-aidlc knowledge sources
dtech-aidlc knowledge list
dtech-aidlc knowledge show <id>
dtech-aidlc knowledge index
dtech-aidlc knowledge check
```

The knowledge house `.aidlc/knowledge/` holds one record per closed intent, plus a generated `_index.md`. When sources disagree, the order of authority is: the request and the owner's decisions in this intent; the project file; a newer record; an older record; declared sources; the project profile.

## What to commit

Commit: `PROJECT.md`, `aidlc.knowledge.json`, `aidlc.deps.json`, the projected skill folders with their `.projection.json`, and under `.aidlc/`: `.gitignore`, `knowledge/` and `intents/`.

Local, not committed: `.aidlc/state.json`, `state.v1.bak.json`, `audit.jsonl`, `config.json`, `shared/`, `repos/` and `*.tmp`. The tool writes a managed block to `.aidlc/.gitignore` for this split and never edits the root `.gitignore`. Committed intent folders carry approver names and analysis content; a team that does not want intents in git adds one line below the managed block. A file named `<name>.local-backup` in a skill folder is a saved copy that should not be committed.

## Harnesses

```sh
dtech-aidlc harness status
dtech-aidlc harness refresh [NAME] [--dry-run] [--force]
dtech-aidlc harness add NAME
```

`harness status` compares the projected skill files with this engine version using the `.projection.json` stamp. `harness refresh` updates files you have not edited, keeps edited files (with `--force` it replaces them and saves your copy as `<name>.local-backup`), and removes files the engine no longer ships. `harness add` projects another harness (claude or deepseek) into the project.

## Default scope

`dtech-aidlc config scope` shows the default scope for new intents and `dtech-aidlc config scope team` changes it.

## Agent model preferences

Each methodology agent has a default model alias. List the current default and effective preference (including its source) with:

```sh
dtech-aidlc config agents
```

After initializing a project, set or reset an agent preference:

```sh
dtech-aidlc config agents set builder-verifier opus
dtech-aidlc config agents reset builder-verifier
dtech-aidlc config agents --project /path/to/project
```

Supported aliases are `opus`, `sonnet`, `haiku`, and `fable`. They are Anthropic model aliases used by the claude harness. A DeepSeek-only project gets no model field in the directive and `config agents set` is refused; in a project with both harnesses the DeepSeek skill ignores the field. Overrides are stored in `.aidlc/config.json` and leave the methodology graph and agent files unchanged. For example:

```json
{
  "harness": "claude",
  "harnesses": ["claude"],
  "scope": "solo",
  "initializedAt": "2026-01-01T00:00:00.000Z",
  "agentModels": {
    "builder-verifier": "opus"
  }
}
```

Aliases are runtime preferences. They do not switch the conductor's model or guarantee a model is available. `orchestrate next` and `scope NAME` expose the effective model and whether it comes from the default or project configuration. Human approval requirements remain unchanged.

## Dependency repositories

Declare repositories in `aidlc.deps.json` at the project root, then check them out under `.aidlc/repos/<name>`. `repos pull` reads that file when it exists, or the file given with `--file`. The runtime copy is `.aidlc/deps.json`. Checked-out commits are recorded in `.aidlc/deps-lock.json`.

```json
{
  "schemaVersion": 1,
  "repos": {
    "contracts": { "url": "https://example.com/contracts.git", "ref": "main" },
    "design": { "url": "https://example.com/design.git", "ref": "v1.2.0" }
  }
}
```

```sh
dtech-aidlc repos add contracts /absolute/path/to/contracts.git --ref main
dtech-aidlc repos add design https://example.com/design.git --ref v1.2.0
dtech-aidlc repos list
dtech-aidlc repos pull
dtech-aidlc repos pull contracts
dtech-aidlc repos remove design
```

`repos pull` clones a missing checkout or fast-forwards an existing one; a dependency registered without `--ref` follows the remote's default branch. It refuses dirty checkouts, non-fast-forward updates, and unmanaged paths. `repos remove` deletes only the manifest entry and leaves the checkout on disk. Repository contents are not executed. URLs may be `https://`, `ssh://`, `git@host:path`, or an absolute local path. Embedded passwords and tokens are rejected.

`deps` is a separate workflow-item graph and does not clone repositories.

## Environment variables

- `DTECH_AIDLC_PREFIX` and `DTECH_AIDLC_SOURCE` supply the defaults for `install.sh --prefix` and `--from`.
- `DTECH_AIDLC_HOME=<directory containing core/, harness/ and package.json>` makes the CLI read the graph, methodology, harness sources, and version from that directory. It must contain `core/data/phase-graph.json`. This setting applies per process, not per project, and is intended for testing a modified engine. Put project-specific rules in `PROJECT.md`, not in a copy of the methodology.

## Development commands

```sh
npm test
npm run lint
```

The test command uses Node's built-in test runner and loads tests from `tests/`. Lint checks package/runtime metadata, the CLI entry point, the methodology graph syntax and references, and that the methodology README's scope lists, agent roster, phase sections and gate ids agree with the graph. It is offline and does not certify that methodology prose is semantically correct. Run both commands before submitting changes.

## Repository layout

- `core/` — CLI, AIDLC methodology, stage graph, templates, and supporting runtime files.
- `harness/` — projections and setup material for supported coding harnesses.
- `scripts/` — local installation and repository lint helpers.
- `tests/` — automated Node.js tests.

The core methodology describes workflow scopes and stages; harness integrations adapt the workflow to their host tools. This package does not imply that every harness supports every feature equally.

## Safety and local state

`install.sh` changes only the three paths it names and never overwrites a file it did not install. Review local source code before installing it. The CLI may create or modify project-level workflow artifacts when you invoke its commands; inspect its help and the methodology and harness guidance before running it in an important repository. Do not put credentials in methodology or generated configuration files.
