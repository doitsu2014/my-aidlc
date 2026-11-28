# Gate recommendation: analysis-baseline (intent 3, monitor --port)

- Gate: analysis-baseline (Analyze, fast depth: problem-framing only), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-28
- Owner decisions (User-provided, 2026-11-28): fast depth; fold into unreleased 1.3.0; same branch; scoped golden re-bless for the monitor/--port help, completion and validation lines only.

## Researched options

### A. Approve after three text fixes (selected)
Trade-offs: For: the ACs are testable after the reviewer's fixes; the only open choice (OQ-1, parser effect on other commands' error text for a value-less `--port`) follows the existing convention for every command-specific option. Against: the conductor, not the owner, settles OQ-1 (disclosed as D5).

### B. Block for the owner's written decision on OQ-1
Trade-offs: For: the owner sees the stderr change on invalid `status --port ...` invocations. Against: stalls a fast intent on a choice the codebase already made for `--depth`, `--intent` and others; exit codes are unchanged.

### C. Make the parser command-aware so `--port` stays out of the global table
Trade-offs: For: no change to other commands at all. Against: rewrites shared parse ordering for every command, with golden risk outside the owner's scoped re-bless. Rejected.

## Selected direction and rationale

Option A. Fixes applied by the conductor: AC-007 golden list adds `routing_golden/unsafe.json` with the `--word-diff=porcelain` check; OQ-1 recorded as decision D5; OQ-2 Windows wording corrected (address sharing needs the same OS user, T2-06). Analyze exit criteria: problem and outcomes stated (problem-statement.md, AC-001..AC-010, US-001, US-002); constraints, risks and open questions recorded in problem-statement.md (fast depth has no context-and-risk stage); owner decisions reflected; AI approval under the contract.

## Evidence trace
- Observed: rust-cli/monitor/mod.rs:23-29 (bind 127.0.0.1:0, startup line), rust-cli/monitor/http.rs Host check from the bound port, rust-cli/args.rs merged flag table, rust-cli/main.rs:66-74 allowed-options text, tests/golden/routing_golden/validation.json monitor entries.
- Observed: independent review found 13 issues, all fixed in the two artifacts.
- User-provided: owner decisions above.

## Residual risks
- D5 changes stderr (not exit codes) of invalid `status --port ...` invocations; disclosed and to be tested.
- Windows address-sharing analysis is from documentation, not tested.
- With a well-known fixed port another local user can bind it first; the monitor then refuses to start and prints no token.

## Boundary
Approves the problem framing only. No code, golden, README or CHANGELOG change is authorised outside the Develop work units; no push, merge or tag.

## Open items
- [follow-up] D5: disclose and test the changed stderr for `status --port`, `status --port=`, `status --port --intent x`, `status --port 1 --wat`, `status --port 1 --port 2`.
- [follow-up] Pass the `--port` value from main.rs to monitor::run and validate it before the telemetry check, token and bind (D4).
- [follow-up] Windows: record same-user address sharing as a residual next to T2-06, or set SO_EXCLUSIVEADDRUSE via std-only FFI (design decides).
- [follow-up] Threat note and README: a well-known fixed port can be pre-bound by another local user (monitor refuses to start, no token printed).
- [follow-up] A fixed port does not make a bookmark self-sufficient (token per run); a persistent token is a separate security decision.
- [follow-up] Low ports and Windows excluded port ranges are documented, not tested.
- [follow-up] Refresh the codebase map for monitor/telemetry at knowledge-writeback.
