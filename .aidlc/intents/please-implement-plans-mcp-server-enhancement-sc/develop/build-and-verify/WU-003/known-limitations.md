# Known limitations: WU-003

- The scope, depth and approved_by resets in `application::next` (MCP) are defence in depth: with `intent` forced to None they are never consulted, so no test can observe them. The intent, mode, force_resume and skip resets are pinned by mutation-proven tests.
- The knowledge-preflight call on the MCP branch of `next` is not pinned by a facade test (the CLI path keeps calling it from main.rs). WU-005/WU-007 exercise it end to end.
- `pendingPrerequisites` on a successful MCP report is not unit-tested here; covered end to end by WU-005/WU-007.
- `status` builds a `Project` with `Engine::embedded(root)` even when DTECH_AIDLC_HOME is set; `status` never reads the engine, so output is unaffected (cosmetic).
- No golden runs `orchestrate queue`; byte-compatibility was checked once by comparing binaries.
- Rollback: revert this unit's commit.
