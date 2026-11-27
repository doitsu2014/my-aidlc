# Failing tests: WU-004 (MCP protocol layer)

- Status: Complete (red-tests, 2026-11-28). Cites TS-001, TS-002, TS-003, FN-010, FN-011, FN-012, FN-016, IF-005, CFG-006, AC-003, AC-004, AC-005.
- Kind: code manifest. All tests in `rust-cli/mcp/protocol.rs` (`#[cfg(test)]`), plus two argument tests in `rust-cli/mcp/mod.rs`.

| Test | Test id | Red observed against the stub `serve` |
|---|---|---|
| `ts001_legacy_session_exact_frames` | TS-001 | yes: `left: []` vs expected legacy frames |
| `ts001_initialize_version_negotiation` | TS-001 | yes: index out of bounds on empty output |
| `ts001_before_initialize_without_key` | TS-001 | yes: index out of bounds |
| `ts002_modern_exact_frames` | TS-002 | yes: `left: []` |
| `ts002_modern_meta_validation` | TS-002 | yes: index out of bounds |
| `ts002_era_mixing_in_one_process` | TS-002 | yes: index out of bounds |
| `ts003_envelope_errors_and_notifications` | TS-003 | yes: `left: []`; review round: ids null instead of echoed for missing/wrong `jsonrpc` |
| `ts003_parse_errors_crlf_blank_lines_and_eof` | TS-003 | yes: `left: []` |
| `ts003_invalid_utf8_keeps_session_alive` | TS-003 | yes: `left: []` |
| `ts003_oversized_line_is_drained_and_session_continues` | TS-003 | yes: `left: []` |
| `ts003_cap_boundaries` (review round) | TS-003 | yes: exact-cap content + CRLF answered -32600 instead of being accepted |
| `ts003_panic_is_recovered_without_detail` | TS-003 | yes: index out of bounds |
| `ts003_empty_stdin_exits_zero_with_no_output` | TS-003 | passed (expects no output; stub also writes none) |
| `ts003_notifications_are_never_answered` | TS-003 | passed (expects no output) |
| `mcp::tests::parse_args_accepts_only_project` (review round) | supporting (FN-016) | yes: reported `--project` instead of the stray token |
