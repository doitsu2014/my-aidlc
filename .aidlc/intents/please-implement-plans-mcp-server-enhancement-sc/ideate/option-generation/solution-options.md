# Solution options: shared application facade and MCP stdio server (intent 1 of 2)

- Purpose: set out the complete, genuinely different solution options for intent 1 (shared application facade plus the `dtech-aidlc-mcp` stdio server exposing status, queue, next and report) so that direction-selection can choose one. This document selects no direction.
- Status: Draft - pending solution-direction gate
- Stage: Ideate / option-generation (solution-explorer). Companion: [trade-off-matrix.md](trade-off-matrix.md).
- Sources:
  - Approved Analyze baseline: `analyze/problem-framing/{problem-statement,success-measures,stakeholder-map,acceptance-criteria,user-stories}.md` and `analyze/context-and-risk/{constraints-and-assumptions,evidence-log,risk-register,open-questions,quality-requirements,gate-recommendation}.md`. AC-001..AC-014 and US-001..US-013 are cited from there, and none are minted here.
  - `.aidlc/knowledge/_codebase-map.md` (draft, mapped at 69cab5a), `PROJECT.md`, `README.md` (registered sources).
  - `plans/mcp-server-enhancement/{mcp-server-architecture.md, mcp-server-detailed-design.md, mcp-server-observability-implementation-plan.md}`, read first-hand on 2026-11-27. These are the owner's proposal and are not approved design.
  - Owner decisions Q1, Q3, Q5 and Q6 (User-provided, project owner, 2026-11-27; evidence-log E18).
  - Ideate research spikes run on 2026-11-27 with rustc/cargo 1.100.0, all under `/private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/spike/`: `mcp-protocol/` (hand-rolled and rmcp servers, transcripts), `facade-researcher/` (approach-a.diff applied to a repo copy), `packaging/` (offline install with clean CARGO_HOMEs), `host-compat/` (a Claude Code frame capture). Nothing in the repo working tree was changed.
- Accepted decision records checked: none. `aidlc.knowledge.json` registers only `project` and `README.md` and has no `adr` or `architecture` source (Observed, 2026-11-27). Conflicts with the plans are still named for each option, because the plans are the owner's proposal.

## 1. Facts every option must respect

These come from the baseline or were verified in the Ideate spikes. They bound all the options below.

1. **Offline install resolves the whole lockfile.** Observed (packaging spike A1, B1, B2-1, C1). `cargo install --offline --locked --path <repo> --bin dtech-aidlc` FAILS with "no matching package named `rmcp` found" on a cache that holds only today's crates in two cases: when rmcp is an optional dependency of the root package, and when rmcp is in a workspace member. It SUCCEEDS when rmcp lives in a nested package with its own `[workspace]` and `Cargo.lock` (b2), and when there is no new dependency (hand-rolled). On a cache that already has rmcp's index entries it passes (A3, A10). This machine already has those entries (mtime 2026-11-27 21:59, origin unknown), so `tests/rust_cargo_install.rs` cannot detect the regression here (Observed).
2. **The protocol now has two eras.** Observed (modelcontextprotocol.io/specification/2026-07-28 changelog and versioning, 2026-11-27). Revision 2026-07-28 removes `initialize` and adds `server/discover`, per-request `_meta`, `resultType`, `ttlMs`/`cacheScope` and error -32022. A legacy-only server works with dual-era clients, and a dual-era server works with every client.
3. **Host behaviour.**
   - Claude Code 2.1.332 (Observed, `host-compat/claude-frames.log`): it first sends `server/discover` (string id, modern `_meta`). After -32601 it sends `initialize` 2025-11-25, then `notifications/initialized` and `tools/list`, and a legacy stub reached "Connected".
   - DeepSeek Harness (Observed from source and docs only; not run, `dsh` is not on PATH): it uses `@modelcontextprotocol/client` 2.2.0, which selects 2026-07-28 when available and falls back to legacy. It starts a temporary probe process before the serving process, so startup must have no side effects and the server must exit on EOF. Tool names are limited to 64 characters `[A-Za-z0-9_-]`.
   - Inference (Assumption, not Observed): if a server offers the modern era, both hosts will drive it in modern mode. No host has yet driven a modern-capable server, hand-rolled or rmcp. The only real host run pointed Claude Code at `legacy_server.py`, which answered `server/discover` with -32601 (Observed, `host-compat/proj*/.mcp.json` and `claude-frames.log`, 2026-11-27). DSH was not run. The inference rests on Claude Code sending `server/discover` first and on the DSH client's documented version preference.
