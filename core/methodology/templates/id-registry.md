# ID registry

Use this for the Develop output `id-registry`, written as `ids.json` beside the work units. `implementation-planner` mints every Develop identifier here during `implementation-design`. Every later writer cites identifiers from this file and never mints a new one.

```json
{
  "schemaVersion": 1,
  "ids": {
    "FN": [{ "id": "FN-001", "title": "Restore a backup", "traces": ["AC-001"] }],
    "IF": [{ "id": "IF-001", "title": "POST /restores", "traces": ["FN-001"] }],
    "TS": [{ "id": "TS-001", "title": "Restore round-trip integration test", "traces": ["AC-001", "FN-001"] }],
    "WU": [{ "id": "WU-01", "title": "Backend restore service" }],
    "CFG": [{ "id": "CFG-001", "title": "RESTORE_TIMEOUT_SECONDS" }]
  }
}
```

| Prefix | Meaning | Written in |
| --- | --- | --- |
| `FN-` | Function in the functional design | functional-design |
| `IF-` | Interface or contract | interfaces-and-contracts |
| `TS-` | Test or test suite named by the test strategy | test-strategy |
| `WU-` | Work unit | work-units, work-units.json |
| `CFG-` | Configuration key, flag or environment variable | implementation-plan, architecture-design |

Rules:

- Identifiers are `PREFIX-` plus digits, unique across the registry, and stable once written. Keep the number format the same within a prefix (`FN-001`, not `FN-1`).
- `AC-` and `US-` identifiers belong to Analyze. Cite them in `traces`; do not list them here.
- The `WU` list, when present, has exactly the unit identifiers in `work-units.json`. Older intents that use `U-01` are read as `WU-01`.
- `dtech-aidlc ids check` lists cited identifiers that are not registered. `orchestrate report` refuses to complete a Develop or Launch stage whose artifacts cite an unregistered identifier.
- A writer that needs a new identifier stops and reports it. The conductor returns to `implementation-design` to register it; the writer does not mint it.
