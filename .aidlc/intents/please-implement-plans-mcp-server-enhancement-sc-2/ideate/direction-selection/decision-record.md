# Decision record: telemetry, monitor and release (intent 2 of 2)

- Purpose: record every decision behind the selected direction once, so implementation-design and later agents can follow it without the design discussion. architecture-design.md section 9 links here and does not restate these decisions.
- Status: Draft - pending solution-direction gate.
- Stage: Ideate / direction-selection (decision-facilitator). Companions: [recommended-direction.md](recommended-direction.md), [scope-boundary.md](scope-boundary.md), plus architecture-design.md and threat-model.md in this folder.
- Numbering: continues the intent-1 record `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/decision-record.md` (ADR-001..ADR-018). This record holds ADR-019..ADR-039. A decision that changes an intent-1 ADR names it and says amended or superseded. These numbers are not contract registry ids. No FN-/IF-/TS-/WU-/CFG- id is minted here; implementation-design does that.
- Sources: [solution-options.md](../option-generation/solution-options.md) (cited as "options s1.1" and so on), [trade-off-matrix.md](../option-generation/trade-off-matrix.md); Analyze baseline `../../analyze/problem-framing/*` and `../../analyze/context-and-risk/*` (AC-001..AC-030, US-001..US-013, Q1-Q25, CON1-CON18, ASM1-ASM9, R1-R26; row labels are local, not citable ids); `plans/mcp-server-enhancement/*` (plan = observability implementation plan, design = detailed design); intent-1 knowledge record `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md`.
- `<spike2>` means `/private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/spike2/`. Every spike ran on macOS arm64 with rustc 1.100.0 only. Code line numbers are Observed in the working tree at commit 3336980 unless stated otherwise.
- Provenance labels: Observed, User-provided, Assumption, Estimate, Open question.

## How to read the status line

- **Owner-decided (User-provided, project owner, 2026-11-28).** The owner chose this through AskUserQuestion in the conductor session: the direction (Option A) and the Analyze decisions Q1-Q5, Q10-Q13, Q15 and the DeepSeek deferral. Gate approval confirms them for this direction.
- **Recommended - pending gate approval.** A decision-facilitator recommendation backed by the cited evidence. It is not approved until the solution-direction gate passes and has no owner approval of its own. This intent runs in AI orchestration mode, so the gate may be approved by "AI Orchestrator" under the AI orchestrator approval contract; that approval is not an owner approval.
- Where an owner decision fixes the principle and this stage sets the details, the status line says so.

| ID | Title | Status |
|---|---|---|
| ADR-019 | Direction: Option A, and the owner decisions it carries | Owner-decided |
| ADR-020 | Telemetry storage, file format, rotation and retention | Validated ids owner-decided (Q13); location from the Analyze baseline (AC-008, plan); values and details Recommended |
| ADR-021 | Background writer thread, failure isolation and dropped events | Owner-decided through Option A; details Recommended |
| ADR-022 | Operation hook in the MCP adapter; tests pass `--telemetry off` | Owner-decided through Option A; details Recommended. Amends ADR-006 |
| ADR-023 | Opt-out flag `--telemetry on\|off` | Owner-decided principle (Q1); flag shape Recommended. Amends ADR-002 |
| ADR-024 | Event vocabulary and schema v1, including `workflow.gate_decided` | Recommended |
| ADR-025 | Outcome and error mapping, and the protocol-error hook | Recommended. Amends ADR-005 (observer; rule 5) |
| ADR-026 | Ids: sessionId from the OS random source | Recommended (includes the trace and parent-span rules) |
| ADR-027 | HTTP server: std only, zero crates | Owner-decided principle (Q3); structure Recommended. Extends ADR-001 |
| ADR-028 | Monitor process model, lifecycle and edge cases | Owner-decided through Option A; details Recommended |
| ADR-029 | Monitor token: source, delivery, page storage, comparison | Owner-decided through Option A; details Recommended |
| ADR-030 | Query and cursor API, read caps and refresh values | Owner-decided through Option A; values Recommended |
| ADR-031 | Dashboard assets embedded as three static files | Recommended |
| ADR-032 | One HTTP status and error-code table | Recommended |
| ADR-033 | Version 1.3.0, scoped re-bless and CHANGELOG | Owner-decided (Q2, Q4, Q5); review method Recommended. Fulfils ADR-018; amends ADR-002 (lock check) |
| ADR-034 | README update and the "proposed" label | Owner-decided (Q10); unit split Recommended. Amends ADR-017 |
| ADR-035 | Installers install both binaries | Owner-decided principle (Q11); details Recommended. Amends ADR-002 |
| ADR-036 | Flaky-test fixes and the missing intent-1 MCP tests | Owner-decided scope (Q12); fixes Recommended |
| ADR-037 | `deps add` takes the state lock | Owner-decided scope (Q12); approach Recommended |
| ADR-038 | Studio HTML mock update and title | Owner-decided update (Q15); title is an owner follow-up |
| ADR-039 | Acceptance-criteria readings applied at the release gate | Recommended |

---

## ADR-019 Direction: Option A, and the owner decisions it carries

- **Context.** solution-options s2 sets out four bundles (A-D) over independent axes; trade-off-matrix rates A strongest on AC coverage with no owner-gated condition. The owner chose on 2026-11-28.
- **Decision.** Option A: a bounded background telemetry writer; operation and tool spans emitted from the MCP adapter; test spawns pass `--telemetry off`; an in-process foreground `dtech-aidlc monitor` on a std-only loopback HTTP server; server-side query with an opaque cursor; a per-run 128-bit token from the OS random source, delivered in the URL fragment. It carries every Analyze decision (options OD1-OD10): MCP-only telemetry, default ON with an opt-out (Q1); zero new crates (Q3); scoped re-bless for the version, `monitor` in help/completion/validation and one managed .gitignore line (Q2, Q4); 1.3.0 prepared, no tag (Q5); README "proposed" removed only after Windows+macOS CI green and a live Claude Code `tools/call`, DeepSeek staying "not verified" (Q10); installers install both binaries (Q11); flaky tests, missing intent-1 MCP tests and the `deps add` lock in scope (Q12); validated ids may be stored (Q13); `mcp-studio-server.html` updated and the dashboard design AI-reviewed (Q15); DeepSeek verification deferred (US-012).
- **Status.** Owner-decided (User-provided, project owner, 2026-11-28).
- **Intent-1 ADRs changed.** ADR-002 amended in three places: installers no longer CLI-only (ADR-035), the MCP flag set becomes `--project` plus `--telemetry` (ADR-023), and its "Cargo.lock no-diff check" (`LOCK_SHA256`, `tests/mcp_packaging.rs:6`) takes a new value for the 1.3.0 package line (ADR-033). ADR-005 amended in two places: an observer that leaves reply bytes unchanged, and rule 5 ("nothing is written to stdout or disk ... in the `initialize`, `server/discover` and list handlers") relaxed for the telemetry write that follows an error reply (ADR-025). ADR-006 amended: its consequence "the facade is also the hook for intent-2 operation telemetry" no longer holds (ADR-022). ADR-017 amended: the `## MCP server (proposed)` heading and its "every statement is labelled proposed" rule end once the Q10 evidence exists, and telemetry, monitor and troubleshooting sections join it; its reopen trigger "intent 2 restructures the README" has fired (ADR-034). ADR-001 extended to HTTP (ADR-027). ADR-018 fulfilled, not superseded (ADR-033). ADR-003, ADR-004, ADR-007..ADR-016 stand unchanged; ADR-009's `readOnlyHint: true` on `status` and `queue` is kept on purpose (ADR-022).
- **Plan text superseded or amended** (plan text, not ADRs): plan:305, :409, :456, :500 and design s7.7 bullet 1 (supervisor with a hidden `current_exe` child) are superseded by ADR-028. Plan:278, :309 and design:312 ("no token in URL query/hash") are amended for the token only by ADR-029. Design s6.3's producer for operation events (the facade) is amended by ADR-022. Plan:119 `--help`/`--version` for the MCP binary was already superseded by the intent-1 Develop contract (`.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/develop/implementation-design/interfaces-and-contracts.md:151`, Observed); recorded here for consistency only. Plan s3.7 "config override" and design s8 "optional bounded time window" for the monitor are not built (zero monitor flags). The schema enumeration values `cli`, `workflow.unit_parked` and `workflow.unit_revisited` are removed from v1 (ADR-024).
- **Alternatives.** B (plan-literal: synchronous sink in the facade, supervisor child, typed token), C (no token, browser-side query), D (static HTML snapshot). Why each lost is in [recommended-direction.md](recommended-direction.md) section 2.
- **Reopen if.** The owner changes direction at the solution-direction gate.

