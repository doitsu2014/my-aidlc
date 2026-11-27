# Red evidence: WU-005

- Status: Complete (red-tests, 2026-11-28). Test-first: yes (public tool contract, input validation at the trust boundary, gate integrity, privacy).

## Initial red run (worker worktree)
- `tools.rs` with tests; `validate()` temporarily stubbed to always `Ok` and `classify()` to always `operation_failed` (real implementation set aside, then restored).
- `cargo test --lib mcp::tools`: 3 passed, 3 failed, behavioural:
  - `ts004_validation_runs_before_the_project_opens` FAILED (tools.rs:323, invalid args accepted).
  - `ts004_errors_are_classified_by_exact_equality_and_redacted` FAILED (tools.rs:388, `STATE_CHANGED` -> `operation_failed`).
  - `ts004_approval_without_label_is_approver_required` FAILED (tools.rs:464).

## Review round (fix worker; re-reviewer repeated the mutations)
- Tool names corrected to the approved design (`status`, `queue`, `next`, `report`; ADR-009, IF-006, AC-003). The conductor's brief had wrongly named them `aidlc_*`. With the tests renamed first and the code still on `aidlc_*`, `call_tool` returned None (unknown tool) and the tests panicked.
- New `ts004_invalid_arguments_win_even_when_open_would_fail`: with validation moved after `application::open`, it failed (left `operation_failed`, right `invalid_arguments`).
- `ts004_results_are_redacted_in_values_and_keys` now checks keys and values directly (the old serialized-JSON check was vacuous on Windows); removing `redact_value` from `finish()` makes it fail.
- `prerequisites::tests::ts010_skip_runs_no_command` flake: not reproducible on demand (millisecond timing); fixed by comparing rows without `startedAt`, the only time-dependent field.
