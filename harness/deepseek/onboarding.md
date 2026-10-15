# my-aidlc for DeepSeek Harness

This project uses **my-aidlc**, an AI-Driven Development Life Cycle workflow
engine built on the native AWS AI-DLC methodology: five phases
**Initialization, Ideation, Inception, Construction, Operation**. The
methodology is fetched, not authored here. `my-aidlc update` snapshots it from
`awslabs/aidlc-workflows` at the version pinned in `aidlc-version.json` into
`{{HARNESS_DIR}}/native/`, and `my-aidlc doctor` fails when it drifts from the
pin. Treat `native/` as read-only.

The invocation in this harness is `{{INVOKE}}`.

## Start a workflow

Type `/aidlc` in the composer, followed by what you want to build:

```text
/aidlc Build a REST API for inventory management
```

Typing `/` opens the command menu, where the aidlc skill is listed with its
description. The same invocation works from the Web GUI, the TUI, and ACP.

The workflow profile is detected from the request. You can name one explicitly:
`classic`, `express`, `feature`, `bugfix`, `mvp`, `poc`, `infra`.

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

On the command line the same operations are available as
`my-aidlc status`, `my-aidlc doctor`, and `my-aidlc orchestrate next`.

## What was installed

- `{{HARNESS_DIR}}/skills/aidlc/` - the orchestrator skill, loaded on demand
- `{{HARNESS_DIR}}/skills/db-*/` - the four database reference skills
- `{{HARNESS_DIR}}/native/` - the fetched AI-DLC methodology: 5 phases, 33
  stages, and 14 agents
- `{{HARNESS_DIR}}/templates/scopes/` - the space-scope profile seeds
- `{{HARNESS_DIR}}/protocols/` - stage, question, recovery, and learnings
  protocols
- `{{HARNESS_DIR}}/tools/` - the deterministic engine

`{{HARNESS_DIR}}/` is the `project-dsh` skill root (rank 100). Only the top
level of a root is scanned, so the aidlc bundle lives directly at
`{{HARNESS_DIR}}/skills/aidlc/SKILL.md`.

## Workspace

`aidlc/` holds the workspace: memory (`spaces/default/memory/`), intent
artifacts (`spaces/default/intents/`), `state.json`, and `audit.log`.

## Principles

- Every requirement traces to a source and has a testable acceptance criterion.
- Authoring is cheap; verification is the constraint.
- Humans own merges. Every agent diff is read before it ships.
- The compounding asset is your context, not the model.
