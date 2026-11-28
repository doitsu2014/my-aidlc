# Implementation: WU-014 (MCP telemetry wiring)

- Status: Complete (build-and-verify, 2026-11-28). Cites FN-029, FN-030, FN-031, IF-015, IF-016, CFG-007, ADR-022, ADR-024, ADR-025.
- `rust-cli/mcp/mod.rs`: `[--project <dir>] [--telemetry on|off]` in any order, default on, duplicates and other values refused; starts `telemetry::Sink` only when on; drains it (500 ms) after `serve` returns.
- `rust-cli/mcp/protocol.rs`: `ToolHost.on_error: fn(&C, &'static str)` called once per error reply after write and flush (not when the write fails); `error_code_name` maps JSON-RPC codes to fixed names (`line_too_long` for oversized frames). Reply bytes unchanged.
- `rust-cli/mcp/tools.rs`: `traced_call` adapter emits tool started/completed and operation events, and workflow events (`stage_selected`, `stage_reported`, `gate_decided`) only after a durable success; ids for reported events come from the report result (`intentId`, `stage`, `unit`); `on_protocol_error` records `mcp.protocol.error`; only fixed names and validated ids reach telemetry. `now_ms` reused from `telemetry::sink` (made `pub(crate)`).
- Tests: existing snapshot spawns pass `--telemetry off` (`run_mcp_env` in tests/fixtures/mcp/common.rs, tests/mcp_stdio.rs, tests/mcp_parity.rs); new `run_mcp_args`; new tests/mcp_telemetry.rs.
- Conductor fix: the failing-store test asserts exactly the unsafe-location line (a regular file at `.aidlc/telemetry` is unsafe on every platform; Observed by the re-reviewer).
- Confirmed (design open point): next-directive ids are `result.intentId`, `result.stage.id`, `result.currentUnit`.
