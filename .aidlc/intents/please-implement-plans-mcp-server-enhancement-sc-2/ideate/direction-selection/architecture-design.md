# Architecture design

- Purpose: the arc42 view of the selected direction for intent 2 (local MCP telemetry, the read-only loopback `dtech-aidlc monitor` with its dashboard, and release hardening), so implementation-design can mint contracts against one picture. It describes the selected direction only (Option A). Decisions are written once, in [decision-record.md](decision-record.md) (ADR-019..ADR-039), and are only listed in section 9.
- Status: Draft - pending solution-direction gate. The direction (Option A) and the Analyze decisions it carries are owner-decided (User-provided, project owner, 2026-11-28). The technical details are Recommended until the gate passes. In AI orchestration mode the gate may be approved by "AI Orchestrator"; that is not an owner approval of the details.
- Stage: Ideate / direction-selection (decision-facilitator). Companions: [recommended-direction.md](recommended-direction.md), [decision-record.md](decision-record.md), [scope-boundary.md](scope-boundary.md), [threat-model.md](threat-model.md).
- Existing architecture: not restated here. See [the codebase map](../../../../knowledge/_codebase-map.md) (components, entry points, data, tests, known debt; stale on version and CI, Q25), [the intent-1 architecture design](../../../please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/architecture-design.md) (the `application` facade, the `dtech-aidlc-mcp` stdio server, protocol layer and tool adapter), [the intent-1 knowledge record](../../../../knowledge/please-implement-plans-mcp-server-enhancement-sc.md) (what intent 1 actually shipped), and [PROJECT.md](../../../../../PROJECT.md) "Architecture overview" (stale facts noted in R26, Q16). No `adr`, `architecture` or `api-spec` source is registered for this intent beyond the intent-1 decision record (Observed: intent-1 ADR-001..ADR-018 are the accepted records checked).
- Sources: Analyze baseline `../../analyze/problem-framing/*` and `../../analyze/context-and-risk/*` (AC-001..AC-030, US-001..US-013; Q1-Q25, CON1-CON18, ASM1-ASM9 and R1-R26 are local row labels, not citable ids); [solution-options.md](../option-generation/solution-options.md), [trade-off-matrix.md](../option-generation/trade-off-matrix.md); `plans/mcp-server-enhancement/*` ("plan" = observability implementation plan, "design" = detailed design; the owner's proposal, not an approved design).
- Labels: Observed, User-provided, Assumption, Estimate, Open question. Code lines are Observed at commit 3336980 unless stated. `<spike2>` = `/private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/spike2/` (macOS arm64, rustc 1.100.0 only). No FN-/IF-/TS-/WU-/CFG- identifiers are minted here; interface and module names below are descriptive, and implementation-design registers them.

## 1. Introduction and goals
- Purpose: let the project owner and assistant users see, on their own machine, what the MCP server did (tool calls, operations, workflow transitions, protocol errors, timings and outcomes) without changing what it does, and release the combined intent-1 and intent-2 work as 1.3.0 (prepared, not tagged) with installers for both binaries (US-001..US-011, US-013; US-012 DeepSeek deferred).
- Quality goals: see [quality-requirements.md](../../analyze/context-and-risk/quality-requirements.md). In priority order for this design:
  1. **Telemetry never changes the workflow.** Same replies, exit codes, state, audit and stdout with telemetry on, off, failing, panicking or stalled; the reply never waits on telemetry I/O (AC-010, AC-028; Availability).
  2. **Privacy by allowlist.** Metadata only, validated ids only, nothing leaves the machine, the store is git-ignored, the dashboard says what is kept (AC-002, AC-005, AC-008, AC-011, AC-020; Privacy).
  3. **Loopback, read-only, browser-safe monitor.** 127.0.0.1 only, GET only, exact Host and Origin, no CORS, per-run token, reads only `.aidlc/telemetry` (AC-013..AC-016, AC-030; Privacy).
  4. **Honest dashboard.** Every state shown, counts always, percentiles only with enough samples, liveness "unknown", coverage gaps and drops disclosed (AC-017, AC-018; Availability).
  5. **Release and platform consistency.** Zero new crates, MSRV 1.100, CLI goldens change only for the three owner-allowed reasons, Windows and macOS CI green, installers for both binaries (AC-023..AC-027, AC-029; Operability).
- Stakeholders: see [stakeholder-map.md](../../analyze/problem-framing/stakeholder-map.md). Design-relevant roles: the project owner (runs the monitor, decides gates and follow-ups), the assistant user (indirect: their host spawns the MCP server with telemetry on by default), the MCP host (passes launch arguments, including `--telemetry off`), maintainers (review the hand-rolled HTTP server and the Windows FFI), and the AI reviewers of the dashboard design (AC-022) and the threat model (AC-030).

## 2. Constraints
- Technical:
  - Zero new crates (Q3, owner-decided): dependencies stay `serde_json` and `sha2`; HTTP, randomness and constant-time compare are std-only or raw OS calls (ADR-027, ADR-029). Cargo.lock changes only for the 1.3.0 package line (ADR-033).
  - `rust-version = "1.100"`. `std::random` is unstable on it (Observed, `<spike2>/decide/r.rs`, E0658), so the OS random source is read directly (ADR-029).
  - CLI behaviour stays frozen except the scoped re-bless (Q2, Q4): the version, `monitor` in help, completion and validation, and one managed `.gitignore` line (ADR-033; AC-027).
  - MCP stdout carries only protocol frames; `serve` stays the only stdout writer (AC-028; intent-1 ADR-005).
  - No phase-graph, state or audit schema change; telemetry never takes the state lock and is never read by workflow code (AC-007, AC-016).
  - MCP tool names and schemas unchanged (CON10, intent-1 ADR-009); no `harness/` edits (CON14).
  - Telemetry is MCP-only; the CLI writes no events (Q1, CON18).
- Organizational:
  - Owner decisions of 2026-11-28 (User-provided): Option A; Q1, Q2, Q3, Q4, Q5, Q10, Q11, Q12, Q13, Q15; DeepSeek deferred. See [decision-record.md](decision-record.md) "How to read the status line".
  - AI orchestration mode: the dashboard design is AI-reviewed before it is built (AC-022, owner-decided Q15), and the threat model is AI-reviewed before release (AC-030, from the Analyze gate [follow-up] under the AI orchestrator approval contract, not an owner decision).
  - PROJECT.md and the codebase map are not edited by this workflow (Q16, Q25; owner follow-ups in [scope-boundary.md](scope-boundary.md) section 4).
  - 1.3.0 is prepared only: no tag, no publish (Q5).
  - CI exists on Windows and macOS (Ubuntu best-effort) since intent 1 (Observed, commit ca30a85); a local Windows target is not available, so Windows claims rest on CI.
- Convention:
  - MCP statements in user docs keep the "proposed" label until both CI legs are green and a live Claude Code `tools/call` is observed (Q10, CON6; ADR-034).
  - A golden change needs `DTECH_AIDLC_BLESS=1` plus a CHANGELOG entry and a filtered-diff review (ADR-033).
  - `tests/repo_content.rs` pins README sections, help/completion/args parity and CHANGELOG headings (Observed, R16).
  - Raw Win32 calls use `#[link] extern "system"` with a SAFETY comment, following `rust-cli/dependencies.rs:338-361` and `rust-cli/knowledge.rs:118-160` (Observed).

## 3. Context and scope
- System context (C4 System Context):

```text
 assistant user ── prompts ──> MCP host (Claude Code | DeepSeek Harness*) ── spawns with [--project] [--telemetry on|off]
                                        │ stdio JSON-RPC (unchanged, intent 1)
                                        v
                               dtech-aidlc-mcp ──appends──> <project>/.aidlc/telemetry/mcp-*.jsonl  (new, git-ignored)
                                        │                                   ^
                                        │ reads/writes workflow state       │ reads only (lists dir, regular files)
                                        v                                   │
                               <project>/.aidlc/ (state, audit, config)     │
                                                                            │
 project owner ── terminal ──> dtech-aidlc monitor ── binds 127.0.0.1:<ephemeral> ── prints URL#token on stderr
                                        ^ HTTP GET (loopback only, Bearer token)
                                        │
                               browser on the same machine ── opens printed URL
 (* DeepSeek verification deferred, US-012)
```

| Element | Responsibility | Interface |
|---|---|---|
| MCP host | Spawns `dtech-aidlc-mcp`; decides the telemetry setting through launch arguments | argv `--project`, `--telemetry on\|off` (ADR-023); stdio frames as in intent 1 |
| `dtech-aidlc-mcp` | Serves the four tools unchanged; emits telemetry events to its own per-process file | stdin/stdout frames; stderr fixed diagnostics only; telemetry files |
| `dtech-aidlc` CLI | Existing commands unchanged; adds `monitor` | argv; `monitor` takes only the global `--project` (ADR-028) |
| `dtech-aidlc monitor` process | Serves the embedded dashboard and a read-only query API over loopback for one project | HTTP/1.x GET on `127.0.0.1:<port>`; stderr startup line; exit 1 on a start failure, otherwise runs until a signal ends it (no path exits 0; Ctrl+C on Unix shows as shell status 130) |
| Browser | Renders the dashboard; holds the token in memory only | `/`, `/app.js`, `/app.css`, `GET /api/events` with `Authorization: Bearer` (ADR-029, ADR-030) |
| `.aidlc/telemetry/` | Telemetry store: JSONL per MCP process, bounded by retention | Files `mcp-<UTC>-<pid>-<8 hex>-<part>.jsonl` (ADR-020) |
| Installers | Install, upgrade, uninstall both binaries into a prefix | `scripts/rust-install/install.sh`, `install.ps1` (ADR-035) |

- External systems: the MCP hosts (Claude Code Observed in intent 1; DeepSeek Harness deferred). The browser is local software, not a remote system. There is no outbound network peer (AC-011): no remote font, script, update check or export.
- Out of scope: see [scope-boundary.md](scope-boundary.md) section 2. In short: DeepSeek verification, tag and publish, CI/lint cleanup, CLI telemetry, env-var opt-out, any crate, a monitor supervisor or flags, write routes, server push or heartbeat, drop counters and gate approve/reject values (both need an AC-002 change), park/revisit events.

## 4. Solution strategy
- Chosen approach: Option A (owner-decided; [recommended-direction.md](recommended-direction.md) section 1).
  - **Telemetry off the request path.** The MCP adapter builds events with an explicit field constructor and hands them to a bounded queue with `try_send`; one writer thread per process owns the file. The reply never waits on disk, and every telemetry failure collapses to "telemetry disabled for this process" plus one fixed stderr line (ADR-021).
  - **Hook at the adapter, not the facade.** A `traced_call` wrapper becomes `ToolHost.call`; `serve` gains a code-only `on_error` observer. `application.rs` and the CLI stay untouched, so CLI bytes cannot move from telemetry (ADR-022, ADR-025).
  - **Tests stay deterministic by argument, not by skipping.** Existing snapshot spawns pass `--telemetry off`, so each snapshot also proves "off writes nothing" (ADR-022).
  - **Monitor in-process, in the foreground.** `dtech-aidlc monitor` binds loopback, prints one URL, and stops with Ctrl+C. No supervisor, child or daemon (ADR-028).
  - **Hand-rolled, small, strict HTTP.** std `TcpListener`, GET only, exact route table, exact Host and Origin, fixed security headers, one stable error table (ADR-027, ADR-032).
  - **Server-side query.** The monitor reads the store per request within read caps and returns one page plus a whole-range summary and an opaque cursor (ADR-030).
  - **Per-run token in the fragment.** 128 bits from the OS random source; never on the wire except as a Bearer header to the same origin (ADR-029).
- Rejected alternatives: B (plan-literal: synchronous facade sink, supervisor, typed token), C (no token, browser-side filtering), D (static snapshot), and the unbundled synchronous writer. Reasons are in [recommended-direction.md](recommended-direction.md) section 2 and are not repeated here.

## 5. Building block view
- Containers (C4 Container):

| Container | Responsibility | Interface | Change |
|---|---|---|---|
| `dtech-aidlc-mcp` binary | Stdio server (intent 1) plus the telemetry sink for its own process | argv `--project`, `--telemetry`; stdio; telemetry files | Extended |
| `dtech-aidlc` binary | CLI (intent 1) plus the `monitor` command hosting the HTTP server | argv; stderr startup line; loopback HTTP | Extended |
| Library crate | Shared domain modules (unchanged by telemetry), the OS random helper, the telemetry event and reader types shared by writer and monitor | Rust API | Extended |
| Embedded dashboard assets | Static UI compiled into the CLI binary | `include_str!` of `index.html`, `app.js`, `app.css` (ADR-031) | New |
| Install scripts | Place both binaries and launchers in a prefix | sh / pwsh arguments (ADR-035) | Extended |

- Components (C4 Component):

```text
 dtech-aidlc-mcp
   stdin ─> [serve (protocol.rs)] ─ host.call ─> [Event hook: traced_call (tools.rs)] ─> [tool fns] ─> [application facade]
                │  on_error(&C, &'static str)            │ build event (allowlist)
                └────────────────────────────────────────┤
                                                         v
                                              [Telemetry sink: bounded queue 1024] ─try_send─> [writer thread] ─> .aidlc/telemetry/*.jsonl
                                                                                                    └─ rotation + prune

 dtech-aidlc monitor
   127.0.0.1:<port> ─> [Monitor server: accept, cap 16, head parse, checks 0-9] ─┬─ /, /app.js, /app.css ─> [Dashboard assets]
                                                                                 └─ /api/events ─> [Query engine] ─ lists + reads ─> .aidlc/telemetry/
```

| Component | Responsibility | Interface (descriptive; registered later) | Decisions |
|---|---|---|---|
| **Event hook** (MCP adapter, `rust-cli/mcp/tools.rs`; observer in `protocol.rs`) | Per tool call: `mcp.request.started`, `operation.started`/`completed` around `application::open` plus the operation, workflow events after Ok, `mcp.tool.completed`. Maps outcomes and error codes. Validates ids against the `tools.rs` validators before storing. Own `catch_unwind`, then `resume_unwind` so `serve` keeps -32603. Protocol errors arrive through `on_error` with a static code. | `traced_call` as `ToolHost.call`; `on_error: fn(&C, &'static str)` | ADR-022, ADR-024, ADR-025 |
| **Telemetry sink** (MCP process) | Owns the queue and the writer thread. Producer side: `try_send` only, first-drop stderr line, disable on disconnect or random-source failure. Writer side: lazy directory creation under `.aidlc` (never creates `.aidlc`), lstat safety check, `create_new` files (0o600, dir 0o700 on Unix), self-ignore `.gitignore`, JSONL append, rotation at 5 MiB, prune at start and rollover, 500 ms EOF flush. | Constructed from the parsed flag; `send(Event)`; writer parameter injectable in unit tests | ADR-020, ADR-021, ADR-023, ADR-026 |
| **OS random helper** (library) | 16 bytes from `/dev/urandom` (Unix) or `BCryptGenRandom` (Windows); fail closed. Used for sessionId and the monitor token. | `os_random(&mut [u8]) -> io::Result<()>` | ADR-026, ADR-029 |
| **Monitor server** (CLI) | Bind `127.0.0.1:0`; print the startup line; thread per connection capped at 16; head deadline 5 s, head cap 8 KiB, write timeout 5 s; run checks in the fixed order; write fixed security headers on every response; half-close and drain on refused methods; constant-time token compare; no per-request logging. | HTTP/1.x; error body `{"error":"<code>"}`; status table in ADR-032 | ADR-027, ADR-028, ADR-029, ADR-032 |
| **Query engine** (CLI) | Validate parameters against allowlists; re-check `.aidlc/telemetry` with lstat per query; list pattern-matching regular files; read within caps (512 files, 64 MiB, 250k events); skip and count malformed, oversized (> 16 KiB), unknown or expired lines and files; re-validate every field; sort newest first; compute summary and coverage; encode and verify the cursor (sha2 prefix of the normalised query plus frozen `to`). Parse cache keyed by (len, mtime). | `GET /api/events?range&component&name&outcome&stage&session&trace&limit&cursor` -> page, summary, coverage, `nextCursor`, `limitReached` | ADR-028, ADR-030 |
| **Dashboard assets** (embedded) | `index.html` (landmarks, labels), `app.js` (reads and strips the fragment, polls, renders only via `textContent`, state machine for the AC-017 states), `app.css` (system fonts, focus, reduced motion, 320 px layout). No remote asset. | Routes `/`, `/app.js`, `/app.css`; no token needed (they carry no data) | ADR-029, ADR-030, ADR-031 |
| **CLI command wiring** (`args.rs`, help, completion) | Register `monitor` with no flags or positional arguments; dispatch after `assert_safe_state_dir`. | `dtech-aidlc [--project <p>] monitor` | ADR-028, ADR-033 |
| **Installers** | Copy both binaries, write launchers, marker `v2`, Windows in-use preflight, best-effort post-commit cleanup. | `install.sh`, `install.ps1` | ADR-035 |

- Responsibilities (split rule):
  - Event hook: decides *what* is observed and maps it to the AC-002 field set. Knows MCP tool semantics; knows nothing about files.
  - Telemetry sink: decides *how* events are stored. Knows files and retention; knows nothing about tools.
  - Protocol layer: still knows nothing about AIDLC or telemetry beyond calling a static-code observer.
  - Query engine: the only reader of the store. Never writes, never deletes, never reads outside `.aidlc/telemetry`.
  - Monitor server: transport and access control only; knows nothing about event semantics.
  - Dashboard: presentation only; trusts nothing it receives beyond rendering it as text.
  - The event schema (field names, kinds, validators, file-name pattern) is one shared library type used by both the sink and the query engine, so writer and reader cannot drift (AC-003).

## 6. Runtime view
- Main scenario 1 - tool call with telemetry on (AC-004, AC-006, AC-007, AC-028):

```text
 host        serve            traced_call              tool + facade          sink queue        writer thread
  │ tools/call ─>│ host.call ─────>│ try_send(request.started) ─────────────────────>│ append line
  │              │                 │ try_send(operation.started) ───────────────────>│
  │              │                 │ ─ application::open + op ─>│                    │
  │              │                 │ <── Ok / Err ──────────────│                    │
  │              │                 │ try_send(operation.completed, durationMs) ─────>│
  │              │                 │ [Ok + durable] try_send(workflow.*) ───────────>│
  │              │                 │ try_send(tool.completed, outcome, errorCode) ──>│
  │              │ <── result ─────│                                                 │ (rotate at 5 MiB, prune)
  │ <── reply ───│ (stdout, only writer)
```

  - Trace: every event of one call carries one `traceId`; the tool span (`request.started`/`tool.completed`) is the root with `parentSpanId` null; the operation span's parent is the tool span; each `workflow.*` event's parent is the operation span; a protocol error is its own one-event trace (ADR-026).
  - Workflow events are sent only after the facade returns Ok, that is after state and audit are written (AC-007). `gate_decided` is sent only for a successful `report` that carried `approval`, with `name=report` and no approve/reject value (ADR-024).
  - The producer stamps timestamps and `durationMs`; the writer only serialises and appends.
  - Protocol error: `serve` writes the error reply first, then calls `on_error` with a static code (`line_too_long`, `parse_error`, `invalid_request`, `method_not_found`, `invalid_params`, `unsupported_protocol_version`, `internal_error`, `protocol_error`). A `tools/call` rejected before `host.call` yields only this protocol event (ADR-025; ADR-039 reading 4).
- Main scenario 2 - monitor request with token (AC-013, AC-015, AC-016, AC-019):

```text
 owner terminal          monitor server                         browser (same machine)
  │ dtech-aidlc monitor ─>│ assert_safe_state_dir; os_random(16) -> token; bind 127.0.0.1:0
  │ <─ stderr: "monitor: http://127.0.0.1:<port>/#token=<32hex>  (Ctrl+C to stop)"
  │ opens URL ──────────────────────────────────────────────────>│ GET /  (fragment never sent)
  │                       │ checks 0-8 pass; 200 index.html  ───>│ GET /app.js, /app.css
  │                       │                                      │ app.js: read location.hash once -> closure const
  │                       │                                      │         history.replaceState(null,'','/')
  │                       │ <── GET /api/events?range=3600 + Authorization: Bearer <token>
  │                       │ checks 0-9 (Host, Origin, Sec-Fetch-Site, route, ct_eq token), 10-12 (params, range, cursor)
  │                       │ Query engine: lstat dir, list, read within caps, summary, cursor
  │                       │ ── 200 JSON page (+ limitReached if a cap hit) ──>│ render via textContent; poll every 5 s
  │ Ctrl+C ──────────────>│ default handler, killed by SIGINT      │ next fetch: network error -> "Monitor stopped
  │                                                                │ or unreachable"; auto-poll stops
```

  - Reload or a duplicated tab has no fragment: the page shows "Token missing: reopen the address printed in the terminal" and makes no API call (AC-019 "Reload").
  - Paging: the cursor freezes `to` and binds the normalised query; a changed filter with an old cursor gets `409 cursor_stale` and the page restarts from the first page (AC-019).
- Main scenario 3 - rotation and prune (AC-009, AC-012):
  - At writer start (first event) and at each rollover the writer lists `.aidlc/telemetry`, considers only pattern-matching regular files (symlink_metadata), never its current file, and deletes first those older than 7 days by mtime, then oldest first until the total is under 50 MiB. Every delete error is ignored (a file held by another process on Windows stays until a later prune; Assumption, R7).
  - When the current file would exceed 5 MiB, the writer closes it and opens the next part (`-<part+1>`) with `create_new`; on AlreadyExists it tries the next part number.
  - Concurrent MCP processes each own their own file (pid plus sessionId prefix in the name), so no two writers share a file (AC-012). Known ceiling (Estimate): between prunes the total can exceed 50 MiB by up to one open file per live process.
  - The monitor never deletes. Expired files still present are counted and reported in the coverage line until a telemetry-on MCP process prunes them at its writer's first event or a rollover (ADR-020, ADR-028 (7)).
- Failure scenarios:

| Failure | Behaviour | Criteria | Decision |
|---|---|---|---|
| Writer slow or stalled (disk, AV scan) | Queue fills; later events are dropped; first drop prints one fixed stderr line; reply unaffected | AC-010, AC-028 (scenario 4); AC-004/AC-018 Met* | ADR-021 |
| Writer panic or I/O error | Channel disconnects; telemetry disabled for the process; one fixed stderr line | AC-010 | ADR-021 |
| Event build panics in the producer | Caught; telemetry disabled; reply unchanged | AC-028 (scenario 3) | ADR-021 |
| `.aidlc` absent | No telemetry, silently; `.aidlc` never created | AC-008 | ADR-020 |
| `.aidlc/telemetry` is a symlink or a file | Telemetry disabled; one fixed stderr line with no path; nothing written through the link | AC-008 | ADR-020 |
| OS random source fails in the MCP process | Telemetry disabled; server keeps working | AC-010 | ADR-026 |
| Monitor: store unsafe or unreadable at start; bind or random failure | Exit 1, fixed message, no path | AC-013 | ADR-028 |
| Monitor: store absent | Starts; "No telemetry observed" | AC-013, AC-017 | ADR-028 |
| Monitor: store becomes unsafe while running | `503 store_unavailable`; page keeps last-known data marked stale | AC-017 | ADR-028, ADR-032 |
| Connection flood | 17th concurrent connection gets `503 busy`; slow heads get `408` at 5 s | AC-014 | ADR-027 |
| Handler panic | `500 internal_error`; slot released by a Drop guard | AC-014 | ADR-027 |
| Read cap reached | `200` with partial newest-first data and `limitReached` | AC-016, AC-017 | ADR-030 |
| Corrupt, oversized or unknown lines | Skipped and counted in coverage; growing file's unterminated tail counted as partial | AC-003, AC-017 | ADR-028 |

## 7. Deployment view
- Environments:
  - Developer workstation (macOS, Windows; Linux best-effort): the only runtime environment. Everything runs as the user's own account.
  - CI: GitHub Actions windows-latest and macos-latest required, ubuntu best-effort (AC-029). Windows-only behaviour (BCryptGenRandom link and run, delete-pending prune, in-use upgrade preflight, read-timeout error kind) is verified only there (Assumption until then).
- Runtime nodes:

```text
 workstation (one OS user)
 ├─ <prefix>/bin/dtech-aidlc            (POSIX exec launcher | Windows .cmd)  ─┐
 ├─ <prefix>/bin/dtech-aidlc-mcp        (POSIX exec launcher | Windows real .exe)
 ├─ <prefix>/lib/dtech-aidlc/           dtech-aidlc[.exe], dtech-aidlc-mcp[.exe], marker v2
 ├─ MCP host process ── spawns (no shell) ──> dtech-aidlc-mcp process (+1 writer thread)
 ├─ terminal ── dtech-aidlc monitor process (listener on 127.0.0.1:<ephemeral>, ≤16 conn threads)
 ├─ browser tab ── loopback HTTP ──> monitor
 └─ <project>/.aidlc/telemetry/  (0o700 dir, 0o600 files on Unix; inherited ACLs on Windows)
```

  - Two binaries from one package, zero new crates; each embeds what it needs (the CLI also embeds the three dashboard assets). Version 1.3.0 is prepared in the tree but not tagged or published (ADR-033).
  - Network: loopback only. The listener binds `127.0.0.1`, never `0.0.0.0`; a connection over the LAN address is refused (Observed, `<spike2>/http/results.txt`). No outbound connection exists (AC-011). Windows and macOS loopback binds are assumed to raise no firewall prompt (ASM6; one manual desktop check on Windows).
  - Upgrade while running: POSIX stage-then-rename leaves running processes on the old inode (Observed, `install.sh:121-125`). Windows refuses to proceed while an installed exe is running and changes nothing (ADR-035; Assumption until windows-latest CI).
  - Windows console: Ctrl+C on `dtech-aidlc monitor` through the `.cmd` launcher may print "Terminate batch job (Y/N)?" after the monitor has already stopped (Assumption; documented in README troubleshooting, ADR-028). Hosts spawn the real MCP exe, so they never see it.

## 8. Cross-cutting concepts
- Security: see [threat-model.md](threat-model.md). Summary of the mechanisms the building blocks share: loopback bind, exact Host/Origin/Sec-Fetch-Site checks, GET only, no CORS, per-run fragment token with constant-time compare, exact route table without filesystem mapping, fixed security headers and CSP without `unsafe-inline`, `textContent`-only rendering, fail-closed random source.
- Privacy and redaction (Data):
  - Field allowlist: events are built field by field to exactly the AC-002 set; the monitor re-serialises only allowlisted fields and never returns raw lines (ADR-024, ADR-030).
  - Never stored: tool arguments beyond validated ids, `detail`, `approved_by`, approval notes, messages, raw frames, paths, argv, env, unknown tool names (stored as `unknown`). Protocol errors carry a `&'static str` code, so frame content cannot reach the store by type (ADR-025).
  - Stored and disclosed: validated `intentId`/`stageId`/`unitId` (slugs of user text, owner-accepted Q13), pid, timings, outcomes, error codes. The dashboard header says "local project", never the path (AC-020).
  - Location and lifetime: `.aidlc/telemetry/` only, git-ignored twice (IGNORE_BLOCK `/telemetry/` line plus a self-ignore `*`), 7 days / 50 MiB, Unix 0o600/0o700 (ADR-020).
  - Telemetry is not audit; approver names are self-asserted (README and CHANGELOG; AC-030).
- Failure isolation:
  - Telemetry: one rule, "any failure disables telemetry for this process and prints one fixed stderr line", applied to unsafe location, random failure, writer panic, writer I/O error and producer panic. The only stdout writer stays `serve` (AC-010, AC-028).
  - Monitor: per-connection threads with a Drop guard and panic isolation; per-query lstat re-check; the monitor never writes or deletes anything (AC-014, AC-016).
  - Dashboard: polls every 5 s, doubles to 60 s on HTTP errors, pauses while hidden, stops on network error (prevents a stale tab sending its token to a later process on the same port) (ADR-030).
- Observability:
  - This intent *is* the observability feature for the MCP server. The monitor itself logs nothing per request (stderr carries only the startup line), so its own access is not observed; this is deliberate to keep the token and request data out of logs.
  - Coverage honesty: the coverage line reports files read, skipped lines, partial tails, unreadable and expired files, "Dropped events: not recorded", "CLI writes no events", and the kinds not observed in v1 (park, revisit, intent pause/resume/abandon) (AC-006, AC-017, AC-018).
- Windows:
  - Random source through `BCryptGenRandom` (first link against bcrypt); the MCP binary installed as a real exe; the CLI keeps its `.cmd` launcher and the batch-prompt note; in-use upgrade preflight and best-effort post-commit cleanup fixing the rollback path at `install.ps1:80` and `:87-88` (Observed bug); delete-pending prune tolerated; read timeouts matched as both `TimedOut` and `WouldBlock`; telemetry files inherit ACLs. All Assumption until windows-latest CI, plus one manual desktop check for the firewall prompt and the batch prompt (ADR-027, ADR-028, ADR-029, ADR-035).
- Configuration: keys, flags and environment variables (Develop registers each as `CFG-` in ids.json):
  - `dtech-aidlc-mcp --telemetry on|off` (default `on`; duplicate, missing or other values exit 1) (ADR-023).
  - `dtech-aidlc-mcp --project <path>` (existing, intent 1).
  - `dtech-aidlc [--project <path>] monitor` (no command flags) (ADR-028).
  - No environment variable is added. Existing ones are unchanged: `DTECH_AIDLC_HOME`, `DTECH_AIDLC_BLESS` (tests). `DTECH_AIDLC_ASSETS` stays a no-op in the Windows launchers (Q18).
  - The numeric values (queue 1024, flush 500 ms, 5 MiB / 7 days / 50 MiB, read caps, 16 connections, 5 s timeouts, 8 KiB head, page 50/200, range 604800 s, 5 s poll, 15 s stale, 15 min quiet, n >= 20) are compiled-in constants, not configuration (ADR-020, ADR-021, ADR-027, ADR-030).

## 9. Architecture decisions
- ADR-019 Direction: Option A, and the owner decisions it carries — accepted (owner-decided) — see [decision-record.md](decision-record.md#adr-019-direction-option-a-and-the-owner-decisions-it-carries)
- ADR-020 Telemetry storage, file format, rotation and retention — proposed (validated ids owner-decided, Q13; location from the Analyze baseline) — see [decision-record.md](decision-record.md#adr-020-telemetry-storage-file-format-rotation-and-retention)
- ADR-021 Background writer thread, failure isolation and dropped events — proposed (background writer owner-decided) — see [decision-record.md](decision-record.md#adr-021-background-writer-thread-failure-isolation-and-dropped-events)
- ADR-022 Operation hook in the MCP adapter; tests pass `--telemetry off` — proposed (adapter hook owner-decided) — see [decision-record.md](decision-record.md#adr-022-operation-hook-in-the-mcp-adapter-tests-pass---telemetry-off)
- ADR-023 Opt-out flag `--telemetry on|off` — proposed (default on and flag principle owner-decided) — see [decision-record.md](decision-record.md#adr-023-opt-out-flag---telemetry-onoff)
- ADR-024 Event vocabulary and schema v1, including `workflow.gate_decided` — proposed — see [decision-record.md](decision-record.md#adr-024-event-vocabulary-and-schema-v1-including-workflowgate_decided)
- ADR-025 Outcome and error mapping, and the protocol-error hook — proposed — see [decision-record.md](decision-record.md#adr-025-outcome-and-error-mapping-and-the-protocol-error-hook)
- ADR-026 Ids: sessionId from the OS random source — proposed — see [decision-record.md](decision-record.md#adr-026-ids-sessionid-from-the-os-random-source)
- ADR-027 HTTP server: std only, zero crates — proposed (zero crates owner-decided) — see [decision-record.md](decision-record.md#adr-027-http-server-std-only-zero-crates)
- ADR-028 Monitor process model, lifecycle and edge cases — proposed (in-process monitor owner-decided) — see [decision-record.md](decision-record.md#adr-028-monitor-process-model-lifecycle-and-edge-cases)
- ADR-029 Monitor token: source, delivery, page storage, comparison — proposed (fragment token owner-decided) — see [decision-record.md](decision-record.md#adr-029-monitor-token-source-delivery-page-storage-comparison)
- ADR-030 Query and cursor API, read caps and refresh values — proposed (server-side query owner-decided) — see [decision-record.md](decision-record.md#adr-030-query-and-cursor-api-read-caps-and-refresh-values)
- ADR-031 Dashboard assets embedded as three static files — proposed — see [decision-record.md](decision-record.md#adr-031-dashboard-assets-embedded-as-three-static-files)
- ADR-032 One HTTP status and error-code table — proposed — see [decision-record.md](decision-record.md#adr-032-one-http-status-and-error-code-table)
- ADR-033 Version 1.3.0, scoped re-bless and CHANGELOG — proposed (version, scope and re-bless reasons owner-decided) — see [decision-record.md](decision-record.md#adr-033-version-130-scoped-re-bless-and-changelog)
- ADR-034 README update and the "proposed" label — proposed (evidence rule owner-decided) — see [decision-record.md](decision-record.md#adr-034-readme-update-and-the-proposed-label)
- ADR-035 Installers install both binaries — proposed (both binaries owner-decided) — see [decision-record.md](decision-record.md#adr-035-installers-install-both-binaries)
- ADR-036 Flaky-test fixes and the missing intent-1 MCP tests — proposed (scope owner-decided) — see [decision-record.md](decision-record.md#adr-036-flaky-test-fixes-and-the-missing-intent-1-mcp-tests)
- ADR-037 `deps add` takes the state lock — proposed (scope owner-decided) — see [decision-record.md](decision-record.md#adr-037-deps-add-takes-the-state-lock)
- ADR-038 Studio HTML mock update and title — proposed (update owner-decided; title is an owner follow-up) — see [decision-record.md](decision-record.md#adr-038-studio-html-mock-update-and-title)
- ADR-039 Acceptance-criteria readings applied at the release gate — proposed — see [decision-record.md](decision-record.md#adr-039-acceptance-criteria-readings-applied-at-the-release-gate)

Intent-1 records touched (statuses kept in [the intent-1 decision record](../../../please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/decision-record.md); the amending decision is named in each entry above): ADR-002 amended (ADR-023, ADR-033, ADR-035); ADR-005 amended (ADR-025: observer and rule 5); ADR-006 amended (ADR-022); ADR-017 amended (ADR-034); ADR-001 extended (ADR-027); ADR-018 fulfilled (ADR-033); ADR-009 kept unchanged on purpose (ADR-022, `readOnlyHint`). No intent-1 ADR is superseded as a whole. Statuses use the template set: "accepted" only where the whole decision is an owner choice (User-provided, 2026-11-28); "proposed" where details are recommendations, with the owner-decided part in brackets.

## 10. Quality requirements
- See [quality-requirements.md](../../analyze/context-and-risk/quality-requirements.md). Each scenario names its quality category, the criteria and the check that measures it. A check that was not run is not a pass; values are Estimates fixed by ADR-030 and ADR-021 unless marked Observed.

| # | Category | Scenario | Measure | AC / US |
|---|---|---|---|---|
| QS-1 | Availability | Telemetry on, off, unwritable (read-only dir, file in place of dir), corrupt store, panicking producer | Same tool replies, exit codes, state, audit and stdout bytes in all conditions; at most one fixed stderr line | AC-010, AC-028; US-004, US-010 |
| QS-2 | Performance | Writer stalled 10 s by an injected slow writer | Tool reply returns in under 1 s; producer never blocks (spike: 1000 `try_send` in 0.39 ms with a stalled writer, Observed `<spike2>/review/tspike`) | AC-028 (scenario 4); US-004, US-010 |
| QS-3 | Privacy | Sentinel values (credential-like string, env value, absolute path, approver label, approval note, unknown tool name, secret inside a malformed frame) passed through every tool and protocol path | Zero hits when the whole store and every monitor response are grepped; field set equals AC-002 exactly | AC-002, AC-005, AC-020; US-001, US-002 |
| QS-4 | Privacy | Forged Host (`evil.example:<port>`, `localhost:<port>`, no port, missing), foreign or `null` Origin, cross-site Sec-Fetch-Site, OPTIONS preflight, over a raw `TcpStream` | 421 / 403 / 405 as ADR-032; no `Access-Control-*` header on any response | AC-014, AC-015; US-005 |
| QS-5 | Privacy | `/api/events` without, with a malformed, or with a wrong token; static routes without a token | 401 `unauthorized` with `WWW-Authenticate: Bearer`; static routes 200; token absent from stderr after startup, telemetry, MCP stdout and argv | AC-015, AC-019, AC-030; US-005, US-011 |
| QS-6 | Privacy | Traversal targets (`/../etc/passwd`, `%2e%2e`, backslash, `//etc/passwd`), `?path=` and `?file=` parameters, symlinked file in the store | 404 or 400 `invalid_filter`; no file outside `.aidlc/telemetry` opened; symlink ignored | AC-016; US-005 |
| QS-7 | Privacy | Run the monitor and dashboard with outbound traffic blocked; scan embedded assets | Dashboard fully works; no `http` URL other than the page's own origin in assets; `app.js` contains none of `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `eval`, `new Function` | AC-011, AC-016 |
| QS-8 | Availability | Monitor start with store absent, unsafe, unreadable; port binding; stop by `Child::kill`/SIGTERM | Absent: starts, empty state. Unsafe: exit 1 with fixed message. Address printed once on stderr; process gone after stop, no orphan | AC-013; US-005 |
| QS-9 | Availability | Slow-header client, 20 KB head, 40 idle sockets, 200 concurrent valid requests | 408 at 5 s; 431; 503 `busy` then recovery; 200 for all (Observed on macOS, `<spike2>/http/results.txt`; to be re-run as contract tests on both OSes) | AC-014 |
| QS-10 | Availability | Dashboard state matrix: loading, empty, partial/limit reached, stale (> 15 s), error, unavailable (503), token missing, monitor stopped, quiet (> 15 min) | Each state renders with text, not colour only; recorded in the AI design-review artifact | AC-017, AC-022; US-006, US-007 |
| QS-11 | Performance | Query on a 600-file fixture and a 70 MiB fixture; range 604801 s; limit 201 | Partial data with `limitReached`, at most 512 files and 64 MiB read; 400 `range_too_large`; 400 `invalid_filter` | AC-016, AC-017 |
| QS-12 | Availability | Percentiles with 19 and 20 completed tool calls; incomplete calls; duplicate eventIds | n=19 shows "insufficient samples"; n=20 shows p50/p95 with n; incomplete counted, never success; duplicates counted once | AC-018; US-006 |
| QS-13 | Availability | 10,000 tool calls with telemetry on; two concurrent MCP processes | Store total under 50 MiB plus one part per live process after rollover; each process has its own parseable file | AC-009, AC-012; US-003 |
| QS-14 | Accessibility | Keyboard-only range, filter, row, trace, refresh and close flows; 320 px width; reduced motion | All flows complete; no horizontal scroll; no animation under `prefers-reduced-motion`; recorded in the AI design-review checklist | AC-021, AC-022; US-007 |
| QS-15 | Operability | `cargo test --no-fail-fast` on Windows and macOS CI after the bump | At or above the 545/0 baseline, 0 ignored; only the 7 listed goldens changed, each with an empty filtered diff apart from `monitor` and `/telemetry/` lines; 1.3.0 agrees across Cargo.toml, Cargo.lock, badge, CHANGELOG, LOCK_SHA256 | AC-023, AC-024, AC-025, AC-027, AC-029; US-008 |
| QS-16 | Operability | Install, upgrade from a v1 prefix, uninstall, through sh and pwsh; Windows upgrade with the installed MCP exe running | Both binaries work (`dtech-aidlc --version`; MCP initialize + `tools/list`); unrecognised files survive; in-use upgrade exits 1 with the message and an unchanged prefix, then succeeds once the exe is closed | AC-026; US-009 |
| QS-17 | Operability | 40 repeated runs of the previously flaky tests; 3 concurrent `deps add` | 40/40 green; 3 edges kept; `deps add` golden byte-identical | AC-027, AC-029; US-013 |

## 11. Risks and technical debt
- Risks carried from [risk-register.md](../../analyze/context-and-risk/risk-register.md), with the design's answer:
  - R1 golden churn: answered by `--telemetry off` on test spawns (ADR-022). Residual: a new snapshot test that forgets the flag fails loudly, not silently.
  - R3 telemetry alters results: answered by the background writer and catch_unwind (ADR-021). Residual: drops under a stalled disk make AC-004 and AC-018 Met* (gate condition).
  - R7 Windows rotation: delete errors ignored; the total can stay above the cap while another process holds an old file. Assumption until windows-latest CI.
  - R9 hand-rolled HTTP: small surface, strict table tests (ADR-027, ADR-032). Residual Low; maintainers own a parser.
  - R10 DNS rebinding and cross-origin: exact Host and Origin plus token (threat-model.md). Residual negligible.
  - R13 monitor lifecycle: in-process, default signal handling. Residual: wrappers must forward SIGTERM (README).
  - R14, R15 installer: real MCP exe, preflight, cleanup outside the try block. Assumption until windows-latest CI.
  - R20 release traps: bump last, filtered-diff review, stop on any unlisted golden (ADR-033).
  - R22 metric honesty: n >= 20, liveness "unknown", coverage line (ADR-030).
  - R25 "proposed" label depends on a human action (Q21, owner follow-up).
  - R26 docs drift: PROJECT.md and the codebase map stay stale (Q16, Q25).
- Technical debt accepted in v1 (each with its trigger in [scope-boundary.md](scope-boundary.md) section 3):
  - No drop counter in the store; no gate approve/reject value. Both need an Analyze-owned AC-002 change.
  - No trusted "configured off" display; the dashboard shows "Setting: unknown" (ADR-039 reading 1).
  - Parse cache memory can reach about the read caps (Estimate about 75 MB at 250k events; not measured).
  - Prune overshoot between rollovers (one open file per live process).
  - The Windows CLI keeps its `.cmd` launcher and the inert `DTECH_AIDLC_ASSETS` variable.
  - The constant-time compare relies on `std::hint::black_box`; a sha256 digest compare is the documented fallback (ADR-029).
  - All numeric values are Estimates not yet measured against real host sessions.

## 12. Glossary
- **Event hook:** the `traced_call` wrapper plus the `on_error` observer that turn MCP activity into telemetry events.
- **Telemetry sink:** the per-process bounded queue plus its writer thread.
- **Span pair:** a `*.started` event and its matching `*.completed` event sharing a spanId; a completion with no start, or a start with no completion, is shown as incomplete.
- **Operation pair:** `operation.started`/`operation.completed` around `application::open` plus the operation, component `application`, caller `mcp`.
- **sessionId:** 32 hex characters from the OS random source, one per MCP process; event, trace and span ids are `<sessionId>-<counter>`.
- **Trace:** all events of one `tools/call`, sharing one `traceId`; `parentSpanId` links the operation span to the tool span and workflow events to the operation span (ADR-026).
- **Part file:** the next file a writer opens when its current file reaches 5 MiB.
- **Coverage line:** the dashboard line that states what the store cannot show (skipped lines, partial tails, expired files, drops not recorded, CLI not observed, kinds not produced).
- **Read cap:** a per-query limit on files, bytes or events; reaching one returns partial data with `limitReached`.
- **Frozen `to`:** the end time fixed in a cursor so paging is stable while new events arrive.
- **Fragment token:** the per-run token after `#token=` in the printed URL; browsers do not send the fragment in requests or Referer.
- **IGNORE_BLOCK:** the managed `.gitignore` block written by `rust-cli/knowledge.rs:22`; intent 2 adds one `/telemetry/` line to it.
- **Met\*:** an acceptance criterion met under a stated reading recorded in ADR-039, to be accepted at the release gate.
