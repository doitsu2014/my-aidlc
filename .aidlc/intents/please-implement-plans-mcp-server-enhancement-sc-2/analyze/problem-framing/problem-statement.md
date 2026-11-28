# Problem statement: intent 2, local telemetry, monitor dashboard and release

Purpose: frame the problem for the remaining scope of `plans/mcp-server-enhancement/` and record open decisions without choosing a solution.
Status: Draft - pending analysis-baseline gate
Sources: `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/intent.md`; `plans/mcp-server-enhancement/mcp-server-observability-implementation-plan.md` (plan); `mcp-server-architecture.md` (arch); `mcp-server-detailed-design.md` (design); `mcp-studio-server.html`; `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md`; `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/archived/knowledge-writeback/next-intent-notes.md`; intent-1 `decision-record.md` (ADR-001..018) and `scope-boundary.md`; `Cargo.toml`; `rust-cli/application.rs`, `rust-cli/main.rs`, `rust-cli/mcp/*`, `rust-cli/knowledge.rs`; `tests/repo_content.rs`, `tests/mcp_packaging.rs`; `README.md`; `.github/workflows/ci.yml`.
Provenance labels: Observed, User-provided, Assumption, Estimate, Open question.

## Request in the project's words
- User-provided (owner, 2026-11-28, `intent.md`): intent 2 of 2 is everything left in `plans/mcp-server-enhancement/` after intent 1: plan phases 3-5 plus related parts of the architecture, detailed design and `mcp-studio-server.html`. Three areas: (1) local telemetry pipeline (event schema, storage, retention, redaction, instrumentation of the shared facade and MCP tools); (2) read-only, loopback-only monitor command and dashboard; (3) hardening and release (version bump, CHANGELOG, README updates including removing "proposed", install-script support).
- User-provided: the owner kept this as ONE intent, not split. Intent 1 decisions stand unless the owner changes them.

## Problem
- Observed (plan s1, arch s1/s3): calls to the shared operations and the MCP tools leave no local trace. People who drive AIDLC through an MCP-capable assistant, and maintainers, cannot see which tool calls happened, which were slow, blocked or failing, or what sequence led to a failure or stage transition.
- Observed (ADR-018): intent 1 left an unreleased second binary and an MSRV pin (`rust-version = 1.100`) with no version bump, no CHANGELOG entry, a README section still labelled "proposed", and install scripts that install only the CLI.
- Why now: intent 1 is closed and shipped the facade and server that telemetry hooks into (Observed, knowledge record).

## Current state (Observed)
- `Cargo.toml`: version 1.2.2, two binaries, dependencies only `serde_json` and `sha2`. No network code exists in `rust-cli/`.
- The facade has four operations (status, queue, next, report) with `Caller::{Cli,Mcp}`. Park, revisit and intent pause/resume/abandon bypass it.
- The audit log is a separate append-only record and is not an event source.

## Desired outcome
- Optional, local, metadata-only telemetry; a separately started read-only loopback dashboard answering the plan's three questions (what events and how fresh; what is slow/blocked/failing; what sequence led to a failure); a released version with accurate docs and installers for both binaries. See `success-measures.md`.

## Non-goals (Observed, plan s1, arch s4, scope-boundary s3)
No remote/OTLP exporter or cloud relay; no HTTP MCP transport; no capture of prompts, raw arguments/results, `detail`, artifact content, credentials, env values, free text or unredacted absolute paths; telemetry is not audit and never proves a gate decision; dashboard is not a control panel; no workflow DB or schema change to phase graph, state or audit; no automatic host-config edits; no third public binary; no browser auto-open, remote fonts or external scripts; no daemon; no process start/stop events; no pause/resume/abandon events. DeepSeek Harness verification is deferred by the owner (User-provided, 2026-11-28) and is out of scope unless the owner re-adds it.

## Depth proposal
- Directive depth is `standard` (Observed, directive). Assumption: acceptable because `standard` lists all 13 stages in the current graph (PROJECT.md); validate at the gate.

