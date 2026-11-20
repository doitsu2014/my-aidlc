# dtech-aidlc

A local-first AIDLC CLI and methodology distribution for Node.js 20 and newer. The command-line entry point is `core/tools/dtech-aidlc.mjs`; the reusable methodology and harness integrations live under `core/` and `harness/`.

## Install from a local checkout

macOS and Linux:

```sh
sh scripts/install.sh
```

Windows PowerShell:

```powershell
powershell -File scripts/install.ps1
```

Node.js 20 or newer is required on all three. The POSIX installer uses `~/.local`. The Windows installer uses `%LOCALAPPDATA%\dtech-aidlc` and writes `bin\dtech-aidlc.cmd`.

## Onboard a project

From the project directory, greenfield or brownfield:

```sh
dtech-aidlc onboard --harness claude --scope solo
```

This initializes the project and writes `.aidlc/shared/project-profile.json`. It records detected languages, architecture evidence, whether the project already contains source, and copies a found BRD into `.aidlc/shared/brd/`. Detection reports only evidence it found.

No installer network access is required. From the repository root:

```sh
sh scripts/install.sh
```

By default this installs under `~/.local`:

- program files: `~/.local/lib/dtech-aidlc/`
- launcher: `~/.local/bin/dtech-aidlc`

Add `~/.local/bin` to `PATH` if necessary. To choose another prefix or install from a separate local source tree:

```sh
sh scripts/install.sh --prefix "$HOME/.local" --from /path/to/dtech-aidlc
```

`DTECH_AIDLC_PREFIX` and `DTECH_AIDLC_SOURCE` can supply the corresponding defaults. The installer verifies that the local source contains the expected CLI, core, and harness. It does not download packages or contact external services. It refuses to replace an existing installation or launcher; remove the named path yourself before reinstalling. Installation copies only the runtime `core/`, `harness/`, and `package.json`, not repository metadata or tests.

The installer and generated launcher use POSIX `/bin/sh` syntax. Zsh completion is installed at `<prefix>/share/zsh/site-functions/_dtech-aidlc`. Add that directory to `fpath` before `compinit`. `dtech-aidlc completion zsh` prints the same function. `ash` does not provide this completion interface.

## Environment variables

`DTECH_AIDLC_HOME=<directory containing core/, harness/ and package.json>` makes the CLI read the graph, methodology, harness sources, and version from that directory. It must contain `core/data/phase-graph.json`. This setting applies per process, not per project, and is intended for testing a modified engine. Put project-specific rules in `PROJECT.md`, not in a copy of the methodology.

## Run without installing

```sh
node core/tools/dtech-aidlc.mjs --help
```

For local development, the package manager dependencies are intentionally empty. Node 20+ is the only runtime requirement.

## Development commands

```sh
npm test
npm run lint
```

The test command uses Node's built-in test runner and loads tests from `tests/`. Lint checks package/runtime metadata, the CLI entry point, methodology graph syntax and references when the relevant snapshot files exist. Run both commands before submitting changes. The methodology lint is deliberately structural and offline; it does not certify that methodology prose is semantically correct.

## Repository layout

- `core/` — CLI, AIDLC methodology, stage graph, templates, and supporting runtime files.
- `harness/` — projections and setup material for supported coding harnesses.
- `scripts/` — local installation and repository lint helpers.
- `tests/` — automated Node.js tests.

The core methodology describes workflow scopes and stages; harness integrations adapt the workflow to their host tools. Consult the relevant files under `core/` and `harness/` for scope-specific behavior and integration details. This package does not imply that every harness supports every feature equally.

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

Supported aliases are `opus`, `sonnet`, `haiku`, and `fable`. Overrides are stored in `.aidlc/config.json` and leave the methodology graph and agent files unchanged. For example:

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

These aliases are runtime preferences, used only when a harness or delegated-work mechanism supports explicit model selection. They do not automatically switch the workflow conductor's model, guarantee a model is available, or imply that a particular environment (including DeepSeek) supports these aliases. `orchestrate next` and `scope NAME` expose the effective model and whether it comes from the default or project configuration. Human approval requirements remain unchanged.

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

`repos pull` clones a missing checkout or fast-forwards an existing one. It refuses dirty checkouts, non-fast-forward updates, and unmanaged paths. `repos remove` deletes only the manifest entry and leaves the checkout on disk. Repository contents are not executed. URLs may be `https://`, `ssh://`, `git@host:path`, or an absolute local path. Embedded passwords and tokens are rejected.

`deps` is a separate workflow-item graph and does not clone repositories.

## Safety and local state

The installer never removes an existing target and intentionally fails rather than overwriting files. Review local source code before installing it. The CLI may create or modify project-level workflow artifacts when you invoke its commands; inspect its help and the relevant methodology/harness guidance before running it in an important repository. Do not put credentials in methodology or generated configuration files.
