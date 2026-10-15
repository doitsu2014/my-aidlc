# AGENTS.md — working in the my-aidlc repository

This file guides AI coding agents contributing to my-aidlc itself. For using
my-aidlc in a project, see [README.md](README.md) and
[docs/guide/01-getting-started.md](docs/guide/01-getting-started.md).

## What this repository is

my-aidlc is one harness-neutral core plus thin per-harness projections:

- `core/native/` — **generated**. The pinned native AWS AI-DLC methodology
  (phases, stages, agents, baseline scopes, protocols), fetched and verified by
  `scripts/fetch-native.mjs`. Never edit it by hand.
- `core/templates/scopes/` — the workflow profiles my-aidlc seeds into a
  project's space (`aidlc/spaces/<space>/scopes/`). This is where a team's
  workflow is authored.
- `core/tools/` — the Node.js engine. `core/tools/lib/native.mjs` owns the pin.
- `harness/<name>/` — a manifest and authored files for PI Agent, Claude Code,
  Codex CLI, and DeepSeek Harness.
- `scripts/` — packaging, installers, the native fetch, and lint.
- `tests/` — the test suite.
- `dist/` — generated; never edit by hand.

## The golden rules

1. **Edit `core/` (except `core/native/`) or `harness/<name>`, never `dist/`.**
   Both generated trees are rebuilt: `dist/` by `node scripts/package.mjs`,
   `core/native/` by `node scripts/fetch-native.mjs`.
2. **The engine owns state.** Never advance a stage, write a state field, or
   append an audit row outside `core/tools/lib/`. The conductor reports
   outcomes; the engine transitions.
3. **Humans own decisions.** No agent may merge, approve, or silently choose an
   architecture, a scope, or a release.
4. **Methodology comes from AWS.** Phases, stages, and agents are native
   AI-DLC, pinned in `aidlc-version.json`. To change which stages a workflow
   runs, edit a space scope's `include` list — never a stage file.

## Development commands

```bash
node scripts/fetch-native.mjs --check  # snapshot matches the pinned release
node scripts/package.mjs               # build all harnesses into dist/
node scripts/package.mjs --check       # determinism guard: fail if dist is stale
node scripts/lint.mjs                  # structural checks on the methodology
node tests/run-tests.mjs               # unit + integration tests
```

Run all five before considering a change complete.

## Changing a workflow profile (scope)

Scopes are data, and they live in the space layer. The seven seeds are under
`core/templates/scopes/`; they are copied into `aidlc/spaces/<space>/scopes/`
on `config`/`init` and never overwritten.

1. Edit `core/templates/scopes/<name>.md`.
2. Frontmatter must include `name`, `depth`, `keywords`, `description`, and a
   complete `include` list of native stage slugs plus a `skip` list naming the
   rest. `scripts/lint.mjs` fails when the two lists do not partition the 33
   native stages exactly.
3. `include` is authoritative: a stage it does not name does not run, whatever
   the native baseline grid says. See `stageRunsInScope` in
   `core/tools/lib/graph.mjs`.
4. Optional ceremony keys: `mode`, `review_required`, `question_budget`,
   `review_cap`, `guard_policy`, `sensors`, `learnings`,
   `summary_confirmation`, `skeleton`.

`my-aidlc scope <name>` prints the resolved workflow; `my-aidlc scope` marks
each profile `space` or `native`.

## Moving to a newer upstream AI-DLC

1. Update `version`, `tag`, `commit`, `asset`, `assetUrl`, and `sha256` in
   `aidlc-version.json`.
2. Run `node scripts/fetch-native.mjs` and review the diff under
   `core/native/` — new, removed, and changed stages.
3. Re-check that every scope in `core/templates/scopes/` still partitions the
   stage inventory; `node scripts/lint.mjs` enforces it.
4. Run the full command set above.

## Adding an agent

Agents are upstream: add `core/native/agents/<slug>.md` only by fetching a
release that ships it. An agent unreferenced by any stage (other than the
cross-cutting composer) fails lint, as does a stage naming an agent that does
not exist. The Initialization stages declare `lead_agent: orchestrator`, which
is a reserved lead for the engine's own conductor rather than an agent file;
`scripts/lint.mjs` and `core/tools/lib/doctor.mjs` both allow it.

## Adding a harness

1. Create `harness/<name>/manifest.mjs` exporting a manifest.
2. Add `harness/<name>/onboarding.md` and native config files.
3. In `coreDirs`, project `native` -> `native` and `templates/scopes` ->
   `templates/scopes`; the engine reads the methodology and the scope seeds
   from those paths.
4. Register the harness dir/invoke strings in `core/tools/lib/paths.mjs`.
5. Run `node scripts/package.mjs <name>` and add it to the README table.

## Code conventions

- Node.js ESM (`.mjs`), no runtime dependencies.
- One module per concern under `core/tools/lib/`.
- Keep the engine deterministic: no random, no wall-clock-dependent decisions
  except timestamps in the audit log. The native snapshot carries no timestamp,
  so `--check` is reproducible.
- Prefer explicit errors over silent fallbacks.

## Reporting

The audit log is append-only. Event names come from
`core/knowledge/audit-format.md`. Do not invent event names.
