# my-aidlc

A local-first AIDLC CLI and methodology distribution for Node.js 20 and newer. The command-line entry point is `core/tools/my-aidlc.mjs`; the reusable methodology and harness integrations live under `core/` and `harness/`.

## Install from a local checkout

No installer network access is required. From the repository root:

```sh
sh scripts/install.sh
```

By default this installs under `~/.local`:

- program files: `~/.local/lib/my-aidlc/`
- launcher: `~/.local/bin/my-aidlc`

Add `~/.local/bin` to `PATH` if necessary. To choose another prefix or install from a separate local source tree:

```sh
sh scripts/install.sh --prefix "$HOME/.local" --from /path/to/my-aidlc
```

`MY_AIDLC_PREFIX` and `MY_AIDLC_SOURCE` can supply the corresponding defaults. The installer verifies that the local source contains the expected CLI, core, and harness. It does not download packages or contact external services. It refuses to replace an existing installation or launcher; remove the named path yourself before reinstalling. Installation copies only the runtime `core/`, `harness/`, and `package.json`, not repository metadata or tests.

The installer and generated launcher use POSIX `/bin/sh` syntax and are intended to work with common minimal shells such as `ash`. `ash` itself does not provide the standard programmable-completion interface used by Bash, Zsh, and similar shells; interactive tab completion is therefore not promised there.

## Run without installing

```sh
node core/tools/my-aidlc.mjs --help
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
my-aidlc config agents
```

After initializing a project, set or reset an agent preference:

```sh
my-aidlc config agents set builder-verifier opus
my-aidlc config agents reset builder-verifier
my-aidlc config agents --project /path/to/project
```

Supported aliases are `opus`, `sonnet`, `haiku`, and `fable`. Overrides are stored in `.aidlc/config.json` and leave the methodology graph and agent files unchanged. For example:

```json
{
  "harness": "claude",
  "scope": "solo",
  "initializedAt": "2026-01-01T00:00:00.000Z",
  "agentModels": {
    "builder-verifier": "opus"
  }
}
```

These aliases are runtime preferences, used only when a harness or delegated-work mechanism supports explicit model selection. They do not automatically switch the workflow conductor's model, guarantee a model is available, or imply that a particular environment (including DeepSeek) supports these aliases. `orchestrate next` and `scope NAME` expose the effective model and whether it comes from the default or project configuration. Human approval requirements remain unchanged.

## Safety and local state

The installer never removes an existing target and intentionally fails rather than overwriting files. Review local source code before installing it. The CLI may create or modify project-level workflow artifacts when you invoke its commands; inspect its help and the relevant methodology/harness guidance before running it in an important repository. Do not put credentials in methodology or generated configuration files.
