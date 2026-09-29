# my-aidlc for PI Agent

This project uses **my-aidlc**, an AI-Driven Development Life Cycle workflow
engine built around five phases: **Analyze → Ideate → Develop → Launch →
Curate**.

## Start a workflow

Type `/aidlc` followed by what you want to build:

```text
/aidlc Build a REST API for inventory management
```

The workflow profile is detected from the request. You can name one explicitly:
`classic`, `express`, `feature`, `bugfix`, `mvp`, `poc`, `infra`.

Force the skill when needed: `/skill:aidlc`.

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

- `{{HARNESS_DIR}}/skills/aidlc/` — the orchestrator skill (PI loads it on demand)
- `{{HARNESS_DIR}}/prompts/aidlc.md` — the `/aidlc` prompt template
- `{{HARNESS_DIR}}/agents/` — the 15 agent personas
- `{{HARNESS_DIR}}/phases/` — the five phases and their stages
- `{{HARNESS_DIR}}/scopes/` — the workflow profiles
- `{{HARNESS_DIR}}/protocols/` — stage, question, recovery, and learnings protocols
- `{{HARNESS_DIR}}/tools/` — the deterministic engine

## Workspace

`aidlc/` holds the workspace: memory (`spaces/default/memory/`), intent
artifacts (`spaces/default/intents/`), `state.json`, and `audit.log`.

## Principles

- Every requirement traces to a source and has a testable acceptance criterion.
- Authoring is cheap; verification is the constraint.
- Humans own merges. Every agent diff is read before it ships.
- The compounding asset is your context, not the model.