4. **Facade seams exist.** Observed (main.rs:325-549, orchestrate.rs:263-289, 1142, 1272; facade spike). `NextOutcome` already carries the outcome kind (Blocked maps to exit 3, BLOCKED_EXIT_CODE at orchestrate.rs:39). `orchestrate_next` creates an intent only when intent text is supplied (`is_new = true` only at orchestrate.rs:1272), so the design s4.3 claim that it creates one without text does not match the code. A thin `application.rs` routing only status, queue, next and report gave 226 insertions and 56 deletions in 5 files, and the full suite stayed at 454 passed / 2 failed, the same failure set as baseline (Observed, `facade-researcher/approach-a.diff`, TMPDIR normalized).
5. **The Q5 guard needs a code change in orchestrate_next.** Observed (orchestrate.rs:1470, 1544, 1694; prerequisites.rs:171-175). All three `run_prerequisites` calls are inside `orchestrate_next`. Prerequisite children do not set stdin, so they would inherit the protocol stdin (A3 confirmed by reading the code). Every option therefore adds a skip option (spike: `NextOptions.skip_prerequisite_checks`, default false). Under skip the result rows say `executed:false` and nothing is spawned.
6. **Only report reaches the identity fallback over MCP, and only because of the next guard.** Observed (identity.rs:28; orchestrate.rs:3275, 1962, 1272). Report calls `resolve_approver` at orchestrate.rs:3275. Next also calls it, at orchestrate.rs:1962, when `is_new` is true, and `is_new` is set only when intent text is supplied (orchestrate.rs:1272). Over MCP that call is unreachable only because the next guard forces `intent=None` (section 2). If the guard were removed or bypassed, next would reach the git/OS fallback too. The Q18 guard (refuse an MCP approval with no label, before any I/O) was spiked and writes nothing.
7. **Library code prints nothing.** Observed (grep of rust-cli outside main.rs, 2026-11-27): there is no `print!`, `println!` or stdout use in library code. The stdin readers (`intent_text`) are in main.rs only.

## 2. Elements common to every option

Unless an option says otherwise, each one includes the elements below. They are listed once here so the per-option sections show only real differences.

