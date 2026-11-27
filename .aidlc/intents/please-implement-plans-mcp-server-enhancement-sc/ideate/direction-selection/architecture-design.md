# Architecture design

- Purpose: the arc42 view of the selected direction for intent 1 (shared `application` facade plus the `dtech-aidlc-mcp` stdio server), so implementation-design can mint contracts against one picture. It describes the approved direction only. Decisions are written once, in [decision-record.md](decision-record.md), and are only listed in section 9.
- Status: Draft - pending solution-direction gate. The direction (Option A, Lean hand-rolled) and its dual-era protocol scope are owner-decided (User-provided, project owner, 2026-11-27; dual-era overrides this stage's earlier legacy-only recommendation). The per-request era rules and the other technical detail are Recommended until the gate is passed.
- Stage: Ideate / direction-selection (decision-facilitator). Companions: [recommended-direction.md](recommended-direction.md), [decision-record.md](decision-record.md), [scope-boundary.md](scope-boundary.md), [threat-model.md](threat-model.md).
- Existing architecture: not restated here. See [the codebase map](../../../../knowledge/_codebase-map.md) (components, entry points, data, tests, known debt) and [PROJECT.md](../../../../../PROJECT.md) sections "Architecture overview" and "Constraints and non-negotiables". No `adr`, `architecture` or `api-spec` source is registered in `aidlc.knowledge.json` (Observed, 2026-11-27).
- Labels: Observed (path or URL, 2026-11-27 unless stated), User-provided, Assumption, Estimate, Open question. Code lines are Observed at commit 69cab5a. `<spike>` = `/private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/spike/`. No FN-/IF-/TS-/WU-/CFG- identifiers are minted here; interface names below are descriptive, and implementation-design registers them.

## 1. Introduction and goals
- Purpose: let people drive AIDLC through an MCP-capable assistant (Claude Code, DeepSeek Harness) with four structured tools, `status`, `queue`, `next` and `report`, over the same library behaviour the CLI uses, while the CLI stays byte-identical (US-001..US-009, AC-001..AC-014). Telemetry, the monitor and the release work are intent 2 (US-010..US-013).
- Quality goals: see [quality-requirements.md](../../analyze/context-and-risk/quality-requirements.md). In priority order for this design:
  1. **CLI parity.** Same pass/fail set, no golden re-bless, same exit codes and audit rows (AC-001; Operability).
  2. **Governance integrity.** MCP cannot bypass gate policy, run owner shell checks or borrow an identity (AC-008, AC-009, AC-010; owner decisions Q5, Q6, Q18).
  3. **Protocol channel integrity and host compatibility.** Stdout carries only frames, one bad frame never ends a session, and both hosts launch and list the tools on whichever era they negotiate, legacy or 2026-07-28 (AC-003, AC-004; Availability).
  4. **Privacy.** Zero absolute paths, secrets, stack traces or file contents in MCP output (AC-006, AC-011; Privacy).
  5. **Offline, dependency-free install.** Zero new crates, Cargo.lock unchanged, offline `--locked` install for both binaries (AC-013; Q3).
- Stakeholders: see [stakeholder-map.md](../../analyze/problem-framing/stakeholder-map.md). Design-relevant roles: the assistant user (primary), the project owner (approver, CLI user, decision-maker), the MCP host (integration peer, not an authorization authority), maintainers (review the facade refactor and the hand-rolled protocol).

## 2. Constraints
- Technical:
  - CLI behaviour is frozen and snapshot-tested (PROJECT.md "Constraints and non-negotiables"; constraint 1). Argument parsing, validation order, stdin reading, printing and exit codes stay in `rust-cli/main.rs`.
  - Dependencies stay `serde_json` (with `preserve_order`) and `sha2`; all synchronous; Cargo.lock committed and unchanged; offline `--locked` install for both binaries (Q3, owner decision; constraints 3, 5, 6, 16).
  - `rust-version = "1.100"` (owner decision, 2026-11-27).
  - No phase-graph, state or audit schema change; no second state store (constraint 12).
  - Root resolution stays lexical (`resolve_like_node`, orchestrate.rs:381; Q16), and `.aidlc` symlinks are refused (`assert_safe_state_dir`).
  - Library errors are `String` (orchestrate.rs:34). The library prints nothing on the four operation paths (Observed, grep of `rust-cli`).
- Organizational:
  - Owner decisions of 2026-11-27 (User-provided): Option A over B, dual-era scope (ADR-004), MSRV, Q1, Q3, Q5, Q6, Q18, N2, N5/Q14, Q2. See [decision-record.md](decision-record.md) "How to read the status line".
  - Two intents: this design covers intent 1 only.
  - No in-repo CI (R17): platform claims are limited to local tests on macOS (Observed environment); Windows is untested.
  - Install scripts stay CLI-only (N2). "Unchanged" means the same pass/fail set with TMPDIR under `/private/tmp` and `target/debug` on PATH (N5/Q14).
- Convention:
  - Every MCP statement in user docs is labelled "proposed" until released (constraint 13; AC-014).
  - A golden change needs `DTECH_AIDLC_BLESS=1` plus a CHANGELOG entry; none is planned (PROJECT.md).
  - `tests/repo_content.rs` constrains README sections, README `sh` blocks and CHANGELOG headings (Observed, lines 1411-1493; see scope-boundary.md section 2).

## 3. Context and scope
- System context (C4 System Context):

```text
   assistant user ── prompts ──> MCP host (Claude Code | DeepSeek Harness) <── model output (untrusted)
                                        │ spawns, stdio JSON-RPC (newline-delimited)
                                        v
   project owner ── terminal ──> dtech-aidlc (CLI)        dtech-aidlc-mcp (new)
                                        │                        │
                                        └──── shared library ────┘  (application facade + existing modules)
                                                    │ reads/writes
                                                    v
                          <project>/.aidlc/ (pointer, intent state, audit.jsonl, config.json, engine/)
                          embedded engine (in the binary) or external engine ($DTECH_AIDLC_HOME)
```

| Element | Responsibility | Interface |
|---|---|---|
| Assistant user | Asks the assistant to advance work; confirms or denies tool calls at the host | Host UI |
| MCP host | Launches `dtech-aidlc-mcp` as a child, relays tool calls from the model, shows results; namespaces tools by server (`mcp__<server>__<tool>` in Claude Code; server-qualified names in DSH; Observed in host docs, solution-options section 2 and `<spike>/host-compat/dsh/README.md:58`) | stdio: one JSON-RPC 2.0 message per line; legacy (2025-11-25, 2025-06-18) and 2026-07-28 MCP eras, chosen per request (ADR-004, ADR-005) |
| `dtech-aidlc-mcp` | Serves four tools for one project fixed at launch | stdin/stdout frames; stderr diagnostics; `--project`, `--help`, `--version` |
| `dtech-aidlc` | Existing CLI, unchanged bytes | argv, stdout JSON, exit codes 0/1/3 (PROJECT.md) |
| Project owner | Runs the CLI, decides gates, runs `orchestrate next` for stages with preflight prerequisites (ADR-011 ceiling) | Terminal |
| `.aidlc/` project state | Workflow pointer, intent state, append-only audit, config, materialised engine (codebase map section 5) | Files; optimistic `ensure_unchanged` compare-before-write |
| Engine | Phase graph and methodology text, embedded or from `DTECH_AIDLC_HOME` | Files; resolved once per call by one shared function (ADR-016) |

- External systems: the two hosts (Claude Code 2.1.332 Observed; DeepSeek Harness via its pinned `@modelcontextprotocol/client` 2.2.0, Observed in `<spike>/host-compat/dsh/package.json`, not run). `git` and `sh` are external only to the CLI path; the MCP server spawns no child process (ADR-011, ADR-013, ADR-014).
- Out of scope: see [scope-boundary.md](scope-boundary.md) section 3. In short: telemetry, the monitor, release work and install-script support (intent 2); network transport, shell/git/file/config tools, resources, prompts, sampling, park/revisit over MCP, a separate approve-gate tool, authenticated approval, schema changes and host-config auto-edit (non-goals, both intents).

## 4. Solution strategy
- Chosen approach: Option A "Lean hand-rolled" with a dual-era protocol, both owner-decided (see [recommended-direction.md](recommended-direction.md)).
  - One thin library facade (`application`) is the single behaviour path for the four operations; the CLI routes those four commands through it and keeps its own parsing, printing and exit codes, so CLI bytes cannot move.
  - The MCP-specific policy that must hold for every caller of the facade (Q5 skip, Q18 guard, continue-only `next`) sits in the facade, switched by `Caller::Mcp`. Everything that is only about MCP presentation (schemas, caps, outcome wrapping, error codes, path redaction) sits in the MCP adapter, so it can never change CLI bytes.
  - The protocol layer is a small synchronous loop behind a `(method, params) -> Result<Value, Value>` boundary. It knows nothing about AIDLC. It serves both eras: per-request era detection, modern `_meta` validation and result stamping live only here, so the adapter and the facade are era-blind. An SDK can replace it, and either era can be removed, without touching the adapter or the facade.
  - A second `[[bin]]` in the same package adds no crates; one lockfile serves both binaries.
- Rejected alternatives: B (rmcp isolated in a nested package), C (plans as written), D (no facade), do-nothing, and A-legacy (this stage's earlier recommendation, superseded by the owner's dual-era decision). Reasons are in [recommended-direction.md](recommended-direction.md) section 3 and are not repeated here.

## 5. Building block view
- Containers (C4 Container):

| Container | Responsibility | Interface |
|---|---|---|
| `dtech-aidlc` binary (`rust-cli/main.rs`) | Parse argv, validate flags in the existing order, read stdin for `--intent-file -`, build a `Project` with `Caller::Cli` for status/queue/next/report, print JSON, map `NextOutcome::Blocked` to exit 3 | argv, stdout, exit code |
| `dtech-aidlc-mcp` binary (new; file layout per implementation-design, plan s3.2 suggests `rust-cli/mcp_main.rs`) | Fix the project root at launch, run the stdio loop, host the protocol layer and the tool adapter | stdin/stdout frames, stderr, `--project`, `--help`, `--version` |
| Library crate (existing modules plus new `application` module) | All domain behaviour; embedded engine assets via build.rs (each binary carries a copy; Estimate about 3.8 MB each, not measured) | Rust API |

- Components (C4 Component), inside `dtech-aidlc-mcp` and the library:

```text
 stdin ─> [Frame reader] ─> [Dispatcher / lifecycle] ─> [Tool adapter] ─> [application facade] ─> [existing domain modules]
                                  │                          │                    │
 stdout <─ [Frame writer] <───────┘ stamps modern results    │ redaction,         └─ Caller::Mcp switches:
                                                             │ outcome wrap,         Q18 guard, Q5 skip,
                                                             │ error codes           continue-only next,
                                                             v                       pendingPrerequisites
                                                     tool result Value
```

| Component | Responsibility | Interface (descriptive; registered later) | Decisions |
|---|---|---|---|
| Frame reader | Bounded byte read per line (cap Estimate 1 MiB), drain oversized lines, strip `\r`, skip blank lines, UTF-8 check, `serde_json` parse with the default depth limit | `next_frame() -> Frame \| FrameError`; EOF ends the loop with exit 0 | ADR-005 |
| Frame writer | The only stdout writer: one compact JSON line plus flush per response, under the stdout lock | `write(Value)` | ADR-005 |
| Dispatcher / lifecycle | JSON-RPC envelope checks (id, `jsonrpc`, batch); per-request era detection (modern when `_meta` carries the protocol-version key; R1a); modern `_meta` validation (-32602, -32022 with `{supported, requested}`); legacy `initialize` negotiation over `["2025-11-25", "2025-06-18"]`; `server/discover` with the exact 2026-07-28 DiscoverResult; legacy-only `ping`; `notifications/*` unanswered; pre-initialize handling; era result stamping (`resultType`, `ttlMs`/`cacheScope`, `_meta` serverInfo on modern results only); -32601 for everything else; `catch_unwind` per dispatch | `handle(method, params) -> Result<Value, Value>` (the replaceable boundary; the era is decided inside it) | ADR-001, ADR-004, ADR-005 |
| Tool adapter | Static `tools/list` (names, `inputSchema` with `additionalProperties: false`, annotations, descriptions); validate arguments and caps before the project is opened; open the project; call the facade; wrap `NextOutcome` as `{outcome, result}`; classify errors by exact equality into `invalid_arguments`, `approver_required`, `state_conflict`, `operation_failed`; redact paths; build `{content, structuredContent, isError}`; era-blind, so the same call gives the same result body in both eras | `list_tools() -> Value`; `call_tool(name, arguments) -> Result<Value, Value>` (-32602 only for unknown tool or malformed call) | ADR-007, ADR-008, ADR-009, ADR-010 |
| `application` facade (new library module) | `Caller {Cli, Mcp}`; `Project {root, graph, engine, caller}` opened per call; `external_engine()` shared by both binaries; four operations; the Mcp-only policy switches | `open(project_path, caller) -> Result<Project, String>`; `status(&Project, Option<&str>) -> Result<Value, String>`; `queue(&Project) -> Result<Value, String>`; `next(&Project, &NextOptions) -> Result<NextOutcome, String>`; `report(&Project, &ReportOptions) -> Result<Value, String>`; `pub const APPROVER_REQUIRED` | ADR-006, ADR-011..ADR-014, ADR-016 |
| Domain changes (existing modules, minimal) | `orchestrate`: `pub const STATE_CHANGED` (same bytes as today; intent_commands.rs drops its private `CHANGED` copy and uses it), `NextOptions.skip_prerequisite_checks` (default false), one helper for the site-1694 `needed` computation, new read-only `pending_prerequisites`. `prerequisites`: `check_prerequisites(root, config, names, skip)`, identical to `run_prerequisites` when `skip == false` | Rust API; CLI path unchanged by construction | ADR-008, ADR-011, ADR-012 |
| Existing domain modules (unchanged) | `status`, `unit_commands` (queue), `orchestrate_next`, `orchestrate_report_with_engine`, `finish_directive`, `finish_report`, `identity::resolve_approver`, `audit`, `workflow`, `scope`, `knowledge` | As today; see the codebase map section 2 | none |

- Responsibilities (split rule):
  - Facade: behaviour every caller must get identically, plus MCP policy that is a governance guard (owner placed Q5 in the facade; Q18 and AC-008 follow the same rule).
  - Adapter: MCP presentation only. Nothing in it is reachable from the CLI.
  - Protocol layer: JSON-RPC and MCP lifecycle only. Nothing in it knows about AIDLC.
  - `main.rs`: CLI presentation only, as today.

## 6. Runtime view
- Main scenario:

**R1. Launch and handshake.** The host spawns `dtech-aidlc-mcp [--project <path>]`. The binary resolves the root (flag or cwd), runs the same root checks as the CLI, writes the `DTECH_AIDLC_HOME` warning to stderr if that variable is set but unusable (ADR-016), and then reads stdin. Nothing is written to stdout or disk before the first request. Modern path: Claude Code 2.1.332 sends `server/discover` (string id) with modern `_meta`, gets the DiscoverResult, then sends `tools/list` (id 0) with `_meta` in the same process (Observed with the dual-era spike, `<spike>/decision-facilitator/cc-dual/frames.log`). The TS client in `auto` mode, which DeepSeek Harness uses, runs a separate probe process (discover, DiscoverResult; the client then terminates it: the probe pid never logged the spike's EOF marker, so a server-side EOF and exit 0 there is an Assumption) and then a serving process that sends 2026-07-28 `_meta` on every request (Observed with client 2.4.0, `ts-dual-auto.log`; 2.2.0 equivalence is an Assumption). Legacy path: a legacy-only client sends `initialize` (for example 2025-11-25 with id 0), `notifications/initialized` and `tools/list`, and gets legacy shapes (the fallback sequence Observed from both hosts against the legacy-only spike mode, `cc-legacy/frames.log`, `ts-legacy-auto.log`). Every path reaches the same tool adapter.

**R1a. Era detection, per request** (ADR-004, ADR-005 rule 4). The session holds only `initialized`. "Key" means `params._meta["io.modelcontextprotocol/protocolVersion"]`.

| Request | Era | Response |
|---|---|---|
| `initialize` (any `_meta`) | legacy | Negotiated version, `capabilities:{tools:{}}`, `serverInfo`; sets `initialized` |
| Key present, string `"2026-07-28"`, `clientCapabilities` an object | modern | `server/discover`, `tools/list`, `tools/call` served and stamped (`resultType`, `_meta` serverInfo; `ttlMs`/`cacheScope` on discover and list); `ping` and unknown methods -32601 |
| Key present, string not in `["2026-07-28"]` | modern | -32022, `data: {supported:["2026-07-28"], requested}` |
| Key present, version not a string, or `clientCapabilities` absent or not an object | modern | -32602 |
| Key absent, after `initialize` | legacy | `ping` `{}`; `tools/*` with legacy shapes (no modern fields); anything else, `server/discover` included, -32601 |
| Key absent, before `initialize` | none yet | `ping` `{}`; anything else -32602 ("send initialize ... or include _meta ...") |
| Notification (no id) | either | No reply; not validated |

Exact modern result shapes and required fields: ADR-004 rules 6-8.


**R2. `status` / `queue` (read-only).** Adapter validates arguments (`intent_id` pattern for status; no fields for queue) → `application::open(root, Mcp)` → `status::status` or the read-only queue preview (unmigrated state shown as `migration: pending`, nothing written; AC-007) → redaction (status `intentDir` becomes `.aidlc/intents/<id>`) → `isError: false`. No state, audit or engine write (AC-006, AC-007).

**R3. `next` (Caller::Mcp).**
1. Adapter accepts only `claim` (boolean) and `unit` (string); both together are refused with the CLI's wording; any other field is `invalid_arguments` (AC-005, AC-012).
2. `application::open(root, Mcp)`.
3. `application::next` resets intent text, scope, depth, mode, `approved_by` and `detail` to none and `force_resume` to false (ADR-014), runs `knowledge_preflight` (Mcp only), `validate_model_config`, then `orchestrate_next` with `skip_prerequisite_checks = true`.
4. No active intent → "No active intent. Start one with: dtech-aidlc orchestrate next --intent ..." with nothing written (orchestrate.rs:1189-1208) → `operation_failed` (AC-008 "no active intent").
5. **Q5 guard path** at each prerequisite site, through `check_prerequisites(..., skip = true)`, which spawns nothing and builds internal rows `{name, ok:false, hint, exitCode:null, executed:false}` (undeclared names: `ok:true, undeclared:true, executed:false`; no `check` command text). Where the not-executed names reach MCP output differs per site (ADR-011):
   - site 1470 (claim loop): the unit is not claimed; the names are returned only in the directive's `warnings` text ("... and its check is not run over MCP; it is not claimed."), and the rows are not returned;
   - site 1544 (held-unit routing): the next ready unit is used and the names are returned only in `warnings` text; the rows feed only the audit, which is suppressed under skip; with no alternative unit the flow falls through to site 1694;
   - site 1694 (stage preflight plus blocked needs): returns `Blocked` with the rows in `needs` (`executed:false`) and the MCP hint "...prerequisite checks are not run over MCP (not executed). Ask the owner to run 'dtech-aidlc orchestrate next' in a terminal...". No `prerequisite.checked` audit row is written under skip (ADR-011).
6. Otherwise `orchestrate_next` returns `Directive`, `Complete`, `Blocked` (stalled with parked or waiting units) or `Busy`; `finish_directive` runs on a directive as on the CLI.
7. Adapter wraps `{outcome: "directive"|"complete"|"blocked"|"busy", result: <CLI payload>}`, redacts paths (methodology files become `.aidlc/engine/methodology/...` or `$DTECH_AIDLC_HOME/...`), returns `isError: false` (ADR-007, ADR-010). On the modern path the dispatcher then stamps `resultType` and `_meta` serverInfo (R1a).

**R4. `report` (Caller::Mcp), including the approval paths.**
1. Adapter validates `stage`, `result`, `detail`, `approval`, `approved_by`, `approval_note`, `needs[]`, `unit` against patterns and caps; `root`, evidence and artifact fields are refused as unknown (AC-005, AC-009, AC-012).
2. `application::open(root, Mcp)` (root, safety check, engine, graph; no state read, no write).
3. **Q18 guard**, the first statement of `application::report`: `approval` present (approved or rejected) and `approved_by` absent, empty or whitespace-only → `APPROVER_REQUIRED` → `approver_required`. No state read, no write, no `resolve_approver`, so no `git` or OS lookup (ADR-013; AC-010).
4. `load_config_unvalidated` → `orchestrate_report_with_engine` → artifact validation, `needs` known-name check, then for a gate decision `resolve_approver` on the flag branch (`bySource: "flag"`, identity.rs `resolve_approver_with`, Observed) and the CLI gate policy unchanged (Q6):
   - AI-mode: only "AI Orchestrator", plus the gate-recommendation checks (`ai_approval_problem`, orchestrate.rs:2750-2769, called at :3288);
   - human-mode: "AI Orchestrator" refused; `allowedApprovers` applies (orchestrate.rs:3296-3307); a named label is recorded as given;
   - this is the live policy code (orchestrate.rs:3273-3307); `gate.rs::validate_gate_approval` is a test-only copy (ADR-013), and parity tests target orchestrate.rs;
   - policy is re-read on every call (AC-010 "policy edited between calls").
5. `ensure_unchanged` → temp-then-rename state write → audit append → `finish_report`. Rows equal the CLI's for the same inputs (AC-002, AC-009).
6. Mcp only: `pending_prerequisites` re-reads state read-only, predicts the next selection on a discarded clone and returns `pendingPrerequisites` rows (declared names, `executed:false`; empty array when none or intent complete). Nothing is executed or written (ADR-012).
7. Redact, return `isError: false`; the dispatcher stamps modern results (R1a).

**R5. CLI path (Caller::Cli).** `main.rs` parses and validates as today, builds `Project` from values it already holds, calls the same facade function with skip off, no Q18 guard and no `pendingPrerequisites`, then prints and exits exactly as before (exit 3 on `Blocked`). The spike kept the full suite at 454 passed / 2 failed with an identical failure set (Observed, `<spike>/facade-researcher/{before2,after2}.fail`).

- Failure scenario:

| Trigger | Path | Result | AC |
|---|---|---|---|
| Unknown tool, `tools/call` without string `name`, non-object `arguments` | Dispatcher/adapter | JSON-RPC -32602 | AC-005 |
| Unknown field (`root`), wrong type, bad enum, over cap | Adapter, before `open` | `isError` result, `invalid_arguments`, nothing read or written | AC-005, AC-012 |
| Gate decision without a label | Facade Q18 guard | `approver_required`, nothing read or written, no `git` spawn | AC-010 |
| State changed between load and persist (CLI wrote meanwhile) | `ensure_unchanged` returns `STATE_CHANGED` | `state_conflict`, `retryable: true`, never retried by the server; no partial state, no extra audit row | AC-011, AC-012 |
| Symlinked `.aidlc`, unreadable state, any other domain or I/O error | Facade | `operation_failed`, redacted message bounded to about 1,000 characters (Estimate) | AC-011 |
| A request other than `ping` without the modern key before `initialize` | Dispatcher | -32602 "Send initialize ... or include _meta ..." | AC-003 |
| Modern request naming an unsupported version (for example `2025-11-25` or `9999-01-01` in `_meta`) | Dispatcher, before dispatch | -32022, `data: {supported:["2026-07-28"], requested}`; nothing read or written | AC-003 |
| Modern request with a non-string version, or `clientCapabilities` absent or not an object | Dispatcher, before dispatch | -32602; nothing read or written | AC-003, AC-005 |
| Modern `ping`; `server/discover` without the modern key after `initialize` | Dispatcher | -32601 | AC-003 |
| Oversized line / invalid UTF-8 / parse error / batch / null id | Frame reader, dispatcher | -32600 or -32700 with id null; the session continues | AC-004, AC-005 |
| Handler panic | `catch_unwind` | -32603 "Internal error", panic text on stderr only; session continues | AC-011, AC-004 |
| Invalid `--project` at launch | Startup | Message on stderr, non-zero exit, no stdout bytes, no files created | AC-004 |
| Stdin EOF (the observed TS probe process was terminated by the client, not ended by EOF; safe either way because nothing is written at startup or in discover, `initialize` or list, TH-20) | Frame reader | Exit 0 | AC-003 |

## 7. Deployment view
- Environments:
  - Developer machine, macOS (Observed environment; rustc 1.100.0). Linux is expected to behave the same (Assumption; no CI). Windows: `.exe` launch, CRLF and path forms specified but untested (A5, R17).
  - No server, network or cloud environment: the server is a local child of the host.
- Runtime nodes:

| Node | What runs | Configuration |
|---|---|---|
| Install location (`~/.cargo/bin` by default) | `dtech-aidlc` and `dtech-aidlc-mcp`, both from `cargo install --path . --locked` (offline `--locked` works for both; Observed, packaging spike C1). `--bin dtech-aidlc` still installs the CLI only; `cargo uninstall dtech-aidlc` removes both (Observed, C3). Install scripts stay CLI-only (N2) | `Cargo.toml`: second `[[bin]]`, `rust-version = "1.100"` |
| Claude Code host process | Spawns one `dtech-aidlc-mcp` per configured server; takes the modern path in the same process against a dual-era server, and falls back to `initialize` in the same process against a legacy-only one (both Observed with the spike) | Proposed docs: `claude mcp add --transport stdio aidlc -- dtech-aidlc-mcp` or a project `.mcp.json`; the project root is `--project` or the launch cwd. Which cwd Claude Code passes is an Assumption, so docs show `--project` as the explicit form |
| DeepSeek Harness host process | Spawns a short-lived probe process (`server/discover`), then the serving process (Observed in the TS client 2.4.0 frames in both eras; DSH not run) | Proposed docs: `cordis.patch.yml` stdio entry with `command`, `args`, `cwd: !!js process.cwd()`; `serverName` must match `[A-Za-z0-9_-]{1,32}`; default `toolCallTimeoutMs` is 60,000 (Observed, `<spike>/host-compat/dsh/README.md:57-67`). DSH passes a scrubbed parent env (Observed, `transport.ts`); that `DTECH_AIDLC_HOME` survives the scrub is an Assumption |
| `dtech-aidlc-mcp` process | One synchronous loop, one call in flight, no threads, no listener, no child processes; exits 0 on EOF | Root fixed at launch; engine resolved per call |
| Project directory | `<project>/.aidlc/` shared with any CLI process on the same machine | No lock; optimistic compare-before-write (ADR-015) |

- Host configuration is documented, never written by install (AC-014). Docs location: README section `## MCP server (proposed)` ([ADR-017](decision-record.md#adr-017-host-configuration-docs-location)). No version bump or CHANGELOG edit in intent 1 ([ADR-018](decision-record.md#adr-018-no-version-bump-and-no-changelog-edit-in-intent-1)).

## 8. Cross-cutting concepts
- Security: see [threat-model.md](threat-model.md). Design rules that every component follows: the root comes only from launch; all inputs are validated before the project is opened; MCP spawns nothing; MCP output is redacted; the approver label is self-asserted and documented as such.
- Data:
  - **No new data store and no schema change.** State and audit rows written over MCP are equal to the CLI's for the same inputs; `bySource` stays `"flag"` (ADR-013). Under skip, no `prerequisite.checked` row is written (ADR-011).
  - **Payload pass-through.** The facade returns the same `serde_json::Value` the CLI prints (key order preserved by `preserve_order`). MCP adds only the `{outcome, result}` wrapper on `next` and `pendingPrerequisites` on `report`. On the modern path the dispatcher adds envelope fields outside `structuredContent` (`resultType`, `_meta` serverInfo, and `ttlMs`/`cacheScope` on discover and list); the payload itself is identical in both eras.
  - **Path redaction (Q8).** One recursive pass in the adapter over every string and key of success and error values, longest prefix first, boundary-checked: launch root (lexical and canonical) → project-relative with `/`; `DTECH_AIDLC_HOME` (raw and resolved) → `$DTECH_AIDLC_HOME`; `HOME` → `~`; anything still absolute → `<redacted-path>` in values, or the generic text for its code in error messages. The canonical forms matter because the materialised engine path is canonical while the root is lexical (`/private/var` versus `/var` on macOS; orchestrate.rs:151, 381). Docs say paths are relative to the launch root (ADR-010).
  - **Stdout purity.** One writer, compact JSON, one line per message; diagnostics and panic output on stderr; the library has no reachable print on the four paths; stdin is owned by the frame reader; no child process inherits it because none is spawned (ADR-005, ADR-011).
  - **Errors.** Two layers. JSON-RPC errors (-32700, -32600, -32601, -32602, -32603, and -32022 for an unsupported modern version with `data: {supported, requested}`) for protocol faults only; the per-code table is ADR-005 rule 7. Error responses never carry `resultType`. Everything else is a tool result with `isError: true` and `structuredContent = {error: {code, message, retryable}}`, `code` in `invalid_arguments | approver_required | state_conflict | operation_failed`, classified by exact equality against shared constants, never by substring. Blocked, complete and busy are results, not errors. No automatic retries anywhere (ADR-007, ADR-008).
  - **Concurrency.** Per-call open, so no stale cache. Optimistic `ensure_unchanged` before each write; a conflict surfaces once as `state_conflict` (retryable, not retried); no operation ID and no lock in v1. The compare-then-rename window and interleaved audit appends from two writers are documented residuals (ADR-015; R6).
- Observability: stderr only (startup failures, the `DTECH_AIDLC_HOME` warning, panic messages). No telemetry, log file or `--telemetry` flag in intent 1 (Q11); intent 2 adds operation telemetry at the facade. Call provenance relies on the host's own tool-call log.
- Configuration: keys, flags and environment variables (Develop registers each as `CFG-` in ids.json; none is minted here):

| Item | Kind | Read by | Notes |
|---|---|---|---|
| `--project <path>` | MCP flag | `dtech-aidlc-mcp` | Optional; defaults to the launch cwd; fixed for the process lifetime (AC-012) |
| `--help`, `--version` | MCP flags | `dtech-aidlc-mcp` | AC-013 |
| `DTECH_AIDLC_HOME` | Env var (existing) | `application::external_engine()` for both binaries | Unusable value: CLI silent fallback unchanged; MCP adds a stderr warning (ADR-016; F3) |
| `HOME` | Env var | MCP adapter (redaction only) | Not used for behaviour |
| `PATH` | Env var | Not used by MCP (no spawns) | CLI unchanged |
| `rust-version = "1.100"` | Cargo.toml key | Cargo | ADR-003 |
| `[[bin]] name = "dtech-aidlc-mcp"` | Cargo.toml table | Cargo | ADR-002 |
| `NextOptions.skip_prerequisite_checks` | Internal option (not user-facing) | `orchestrate_next` | Set only by the facade for `Caller::Mcp` |
| `LEGACY`, `MODERN`, cache-hint constants | Compile-time constants | Dispatcher | Values: ADR-004 (Decision, rule 10); a new revision is added only after its schema is checked |
| Frame cap (Estimate 1 MiB), field caps, error-message bound | Compile-time constants | Frame reader, adapter | Numbers fixed in implementation-design |
| `.aidlc/config.json` `allowedApprovers`, gate modes, prerequisites | Project config (existing) | Facade via the existing loaders, per call | MCP exposes no config tool |
| Host config (`.mcp.json`, `cordis.patch.yml`) | Host files | Hosts | Documented, never written by us (AC-014) |

## 9. Architecture decisions
- ADR-001 Hand-rolled JSON-RPC over stdio instead of an MCP SDK — accepted (the owner-decided choice only; the conformance plan is proposed) — see [decision-record.md](decision-record.md#adr-001-hand-rolled-json-rpc-over-stdio-instead-of-an-mcp-sdk)
- ADR-002 Packaging: second `[[bin]]` in the same package, zero new crates — accepted (the owner-decided packaging only; the flag set is proposed) — see [decision-record.md](decision-record.md#adr-002-packaging-second-bin-in-the-same-package-zero-new-crates)
- ADR-003 MSRV pinned at 1.100 — accepted (owner-decided) — see [decision-record.md](decision-record.md#adr-003-msrv-pinned-at-1100)
- ADR-004 Protocol era: dual-era for v1 — accepted (the owner-decided dual-era scope only; the per-request rules are proposed) — see [decision-record.md](decision-record.md#adr-004-protocol-era-dual-era-for-v1)
- ADR-005 Framing, lifecycle, per-request era detection and process behaviour — proposed — see [decision-record.md](decision-record.md#adr-005-framing-lifecycle-per-request-era-detection-and-process-behaviour)
- ADR-006 Facade shape — proposed (the thin-facade principle within it is owner-decided) — see [decision-record.md](decision-record.md#adr-006-facade-shape)
- ADR-007 Outcome representation and exit code 3 (Q7) — proposed — see [decision-record.md](decision-record.md#adr-007-outcome-representation-and-exit-code-3-q7)
- ADR-008 Error model and tool result shape — proposed — see [decision-record.md](decision-record.md#adr-008-error-model-and-tool-result-shape)
- ADR-009 Tool names, input schemas and annotations — proposed — see [decision-record.md](decision-record.md#adr-009-tool-names-input-schemas-and-annotations)
- ADR-010 Path form in MCP output (Q8) — proposed — see [decision-record.md](decision-record.md#adr-010-path-form-in-mcp-output-q8)
- ADR-011 Q5 prerequisite guard — proposed (the Q5 principle within it is owner-decided) — see [decision-record.md](decision-record.md#adr-011-q5-prerequisite-guard)
- ADR-012 Pending-prerequisite list on MCP report (AC-009) — proposed — see [decision-record.md](decision-record.md#adr-012-pending-prerequisite-list-on-mcp-report-ac-009)
- ADR-013 Approver handling (Q6, Q18) — proposed (the Q6 and Q18 principles within it are owner-decided) — see [decision-record.md](decision-record.md#adr-013-approver-handling-q6-q18)
- ADR-014 MCP next only continues an existing active intent (AC-008) — proposed — see [decision-record.md](decision-record.md#adr-014-mcp-next-only-continues-an-existing-active-intent-ac-008)
- ADR-015 Concurrency without an operation ID (Q10) — proposed — see [decision-record.md](decision-record.md#adr-015-concurrency-without-an-operation-id-q10)
- ADR-016 Engine resolution sharing (Q15) — proposed — see [decision-record.md](decision-record.md#adr-016-engine-resolution-sharing-q15)
- ADR-017 Host-configuration docs location — proposed — see [decision-record.md](decision-record.md#adr-017-host-configuration-docs-location)
- ADR-018 No version bump and no CHANGELOG edit in intent 1 — proposed — see [decision-record.md](decision-record.md#adr-018-no-version-bump-and-no-changelog-edit-in-intent-1)

Statuses use the template set proposed | accepted | superseded. "Accepted" is used only where the whole decision is an owner choice recorded as User-provided on 2026-11-27, and it covers only that owner-decided part; the gate confirms it for this direction. A decision whose details are recommendations is "proposed", with the owner-decided principle named in brackets. "Proposed" means a decision-facilitator recommendation not yet approved. No decision record is superseded: ADR-004 keeps its id, its earlier legacy-only recommendation was replaced in place by the owner's dual-era decision, and that rationale is kept inside ADR-004.

## 10. Quality requirements
- See [quality-requirements.md](../../analyze/context-and-risk/quality-requirements.md). Each scenario below names its quality category, criteria and the check that measures it. A check that was not run is not a pass.

| # | Category | Scenario | Measure | AC / US |
|---|---|---|---|---|
| Q-1 | Operability | Routing status, queue, next and report through the facade | Same pass/fail set before and after (`cargo test --no-fail-fast`, TMPDIR under `/private/tmp`, `target/debug` on PATH), no golden re-bless; `Blocked` still exits 3. Spike: 454/2 before and after (Observed) | AC-001, AC-002; US-001, US-002 |
| Q-2 | Operability | CLI and MCP on equivalent projects, same inputs | Equal payloads (after redaction) and byte-equal state and audit rows; engine parity with `DTECH_AIDLC_HOME` set | AC-002, AC-009 |
| Q-3 | Availability | Host spawns the server, handshakes, lists four tools, shuts down, restarts, in either era | Transcript replays of real Claude Code modern frames and of the TS modern probe and serving processes, plus a legacy handshake transcript, all pass; manual `claude mcp list` "Connected" (Observed with the dual-era spike on the modern path, and with the legacy-only mode on the fallback path) and a DeepSeek Harness launch-and-list (not run; required before close), each recording the negotiated era; project tree byte-identical after discover or initialize, list and EOF | AC-003; US-003 |
| Q-4 | Availability | One call fails (schema, domain, oversized frame, bad UTF-8, panic) mid-session | The next valid request in the same session gets a normal reply | AC-004, AC-011 |
| Q-5 | Privacy | Every success and error value of all four tools, project under `/var/folders`, external engine set, forced I/O error | Zero strings containing the root (lexical or canonical), `HOME` or the engine home; zero strings starting with `/`, a drive letter or `\\` | AC-006, AC-008, AC-011; US-005 |
| Q-6 | Privacy / Security | A session mixing valid calls, schema errors and domain errors | Every stdout line parses as JSON-RPC 2.0; no listening socket; no `panicked at` or canary env value in output | AC-004, AC-011 |
| Q-7 | Security | MCP next and report on fixtures reaching sites 1470, 1544, 1694 and a report into a stage with a prerequisite, with a PATH shim for `sh`, `git`, `cmd` and a `touch <marker>` prerequisite | No marker file; no `prerequisite.checked` row; CLI goldens unchanged. Sites 1470 and 1544: each not-executed name appears in the directive's `warnings` with "its check is not run over MCP" (the AC-008 channel there; rows are not returned). Site 1694: `needs` rows `executed:false`. Report: `pendingPrerequisites` rows `executed:false` | AC-008, AC-009; US-006, US-007 |
| Q-8 | Security | Report with `approval` approved or rejected and the label absent, null, `""` or whitespace | `approver_required`; no `git` marker; `state.json` and `audit.jsonl` byte-identical | AC-010; US-008 |
| Q-9 | Security | AI-mode approved, AI-mode without recommendation, human-mode with the AI label, human-mode named label, policy edited between calls | Same accept/refuse and the same state and audit as the CLI | AC-010; US-008 |
| Q-10 | Availability | State changed between load and persist | Exactly one `state_conflict` (`retryable: true`), no partial state, no extra audit row, no automatic retry | AC-011, AC-012 |
| Q-11 | Operability | Install from a clean `CARGO_HOME`, offline, `--locked`, both binaries and CLI-only | Install, `--help`, `--version`, uninstall from outside the checkout; Cargo.lock no diff; host settings bytes unchanged | AC-013, AC-014; US-009 |
| Q-12 | Performance | A status, queue, next or report call through MCP | Interactive, and well under DSH's default 60 s tool-call timeout; no call waits on owner shell commands (none run). A numeric per-call bound is an Open question (N6), set in implementation-design from measured CLI latency | quality-requirements Performance |
| Q-13 | Accessibility | Not applicable: no user interface in intent 1 (quality-requirements Accessibility). Error text stays plain and actionable | Review of error messages | AC-011 |
| Q-14 | Interoperability (Availability) | Modern-era conformance: discover, list, call, unsupported version, missing `clientCapabilities`, era mixing in one process (ADR-001 cases M1-M6) | Every modern success result equals its expected frame (exact shapes: ADR-004 rules 6-8); legacy results carry no modern field; -32022 carries the data of ADR-004 rule 5; error responses carry no `resultType`; the same `tools/call` gives equal `structuredContent` in both eras | AC-003, AC-004 |
| Q-15 | Security | Hostile modern `_meta`: numeric or object version, a 100 KiB version string, `clientCapabilities` null or an array, unknown `_meta` keys, a spoofed `clientInfo` | -32602 or -32022 as specified; the session continues; nothing read or written; behaviour identical whatever `clientInfo` says | AC-004, AC-005 |

## 11. Risks and technical debt

| Risk / debt | Design response | Residual | Source |
|---|---|---|---|
| R1 facade changes CLI bytes | Payload pass-through, CLI keeps parsing/printing/exit codes, one command at a time, goldens after each step | Low (spike evidence) | risk-register.md |
| R3 / R4 prerequisites run, or a child inherits MCP stdin | Skip at all three sites; zero spawns over MCP | Low; CLI keeps inheriting stdin (N1 follow-up) | ADR-011 |
| R5 self-asserted approver | CLI gate policy, Q18 guard, documentation, host confirmation | High for a spoofed name on a human gate, owner-accepted (Q6) | ADR-013; threat-model.md TH-01 |
| R6 lost update with a concurrent CLI | Optimistic check, retryable conflict, documented window | Low likelihood, High impact; AC-012 met only up to the window, the same as two CLI runs today (F9) | ADR-015 |
| R9 path leaks | Recursive adapter redaction with canonical forms and a last-resort check | Low-Medium (free-text Windows forms) | ADR-010 |
| R11 stdout pollution | Single writer, no library prints, `catch_unwind` | Low | ADR-005 |
| R13 engine divergence | One shared `external_engine()`; six other library copies left as they are | Low for the four operations; drift in the other copies is existing debt | ADR-016 |
| R15 unbounded input | Frame cap, field caps, `additionalProperties: false`, validation before open | Low (caps are Estimates) | ADR-005, ADR-009 |
| R17 no CI, Windows untested | Platform claims limited to local tests | Medium | recommended-direction.md section 4 |
| R18 host compatibility | Both eras observed: Claude Code 2.1.332 and TS client 2.4.0 took the modern path with the dual-era spike and fell back to legacy with the legacy-only mode; exact-frame transcripts for both eras; manual checks in both hosts recording the negotiated era | Medium until DSH is run and a model-driven `tools/call` is observed in Claude Code. The hand-rolled modern path is now the default path for both hosts; TS client 2.4.0 falls back to legacy only on a malformed DiscoverResult or a non--32022 error on the discover probe (Observed); no fallback is shown for a malformed modern list or call result, so M6 (exact-frame shape sweep, ADR-001) is the mitigation for those; Claude Code's reaction to a malformed modern response is untested (Open question) | ADR-004 |
| Hand-tracked spec and conformance burden | Three revisions across two eras; about 6 modern transcript cases (ADR-001 M1-M6) on top of the legacy ones; exact-frame equality reviewed once against the saved schemas; a new revision added only after its schema is checked; upgrade triggers | Medium (raised from Low-Medium by the owner's dual-era decision, which accepted the larger claim); we own drift tracking | ADR-001, ADR-004 |
| Fail-closed Q5 ceiling (preflight prerequisites, or a blocked report naming `needs`) | Documented; owner runs CLI `next` for such stages | Usability limit, owner awareness (F6) | ADR-011 |
| `pendingPrerequisites` is a preview | Shared `needed` helper; description says preview | May differ from the next pick on claim, pinned or parallel branches | ADR-012 |
| Silent CLI fallback on unusable `DTECH_AIDLC_HOME` | MCP stderr warning, same selection | Existing CLI debt (F3) | ADR-016 |
| Duplicate gate-approval policy: the live report path uses orchestrate.rs (`ai_approval_problem` at :2750-2769, allowlist at :3296-3307); `gate.rs::validate_gate_approval` (via `orchestration_policy` / `orchestration_report_validation`) is a second copy that only tests reach (Observed, grep 2026-11-27) | Not touched; parity tests target the orchestrate.rs path | Existing debt: the copies can drift | ADR-013 |
| Stranded logic in main.rs, very large orchestrate.rs and work_units.rs | Facade extracts only the four operations | Existing debt | codebase map section 9 |

## 12. Glossary
- **Facade (`application`)**: the new library module that is the single behaviour path for status, queue, next and report.
- **Caller**: `Cli` or `Mcp`; switches the MCP-only governance guards in the facade.
- **Tool adapter**: MCP-only code that validates arguments, calls the facade and shapes results (redaction, outcome wrapper, error codes).
- **Protocol layer**: the JSON-RPC and MCP lifecycle loop behind `(method, params) -> Result<Value, Value>`.
- **Legacy era / modern era**: MCP revisions with `initialize` (2025-11-25, 2025-06-18) versus the stateless 2026-07-28 model (`server/discover`, per-request `_meta`). v1 serves both, chosen per request.
- **A-legacy**: Option A with a legacy-only protocol; this stage's earlier recommendation, superseded by the owner's dual-era decision.
- **Era detection**: the per-request rule that a request is modern when its `_meta` carries `io.modelcontextprotocol/protocolVersion` (ADR-005 rule 4).
- **Result stamping**: adding `resultType`, `_meta` serverInfo and, where required, `ttlMs`/`cacheScope` to modern results in the dispatcher.
- **Skip row**: a prerequisite result produced without running the check, marked `executed:false`.
- **Launch root**: the project root fixed when the server starts, from `--project` or the cwd.
- **Probe process**: the short-lived server process a TS-client host starts to detect the protocol era before the serving process.
- Domain terms (intent, stage, gate, work unit, directive, audit) are defined in [PROJECT.md](../../../../../PROJECT.md) "Domain glossary".