## ADR-020 Telemetry storage, file format, rotation and retention

- **Context.** AC-008, AC-009, AC-012; Q6 safe default; R1, R2, R7, R8.
- **Decision.**
  - Location `.aidlc/telemetry/`. One file per MCP process, named `mcp-<UTC yyyymmddThhmmssZ>-<pid>-<first 8 hex of sessionId>-<part, 3 digits>.jsonl`. The writer and the monitor share one strict name pattern.
  - JSONL, one event per line, UTF-8, `\n` terminated. Timestamps RFC 3339 UTC with milliseconds, reusing the formatting in `rust-cli/audit.rs:20-58` (Observed).
  - Never creates `.aidlc`. The writer writes only when `.aidlc` exists, passes `assert_safe_state_dir`, and `.aidlc/telemetry` is absent or a real directory (lstat). Creation is lazy, at the first event. If `.aidlc` is absent, events are dropped silently (keeps `tests/mcp_stdio.rs:548-549` true, Observed by research). An unsafe location gives one fixed stderr line with no path, and telemetry is disabled for the process (AC-008 symlink scenario, AC-010).
  - Unix: directory 0o700, files `create_new` with 0o600 (pattern from `rust-cli/audit.rs:172-195`). `create_new` refuses an existing path or final-component symlink; on AlreadyExists the writer retries with the next part number (AC-012 "Name collision"). Windows: inherited ACLs, documented in the README and the AC-020 disclosure (Assumption).
  - Git: one `/telemetry/` line added to IGNORE_BLOCK (`rust-cli/knowledge.rs:22`), marker stays `v1` (CON3). On directory creation the writer also writes `.aidlc/telemetry/.gitignore` containing `*`, as ENGINE_GITIGNORE does (`rust-cli/orchestrate.rs:96-98`), because `ensure_ignore_file` runs only from `finish_directive` (R2, Observed).
  - Retention (Estimates, confirmed for v1): 5 MiB per file, then a new part file (settles AC-009 scenario 2 as "new file"); 7 days by mtime; 50 MiB total. Pruning runs only in a telemetry-on MCP process, at the writer's first event (its lazy start) and at each rollover: expired files first, then oldest first until under the total. It touches only pattern-matching regular files (symlink_metadata), never the current file, and ignores every delete error. Known ceiling (Estimate): between prunes the total can exceed 50 MiB by up to one open file per concurrent process.
- **Status.** "Validated ids only" is owner-decided (Q13). The location `.aidlc/telemetry/` is not an owner decision: it comes from the Analyze baseline (AC-008 and the plan). Values and mechanics Recommended - pending gate approval.
- **Alternatives.** OS per-user state directory, one shared rolling file, files beside `audit.jsonl`: rejected (options s4: AC-008, AC-012).
- **Evidence.** `<spike2>/review/tspike` re-run (Observed): rotation peaked at 5,242,878 bytes under a 5,242,880 cap; pruning kept an unrelated file.
- **Consequences.** Two lifecycle goldens change for the IGNORE_BLOCK line (ADR-033). Windows delete of a file another live process holds is an Assumption (delete-pending via FILE_SHARE_DELETE); errors are ignored either way; verified on windows-latest CI (R7).
- **Reopen if.** Real sessions exceed the caps often, or Windows CI shows pruning failing in a way that grows the store without bound.

## ADR-021 Background writer thread, failure isolation and dropped events

- **Context.** AC-004, AC-010, AC-018, AC-020, AC-028 (scenarios 2-4); R3. A std file write has no timeout, so only a writer off the request thread meets AC-028 scenario 4.
- **Decision.**
  - One writer thread per MCP process, fed by `std::sync::mpsc::sync_channel(1024)` (Estimate; about 600 KiB worst case, about 170 calls of headroom at about 6 events per call). The producer only calls `try_send` and never touches the disk. The AC-028 scenario 4 bound is therefore "the reply never waits on telemetry I/O".
  - The producer stamps timestamp and `durationMs` itself. Building and sending an event is wrapped in `catch_unwind`; a panic there disables telemetry for the process and never changes the reply (AC-028 scenario 3).
  - Disconnected channel (writer panicked or hit an I/O error): telemetry disabled for the process, one fixed stderr line with no path.
  - At stdin EOF: drop the sender, wait at most 500 ms (Estimate) for the writer, then exit.
  - **Drops: disclosure only, no AC-002 change.** A full queue drops the event. The first drop in a process prints one fixed stderr line, `dtech-aidlc-mcp: telemetry events dropped (writer queue full)`, with no path; later drops print nothing. stderr is allowed under AC-028. The AC-020 privacy help says "Events can be dropped when the disk is slow; drops are not recorded in the store", and the dashboard coverage line shows "Dropped events: not recorded". A dropped completion whose start was written shows as incomplete, never as success (AC-004 "Process dies mid-call").
  - The stall test injects a slow writer through the sink's writer parameter in a unit test (no runtime switch, no env var): with a 10 s stall, the producer returns in under 1 s.
- **Status.** The background writer is owner-decided through Option A; queue size, flush wait and drop handling Recommended - pending gate approval.
- **Alternatives.** (a) A counted drop field in the next event: rejected, because AC-002 fixes the exact field set (Observed) and no listed field can carry a count without changing its meaning; it needs an Analyze-owned AC-002 amendment. (b) Synchronous writer (axis T1): not chosen by the owner; it makes AC-028 scenario 4 Partial. (c) Queue 256 (the spike value): drops sooner for a 40 KiB memory saving.
- **Evidence.** `<spike2>/review/tspike` re-run (Observed): with the writer stalled at 50 ms per event and a 256 queue, 1000 `try_send` calls took 0.39 ms on the producer (257 sent, 743 dropped); a simulated writer panic surfaced as Disconnected at event 4 with the main thread alive.
- **Consequences.** AC-004 and AC-018 are Met* (met while the queue does not fill). With 1024 slots and human-paced calls, a drop needs a disk stall of minutes (Estimate). About 40 more lines than a synchronous sink.
- **Reopen if.** Drops are observed in real use, or Analyze or the owner amends AC-002 to add a drop counter.

## ADR-022 Operation hook in the MCP adapter; tests pass `--telemetry off`