- **Tool surface.** Four tools, default names `aidlc_status`, `aidlc_queue`, `aidlc_next`, `aidlc_report` (Q9 default). Each is safe in both hosts (`mcp__aidlc__aidlc_status` is 24 characters, under the DSH limit of 64 and the Claude limit of 128; Observed in the docs). Open question: AC-003 text says "status, queue, next and report" and must be reconciled with Q9. Owner: direction-selection.
- **Input validation before project access (AC-005).** Strict per-tool schemas: unknown fields rejected, enums checked, string and list caps applied. Excluded fields: `root`, next's intent text (`intent`), `scope`, `depth`, `mode`, `force_resume` and evidence fields. This exclusion is not about the status intent selector: status accepts an optional `intent_id` (Q13; `status::status` takes `Option<&str>` at status.rs:233, Observed), and an unknown selector returns the scenario error in AC-006. This runs in the MCP adapter before the project is opened.
- **Root (AC-012).** The root is fixed at launch from `--project` or the startup directory and passed through `resolve_like_node` and `assert_safe_state_dir` (lexical, as the code does today; Q16). Project state is re-read on every call (open per call), config is reloaded per operation as the CLI does, and there is no lock (A1).
- **Concurrent writes (AC-012 scenario "simultaneous CLI and MCP writes", Q10, R6).** Every option reuses the existing optimistic check: `ensure_unchanged` refuses a write when the pointer or intent state changed since load, with the error "Workflow state changed while this command was running; no changes were made. Run the command again." (orchestrate.rs:1071-1088, Observed). Over MCP this becomes a typed, retryable conflict code that is never retried automatically (R6 mitigation, Q10 proposed default; no operation ID in v1). The compare-then-rename window is not atomic and both writers append audit, so the residual window is documented, not closed (R6 stays Low likelihood, High impact). Each option adds a deterministic interleaving test. How the conflict is recognised depends on the error model: with `String` errors (A, B, D) the adapter matches that message text, which CLI goldens already pin; with a typed `AppError` (C) it is a variant. This is a point in the `String` versus `AppError` choice (section 4).
- **Paths (AC-006, AC-008).** Paths are redacted or made project-relative recursively, in the MCP adapter only (A4, R9), so CLI bytes do not change.
- **Next guard (AC-008).** The MCP next request carries no intent text, and the facade or adapter forces `intent=None`. With no active intent this returns the existing "No active intent" error and writes nothing (Observed code path).
- **Prerequisite guard (Q5, AC-008).** `skip_prerequisite_checks=true` on the MCP path covers all three call sites. Open decisions: whether the `prerequisite.checked` audit row (orchestrate.rs:1700-1706) is still written under skip, and the wording of the Blocked hint for checks that never ran.
- **Report (AC-009, AC-010).** Report maps only to the existing `ReportOptions` fields. The Q18 guard refuses an approval with no label, so no git/OS fallback happens over MCP. The gate policy is the CLI's own code path (Q6). Pending prerequisites are required, not optional: approved AC-009 (scenario "report with a configured prerequisite") and constraint 17 require MCP report to return the prerequisite names that would run when advancing, marked not executed. The CLI report neither runs nor lists them (orchestrate.rs:2986 only checks that `--needs` names are known, Observed), so computing the list is new work in every option. It is on the MCP path only, so CLI bytes do not change. A likely source is `preflight_names(config, next stage)` (prerequisites.rs:118) plus the unit's needs, as next assembles them at orchestrate.rs:1462-1468 (Assumption). Estimate: 0.5-1 day including tests, counted in each option's effort below. Only the owner reopening AC-009 would remove this work.
- **Errors (AC-011).** Errors are bounded, carry a stable code and are never retried automatically. Whether this uses string classification or a typed error depends on the option.
- **Host docs (AC-014).** Claude Code: `claude mcp add --transport stdio aidlc -- dtech-aidlc-mcp`, plus a `.mcp.json` example. DeepSeek Harness: a `cordis.patch.yml` with `cwd: !!js process.cwd()`. Every MCP statement is labelled "proposed" until a release. Nothing writes host settings.
- **MSRV (Q3 follow-up: a concrete rust-version).** Proposal for every option (Estimate; the owner confirms at the gate): declare `rust-version = "1.100"` in the root `Cargo.toml`. That matches the owner's "MSRV = current stable" with rustc 1.100.0 observed, and it is above the de-facto floor of 1.79 (constraint 1). The line changes `Cargo.toml` but not `Cargo.lock`, so `--locked` and offline install are unaffected (Assumption). Open question for the owner: whether "current stable" means bumping the value with each stable release or pinning it once. Option B's `mcp/` package declares the same value. rmcp's own MSRV is 1.88 (Observed, crates.io), so it does not raise the floor.
- **Ordering (Q17).** Every option keeps the existing per-command validation and write ordering (the Q17 proposed default): A, B and C because validation order stays in main.rs and the facade calls the existing functions in their current order, and D because main.rs is untouched. Reconciling the plans' fixed ordering with the code's per-command ordering, and their differing open-decision lists, is deferred to implementation-design. It does not distinguish the options. Option C's `AppError` round-trip is the one place where error ordering must be re-checked.
- **Packaging defaults.** The install scripts stay CLI-only (N2 default). The MCP binary embeds the same assets as the CLI if it links the library: about 3.8 MB (Estimate, from the CLI size; A7 not measured with assets).
- **Process behaviour.** Nothing is written at startup, which DSH's probe process needs. The server exits 0 on stdin EOF, tolerates SIGTERM, keeps string ids, writes diagnostics only to stderr and opens no listener (AC-004).
- **Out of scope in every option.** Telemetry, the monitor, and plan phases 3-5 (intent 2). Park and revisit over MCP. Network transport. Authenticated approval. Install-script support for the MCP binary.

## 3. Options

### Option A: Lean hand-rolled. Thin facade, dual-era sync JSON-RPC, second `[[bin]]` in the same package

**Summary.**
- Facade: approach (a), a thin `rust-cli/application.rs` with a `Caller {Cli, Mcp}` and four operations over the existing functions (Observed spike). The CLI routes only status, queue, next and report through it. Argument parsing, flag validation order, `intent_text`, printing and exit codes stay in main.rs.
- Server: `dtech-aidlc-mcp` is a second `[[bin]]` in the existing package, with no features and no new crates. It speaks dual-era JSON-RPC 2.0 over newline-delimited stdio using `serde_json` and synchronous `std::io`, and calls the facade directly with no runtime.
- Protocol scope: legacy `initialize` for 2024-11-05..2025-11-25, and modern 2026-07-28 with `server/discover`, `_meta` validation and -32022. Tools only.
- An ADR records why hand-rolled was chosen over an SDK, plus a conformance plan: transcript tests plus a manual check in each host. Q3 explicitly allows hand-rolled JSON-RPC with an ADR, so A is feasible whatever the ADR's rationale. What the rationale can honestly claim is narrower. Fact 1 is a concrete blocker only for an SDK inside the root package or a workspace member (Option C). Option B's spike shows that an SDK isolated in its own package leaves CLI offline install working (Observed, packaging b2). So fact 1 is a blocker for A only if the owner rules that Q3's offline promise also covers `dtech-aidlc-mcp`, or accepts dependency footprint (53-62 extra crates including tokio and a second sha2) and release churn (R8) as the blocker. Both are Open questions for the owner.

