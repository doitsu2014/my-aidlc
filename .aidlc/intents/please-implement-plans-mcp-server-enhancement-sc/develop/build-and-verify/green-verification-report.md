# Green verification report: final sweep (release-candidate gate)

- Purpose: evidence that the intent (shared facade plus MCP stdio server, ten work units) is verified as a release candidate.
- Status: Complete for the gate, with one acceptance criterion only partly evidenced (AC-003, DeepSeek Harness half not run). 2026-11-28, HEAD `ca30a85`.
- Cites WU-001..WU-010, FN-001..FN-022, TS-001..TS-020, AC-001..AC-014, US-001..US-009.

## Results per test level (all Observed unless stated)

### Whole existing suite, local sweep
- `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 33 targets, **545 passed, 0 failed**. Intent baseline 454 passed / 2 failed; both failures fixed by WU-010. Details in `regression-report.md`.
- `cargo fmt --check` clean. Clippy: only the known pre-existing warning (`tests/orchestration_golden.rs:1108`).
- `git diff 69cab5a..HEAD -- Cargo.lock tests/golden` empty: no new crate, no golden changed.

### Unit level (library tests)
- Per-unit focused runs all passed (packaging 5, guard 7, protocol 16, redaction 11, facade 5, adapter plus guards 35, lock 8, README guards 36). Library tests cover the guard (FN-005, FN-006, FN-007, FN-020), facade, protocol, tools, redaction and lock (FN-022).

### Integration level (child process, real binaries)
- `tests/mcp_stdio.rs` 13, `tests/mcp_parity.rs` 8, `tests/mcp_redaction.rs` 8, `tests/rust_cargo_install.rs` 6, `tests/mcp_packaging.rs` 5. CI "MCP transcript tests" step: 29 of 29 on every OS (13 + 8 + 8).
- Process-level smoke on the real binary (WU-004, WU-005): exactly `status`, `queue`, `next`, `report` listed; `report` approval without a label gives `approver_required`; unknown argument gives `invalid_arguments`; the CLI `status` printed 1 absolute path where MCP `status` and `queue` printed 0.

### Golden and CLI-invariance level
- All golden and orchestration suites pass unchanged (cli_golden 4, onboard_golden 4, orchestration_golden 14, routing_golden 11, scope_golden 1, status_golden 1, utility_golden 2, and others). No golden re-blessed (TS-011).

### CI level (run 46090623346 on GitHub, conclusion success)
- windows-latest: Test 547 passed / 0 failed; "Installer tests must not skip" 6 of 6, 0 skips; "MCP transcript tests" 29 of 29.
- macos-latest: Test 545 / 0; installer 6 of 6, 0 skips; MCP 29 of 29.
- ubuntu-latest (best effort): Test 545 / 0; installer 6 of 6; MCP 29 of 29.
- Only warning: `actions/checkout@v4` on the deprecated Node 20 runtime.

### Manual host level (TS-013), partial
- Observed: Claude Code 2.1.332, `claude mcp list` with an isolated `CLAUDE_CONFIG_DIR` and the real `target/debug/dtech-aidlc-mcp --project <empty temp dir>` -> "Connected"; the project directory stayed empty; the user's `~/.claude.json` was not modified.
- **Not run:** a model-driven `tools/call` in a live Claude Code session.
- **Not run:** DeepSeek Harness launch-and-list (`dsh` not installed on this machine).
- **Not run:** any Windows host launch.

## Quality requirements verified

| Requirement | Evidence | Result |
|---|---|---|
| Operability: identical pass/fail set, no golden re-bless (AC-001) | Sweep 545 / 0 against baseline 454 / 2; goldens untouched | Met (macOS, Windows, Ubuntu CI) |
| Operability: dependency footprint, offline `--locked` install (AC-013) | `ts016_dependency_set_is_unchanged`, `ts016_cargo_lock_bytes_unchanged`, `ts009_cargo_install_and_uninstall_both_binaries`; empty Cargo.lock diff | Met |
| Privacy: no absolute path, secret or trace in values or errors (AC-006, AC-011) | TS-004, TS-008 suites, including macOS `/private/var` twin and Windows-form strings; process-level path count 0 | Met for registered prefixes and messages; value-space limitation recorded |
| Privacy: no network listener, no host settings written (AC-014) | No HTTP transport in the code; install writes no host configuration; user `~/.claude.json` unmodified in the TS-013 partial check | Met (Observed from code and the partial check; no dedicated socket test was run) |
| Availability: session survives a bad call; stdout carries frames only (AC-003, AC-004, AC-011) | `ts005_*`, `ts003_*`, `ts014_*` suites on three OSes | Met in child-process tests; real-host check partial |
| Security: no prerequisite command over MCP, no intent creation, approval needs an explicit label (AC-008, AC-010) | Marker-file and mutation-proven tests (`ts010_*`, `ts006_*`) | Met for AC-008; AC-010 partial (see trace row)|
| Concurrency: no lost update, loser gets retryable conflict (AC-012) | `ts007_concurrent_reports_never_lose_an_update`, `ts020_*`; passed on all CI OSes | Met |
| Performance: no CLI regression beyond noise | Goldens identical; no timing measurement was recorded (the target was an estimate) | Not measured (Open question: accept as estimate) |
| Accessibility | Not applicable (CLI and stdio server) | Not applicable |

## Trace table (one row per acceptance criterion)

| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-001 | AC-001 CLI behaviour unchanged | FN-009, FN-018, FN-022 | full suite and all golden suites (TS-011, TS-017); `onboarding::tests::rejects_traversal_and_absolute_paths`, `onboarding::projection::tests::refuses_symlink_targets` (TS-019); `prerequisites::tests::ts010_skip_runs_no_command`; `ts020_lock_file_never_survives_a_command`, `ts020_root_without_aidlc_runs_unlocked_and_creates_nothing`; `ts015_ci_workflow_and_readme_mcp_section`; CI Test step on Windows (547/0), macOS (545/0), Ubuntu (545/0) | passed |
| US-002 | AC-002 One shared behaviour path | FN-001, FN-002, FN-003, FN-004, FN-005, FN-006 | CLI routed through the facade plus goldens (TS-011); `ts011_external_engine_from_honours_the_injected_home`; `ts006_lifecycle_through_mcp_matches_the_cli_state_and_audit`, `ts006_status_matches_the_cli` (TS-006) | passed |
| US-003 | AC-003 Server launches and exposes tools | FN-011, FN-013, FN-016, FN-018 | `ts001_legacy_session_exact_frames`, `ts001_initialize_version_negotiation`, `ts002_modern_exact_frames`, `ts002_era_mixing_in_one_process`, `ts018_exactly_four_tools_with_contract_annotations` (TS-018), `ts005_legacy_session_is_stdout_pure_and_answers_every_request`, `ts005_modern_session_needs_no_handshake_and_stamps_results_not_errors`, `ts005_eras_mix_per_request_on_one_connection`; CI "MCP transcript tests" 29/29 on Windows, macOS, Ubuntu; TS-013 Claude Code `claude mcp list` Connected | **partial: handshake, discovery and Claude Code launch passed; DeepSeek Harness launch-and-list NOT RUN (required by the owner decision); model-driven `tools/call` NOT RUN; the restart scenario has no dedicated test named in the unit reports (Open question)** |
| US-003 | AC-004 Stdout carries only protocol frames | FN-010, FN-011, FN-012, FN-016, FN-017 | `ts003_envelope_errors_and_notifications`, `ts003_parse_errors_crlf_blank_lines_and_eof`, `ts003_invalid_utf8_keeps_session_alive`, `ts003_panic_is_recovered_without_detail`, `ts003_empty_stdin_exits_zero_with_no_output` (TS-003); `ts005_empty_stdin_exits_zero_and_writes_nothing`, `ts005_crlf_framing_is_accepted_and_replies_are_lf_only`, `ts005_startup_failure_exits_one_before_reading_stdin`, `ts014_hostile_protocol_input_table_keeps_the_session_alive` | passed |
| US-004 | AC-005 Requests validated before project access | FN-010, FN-013 | `ts004_validation_runs_before_the_project_opens`, `ts004_invalid_arguments_win_even_when_open_would_fail` (TS-004); `ts003_oversized_line_is_drained_and_session_continues`, `ts003_cap_boundaries`; `ts014_hostile_tool_arguments_are_rejected_before_the_project_is_opened`, `ts014_boundary_values_pass_validation`, `ts014_line_of_exactly_the_cap_is_accepted` | passed |
| US-005 | AC-006 Status is read-only and path-safe | FN-003, FN-014, FN-015 | `ts004_results_are_redacted_in_values_and_keys`; `ts008_longest_prefix_wins_and_is_relative`, `ts008_boundary_is_respected`, `ts008_windows_forms_on_any_os`, `ts008_macos_var_and_private_var_both_covered` (TS-008); `ts008_status_and_queue_on_the_default_temp_base_carry_no_absolute_path`, `ts008_macos_private_var_twin_is_registered_for_a_var_root`; `ts006_status_matches_the_cli` | passed |
| US-005 | AC-007 Queue is a read-only preview | FN-003, FN-014 | `ts006_queue_is_a_read_only_preview_and_matches_the_cli` (state.json and audit.jsonl bytes unchanged) (TS-006) | passed |
| US-006 | AC-008 Next-stage guard | FN-004, FN-005, FN-007, FN-014, FN-015 | `ts010_mcp_next_cannot_create_an_intent`, `ts010_mcp_next_skips_prerequisite_checks_and_cli_runs_them`, `prerequisites::tests::ts010_skip_runs_no_command`, `orchestrate::tests::ts010_skip_runs_no_command`, `ts010_skip_on_an_existing_intent_runs_no_command_and_writes_nothing`, `skip_does_not_hold_or_audit_a_unit_whose_needs_check_was_not_run` (TS-010); `ts004_blocked_next_is_a_result_not_an_error`; `ts006_no_prerequisite_command_runs_over_mcp_but_the_cli_runs_it`, `ts006_an_intent_cannot_be_created_over_mcp`; `ts008_next_and_report_results_are_project_relative`; `ts018_exactly_four_tools_with_contract_annotations` (no doctor or config tool) | passed |
| US-007 | AC-009 Report keeps validation, audit and concurrency | FN-006, FN-007, FN-020 | `ts010_pending_prerequisites_preview_is_read_only` (TS-010); `ts006_no_prerequisite_command_runs_over_mcp_but_the_cli_runs_it` (pendingPrerequisites after an MCP report); `ts006_lifecycle_through_mcp_matches_the_cli_state_and_audit` (audit rows equal the CLI's) | passed |
| US-008 | AC-010 Approvals over MCP follow the CLI gate policy | FN-006, FN-014 | `ts010_blank_approver_refused_over_mcp_before_any_lookup`, `ts010_cli_caller_is_not_subject_to_the_approver_guard` (TS-010); `ts004_approval_without_label_is_approver_required`; `ts006_lifecycle_through_mcp_matches_the_cli_state_and_audit` (missing or blank label refused; labelled approval equals the CLI); human-mode gate with a named approver ('Pat Owner', `tests/mcp_parity.rs:254-277`) | partial. Observed: the omitted or blank label refusal and the human-mode named-approver gate are tested. Assumption (covered by the shared facade, AC-002, no MCP test): AI-mode gate approved with 'AI Orchestrator' and a valid recommendation file; AI-mode gate refused without a recommendation; human-mode gate refusing the 'AI Orchestrator' label; policy edited between calls. See `known-limitations.md` |
| US-004 | AC-011 Errors are safe and not retried | FN-001, FN-008, FN-014, FN-015, FN-017, FN-019 | `ts007_stale_load_error_equals_state_changed`; `ts004_errors_are_classified_by_exact_equality_and_redacted`; `ts008_message_is_redacted_with_fallback`, `messages_use_generic_text_and_keys_do_not_collide`; `ts008_error_messages_never_carry_a_path`, `ts008_unusable_engine_home_warns_on_stderr_without_the_path_and_stdout_stays_pure` | passed for the cited scenarios. Open question: the 'unsafe project layout' scenario (state dir is an unsafe path or symlink, so any tool call is refused and nothing is written) has no MCP-level test and test-strategy.md does not plan one; Assumption: the shared engine refusal covers it. See `known-limitations.md` |
| US-004 | AC-012 Root fixed at launch; concurrent use safe | FN-008, FN-016, FN-019, FN-022 | `ts007_concurrent_reports_never_lose_an_update` (TS-007); `ts020_racing_persist_active_calls_land_exactly_one_write`, `ts020_held_lock_times_out_with_state_changed_without_running_f` (TS-020); `active_update_keeps_pointer_and_refuses_concurrent_changes`; `ts005_root_is_fixed_at_launch_and_never_taken_from_a_call` (TS-005); passed on Windows, macOS and Ubuntu CI | passed for MCP-vs-MCP and in-process races. Observed: `ts007_concurrent_reports_never_lose_an_update` races two MCP server processes (racer helper, `tests/mcp_parity.rs:384-410`); `ts020_racing_persist_active_calls_land_exactly_one_write` is an in-process race. No test mixes a CLI writer with an MCP writer; Assumption: the same `with_state_lock` code path covers the mixed case. See `known-limitations.md` |
| US-009 | AC-013 CLI optional and binaries install cleanly | FN-016, FN-018 | `ts016_dependency_set_is_unchanged`, `ts016_rust_version_pinned`, `ts016_both_bins_declared`, `ts016_cli_stays_the_default_run_target`, `ts016_cargo_lock_bytes_unchanged` (TS-016); `ts009_cargo_install_and_uninstall_both_binaries` (TS-009); CI "Installer tests must not skip" 6/6 with 0 skips on Windows and macOS | passed, with caveats. Observed: `ts009_*` gives the MCP binary only initialize and tools/list, not help or version, and does not assert that a settings file is unchanged (same gap as AC-014); no background process or user-level assistant settings change is evidenced by the code only |
| US-009 | AC-014 Manual host configuration and honest labels | FN-021 | `cargo test --test repo_content` README guards (36 passed, sweep log) including `ts015_ci_workflow_and_readme_mcp_section` (TS-015: section labelled proposed, four tools named, approver self-asserted, `state_conflict` retryable); TS-013 partial check (host settings unmodified) | passed (docs present and labelled proposed; the install-writes-nothing scenario is evidenced by the code and the partial TS-013 check, not by a dedicated settings-bytes test; DeepSeek Harness entry is documentation only and was never exercised) |

## Summary for the gate
- 12 of 14 criteria passed (AC-011, AC-012, AC-013 and AC-014 with labelled caveats); AC-003 and AC-010 are partial. AC-003: the DeepSeek Harness half was not run and no model-driven call was made. AC-010: no MCP test for the AI-mode gate scenarios. No criterion failed.
- Open items and limitations: `known-limitations.md`.


## Owner decision at the gate (2026-11-28)

The AC-003 row stays "partial" as observed. The owner deferred the DeepSeek Harness half ("không cần deepseek harness giờ đâu"; scope-boundary.md), so for this release AC-003 is judged on Claude Code (Connected with the real binary) plus the protocol tests and CI.
