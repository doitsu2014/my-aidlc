# CLI reference

```
my-aidlc <command> [options]
```

## Human commands

### `my-aidlc config`

Configure the project for a harness.

| Flag | Meaning |
| --- | --- |
| `--harness <name>` | `pi`, `claude`, `codex`, or `deepseek` |
| `--mode <normal\|yolo>` | Execution mode: `normal` asks questions and presents gates; `yolo` auto-picks recommended answers and auto-approves gates |
| `--review-required <true\|false>` | Gate before leaving every phase, in either mode (default `false`) |
| `--questions-min <n>` | Minimum clarifying questions per stage |
| `--questions-max <n>` | Maximum clarifying questions per stage (`0` disables) |
| `--project <dir>` | Project root (default: current directory) |
| `--json` | Machine-readable output |

With no `--harness`, prints the current configuration. With `--harness`, it
writes the harness directory, the engine, the pinned native methodology, the
orchestrator skill, the memory files, and the space-scope seeds, then records
the harness in `aidlc/config.json`. Seeding never overwrites an existing file,
so re-running `config` is safe.

### `my-aidlc update [--check]`

Fetch the pinned AI-DLC release and rewrite `core/native/`. This is the only
command that writes the methodology, and it runs from a my-aidlc checkout (a
directory holding `aidlc-version.json`); from an installed project it reports
where the engine was built from instead.

| Flag | Meaning |
| --- | --- |
| `--check` | Verify the committed snapshot against `core/native/lock.json` with no network access; exit non-zero on any drift |
| `--json` | Machine-readable summary (pin, stage/agent/scope counts, drift) |

`--check` re-hashes every file in the snapshot, so it is the CI guard for
"nobody hand-edited `core/native/`". After `update`, re-project each project
with `my-aidlc config --harness <name>`.

### `my-aidlc init`

Create the `aidlc/` workspace, copy the memory templates, and seed the space
scopes. Runs automatically on the first workflow, so it is usually
unnecessary. Missing seeds are copied back; existing files are never
overwritten. Accepts `--project <dir>`.

### `my-aidlc doctor`

Validate the Node version, the native pin, the methodology, agent references,
the space scopes, memory files, harness configuration, and state consistency.
It warns when a space has no scopes, which means the project is running on the
native baseline. Exits non-zero on failure.

### `my-aidlc status`

Show the active intent, scope, current stage, and per-stage progress.
`--json` prints the full report.

### `my-aidlc list <phases|stages|scopes|agents>`

List methodology entries. `--json` for machine output.

### `my-aidlc stage <slug>` / `my-aidlc scope [<name>]` / `my-aidlc phase <slug>`

Show one entry in detail. `my-aidlc scope` with no name lists every workflow
profile, marking each one `space` (authored in your space) or `native` (a
baseline shipped by the pinned release); `my-aidlc scope <name>` prints that
profile's resolved stage list and the file it came from.

### `my-aidlc graph`

Print the compiled workflow graph as JSON, including the `native` pin block,
the phases, the stages, the scopes, and the `scopeGrid` (profile to ordered
stage slugs).

### `my-aidlc version` / `my-aidlc help`

Print the version or help.

### `my-aidlc completion`

Print or install shell completion for `bash`, `zsh`, `fish`, and
`powershell`. The vocabulary is generated from the live methodology, so new
stages, scopes, and agents appear automatically.

| Command | Meaning |
| --- | --- |
| `my-aidlc completion` | Show supported shells and the detected one |
| `my-aidlc completion <shell>` | Print the completion script to stdout |
| `my-aidlc completion install [--shell <shell>] [--dir <dir>] [--no-rc]` | Write the script and wire it into your shell rc file |
| `my-aidlc completion uninstall [--shell <shell>]` | Remove the script and its managed rc block |

`--shell` defaults to the shell detected from `$SHELL`; pass it explicitly
from scripts or on Windows. `--no-rc` writes only the script file and leaves
your shell configuration untouched. The rc integration is idempotent and
delimited by `# >>> my-aidlc completion >>>` / `# <<< my-aidlc completion <<<`
markers.

Install locations:

| Shell | Script | Activation |
| --- | --- | --- |
| bash | `$XDG_DATA_HOME/my-aidlc/completions/my-aidlc.bash` | sourced from `~/.bashrc` |
| zsh | `$XDG_DATA_HOME/my-aidlc/completions/_my-aidlc` | sourced from `~/.zshrc` |
| fish | `$XDG_CONFIG_HOME/fish/completions/my-aidlc.fish` | autoloaded |
| powershell | `$XDG_CONFIG_HOME/powershell/my-aidlc-completion.ps1` | dot-sourced from the PowerShell profile |