**How it meets the criteria.**

| Criterion | How |
|---|---|
| AC-001 | Validation order stays in main.rs. The facade calls `knowledge_preflight` at the original spot. Observed 454/2 identical in the spike. |
| AC-002 | Both binaries call the same `application::{status,queue,next,report}`. There is one `external_engine()` with main.rs semantics (R13). |
| AC-003 | Dual-era answers Claude Code's `server/discover` probe and DSH's modern-first negotiation, with legacy `initialize` as fallback. Observed equivalent transcript shapes to rmcp (`mcp-protocol/transcripts`). Host launch-and-list must be checked manually in both hosts. |
| AC-004 | There is exactly one stdout writer, the main loop. A purity test spawns the binary. |
| AC-005..AC-012 | As in section 2, including the AC-009 pending-prerequisite list (new work) and the AC-012 conflict code. AC-011 errors are classified in the adapter from the facade's `String` errors (Estimate: a small error-code table). |
| AC-013 | Cargo.lock is unchanged. `cargo install --offline --locked` works for both binaries on a clean cache (Observed C1). `cargo install --path . --locked` installs both. `cargo uninstall dtech-aidlc` removes both (Observed C3). |
| AC-014 | As in section 2. |
| Q1 | Both hosts are served by dual-era. Their actual launch is not yet verified for DSH. |
| Q3 | Fully compliant. No new crates, the CLI graph is untouched, one lockfile, offline `--locked` for both binaries, `rust-version = "1.100"` as in section 2 (no SDK raises it). The ADR route is explicitly allowed by Q3. |
| Q5 | The guard sits in the shared facade (`Caller::Mcp` sets skip) and is off for the CLI. |
| Q6/Q18 | Report accepts the label under the CLI gate path. The facade refuses an MCP approval with no label before any I/O (spiked). |

**Left out or deferred.** The protocol features not needed for a tools-only server: resources, prompts, sampling, progress, `listChanged`, and acting on cancellation mid-call (tools are short and synchronous). A typed `AppError`, which can come later. Running the official conformance suite: it is HTTP-only for servers (Observed), and a test-only bridge would be extra work.

**Key risks.**
- R18: the hand-rolled modern path is probably what both hosts will use (fact 3 inference, Assumption; no host has driven a modern-capable server yet). A schema detail wrong in `server/discover` or `tools/list` (`resultType`, `ttlMs`, `cacheScope`, where `serverInfo` goes) could break a host. Whether a host falls back to legacy after a bad modern answer is unknown (Open question).
- Unregistered risk, related to R8 and R18: spec tracking is ours. There were five revisions in about 20 months, and 2026-07-28 was breaking. A 12-month deprecation window applies (Observed, changelog).
- R1, R3 and R9 as for every facade option. R4 and R7 are removed for MCP.
- The full 2026-07-28 `schema.json` has not been checked yet (research gap).

**Effort (Estimate).** About 9-13 engineer-days.
- Facade productionized with Q5 and Q18 tests, including fixtures for sites 1470 and 1544: 2-3 days (spike exists).
- Protocol layer plus transcript tests: 1-2 days (about 100 lines measured; 150-250 production lines plus 200-400 test lines).
- Tool schemas, validation, redaction and error mapping: 2-3 days.
- AC-009 pending-prerequisite list on report (section 2): 0.5-1 day.
- AC-012 conflict code and deterministic interleaving test (R6): 0.5-1 day.
- Parity and child-process tests: 2 days.
- Docs, ADR and manual host checks: 1 day.
- Basis: spike line counts and research estimates. Uncertainty ±30%.

**Variant A-legacy: facade plus legacy-only hand-rolled.** The same as A except that the protocol answers only legacy `initialize` (2025-06-18..2025-11-25) and returns -32601 for `server/discover`. That is about 30 fewer lines (Estimate). It has a different risk profile from A, and neither one dominates the other:
- For: legacy-only is the only protocol path observed connecting in a real host (Claude Code reached "Connected" after the -32601 fallback, Observed `host-compat/claude-frames.log`). It removes the hand-rolled modern-path R18 risk and the need to check the 2026-07-28 `schema.json` before Develop.
- Against: it fails any modern-only client. Moving to dual-era later is subject to Claude's up-to-7-day re-probe skip (Observed, docs). DSH legacy fallback is documented but not run.
- Everything else (facade, Q3, Q5, Q6/Q18, intent-2 fit) is as A. Effort: about 0.25-0.5 day less than A (Estimate).

