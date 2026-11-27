# Failing tests: WU-005 (MCP tool adapter)

- Status: Complete (red-tests, 2026-11-28). Cites TS-004, TS-018, FN-013, FN-014, FN-016, IF-006, CFG-005, AC-003, AC-005, AC-006, AC-008, AC-010, AC-011.
- Kind: code manifest. Tests in `rust-cli/mcp/tools.rs` (`#[cfg(test)]`).

| Test | Test id | Red observed |
|---|---|---|
| `ts004_validation_runs_before_the_project_opens` | TS-004 | yes (validate stubbed to Ok): invalid arguments were accepted |
| `ts004_invalid_arguments_win_even_when_open_would_fail` (review round) | TS-004 | yes, by mutation (validation moved after `application::open`): left `operation_failed`, right `invalid_arguments` |
| `ts004_errors_are_classified_by_exact_equality_and_redacted` | TS-004 | yes (classify stubbed): `STATE_CHANGED` classified as `operation_failed` instead of retryable `state_conflict` |
| `ts004_approval_without_label_is_approver_required` | TS-004 | yes (stubs): not mapped to `approver_required` |
| `ts004_results_are_redacted_in_values_and_keys` (rewritten in review) | TS-004 | yes, by mutation (re-reviewer): removing `redact_value` from `finish()` fails it |
| `ts004_blocked_next_is_a_result_not_an_error` | TS-004 | passed in the stub run (outcome mapping, not stubbed) |
| `ts018_exactly_four_tools_with_contract_annotations` | TS-018 | passed in the stub run; after the review rename it failed until the code used the design names `status`, `queue`, `next`, `report` (`call_tool` returned None for an unknown tool) |
| `prerequisites::tests::ts010_skip_runs_no_command` (WU-002's test; conductor-authorised test-only flake fix) | TS-010 | intermittent failure observed once under load: the two real runs differed in millisecond `startedAt`; now compared without `startedAt` |
