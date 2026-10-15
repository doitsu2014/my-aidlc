# Workflows

my-aidlc runs the native AWS AI-DLC methodology from
[`awslabs/aidlc-workflows`](https://github.com/awslabs/aidlc-workflows),
pinned in `aidlc-version.json` (AI-DLC 2.11.0). It has five phases and 33
stages, and every stage ends at a human approval gate. Which stages actually
run is decided by a workflow profile (a scope): your team owns the profiles in
its space, and they layer over the baseline the pinned release ships.

## The phases

| Phase | Focus | AI role | Stages |
| --- | --- | --- | --- |
| Initialization | Setup | Workspace orchestrator | 3 |
| Ideation | Intent | Product partner | 7 |
| Inception | Design | Architect partner | 9 |
| Construction | Code | Coding agent | 7 |
| Operation | Run | Operations assistant | 7 |

Some stages are CONDITIONAL: they self-skip at runtime and record a reason
when they do not apply (for example `reverse-engineering` on a greenfield
project). `my-aidlc stage <slug>` shows a stage's execution mode, lead agent,
and full artifact list.

### Initialization

Creates the intent and its record directory, classifies the workspace, and
routes the plan the scope selects. These three stages are led by the engine's
own conductor rather than an agent persona, and they write the record
directory rather than a named artifact.

| Stage | Lead | Produces |
| --- | --- | --- |
| Workspace Scaffold | orchestrator | record dir |
| Workspace Detection | orchestrator | record dir |
| State Initialization | orchestrator | record dir |

### Ideation

Captures intent, researches the market and prior art, tests feasibility,
defines scope, forms the team, sketches the experience, and secures approval
before technical work begins.

| Stage | Lead | Produces |
| --- | --- | --- |
| Intent Capture & Framing | aidlc-product-agent | intent-statement, stakeholder-map |
| Market Research | aidlc-product-agent | competitive-analysis, market-trends |
| Feasibility & Constraints | aidlc-architect-agent | feasibility-assessment, constraint-register |
| Scope Definition | aidlc-product-agent | scope-document, intent-backlog |
| Team Formation | aidlc-delivery-agent | team-assessment, skill-matrix |
| Rough Mockups | aidlc-design-agent | wireframes, user-flow |
| Approval & Handoff | aidlc-delivery-agent | initiative-brief, decision-log |

### Inception

Turns approved intent into reverse-engineered understanding, formal
requirements, user stories, refined mockups, domain and contract design, units
of work, and a delivery plan.

| Stage | Lead | Produces |
| --- | --- | --- |
| Reverse Engineering | aidlc-developer-agent | architecture, code-structure |
| Requirements Analysis | aidlc-product-agent | requirements |
| User Stories | aidlc-product-agent | stories, personas |
| Refined Mockups | aidlc-design-agent | mockups, interaction-spec |
| Domain Design | aidlc-architect-agent | components, decisions |
| Contract Design | aidlc-architect-agent | contract-summary |
| Units Generation | aidlc-architect-agent | unit-of-work, traceability |
| Delivery Planning | aidlc-delivery-agent | bolt-plan, risk-and-sequencing-rationale |
| Practices Discovery | aidlc-pipeline-deploy-agent | team-practices, discovered-rules |

### Construction

Designs the functional and non-functional detail, specifies infrastructure,
generates code per unit, tests the build, and wires the CI pipeline.

| Stage | Lead | Produces |
| --- | --- | --- |
| Functional Design | aidlc-architect-agent | functional-spec, entities |
| NFR Requirements | aidlc-architect-agent | performance-requirements, security-requirements |
| NFR Design | aidlc-architect-agent | performance-design, observability-design |
| Infrastructure Design | aidlc-aws-platform-agent | infrastructure-specification, monitoring-design |
| Code Generation | aidlc-developer-agent | code-generation-plan, code-summary |
| Build and Test | aidlc-quality-agent | build-instructions, build-test-results |
| CI Pipeline | aidlc-pipeline-deploy-agent | ci-config, quality-gates |

### Operation

Deploys, provisions environments, observes the running system, prepares
incident response, validates performance against the non-functional targets,
and feeds improvements back.

| Stage | Lead | Produces |
| --- | --- | --- |
| Deployment Pipeline | aidlc-pipeline-deploy-agent | cd-config, rollback-runbook |
| Environment Provisioning | aidlc-aws-platform-agent | environment-inventory, validation-report |
| Deployment Execution | aidlc-pipeline-deploy-agent | deployment-log, health-check-report |
| Observability Setup | aidlc-operations-agent | dashboards, alarms |
| Incident Response | aidlc-operations-agent | runbooks, incident-plan |
| Performance Validation | aidlc-quality-agent | load-test-plan, load-test-results |
| Feedback & Optimization | aidlc-operations-agent | slo-report, cost-analysis |

## Where the workflow comes from

The methodology is fetched, not authored. Two layers meet in the engine:

- The native layer is the pinned AWS AI-DLC release, snapshotted under
  `core/native/` and projected with your harness as `<harness dir>/native/`.
  It defines the five phases, the 33 stages, the 14 agents, and 11 native
  baseline scopes. `my-aidlc update` rewrites it from the pin in
  `aidlc-version.json`; `my-aidlc update --check` verifies it offline. Never
  hand-edit it.
- The space layer is yours, at `aidlc/spaces/<space>/scopes/<name>.md`. A
  space scope with the same name as a native baseline replaces it, and a new
  name adds a profile. This is where `include` and `skip` are authored.

## Workflow profiles (scopes)

`my-aidlc config` and `my-aidlc init` seed the space-scope directory from
`core/templates/scopes/` and never overwrite a file that already exists. The
seven seeds are the profiles below. Each one states a complete partition of
the 33 native stages: everything it runs is in `include`, and everything else
is in `skip`.

| Profile | Depth | Stages | Shape |
| --- | --- | --- | --- |
| `classic` | Standard | 33 | Full lifecycle, one gate per stage |
| `express` | Minimal | 10 | Requirements to deploy, no design pass |
| `feature` | Standard | 32 | Full design pass minus `feedback-optimization` |
| `bugfix` | Minimal | 10 | Defect statement, fix, verify, ship |
| `mvp` | Standard | 16 | Thin design pass, no Operation |
| `poc` | Minimal | 7 | Prove one assumption, then stop |
| `infra` | Standard | 27 | Infrastructure design and full Operation |

The pinned release also ships 11 native baseline scopes: `bugfix`, `classic`,
`enterprise`, `express`, `feature`, `infra`, `mvp`, `poc`, `refactor`,
`security-patch`, and `workshop`. They declare no `include`, so they resolve
through the stage-declared grid. `my-aidlc scope` marks each profile `space` or
`native`, and prints the stage count for a space profile:

```bash
my-aidlc scope            # every profile, source-marked
my-aidlc scope feature    # one profile's resolved stage list
my-aidlc list scopes      # the profiles as the engine sees them
```

### How a stage resolves

A stage runs under a profile when, most specific first:

1. the profile's `include` list is non-empty, and the stage's slug is in it;
2. the profile's `skip` list names the stage (so it does not run);
3. the stage's own `scopes:` declaration names the profile (the native grid);
4. the profile's `phases` list contains the stage's phase.

`include` is authoritative. When it is non-empty, a stage it does not name
does not run, whatever the native baseline says. That is why a space scope can
re-map the whole workflow without touching a single stage file. Because
`include` is authoritative, a partial list would silently drop the unlisted
stages; the seeds therefore state the remainder under `skip`, and
`node scripts/lint.mjs` fails when a seed's two lists do not partition the 33
native stages exactly. `my-aidlc doctor` fails on an unknown stage slug in any
scope, space scopes included.

### Changing your workflow

Edit the profile file in your space. A stage moves between the lists:

```yaml
# aidlc/spaces/default/scopes/classic.md
---
name: classic
depth: Standard
description: "Full lifecycle through all five phases"
include:
  - workspace-scaffold
  - workspace-detection
  - state-init
  # ... every other stage this profile should run
skip:
  # ... every remaining stage
---
```

- To add a stage, move its slug from `skip` to `include`.
- To drop a stage, move its slug from `include` to `skip`.
- To add a profile, copy a seed to a new file, rename it, and edit its lists.
  Give it `keywords` if you want auto-detection, and a `description` for
  `my-aidlc scope`.
- To restore an edited or deleted seed, run `my-aidlc init` (or
  `my-aidlc config`): seeding copies back any missing file and leaves the
  others alone.

### Choosing a profile

The profile is detected from your request by keyword, longest match wins.
`classic` is the fallback, and the project config key `defaultScope` can change
it. You can also name a profile explicitly:

```bash
my-aidlc orchestrate next --scope bugfix "Login fails after a password reset"
```

Seeded keywords:

| Profile | Keywords |
| --- | --- |
| `bugfix` | bug, fix, regression, hotfix, defect, broken |
| `express` | express, quick, lightweight, small, tiny |
| `feature` | feature, add, enhance, capability, story |
| `infra` | infra, infrastructure, terraform, kubernetes, pipeline, platform, observability |
| `mvp` | mvp, prototype, launch fast, ship fast, first version |
| `poc` | poc, proof of concept, spike, experiment, prototype, validate idea |
| `classic` | (none; the default) |

## Approval gates

Every executed stage ends at a gate: **Approve**, **Request Changes**, and,
where relevant, **Skip**. On Request Changes you choose to Keep, Modify, or Redo
the artifact. A conditional stage that does not apply reports a skip with a
recorded reason.

## Execution modes: normal and YOLO

The project runs in one of two modes:

| Mode | Questions | Approval gates | Use for |
| --- | --- | --- | --- |
| `normal` (default) | Asked (bounded by the question budget) | Presented to the human | Anything that ships |
| `yolo` | Skipped; the recommended answer is chosen | Auto-satisfied | Demos, POCs, trusted automation |

```bash
my-aidlc config --mode yolo     # auto-pick answers, auto-approve gates
my-aidlc config --mode normal   # back to human questions and gates
my-aidlc config                 # show the current mode
```

YOLO does **not** skip any stage: every stage still runs and still writes its
artifacts. It removes the human in the loop, not the work. Every auto-approval
is recorded as `STAGE_AUTO_APPROVED` in `aidlc/audit.log`, so an unattended run
stays auditable.

### Review checkpoints

YOLO does not have to run blind. A stage whose frontmatter sets
`review: required` always presents its human gate, even in YOLO mode:

```yaml
---
slug: code-generation
review: required
---
```

The stage still runs with YOLO's recommended answers, then stops and presents
the approval gate with its produced artifacts. The engine reports it exactly
like a normal gate (`STAGE_AWAITING_APPROVAL`, then `STAGE_APPROVED` or
`STAGE_REJECTED`), so you can approve, request changes, or redo before the
workflow continues. `my-aidlc status` marks such stages with `reviewRequired`.

Stage files are part of the fetched native snapshot, so `my-aidlc update`
rewrites them. The default is `review: auto`, which follows the execution mode.
For a checkpoint your team owns, use the phase toggle or a scope's
`review_required` below.

### Phase review

To review a whole phase instead of a single step, turn on the project toggle:

```bash
my-aidlc config --review-required true    # gate before leaving each phase
my-aidlc config --review-required false   # off (default)
```

When the last stage of that phase finishes, the workflow pauses and presents
all the phase's stages and artifacts. Approve it to move on, or request changes
to re-open the whole phase, apply your feedback, and re-run it; it gates again
when the phase completes. Phase review works in both normal and YOLO mode, and
`my-aidlc status` shows the phase awaiting review.

The toggle resolves scope `review_required:` -> project `reviewRequired` ->
off, and is independent of the execution mode. A single phase can also opt in
with `review: required` in the phase's own file
(`core/native/phases/<phase>.md`), but that file is generated and `update`
rewrites it, so prefer the toggle or a scope's `review_required`.

### Per-scope overrides

A profile can pin its own mode in frontmatter, so a throwaway POC profile can
run unattended while the project stays `normal`:

```yaml
# aidlc/spaces/default/scopes/poc.md
mode: yolo
question_budget:
  min: 1
  max: 3
```

The mode resolves profile `mode:` -> project `mode` -> `normal`, and
`review_required:` resolves profile -> project -> off.

## Question budget

Each stage asks a bounded number of clarifying questions before its gate. The
effective budget is resolved most-specific-first:

1. stage frontmatter `question_budget`
2. scope frontmatter `question_budget`
3. project config `questionBudget`
4. the built-in default (`min: 0`, `max: 5`)

Set the project default with `my-aidlc config`:

```bash
my-aidlc config --questions-min 1 --questions-max 3   # tighter
my-aidlc config --questions-max 0                     # skip questions entirely
my-aidlc config                                       # show the current budget
```

`max: 0` disables the question flow for a stage; the conductor then generates
the artifacts directly. The budget never applies to the approval gate itself,
which is always exactly one decision.

## The ten rules

The AIDLC operating model, from the cheatsheet:

1. AI in the SDLC is an operating model change, not a tooling purchase.
2. Authoring is cheap. Verification is the constraint.
3. Individual output is not organizational throughput.
4. Governance before scale.
5. Baseline before adoption.
6. Measure outcomes, not activity.
7. Humans own merges.
8. The bar for AI-authored code is the bar for human code, applied with more
   suspicion.
9. Agents are services.
10. The compounding asset is your context, not the model.
