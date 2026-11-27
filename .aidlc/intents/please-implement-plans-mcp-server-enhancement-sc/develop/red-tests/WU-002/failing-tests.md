# Failing tests: WU-002 (Prerequisite guard, error constant and pending preview)

- Purpose: tests written before WU-002's implementation.
- Status: Complete (red-tests, 2026-11-28). Cites TS-007, TS-010, TS-011, FN-007, FN-008, FN-019, FN-020, AC-001, AC-008, AC-009, AC-011, AC-012.
- Kind: code manifest.

| Test | File | Test id | Red observed |
|---|---|---|---|
| `prerequisites::tests::ts010_skip_runs_no_command` | `rust-cli/prerequisites.rs` | TS-010 | yes: panic "skip must not spawn the check" |
| `orchestrate::tests::ts010_skip_runs_no_command` | `rust-cli/orchestrate.rs` | TS-010 | yes: panic "expected blocked" (skip flag ignored) |
| `orchestrate::tests::ts010_skip_on_an_existing_intent_runs_no_command_and_writes_nothing` | `rust-cli/orchestrate.rs` | TS-010 | yes (review round): audit.jsonl gained a `prerequisite.checked` row for a check that never ran |
| `orchestrate::tests::skip_does_not_hold_or_audit_a_unit_whose_needs_check_was_not_run` | `rust-cli/orchestrate.rs` | TS-010 | yes (review round): audit events contained `prerequisite.checked` |
| `orchestrate::tests::ts010_pending_prerequisites_preview_is_read_only` | `rust-cli/orchestrate.rs` | TS-010 | yes: left `[]` vs right one `toolchain` row with `executed:false` |
| `orchestrate::tests::ts007_stale_load_error_equals_state_changed` | `rust-cli/orchestrate.rs` | TS-007 | passed (guard: the constant is a pure hoist, bytes must equal the old text) |
| `orchestrate::tests::active_update_keeps_pointer_and_refuses_concurrent_changes` (existing, tightened to `assert_eq!(error, STATE_CHANGED)`) | `rust-cli/orchestrate.rs` | TS-007 | passed (existing behaviour) |
| TS-011 | full existing suite (CLI parity) | TS-011 | regression gate, run in build-and-verify |
