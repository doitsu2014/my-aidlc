# Workflows

my-aidlc follows the five AIDLC phases from the
[cheatsheet](https://aidlc.io/cheatsheet/). Each phase contains stages, and each
stage ends at a human approval gate.

| Phase | Focus | AI role | Output | Stages |
| --- | --- | --- | --- | --- |
| Analyze | Requirements | Research assistant | Specs, analysis docs | 4 |
| Ideate | Design | Architect partner | Diagrams, specs | 4 |
| Develop | Code | Coding agent | Working code | 4 |
| Launch | Deploy | QA engineer | Tests, pipelines | 5 |
| Curate | Maintain | SRE assistant | Fixes, updates | 4 |

## The stages

### Analyze

| Stage | Lead | Produces |
| --- | --- | --- |
| Intent Capture | product-agent | intent-statement |
| Requirements Analysis | business-analyst-agent | requirements |
| Research Synthesis | research-agent | research-report, competitive-analysis |
| Feasibility Assessment | architect-agent | feasibility-assessment, constraint-register |

### Ideate

| Stage | Lead | Produces |
| --- | --- | --- |
| Architecture Design | architect-agent | architecture-doc |
| Design Pattern Selection | design-agent | design-decisions |
| API Contract Design | architect-agent | api-contract |
| Technical Specification | design-agent | technical-spec |

### Develop

| Stage | Lead | Produces |
| --- | --- | --- |
| Task Planning | delivery-agent | implementation-plan, unit-breakdown |
| Code Generation | developer-agent | source-changes |
| Code Review | code-reviewer-agent | review-record |
| Refactoring | developer-agent | refactor-notes |

### Launch

| Stage | Lead | Produces |
| --- | --- | --- |
| Test Plan | qa-agent | test-plan |
| Test Generation | qa-agent | test-suite |
| CI Pipeline | devops-agent | ci-config |
| Deployment Automation | devops-agent | deployment-runbook, deployment-scripts |
| Release Validation | qa-agent | release-validation-report |

### Curate

| Stage | Lead | Produces |
| --- | --- | --- |
| Observability Setup | sre-agent | observability-config |
| Incident Analysis | sre-agent | incident-report |
| Performance Optimization | performance-agent | performance-report |
| Documentation Update | tech-writer-agent | updated-docs |

## Workflow profiles (scopes)

A profile selects which phases and stages run, and how much ceremony applies.

| Profile | Depth | Shape |
| --- | --- | --- |
| `classic` | Standard | All five phases, one gate per stage |
| `express` | Minimal | Analyze → Develop → Launch, no design pass |
| `feature` | Standard | Analyze → Ideate → Develop → Launch |
| `bugfix` | Minimal | Reproduce → fix → verify |
| `mvp` | Standard | Thin design pass, walking skeleton on |
| `poc` | Minimal | Prove an assumption, then stop |
| `infra` | Standard | Design, deploy, and operate infrastructure |

The profile is detected from your request (`bugfix`, `express`, `poc`, `mvp`,
`infra`, and `feature` have keywords) or named explicitly. `classic` is the
default.

Inspect a profile:

```bash
my-aidlc scope bugfix
my-aidlc list scopes
```

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

YOLO does not have to run blind. Mark a stage with `review: required` in its
frontmatter to keep one human gate while every other stage auto-approves:

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

The default is `review: auto`, which follows the execution mode.

### Phase review

To review a whole phase instead of a single step, set `review: required` on the
phase:

```yaml
# core/phases/develop/phase.md
---
slug: develop
review: required
---
```

When the last stage of that phase finishes, the workflow pauses and presents
all the phase's stages and artifacts. Approve it to move on, or request changes
to re-open the whole phase, apply your feedback, and re-run it; it gates again
when the phase completes. Phase review works in both normal and YOLO mode, and
`my-aidlc status` shows the phase awaiting review.

To gate *every* phase with one switch instead of editing each phase file:

```bash
my-aidlc config --review-required true    # gate before leaving each phase
my-aidlc config --review-required false   # off (default)
```

The toggle resolves scope `review_required:` → project `reviewRequired` → off,
and is independent of the execution mode.

The mode is resolved most-specific-first: scope `mode:` → project `mode` →
`normal`. A scope can therefore pin itself, e.g. a throwaway POC scope could set
`mode: yolo` while the project default stays `normal`.

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
