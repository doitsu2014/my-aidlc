# Quality requirements

- Purpose: measurable quality requirements for intent 1 of 2 (shared facade plus MCP stdio server).
- Status: Draft - pending analysis-baseline gate
- Sources: acceptance-criteria.md (AC-001..AC-014); risk-register.md; evidence-log.md. Targets marked Estimate have no baseline (Assumption A6, owner confirms). No numeric target is Observed.

## Performance
- Measure: wall time of a status, queue, next or report call through the MCP server on an equivalent project, and CLI command runtime before versus after the refactor.
- Target: Estimate: no regression beyond noise for CLI commands (goldens identical); MCP read calls return interactively, with a documented per-call bound. A concrete millisecond figure is an Open question for Ideate (N6, decision-facilitator); the docs give no baseline. Prerequisite checks are not run by MCP (AC-008), so no call blocks on owner shell commands.
- Verified by: timing comparison in the child-process test and CLI test run; manual review of the documented bound.
## Availability
- Measure: server starts in each supported host (Claude Code and DeepSeek Harness), completes the handshake, lists the four tools, and recovers after shutdown and restart; failure of one call does not end the session.
- Target: 100 percent of the child-process session scenarios pass (AC-003, AC-004, AC-011); state and audit are byte-identical across restart. The server is a local, host-launched process; uptime or SLA is not applicable.
- Verified by: child-process integration test (initialize, list, valid call, schema error, domain error, shutdown, restart) and concurrency interleaving test (AC-012).
## Privacy
- Measure: count of absolute filesystem paths, secrets, stack traces or file contents in any MCP success value or error text; count of network listeners; changes to user-level assistant settings.
- Target: zero in each case (AC-004, AC-006, AC-011, AC-014). Approver names are described as self-asserted, not authenticated (AC-010). Telemetry is out of scope (intent 2), so no telemetry privacy target applies here.
- Verified by: recursive path-scan test over nested values and error strings (including macOS /var versus /private/var and Windows forms); socket check of the process; settings-file byte comparison in the install smoke test.
## Accessibility
- Measure: not applicable.
- Target: not applicable. This intent delivers a CLI and a stdio protocol server with no user interface; the dashboard (mcp-studio-server.html) is intent 2. Error text should still be plain and actionable (AC-011).
- Verified by: not applicable.
## Operability
- Measure: CLI behaviour parity (existing suite pass/fail set and goldens), install smoke result, documentation completeness, and diagnosability from stderr.
- Target: identical test pass/fail set before and after in the agreed environment (TMPDIR under /private/tmp, binary on PATH) with no golden re-bless (AC-001); both binaries install, print help and version, and uninstall from outside the checkout (AC-013); a dependency footprint where the CLI binary keeps only serde_json and sha2 and `--locked` offline install still passes (Q3); copy-ready host-config examples and troubleshooting exist, with MCP labelled proposed until released (AC-014); startup failures print to stderr and exit non-zero (AC-004).
- Verified by: `cargo test --no-fail-fast` before and after (baseline Observed: 450 passed, 6 failed here, all listed in PROJECT.md); install smoke test; documentation review.

## Revision note
- 2026-11-27 (attempt 2): Availability names both hosts (Q1); Operability adds the dependency-footprint target (Q3). Owner decisions are User-provided.
