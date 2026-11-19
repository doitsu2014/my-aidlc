---
id: ideate
name: Ideate
order: 2
agents:
  - solution-explorer
  - decision-facilitator
stages:
  - option-generation
  - direction-selection
gate: solution-direction
---

# Ideate

Explore alternatives and select a solution direction grounded in the approved analysis.

## Entry

- Analyze exit criteria are met and its approval is recorded.

## Exit

- At least one feasible option and meaningful alternatives are documented.
- Trade-offs, scope, and a recommended direction are explicit.
- The project owner approves the selected direction before implementation planning.

On rejection, revisit options and trade-offs. Do not enter Develop.

## Stages

1. **option-generation** — `solution-explorer`. Produces solution options and a trade-off matrix.
2. **direction-selection** — `decision-facilitator`. Produces a recommended direction, a decision record, and a scope boundary. This stage is the phase gate.
