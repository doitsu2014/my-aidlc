---
id: analyze
name: Analyze
order: 1
agents:
  - brief-analyst
  - context-risk-analyst
stages:
  - problem-framing
  - context-and-risk
gate: analysis-baseline
---

# Analyze

Establish the problem, context, constraints, stakeholders, and evidence before selecting a direction.

## Entry

- A project brief or explicit request exists.

## Exit

- The problem and desired outcomes are stated.
- Constraints, assumptions, risks, and open questions are recorded.
- The project owner approves the problem framing and analysis baseline.

On rejection, revise the analysis artifacts and resubmit. Do not enter Ideate.

## Stages

1. **problem-framing** — `brief-analyst`. Produces a problem statement, success measures, and a stakeholder map.
2. **context-and-risk** — `context-risk-analyst`. Produces constraints and assumptions, an evidence log, a risk register, and open questions. This stage is the phase gate.
