# Methodology reference

The methodology is data: phases, stages, and scopes authored as Markdown with
YAML frontmatter. The engine loads it at runtime and derives the workflow order
for a scope.

## Layout

```
core/
  phases/<phase>/phase.md          # phase metadata
  phases/<phase>/stages/<stage>.md # stage metadata + body
  scopes/<scope>.md                # workflow profile
  agents/<agent>.md                # agent persona
  protocols/*.md                   # stage, question, recovery, learnings
  knowledge/*.md                   # audit format, artifact vocabulary
  memory/{org,team,project}.md     # memory templates
```

## Stage frontmatter

| Field | Type | Notes |
| --- | --- | --- |
| `slug` | string | Must match the filename stem |
| `name` | string | Display name (defaults to title case) |
| `phase` | string | One of the five phase slugs |
| `execution` | `ALWAYS` \| `CONDITIONAL` | |
| `condition` | string | Required for conditional stages |
| `lead_agent` | string | Must reference a known agent |
| `support_agents` | string[] | |
| `mode` | `inline` \| `subagent` \| `pipeline` \| `mob` | Default `inline` |
| `reviewer` | string | Optional reviewer agent |
| `review_class` | `adversarial` \| `advisory` | |
| `produces` | string[] | Artifact names |
| `consumes` | object[] | `{ artifact, required }` |
| `requires_stage` | string[] | Ordering / dependency edges |
| `workspace_requires` | boolean | Stage writes code, not just docs |
| `question_budget` | `{ min, max }` | Optional per-stage question cap override |
| `inputs` / `outputs` | string | Human-facing prose |

## Scope frontmatter

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Profile name |
| `depth` | `Minimal` \| `Standard` | |
| `keywords` | string[] | Trigger words for auto-detection |
| `description` | string | |
| `phases` | string[] | Phases that run |
| `include` / `skip` | string[] | Stage-level overrides |
| `skeleton` | `on` \| `off` | Walking-skeleton ceremony |
| `review_cap` | `adversarial` \| `advisory` \| `none` | |
| `guard_policy` | `strict` \| `relaxed` \| `off` | |
| `sensors` | `on` \| `off` | |
| `learnings` | `on` \| `off` | |
| `summary_confirmation` | `on` \| `off` |
| `mode` | `normal` \| `yolo` | Optional autonomy override for this scope |
| `question_budget` | `{ min, max }` | Optional per-scope question cap override | |

## Applicability

A stage runs under a scope when:

1. the stage declares an explicit `scopes` list and the scope is in it, or
2. the scope's `include` names the stage, or
3. the stage's phase is in `scope.phases` and the scope's `skip` does not name
   the stage.

## Workflow order

Applicable stages are topologically sorted by `requires_stage` edges, with
ties broken by phase order and then slug. This yields the deterministic
`stage_total` and `stage_index` that directives carry.

## Compiled graph

`node scripts/package.mjs` compiles the methodology to
`core/data/stage-graph.json`, containing phases, stages, scopes, and the
`scopeGrid` (scope → ordered stage slugs). `my-aidlc graph` prints the same
structure.

## Adding a stage

1. Add `core/phases/<phase>/stages/<slug>.md` with the frontmatter above.
2. Reference only known agents and produced artifacts.
3. Run `node scripts/lint.mjs` and `node tests/run-tests.mjs`.
4. Add the stage to a scope's `include` (or leave it to phase membership).
