# my-aidlc

![version](https://img.shields.io/badge/version-0.1.0-blue)
![license](https://img.shields.io/badge/license-MIT--0-green)

my-aidlc turns AI coding assistants into a structured, verifiable
software-delivery workflow. One harness-neutral core runs natively in
**PI Agent**, **Codex CLI**, **Claude Code**, and **DeepSeek Harness**.

The methodology is the **native AI-DLC from AWS**
([`awslabs/aidlc-workflows`](https://github.com/awslabs/aidlc-workflows)) —
five phases, 33 stages, 14 agents. my-aidlc does not re-author it: it pins one
release in `aidlc-version.json`, verifies it, and projects it into your harness.

```
Initialization -> intent + record dir        (Workspace orchestrator)
Ideation       -> intent, scope, approval    (Product partner)
Inception      -> requirements & design      (Architect partner)
Construction   -> code, build & test         (Coding agent)
Operation      -> deploy, observe, improve   (Operations assistant)
```

What my-aidlc adds is the **space layer**: your workflow profiles
(`classic`, `express`, `feature`, `bugfix`, `mvp`, `poc`, `infra`) live in the
workspace at `aidlc/spaces/<space>/scopes/`, where you can edit them, and they
decide which of the 33 native stages each profile runs.

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
my-aidlc config --harness pi      # or: claude, codex, deepseek
my-aidlc doctor
```

This projects the pinned native AI-DLC engine and methodology into the harness
directory, and seeds your workspace: memory files plus the seven workflow
profiles at `aidlc/spaces/default/scopes/`. The execution mode and question
budget default to `normal` and `{ min: 0, max: 5 }`; see
[Configuration](#configuration) to change them.

### 3. Start a workflow

Open your harness in the configured project and describe the work:

```text
/aidlc Build a REST API for inventory management
```

Codex CLI uses `$aidlc`; PI Agent, Claude Code, and DeepSeek Harness use
`/aidlc`. my-aidlc detects a workflow profile from the request, asks for
missing decisions, and stops at an approval gate before each stage is
committed.

## Pick your harness

| Harness | Configure | Open | Invoke |
| --- | --- | --- | --- |
| PI Agent | `my-aidlc config --harness pi` | `pi` | `/aidlc` |
| Codex CLI | `my-aidlc config --harness codex` | `codex` | `$aidlc` |
| Claude Code | `my-aidlc config --harness claude` | `claude` | `/aidlc` |
| DeepSeek Harness | `my-aidlc config --harness deepseek` | DeepSeek Harness | `/aidlc` |

## The methodology comes from AWS

The five phases, 33 stages, and 14 agents are native AI-DLC. `my-aidlc update`
downloads the release pinned in [`aidlc-version.json`](aidlc-version.json),
verifies its SHA-256, and rewrites the engine's `native/` tree byte-for-byte.
Stage bodies keep the upstream `{{INVOKE}}` and `{{HARNESS_DIR}}` placeholders,
so the packager binds them per harness.

```bash
my-aidlc update           # fetch the pinned release and refresh the methodology
my-aidlc update --check   # verify the snapshot still matches the pin
node scripts/fetch-native.mjs --check
```

The committed snapshot exists so builds, lint, and tests work offline; it is
derived, never hand-edited, and `doctor` fails when it drifts from the pin.
`native/ADAPTATION.md` records how the native runtime's engine calls map onto
my-aidlc's binary, and which of its helpers are reference-only.

## Your workflow profiles live in the space

A **space** is one team's world: memory, and the workflow profiles it runs.
Editing a profile is how a team changes what a workflow does.

```text
aidlc/spaces/default/
  memory/            org.md, team.md, project.md - your standing rules
  scopes/            your workflow profiles (seeded, never overwritten)
  intents/<id>/      one directory per intent; artifacts per phase/stage
```

A profile states its complete stage list in `include` and the remainder in
`skip`, so it is the authority on its own workflow:

```yaml
# aidlc/spaces/default/scopes/poc.md
name: poc
include:
  - workspace-scaffold
  - workspace-detection
  - state-init
  - intent-capture
  - feasibility
  - requirements-analysis
  - code-generation
skip: [ ... the other 26 native stages ... ]
```

| Profile | Stages | Shape |
| --- | --- | --- |
| `classic` | 33 | Full lifecycle through all five phases (default) |
| `feature` | 32 | Everything except continuous optimisation |
| `infra` | 27 | Design, deploy, and operate infrastructure |
| `mvp` | 16 | Thin design pass, no Operation |
| `bugfix` | 10 | Reproduce, fix, build, ship |
| `express` | 10 | Requirements to deploy, no design pass, no reviewers |
| `poc` | 7 | Frame the assumption, prove it, stop |

The native baseline scopes (`enterprise`, `refactor`, `security-patch`,
`workshop`) stay available underneath; a space profile with the same name
replaces the baseline. `my-aidlc scope` marks each profile `space` or `native`.

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
my-aidlc config --harness pi      # pi | claude | codex | deepseek
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
mode. Individual phases can still opt in with `review: required` in the
engine's phase metadata (`<harness dir>/native/phases/<phase>.md`), and a
profile can override the toggle with `review_required:` in its frontmatter.

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

### Per-profile overrides

A workflow profile can pin its own mode and budget in frontmatter, so a
throwaway profile can run unattended while the project stays `normal`:

```yaml
# aidlc/spaces/default/scopes/poc.md
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
    scopes/              # your workflow profiles (seeded, yours to edit)
    intents/<id>/        # one directory per intent; artifacts per phase/stage
```

`state.json` and `audit.log` are tool-owned: never edit them by hand. The
memory files and the space scopes are yours to edit; they are the method every
stage reads.

### Inspecting and validating

```bash
my-aidlc config           # show harness, mode, and question budget
my-aidlc status           # active intent, mode, and per-stage progress
my-aidlc scope            # list workflow profiles, marked space or native
my-aidlc doctor           # validate the engine, native snapshot, workspace, config
my-aidlc update --check   # confirm the methodology still matches the AWS pin
```

## Why my-aidlc

- **Native AWS AI-DLC methodology** — 5 phases, 33 stages, 14 agents, pinned
  and checksum-verified; never a hand-maintained fork
- **Your workflow in your space** — the seven shipped profiles live in
  `aidlc/spaces/<space>/scopes/`, where a team edits which stages run
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

- `core/` — harness-neutral engine plus the pinned methodology
  - `core/native/` — **generated** from the AWS AI-DLC release; do not edit
    - `stages/<phase>/` — the 33 native stage definitions, byte-for-byte
    - `agents/` — the 14 native agent personas
    - `scopes/` — the 11 native baseline workflow scopes
    - `protocols/`, `phases/`, `lock.json`, `ADAPTATION.md`
  - `core/templates/scopes/` — the seven workflow profiles seeded into a space
  - `core/protocols/` — stage, question, gate, and recovery protocols
  - `core/skills/` — the orchestrator skill and database skills
  - `core/memory/` — org/team/project memory templates
  - `core/tools/` — the Node.js engine (`lib/native.mjs` owns the pin)
  - `core/data/` — compiled stage graph and phase metadata
- `harness/<name>/` — thin, harness-specific manifests and integrations
- `scripts/fetch-native.mjs` — fetch, verify, and normalise the pinned release
- `scripts/` — packaging, installer, and release tooling
- `tests/` — smoke, unit, and integration tests
- `docs/` — user and developer documentation

Edit `core/` (except `core/native/`) or `harness/<name>/`, never generated
`dist*/` output.

## Development

```bash
node scripts/fetch-native.mjs --check    # snapshot matches the pin
node scripts/package.mjs                 # generate every harness into dist/
node scripts/package.mjs --check         # determinism guard
node scripts/lint.mjs                    # methodology integrity
node tests/run-tests.mjs                 # smoke + unit + integration
```

## Harnesses

- **PI Agent** — project config in `.pi/`, skills in `.pi/skills/`,
  prompt template `.pi/prompts/aidlc.md`, context `AGENTS.md`.
- **Codex CLI** — project config in `.codex/`, skills in `.agents/skills/`,
  context `AGENTS.md`, invoke with `$aidlc`.
- **Claude Code** — project config in `.claude/`, skills in
  `.claude/skills/aidlc/`, ambient method import via `.claude/rules/aidlc.md`.
- **DeepSeek Harness** — project skills in `.dsh/skills/aidlc/` (the
  `project-dsh` root, rank 100), context `AGENTS.md`, invoke with `/aidlc`.
  DSH config is home-scoped (`$DSH_HOME/profiles/<name>/cordis.patch.yml`), so
  no project-level native config file is shipped.

## References

- [awslabs/aidlc-workflows](https://github.com/awslabs/aidlc-workflows) — the native AI-DLC this pins
- [AWS AI-DLC blog post](https://aws.amazon.com/blogs/devops/ai-driven-development-life-cycle/)
- [AIDLC cheatsheet](https://aidlc.io/cheatsheet/)
- [MIT No Attribution license](LICENSE)
