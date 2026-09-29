# my-aidlc

This project uses **my-aidlc** for structured AI-assisted development across
five phases: **Analyze → Ideate → Develop → Launch → Curate**.

## Start a workflow

Describe what you want to build:

```text
{{INVOKE}} Build a REST API for inventory management
```

The workflow profile is detected from the request. You can also name one
explicitly: `classic`, `express`, `feature`, `bugfix`, `mvp`, `poc`, `infra`.

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

- `aidlc/` — the workspace: memory, artifacts, state, and audit log
- `{{HARNESS_DIR}}/` — the harness configuration and engine
