# Work units

Use this for the Develop outputs `work-units` (Markdown) and `work-units-json` (`work-units.json` beside it). Each unit is a small, reviewable vertical slice: a coherent behavior with implementation and relevant tests verified together. Use a separate test-first step only when it adds value for the risk (for example regressions, security-sensitive behavior, public contracts, or complex rules); avoid test-only units that leave the slice's behavior unfinished. Both files are required at `full` and `standard` depth: `orchestrate report` refuses `implementation-design` without a valid `work-units.json`, so a missing file can no longer turn the phase into one implicit unit.

```markdown
# Work units

## WU-01 <title>
- Stories: US-001
- Criteria: AC-001
- Depends on:
- Done when:
```

`work-units.json`:

```json
{
  "units": [
    { "id": "WU-01", "title": "Backend restore service", "criteria": ["AC-001"], "dependsOn": [] },
    { "id": "WU-02", "title": "Restore API and UI", "criteria": ["AC-002"], "dependsOn": ["WU-01"] }
  ]
}
```

Rules:

- Identifiers are `WU-` plus digits, unique, in the order Develop should run them. Older intents that use `U-01` are still accepted.
- `dependsOn` names only units in the same file and must not form a cycle.
- List the unit identifiers in the `WU` section of `ids.json` as well (see [the ID registry template](id-registry.md)).
- If the plan has more units than the project's split threshold (6 by default), or units touch unrelated components, say so in the plan: the owner may split the intent (for example a backend intent and an API/UI intent).
