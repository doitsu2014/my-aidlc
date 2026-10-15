# Methodology reference

The methodology is data, but it is not authored here. It is the native AWS
AI-DLC release pinned in `aidlc-version.json` and snapshotted under
`core/native/`. `my-aidlc update` is the only writer, and
`my-aidlc update --check` verifies the snapshot offline against
`core/native/lock.json`. The engine loads the snapshot at runtime and derives
the workflow order for a profile. The profiles your team runs live in the
space layer under `aidlc/spaces/<space>/scopes/`.

## Layout

The engine tree is projected with your harness. `<harness dir>` is `.pi`,
`.claude`, `.codex`, or `.dsh`; the
[harness guide](../guide/04-harnesses.md) lists the harness-specific files.

```
<harness dir>/
  native/
    phases/<phase>.md            # display + gate metadata for the 5 phases
    stages/<phase>/<slug>.md     # the 33 native stage definitions
    agents/<slug>.md             # the 14 native agent personas
    scopes/<name>.md             # the 11 native baseline scopes
    protocols/*.md               # the 9 native protocols (reference)
    lock.json                    # the pin plus a SHA-256 for every file
    ADAPTATION.md                # native call -> my-aidlc engine mapping
  templates/scopes/<name>.md     # the 7 seeds copied into a project's space
  protocols/*.md                 # stage, question, recovery, learnings
  knowledge/*.md                 # audit format, artifact vocabulary
  skills/                        # the orchestrator skill and database skills
  tools/                         # the Node.js engine
  version.json                   # the projected framework version
```

In a project, your space is separate from the engine:

```
aidlc/spaces/<space>/
  memory/{org,team,project}.md   # your standing rules
  scopes/<name>.md               # your workflow profiles
  intents/<id>/<phase>/<stage>/  # artifacts and memory per intent
```

`native/` is generated. Edit a seed under `core/templates/scopes/`, or its
projected copy under `<harness dir>/templates/scopes/`, or the seeded profile
in `aidlc/spaces/<space>/scopes/`; never edit `native/` by hand.

## Phase frontmatter

| Field | Type | Notes |
| --- | --- | --- |
| `slug` | string | One of the five phase slugs |
| `name` | string | Display name |
| `order` | number | Sort order across phases |
| `focus` / `ai_role` / `output` | string | Human-facing metadata |
| `description` | string | One-line phase summary |
| `key_activities` | string[] | Human-facing list |
| `example_prompts` | string[] | Human-facing prompts |
| `review` | `auto` \| `required` | Default `auto`. `required` stops for a phase review before the workflow leaves the phase |

Phase files are part of the native snapshot: `my-aidlc update` rewrites them.
Set the durable phase-review toggle with `my-aidlc config --review-required
true` or a scope's `review_required` instead.

## Stage frontmatter

| Field | Type | Notes |
| --- | --- | --- |
| `slug` | string | Must match the filename stem |
| `name` | string | Display name (defaults to title case) |
| `phase` | string | One of the five phase slugs |
| `execution` | `ALWAYS` \| `CONDITIONAL` | |
| `condition` | string | Required for conditional stages |
| `lead_agent` | string | Must reference a known agent, or `orchestrator` |
| `support_agents` | string[] | |
| `mode` | `inline` \| `subagent` \| `pipeline` \| `mob` | Default `inline` |
| `reviewer` | string | Optional reviewer agent |
| `review_class` | `adversarial` \| `advisory` | |
| `review` | `auto` \| `required` | Default `auto`. `required` always presents the human gate, even in YOLO mode |
| `review_artifact` | string | Artifact the reviewer inspects |
| `for_each` | string | Repeat the stage per unit (when set) |
| `workspace_requires` | boolean | Stage writes code, not just docs |
| `question_budget` | `{ min, max }` | Optional per-stage question cap override |
| `produces` | string[] | Artifact names |
| `consumes` | object[] | `{ artifact, required }` |
| `requires_stage` | string[] | Ordering / dependency edges |
| `scopes` | string[] | The native baseline grid: which baseline scopes run this stage |
| `inputs` / `outputs` | string | Human-facing prose |

