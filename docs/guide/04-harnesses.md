# Harnesses

my-aidlc is one harness-neutral core with thin per-harness projections. The
methodology, agents, stages, scopes, and engine are identical on every harness;
only discovery paths, invocation, and settings differ. The methodology is the
native AWS AI-DLC snapshot, fetched into `core/native/` and projected as
`<harness dir>/native/`; see the
[methodology reference](../reference/02-methodology.md).

| Harness | Configure | Invoke | Harness dir | Skill location | Context file |
| --- | --- | --- | --- | --- | --- |
| PI Agent | `my-aidlc config --harness pi` | `/aidlc` | `.pi/` | `.pi/skills/aidlc/` | `AGENTS.md` |
| Codex CLI | `my-aidlc config --harness codex` | `$aidlc` | `.codex/` | `.agents/skills/aidlc/` | `AGENTS.md` |
| Claude Code | `my-aidlc config --harness claude` | `/aidlc` | `.claude/` | `.claude/skills/aidlc/` | `CLAUDE.md` |
| DeepSeek Harness | `my-aidlc config --harness deepseek` | `/aidlc` | `.dsh/` | `.dsh/skills/aidlc/` | `AGENTS.md` |

## PI Agent

Project config lives in `.pi/`. PI discovers project skills under
`.pi/skills/` and prompt templates under `.pi/prompts/` after project trust is
granted. my-aidlc ships:

- `.pi/skills/aidlc/SKILL.md` - the orchestrator, loaded on demand
- `.pi/prompts/aidlc.md` - the `/aidlc` prompt template
- `.pi/native/` - the pinned native methodology (phases, stages, agents, and
  baseline scopes)
- `.pi/templates/scopes/` - the workflow-profile seeds copied into your space
- `.pi/protocols/`, `.pi/knowledge/`, `.pi/tools/`, and `.pi/skills/`

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
reference - it is a stub, not a copy. Edit the method at the workspace root.

`.claude/settings.json` pre-approves the engine's own commands. Copy
`.claude/settings.local.json.example` to `.claude/settings.local.json` for
personal overrides.

## DeepSeek Harness

Project files live in `.dsh/`. DSH scans its skill roots in this provider's rank
order, so the orchestrator sits in the `project-dsh` root at rank 100 - the
highest-priority project root:

| Rank | Source | Path |
| --- | --- | --- |
| 100 | `project-dsh` | `.dsh/skills/` |
| 200 | `project-agents` | `.agents/skills/` |
| 400 | `user-dsh` | `$DSH_HOME/skills/` |
| 500 | `user-agents` | `$DSH_AGENTS_HOME/skills/` |

Only the top level of a root is scanned, so the orchestrator lives at
`.dsh/skills/aidlc/SKILL.md` and the four database skills sit beside it.

DSH turns any skill whose frontmatter does not set `user-invocable: false` into
a `/<name>` command, available identically from the Web GUI, the TUI, and ACP.
The orchestrator is therefore invoked as `/aidlc` with no separate prompt
template or settings file.

The context file is `AGENTS.md` at the project root; DSH walks from the project
root (the nearest ancestor holding `.git`) down to the session working
directory, loading `AGENTS.md` then `CLAUDE.md` at each level.

DSH has no project-scoped native config file. Plugins and config patches
compose from `$DSH_HOME/profiles/<name>/cordis.patch.yml`, which belongs to the
user's installation rather than to the project, so `config --harness deepseek`
writes no file under `.dsh/` beyond the projected core.

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
