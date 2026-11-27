# Constraints and assumptions

- Purpose: record the constraints, dependencies and assumptions that bound intent 1 of 2 (shared application facade plus MCP stdio server, plan phases 1-2).
- Status: Draft - pending analysis-baseline gate
- Sources: problem-framing artifacts (AC-001..AC-014, US-001..US-013); `.aidlc/knowledge/_codebase-map.md`; PROJECT.md; README.md; Cargo.toml; rust-cli/{main,orchestrate,gate,identity,prerequisites,status}.rs; plans/mcp-server-enhancement/*. Code facts are Observed at git commit 69cab5a. Scope is User-provided.

## Constraints
1. CLI behaviour is frozen. Observed (PROJECT.md "Constraints"): outputs, exit codes and written files are snapshot-compared in tests/golden/; a snapshot change needs DTECH_AIDLC_BLESS=1 plus a CHANGELOG entry. Argument validation, root resolution, engine selection, preflights, exit-code mapping and printing live in rust-cli/main.rs, so the facade must absorb them without byte change. Related: AC-001, AC-002, US-001.
2. Facade must carry outcome kind, not only a value. Observed (rust-cli/orchestrate.rs:39, main.rs run_orchestrate): Blocked yields exit code 3 after printing the payload; all other errors exit 1. finish_directive and finish_report run after next/report. Related: AC-001, AC-008.
3. JSON key order must survive. Observed (Cargo.toml): serde_json uses preserve_order; output is to_string_pretty plus newline. The facade passes the same serde_json::Value rather than typed structs. Related: AC-001, AC-002.
4. Stdout is the protocol channel. Observed: library modules print nothing (all stdout is in main.rs); `--intent-file -` reads stdin in main.rs and is excluded from the MCP path. Related: AC-004.
5. Minimal dependency baseline. Observed (Cargo.toml, Cargo.lock): direct dependencies are serde_json and sha2 only; all synchronous; no rust-version and no rust-toolchain file (PROJECT.md). Decided 2026-11-27 (User-provided, Q3): at most one reviewed SDK tree, used only by the MCP binary; the CLI binary keeps only serde_json and sha2; Cargo.lock committed; MSRV = current stable (see constraint 16). Related: AC-003, AC-013.
6. Offline locked install test. Observed (tests/rust_cargo_install.rs): runs `cargo install --offline --locked --path <repo> --bin dtech-aidlc`. Offline resolution needs every crate of the package graph (including any new optional dependency) in the local Cargo cache, even if only one bin is built. Plans already note this. Related: AC-013.
7. Second binary and packaging. Observed (Cargo.toml, README, scripts/rust-install/install.sh and install.ps1): one `[[bin]]`; README and the install test pass `--bin dtech-aidlc`; install scripts hard-code one binary, one launcher and one marker file. build.rs embeds assets into the library, so each binary carries a copy. Related: AC-013, AC-014.
8. Project root fixed at launch; no root in tool calls. Observed (plans/mcp-server-enhancement design s3.1, 69cab5a, cited via problem-statement.md), Proposed; only the intent scope is User-provided. Observed: resolve_like_node is lexical (orchestrate.rs:381) and assert_safe_state_dir refuses symlinked state dirs; design text says "canonicalized", which does not match code. Related: AC-006, AC-011, AC-012.
9. Gate integrity. Observed (gate.rs, identity.rs): AI-mode intents accept only approver "AI Orchestrator"; human-mode gates refuse it; approver precedence is flag, git user.name, OS user, "unknown". An MCP path that omits approver would inherit the server host's git or OS identity; decided 2026-11-27 (Q6) that MCP accepts a self-asserted approver label with the CLI gate policy, and the omitted-label case is proposed to be refused (Q18, Ideate). Related: AC-010, US-008.
10. Prerequisite shell checks. Observed (rust-cli/orchestrate.rs; prerequisites.rs): only the next path runs owner-configured shell checks (run_prerequisites at lines 1470, 1544, 1694, all inside orchestrate_next, starting at line 1142); the report path runs none (orchestrate_report and finish_report only check that needs names are known, require_known_prerequisites, line 2986; main.rs:437-457). force_resume does not gate execution: run_prerequisites runs whatever force_resume is set to, and the flag only overrides the hold (lines 1699, 1745). There is no no-exec mode today, and force_resume cannot serve as the Q5 guard; the new facade option must skip the call at line 1694 as well as 1470 and 1544. Decided 2026-11-27 (Q5): MCP never runs them (see constraint 17). Related: AC-008, AC-009.
11. Existing consistency tests. Observed (tests/repo_content.rs): help/completion/command-table lockstep; README sh blocks may use `dtech-aidlc <word>` only for commands listed by help (the matcher requires whitespace after `dtech-aidlc`, so `dtech-aidlc-mcp` lines do not match); required README sections; CHANGELOG heading per crate version. Related: AC-013, AC-014.
12. Non-goals carried from the plans. Observed: no CLI replacement, no network transport, no arbitrary shell/Git/file tools, no second state store, no schema change, no host-config auto-edit, tools only, no authenticated approval. Related: AC-003, AC-010, AC-014.
13. Label MCP features "proposed" until released. Observed (plan s6, cited via problem-statement.md; the plans were not read directly by this writer, see evidence-log E5). Related: AC-014.
14. Test baseline is environment dependent. Observed (map, PROJECT.md): 450 passed and 6 failed in this environment (2 real, 4 from TMPDIR /private/var canonicalization); PROJECT.md reports 439/17 default and 454/2 with TMPDIR fixed under /private/tmp and target/debug on PATH (PROJECT.md:330). Related: AC-001.
15. Queue baseline already meets AC-007. Observed (rust-cli/unit_commands.rs:7-8, :182, 69cab5a): `queue` is read-only and lists ready, waiting, blocked, parked and done units; unmigrated 1.0.0 state is shown with `migration: pending` and nothing is written. The facade must preserve this (no persistence added on the MCP read path). Related: AC-007.

## Dependencies
1. Rust toolchain: rustc 1.100.0 stable (Observed, map). Code uses std::path::absolute (stable since 1.79), so the de-facto minimum is at least 1.79 (Observed, orchestrate.rs:383; version inference is an Assumption).
2. Embedded engine assets via build.rs (core/, harness/) shared by both binaries (Observed).
3. Optional external engine tree via DTECH_AIDLC_HOME, read in several places including main.rs external_engine() (Observed; Q15).
4. A host-side MCP client to exercise AC-003. Hosts decided 2026-11-27 (User-provided, Q1): Claude Code and DeepSeek Harness; both must launch and list tools.
5. A candidate MCP SDK or a hand-rolled JSON-RPC loop. Open question (Ideate research; policy bound is owner).
6. git on PATH for identity lookup and `repos` (Observed, identity.rs, dependencies.rs).

## Assumptions
| # | Assumption | Impact if wrong | How to validate |
|---|-----------|-----------------|-----------------|
| A1 | No file lock exists; the only guard is compare-before-write (ensure_unchanged, orchestrate.rs:1071-1088). Grep of rust-cli for lock/flock found nothing (Observed in research; not exhaustively audited) | A long-lived server plus CLI could lose an update; AC-012 scenario may need design change | Interleaving test in Develop; read workflow/mod.rs writers |
| A2 | The facade can stay synchronous (blocking fs I/O) | An async SDK forces spawn_blocking or a single-thread loop | Ideate spike |
| A3 | Prerequisite child processes do not set stdin to null (no "stdin" token in prerequisites.rs; Observed), so a child could read MCP protocol bytes | Protocol corruption or hang | Test running a prerequisite that reads stdin; set stdin null in shared code if confirmed |
| A4 | Redaction can be done only in the MCP adapter without changing CLI goldens | If paths appear in error text outside the adapter's reach, AC-006 fails | Recursive redaction test over success and error values, including macOS /var vs /private/var and Windows forms |
| A5 | No CI exists in-repo (.github has only badges); Windows plus POSIX CI is a new requirement | AC-013 cross-platform claim unverified | Owner confirms external CI |
| A6 | No measured baseline or pain metric exists; success is pass/fail | Numeric targets in quality-requirements are Estimates | Owner confirms |
| A7 | A second binary roughly doubles on-disk size because assets are embedded per binary (Estimate; not measured) | Install size, build time | Measure in spike |
| A8 | Maintainers are relevant to intent 1 only as reviewers | Missing reviewer for dependency choice | Owner confirms |
| A9 | The existing plans' design is proposal-only and not approved | Phase 1 may start prematurely | Settled by methodology order (Ideate before Develop); confirm at the Ideate gate (Q2) |

## Constraints from the 2026-11-27 owner decisions (User-provided, project owner, conductor AskUserQuestion)
16. Dependency policy (Q3). At most one reviewed SDK dependency tree, used only by the MCP binary; the CLI binary keeps only serde_json and sha2; Cargo.lock is committed; `--locked` and offline `cargo install` must keep working; MSRV = current stable. Hand-rolled JSON-RPC is still allowed if recorded in an ADR (Ideate). Related: AC-003, AC-013, R7, R8.
17. No prerequisite execution from MCP (Q5). MCP next and report never run owner-configured prerequisite shell checks; they return pending prerequisites marked not executed. The guard lives in the shared facade as an option that is off for the CLI, so CLI behaviour stays byte-identical. Related: AC-001, AC-008, AC-009, R3.
18. Approvals over MCP (Q6). MCP report accepts an approver field like the CLI --approved-by, self-asserted; gate policy is the CLI's (AI-mode needs "AI Orchestrator" plus gate-recommendation checks; human-mode refuses "AI Orchestrator"). Omitted-label behaviour is an Ideate decision (Q18), proposed default: refused, no git or OS fallback over MCP. The audit should record the MCP origin if feasible (Ideate research). Related: AC-009, AC-010, R5.
19. Supported hosts (Q1). Claude Code and DeepSeek Harness must both pass launch-and-discover (AC-003). Ideate pins the protocol revision. Related: AC-003, AC-014, R18.

## Revision note
- 2026-11-27 (attempt 2): constraints 5, 9, 10 updated and 16-19 added from the owner decisions; Dependencies 4 updated; A9 validation updated.