## Open decisions (none resolved here)
| # | Open question | Owner | Why it matters (Observed) |
|---|---|---|---|
| Q1 | Telemetry default on or off, and for which callers (MCP only or CLI too); mechanism (flag, config key, env var). | Owner (privacy decision) | Plan s3.7/s8.1 proposes ON "do not finalize by implication". Golden walkers in `tests/rust_intent_commands.rs` and `tests/routing_golden.rs` snapshot everything under `.aidlc`, so default-on CLI writes would change snapshots (Assumption, validate by running goldens with telemetry on). Intent 1 shipped no `--telemetry` flag; `tests/mcp_stdio.rs` refuses unknown args. Plan says "config override" while also proposing no config/state schema change. |
| Q2 | Storage location and golden policy: may owner-scoped re-blesses be limited to (a) the managed `.gitignore` block line in the two lifecycle goldens, (b) help/completion/validation goldens for a new command, (c) the seven version-bearing goldens at release; or should test walkers ignore the telemetry directory? | Owner | Intent-1 rule "tests/golden byte-identical, do not re-bless". The managed block literal is embedded in `lifecycle.json` and `lifecycle-embedded.json`. |
| Q3 | HTTP server with zero crates (std-only, hand-rolled, extending ADR-001) or a new dependency (Cargo.lock change)? Same for time formatting, ID generation, per-run token entropy. | Owner | `tests/mcp_packaging.rs` pins dependency names and the Cargo.lock hash. Assumption: std-only is feasible; validate with a spike. |
| Q4 | `monitor` subcommand vs CLI invariants: list it in `help.txt` and re-bless help goldens, or keep it unlisted (then README `dtech-aidlc monitor` lines fail the content test unless amended)? How is the hidden child mode kept out of help, completion and the flag table? Is the name `monitor` final (plan calls it provisional; design treats it as contract)? | Owner | `tests/repo_content.rs` requires every README command and registered flag to appear in help. |
| Q5 | Version number (patch, minor, other) and whether intent 2 only prepares or also tags/publishes. | Owner | Bump touches Cargo.toml, Cargo.lock, badge, CHANGELOG heading, seven goldens and the pinned lock-hash test. |
| Q6 | Retention numbers (5 MiB/file, 7 days, 50 MiB total are proposed starting defaults), behavior at the file cap (new file or stop), pruning moment, Windows file permissions. | Owner / ops review | Design s10 item 5; rotation behavior is unspecified in the docs. |
| Q7 | Monitor per-run access token: yes/no, how minted without a crate, how delivered without URL/file/telemetry/MCP output; is Host/Origin allowlist plus loopback enough? | Owner + threat-model review | Plan s8.6, design s10.7; mandatory design gate before release. |
| Q8 | Which events ship: park/revisit events need those commands routed through the facade (CLI refactor with golden risk) or must be dropped from the v1 vocabulary; which CLI-only events exist, if any. | Owner / design | Plan test demands a producer hook for every v1 event value. |
| Q9 | Error and outcome mapping: telemetry `errorCode` needs a stable allowlist but only four coarse codes exist; does this trigger the deferred typed error item (ADR-008, scope-boundary s4)? How do "complete" and "busy" map to ok/blocked/error? Is widening the protocol host to emit protocol-error events accepted? | Design / owner | Plan schema has three outcomes only. |
| Q10 | Evidence required to remove "proposed" with DeepSeek waived: live Claude Code `tools/call`, Windows host launch? Who edits the README platform sentence and the "not yet run" sentence (notes say the owner)? | Owner | Plan AC 13 keeps the label until implemented AND released. `tests/repo_content.rs` requires the exact heading and the word "proposed", so that test changes in the same unit. |
| Q11 | Install scripts: both binaries by default or opt-in; installer marker bump; what smoke check works given the MCP binary refuses `--version`/`--help` (`tests/mcp_stdio.rs`)? Self-launch of the monitor from an installed copy must inherit the assets setting. | Owner / design | ADR-002 text vs test disagree (see contradictions). |
| Q12 | Which inherited intent-1 follow-ups (missing MCP tests AC-010..014, timing flakes, checkout runtime upgrade, old clippy warning, `deps add` lock) are in or out of intent 2. | Owner | Not named in the intent text. |
| Q13 | Intent/stage/unit IDs derive from user text slugs; acceptable under "no free-form text", or shorten/hash? | Owner / privacy review | Assumption in design s6.2: validated IDs are allowed. |
| Q14 | Minimum sample size for latency percentiles; heartbeat/liveness signal in or out of v1 (docs: out, show "unknown"); how the monitor learns a trusted "configured off" input; supervisor shutdown without a signal-handling crate (Windows vs POSIX proof). | Owner / design | Design s10 item 8; plan s3.5. |
| Q15 | Dashboard design pack: who reviews before implementation; ASCII wireframes enough or rendered mock; is `mcp-studio-server.html` revised (relabel, tool names, remove remote fonts, status text) or left as historical? | Owner | Plan s3.6, AC 10; scope-boundary lists the HTML under intent 2. |

