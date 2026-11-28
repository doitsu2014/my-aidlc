# Implementation: WU-015 (Missing intent-1 MCP tests)

- Status: Complete (build-and-verify, 2026-11-28). Cites FN-044, TS-033, AC-027, AC-029, ADR-036.
- New `tests/mcp_followups.rs` with a long-lived `Session` driver (`--project <Tmp> --telemetry off`) and the 8 `ts033_` tests listed in red-tests. No product code changed; `tests/fixtures/mcp/common.rs` reused unchanged.
- Conductor fixes from review: the mid-line EOF case now pins the real behaviour (the unterminated fragment gets one `-32700` Parse error reply with a null id, exit 0; the worker's note that the fragment writes nothing was wrong); the sequential interleave test asserts the MCP `next` sees the CLI's report detail; a losing CLI run in the race must exit 1, never crash.
