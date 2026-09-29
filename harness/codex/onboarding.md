# my-aidlc for Codex CLI

This project uses **my-aidlc**, an AI-Driven Development Life Cycle workflow
engine built around five phases: **Analyze → Ideate → Develop → Launch →
Curate**.

## Start a workflow

Type `$aidlc` followed by what you want to build:

```text
$aidlc Build a REST API for inventory management
```

The workflow profile is detected from the request. You can name one explicitly:
`classic`, `express`, `feature`, `bugfix`, `mvp`, `poc`, `infra`.

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

- `.agents/skills/aidlc/` — the orchestrator skill (Codex discovers it there)
- `{{HARNESS_DIR}}/agents/` — the 15 agent personas
- `{{HARNESS_DIR}}/phases/` — the five phases and their stages
- `{{HARNESS_DIR}}/scopes/` — the workflow profiles
- `{{HARNESS_DIR}}/protocols/` — stage, question, recovery, and learnings protocols
- `{{HARNESS_DIR}}/tools/` — the deterministic engine

## Trust

Codex runs project hooks and skills only in a trusted project. Approve the
project when Codex asks; if skills do not appear, trust the folder and restart
the session.
