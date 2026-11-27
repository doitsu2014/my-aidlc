# Functional design

## 1. Purpose and trace
- Approved direction: Option A. A thin shared `application` facade in the library, a second `[[bin]]` `dtech-aidlc-mcp` in the same package, hand-rolled dual-era JSON-RPC over stdio (serde_json and std I/O, zero new crates), `rust-version = "1.100"` (ADR-001, ADR-002, ADR-003, ADR-004, ADR-006).
- Post-gate amendment: Windows and macOS are release-claimed platforms, Linux is best-effort, and a CI workflow runs build and test on windows-latest and macos-latest (FN-018, FN-019, CFG-003). Pushing to run CI needs separate owner authorization and is not part of this design.
- Acceptance criteria covered: AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013 and AC-014 (documentation contract, FN-021).
- This stage is design only. No source, test, Cargo, README, PROJECT.md or .github file is changed here.

## 2. Context
- Actors: an AI host (Claude Code, DeepSeek Harness) acting for the owner; the owner at a terminal using the `dtech-aidlc` CLI; CI runners.
- In scope: the four tools `status`, `queue`, `next`, `report`; the facade that both binaries call; the prerequisite guard (Q5); self-asserted approver label (Q6, Q18); path redaction; per-request era detection; CI workflow.
- Out of scope: any other tool (doctor, config prerequisite check, onboarding, repos, intent creation), an HTTP transport, install-script changes (N2), version bump and CHANGELOG (ADR-018), golden re-bless (N5, Q14). The two known lib-test baseline failures are fixed by WU-010 (tests only; supersedes N5 for those two tests).
- Existing components affected: `rust-cli/main.rs` (routing), `orchestrate.rs` (three prerequisite sites, `ensure_unchanged`, new preview), `prerequisites.rs`, `intent_commands.rs` (private `CHANGED`), `lib.rs`, `Cargo.toml`. New: `application.rs`, `mcp/{mod,protocol,tools,redact}.rs`, `mcp_main.rs`, `.github/workflows/ci.yml`, three new test files.
- Conventions kept: library functions return `Result<_, String>`; the CLI owns printing and exit codes; tests are Rust integration tests under `tests/`; goldens are never edited.

## 3. Functional decomposition

### FN-001 application::open
- Traces to: AC-002, AC-011 (ADR-006, ADR-016)
- Actor: CLI `next`/`report`/`queue`; MCP adapter on every call.
- Preconditions: a project path.
- Main flow: `orchestrate::resolve_like_node`, `audit::assert_safe_state_dir`, `external_engine()`, `scope::read_graph(engine_root)`, then `Engine::external` or `Engine::embedded(&root)`. The order is the exact order main.rs used.
- Alternate and error flows: any step failing returns its existing message; the MCP adapter maps it to `operation_failed` after redaction. The MCP opens fresh per call, so nothing is cached.
- Postconditions: a `Project{root, graph, engine, caller}`; nothing written.

### FN-002 application::external_engine
- Traces to: AC-002 (ADR-016)
- Main flow: the existing body moves unchanged; `main.rs` delegates. Both binaries resolve `DTECH_AIDLC_HOME` identically.
- Alternate flows: unset or unusable returns `None` (embedded engine). The six other library copies are not touched.
- Postconditions: no side effects.

### FN-003 application::status and application::queue
- Traces to: AC-006, AC-007
- Main flow: `status` passes through to `status::status`. `queue` runs `load_config_unvalidated` then `unit_commands::run("orchestrate queue", ...)` with default options.
- Alternate and error flows: unknown intent selector returns the CLI's error. Unmigrated state keeps `migration: pending` and nothing is persisted.
- Postconditions: read-only; state and audit bytes unchanged. The CLI status arm builds a `Project` from values it already holds so its error order cannot move.

### FN-004 application::knowledge_preflight
- Traces to: AC-001, AC-008
- Main flow: root-exists guard, then `knowledge::knowledge_preflight`, discarding the tuple.
- Notes: the CLI keeps its own call before intent-text and flag validation. The facade calls it inside `next` only for `Caller::Mcp`, so the CLI path runs it exactly once.

