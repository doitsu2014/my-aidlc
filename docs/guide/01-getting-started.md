# Getting started

my-aidlc structures AI-assisted software delivery into five phases — Analyze,
Ideate, Develop, Launch, Curate — and keeps a human in control at every stage.

## 1. Install

Node.js 20 or newer is required.

From a clone:

```bash
git clone https://github.com/doitsu2014/my-aidlc.git
cd my-aidlc
./scripts/install.sh --from .
```

This installs the runtime under `~/.local/share/my-aidlc` and writes the
`my-aidlc` command to `~/.local/bin`. Follow the printed PATH instruction if
needed.

Windows PowerShell:

```powershell
./scripts/install.ps1 -From .
```

## 2. Configure a project

From the project root, choose your harness:

```bash
cd /path/to/your-project
my-aidlc config --harness pi      # or: claude, codex
```

`config` writes the harness directory, the engine, the agent and phase
methodology, the orchestrator skill, and the workspace memory files. Then
validate:

```bash
my-aidlc doctor
```

## 3. Start a workflow

Open your harness and describe the work.

- PI Agent: `/aidlc Build a REST API for inventory`
- Claude Code: `/aidlc Build a REST API for inventory`
- Codex CLI: `$aidlc Build a REST API for inventory`

The workflow profile is detected from the request. my-aidlc then:

1. selects the first stage,
2. asks the stage's questions,
3. writes the artifacts,
4. stops at an approval gate.

You approve, request changes, or skip. The next stage begins only after you
approve.

## 4. Watch progress

```bash
my-aidlc status          # active intent, scope, and stage
my-aidlc list stages     # every stage in the methodology
my-aidlc scope feature   # the stages in a profile
```

## 5. Where your work lives

```
aidlc/
  config.json           # project configuration
  state.json            # active intent and stage progress
  audit.log             # append-only event log
  spaces/default/
    memory/             # org.md, team.md, project.md — your standing rules
    intents/<id>/       # one directory per intent
      analyze/intent-capture/intent-statement.md
      ideate/architecture-design/architecture-doc.md
      ...
```

Memory files are yours to edit. State and audit files are tool-owned: never
edit them by hand.

## Next steps

- Read the [workflow guide](02-workflows.md) to understand the phases.
- Read the [agents guide](03-agents.md) to learn who does what.
- Read the [harness guide](04-harnesses.md) for harness-specific setup.
