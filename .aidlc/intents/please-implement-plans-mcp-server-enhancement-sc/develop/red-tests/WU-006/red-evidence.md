# Red evidence: WU-006

- Status: Complete (red-tests, 2026-11-28). Test-first: yes (privacy boundary: no absolute path in MCP output, AC-006/AC-011, TH-11).

## Initial red run (worker worktree)
- Stub `redact.rs` (identity rewrite, no-op `redact_value`, unchanged message) with the six `ts008_` tests: `cargo test --lib mcp::redact` -> 6 failed / 0 passed, all assertion failures (examples in failing-tests.md).

## Review round 1 (fix worker; reproduced independently by the reviewer)
- `rewrite_terminates_on_relative_prefixes`: never finished (infinite loop on prefix `.`, killed after 60 s). With `DTECH_AIDLC_HOME=.` the server would have hung and grown memory.
- `paths_after_backtick_colon_and_file_url`: left "cannot read `/Users/me/x`", right "cannot read `~/x`" (leak).
- `messages_use_generic_text_and_keys_do_not_collide`: a message kept a path token; a colliding object key overwrote another value.

## Review round 2
- Message redaction changed to ADR-010 step 4 (whole message becomes the code's generic text when any unregistered absolute path remains); the old behaviour produced "cannot read [path]: no" and leaked "Disk/secret" from a path with a space. Red was not captured as a separate run for this assertion; the re-reviewer confirmed the new assertions fail against the round-1 code.

## Integration (conductor)
- `ts008_second_path_after_colon_or_equals_is_not_swallowed`, written before the fix: `cargo test --lib second_path_after` -> FAILED, left "PATH=bin:/usr/secret/bin", right "PATH=bin:<redacted-path>" (a second absolute path joined by `:` or `=` after a registered prefix leaked). Passes after the fix.
