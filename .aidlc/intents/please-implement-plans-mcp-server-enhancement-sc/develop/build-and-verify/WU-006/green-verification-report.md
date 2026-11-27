# Green verification report: WU-006

- Status: Complete (build-and-verify, 2026-11-28). Integrated main tree, base `6b60591`.

## Results per test level (Observed)
- Focused: `cargo test --lib mcp` -> 27 passed (16 protocol + 11 redact), 0 failed.
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast`
  - run 1: 493 passed, 1 failed: `prerequisites::tests::check_timeout_is_reported` (pre-existing test from commit 613c7e46, not touched by this unit; races `sleep 1` against a 1 s timeout; machine load average ~4.8 while another unit built in parallel). Re-ran it alone 5 times: 5 passed.
  - run 2 (same tree): 30 targets, **494 passed, 0 failed** (baseline 483/0; +11 redact tests).
- `cargo fmt --check` clean; clippy: only the known pre-existing warning. No stdout writes, no process spawning in `redact.rs`.
- Not run: Windows execution of the Windows-form tests (they run on any OS as string tests; real Windows paths on the WU-008 CI leg).

## Quality requirements verified
- Privacy (AC-006, AC-011, TH-11): registered prefixes become relative/symbolic; unregistered absolute paths never survive in messages; values get `<redacted-path>`.
- Robustness: the scan terminates for degenerate prefixes (`.`, `/`, `//`, `\\?\`).

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-005 | AC-006 | FN-015 | `ts008_longest_prefix_wins_and_is_relative`, `ts008_boundary_is_respected`, `ts008_windows_forms_on_any_os`, `ts008_macos_var_and_private_var_both_covered`, `ts008_remaining_absolute_and_non_paths`, `ts008_second_path_after_colon_or_equals_is_not_swallowed`, `paths_after_backtick_colon_and_file_url` (TS-008) | passed |
| US-006 | AC-008 | FN-015 | `ts008_longest_prefix_wins_and_is_relative` (directive paths become project-relative) (TS-008) | passed |
| US-004 | AC-011 | FN-015 | `ts008_message_is_redacted_with_fallback`, `messages_use_generic_text_and_keys_do_not_collide`, `rewrite_terminates_on_relative_prefixes`, `relative_engine_home_is_resolved_not_registered_raw` (TS-008) | passed |