## Decision status (added during context-and-risk)
- User-provided (owner, 2026-11-28): Q1, Q2, Q3, Q4, Q5, Q10, Q11, Q12, Q13 and Q15 are Decided; Q12 brings the named intent-1 follow-ups into scope. DeepSeek verification stays deferred. Q6, Q7, Q8, Q9 and Q14 remain open for Ideate. Decision text and new questions are in `../context-and-risk/open-questions.md`.

## Contradictions (both sources recorded)
1. Tool names: plan/design/HTML use `aidlc_status`, `aidlc_queue`, `aidlc_next`, `aidlc_report`; shipped code uses `status`, `queue`, `next`, `report`. Observed (ADR-009, `rust-cli/mcp`, `tests/repo_content.rs`): the shipped names are decided and intent-1 decisions stand, so the plan, design and HTML `aidlc_*` names are the stale side. Telemetry names and dashboard text follow the shipped names. Open question (owner) only if the owner wants to reopen ADR-009.
2. Human-mode approval: plan, arch, design s4.4 and the HTML say MCP must not submit approvals on human-mode gates; ADR-013 allows a required self-asserted label. Docs must follow ADR-013 (Observed); owner confirms wording.
3. SDK: plan Phase 0 / design s4.2 say use a maintained SDK; ADR-001 hand-rolled with zero crates.
4. Telemetry flag: plan s3.2 lists `--telemetry on|off` for the MCP server; shipped server rejects unknown arguments; design s4.1 says mechanism undecided.
5. Default: plan s3.7 "Proposed: on"; arch s9 "disabled-by-default remains an alternative"; HTML "TELEMETRY DEFAULT UNDECIDED". Also plan says CLI behavior and write behavior stay golden-stable.
6. Events: plan s3.5 lists park/revisit events "when exposed"; both intents list them as non-goals over MCP and they bypass the facade (Observed `rust-cli/main.rs`).
7. API errors: plan s3.6 lists four codes; design s7.7 adds `query_limit_reached`.
8. Monitor name: plan s3.6 "provisional"; design s7.7 and HTML treat `dtech-aidlc monitor` as the contract.
9. Dashboard data: design s7.7 says telemetry files only, yet plan asks for a workflow timeline; coverage is limited to emitted events (docs acknowledge partial coverage).
10. README label: `intent.md` says remove "proposed"; plan AC 13 says keep until implemented and released; knowledge record says removal needs host evidence while DeepSeek is waived.
11. HTML: loads remote fonts and says "NOT YET IMPLEMENTED"; dashboard rule says no remote fonts.
12. CHANGELOG: ADR-018 deferred it to this intent; content tests tie the CHANGELOG heading and badge to the crate version, so bump, heading, badge and tests change together.
13. Installers: README and `tests/repo_content.rs` assert installers are CLI-only and CI must not run them; install support needs README/test edits and coverage inside the test suite only.
14. PROJECT.md says no CI, no MSRV, macOS/Linux unverified; repo shows CI and rust-version 1.100. Repo files are authoritative (PROJECT.md not editable by this workflow).
15. Design s7.4 rejects custom ranges above a maximum but states no maximum value (Open question, owner, folded into Q14).

## Risks to carry to context-and-risk (Estimate unless noted)
Golden churn from default-on telemetry (Observed mechanism, Medium-high); monitor security (DNS rebinding, token) (Medium); stdout purity and non-panicking sink (Medium); Windows behavior (Low-Medium); intent size, Estimate 8-12 work units (Medium).