**Reversibility.** High. The protocol loop is isolated behind a `(method, params) -> result` boundary, and the tool handlers take and return `Value`. Swapping in an SDK later (as Option B) changes packaging and the loop, not the facade or the tools. Dual-era can be trimmed to legacy-only (variant A-legacy) in about 30 lines.

**Plan conflicts (named).**
- Design s4.2 ("Do not implement JSON-RPC framing by hand unless an ADR documents the SDK blocker and a conformance plan") and plan s4 Phase 0 step 3 ("do not hand-roll MCP protocol unless the SDK evaluation documents a concrete blocker"): whether the ADR satisfies these depends on an owner ruling. If the owner rules that Q3's offline promise covers `dtech-aidlc-mcp`, or accepts dependency footprint and churn as the blocker, the ADR can name a concrete blocker and satisfies both. Otherwise Option B shows there is no hard blocker, and A departs from these plan clauses under Q3's own ADR allowance (the owner decision supersedes the plan wording rather than satisfying it).
- Plan s4 Phase 2 step 2 ("through the selected SDK") is departed from in either case.
- Design s3.1 says the `ProjectContext` graph is cached per process and the root is "canonicalized". This option opens per call and resolves lexically, matching the code (Q16).
- Plan s3.3, s8.5 and design s4.4 ("MCP must not submit an approved decision for a human-mode gate") are superseded by owner decision Q6 in every option.

### Option B: Official SDK, isolated. rmcp in a nested standalone package with its own lockfile

**Summary.**
- Facade: the same thin facade as Option A, in the root library.
- Server: a nested package `mcp/` (`dtech-aidlc-mcp`) with an empty `[workspace]` table and its own `mcp/Cargo.lock`. It path-depends on the `dtech-aidlc` library. It uses `rmcp` 3.12.x (`default-features=false`, features `server`, `transport-io`, no macros) plus `tokio` (`rt`, `macros`, `io-std`).
- The root `Cargo.toml` and `Cargo.lock` are byte-identical to today (Observed, packaging b2), apart from the `rust-version` line that section 2 adds in every option, independently of the SDK.
- rmcp is dual-era out of the box (Observed, `mcp-protocol/rmcp-min`, 29 lines). The synchronous facade runs inside `call_tool` via `spawn_blocking` (Assumption A2).
- Install with `cargo install --locked --path mcp`.

**How it meets the criteria.**

| Criterion | How |
|---|---|
| AC-001 | As Option A (same facade). The root package is untouched by the SDK. |
| AC-002 | As Option A, across the package boundary through the path dependency. |
| AC-003 | Dual-era is maintained upstream. Observed in the spike: legacy 2025-11-25 and 2024-11-05, unknown-version downgrade, modern `server/discover` with all five versions, -32022, -32601. `serverInfo` must be overridden (it defaults to "rmcp"). Manual host checks are still needed. |
| AC-004 | rmcp's transport is the only stdout writer. Panics and `ConnectionClosed` go to stderr (Observed). Any tracing subscriber must write to stderr. |
| AC-005..AC-012 | As in section 2. Schemas can be hand-written JSON or typed parameters. Typed parameters (`Parameters<T>` plus serde derive) are available inside the SDK tree, so unknown-field rejection is a `deny_unknown_fields` attribute. |
| AC-013 | CLI offline `--locked` install on a clean cache: Observed success (B2-1, B2-2). MCP binary: installed from `mcp/`, needs the SDK crates in the cache (expected). Two install commands. Root `cargo test` does not cover `mcp/`, so CI and tests need `--manifest-path mcp/Cargo.toml`. |
| AC-014 | As in section 2, plus the separate install command in the README. |
| Q1 | Both hosts are served by rmcp dual-era (not host-verified). |
| Q3 | CLI: compliant (root graph and lockfile unchanged). MCP binary: one SDK tree (rmcp plus its runtime, 62 new lock entries, Observed), rmcp's MSRV is 1.88, under stable 1.100. Both packages declare `rust-version = "1.100"` (section 2), so the separate package does not create a second MSRV, but the value must be bumped in two manifests if the owner wants it to track stable. Both lockfiles committed. Whether "offline cargo install keeps working" also covers the MCP binary is an Open question for the owner. If it does, this option needs a primed cache or vendoring for that binary. |
| Q5, Q6/Q18 | As Option A (same facade). |

