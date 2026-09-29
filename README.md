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

The installer adds the `my-aidlc` command. Node.js 20+ is required.

### 2. Configure a project

From the project root, select the harness you use:

```bash
cd /path/to/your-project
my-aidlc config --harness pi      # or: claude, codex
my-aidlc doctor
```

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

## Why my-aidlc

- **5 phases / 21 stages** from requirements through continuous curation
- **15 agents** — domain experts, reviewers, and an adaptive composer
- **7 workflow profiles** for features, bug fixes, MVPs, infrastructure,
  proof of concepts, express changes, and full lifecycle delivery
- **Human approval gates** at every stage
- **Database skills** for PostgreSQL, MySQL, SQL Server, and MongoDB
- **Audit trail** plus persistent project/team/org memory
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