The only flag forms the dispatcher resolves before anything else are
`--version` and `--help`. Everything else is a command word, including
`my-aidlc status`, `my-aidlc doctor`, and `my-aidlc config`.

## Orchestration commands

These are what a harness conductor calls. They print one JSON directive.

### `my-aidlc orchestrate next [options] "<description>"`

| Flag | Meaning |
| --- | --- |
| `--new-intent` | Start a new intent even if one is active |
| `--scope <name>` | Force a workflow profile |
| `--resume` | Clear a park marker and continue |

With no active workflow and no description, returns an `error` directive.

### `my-aidlc orchestrate report --stage <slug> --result <outcome>`

| Result | Effect |
| --- | --- |
| `in-progress` | Mark the stage active |
| `awaiting-approval` | Mark the stage awaiting the human gate |
| `approved` | Complete the stage and advance |
| `completed` | Complete the stage without a human gate and advance |
| `rejected` | Keep the stage active and record the feedback |
| `revised` | Reopen the gate after a revision |
| `skipped` | Skip a conditional stage (requires `--reason`) |

Decide a phase review with `--phase <slug>` instead of `--stage <slug>`.
Optional: `--user-input "<text>"`, `--reason "<text>"`.

### `my-aidlc orchestrate park`

Park the workflow at the current inter-stage boundary.

## Directives

A directive is a JSON object with a `kind`:

| kind | Meaning |
| --- | --- |
| `run-stage` | Run the named stage, then report |
| `ask` | Present a question or gate, then follow `route` |
| `print` | Do what `message` says, print output, stop |
| `error` | Print `message` and stop |
| `done` | Workflow complete |
| `parked` | Workflow parked |

A `run-stage` directive carries `stage_file`, `lead_agent_file`, `produces`,
`produce_paths`, `optional_produces`, `optional_produce_paths`, `consumes`,
`consumes_absent`, `memory_path`, `protocol_modules`, `record_dir`,
`narration`, `execution_mode`, `auto_approve`, `review_required`,
`answer_policy`, `question_budget`, `workflow`, `support_agents`,
`support_agent_files`, `mode`, and `reviewer`.

`optional_produce_paths` names the artifacts a stage writes only when its
condition selects them — for example `functional-design` writes
`frontend-components.md` only for a `ui` unit.

An `ask` gate directive carries `question`, `options`, `route`, `produces`,
`produce_paths`, and `review_required`, so a review checkpoint hands the human
the artifacts to inspect. A phase review uses `ask_type: "phase-review"`,
`route: "report-phase"`, and lists `stages` plus their artifacts; decide it with
`orchestrate report --phase <phase> --result approved|rejected`.

## Aliases

Six top-level flags are shorthand for the command of the same name. They are
checked before the "no command" fallback, so `my-aidlc --status` and
`my-aidlc status` are the same invocation.

| Flag | Command |
| --- | --- |
| `--config` | `my-aidlc config` |
| `--update` | `my-aidlc update` |
| `--status` | `my-aidlc status` |
| `--doctor` | `my-aidlc doctor` |
| `--version` | `my-aidlc version` |
| `--help` | `my-aidlc help` |

Running `my-aidlc` with no arguments prints help. Any other unrecognised
command fails with `Unknown command "<name>"`.

## Shell completion

```bash
my-aidlc completion                 # list shells and usage
my-aidlc completion zsh             # print the script for a shell
my-aidlc completion install         # install for the detected shell
my-aidlc completion install --shell bash
my-aidlc completion status          # is the installed script current?
my-aidlc completion uninstall
```

`completion status` compares the installed script against a freshly generated
one and reports `up to date`, `STALE`, or `not installed`. If it is stale, run
`completion install` again, then reload your shell (`source` the script, or
`exec zsh`). A running shell keeps the function it loaded at startup; a new
completion is not visible until it reloads.

Completion covers the command surface from the live methodology: commands,
subcommands, stage/scope/phase/agent names, `--harness`, `--shell`, `--scope`,
`--stage`, `--phase`, `--result`, `--mode` (with its `normal`/`yolo` values),
and `--review-required` (with its `true`/`false` values).
