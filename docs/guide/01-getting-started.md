# Getting started

my-aidlc runs the native AWS AI-DLC methodology: five phases -
Initialization, Ideation, Inception, Construction, and Operation - over 33
stages, with a human in control at every stage.

The methodology is not authored in this repository. It is pinned to an AWS
AI-DLC release and fetched into an offline snapshot, while the workflow your
team actually runs lives in a space inside your project. Both are described
below.

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
my-aidlc config --harness pi      # or: claude, codex, deepseek
```

`config` writes the harness directory, the engine, the fetched native
methodology, the orchestrator skill, the workspace memory files, and the
space-scope seeds. Then validate:

```bash
my-aidlc doctor
```

`doctor` reports the native pin (repository, tag, and commit) and the space
scopes it found. It warns when your space has no scopes, which means the
project is running on the native baseline rather than on a workflow you own.

## 3. Start a workflow

Open your harness and describe the work.

- PI Agent: `/aidlc Build a REST API for inventory`
- Claude Code: `/aidlc Build a REST API for inventory`
- Codex CLI: `$aidlc Build a REST API for inventory`
- DeepSeek Harness: `/aidlc Build a REST API for inventory`

The workflow profile is detected from the request. my-aidlc then:

1. selects the first stage of the resolved profile,
2. asks the stage's questions,
3. writes the artifacts,
4. stops at an approval gate.

You approve, request changes, or skip. The next stage begins only after you
approve.

## 4. Watch progress

```bash
my-aidlc status          # active intent, scope, and stage
my-aidlc list stages     # every stage in the methodology
my-aidlc scope           # every profile, marked space or native
my-aidlc scope feature   # the resolved stages in one profile
```

## 5. Where your work lives

```
aidlc/
  config.json           # project configuration
  state.json            # active intent and stage progress
  audit.log             # append-only event log
  spaces/default/
    memory/             # org.md, team.md, project.md - your standing rules
    scopes/             # your workflow profiles, seeded from the engine
    intents/<id>/       # one directory per intent
      ideation/intent-capture/intent-statement.md
      inception/requirements-analysis/requirements.md
      ...
```

Memory files and space scopes are yours to edit. State and audit files are
tool-owned: never edit them by hand.

The methodology itself is not in `aidlc/`. It is projected with your harness,
under `<harness dir>/native/`, and it is generated. Do not hand-edit it.

## 6. The fetch and pin model

The engine pins exactly one AWS AI-DLC release in `aidlc-version.json`
(repository, tag, commit, asset URL, and archive SHA-256) and keeps a
committed snapshot of it under `core/native/`. From a my-aidlc checkout:

```bash
my-aidlc update          # fetch the pinned release and rewrite core/native/
my-aidlc update --check  # verify the snapshot against its lock, offline
```

`--check` makes no network call. It re-hashes the committed snapshot and fails
if a single file has drifted from `core/native/lock.json`, which makes it safe
for CI. `update` is the only writer of `core/native/`: never hand-edit that
tree. After updating, re-project each project with
`my-aidlc config --harness <name>`.

Your space scopes are never overwritten by `update`, but a new release can add,
remove, or rename stages. Re-check that each seed in `core/templates/scopes/`
still partitions the new stage inventory; `node scripts/lint.mjs` enforces
that, and `my-aidlc doctor` fails on any scope (space scopes included) that
names a stage which no longer exists.

## 7. Change your workflow

Your profiles are the files under `aidlc/spaces/<space>/scopes/`. They were
seeded from the engine on `config`/`init` and are never overwritten. Edit
`include` to state the complete stage list for a profile (it is authoritative)
and `skip` to name the rest:

```yaml
---
name: classic
include:
  - workspace-scaffold
  - workspace-detection
  - state-init
  # ... the rest of your stages
skip: []
---
```

A profile named after a native baseline scope replaces it; a new name adds a
new profile. See the [workflow guide](02-workflows.md) for the seeded profiles
and the full rules.

## Next steps

- Read the [workflow guide](02-workflows.md) to understand the phases and the
  space scopes you own.
- Read the [agents guide](03-agents.md) to learn who does what.
- Read the [harness guide](04-harnesses.md) for harness-specific setup.
