# Test strategy

Use this for the Develop output `test-strategy`. For each level say whether it applies and why. Map each acceptance criterion to a level. Name the exact commands. Map each test to the work unit that owns it, and mirror that mapping in the `tests` list of each unit in `work-units.json`: the CLI reads only `work-units.json` and `ids.json` to attribute a failing test to its unit. Where the framework allows, start a test name with its identifier (for example `ts027_restore_rejects_a_stale_lock` for TS-027), so a failure can be attributed by name.

```markdown
# Test strategy

## Levels
### Unit
- Applies: yes | no
- Why:
### Integration
- Applies: yes | no
- Why:
### End to end
- Applies: yes | no
- Why:
### Performance
- Applies: yes | no
- Why:
### Security
- Applies: yes | no
- Why:

## Criterion to level
| Test | Criterion | Function | Level |
| TS-001 | AC-001 | FN-001 | unit |

## Work unit to test mapping
| Work unit | Tests |
| WU-01 | TS-001, TS-002 |

## Commands
- Build:
- Format:
- Test:
- Baseline (whole existing suite):

## Environment prerequisites
- Name, check command, and how to fix it (for example: docker — `docker info` — start Docker Desktop)

## Environments and data
-

## Unverified
-
```
