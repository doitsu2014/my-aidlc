# CLI reference

```
my-aidlc <command> [options]
```

## Human commands

### `my-aidlc config`

Configure the project for a harness.

| Flag | Meaning |
| --- | --- |
| `--harness <name>` | `pi`, `claude`, or `codex` |
| `--mode <normal\|yolo>` | Execution mode: `normal` asks questions and presents gates; `yolo` auto-picks recommended answers and auto-approves gates |
| `--review-required <true\|false>` | Gate before leaving every phase, in either mode (default `false`) |
| `--questions-min <n>` | Minimum clarifying questions per stage |
| `--questions-max <n>` | Maximum clarifying questions per stage (`0` disables) |
| `--project <dir>` | Project root (default: current directory) |
| `--json` | Machine-readable output |

With no `--harness`, prints the current configuration.

### `my-aidlc init`

Create the `aidlc/` workspace and copy the memory templates. Runs automatically
on the first workflow, so it is usually unnecessary.

### `my-aidlc doctor`

Validate Node version, methodology integrity, agent references, memory files,
harness configuration, and state consistency. Exits non-zero on failure.

### `my-aidlc status`

Show the active intent, scope, current stage, and per-stage progress.
`--json` prints the full report.

### `my-aidlc list <phases|stages|scopes|agents>`

List methodology entries. `--json` for machine output.

### `my-aidlc stage <slug>` / `my-aidlc scope <name>` / `my-aidlc phase <slug>`

Show one entry in detail.

### `my-aidlc graph`

Print the compiled workflow graph (phases, stages, scopes, and the scope grid)
as JSON.

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

Aliases: `--status`, `--doctor`, `--version`, `--help`, `--config`.

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
`produce_paths`, `consumes`, `consumes_absent`, `memory_path`,
`protocol_modules`, `record_dir`, `narration`, `execution_mode`,
`auto_approve`, `review_required`, `answer_policy`, `question_budget`, and
`workflow`.

An `ask` gate directive carries `question`, `options`, `route`, `produces`,
`produce_paths`, and `review_required`, so a review checkpoint hands the human
the artifacts to inspect. A phase review uses `ask_type: "phase-review"`,
`route: "report-phase"`, and lists `stages` plus their artifacts; decide it with
`orchestrate report --phase <phase> --result approved|rejected`.

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
