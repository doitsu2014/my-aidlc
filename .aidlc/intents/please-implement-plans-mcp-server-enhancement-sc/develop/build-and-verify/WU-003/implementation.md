# Implementation: WU-003 (Facade and CLI routing)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-003, FN-001..FN-006, FN-009, IF-001, TS-010, TS-011; ADR-006, ADR-007, ADR-013, ADR-016.
- Kind: code manifest.

| File | Change |
|---|---|
| `rust-cli/application.rs` | The shared facade (IF-001): `Caller {Cli, Mcp}`, `Project {root, graph, engine, caller}`, `open(project, caller)` (resolve root -> `assert_safe_state_dir` -> `external_engine()` -> `read_graph`), `status`, `queue`, `knowledge_preflight`, `next`, `report`, `external_engine()` / `external_engine_from(Option<&OsStr>)` (DTECH_AIDLC_HOME, used by WU-005). For `Caller::Mcp`: `next` runs the knowledge preflight, forces `intent/scope/depth/mode/approved_by/detail` to None, `force_resume` false and `skip_prerequisite_checks` true (MCP cannot create an intent or run shell checks, Q5); `report` refuses an approval whose approver label is missing or whitespace-only with `APPROVER_REQUIRED`, before any state read or identity lookup (Q18), and adds `pendingPrerequisites` (AC-009). No stdin, stdout or exit codes in the facade. |
| `rust-cli/main.rs` | `status`, `orchestrate queue`, `orchestrate next` and `orchestrate report` route through the facade (`Caller::Cli`); the exit-code-3 mapping for Blocked stays in main.rs; park/revisit stay on `run_unit_queue`. Error order and output unchanged. |

Review history: worker patch -> review (a test passing for the wrong reason, no facade test for the MCP skip/resets, queue not routed) -> fix round -> re-review ok. The re-reviewer compared `orchestrate queue` stdout/stderr/exit code between the HEAD and patched binaries (identical) because no golden runs that command.

Codebase map updated (facade now exists).
