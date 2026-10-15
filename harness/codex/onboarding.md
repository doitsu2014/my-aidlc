# my-aidlc for Codex CLI

This project uses **my-aidlc**, an AI-Driven Development Life Cycle workflow
engine built on the native AWS AI-DLC methodology: five phases
**Initialization, Ideation, Inception, Construction, Operation**. The
methodology is fetched, not authored here. `my-aidlc update` snapshots it from
`awslabs/aidlc-workflows` at the version pinned in `aidlc-version.json` into
`{{HARNESS_DIR}}/native/`, and `my-aidlc doctor` fails when it drifts from the
pin. Treat `native/` as read-only.

The invocation in this harness is `{{INVOKE}}`.

## Start a workflow

Type `$aidlc` followed by what you want to build:

```text
$aidlc Build a REST API for inventory management
```

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

- `.agents/skills/aidlc/` - the orchestrator skill (Codex discovers it there)
- `{{HARNESS_DIR}}/native/` - the fetched AI-DLC methodology: 5 phases, 33
  stages, and 14 agents
- `{{HARNESS_DIR}}/templates/scopes/` - the space-scope profile seeds
- `{{HARNESS_DIR}}/protocols/` - stage, question, recovery, and learnings
  protocols
- `{{HARNESS_DIR}}/tools/` - the deterministic engine

## Trust

Codex runs project hooks and skills only in a trusted project. Approve the
project when Codex asks; if skills do not appear, trust the folder and restart
the session.