### FN-005 application::next
- Traces to: AC-001, AC-002, AC-008 (ADR-007, ADR-011, ADR-014)
- Actor: CLI, MCP.
- Preconditions: an opened `Project`.
- Main flow: for `Caller::Mcp` run FN-004, clone the options and force intent, scope, depth, mode, approved_by and detail to `None`, `force_resume=false`, `skip_prerequisite_checks=true`. Then `scope::validate_model_config`, `orchestrate::orchestrate_next`, and on a directive `orchestrate::finish_directive`. The `NextOutcome` is returned unchanged.
- Alternate and error flows: no active intent returns the CLI error (`operation_failed` over MCP, nothing created); a finished intent returns `Complete`; a prerequisite hold or a stalled queue returns `Blocked`; a concurrent run returns `Busy`. The facade never computes an exit code, so the CLI still maps `Blocked` to exit 3.
- Postconditions: for MCP, zero child processes; depth-approver identity lookup (new intents only) is unreachable.

### FN-006 application::report
- Traces to: AC-009, AC-010 (ADR-012, ADR-013)
- Main flow: for `Caller::Mcp`, first statement: if an approval value is present and `approved_by` is missing or empty after trim, return `APPROVER_REQUIRED` before any state read and before `identity::resolve_approver` (Q18). Otherwise `load_config_unvalidated`, `orchestrate_report_with_engine`, `finish_report`; for MCP then insert `pendingPrerequisites` (FN-020, always an array).
- Alternate and error flows: a labelled approval runs the unchanged CLI gate policy (AI-mode requires the AI label plus a gate recommendation; a human-pinned gate refuses the AI label). Stale state returns `STATE_CHANGED`. Accepted divergence: a labelless approval on a non-gate stage is also refused over MCP.
- Postconditions: audit schema unchanged; `bySource` stays `flag` (TH-16 residual accepted).

### FN-007 prerequisites::check_prerequisites and call-site swaps
- Traces to: AC-001, AC-008, AC-009 (ADR-011, TH-13)
- Main flow: `skip=false` delegates untouched to `run_prerequisites`. `skip=true` spawns nothing and returns one row per distinct name (see IF-003). The three call sites in `orchestrate_next` (currently near lines 1470, 1544, 1694) each pass `options.skip_prerequisite_checks`.
- Alternate flows (fail-closed): a not-run check counts as failing. Site 1: the unit is not claimed and a warning is added. Site 2: another ready unit is chosen or control falls to site 3. Site 3: `Blocked` with `needs` rows marked `executed:false` and skip-only wording; no `prerequisite.checked` audit row; held checks are not pushed.
- Postconditions: CLI wording and rows are byte-identical. `doctor` and `config prerequisite check` also call `run_prerequisites` but are not exposed over MCP (FN-013 asserts no such tool).

### FN-008 orchestrate::STATE_CHANGED hoist
- Traces to: AC-011, AC-012 (ADR-008, ADR-015)
- Main flow: the local string in `ensure_unchanged` becomes `pub const STATE_CHANGED`; `intent_commands.rs` uses it instead of its private `CHANGED`. Bytes are unchanged, so CLI output is identical. The existing test assertion is tightened to equality.

### FN-009 main.rs routing
- Traces to: AC-001, AC-002 (ADR-006, ADR-007)
- Main flow: `external_engine()` delegates; `status` arm builds a CLI `Project`; `run_orchestrate` calls `open`; `report` arm calls `application::report` (keeps the `--unit takes one work unit` check and list-flag parsing); `next` arm keeps preflight, intent text, flag validation, then calls `application::next` and matches `NextOutcome` (`Blocked` returns `BLOCKED_EXIT_CODE` 3, the others print JSON and return 0); `queue` goes through `application::queue`. Argument parsing, help, printing and exit mapping stay in main.rs.
- Error flows: all CLI messages and exit codes are unchanged.

### FN-010 mcp::protocol::read_frame
- Traces to: AC-004, AC-005 (ADR-005, TH-07, TH-08, TH-09)
- Main flow: bounded byte read (`take(MAX_LINE_BYTES + 1)` with `read_until(b'\n')`), strip a trailing `\r`, skip blank lines, validate UTF-8 per line.
- Alternate flows: oversized line drains the remainder and yields `-32600` with id null; bad UTF-8 yields `-32700` with id null; both keep the session alive. Never `lines()`. EOF ends the loop.

