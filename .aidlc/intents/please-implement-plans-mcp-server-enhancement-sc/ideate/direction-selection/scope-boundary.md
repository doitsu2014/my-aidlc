# Scope boundary: shared facade and MCP stdio server (intent 1 of 2)

- Purpose: state what intent 1 delivers under the selected direction (Option A, dual-era protocol, both owner-decided), what it leaves out, what is deferred and under which trigger, which decisions still belong to the project owner, and which plan statements the direction supersedes.
- Status: Draft - pending solution-direction gate.
- Stage: Ideate / direction-selection (decision-facilitator). Companions: [recommended-direction.md](recommended-direction.md), [decision-record.md](decision-record.md).
- Sources: intent scope (User-provided: plan phases 1-2); `analyze/problem-framing/{problem-statement,user-stories,acceptance-criteria}.md`; `analyze/context-and-risk/{constraints-and-assumptions,open-questions}.md`; [solution-options.md](../option-generation/solution-options.md) section 2; owner decisions of 2026-11-27 (User-provided); `tests/repo_content.rs`, `tests/rust_cargo_install.rs`, `Cargo.toml`, `CHANGELOG.md` (Observed 2026-11-27); `plans/mcp-server-enhancement/*.md` (the owner's proposal).

## 1. In scope (intent 1)

| Item | Criteria / stories | Decision |
|---|---|---|
| Thin shared `application` facade; the CLI routes status, queue, next and report through it with byte-identical output | AC-001, AC-002; US-001, US-002 | ADR-006, ADR-007, ADR-016 |
| `dtech-aidlc-mcp` second `[[bin]]`, zero new crates, Cargo.lock unchanged, `--project`/`--help`/`--version` | AC-003, AC-013; US-003, US-009 | ADR-002 |
| `rust-version = "1.100"` in `Cargo.toml` | AC-013 | ADR-003 |
| Hand-rolled dual-era JSON-RPC over stdio: the legacy `initialize` flow (2025-11-25, 2025-06-18) and the stateless 2026-07-28 flow (`server/discover`, per-request `_meta` protocolVersion and clientCapabilities, `resultType`, `ttlMs`/`cacheScope`, -32022), with per-request era detection, framing, lifecycle and panic containment | AC-003, AC-004; US-003 | Owner-decided dual-era (User-provided, 2026-11-27); ADR-001, ADR-004, ADR-005 |
| Four tools `status`, `queue`, `next`, `report` with strict schemas, MCP-only caps and annotations | AC-003, AC-005, AC-006, AC-007; US-004, US-005 | ADR-009 |
| Error model: `invalid_arguments`, `approver_required`, `state_conflict` (retryable, never auto-retried), `operation_failed`; JSON-RPC codes for protocol faults only | AC-005, AC-011, AC-012; US-004 | ADR-008, ADR-015 |
| Recursive path redaction in the adapter only | AC-006, AC-008; US-005, US-006 | ADR-010 |
| Q5 skip at all three prerequisite sites, fail-closed, zero child processes over MCP | AC-008, AC-009; US-006, US-007 | ADR-011 |
| `pendingPrerequisites` preview on MCP report | AC-009; US-007 | ADR-012 |
| Q6/Q18 approver handling; CLI gate policy reused; no approve-gate tool | AC-010; US-008 | ADR-013 |
| MCP next continues only an existing active intent | AC-008; US-006 | ADR-014 |
| Tests: transcript and child-process protocol tests for both eras (legacy cases plus about 6 modern-era cases: Claude Code and TS-client modern replays, -32022, missing `clientCapabilities`, era mixing, a result-shape sweep; ADR-001), stdout purity, Q5 fixtures for sites 1470/1544/1694, Q18 PATH-shim test, path-scan test, interleaving conflict test, CLI-versus-MCP parity tests, `tests/rust_cargo_install.rs` extended to `--bin dtech-aidlc-mcp` with a clean `CARGO_HOME`, Cargo.lock no-diff check | AC-001..AC-013 | ADR-001, ADR-002 |
| "Unchanged" check: the same pass/fail set before and after, with TMPDIR under `/private/tmp` and `target/debug` on PATH, and no golden re-bless | AC-001 | Owner-decided (N5/Q14, User-provided 2026-11-27) |
| Manual host checks: Claude Code and DeepSeek Harness both launch the server and list the four tools; each check records the era the host negotiated | AC-003; US-003 | Owner-decided (Q1); ADR-004 |
| Host-configuration docs with copy-ready examples and troubleshooting, every MCP statement labelled "proposed" (see section 2) | AC-014; US-009 | ADR-017 |

## 2. README, CHANGELOG and host-config docs

Q12 is answered: host-config docs ship in intent 1 (User-provided scope; open-questions.md "Answered by evidence"). N2 puts README and changelog release work in intent 2. The repository's content tests constrain both:

- **Observed (tests/repo_content.rs:1411-1433):** the README must keep six `## ` sections (Onboard a project, Project knowledge, What to commit, Harnesses, Upgrade and uninstall, Environment variables) and five mentions. Adding a section does not break this.
- **Observed (repo_content.rs:1436-1459):** every README `sh`-block line that starts with `dtech-aidlc` followed by whitespace must name a command listed by `--help`. Lines starting `dtech-aidlc-mcp`, `claude mcp add ...` or `cargo install ...` do not match, so host-config examples pass.
- **Observed (repo_content.rs:1462-1493):** `CHANGELOG.md` must contain `## <crate version>`, and the README version badge must show the crate version. The crate is at 1.2.2 (Cargo.toml), and `## 1.2.2` exists. Without a version bump, no CHANGELOG or badge edit is required by the tests.

The decisions are recorded once, in the decision record. Summary:

1. **Host-config docs location:** a new README section `## MCP server (proposed)` after `## Harnesses`; existing install sections unchanged ([ADR-017](decision-record.md#adr-017-host-configuration-docs-location)).
2. **No crate version bump and no CHANGELOG edit in intent 1:** the entries for the new binary and the `rust-version` pin come with the intent-2 release ([ADR-018](decision-record.md#adr-018-no-version-bump-and-no-changelog-edit-in-intent-1)).
3. **Other README work stays in intent 2:** Highlights, Install options, Repository layout and any release wording (US-013, plan phases 3-5).
4. **Install scripts unchanged** (N2, owner-decided).

## 3. Out of scope

**Intent 2 (starts after intent 1 closes; User-provided scope):**
- Telemetry schema, sink, retention, redaction and privacy tests (US-010); workflow and operation event instrumentation (US-011); the `--telemetry` flag (Q11: omitted in intent 1).
- The monitor command and dashboard, including `mcp-studio-server.html` (US-012).
- Plan phases 3-5: hardening, README beyond the host-config section, CHANGELOG, version bump and release (US-013).
- Install-script support (`scripts/rust-install/install.sh`, `install.ps1`) for the MCP binary (N2, owner-decided).

**Non-goals in both intents (carried from plan s1; constraint 12):** CLI replacement; network or HTTP transport; shell, git, file or config tools; MCP resources, prompts, subscriptions or sampling; park and revisit over MCP; a separate approve-gate tool; authenticated approval; a second state store; any phase-graph, state or audit schema change; automatic editing of host configuration.

**Explicitly not changed in intent 1:** CLI output, exit codes, file effects and audit rows (AC-001); CLI prerequisite execution and child stdin inheritance (N1); the six library copies of `DTECH_AIDLC_HOME` resolution; the two pre-existing library test failures (N5, owner-decided: stay out of scope); install scripts; host settings.

## 4. Deferred, with triggers

| Item | Trigger to revisit | Where recorded |
|---|---|---|
| A protocol revision newer than 2026-07-28, or removal of the legacy era | A new revision is published (add it only after its schema is checked), or the spec deprecates the `initialize` era or 2025-11-25 | ADR-004 |
| An MCP SDK instead of hand-rolled | The conformance burden grows, a non-stdio transport is needed, or the owner relaxes Q3 | ADR-001 |
| `outputSchema` on tools | Payload shapes approved and frozen | ADR-008 |
| Typed `AppError` and finer error codes (for example a distinct I/O code) | Intent 2 telemetry needs them, or the owner reads AC-011 that way | ADR-008 |
| Operation ID or file lock | A lost update or torn audit line is observed | ADR-015 |
| CLI prerequisite children with stdin set to null (N1) | Owner-approved CLI behaviour change, with a CHANGELOG entry | ADR-011 |
| An explicit owner opt-in for running prerequisites over MCP | New owner decision on Q5 | ADR-011 |
| A stricter approver-label character policy (bidi, zero-width, DEL, C1) for both CLI and MCP | Owner decision (F2) | ADR-013 |
| MCP provenance in the audit | Owner decision plus an AC-002/AC-009 change, or intent-2 telemetry | ADR-013 |
| Refusing an invalid `DTECH_AIDLC_HOME` instead of silent CLI fallback | Owner decision (F3) | ADR-016 |
| Windows verification and external CI | Owner confirms CI (N3, default: none, platform claims limited to local tests) | recommended-direction.md section 4 |

## 5. Owner decisions and open gate items

Already decided by the owner (User-provided, 2026-11-27): the Option A direction (chosen over B), the dual-era protocol for v1 (overriding this stage's earlier legacy-only recommendation; ADR-004), MSRV 1.100 pinned, Q1, Q3 (SDK question moot), Q5, Q6, Q18, N2, N5/Q14, acceptance of the conductor's codebase-map review, and Q2 (the plans are the owner's proposal, evaluated in Ideate).

Gate handling: this directive's `gateMode` is `ai`, so the solution-direction gate is decided by the AI Orchestrator under the gate-recommendation policy (orchestrate.rs:2750-2769). The gate decision is not an owner action.

| # | Item | Recommended default | Effect if decided otherwise | Gate classification |
|---|---|---|---|---|
| O1 | AI-mode gate decision on the solution direction | Approve A with the owner-decided dual-era protocol and the recommended ADRs | On rejection, Ideate revisits options; Develop does not start | Not an open item: it is the gate decision itself |
| O2 | Host-config docs location ([ADR-017](decision-record.md#adr-017-host-configuration-docs-location)) | README section `## MCP server (proposed)` | A separate file changes only where implementation-design puts the text | `[follow-up]` |
| O3 | CHANGELOG and version-bump timing for the new binary and the `rust-version` pin ([ADR-018](decision-record.md#adr-018-no-version-bump-and-no-changelog-edit-in-intent-1)) | No bump and no CHANGELOG edit until the intent-2 release | An entry now would need a new heading convention (for example "Unreleased"), because the tests key headings to the crate version | `[follow-up]` |

Classification (this stage's call, per decision-facilitator.md): O2, O3 and F1-F9 below are `[follow-up]`. Each has a safe, reversible default, none changes the direction, and none is a precondition of the direction. No item is `[written-approval]`. If the gate-recommendation writer judges that an AC reinterpretation needs the owner before Develop (F3, the AC-002 reading, or F9, the AC-012 window), that item is to be tagged `[written-approval]`, and the AI Orchestrator then cannot approve the gate.

## 6. Follow-ups for owner awareness (no AC text changed here)

- **F1. AC-010 pending scenario.** The scenario "approval without an explicit label (pending the Ideate decision)" and the US-008 parenthetical "(proposed Ideate default, Q18, not an owner decision)" are now settled by the owner's Q18 decision. The Analyze artifacts were not edited by this stage. implementation-design treats the scenario as decided.
- **F2. Approver label characters.** v1 keeps CLI parity, because AC-010 says "exactly as the CLI handles that label". Bidi and zero-width characters can spoof audit display (TH-03, residual Low). Tightening needs an owner decision covering both CLI and MCP.
- **F3. `DTECH_AIDLC_HOME` fallback.** The CLI silently uses the embedded engine when `DTECH_AIDLC_HOME` is set but unusable (main.rs:403-409). The MCP binary adds a stderr warning but selects the same engine. The AC-002 phrase "neither silently falls back" is read as parity between the two binaries.
- **F4. AC-011 codes.** Domain and I/O failures share `operation_failed`.
- **F5. Tool names.** `status`, `queue`, `next`, `report` match AC-003 verbatim, so no AC wording change is needed. They depart from the Q9 default and the plans' `aidlc_*` names.
- **F6. Fail-closed Q5 ceiling.** MCP `next` cannot hand out a stage that has preflight prerequisites or a blocked report naming `needs`. After an MCP `report result=blocked needs=[x]`, MCP `next` on that stage or unit returns Blocked until a new report clears the block, even after the owner has run the CLI `next` (orchestrate.rs:1653-1688; ADR-011). The owner runs the CLI `next` for such a stage.
- **F7. Repudiation.** MCP approvals cannot be told apart from CLI approvals in `audit.jsonl` (`bySource: "flag"`).
- **F8. Host verification gaps.** DeepSeek Harness has not been run, no model-driven `tools/call` was observed in Claude Code in either era, and Claude Code's reaction to a malformed modern response was not tested. Both observed hosts take the modern path by default, so the manual checks should exercise it. These are AC-003 checks before the intent closes.
- **F9. AC-012 concurrency window.** The optimistic `ensure_unchanged` compare-then-rename is not atomic (ADR-015). Two writers can both pass the compare inside that window, so AC-012 ("shall not corrupt intent state"; at most one conflicting write succeeds) is met only up to the window. The window is the same as two concurrent CLI runs have today. AC-012's Source names the sufficiency of the current concurrency check as an owner open question. No lock and no operation ID in v1 (threat-model.md TH-15).

## 7. Plan statements the direction supersedes

The plans are the owner's proposal (Q2). Where the direction departs from them, the governing source is named.

| Plan statement | Superseded by | Governing source |
|---|---|---|
| MCP must not submit an approved decision for a human-mode gate; the owner completes it through the CLI (architecture line 32; plan s3.3 lines 147-148, risk table line 484, s8.5 line 503; design s4.4 lines 146-147, s10 item 3) | MCP report accepts a self-asserted label under the CLI gate policy; omitted label refused | Owner decisions Q6, Q18; ADR-013 |
| Select a maintained Rust MCP SDK; do not hand-roll unless the SDK evaluation documents a concrete blocker (plan Phase 0 step 3, line 354; design s4.2); implement "through the selected SDK" (plan Phase 2 step 2) | Hand-rolled JSON-RPC under Q3's ADR allowance | Owner decision (Option A); ADR-001 |
| `ProjectContext` resolved once per process with a "canonicalized" root (design s3.1; plan line 104) | Opened per call; lexical root via `resolve_like_node` | ADR-006; Q16 |
| `Result<Value, AppError>` facade signatures (design s3.2 lines 74-77, 90) | `String` errors plus `NextOutcome`; codes classified in the adapter | ADR-007, ADR-008 |
| Tool names `aidlc_status`, `aidlc_queue`, `aidlc_next`, `aidlc_report` (design s4.3 headings; plan s3.3 table; plan line 386; telemetry wireframes) | `status`, `queue`, `next`, `report` | ADR-009 (AC-003 wording) |
| `--telemetry on\|off`, default on (plan line 119) | No telemetry flag in intent 1 | Q11; intent 2 |
| "Expose requirements or a separately approved explicit opt-in" for prerequisites (plan s3.3 line 132) | No opt-in in intent 1; fail-closed skip | Owner decision Q5; ADR-011 |
| Include relevant artifact paths (plan s3.3) versus redact every absolute path (design s4.3) | Recursive redaction to project-relative or placeholder forms | ADR-010 |
| Results and operation IDs "should be used where available" (design s5 line 157) | No operation ID in v1 | ADR-015 |
| `aidlc_next` creates an intent without intent text (design s4.3) | Code mismatch: an intent is created only with intent text (orchestrate.rs:1272); MCP next never supplies it | ADR-014 |

Consistent with the plans, not superseded: design s2's two public binaries (`dtech-aidlc`, `dtech-aidlc-mcp` from `rust-cli/mcp_main.rs`) and its `application.rs` module name (ADR-002, ADR-006). Design s2's "decide and document an MSRV" is met by ADR-003.

## Amendment after the solution-direction gate (2026-11-27)

- **Owner decision (User-provided, project owner, 2026-11-27, in the conductor session after the gate was approved):** the MCP server must support **Windows and macOS** ("Window and MacOS too"), and Windows support is verified in this intent by **adding a GitHub Actions CI workflow** that runs `cargo build`, `cargo test` and the MCP transcript tests on `windows-latest` and `macos-latest`.
- **What this changes:** the row "Windows verification and external CI" above (N3 default: no CI; platform claims limited to local tests) is superseded. Windows/macOS support and the CI workflow file under `.github/workflows/` are now in scope. Linux is not named by the owner and stays best-effort (not a release claim).
- **Consequences:** the release-candidate gate needs a green CI run on both platforms. That run happens only after the branch is pushed, and pushing is an external action that needs the owner's separate authorization. Design must cover Windows specifics already noted as untested (CRLF input, stdin/stdout handling, `.exe` names, path separators in redaction, A5/R17). The baseline definition for "unchanged" (N5/Q14) applies to macOS locally; CI establishes the Windows baseline on its first run.
- **Decision record:** this amends the scope only; ADR-002 (packaging) and ADR-005 (framing, CRLF stripping) already cover the platform behaviour. implementation-design records the CI work as a work unit and mints its identifiers.

## Amendment at the release-candidate gate (2026-11-28)

- **Owner decision (User-provided, project owner, 2026-11-28, in the conductor session after the release-candidate gate was blocked on AC-003):** "không cần deepseek harness giờ đâu" (DeepSeek Harness is not needed now).
- **What this changes:** owner decision Q1 (both Claude Code and DeepSeek Harness must launch and list the tools, AC-003) is narrowed for this release: Claude Code is the required host; DeepSeek Harness launch-and-list verification is deferred (follow-up, later intent). The README keeps the DeepSeek Harness configuration, labelled proposed and not verified.
- **Unchanged:** every other acceptance criterion and owner decision.
