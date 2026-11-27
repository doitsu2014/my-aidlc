# Red evidence: WU-002

- Status: Complete (red-tests, 2026-11-28). Test-first: yes (security boundary: no shell execution over MCP, owner decision Q5; audit integrity).

## Initial red run (worker worktree; stubs so tests execute)
- Stubs: `NextOptions.skip_prerequisite_checks` field, `STATE_CHANGED` const, `check_prerequisites` ignoring skip, `pending_prerequisites` returning `Ok(vec![])`.
- `cargo test --lib ts0`: 4 tests, 3 failed behaviourally, 1 passed:
  - `prerequisites::tests::ts010_skip_runs_no_command` FAILED: "skip must not spawn the check".
  - `orchestrate::tests::ts010_skip_runs_no_command` FAILED: "expected blocked".
  - `orchestrate::tests::ts010_pending_prerequisites_preview_is_read_only` FAILED: `[]` vs one row `{name: toolchain, executed: false, ...}`.
  - `ts007_stale_load_error_equals_state_changed` passed (expected; pins byte equality of the hoisted constant).

## Review-round red runs (independent review found two FN-007 gaps; fix worker, then re-reviewer reproduced both)
- With the site-3 guard as `if !is_new` (original patch): `ts010_skip_on_an_existing_intent_runs_no_command_and_writes_nothing` FAILED on the snapshot assertion: audit.jsonl gained `prerequisite.checked` with results `[{name: toolchain, ok: false, exitCode: null}]` although no check ran. This is the shape every MCP `next` takes (existing intent).
- With the site-2 `held_checks` push unguarded: `skip_does_not_hold_or_audit_a_unit_whose_needs_check_was_not_run` FAILED at the audit assertion (events contained `prerequisite.checked`).
- All pass with the fixes; the re-reviewer confirmed both reds independently in a copy.
