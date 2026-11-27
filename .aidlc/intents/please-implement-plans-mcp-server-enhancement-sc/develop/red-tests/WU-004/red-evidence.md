# Red evidence: WU-004

- Status: Complete (red-tests, 2026-11-28). Test-first: yes (public protocol contract, ADR-004/ADR-005; stdout purity AC-004).

## Initial red run (worker worktree)
- Tests written first against a stub `serve` returning 0 and writing nothing (`ToolHost` and signatures present, so tests compile and execute).
- `cargo test --lib mcp::protocol`: `test result: FAILED. 2 passed; 11 failed`. The two passes expect no output (empty stdin, notifications). The 11 failures are assertion failures (`left: []` against the expected exact frames) or index-out-of-bounds panics on the empty output: behavioural, not compile errors.

## Review-round red run (fix worker)
- `cargo test --lib mcp`: 3 failures before the fix:
  - `ts003_envelope_errors_and_notifications`: ids for the missing-`jsonrpc` and `jsonrpc: "1.0"` cases came back `null` instead of `1` and `2` (FN-011: echo a valid id).
  - `parse_args_accepts_only_project`: `Err("unexpected argument: --project")` instead of `Err("unexpected argument: x")`.
  - `ts003_cap_boundaries`: a line of exactly 1048576 content bytes ending in CRLF returned -32600 "line exceeds 1048576 bytes" instead of being read (Windows senders got a cap one byte smaller).
- After the fix: 16 mcp tests pass.