### FN-011 mcp::protocol::handle and dispatch
- Traces to: AC-003, AC-004 (ADR-004, ADR-005, TH-19, TH-21, TH-22)
- Main flow: validate the envelope (object, no batch, non-null id, `jsonrpc:"2.0"`) else `-32600`; echo id verbatim; never answer notifications; per-request era detection with one `initialized` flag: `initialize` is always legacy (echo `2025-11-25` or `2025-06-18`, else answer `2025-11-25`); a `_meta` protocol-version key present means modern (validate, `-32602` for malformed, `-32022` with `{supported, requested}` for unsupported; serve `server/discover`, `tools/list`, `tools/call`; `ping` and unknown methods `-32601`); key absent after `initialize` means legacy (`ping`, `tools/list`, `tools/call`); key absent before `initialize` answers `ping` with `{}` and everything else `-32602`. Modern results are stamped once in the dispatcher.
- Alternate flows: unknown tool name or non-object params give `-32602`. Each request runs under `catch_unwind` and a panic gives `-32603 Internal error` with no detail, and the next request is still served.

### FN-012 mcp::protocol::serve
- Traces to: AC-004
- Main flow: `read_frame`, parse (failure `-32700` id null), `handle`, then one compact `writeln!` plus flush per response under one stdout lock. Writes nothing at startup, initialize, discover or list. Stops on a write error; returns 0 on EOF.

### FN-013 mcp::tools::tool_list and validate_args
- Traces to: AC-003, AC-005 (ADR-009, TH-06, TH-07)
- Main flow: static schemas for four tools; a hand-written validator checks object shape, unknown or excluded fields, types, enums and the CFG-006 caps, and the CLI's claim-versus-unit wording. It runs before the project is opened.
- Error flows: `invalid_arguments` naming the field; no file access, so a nonexistent root stays untouched.

### FN-014 mcp::tools::call_tool
- Traces to: AC-006, AC-007, AC-008, AC-009, AC-010, AC-011 (ADR-007, ADR-008, ADR-010, ADR-013, ADR-014)
- Main flow: validate (FN-013), `application::open(root, Mcp)` fresh, run the facade operation, map `NextOutcome` to `{outcome, result}` (`Directive`, `Complete`, `Blocked`, `Busy`; all `isError:false`), redact the whole structured value, serialise the text block from the redacted value.
- Error flows: classify `Err(String)` by exact equality: `STATE_CHANGED` is `state_conflict` (retryable), `APPROVER_REQUIRED` is `approver_required`, anything else `operation_failed`. Messages are redacted before truncation to 1,000 characters. The adapter never retries.
- Tool descriptions state: the approver label is self-asserted and not authentication; `approval` is only for a decision the human owner made in this conversation; `next` does not execute prerequisite checks; `pendingPrerequisites` is a preview.