The three Initialization stages declare `lead_agent: orchestrator`, a reserved
lead for the engine's own conductor rather than an agent file.

## Scope frontmatter

Space scopes and native baseline scopes use the same shape.

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Profile name |
| `depth` | string | `Minimal`, `Standard`, or `Comprehensive` |
| `keywords` | string[] | Trigger words for auto-detection |
| `description` | string | Shown by `my-aidlc scope` |
| `include` | string[] | The complete stage list for this profile. Authoritative when non-empty |
| `skip` | string[] | Stages that do not run, or the remainder of the inventory |
| `phases` | string[] | Phases that run, when `include` and `skip` do not decide |
| `mode` | `normal` \| `yolo` | Optional autonomy override for this profile |
| `review_required` | boolean | Optional phase-review override for this profile |
| `question_budget` | `{ min, max }` | Optional per-profile question cap override |
| `learnings` | `on` \| `off` | Include the learnings protocol in this profile's stages |
| `skeleton` | `on` \| `off` | Walking-skeleton ceremony, carried in the file |
| `review_cap` | `adversarial` \| `advisory` \| `none` | Carried in the file |
| `guard_policy` | `strict` \| `relaxed` \| `off` | Carried in the file |
| `sensors` | `on` \| `off` | Carried in the file |
| `summary_confirmation` | `on` \| `off` | Carried in the file |

The engine acts on `name`, `keywords`, `include`, `skip`, `phases`, `mode`,
`review_required`, `question_budget`, and `learnings`. It loads the remaining
keys and surfaces them through `my-aidlc scope --json`; the native baseline
files also carry upstream keys that the loader ignores.

## Applicability

A stage runs under a scope when, most specific first:

1. the scope's `include` list is non-empty and names the stage's slug;
2. the scope's `skip` list names the stage (so it does not run);
3. the stage's own `scopes:` list names the scope (the native baseline grid);
4. the scope's `phases` list contains the stage's phase.

Because `include` is authoritative, a space scope can re-map the whole workflow
without touching a stage file. A partial `include` silently drops every unlisted
stage, so the seeds name the remainder under `skip`;
`node scripts/lint.mjs` fails unless a seed's two lists partition the 33 native
stages exactly, and `my-aidlc doctor` fails on an unknown slug in any scope,
space scopes included. The 11 native baseline scopes declare no `include`, so
they resolve through rules 3 and 4.

## Workflow order

Applicable stages are topologically sorted by `requires_stage` edges, with
ties broken by phase order and then slug. This yields the deterministic
`stage_total` and `stage_index` that directives carry.

## Compiled graph

`node scripts/package.mjs` compiles the methodology to
`core/data/stage-graph.json`, containing a `native` block (repository,
version, tag, commit, and counts), `phases`, `stages`, `scopes`, and the
`scopeGrid` (profile to ordered stage slugs). It is a repo artifact and is not
projected into a project; `my-aidlc graph` compiles and prints the same
structure on demand.

## Changing the methodology

Methodology is upstream data, so there are two supported changes:

- To change which stages a workflow runs, edit a space scope's `include` list
  in `aidlc/spaces/<space>/scopes/`, or the seed it came from in
  `core/templates/scopes/`. Never edit a stage file.
- To move to a newer AWS AI-DLC release, update the pin in
  `aidlc-version.json` and run `node scripts/fetch-native.mjs` (or
  `my-aidlc update` from the checkout). Review the diff under `core/native/`,
  then re-check that every seed still partitions the stage inventory. The
  script also accepts `--from <archive>` and `--dry-run`.

Adding a stage or an agent by hand is not supported: stages and agents come
from the pinned release.

## Verification

Run these from the repo root before considering a methodology change complete:

```bash
node scripts/fetch-native.mjs --check   # snapshot matches the pinned release
node scripts/lint.mjs                   # structural checks on the methodology
node scripts/package.mjs                # rebuild dist/ and the compiled graph
node scripts/package.mjs --check        # determinism guard
node tests/run-tests.mjs                # unit + integration tests
```
