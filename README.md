# my-aidlc

![version](https://img.shields.io/badge/version-0.1.0-blue)
![license](https://img.shields.io/badge/license-MIT--0-green)

my-aidlc turns AI coding assistants into a structured, verifiable
software-delivery workflow. One harness-neutral core runs natively in
**PI Agent**, **Codex CLI**, and **Claude Code**.

The methodology follows the five AIDLC phases — **Analyze**, **Ideate**,
**Develop**, **Launch**, **Curate** — as described in the
[AIDLC cheatsheet](https://aidlc.io/cheatsheet/).

```
Analyze  -> requirements & research      (Research assistant)
Ideate   -> architecture & design        (Architect partner)
Develop  -> code & review                (Coding agent)
Launch   -> tests, CI/CD & release       (QA engineer)
Curate   -> monitoring & improvement     (SRE assistant)
```

## Quick start

### 1. Install

macOS, Linux, or WSL (from a clone):

```bash
git clone https://github.com/doitsu2014/my-aidlc.git
cd my-aidlc
./scripts/install.sh --from .
```

Windows PowerShell:

```powershell
git clone https://github.com/doitsu2014/my-aidlc.git
cd my-aidlc
./scripts/install.ps1 -From .
```

The installer adds the `my-aidlc` command and shell completion for your
login shell. Node.js 20+ is required. To skip completion, pass
`--no-completion` (or `-NoCompletion` on Windows); manage it later with
`my-aidlc completion install|uninstall`.

### 2. Configure a project

From the project root, select the harness you use:

```bash
cd /path/to/your-project
my-aidlc config --harness pi      # or: claude, codex
my-aidlc doctor
```

This writes the harness directory, the engine, the methodology, the
orchestrator skill, and the workspace memory files. The execution mode and
question budget default to `normal` and `{ min: 0, max: 5 }`; see
[Configuration](#configuration) to change them.

### 3. Start a workflow

Open your harness in the configured project and describe the work:

```text
/aidlc Build a REST API for inventory management
```

Codex CLI uses `$aidlc`; PI Agent uses `/aidlc` or `/skill:aidlc`.
my-aidlc detects a workflow profile from the request, asks for missing
decisions, and stops at an approval gate before each stage is committed.

## Pick your harness

| Harness | Configure | Open | Invoke |
| --- | --- | --- | --- |
| PI Agent | `my-aidlc config --harness pi` | `pi` | `/aidlc` |
| Codex CLI | `my-aidlc config --harness codex` | `codex` | `$aidlc` |
| Claude Code | `my-aidlc config --harness claude` | `claude` | `/aidlc` |

## Configuration

Project configuration is layered, most specific wins. Show the current values
with `my-aidlc config` (or `my-aidlc config --json`).

| File | Purpose | Committed |
| --- | --- | --- |
| `aidlc/config.json` | Shared project settings | yes |
| `aidlc/config.local.json` | Per-developer overrides (wins over `config.json`) | ignored |

`aidlc/config.json` stores the settings you change; unset keys fall back to
defaults. A project configured for PI Agent with a tighter question budget:

```json
{
  "harness": "pi",
  "version": "0.1.0",
  "mode": "normal",
  "questionBudget": { "min": 1, "max": 3 }
}
```

The optional `defaultScope` key sets the profile used when a request matches no
keyword (default `classic`).

### Harness

```bash
my-aidlc config --harness pi      # pi | claude | codex
```

`config` scaffolds the harness directory, the engine and methodology, and the
`aidlc/` workspace. Re-run it after switching harnesses or upgrading.

### Execution mode

| Mode | Questions | Approval gates | Use for |
| --- | --- | --- | --- |
| `normal` (default) | Asked, bounded by the question budget | Presented to the human | Anything that ships |
| `yolo` | Skipped; the recommended answer is chosen | Auto-satisfied | Demos, POCs, trusted automation |

```bash
my-aidlc config --mode yolo       # auto-pick answers and auto-approve gates
my-aidlc config --mode normal     # back to human questions and gates
```

YOLO does **not** skip any stage: every stage still runs and still writes its
artifacts. It removes the human in the loop, not the work. Every auto-approval
is recorded as `STAGE_AUTO_APPROVED` in `aidlc/audit.log`, so an unattended run
stays auditable. To keep a human gate in an otherwise unattended run, mark a
stage or phase `review: required`; it always stops at its approval gate.

### Phase review

Stop and review after every phase with one project-wide toggle:

```bash
my-aidlc config --review-required true    # gate before leaving each phase
my-aidlc config --review-required false   # back to stage-only gates (default)
```

It applies in both normal and YOLO mode and is independent of the execution
mode. Individual phases can still opt in with `review: required` in
`core/phases/<phase>/phase.md`, and a scope can override the toggle with
`review_required:` in its frontmatter.

### Question budget

Cap how many clarifying questions each stage asks before its gate:

```bash
my-aidlc config --questions-min 1 --questions-max 3
my-aidlc config --questions-max 0     # no questions; generate artifacts directly
```

Precedence, most specific first: **stage** `question_budget` → **scope**
`question_budget` → project `questionBudget` → default `{ min: 0, max: 5 }`.
`max: 0` disables the question flow for a stage. The budget never applies to
the approval gate, which is always exactly one decision.

### Per-scope overrides

A workflow profile can pin its own mode and budget in frontmatter, so a
throwaway profile can run unattended while the project stays `normal`:

```yaml
# core/scopes/poc.md
mode: yolo
question_budget:
  min: 1
  max: 3
```

### Workspace layout

```text
aidlc/
  config.json            # project configuration
  config.local.json      # per-developer overrides (gitignored)
  state.json             # active intent and stage progress (tool-owned)
  audit.log              # append-only event log (tool-owned)
  spaces/default/
    memory/              # org.md, team.md, project.md - your standing rules
    intents/<id>/        # one directory per intent; artifacts per phase/stage
```

`state.json` and `audit.log` are tool-owned: never edit them by hand. The
memory files are yours to edit; they are the method every stage reads.

### Inspecting and validating

```bash
my-aidlc config           # show harness, mode, and question budget
my-aidlc status           # active intent, mode, and per-stage progress
my-aidlc doctor           # validate the engine, workspace, and configuration
```

## Why my-aidlc

- **5 phases / 21 stages** from requirements through continuous curation
- **15 agents** — domain experts, reviewers, and an adaptive composer
- **7 workflow profiles** for features, bug fixes, MVPs, infrastructure,
  proof of concepts, express changes, and full lifecycle delivery
- **Human approval gates** at every stage (Normal mode), or an optional
  **YOLO mode** that auto-picks recommended answers and auto-approves gates
  while recording every auto-approval in the audit log
- **Review checkpoints** — mark a stage, or a whole phase, `review: required`
  to keep a human gate even under YOLO
- **Database skills** for PostgreSQL, MySQL, SQL Server, and MongoDB
- **Audit trail** plus persistent project/team/org memory
- **Shell completion** for bash, zsh, fish, and PowerShell
- **One deterministic engine** across every supported harness

## Repository layout

- `core/` — hand-authored, harness-neutral methodology and engine
  - `core/agents/` — 15 agent definitions
  - `core/phases/` — the five phases and their stage files
  - `core/scopes/` — workflow profiles
  - `core/protocols/` — stage, question, gate, and recovery protocols
  - `core/skills/` — the orchestrator skill and database skills
  - `core/memory/` — org/team/project memory templates
  - `core/tools/` — the Node.js engine and authoring tools
  - `core/data/` — compiled stage graph and scope grid
- `harness/<name>/` — thin, harness-specific manifests and integrations
- `scripts/` — packaging, installer, and release tooling
- `tests/` — smoke, unit, and integration tests
- `docs/` — user and developer documentation

Edit `core/` or `harness/<name>/`, never generated `dist*/` output.

## Development

```bash
node scripts/package.mjs                 # generate every harness into dist/
node scripts/package.mjs --check         # determinism guard
node tests/run-tests.mjs                 # smoke + unit + integration
```

## Harnesses

- **PI Agent** — project config in `.pi/`, skills in `.pi/skills/`,
  prompt template `.pi/prompts/aidlc.md`, context `AGENTS.md`.
- **Codex CLI** — project config in `.codex/`, skills in `.agents/skills/`,
  context `AGENTS.md`, invoke with `$aidlc`.
- **Claude Code** — project config in `.claude/`, skills in
  `.claude/skills/aidlc/`, ambient method import via `.claude/rules/aidlc.md`.

## References

- [AIDLC cheatsheet](https://aidlc.io/cheatsheet/)
- [AWS AI-DLC blog post](https://aws.amazon.com/blogs/devops/ai-driven-development-life-cycle/)
- [MIT No Attribution license](LICENSE)