**Left out or deferred.** rmcp `macros`/`schemars` features (they would grow the tree to 81/98 crates, Observed). Vendoring. Install-script support. A unified version policy for the two packages (Open question).

**Key risks.**
- R8: rmcp shipped three major versions in 2026 and 3.10-3.12 within 14 days (Observed). Expect lockfile and upgrade churn.
- R7: the residual risk is limited to the MCP binary's offline install.
- R10: two packages, two lockfiles that can drift (serde_json and sha2 versions), two version numbers, separate CI.
- R17: the extra CI invocation does not exist yet.
- Duplicate `sha2` 0.10/0.11 and `syn` 2/3. Binary about 3.5 MB plus about 3.8 MB of assets (Estimate).
- License review: rmcp is Apache-2.0, but the project's own license is unknown because there is no root LICENSE (Observed). `cargo audit` has not been run.
- Async bridging (A2) has not been exercised with real facade calls.
- A non-JSON input line is silently dropped with no -32700 reply (Observed). This is minor.

**Effort (Estimate).** About 10-14 engineer-days, plus ongoing upgrades.
- Facade as in A: 2-3 days.
- rmcp glue and tools: 3-4 days (glue about 80-150 lines plus the same schemas).
- AC-009 pending-prerequisite list and AC-012 conflict code with interleaving test (section 2): 1-2 days.
- Packaging, second lockfile, CI and README install changes: about 1 day.
- Dependency review (license, audit, version pin): 0.5-1 day.
- Parity, child-process and host checks: 2-3 days.
- Basis: spike sizes and packaging research. Uncertainty ±30%. Ongoing upgrade cost is not estimated.

**Reversibility.** Medium. The SDK is fully isolated, so deleting `mcp/` reverts it cleanly. But published install instructions, the second lockfile and the CI path are user-visible, and moving to Option A later changes how users install.

**Plan conflicts (named).**
- Design s2 and plan s3.2 ("second Cargo binary ... with an explicit Cargo target in `Cargo.toml` and a source entry such as `rust-cli/mcp_main.rs`"; "Install the two public binaries ... through the existing Cargo path") are departed from: a nested package with its own manifest and install command.
- Design s3.1 per-process context: as in Option A.
- Plan s3.3, s8.5 and design s4.4 human-mode refusal: superseded by Q6.
- This option otherwise follows design s4.2 and plan Phase 0 step 3 (an SDK first).

### Option C: The plans as written. rmcp as an optional dependency of the root package, plan module layout, process-lifetime context, typed AppError

**Summary.** Follow the plans' structure literally (design s2, s3.1-3.2, plan s3.2):
- `[[bin]] dtech-aidlc-mcp` → `rust-cli/mcp_main.rs` in the root package, with `rust-cli/mcp/{mod,tools,errors}.rs`.
- rmcp and tokio as optional dependencies behind `[features] mcp`, with `required-features = ["mcp"]` on the MCP binary.
- A `ProjectContext` resolved once per process (root, graph, engine, caller), with a typed `AppError { code, message, source }` returned by the facade.
- One root `Cargo.lock`, which grows from 25 to 87 packages (Observed, A).
- Owner decision Q6 overrides the plans' human-mode refusal, so this option takes the CLI gate policy as in A and B.

**How it meets the criteria.**

| Criterion | How |
|---|---|
| AC-001 | Thin facade as in A, with errors mapped to `AppError` and back to the same `String` text for the CLI. That mapping is an extra golden risk (R1). |
| AC-002 | Same facade. |
| AC-003, AC-004 | As Option B (rmcp). |
| AC-005..AC-012 | As in section 2. Engine and graph are cached for the process lifetime. Config is still reloaded per call (design s3.1). AC-011 is served by typed codes. |
| AC-013 | `cargo install --path . --locked --features mcp` installs both binaries. CLI offline `--locked` install on a clean cache FAILS (Observed A1, A2). It passes only on a cache primed by any networked build (A3, A10). The existing test cannot see the difference on a warm machine. A clean-CARGO_HOME CI check would be needed to even detect it. |
| AC-014 | As in section 2. |
| Q1 | As Option B. |
| Q3 | Not compliant under the strict reading. Feasible only if the owner rules (a) that "offline" means "a cache primed by a normal networked build", and (b) that "CLI binary keeps only serde_json+sha2" is met when the SDK sits in the same package behind a feature. Rmcp and tokio appear in the package graph when the feature is on (Observed A11), but the CLI binary links nothing extra (release sizes identical, Observed). Both rulings are Open questions for the owner. |
| Q5, Q6/Q18 | As Option A. Q18 refusal gets a stable `AppError` code. |

