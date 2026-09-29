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
`protocol_modules`, `record_dir`, `narration`, and `workflow`.
