# Agents

my-aidlc defines fourteen agents, all from the pinned native AI-DLC release.
Each is a persona with a clear remit; the conductor adopts the lead agent's
voice for a stage and may dispatch support agents or a reviewer. Agent slugs
are prefixed `aidlc-`.

## Cross-cutting

| Agent | Tier | Owns |
| --- | --- | --- |
| `aidlc-composer-agent` | judgment | Composing the minimum viable workflow for an intent; dispatched by the orchestrator, never by a stage |

## Product and design

| Agent | Tier | Owns |
| --- | --- | --- |
| `aidlc-product-agent` | judgment | Intent Capture & Framing, Market Research, Scope Definition, Requirements Analysis, User Stories |
| `aidlc-product-lead-agent` | balanced | Reviewing requirements, user stories, and UX artifacts (does not produce) |
| `aidlc-design-agent` | judgment | Rough Mockups, Refined Mockups; supports Domain Design and User Stories |
| `aidlc-delivery-agent` | templated | Team Formation, Approval & Handoff, Delivery Planning; supports Scope Definition and Units Generation |

## Architecture and platform

| Agent | Tier | Owns |
| --- | --- | --- |
| `aidlc-architect-agent` | judgment | Feasibility & Constraints, Domain Design, Contract Design, Units Generation, Functional Design, NFR Requirements, NFR Design |
| `aidlc-architecture-reviewer-agent` | balanced | Reviewing technical design artifacts for soundness, implementability, and coherence |
| `aidlc-aws-platform-agent` | judgment | Infrastructure Design, Environment Provisioning; supports Feasibility, Domain Design, Contract Design, NFR Design, Feedback & Optimization |
| `aidlc-compliance-agent` | judgment | Compliance mapping, data classification, and risk assessment; support-only |

## Engineering and delivery

| Agent | Tier | Owns |
| --- | --- | --- |
| `aidlc-developer-agent` | judgment | Reverse Engineering, Code Generation; supports Practices Discovery and User Stories |
| `aidlc-pipeline-deploy-agent` | templated | Practices Discovery, CI Pipeline, Deployment Pipeline, Deployment Execution |
| `aidlc-devsecops-agent` | judgment | Threat modelling and secure pipeline integration; supports NFR Requirements, Infrastructure Design, Build and Test, Environment Provisioning |
| `aidlc-quality-agent` | judgment | Build and Test, Performance Validation; supports NFR Requirements and Functional Design |

## Operations

| Agent | Tier | Owns |
| --- | --- | --- |
| `aidlc-operations-agent` | templated | Observability Setup, Incident Response, Feedback & Optimization; supports Performance Validation |

The three Initialization stages are not led by an agent. They declare
`lead_agent: orchestrator`, which is a reserved lead for the engine's own
conductor.

## Reviewers

A stage's `reviewer` field names the reviewer the engine invokes after artifact
production and before the gate. Two agents review; neither produces:

| Reviewer | Stages |
| --- | --- |
| `aidlc-product-lead-agent` | intent-capture, requirements-analysis, user-stories, rough-mockups, refined-mockups |
| `aidlc-architecture-reviewer-agent` | domain-design, contract-design, units-generation, functional-design, nfr-requirements, nfr-design, infrastructure-design, code-generation |

`aidlc-devsecops-agent` and `aidlc-compliance-agent` are support-only: they
appear as support agents, not as reviewers.

## Tiers

Tiers describe how much judgement an agent's output needs and how much model
capability it warrants. `judgment` agents make recommendations a human must
confirm; `balanced` agents execute a defined remit; `templated` agents follow a
fixed shape. Tiers never change the approval model: humans own every decision.

## Delegation

The conductor is the bus. Agents never invoke one another. A run directive
names the stage's `lead_agent`, its `support_agents`, its `mode` (`inline`,
`subagent`, `pipeline`, or `mob`), and, when the stage declares one, its
`reviewer`. In `inline` mode the conductor speaks as the lead agent; support
agents appear in the stage record.
