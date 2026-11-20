# Architecture design

Use this for the Ideate output `architecture-design`. Follow the arc42 structure. Use C4 names for the context and building-block views: System Context, Container, and Component. Significant decisions are written once, in the decision record (the output `decision-record`), each under its identifier with the headings Context, Decision, and Consequences; section 9 of this document only lists them.

```markdown
# Architecture design

## 1. Introduction and goals
- Purpose:
- Quality goals: see quality-requirements.md
- Stakeholders:

## 2. Constraints
- Technical:
- Organizational:
- Convention:

## 3. Context and scope
- System context:
- External systems:
- Out of scope:

## 4. Solution strategy
- Chosen approach:
- Rejected alternatives:

## 5. Building block view
- Containers:
- Components:
- Responsibilities:

## 6. Runtime view
- Main scenario:
- Failure scenario:

## 7. Deployment view
- Environments:
- Runtime nodes:

## 8. Cross-cutting concepts
- Security: see threat-model.md
- Data:
- Observability:

## 9. Architecture decisions
- ADR-001 <title> — <proposed | accepted | superseded> — see decision-record.md

## 10. Quality requirements
- See quality-requirements.md
- Scenario:
- Measure:

## 11. Risks and technical debt

## 12. Glossary
```

Rules:

- Describe the approved direction only. Do not reopen an unapproved option as if it were chosen.
- A box in a diagram needs a responsibility and an interface. A picture alone is not the design.
- Mark any section that does not apply as "not applicable" and say why.
- If the project already has decision records, continue their numbering and keep their convention; `ADR-001` in this template is a placeholder for the next free number.
- The decision record is where a decision is written. Section 9 lists each decision by its identifier with a relative link to the decision record and does not restate it.
