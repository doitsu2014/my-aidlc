# Agents

my-aidlc defines fifteen agents. Each is a persona with a clear remit; the
conductor adopts the lead agent's voice for a stage and may dispatch support
agents or a reviewer.

## Cross-cutting

| Agent | Tier | Owns |
| --- | --- | --- |
| `composer-agent` | judgment | Detecting and composing the right workflow profile, and reshaping a plan mid-flight |

## Analyze

| Agent | Tier | Owns |
| --- | --- | --- |
| `product-agent` | judgment | Intent capture, scope, the product view of requirements |
| `business-analyst-agent` | judgment | Requirements analysis, classification, traceability |
| `research-agent` | balanced | Research synthesis, competitive and technical discovery |

## Ideate

| Agent | Tier | Owns |
| --- | --- | --- |
| `architect-agent` | judgment | Architecture, API contracts, feasibility |
| `design-agent` | balanced | Design patterns, the technical specification |

## Develop

| Agent | Tier | Owns |
| --- | --- | --- |
| `delivery-agent` | balanced | Task planning, unit decomposition, sequencing |
| `developer-agent` | balanced | Implementing approved units and verifying them |
| `code-reviewer-agent` | judgment | Reviewing every diff against its contract |

## Launch

| Agent | Tier | Owns |
| --- | --- | --- |
| `qa-agent` | balanced | Test plan, test generation, release validation |
| `devops-agent` | balanced | CI pipelines, deployment automation, rollback |

## Curate

| Agent | Tier | Owns |
| --- | --- | --- |
| `sre-agent` | judgment | Observability, SLOs, incident analysis |
| `performance-agent` | balanced | Baseline, bottleneck, verified optimization |
| `tech-writer-agent` | templated | Keeping documentation aligned with the system |

## Reviewers

`security-agent` (judgment) reviews architecture and code for threats, authz,
and secret handling. `code-reviewer-agent` reviews diffs. A stage's `reviewer`
field names the reviewer the engine invokes after artifact production and
before the gate.

## Tiers

Tiers describe how much judgement an agent's output needs and how much model
capability it warrants. `judgment` agents make recommendations a human must
confirm; `balanced` agents execute a defined remit; `templated` agents follow a
fixed shape. Tiers never change the approval model: humans own every decision.

## Delegation

The conductor is the bus. Agents never invoke one another. When a stage's
`mode` is `inline`, the conductor speaks as the lead agent. Support agents
appear in the stage record and are dispatched only when a stage declares a
dispatched topology.
