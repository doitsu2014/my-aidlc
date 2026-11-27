# Implementation: WU-004 (MCP protocol layer)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-004, FN-010, FN-011, FN-012, FN-016, IF-005, CFG-005, CFG-006, TS-001, TS-002, TS-003; ADR-001, ADR-004, ADR-005.
- Kind: code manifest.

| File | Change |
|---|---|
| `rust-cli/mcp/protocol.rs` (new, 711 lines incl. tests) | Hand-rolled dual-era JSON-RPC 2.0 over stdio, serde_json + std only. `read_frame`: newline-delimited, 1 MiB content cap (CFG-006) measured after stripping LF and an optional CR, oversized lines drained and answered -32600, CRLF accepted, blank lines skipped, invalid UTF-8 answered without ending the session. `handle`: envelope checks (id echoed when valid, batch/null id -32600), per-request era detection (ADR-005 rule 4: modern when `_meta` carries the protocol-version key; legacy `initialize` negotiation over 2025-11-25, 2025-06-18; modern 2026-07-28 with `_meta` validation, -32602 and -32022 `{supported, requested}`), `server/discover` with the exact DiscoverResult (no top-level serverInfo), modern stamping (`resultType`, `ttlMs:0`, `cacheScope:"private"`, `_meta` serverInfo) only on modern results, legacy-only `ping`, notifications never answered, -32601 otherwise, `catch_unwind` per request returning -32603 "Internal error" with no detail. `serve(input, output, &ToolHost<C>, &ctx)`: single writer, compact JSON + newline + flush, exit 0 on EOF (1 on a stdout write error). |
| `rust-cli/mcp/mod.rs` | `pub mod protocol;`; `run(Vec<OsString>)` parses only `--project <dir>` (else cwd), checks the launch root, serves an empty tool table until WU-005 wires the adapter (marked `// WU-005:`); startup errors go to stderr only. |

Deviations (recorded): `ToolHost` is generic `ToolHost<C>` with the context passed to `serve`, because `ToolContext` belongs to WU-005 (IF-005 shape otherwise as designed). The DTECH_AIDLC_HOME startup warning and the startup Redactor (FN-016/CFG-005) are re-assigned to WU-005 by the conductor, because they need WU-003's `application::external_engine` and WU-006's `redact.rs`; WU-005 may edit the host/context block of `mcp/mod.rs` for that wiring. Message texts the design left open are frozen by tests (for example "Parse error", "Invalid request: line exceeds 1048576 bytes", "Method not found: <m>"). Modern field names were checked by the independent reviewer against the saved 2026-07-28 schema.json (all match).

Codebase map updated (protocol module).