**Left out or deferred.** As Option B. A workspace-member variant behaves the same for offline install (Observed B1) and is not listed separately.

**Key risks.**
- R7 is realized: offline `--locked` CLI install breaks on an air-gapped or CLI-only cache, and the existing test cannot detect it.
- R8: as Option B, in the root lockfile, so every SDK bump touches the CLI's lockfile.
- R1 is higher: `AppError` round-trip and a per-process graph cache (behaviour differs from the CLI if `DTECH_AIDLC_HOME` content changes while the server runs; this is minor).
- R17: CI needs `--features mcp`, and `cargo test` without features does not build the MCP binary.
- R14: building from unapproved plan detail.

**Effort (Estimate).** About 10-14 engineer-days.
- As Option B (including the AC-009 pending-prerequisite list and the AC-012 conflict code and test), minus the second-lockfile work.
- Plus about 1 day for `AppError` (+40-80 lines, research estimate) and the clean-cache CI check.
- Uncertainty ±30%, plus the time for the owner rulings.

**Reversibility.** Medium-low. Removing the SDK later rewrites the root `Cargo.lock` again and changes feature flags users may have scripted. `AppError` is a wider API change to unwind.

**Plan conflicts (named).** It matches design s2, s3.1, s3.2, s4.2 and plan s3.1-3.2 and Phase 2 step 2. It conflicts with owner decision Q3 (constraint 16) under the strict reading, and with design s3.1's "canonicalized" root (code is lexical, Q16). Plan s3.3, s8.5 and design s4.4 human-mode refusal are superseded by Q6. The plans themselves flag the risk: design s2 says to "treat offline-cache availability as a separate deployment constraint", and arch s9 asks the spike to prove `--locked` reproducibility. The spike shows that `--locked` holds but offline install does not.

### Option D: Minimal, no facade. The MCP binary calls the library directly, legacy-only hand-rolled protocol

**Summary.**
- main.rs is untouched. `dtech-aidlc-mcp` (a second `[[bin]]`, no new crates) calls `status::status`, `unit_commands::run("orchestrate queue", ...)`, `orchestrate_next`/`finish_directive`, `orchestrate_report_with_engine`/`finish_report` and `knowledge_preflight` itself. That duplicates about 60-80 lines of main.rs glue (Estimate, facade research alternative c).
- The protocol is legacy-only: answer `initialize` with 2025-11-25 (accepting down to 2025-06-18) and return -32601 for `server/discover`. This is about 70 lines (Estimate).
- The Q5 skip option is still added to `NextOptions` in orchestrate.rs, because there is no other place to put it. The MCP adapter sets it.
- The Q18 guard lives in the MCP adapter.
- **Not compliant with owner decision Q5 as decided.** The owner decided "guard in shared facade, off for CLI" (User-provided, 2026-11-27; constraint 17), and D has no facade by design. D is feasible only if the owner rules that a guard in orchestrate.rs plus the MCP adapter satisfies Q5. This is the same treatment as Option C under Q3.

**How it meets the criteria.**

| Criterion | How |
|---|---|
| AC-001 | Strongest. main.rs has zero changes, and the only shared change is the default-off `NextOptions` field. |
| AC-002 | Weak. Both entry points reach the same library functions, with no subprocess and no second engine, so the AC statement is arguably met in letter. But route-specific preflight, finalizer ordering and engine resolution are duplicated. That is the drift design s3.2 warns about (R13), and it would become an eighth `DTECH_AIDLC_HOME` copy unless main.rs's copy moves to the library. |
| AC-003 | Legacy-only works with Claude Code today (Observed: a legacy stub reached "Connected" after the `server/discover` -32601 fallback). It works with DSH per the docs and the spec matrix, but DSH was not run. It would fail any future modern-only client. |
| AC-004 | One stdout writer. |
| AC-005..AC-012 | As in section 2, all inside the MCP binary. |
| AC-013 | As Option A (Observed C1, C3). |
| AC-014 | As in section 2. |
| Q1 | Met today by the observed and documented fallback behaviour. |
| Q3 | Fully compliant (as A). |
| Q5 | Not compliant as decided. The skip behaviour exists (orchestrate.rs option set by the adapter, all three sites skipped), but the owner decided "guard in shared facade, off for CLI" (constraint 17) and D has no shared facade. Feasible only if the owner rules otherwise (Open question). |
| Q6/Q18 | Met in the adapter. CLI unaffected. |

**Left out or deferred.** The shared facade (plan Phase 1), modern-era support, typed errors, and any CLI refactor.