- **Context.** Q22 and CON4: the facade `rust-cli/application.rs:72-123` serves both callers, and the CLI writes nothing (Q1). Q24: default-ON telemetry would change MCP snapshot assertions (R1, ASM5). `ToolHost` has only `list` and `call` (`rust-cli/mcp/protocol.rs:13-17`, Observed).
- **Decision.**
  - A `traced_call` wrapper in `rust-cli/mcp/tools.rs` becomes `ToolHost.call`. Per call it emits `mcp.request.started`, then `operation.started`/`operation.completed` (component `application`, caller `mcp`) around `application::open` plus the operation, then the workflow events after Ok (ADR-024), then `mcp.tool.completed`. It has its own `catch_unwind`; on a panic it emits the completion as error/`internal_error` and calls `resume_unwind`, so protocol.rs keeps its -32603 behaviour.
  - `application.rs` and the CLI path are not touched. No sink state enters `Project`.
  - Every existing snapshot-comparing spawn passes `--telemetry off`: `run_mcp_env` (`tests/fixtures/mcp/common.rs:159`), `tests/mcp_stdio.rs:240`, `tests/mcp_parity.rs:395` (about 3 test-source lines). The snapshot helper (`common.rs:244`) is not changed, so Q24 needs no owner approval. Each existing snapshot then also proves "off writes nothing" (AC-001). A new telemetry test file runs with telemetry on. `tests/rust_cargo_install.rs:166` sends only `initialize` and `tools/list`, so it writes no events and needs no flag (Observed by review).
  - **ADR-009 annotations stand.** `status` and `queue` keep `readOnlyHint: true` (`rust-cli/mcp/tools.rs:133`, Observed) although each call now writes telemetry by default. The hint describes what the tool does to the project the host is working on (workflow state, audit, artifacts); `status` and `queue` still never change any of it. The telemetry line is the server's own diagnostic, written the same way for every tool, switched off by `--telemetry off`, and never read by workflow code (AC-007, AC-016). Setting the hint to false would make hosts that honour it ask for confirmation before read-only views with no safety gain, and would change the `tools/list` bytes that CON10 and the intent-1 transcripts pin. Reopen if a host or the AC-030 review treats the telemetry write as an environment change.
- **Status.** Owner-decided through Option A; wrapper details Recommended - pending gate approval.
- **Amends ADR-006.** ADR-006's consequence "The facade is also the hook for intent-2 operation telemetry" no longer holds: the hook is the MCP adapter. ADR-006's reopen trigger ("intent 2 telemetry needs per-process state") does not fire, because no per-process state enters the facade. Design s6.3's producer column for `operation.*` is amended the same way.
- **Alternatives.** Facade hook (Option B): puts a per-process sink into `Project` and reopens ADR-006. Snapshot skip of `.aidlc/telemetry`: needs owner written approval (Q24) and stops existing snapshots from guarding "off writes nothing".
- **Consequences.** CLI goldens cannot change from telemetry (AC-027). AC-006 is Met* under CON18: caller is always `mcp`, and the coverage label says the CLI writes no events.
- **Reopen if.** The CLI is later asked to emit events (a Q1 change).

## ADR-023 Opt-out flag `--telemetry on|off`

