# Failing tests: WU-006 (Path redaction)

- Status: Complete (red-tests, 2026-11-28). Cites TS-008, FN-015, IF-007, AC-006, AC-008, AC-011; TH-11.
- Kind: code manifest. All tests in `rust-cli/mcp/redact.rs` (`#[cfg(test)]`).

| Test | Test id | Red observed |
|---|---|---|
| `ts008_longest_prefix_wins_and_is_relative` | TS-008 | yes (identity stub): `"/a/b/c/d.txt"` vs `"c/d.txt"` |
| `ts008_boundary_is_respected` | TS-008 | yes: `"/a/bc/x"` vs `"<redacted-path>"` |
| `ts008_windows_forms_on_any_os` | TS-008 | yes: `C:\Users\me\proj\src\x.rs` vs `src/x.rs` |
| `ts008_remaining_absolute_and_non_paths` | TS-008 | yes (identity stub) |
| `ts008_macos_var_and_private_var_both_covered` | TS-008 | yes: unredacted `/var/folders` object |
| `ts008_message_is_redacted_with_fallback` | TS-008 | yes (identity stub) |
| `rewrite_terminates_on_relative_prefixes` (review round 1) | TS-008 | yes: hang on prefix `.` (killed after 60 s) |
| `relative_engine_home_is_resolved_not_registered_raw` (review rounds 1-2) | TS-008 | yes: relative `engine` registered raw over-redacted "the engine failed" |
| `paths_after_backtick_colon_and_file_url` (review round 1) | TS-008 | yes: "cannot read `/Users/me/x`" left unredacted |
| `messages_use_generic_text_and_keys_do_not_collide` (review rounds 1-2) | TS-008 | yes: message kept a path token; colliding keys dropped a value |
| `ts008_second_path_after_colon_or_equals_is_not_swallowed` (conductor, integration) | TS-008 | yes: `"PATH=bin:/usr/secret/bin"` vs `"PATH=bin:<redacted-path>"` |