**Key risks.**
- R13 and CLI/MCP divergence over time.
- R18 if a host pins 2026-07-28.
- Claude may skip re-probing for 7 days after a server is upgraded to modern (Observed, docs).
- Intent 2 rework: shared operation telemetry needs a facade (plan s2.4, design s6.3), so Phase 1 would be done later anyway.
- R1 is minimal and R7 is removed.

**Effort (Estimate).** About 6-9 engineer-days.
- Glue and guards: 1-1.5 days.
- Protocol: under 1 day.
- Tools and redaction: 2 days.
- AC-009 pending-prerequisite list and AC-012 conflict code with interleaving test (section 2), in the adapter: 1-2 days.
- Tests and docs: 1.5-2 days.
- Uncertainty ±30%. Intent 2 would later absorb the deferred facade work, about 2-3 days.

**Reversibility.** High for code, but it creates debt. Moving to a facade later is a refactor of duplicated glue. Moving to dual-era later is about 30 lines plus host re-probe lag.

**Plan conflicts (named).** It conflicts with arch s3 goal 3 and arch s11 criterion 3 ("Both adapters call shared operations"), plan s3.1 and the Phase 1 exit, design s3.2 ("The shared façade is not behaviorally complete until ..."). It does not comply with owner decision Q5 as decided (constraint 17), which is an owner-decision conflict, not only a plan conflict. Design s4.2 hand-rolled needs an ADR (as Option A). Plan s3.3, s8.5 and design s4.4 human-mode refusal are superseded by Q6.

### Baseline: do nothing or defer

Ship nothing in intent 1. Hosts keep driving AIDLC through the projected `/dtech-aidlc` and `/aidlc` skills calling the CLI (Observed, harness manifests).
- AC-001 is trivially met. AC-002..AC-014 are unmet. Intent 2 is blocked, because it builds on the facade.
- Cost: zero. Risk: none added. It is fully reversible.
- A partial variant is **facade only** (plan Phase 1: Option A's facade, no MCP binary). It meets AC-001 and the shared-path half of AC-002, de-risks R1 early, and defers every protocol and packaging choice. It does not meet the intent's stated scope (User-provided: phases 1-2).

## 4. Open questions carried to direction-selection

These are not decided here. Owner or decision-facilitator.
- Q3 scope: does "offline cargo install keeps working" cover `dtech-aidlc-mcp` too? This decides Option B's residual risk and rules Option C in or out. Also: does the SDK behind a feature in the same package satisfy "CLI binary keeps only serde_json+sha2" (Option C only)? Owner: project owner.
- Q5 scope: does a guard in orchestrate.rs plus the MCP adapter, with no shared facade, satisfy "guard in shared facade, off for CLI"? This rules Option D in or out. Owner: project owner.
- Q3 ADR rationale: if the owner rules that Q3's offline promise covers `dtech-aidlc-mcp`, or accepts dependency footprint and churn as the blocker, Option A's ADR satisfies design s4.2 and plan Phase 0 step 3; otherwise A relies on Q3's ADR allowance. Owner: project owner.
- MSRV: confirm `rust-version = "1.100"` and whether "current stable" means track-and-bump or pin once. Owner: project owner.
- AC-003 tool-name wording versus the Q9 `aidlc_*` default.
- AC-009 pending prerequisites on report are required in every option (section 2). Open only in detail: the exact source of the list (next stage's preflight names plus unit needs) and the field shape. Only the owner reopening AC-009 would remove the work.
- Q5 details: whether the audit row is written under skip, the Blocked hint wording, and dedicated tests for sites 1470 and 1544.
- Q10/R6 concurrency: confirm no operation ID in v1, the retryable conflict code, and the documented compare-then-rename window (section 2).
- Error model: `String` plus adapter classification versus a typed `AppError` (Options A, B and D can do either; Option C requires `AppError`). The AC-012 conflict code weighs here: with `String` it depends on matching the `ensure_unchanged` message text.
- Q17: reconcile the plans' fixed ordering and open-decision lists with the code's per-command ordering. Deferred to implementation-design; every option keeps existing ordering.
- Host verification: DSH has not been run, and Claude Code was checked only via `claude mcp list` (no model call). Each option still needs a manual launch-and-list in both hosts (AC-003).
- Protocol detail: check the full 2026-07-28 `schema.json` (DiscoverResult, ListToolsResult, CallToolResult, whether a top-level `serverInfo` is allowed) before Develop if the modern era is chosen (Options A, B, C).
- Untested: Windows (CRLF, stdin, `.exe`) for all options (A5, R17). MCP binary size with embedded assets (A7). Read-call latency bound (N6).
