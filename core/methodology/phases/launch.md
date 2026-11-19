---
id: launch
name: Launch
order: 4
agents:
  - release-coordinator
  - operations-handoff
stages:
  - release-readiness
  - operational-handoff
gate: launch-and-handoff
---

# Launch

Coordinate a controlled release, communication, and the initial operational handoff.

## Entry

- Develop exit criteria are met and the release candidate is approved.

## Exit

- Release status and deployment evidence are recorded.
- Monitoring, support ownership, and the rollback path are confirmed.
- The project owner approves the launch outcome and operational handoff.

On rejection, pause or roll back according to the approved plan. Resolve the concerns before declaring launch complete.

## Stages

1. **release-readiness** — `release-coordinator`. Produces a release checklist, a deployment and rollback plan, and a communications draft.
2. **operational-handoff** — `operations-handoff`. Produces a launch record, monitoring and support ownership, and a handoff record. This stage is the phase gate.
