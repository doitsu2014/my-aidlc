# Red evidence: WU-003

- Status: Complete (red-tests, 2026-11-28). Test-first: yes for the Q18 approver guard (security/gate integrity) and the MCP option resets; CLI routing is covered by the existing goldens (parity), where a separate red run adds nothing.

## Initial red run (worker worktree)
- Facade complete except the Q18 guard stubbed off (`if false && ...`) so the test compiled and executed: `cargo test --lib application` -> 3 passed, 1 failed. `ts010_blank_approver_refused_over_mcp_before_any_lookup` panicked at application.rs:150: left "No active workflow state", right "approved_by is required for a gate decision over MCP (no git or OS fallback)". Without the guard the blank approver reached the state read.

## Review round (fix worker; re-reviewer repeated the mutations)
- The independent review found the original `ts010_mcp_next_cannot_create_an_intent` passed for the wrong reason (uninitialized project). Rewritten on an initialized project and proven by mutation:
  - (a) remove `options.intent = None` -> `ts010_mcp_next_cannot_create_an_intent` FAILED (unwrap_err panicked: MCP next went on to create/continue an intent).
  - (b) remove `options.skip_prerequisite_checks = true` -> `ts010_mcp_next_skips_prerequisite_checks_and_cli_runs_them` FAILED at the marker/executed assertion (a prerequisite command ran over MCP).
  - (c) remove `options.mode = None` or `options.force_resume = false` -> the skip test FAILED.
- All restored; 5 facade tests pass.
