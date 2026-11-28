# Gate recommendation: release-candidate (intent 3, monitor --port)

- Gate: release-candidate (Develop), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-28
- Candidate: e319bac on feature/please-implement-plans-mcp-server-enhancement-sc, folded into the unreleased 1.3.0 (owner decision).

## Researched options

### A. Approve with follow-ups only (selected)
Trade-offs: For: every AC passes with tests, CI is green on all three OSes, the golden change is inside the owner's scope, an independent review passed and its findings were fixed. Against: D5 (stderr change for invalid `status --port ...` invocations) is an AI decision, disclosed and pinned by tests.

### B. Block for the owner's written decision on D5
Trade-offs: For: the owner confirms the stderr change. Against: D5 follows the existing convention for every command-specific option, exit codes are unchanged, no golden covers it, and the Analyze gate already carried it as a follow-up; blocking adds latency without changing the decision quality.

## Selected direction and rationale

Option A. Develop exit criteria: implementation matches the plan and ACs; tests written first and shown red; local suite 670 passed / 0 failed / 1 ignored (baseline 657/0); CI run 46124668134 green on windows-latest (706/0), macos-latest (715/0) and ubuntu-latest (715/0); Cargo files unchanged; golden word-diff limited to the monitor/--port lines in the three owner-scoped files; README and CHANGELOG updated; rollback is one revert.

## Evidence trace
- Observed: `develop/red-tests/red-evidence.md`, `develop/build-and-verify/green-verification-report.md`, CI run 46124668134, conductor mutation check (fallback bind fails in 10.02 s).
- User-provided: fast depth, fold into 1.3.0, same branch, scoped re-bless, push for CI (2026-11-28).

## Residual risks
- D5 stderr change for invalid invocations of other commands (exit codes unchanged).
- Same-user address sharing on Windows and pre-binding of a well-known port by another local user (T2-06 class).
- Loopback-only bind guarded by review, not by a test.

## Boundary
Approves the candidate on the feature branch only. Does not authorise a merge, PR, tag or publish beyond what the owner already chose (fast-forward `main` after this intent closes).

## Open items
- [follow-up] D5: owner may ask to keep `--port` out of the merged flag table (parser rewrite) if the changed stderr for invalid `status --port ...` matters.
- [follow-up] Windows: SO_EXCLUSIVEADDRUSE not set; same-user address sharing recorded next to T2-06.
- [follow-up] README documents low-port privileges and Windows excluded port ranges; not tested.
- [follow-up] A persistent token for bookmarks would be a separate security decision.
- [follow-up] Add a loopback-only bind test on Linux/macOS when convenient.
