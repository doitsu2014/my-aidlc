# my-aidlc

This project uses **my-aidlc** for structured AI-assisted development on top
of the native AWS AI-DLC methodology: five phases, **Initialization,
Ideation, Inception, Construction, Operation**.

The methodology is fetched, not authored here. `my-aidlc update` snapshots it
from `awslabs/aidlc-workflows` at the version pinned in `aidlc-version.json`
into the engine's `{{HARNESS_DIR}}/native/` directory. Treat `native/` as
read-only: `my-aidlc doctor` fails when it drifts from the pin.

## Start a workflow

Describe what you want to build:

```text
{{INVOKE}} Build a REST API for inventory management
```

The workflow profile is detected from the request. You can also name one
explicitly: `classic`, `express`, `feature`, `bugfix`, `mvp`, `poc`, `infra`.

Profiles are the team's own. They live in the space at
`aidlc/spaces/<space>/scopes/<name>.md`, seeded from
`{{HARNESS_DIR}}/templates/scopes/` by `my-aidlc config` and never
overwritten. Each profile states its complete stage set in `include`, so edit
one to change which native stages that workflow runs.

## Commands

| Command | Purpose |
| --- | --- |
| `{{INVOKE}}` | Start or resume the active workflow |
| `{{INVOKE}} --status` | Show the active intent, scope, and stage |
| `{{INVOKE}} --doctor` | Validate the workspace and configuration |
| `{{INVOKE}} --version` | Print the framework version |

On the command line, the same operations are available as
`my-aidlc status`, `my-aidlc doctor`, and `my-aidlc orchestrate next`.

## Principles

- Every requirement traces to a source and has a testable acceptance criterion.
- Authoring is cheap; verification is the constraint.
- Humans own merges. Every agent diff is read before it ships.
- The compounding asset is your context, not the model.

## Where things live

- `aidlc/` - the workspace: memory, artifacts, state, and audit log
- `aidlc/spaces/<space>/scopes/` - the workflow profiles the team owns
- `{{HARNESS_DIR}}/` - the harness configuration and engine
- `{{HARNESS_DIR}}/native/` - the fetched AI-DLC methodology, read-only