### FN-015 mcp::redact::Redactor
- Traces to: AC-006, AC-008, AC-011 (ADR-010, TH-11, TH-12)
- Main flow: built once at startup. Longest prefix first, boundary-checked: project root (lexical and canonical) becomes a project-relative forward-slash path or `.`; `DTECH_AIDLC_HOME` raw and resolved becomes `$DTECH_AIDLC_HOME`; home directory (`HOME`, or `USERPROFILE` on Windows) becomes `~`; any remaining string starting with `/`, a drive letter or `\\` becomes `<redacted-path>` (values) or generic text (error messages).
- Platform rules: structured values compare by path components; free text compares every textual variant (native, forward-slash, backslash, Windows verbatim `\\?\` and plain forms), ASCII case-insensitive only on Windows. On macOS both `/var/...` and `/private/var/...` are registered. Object keys are redacted as well as values.

### FN-016 mcp_main and mcp::run
- Traces to: AC-003, AC-004, AC-012, AC-013 (TH-17, ADR-016)
- Main flow: parse optional `--project <dir>` from `args_os`; root is that or `current_dir()`, resolved once through `resolve_like_node` and frozen; validate it is a directory and passes `assert_safe_state_dir`; build the redactor; warn once on stderr if `DTECH_AIDLC_HOME` is set but unusable; call `serve`.
- Error flows: any startup failure prints `dtech-aidlc-mcp: <msg>` to stderr, exits 1, writes nothing and never reads stdin.

### FN-017 Stdout purity and unwinding policy
- Traces to: AC-004, AC-011 (TH-14, TH-21)
- Behaviour: the serve loop is the only stdout writer; diagnostics, warnings and the panic hook use stderr; no profile sets `panic = "abort"`. Enforced by the child-process test, not by library change.

### FN-018 CI workflow
- Traces to: AC-001, AC-003, AC-013 (post-gate amendment)
- Main flow: `.github/workflows/ci.yml` runs on push and pull_request; a `build-test` matrix over windows-latest and macos-latest (ubuntu-latest optional, `continue-on-error`) installs toolchain 1.100, runs `cargo build --locked`, `cargo test --locked --no-fail-fast`, then a separately named step for the three MCP test targets. macOS sets `TMPDIR=/private/tmp/aidlc-test`; both OSes put `target/debug` on PATH.
- Error flows: the two known Unix-only lib failures need an owner decision (see implementation plan); the workflow does not hide MCP test results behind skip lists. Install scripts are never run.

### FN-019 Platform portability and state-replace mapping
- Traces to: AC-011, AC-012
- Behaviour: only `redact.rs` uses `cfg!(windows)`. Everything else is std-portable. On Windows a rename over `state.json` held open by another process can fail with a permission error; it surfaces as `operation_failed` (not retryable), an accepted limitation documented in the README.

### FN-020 orchestrate::pending_prerequisites
- Traces to: AC-009 (ADR-012)
- Main flow: read-only; load state, clone it, predict the next selection (work-unit selection when a ledger exists, else next-stage selection), apply the same `needed` rule as the `orchestrate_next` blocked path (extracted into one private helper used by both), return skip-form rows. Empty list when complete or nothing is needed.
- Notes: a preview only; claim, pinned and parallel selections can differ from a later `next`.

### FN-021 README and host-configuration documentation contract
- Traces to: AC-014
- Main flow: README gains a `## MCP server (proposed)` section (WU-009) giving install, Claude Code and DeepSeek Harness configuration, Windows and macOS notes, and the self-asserted approver and `state_conflict` notes. Every sh-block command line is `dtech-aidlc-mcp` or a non-command line, so `tests/repo_content.rs` help-agreement checks stay green.
- Notes: no behaviour; verified by TS-015 (text checks) and TS-013 (manual host launch).

## 4. Data
- Entity `Project`: root (frozen at launch), graph, engine, caller. Not persisted.
- Entity structured result: JSON object; Fields per IF-006. Validation: `additionalProperties:false` inputs with CFG-006 caps. Retention: none; state and audit files keep their existing schema.
- Entity prerequisite row: `{name, ok, hint, exitCode, executed, undeclared?}`; the owner's `check` command line is never included (TH-12, TH-13).

## 5. Interfaces
Contracts are in interfaces-and-contracts.md: IF-001 facade, IF-002 error constants, IF-003 and IF-004 prerequisite functions, IF-005 protocol boundary, IF-006 wire contract, IF-007 redactor, IF-008 adapter, IF-009 binaries, IF-010 CI workflow. Idempotency: `status` and `queue` are idempotent; `next` and `report` are not and are never retried by the server (ADR-015).

## 6. Interaction
- Caller: host process over stdio. Callee: `serve` (FN-012) then `handle` (FN-011), `call_tool` (FN-014), then the facade (FN-001 to FN-006, FN-020), then existing library code.
- Sequence for a tool call: read frame, parse, era check, validate arguments, open project, facade operation, map outcome or error, redact, wrap in the result shape for the era, write one line.
- CLI sequence: argument parsing in main.rs, facade, print, exit code.

## 7. State
- Server states: before-initialize, initialized (legacy), and per-request modern; stateless per call apart from the `initialized` flag.
- Transitions: `initialize` sets the flag; EOF exits 0.
- Forbidden: serving `tools/*` before `initialize` without a modern `_meta` key; creating an intent over MCP; running any prerequisite command; changing the root after launch.
- Workflow state transitions are those of the existing engine, unchanged.

## 8. Dependencies
- serde_json and sha2 only (already present); Cargo.lock stays byte-identical (CFG-001, CFG-002).
- Engine files via `DTECH_AIDLC_HOME` or the embedded engine; failure falls back to embedded with one stderr warning.
- Failure behaviour: any library error becomes a redacted `operation_failed`; a stdout write error ends the server.
- CI: GitHub-hosted runners; first run is the Windows baseline measurement.
