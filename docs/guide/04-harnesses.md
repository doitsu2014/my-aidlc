# Harnesses

my-aidlc is one harness-neutral core with thin per-harness projections. The
methodology, agents, stages, scopes, and engine are identical on every harness;
only discovery paths, invocation, and settings differ.

| Harness | Configure | Invoke | Harness dir | Skill location | Context file |
| --- | --- | --- | --- | --- | --- |
| PI Agent | `my-aidlc config --harness pi` | `/aidlc` | `.pi/` | `.pi/skills/aidlc/` | `AGENTS.md` |
| Codex CLI | `my-aidlc config --harness codex` | `$aidlc` | `.codex/` | `.agents/skills/aidlc/` | `AGENTS.md` |
| Claude Code | `my-aidlc config --harness claude` | `/aidlc` | `.claude/` | `.claude/skills/aidlc/` | `CLAUDE.md` |

## PI Agent

Project config lives in `.pi/`. PI discovers project skills under
`.pi/skills/` and prompt templates under `.pi/prompts/` after project trust is
granted. my-aidlc ships:

- `.pi/skills/aidlc/SKILL.md` — the orchestrator, loaded on demand
- `.pi/prompts/aidlc.md` — the `/aidlc` prompt template
- `.pi/agents/`, `.pi/phases/`, `.pi/scopes/`, `.pi/protocols/`, `.pi/tools/`

Force the skill with `/skill:aidlc` when automatic routing misses it.

## Codex CLI

Project config lives in `.codex/`. Codex discovers skills at
`.agents/skills/`, so my-aidlc places the orchestrator there. The context file
is `AGENTS.md` at the project root and the invoke token is `$aidlc`.

Codex runs project skills and hooks only in a trusted project. Approve the
project when prompted; if skills do not appear, trust the folder and restart
the session. `.codex/config.toml` sets `project_doc = "AGENTS.md"`.

## Claude Code

Project config lives in `.claude/`. The orchestrator skill is at
`.claude/skills/aidlc/SKILL.md`, and `.claude/rules/aidlc.md` imports the
method memory files (`aidlc/spaces/default/memory/*`) into ambient context by
reference — it is a stub, not a copy. Edit the method at the workspace root.

`.claude/settings.json` pre-approves the engine's own commands. Copy
`.claude/settings.local.json.example` to `.claude/settings.local.json` for
personal overrides.

## Adding a harness

A harness is a manifest plus a handful of authored files:

1. Create `harness/<name>/manifest.mjs` exporting a manifest object.
2. Add `harness/<name>/onboarding.md` and any native config files.
3. Run `node scripts/package.mjs <name>`.

The manifest declares `coreDirs`, `coreFiles`, `harnessFiles`,
`projectFiles`, `coreProjectFiles`, and `onboarding`. The packager substitutes
`{{HARNESS_DIR}}`, `{{INVOKE}}`, and `{{PRODUCT}}` in every text file. See
[`harness/pi/manifest.mjs`](../../harness/pi/manifest.mjs) for the simplest
example.
