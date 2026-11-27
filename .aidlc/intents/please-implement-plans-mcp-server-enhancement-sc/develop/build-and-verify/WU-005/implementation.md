# Implementation: WU-005 (MCP tool adapter)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-005, FN-013, FN-014, FN-016, IF-006, CFG-005, CFG-006, TS-004, TS-018; ADR-008, ADR-009, ADR-010, ADR-011, ADR-013, ADR-015.
- Kind: code manifest.

| File | Change |
|---|---|
| `rust-cli/mcp/tools.rs` (new) | Exactly four tools, `status`, `queue`, `next`, `report` (no doctor/config tool), each with a JSON Schema (`additionalProperties: false`, string limits, id patterns, enums `complete|blocked|skipped` and `approved|rejected` read once from the CLI parser) and annotations (`readOnlyHint`, `destructiveHint`, `openWorldHint: false`). `call_tool`: unknown tool -> None (protocol answers -32602); arguments validated before `application::open(root, Caller::Mcp)`; results and errors built by `finish()`, which redacts every value and key (`redact_value`) and every error message (`redact_message` with the code's generic text, then capped at 1000 characters). Errors classified by exact equality (`STATE_CHANGED` -> retryable `state_conflict`, `APPROVER_REQUIRED` -> `approver_required`, validation -> `invalid_arguments`, others -> `operation_failed`) and returned as tool results with `isError: true`. A Blocked `next` is a result with outcome `blocked`, not an error. The `report` description says the approver label is self-asserted, not authentication, and that `pendingPrerequisites` is a preview. |
| `rust-cli/mcp/mod.rs` | `pub mod tools;`; `run` builds the `Redactor` from the launch root, writes a stderr warning (path redacted) when `DTECH_AIDLC_HOME` is set but unusable, and serves `ToolHost<ToolContext>`; module doc updated. |
| `rust-cli/prerequisites.rs` (test only) | `ts010_skip_runs_no_command` compares the two real runs without the millisecond `startedAt` field (flake observed once under load). No product code. |

Recorded deviations: the mod.rs edit is about 15 lines (FN-016 startup wiring re-assigned from WU-004 to WU-005 by the conductor) rather than one line; length limits count characters (JSON Schema `maxLength`), not bytes; `approved_by` `minLength: 1` applies to the raw string, so a whitespace-only label passes validation and the facade returns `approver_required`; the test-only change in `prerequisites.rs` (owned by WU-002) was authorised by the conductor to remove a flake.

Codebase map updated (tool adapter wired; the server is now functional).
