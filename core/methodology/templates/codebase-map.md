Use this template for the one-time project-level codebase map at `.aidlc/knowledge/_codebase-map.md`.

```markdown
# Codebase map

- Status: draft | approved
- Mapped on:
- Mapped at commit: (git commit id, or `not a git repository`)
- Mapped by:
- Sources consulted: (registered source ids and code paths)

## 1. System summary

## 2. Components

| Name | Path | Responsibility | Depends on |
|---|---|---|---|

## 3. Entry points

## 4. External dependencies and integrations

## 5. Data

## 6. Conventions

## 7. Tests

## 8. Build and run

## 9. Known risks and technical debt

## 10. Gaps
```

## Rules

- Label every statement **Observed** (with a path) or **Assumption**, as defined in [the contracts](../contracts.md).
- A test or build command is **Observed** only if it was actually run and its result was seen.
- Link to registered documents rather than restating them; link to `PROJECT.md` and `conventions` sources instead of repeating conventions.
- Write every path relative to the project root, never an absolute machine path. Do not include secrets or credentials.
- In Tests, name the framework, test location, exact whole-suite and single-test commands, typical duration, and known failures.
- Aim for under 300 lines. Every documentation-versus-code contradiction also becomes an open question in the Analyze output.
