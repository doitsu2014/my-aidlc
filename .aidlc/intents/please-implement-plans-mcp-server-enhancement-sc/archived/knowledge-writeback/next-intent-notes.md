# Next-intent notes: intent 2 (telemetry, monitor dashboard, release work)

Scope: plan phases 3-5 of `plans/mcp-server-enhancement/`. Read `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md` first.

## Where to hook in

- `rust-cli/application.rs`: the shared facade (`open(project, Caller)`, `status`, `queue`, `next`, `report`). Both the CLI and MCP pass through it, so operation telemetry belongs here. The decision record names it as the intent-2 hook (ADR-006).
- `rust-cli/mcp/tools.rs`: the four tool handlers. Add any telemetry fields at the adapter edge only if they are MCP-specific.
- `rust-cli/mcp/redact.rs`: all MCP output passes through it. Any new telemetry or dashboard payload that leaves the process must be redacted the same way.
- `rust-cli/mcp/protocol.rs`: the only writer to stdout.

## Constraints to keep

- `tests/golden/` stays byte-identical. Do not re-bless goldens to make a refactor pass.
- Zero new crates unless the owner decides otherwise (ADR-001, ADR-002). `Cargo.lock` changes need that decision. MSRV stays 1.100.
- Stdout purity for `dtech-aidlc-mcp`: nothing but protocol frames. No `println!` in library code.
- Windows and macOS CI must stay green (run 46090623346 is the reference: Windows 547/0, macOS 545/0). Keep CRLF, `.exe` and path-separator behaviour in mind.
- Tool names `status`, `queue`, `next`, `report`, no prerequisite execution over MCP, and a required explicit approver label.
- State writes go through the state lock. New writers must use the same `with_state_lock` path.

## Open items inherited

- DeepSeek Harness launch-and-list verification (AC-003 second host, TS-013). The owner deferred it; it still needs a machine with `dsh`. Also a live Claude Code `tools/call` and a Windows host launch.
- Version bump from 1.2.2 and a CHANGELOG entry (ADR-018 deferred both).
- Remove the "proposed" label from the README MCP section, once the above evidence exists.
- README platform sentence near the install section and the "launch-and-list check ... not yet run" sentence. The owner edits these now that CI is green on both OSes.
- Install-script support for the MCP binary (N2 kept scripts CLI-only).
- Test gaps: AC-010, AC-011, AC-012, AC-013 and AC-014 over MCP. Timing flakes. `actions/checkout` Node 20 upgrade. Pre-existing clippy warning at `tests/orchestration_golden.rs:1108`.
- `deps add` has no state lock (FN-022). Fix it if intent 2 adds writers.

## Lessons from this intent

- Reviewer rounds caught real bugs that tests missed at first: a hang in redaction, a lost update between concurrent writers (which led to the state lock and the ADR-015 supersession), and a Windows CRLF hash mismatch. Keep an independent review step on each work unit.
- Worker briefs must quote the approved design names (ids, tool names, error codes, file paths) and not paraphrase them. Drift between brief and design cost rework.
- A concurrency decision accepted on paper ("document the window") failed once measured. Test race claims early.
- A CI run on the real target operating systems found issues local macOS runs could not. Push early when platform support is a requirement.
- Record owner decisions in the artifacts. `audit.jsonl` has no rows for them.
