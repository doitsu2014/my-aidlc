# Failing tests: WU-012 (deps add lock)

- Purpose: list the test written first for this unit.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-031, FN-037, AC-027, US-013.
- Kind: code manifest.

| Test | File | Test id | State before the fix (Observed) |
|---|---|---|---|
| `ts031_concurrent_deps_add_keeps_three_edges` | `tests/deps_lock.rs` (new) | TS-031 | FAILED: `edge e2 lost: [{"id":"x","dependency":"e1"}]` (lost update) |