- **Context.** Q1 (default ON with an opt-out flag), Q20, AC-001. `rust-cli/mcp/mod.rs:13` accepts only `--project`, and its unit test `parse_args_accepts_only_project` (`:86`) pins that.
- **Decision.** `dtech-aidlc-mcp [--project <path>] [--telemetry on|off]`, default `on`. `parse_args` becomes an order-independent loop returning project and telemetry, and refuses a duplicate flag, a missing value or any other value (exit 1 through the existing usage path). No environment variable (YAGNI; the decision says "flag"). With `off`, the sink is never constructed and no file is created or changed (AC-001 scenario 1). Test-source edits: the `parse_args` unit test and the refusal loop at `tests/mcp_stdio.rs:196`.
- **Status.** Default and the flag principle owner-decided (Q1); spelling and parsing rules Recommended - pending gate approval.
- **Amends ADR-002.** ADR-002's flag set (`--project`, `--help`, `--version`; "no `--telemetry` flag in intent 1") becomes `--project` and `--telemetry`. The `--help`/`--version` text was already replaced by the intent-1 Develop contract (`interfaces-and-contracts.md:151`, "no `--version` or `--help`"; `tests/mcp_stdio.rs:196-206`, Observed); consistency note only.
- **Alternatives.** An env var as well: no use case beyond the flag; adds a second input to document and test.
- **Consequences.** No trusted "configured off" input reaches the monitor (the setting lives in the host's launch arguments), so the dashboard shows "Setting: unknown, configured in your MCP host launch arguments" (ADR-039 reading 1).
- **Reopen if.** A host cannot pass arguments to a stdio server.

## ADR-024 Event vocabulary and schema v1, including `workflow.gate_decided`

- **Context.** AC-002, AC-003, AC-007, Q8, Q13. Design s6.2 is normative; plan:202 says "`name` carries the tool/operation identifier" (Observed). The approval argument is the closed set approved|rejected (`rust-cli/mcp/tools.rs:89`, Observed); AC-002 has no decision field (Observed).
- **Decision.**
  - `schemaVersion: 1`. Fields exactly as AC-002, built by an explicit field-by-field constructor, never a generic `Value`.
  - Event kinds produced: `mcp.request.started`, `mcp.tool.completed`, `mcp.protocol.error`, `operation.started`, `operation.completed`, `workflow.stage_selected`, `workflow.stage_reported`, `workflow.gate_decided`. Removed from the v1 enumeration because nothing produces them (AC-003 forbids documenting kinds never produced): `workflow.unit_parked`, `workflow.unit_revisited`, intent pause/resume/abandon. `component` is `mcp` or `application`; `caller` is always `mcp` (CON18). The `cli` value is not in the v1 enumeration.
  - `name`: for `mcp.*` tool events the shipped tool name (`status`, `queue`, `next`, `report`), or the fixed value `unknown` for an unknown tool (the raw name is never stored); for `mcp.protocol.error` the fixed value `protocol`; for `operation.*` the operation name; for `workflow.stage_selected` `next`; for `workflow.stage_reported` and `workflow.gate_decided` `report`.
  - `intentId`, `stageId`, `unitId` are stored only if they pass the `tools.rs` validators and 64-character caps; otherwise they are omitted (Q13, R5). Never stored: `detail`, `approved_by`, approval note, messages, paths, argv, env, raw frames.
  - Workflow events after durability (AC-007): `stage_selected` after `application::next` returns a directive; `stage_reported` after `application::report` returns Ok; `gate_decided` after report returns Ok for a call that carried `approval` (state and audit already written). A failed or conflicting report emits only the error operation event.
  - **`workflow.gate_decided` carries no approve/reject value in v1.** Outcome is `ok` (the call succeeded), errorCode null. The audit log and state remain the record of the decision; telemetry is not audit (ADR-013, AC-030). The dashboard shows "gate decided for <stage>".
- **Status.** Recommended - pending gate approval.
- **Alternatives (gate value).** `name: approved|rejected`: fits AC-002 syntactically but stretches `name` against plan:202 and design s6.2. Outcome ok/blocked for approved/rejected: overloads call outcome and distorts the AC-018 blocked ratio. Separate kinds `gate_approved`/`gate_rejected`: adds names to the design s6.2 enumeration.
- **Consequences.** The v1 schema documentation lists the per-event meaning of `name`. The dashboard coverage line says park, revisit and intent pause/resume/abandon are not observed (AC-006 "Coverage gap shown").
- **Reopen if.** The AI dashboard design review or the owner needs approve/reject counts; that needs an AC-002 amendment adding an enumerated `decision` field.

## ADR-025 Outcome and error mapping, and the protocol-error hook

- **Context.** AC-004, AC-005, Q9, R6. An oversized line and a malformed envelope both reply -32600 (`protocol.rs:203`, `:256`, Observed), so a code-only hook cannot give AC-005's two scenarios distinct codes. `serve` is the only stdout writer and has the `Frame` variant at the reply site (`protocol.rs:243-277`, Observed).
- **Decision.**
  - Tool completion: directive and complete -> `ok`; blocked and busy -> `blocked` (ADR-007); status/queue Ok -> `ok`; `isError` -> `error` with errorCode from the existing four (`invalid_arguments`, `approver_required`, `state_conflict`, `operation_failed`; `tools.rs:203`); unknown tool -> `error`/`unknown_tool`; caught panic -> `error`/`internal_error`. No typed-error work (ADR-008 stands).
  - Hook: `ToolHost<C>` gains `on_error: fn(&C, &'static str)`. `serve` calls it once per error reply, after writing that reply. Codes: `Frame::Oversized` -> `line_too_long`; `Frame::BadUtf8` and JSON parse failure -> `parse_error`; any other error reply by its code: -32600 `invalid_request`, -32601 `method_not_found`, -32602 `invalid_params`, -32022 `unsupported_protocol_version`, -32603 `internal_error`, any other `protocol_error`. Notifications get no reply and so no event. The `&'static str` type means no frame text, message or argument can reach telemetry (AC-005 "Content excluded"). The adapter's implementation only calls `try_send`. Two construction sites: `rust-cli/mcp/mod.rs` and the protocol test HOST (`protocol.rs:294`), which uses a no-op.
  - AC-004 reading confirmed: a `tools/call` that `serve` rejects before `host.call` (no string `name`, or non-object `arguments`; `protocol.rs:107-115`) yields only `mcp.protocol.error`/`invalid_params`, no start/completion pair. Span emission is not moved into `serve`. An unknown tool yields one tool span (error/`unknown_tool`) and one `invalid_params` protocol event; they are separate dashboard populations.
- **Status.** Recommended - pending gate approval.
- **Amends ADR-005 in two places.**
  - (1) Adds an observer to `serve`. Reply bytes are unchanged, so the intent-1 transcripts and ADR-005's code table stand.
  - (2) Rule 5 is amended for telemetry after error replies. Rule 5 says "Nothing is written to stdout or disk at startup, or in the `initialize`, `server/discover` and list handlers". With telemetry on and `.aidlc` present, an error reply from those paths now sends one `mcp.protocol.error` event, and the writer may then create `.aidlc/telemetry`, write a file and prune (ADR-020). Examples: `initialize` with no string `protocolVersion` (-32602); a modern `tools/list` or `server/discover` with a bad `_meta` (-32602 or -32022); a keyless pre-`initialize` request (-32602). Still true: nothing is written at startup (the writer starts lazily at the first event); a successful `initialize`, `server/discover`, `tools/list` or `ping` writes nothing; the stdout half of rule 5 is unchanged; with `--telemetry off` rule 5 holds as written.
- **Alternatives.** Suppress `on_error` until the session's first `tools/call`, which would keep rule 5 as written: rejected, because it hides the protocol errors of a host that never gets as far as `tools/call` (for example a wrong protocol version), which is the case the AC-005 events exist to show. `fn(&C, i64)` (options s1.1): cannot split `line_too_long` from `invalid_request`. Wrapping the stdout writer to parse replies: more code, fragile. Emitting spans from `serve`: puts tool semantics in the protocol layer.
- **Consequences.** About 15 lines plus a table test. Keeps the SDK-swap seam (R6, ADR-001).
- **Reopen if.** The dashboard needs -32602 split by reason, or an SDK replaces `serve` (ADR-001 reopened).

## ADR-026 Ids: sessionId from the OS random source

- **Context.** AC-002, AC-006 ("the MCP call links to its tool span by trace"), AC-012, AC-019 (trace filter and "parent navigation"). Options s1.1 proposed RandomState/SipHash with no cross-process collision test (Assumption). Design s6.3 (design:214) asks to propagate one `traceId` and parent span through an MCP call into the operation; the design sample shows `"parentSpanId": null` for a root (design:183, Observed).
- **Decision.**
  - `sessionId` = 16 bytes from `os_random` (ADR-029), as 32 lowercase hex. Every new `eventId`, `traceId` and `spanId` is `<sessionId>-<n>`, with `n` from one per-process `AtomicU64` counter, so no two ids in a process repeat and the sessionId makes them unique across processes. If the OS random source fails at MCP start, telemetry is disabled for that process with one fixed stderr line; the server keeps working (AC-010). Ids are dedup keys, not credentials (design s6.3).
  - **One trace per `tools/call`.** `traced_call` mints one `traceId` when the call reaches it. Every event of that call carries it: `mcp.request.started`, `operation.started`/`operation.completed`, any `workflow.*` event and `mcp.tool.completed`.
  - **Tool span.** `mcp.request.started` and `mcp.tool.completed` share one `spanId` (the tool span). It is the root: `parentSpanId` is `null`. Component `mcp`, caller `mcp`.
  - **Operation span.** `operation.started` and `operation.completed` share a second `spanId`, and their `parentSpanId` is the tool span's id. Component `application`, caller `mcp`.
  - **Workflow events.** Each `workflow.*` event is a point event with its own new `spanId`; `parentSpanId` is the operation span's id. Component `application` (the facade made the change; design sample at design:256), caller `mcp`.
  - **Protocol errors.** Each `mcp.protocol.error` is its own one-event trace: new `traceId` and `spanId`, `parentSpanId` `null`, component `mcp`, caller `mcp`, name `protocol`. It is not linked to a tool span, because `on_error` carries only a static code (ADR-025); an unknown tool's tool span and its `invalid_params` protocol event are therefore separate traces.
  - **`parentSpanId` is always present.** It is written as `null` for a root, never left out. The only fields AC-002 lets an event omit are `caller` (v1 always knows it: `mcp`) and the optional `intentId`, `stageId`, `unitId` (ADR-024).
  - **Monitor use.** The `trace` filter is an exact match on `traceId` (ADR-030). Event detail offers parent navigation: it links to the event or span pair in the same trace whose `spanId` equals the selected event's `parentSpanId`; for a root it shows "no parent". A `parentSpanId` with no matching span in the store (dropped, expired, or past a read cap) is shown as "parent not in store".
- **Status.** Recommended - pending gate approval. Replaces the RandomState proposal in options s1.1 (a proposal, not an ADR).
- **Alternatives.** RandomState/SipHash: no cryptographic or cross-process guarantee from std (ASM2), and a second generator to own.
- **Evidence.** The telemetry spike's 400,000 unique ids measured a different generator (Observed, re-run); not relied on.
- **Reopen if.** Windows CI shows the random call failing or slow at MCP start.

## ADR-027 HTTP server: std only, zero crates

- **Context.** Q3 (zero crates; HTTP hand-rolled with std), ASM1, R9, R12, AC-013..AC-016.
- **Decision.** `std::net::TcpListener` bound to `127.0.0.1:0` only (never `0.0.0.0`). One thread per connection, at most 16 in flight; more get `503 busy` written immediately. A guard with `Drop` releases the slot even if a handler panics; a handler panic answers `500 internal_error`. The whole request head must arrive within 5 s total, recomputed before each read (`408`); head cap 8 KiB (`431`). Write timeout 5 s. One request per connection, `Connection: close`. The request body is never read; for a refused method the server half-closes and drains to a bound so the reply is delivered. Exact route table on the raw target, no percent-decoding, no filesystem mapping. The server writes nothing to stderr per request. Status codes and headers: ADR-032.
- **Status.** Zero crates owner-decided (Q3); structure Recommended - pending gate approval.
- **Extends ADR-001.** ADR-001's hand-rolled, zero-crate principle now covers the loopback HTTP server. ADR-001 is not changed.
- **Alternatives.** tiny_http, hyper or any crate: Infeasible (Q3). Async or non-blocking I/O: no need at a 16-connection local cap.
- **Evidence.** `<spike2>/http/results.txt` (Observed, macOS): 408 at 5 s under slowloris; 431 for 20 KB and 1 MiB heads; 405 with Allow; 421/403 for foreign Host/Origin; 503 under 40 idle sockets, then recovery; 200 for 200 concurrent valid requests; connection over the LAN IP refused. `shutdown-results.txt` case D: a POST with a 2 MiB body still gets its 405 (half-close plus drain).
- **Consequences.** About 250 lines of server plus table tests (Estimate). Windows: a read timeout surfaces as `TimedOut`, not `WouldBlock`, so match both (Assumption); verified on windows-latest.
- **Reopen if.** The owner approves a crate, or Windows CI shows the drain or timeout behaviour failing.

## ADR-028 Monitor process model, lifecycle and edge cases

- **Context.** AC-013, AC-016, AC-017, AC-026, Q19, ASM3, R13. A registered hidden flag breaks the help invariant (`tests/repo_content.rs:1353-1372`, Observed).
- **Decision.**
  - `dtech-aidlc monitor` runs the server in-process in the foreground. Global `--project` only, no command flags, no positional arguments. It dispatches after `assert_safe_state_dir`. It prints one line on stderr, `monitor: http://127.0.0.1:<port>/#token=<32 hex>  (Ctrl+C to stop)`, writes nothing to stdout, opens no browser and does not daemonize.
  - Exit: 1 for a start failure (edge case 2). No path exits 0: once serving, the monitor runs until a signal ends it. Stop: Ctrl+C with default signal handling; on Unix the process dies of SIGINT, which the shell reports as status 130 (SIGTERM: 143); there is no exit call. AC-013 tests stop it with `Child::kill` or SIGTERM, never a scripted SIGINT, because a background job of a non-interactive script inherits SIGINT as ignored. The README tells wrapper authors to forward SIGTERM.
  - Edge cases. (1) `.aidlc` or `.aidlc/telemetry` absent: start normally and show "No telemetry observed"; never create directories. (2) Either is a symlink or non-directory, or unreadable, at start; bind failure; random-source failure: exit 1 with a fixed message and no path (the existing `assert_safe_state_dir` message for `.aidlc`). (3) The same condition arising while running: each query re-checks with lstat and answers `503 store_unavailable`; the page keeps last-known data marked stale. A directory created after start is picked up, because every query lists it. (4) Only pattern-matching regular files are read (symlink_metadata). Symlinks, non-pattern files (such as `.gitignore`) and subdirectories are ignored, never followed. A matching file that cannot be opened counts as a skipped file. (5) A file deleted between listing and open (pruned by a live writer): skipped silently. (6) A growing file: read up to its length at open; an unterminated last line counts as "partial tail", not corrupt; parse cache keyed by (len, mtime). (7) Pattern files older than 7 days: counted, not shown, never deleted; coverage line "N files older than retention not shown (removed the next time a telemetry-on MCP server writes)". Pruning happens only at a writer's first event or rollover (ADR-020), so with telemetry off they stay until a telemetry-on run. (8) Malformed lines, lines over 16 KiB, unknown fields and unknown kinds: skipped and counted (AC-003). (9) Several monitors on one project: each has its own port and token. (10) Shutdown: the page detects a network error and shows "Monitor stopped or unreachable" (the AC-017 "shutting down" state; design s7.5 "stop refresh").
  - Windows `.cmd` launcher: the CLI keeps `bin\dtech-aidlc.cmd` (ADR-035). The README troubleshooting section says: "On Windows, Ctrl+C on `dtech-aidlc monitor` may print 'Terminate batch job (Y/N)?'. The monitor has already stopped; answer either." The exe is the last line of the batch file, so either answer ends it (Assumption: the default console handler ends the Rust process). The MCP binary is a real exe, so hosts never see the prompt.
- **Status.** In-process monitor owner-decided through Option A; edge-case behaviour Recommended - pending gate approval.
- **Supersedes plan text** plan:305, :409, :456, :500 and design s7.7 bullet 1 (foreground supervisor with a hidden `current_exe` child).
- **Alternatives.** Supervisor plus env-var child (Option B): orphans the child on SIGKILL until stdin EOF propagates (R13), adds an undocumented entry point, needs cleanup tests on three OSes. A `--exit-on-stdin-eof` flag (shutdown spike case C): no supervisor needs it (YAGNI). A SIGINT handler: needs FFI or a crate. Nonzero exit when telemetry is absent: a project that never ran the MCP server is a normal state.
- **Evidence.** `<spike2>/http/shutdown-results.txt` (Observed): interactive SIGINT gives exit 130 and no process left; a background job of a non-interactive script survives SIGINT and needs SIGTERM (exit 143). `install.ps1:75` writes the three-line `.cmd` wrapper (Observed).
- **Consequences.** AC-026 "launch the monitor from its own location" is read as "the installed `bin/dtech-aidlc monitor` runs the server in-process" (ADR-039). Windows loopback bind is assumed to raise no firewall prompt (ASM6); needs one manual desktop check.
- **Reopen if.** The AI design review wants a nonzero exit for an absent store, a supervisor needs a stdin-EOF stop, or Windows users report the batch prompt as confusing.

## ADR-029 Monitor token: source, delivery, page storage, comparison

- **Context.** Q7, AC-015, AC-019, AC-030, ASM2. The loopback port is reachable by every local account; a token adds protection against other OS users and against local processes that cannot read the terminal or the telemetry files. A same-user process can read `.aidlc/telemetry` directly, so the token does not protect against it (accepted; threat-model.md).
- **Decision.**
  - **Source.** One library helper `os_random(&mut [u8]) -> io::Result<()>`. Unix (macOS, Linux): `File::open("/dev/urandom")` then `read_exact`, no unsafe. Windows: `BCryptGenRandom(NULL, buf, len, BCRYPT_USE_SYSTEM_PREFERRED_RNG = 0x2)` through a plain `#[link(name = "bcrypt")] extern "system"` block, the same form as the kernel32 precedent at `rust-cli/dependencies.rs:338-361`; about 8 lines of unsafe with a SAFETY comment; a negative NTSTATUS is an error. The spike (`<spike2>/decision-facilitator/win.rs`) used `kind = "raw-dylib"` instead; either needs no crate. Plain is chosen to match the precedent; `raw-dylib` is the fallback if windows-latest cannot find the bcrypt import library. Token = 16 bytes as 32 lowercase hex. Fail closed: the monitor exits 1; no fallback generator.
  - **Delivery.** Only in the stderr startup line (ADR-028), in the URL fragment `#token=`. Never in argv, env, files, telemetry, logs or MCP output.
  - **Page storage.** `app.js` reads `location.hash` once, keeps the token in a module-scoped `const` inside a closure, and calls `history.replaceState(null, '', '/')` before the first fetch. Never sessionStorage, localStorage, cookies, IndexedDB, the DOM or a window global (design s7.4's ban on persistent storage). Sent as `Authorization: Bearer <token>` on `/api/*` only. `/`, `/app.js` and `/app.css` carry no data and need no token. On reload, or with no token, the page shows "Token missing: reopen the address printed in the terminal" and makes no API call. AC-019 "Reload" (filters back to defaults) is therefore met only because a reload drops the token and the page shows no data at all; this is recorded as ADR-039 reading 8 for the AI design review.
  - **Comparison.** Lengths first (the length is public, always 32), then an XOR fold over every byte with the accumulator passed through `std::hint::black_box`; accept only when the fold is 0. The presented value is the exact bytes after `Bearer `, with no trimming or case folding. Missing, malformed and wrong tokens all get the same `401 unauthorized`.
- **Status.** Fragment token owner-decided through Option A; source, storage and comparison Recommended - pending gate approval.
- **Amends plan text** plan:278, :309 and design:312 for the token only: the fragment is never sent in the request line or in Referer, and is stripped on load. Project path and filter state still never go in the address (AC-019, AC-020).
- **Alternatives.** `std::random`: unstable on rustc 1.100 (E0658, issue #130703; Observed, `<spike2>/decide/r.rs`). RandomState plus sha2 (the Q7 safe default): std gives no cryptographic guarantee (ASM2). ProcessPrng: newer API, no import library in older SDKs. RtlGenRandom: legacy alias. getrandom or subtle crates: Infeasible (Q3). sessionStorage (HTTP spike): breaks design s7.4. Typed token (Option B) and no token (Option C): not chosen by the owner. Plain `==`: can short-circuit; sha256 digest compare with the existing sha2 crate is the fallback if a reviewer distrusts `black_box`.
- **Evidence.** `<spike2>/decide/token.rs` and `<spike2>/decision-facilitator/tok.rs` (Observed, macOS): 100,000 tokens, all unique, 32 lowercase hex each; the fold compare took 97.0/97.5 ms for a first-byte mismatch and 97.0/97.6 ms for a last-byte mismatch over 5M compares (two runs, -O), and the equal, short and one-bit-different unit asserts passed. Raw Win32 FFI precedent without crates: `rust-cli/dependencies.rs:338-361` and `rust-cli/knowledge.rs:118-160` (Observed). The Windows extern block is not compiled or type-checked on macOS at all, because a `cfg(windows)` item is skipped; it has never been built or run (Open question until windows-latest CI).
- **Consequences.** First link against bcrypt. Residuals for the AC-030 review: the token sits in terminal scrollback; browser global history may keep the original fragment URL before `replaceState` (Assumption, not tested in a browser); the token lives as long as the process.
- **Reopen if.** windows-latest CI fails to link or run the call, `std::random` stabilises within the MSRV, the owner approves a crate, or the AI threat review rates the history residual high.

## ADR-030 Query and cursor API, read caps and refresh values

- **Context.** AC-016..AC-019, Q14, design s7.4-s7.7, plan:288 and design:348 (server cursor). The values below are Estimates confirmed for v1; they make the AC-016 range, AC-017 stale, AC-018 sample-size and AC-028 stall scenarios testable.
- **Decision.**
  - One data route, `GET /api/events`, parameters: `range` (seconds, integer 1..604800; the page offers presets 900, 3600, 86400, 604800 = "all retained"), `component`, `name`, `outcome`, `stage`, `session`, `trace`, `limit` (1..200, default 50), `cursor`. Every parameter is optional; a missing `range` means 3600 (the last hour, the page's first preset), and a missing filter means no filter on that field. Each value is checked against its enumeration or the id character set and length. Unknown, duplicate or malformed parameters, any `%`, and any path-like key (`path`, `file`) give `invalid_filter`; a range above 604800 gives `range_too_large` (ADR-032).
  - Response: the page of events (allowlisted fields re-serialised, never raw lines), newest first, tie-break timestamp then eventId; a summary over the whole filtered range (tool-call count = distinct `mcp.tool.completed` eventIds; blocked and error ratios with numerator and denominator; incomplete count; separate operation counts; p50/p95 by nearest rank only at n >= 20 completed tool calls with n shown, else "insufficient samples"); coverage counters (files read, skipped lines, partial tails, unreadable files, expired files); `nextCursor`; and `limitReached: "query_limit_reached"` when a read cap was hit. Field names are fixed in implementation-design.
  - Cursor: opaque hex of (frozen `to` in ms, a 16-hex sha256 prefix of the normalised query using the existing sha2 crate, last timestamp, last eventId). Not a secret. Any mismatch or decode failure gives `cursor_stale`. Events newer than the frozen `to` appear on a fresh query.
  - Read caps: 512 files, 64 MiB, 250,000 events per query (64 MiB sits above 50 MiB plus one 5 MiB overshoot). Reaching one returns 200 with partial newest-first data and the limit flag.
  - Page refresh: every 5 s; on an HTTP error the interval doubles up to 60 s; paused while `document.hidden`; on a network error the page stops auto-polling and offers manual retry (ADR-028 (10); also stops a stale tab sending its token to a process that later takes the same port). Stale: the last refresh failed or the last success is older than 3 intervals (15 s). Quiet: no event in 15 min (not a liveness claim). Liveness is always "unknown".
- **Status.** Server-side query and cursor owner-decided through Option A; parameters and values Recommended - pending gate approval.
- **Alternatives.** Browser-side filtering of a bounded bundle (Option C): not chosen; AC-019 stale cursor becomes client-only. Percentiles at n >= 100 for p95: more honest, but empty for most sessions. Presets only (no integer range): leaves `range_too_large` unreachable and AC-016 untestable.
- **Consequences.** The per-file parse cache can hold memory up to about the read caps (Estimate about 75 MB at 250k events; not measured).
- **Reopen if.** Real sessions hit the caps, the 5 s refresh flickers stale, or the AI design review asks for a separate p95 threshold or a different paging contract.

## ADR-031 Dashboard assets embedded as three static files

- **Context.** AC-011, AC-016, design s7.7 "static UI is bundled/embedded", R21. `embedded::ASSETS` is read by `scope.rs` as methodology (options s1.2, Observed by research).
- **Decision.** Three files, `index.html`, `app.js`, `app.css`, embedded with `include_str!` from a new monitor module (the `rust-cli/main.rs:48,93` pattern), not in `embedded::ASSETS`; no build.rs change. Routes `/`, `/app.js`, `/app.css`. Because script and style are external, the CSP needs no `unsafe-inline` for either. Data enters the DOM only through `textContent`; no `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `eval`, `new Function` or inline handlers (a repo-content test checks `app.js`). System font stacks; no remote asset of any kind.
- **Status.** Recommended - pending gate approval.
- **Alternatives.** Two files with `style-src 'unsafe-inline'`: one file fewer, but a weaker CSP for no gain. One HTML file with an inline script and a hash CSP: needs a hand-rolled base64 encoder. Assets in `embedded::ASSETS`: mixes UI with methodology.
- **Reopen if.** The AI dashboard review needs a fourth asset (for example an icon).

## ADR-032 One HTTP status and error-code table

- **Context.** Design s7.7 names `invalid_filter`, `range_too_large`, `cursor_stale`, `store_unavailable`, `query_limit_reached` and no status values. The two spikes disagree (the HTTP spike accepts `Host: localhost`, uses `token_required`, `path_rejected`, `invalid_limit`, `unknown_parameter`, `body_not_allowed`; the monitor spike uses `forbidden_origin`), Observed. AC-014 requires a method error for write methods; R10 asks for Sec-Fetch-Site.
- **Decision.** Every error body is `{"error":"<code>"}` (application/json), never echoing input or a path. Checks run in this order:

  | Order | Condition | Status | Code |
  |---|---|---|---|
  | 0 | 16 connections already in flight (at accept, nothing read) | 503 | `busy` |
  | 1 | Head not complete within 5 s | 408 | `request_timeout` |
  | 2 | Head over 8 KiB | 431 | `header_too_large` |
  | 3 | Malformed request line, non-ASCII or control bytes, target not starting with `/`, version other than HTTP/1.0 or 1.1 | 400 | `bad_request` |
  | 4 | Any method other than GET, including HEAD and OPTIONS; header `Allow: GET` | 405 | `method_not_allowed` |
  | 5 | Duplicate Host, Content-Length other than 0, or any Transfer-Encoding | 400 | `bad_request` |
  | 6 | Host missing or not exactly `127.0.0.1:<port>` (`localhost` refused) | 421 | `host_rejected` |
  | 7 | Origin present and not exactly `http://127.0.0.1:<port>` (including `null`), or Sec-Fetch-Site present and not `same-origin` or `none` | 403 | `origin_rejected` |
  | 8 | Target not exactly `/`, `/app.js`, `/app.css` or `/api/events` (query allowed only on `/api/events`); traversal forms land here | 404 | `not_found` |
  | 9 | `/api/*` with a missing, malformed or wrong bearer token; header `WWW-Authenticate: Bearer` | 401 | `unauthorized` |
  | 10 | Unknown, duplicate or malformed parameter, unknown filter value, `%` in the query, path-like key, limit outside 1..200, non-integer range | 400 | `invalid_filter` |
  | 11 | Range above 604800 s | 400 | `range_too_large` |
  | 12 | Cursor does not match the normalised query or frozen `to`, or cannot be decoded | 409 | `cursor_stale` |
  | 13 | `.aidlc` or `.aidlc/telemetry` unsafe or unreadable at query time | 503 | `store_unavailable` |
  | 14 | Handler panic | 500 | `internal_error` |
  | - | A read cap reached: not an error; 200 with partial data and `limitReached: "query_limit_reached"` | 200 | - |

  Method is checked before the body headers so a POST with a body still gets AC-014's method error. Every response carries `Content-Type`, `Content-Length`, `Connection: close`, `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `Cross-Origin-Resource-Policy: same-origin` and `Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`, and never an `Access-Control-*` header. A cross-origin fetch with `Authorization` must preflight; the preflight gets 405 with no CORS headers, so the browser blocks it.
- **Status.** Recommended - pending gate approval.
- **Alternatives.** Accept `localhost:<port>` (HTTP spike): widens the allowlist with no need, since the printed URL uses 127.0.0.1. Separate `body_not_allowed`, `path_rejected`, `invalid_limit`, `unknown_parameter` codes: more codes, no client behaviour difference. `cursor_stale` as 400: lets a client confuse it with a malformed request; 409 marks it as a state mismatch (the client keys on the code either way). `query_limit_reached` as an error status: would hide the partial data design s7.5 asks to show.
- **Evidence.** `<spike2>/http/results.txt` and `shutdown-results.txt` (Observed, macOS) for 405, 408, 421, 403, 401, 431, 503 and the drain.
- **Consequences.** 14 error codes (rows 3 and 5 share `bad_request`) plus the `limitReached` flag, which is a field in a 200 response, not an error body; each code and the flag get one contract test over a raw `TcpStream`. Favicon requests get 404 (harmless).
- **Reopen if.** The AI dashboard design review needs different client handling, or a browser sends Origin or Sec-Fetch-Site in a form this exact match refuses.

## ADR-033 Version 1.3.0, scoped re-bless and CHANGELOG

- **Context.** Q2, Q4, Q5, CON2, CON3, AC-023, AC-024, AC-027, R20. ADR-018 deferred the version bump and CHANGELOG entries for intent 1 to intent 2.
- **Decision.** Prepare 1.3.0 with no tag and no publish: `Cargo.toml`, the `Cargo.lock` package line, `.github/badges/version.svg`, CHANGELOG `## 1.3.0`, and `LOCK_SHA256` in `tests/mcp_packaging.rs:6`. Re-bless exactly the 7 goldens Observed in the release spike: `cli_golden/version.json`, `utility_golden/help-and-completion.json`, `routing_golden/{harness,lifecycle,lifecycle-embedded,read-only,validation}.json` (plus new validation steps `monitor --no-such-option` and `monitor extra0`). Review each with `diff <(sed 's/1\.2\.2/1.3.0/g' old) new`, which must be empty apart from the `monitor` lines and the `/telemetry/` line. Do the bump last. One `## 1.3.0` entry covers intents 1 and 2: the `dtech-aidlc-mcp` binary, Rust 1.100 minimum, MCP telemetry (default on, `--telemetry off`, location, retention, CLI writes none, drops not recorded), the `monitor` command (loopback, read-only, token), the approved golden changes (version, `monitor` in help/completion/validation, the managed .gitignore line), installers for both binaries, `deps add` lock, the AC-024 statement that the MCP server, telemetry and the monitor are local optional capabilities, "approver names are self-asserted" and "telemetry is not audit". `harness/` files stay untouched (CON14).
- **Status.** Version, scope and re-bless reasons owner-decided (Q2, Q4, Q5); review method and entry contents Recommended - pending gate approval.
- **Fulfils ADR-018.** ADR-018 is scoped to intent 1 and said the entries come with the intent-2 bump; it is not superseded.
- **Amends ADR-002 (lock check).** ADR-002's consequence "Cargo.lock is byte-identical ... Add a Cargo.lock no-diff check" became `LOCK_SHA256` in `tests/mcp_packaging.rs:6`. The 1.3.0 package line changes Cargo.lock once, so the pinned hash takes the new value; the check itself stays and still fails on any dependency change.
- **Evidence.** Release spike (Observed by research, `<spike2>/release-hardening/`): 7 goldens change after `DTECH_AIDLC_BLESS=1`; new LOCK_SHA256 begins e0c3502f, valid only with zero new crates.
- **Reopen if.** A golden outside the list changes (stop and investigate, never re-bless).

## ADR-034 README update and the "proposed" label

- **Context.** Q10, Q21, CON6, AC-025, R25, R26.
- **Decision.** The README gains telemetry schema and retention, the opt-out, monitor usage and security boundary (token, loopback, same-user boundary), coverage limits, troubleshooting (SIGTERM for wrappers, Windows batch prompt, Windows ACLs), and the two AC-030 sentences. `README.md:380` ("installers stay CLI-only") changes in the installer unit. The `## MCP server (proposed)` heading, the `tests/repo_content.rs:1940-1946` needle, the platform sentence (line 191) and the host sentence (line 421) change together in one late unit, only after both CI legs are green and a live Claude Code `tools/call` is observed. Without that evidence they stay, and the release notes say which phases are released. DeepSeek stays "not verified".
- **Status.** Evidence rule owner-decided (Q10); unit split Recommended - pending gate approval. Who supplies the live observation is owner question Q21 (scope-boundary.md section 4).
- **Amends ADR-017.** ADR-017 put host configuration in a README section `## MCP server (proposed)` and said "Every statement is labelled proposed". Once the Q10 evidence exists, that heading and label are dropped; until then they stay as ADR-017 wrote them. The README also gains telemetry, monitor and troubleshooting sections beside it. ADR-017's reopen trigger "intent 2 restructures the README" has fired; its location choice (the README, not a separate document) stands.
- **Reopen if.** The owner accepts different evidence.

## ADR-035 Installers install both binaries

- **Context.** Q11, Q18, CON7, AC-026, R14, R15, ASM7. Observed in `scripts/rust-install/install.ps1`: the `.cmd` launcher at line 75; the post-commit `Remove-Item $old -Recurse` at line 80 runs inside the try block, so a failure there (for example a locked exe) runs the catch path (lines 87-88), which deletes the new install and moves a possibly half-deleted old directory back.
- **Decision.**
  - `install.sh` and `install.ps1` install, upgrade and uninstall both binaries: copy `dtech-aidlc-mcp[.exe]` into `lib/dtech-aidlc/`, marker bumped to `v2` listing its files. Upgrade from a v1 CLI-only prefix adds the MCP binary and keeps unrecognised files. Uninstall removes only recognised files.
  - POSIX: `bin/dtech-aidlc-mcp` exec launcher. The existing stage-then-rename swap (`install.sh:121-125`) is safe while a binary runs, because the old inode survives.
  - Windows: a real `bin\dtech-aidlc-mcp.exe`, because hosts spawn without a shell (ASM7). The CLI keeps its `.cmd` launcher; `DTECH_AIDLC_ASSETS` stays as a harmless no-op (Q18: no reader in `rust-cli`, Observed by grep).
  - Windows upgrade or uninstall while an exe is in use: (1) before any change, a preflight lists processes whose path is under the prefix's `lib\dtech-aidlc\` or is `bin\dtech-aidlc-mcp.exe`; if any, exit 1 with "dtech-aidlc files are in use: stop the AI host and any `dtech-aidlc monitor`, then re-run" and change nothing. (2) The existing rollback stays as the backstop for a rename that still fails. (3) After the commit point, removing the old directory and backups is best-effort, outside the try block; a leftover `.dtech-aidlc-rust.old.*` is recognised by name and removed at the start of the next run. (4) `bin\dtech-aidlc-mcp.exe` sits outside the staged `lib` directory swap, so it follows the launcher's existing backup pattern (`$tmpLauncher` and `$launcherBackup`, `install.ps1:65-66, 78-79`, Observed): the new exe is copied to `bin\.dtech-aidlc-mcp.tmp.<PID>` before the swap; inside the try block the current exe is moved to `bin\.dtech-aidlc-mcp.backup.<PID>` and the temporary file moved into place; the catch path removes a new exe and moves the backup back; after the commit point the backup is removed best-effort and a leftover is recognised by name on the next run.
  - Tests copy the real cargo-built binaries into the fixture and run install, upgrade and uninstall through sh and pwsh inside `cargo test`, smoke-checking `dtech-aidlc --version` and MCP initialize plus `tools/list` (the MCP binary refuses `--version`). A windows-latest test starts the installed MCP exe with stdin held open, runs `-Upgrade`, and asserts exit 1, the message and an unchanged prefix; then closes it and asserts the upgrade succeeds.
- **Status.** Both binaries owner-decided (Q11); launcher, preflight and cleanup Recommended - pending gate approval.
- **Amends ADR-002.** Supersedes ADR-002's sentence "The install scripts ... stay CLI-only, and support for the second binary there is deferred to intent 2"; ADR-002's own reopen trigger ("the owner wants script install support (intent 2)") has fired.
- **Alternatives.** Replace the CLI `.cmd` with a real exe (retires `DTECH_AIDLC_ASSETS`): widens the change and the repo_content pins beyond Q11. Rename a running exe aside: more partial states; renaming a directory that holds a running image is uncertain.
- **Consequences.** Assumptions until windows-latest CI: Windows refuses to delete a running exe, and `Get-Process` exposes the path for the user's own processes. Estimate 4-6 h.
- **Reopen if.** CI contradicts either assumption, or the installer unit finds real exes for both binaries simpler.

## ADR-036 Flaky-test fixes and the missing intent-1 MCP tests

- **Context.** Q12, Q17, Q23, ASM4, R18, R19, CON5.
- **Decision.** Flakes: confirm each root cause by a repeated local loop first (Q23), then add an `AtomicUsize` counter to the temp-dir helpers in `rust-cli/artifacts.rs:371`, `onboarding/projection.rs:374` and `scope.rs:1000`, and widen the prerequisite timeout test to `sleep 5` / `ping -n 6`. Missing intent-1 MCP tests (the AC numbers here are intent-1 ids): AI-mode gate over MCP and policy edited between calls (intent-1 AC-010), unsafe layout at launch and mid-session (AC-011), CLI+MCP interleaving sequential and concurrent (AC-012), install smoke `--version`/`--help` exit 1 and user settings bytes unchanged (AC-013/AC-014), restart/shutdown mid-session (AC-003). Every new snapshot-comparing test passes `--telemetry off`. A real bug found by these tests is fixed in the same unit if small, otherwise reported for the owner.
- **Status.** Scope owner-decided (Q12); fixes and list Recommended - pending gate approval.
- **Evidence.** Release research (Observed by that thread; no saved log): 12 failures in 40 baseline runs across three tests, one of them `validates_work_units_and_registry_json`, not on the Q12 list; 40/40 green after the counter fix. A reviewer saw the `artifacts` flake fail in 2 of 4 runs (Observed by the reviewer).
- **Consequences.** The fourth flaky test is fixed by the same counter: it sits in the same test module (`rust-cli/artifacts.rs:429`, Observed) and uses the `artifacts.rs:371` helper. It is covered as the same root cause, not as new scope.
- **Reopen if.** A loop shows a different root cause.

## ADR-037 `deps add` takes the state lock

- **Context.** Q12, R17. `dependencies.rs:1264-1306` loads and saves state without `with_state_lock` (Observed; no `with_state_lock` call in `rust-cli/dependencies.rs`).
- **Decision.** Wrap load, edit, save and audit of `deps add` in `with_state_lock`, following `intent_commands.rs:304-330`. Regression test: 3 concurrent adds keep 3 edges. The `deps add` golden stays byte-identical.
- **Status.** Scope owner-decided (Q12); approach Recommended - pending gate approval.
- **Evidence.** `<spike2>/release-hardening/deps-lock.diff` (+13 lines; Observed by research): 3 parallel adds lost edges in 6/6 runs before and kept all edges in 6/6 after. Ceiling: 12 parallel writers hit the existing stale-lock message (`orchestrate.rs:1327`).
- **Reopen if.** The golden changes.

## ADR-038 Studio HTML mock update and title

- **Context.** Q15, CON9, AC-011, AC-022, R21, R23.
- **Decision.** Update `plans/mcp-server-enhancement/mcp-studio-server.html` as options s1.3 lists: remove Google Fonts (lines 123-125) for system stacks; shipped tool names in shipped order; "TELEMETRY MCP-ONLY · DEFAULT ON · OPT-OUT"; relabel operation events as MCP-caller only; rewrite the "separate process" guardrail to the in-process monitor; principles state the fragment token and the ADR-020 retention values; replace "human-mode approvals rejected" with ADR-013 wording; keep "proposed" for MCP until ADR-034's evidence. **Title:** the mock's `<title>` and h1 read "Orca MCP Studio" (lines 2 and 130, Observed). The proposed default replaces "Orca MCP Studio" at both places with "dtech-aidlc MCP Studio", the project's name in place of what looks like a stray product name; the live page is titled "dtech-aidlc Monitor". The Studio unit keeps the current title until the owner confirms, so no naming changes by implication.
- **Status.** Update owner-decided (Q15); title is an owner follow-up, non-blocking (scope-boundary.md section 4).
- **Reopen if.** The owner says "Orca" is intended.

## ADR-039 Acceptance-criteria readings applied at the release gate

- **Context.** Several AC texts admit two readings under the owner decisions (options s1.1, s1.2, s3). Analyze owns the ACs; this stage records the readings so the release gate applies them consistently.
- **Decision.** (1) AC-017 "off (trusted input only)": no trusted input exists (ADR-023), so the dashboard shows "Setting: unknown" and never "off" in v1; compliant with AC-001 scenario 2. "Shutting down" is the client-detected "Monitor stopped or unreachable" state. (2) AC-014: GET-only is "only read requests"; HEAD and OPTIONS get 405. (3) AC-013 "telemetry location is unavailable" means unsafe or unreadable, or a bind or random-source failure; absent is the empty state. (4) AC-004: calls rejected by `serve` before the tool host get only a protocol-error event (ADR-025). (5) AC-026: the installed CLI runs the monitor in-process (ADR-028). (6) AC-004/AC-018/AC-020: met while the writer queue does not fill, with drops disclosed (ADR-021). (7) AC-006 and AC-012 under CON18. (8) AC-019 "Reload": a reload drops the token, so the page shows "Token missing" and no data at all (ADR-029); filters are back to defaults only in that sense, and the user must reopen the printed address to see data again. For the AI design review to accept or change. (9) AC-024: the CHANGELOG review checklist includes the statement that these are local optional capabilities (ADR-033); an entry without it is a "Missing item". The AI dashboard design review (AC-022) and the AI threat review (AC-030) each record the readings that touch them.
- **Status.** Recommended - pending gate approval.
- **Reopen if.** Analyze amends any of these ACs, or the owner rejects a reading.

## Decisions not taken here

- Module and file names, the response field names and the test layout belong to implementation-design, which also mints FN-/IF-/TS-/WU-/CFG- ids.
- All numeric values (queue 1024, 500 ms flush, retention, read caps, 16 connections, 5 s timeouts, 15 s stale, 15 min quiet, n >= 20) are Estimates, not measured against real host sessions.
- Windows behaviour (BCryptGenRandom link and run, delete-pending pruning, in-use upgrade, batch prompt, firewall prompt) is Assumption until windows-latest CI and one manual desktop check.
