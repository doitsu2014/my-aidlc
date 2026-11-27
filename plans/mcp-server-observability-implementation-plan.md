# MCP Server and Local Observability — Implementation Plan

- **Status:** Proposed; no MCP server, telemetry pipeline, or dashboard is implemented yet.
- **Design documents:** [Architecture design](mcp-server-architecture.md) · [Detailed design, including dashboard](mcp-server-detailed-design.md)
- **Scope:** Add an optional local MCP interface and a separate local observability dashboard while keeping the existing `dtech-aidlc` CLI fully usable.
- **Primary users:** People who drive AIDLC through an MCP-capable assistant, and maintainers who need to see tool-call and workflow progress locally.
- **Design rule:** MCP is an adapter to shared AIDLC application operations, not a CLI subprocess proxy and not a second workflow engine.
- **Repository baseline:** Rust crate with a library at `rust-cli/lib.rs` and CLI binary at `rust-cli/main.rs`; dependencies currently include `serde_json` and `sha2` (`Cargo.toml`). Command routing and some orchestration adaptation are still in the binary. The library exposes domain modules, including `orchestrate`, `status`, `audit`, `workflow`, `unit_commands`, and `work_units`.

---

## 1. Goals and non-goals

### Goals

1. Let an MCP-capable assistant invoke a small, schema-validated set of AIDLC operations through a local stdio process.
2. Keep the CLI as a complete, supported, independent entry point.
3. Make both entry points reuse the same workflow rules, validation, state transitions, audit behavior, and gate policy.
4. Provide local, privacy-conscious observability for MCP tool calls and explicitly instrumented shared-operation/workflow events.
5. Provide an optional read-only local dashboard for call health and workflow progress.
6. Keep the MCP protocol stream, AIDLC audit trail, and observability telemetry separate.
7. Preserve current project state formats and existing CLI behavior unless an explicit, separately reviewed migration is approved.

### Non-goals for the first release

- Replacing, removing, or requiring the CLI.
- Making the MCP server remotely reachable, exposing an HTTP MCP transport, or adding a cloud relay.
- Giving the assistant arbitrary shell, Git, or filesystem tools.
- Treating the MCP server or dashboard as an authorization authority.
- Treating telemetry as the audit log, or using telemetry to decide workflow state or approvals.
- Exporting telemetry to a hosted vendor or implementing general OpenTelemetry export in the initial release.
- Automatically editing Claude Code, DeepSeek, or other host configuration during onboarding. Start with documented manual configuration; host-specific setup can be a later feature.
- Exposing MCP resources, prompts, subscriptions, or sampling in the first cut. Begin with bounded tools only.

---

## 2. Architecture design

### 2.1 Logical architecture

```text
                         local machine / selected project root

  Assistant harness
       │ MCP JSON-RPC over stdio
       ▼
  dtech-aidlc-mcp ───────────────┐
  protocol adapter               │ calls shared application operations
                                 ▼
  dtech-aidlc CLI ───────────> Shared AIDLC application layer
  direct human/script path       │
                                 ├── reads project context and workflow state
                                 ├── validates operations and gate policy
                                 ├── writes existing project artifacts/state
                                 └── appends existing workflow audit events

  MCP adapter ── MCP spans ───────────────┐
  Shared operations ─ lifecycle events ───┴──> local telemetry event files
                                                   │ read-only
                                                   ▼
                                      optional local dashboard
                                      loopback HTTP only; separately started
```

### 2.2 Process and trust boundaries

- **Assistant harness:** Starts and supervises `dtech-aidlc-mcp`; communicates with it over stdin/stdout. The host/model's tool selection is not a trust boundary for project authorization.
- **MCP process:** Translates MCP tool requests into typed application calls and converts results/errors back to MCP content. It has only the filesystem access already needed by AIDLC within the selected project root. It is not a general-purpose shell.
- **Shared application layer:** Owns command semantics, path checks, project-state validation, concurrency checks, gate enforcement, existing audit writes, and telemetry hooks. It is the single behavioral authority for both CLI and MCP.
- **Project workspace:** Existing project documents and `.aidlc/` state remain authoritative. No new workflow state database is introduced.
- **Telemetry store:** A separate, local-only, bounded store of operational events. It is not `.aidlc/audit.jsonl`, and it cannot mutate project files or authorize operations.
- **Dashboard:** A separately invoked read-only process. It reads telemetry only; it does not call workflow mutation operations. Its HTTP server binds to loopback only, never `0.0.0.0`.

### 2.3 Request/data flow

1. The user configures their MCP host to start `dtech-aidlc-mcp` in the intended project directory (or supplies an explicit `--project` at process launch).
2. The host and MCP server negotiate the supported MCP protocol over stdio. Stdout is reserved for protocol frames; diagnostics go to stderr.
3. The host invokes a named tool. The adapter validates the input schema and bounds before constructing an application request.
4. The shared application layer resolves the selected project root, checks safe state paths and current state, and runs the same operation used by the CLI.
5. Reads return structured results. Mutations follow the existing validate → preflight audit → persistence/concurrency checks → audit sequence and configured gate rules.
6. The MCP adapter records MCP-specific request spans; the shared layer records operation and lifecycle events. Telemetry uses a dedicated local file sink, never MCP stdout.
7. The result returns to the harness as MCP content. Separately, the optional dashboard reads telemetry files and renders a local, read-only view.

### 2.4 Observability coverage

- MCP-level spans cover requests that pass through the MCP adapter only.
- Shared-operation events can cover calls from both MCP and CLI, provided instrumentation is placed inside the shared application layer.
- Direct legacy CLI paths that still bypass that layer will not appear in operation-level telemetry until they are routed through it.
- A CLI call never receives an MCP span merely because the MCP server exists.
- Existing audit events remain the durable record for workflow actions. They are not repurposed as tracing spans or metrics.
- Telemetry is best-effort and must never change the outcome of an AIDLC operation. A telemetry write failure may generate a stderr diagnostic but cannot fail a workflow command or corrupt stdout.

---

## 3. Detailed design

### 3.1 Application boundary and CLI refactor

Add a library-level application façade (proposed module: `rust-cli/application.rs`) with explicit request/result types for operations that both interfaces need. Do not invoke the CLI executable from MCP, and do not fake argv or capture CLI stdout as the MCP API.

Initial façade responsibilities:

- Resolve a `ProjectContext` once per process from the startup working directory or explicit `--project` argument.
- Load runtime graph/configuration with the same embedded/external engine behavior as the CLI.
- Expose `status`, `orchestrate_next`, `orchestrate_report`, and `orchestrate_queue` operations through typed wrappers over the existing library behavior.
- Reuse `orchestrate::orchestrate_next`, `orchestrate::orchestrate_report_with_engine`, and the existing unit queue handler/domain logic; move only adapter/router code needed to make these callable from both binaries.
- Keep the CLI's existing parser, text/JSON presentation, stdout/stderr behavior, and exit codes stable. Its `main.rs` becomes a thin CLI adapter that parses arguments, invokes the façade, and renders results.
- Keep MCP-specific JSON mapping and error formatting out of business/domain modules.

Refactor incrementally. Do not attempt to move every command handler before an MCP vertical slice works. Keep unrelated command dispatch in the CLI until a concrete shared use case requires moving it.

### 3.2 Binaries, launch, and project selection

- Keep the current binary name `dtech-aidlc`.
- Add a second Cargo binary, proposed name `dtech-aidlc-mcp`, with an explicit Cargo target in `Cargo.toml` and a source entry such as `rust-cli/mcp_main.rs`.
- Install both binaries through the existing Cargo installation path; verify platform installers/distribution docs if they package one binary explicitly.
- MCP process arguments: `--project <path>` (optional; defaults to startup current directory), `--telemetry on|off` (default on for local metadata-only events, with a documented opt-out), and `--help`/`--version`.
- Project root is chosen at process launch, not supplied independently on each tool call. Do not accept arbitrary root paths as tool arguments.
- The MCP process is foreground and host-managed. It is not a background daemon and does not open a network port.
- Initial host configuration is documented, with copy-ready examples for supported harnesses, but the server does not write user-level assistant settings.

### 3.3 MCP tool contract

First release exposes a deliberately small set of tools. Use strict JSON schemas, reject unknown fields, cap string/list sizes, and validate enum values before touching the project.

| Tool | Access | Inputs / behavior | Side effects |
|---|---|---|---|
| `aidlc_status` | Read | Optional intent selector if the existing status operation supports it; otherwise report active intent/status. Return structured status. | None beyond telemetry. |
| `aidlc_queue` | Read | Return ready, blocked, parked, and waiting work-unit information using the existing queue logic. | None beyond telemetry. |
| `aidlc_next` | Workflow mutation | Select and return the next directive using existing `orchestrate next` rules. Clearly document that selection can persist state/audit information. | Existing state/audit effects; telemetry. |
| `aidlc_report` | Workflow mutation | Inputs map to supported `orchestrate report` fields: stage, result, detail, needs, artifact/evidence fields, and gate-decision fields only where existing rules allow them. | Existing validation, state writes, audit, and configured gate behavior. |

Tool response rules:

- Return JSON-serializable structured data with a concise human-readable text summary where useful.
- Include `complete`, `blocked`, stage/intent identifiers, relevant artifact paths, and actionable validation errors where those are already part of the operation result.
- Do not claim an operation succeeded if the application layer rejected it or persistence failed.
- Return expected domain/validation errors as tool errors with bounded safe text. Do not expose Rust backtraces, environment secrets, raw project file content, or full filesystem paths unnecessarily.
- MCP protocol framing and server diagnostics are separate: protocol messages on stdout only, diagnostics on stderr only.

#### Gate/approval semantics

- Do not add a privileged `aidlc_approve_gate` tool that treats model-supplied identity as authentication.
- Keep the existing gate policy and report validation as the source of truth. The current project explicitly treats approver names as caller-supplied/self-asserted entries, not authenticated signatures; MCP must not imply otherwise.
- `aidlc_report` may submit gate fields only in the same conditions the CLI accepts. It must preserve `gate-mode`, allowed-approver, AI recommendation, and open-item checks.
- The harness should request explicit human confirmation before submitting a human-mode approval when its UI supports it. The server cannot assume every host provides that UI; document this limitation and do not describe MCP as cryptographic or identity-backed authorization.
- Any stronger authenticated approval flow is a separate architecture decision and must not be smuggled into this adapter change.

### 3.4 Telemetry model and persistence

Use a versioned, local JSON Lines event format separate from `.aidlc/audit.jsonl`.

Proposed event envelope:

```json
{
  "schemaVersion": 1,
  "eventId": "...",
  "timestamp": "...",
  "processId": 1234,
  "sessionId": "...",
  "traceId": "...",
  "spanId": "...",
  "parentSpanId": null,
  "component": "mcp|application|cli",
  "event": "tool.completed|operation.completed|stage.selected|gate.decided",
  "name": "aidlc_next",
  "durationMs": 42,
  "outcome": "ok|error|blocked",
  "errorCode": null
}
```

Fields are illustrative; finalize one schema before implementation and keep it allowlisted. Never store prompt text, tool argument `detail`, artifact contents, full tool results, credentials, environment variables, or unredacted absolute project paths.

Storage decisions for MVP:

- Store under `.aidlc/telemetry/`, which must be covered by the managed `.gitignore` block through the existing ignore-file maintenance logic and its tests.
- Prefer one event file per process/session, created exclusively with a collision-safe name, to avoid multiple CLI/MCP processes interleaving writes into one JSONL file.
- Use bounded file size and retention defaults (proposed starting defaults: 5 MiB per session file, retain at most 7 days and 50 MiB total). Pruning is best-effort and limited to recognized telemetry files under the validated telemetry directory.
- Do not modify/delete `.aidlc/audit.jsonl`; keep its existing symlink/path protections and append semantics intact.
- Protect telemetry directory and files from symlink/path traversal. Treat malformed or partially written telemetry lines as skippable dashboard records, not a reason to fail AIDLC operations.
- If telemetry is disabled or cannot be written, operations continue normally. Keep the opt-out explicit and observable in CLI/MCP diagnostics without leaking details to the model.
- Do not add an OTLP/network exporter in the first release. If a later opt-in exporter is proposed, it needs an explicit destination, redaction policy, transport/security review, and separate approval.

### 3.5 Instrumentation boundaries and event vocabulary

Introduce an optional telemetry sink/context passed from the interface adapters into shared operations. Avoid hidden global state where practical.

Minimum events:

- Process lifecycle: `process.started`, `process.stopped` (best effort).
- MCP request lifecycle: `mcp.request.started`, `mcp.tool.completed`, `mcp.protocol.error`.
- Shared operation lifecycle: `operation.started`, `operation.completed`, with duration and bounded outcome/error code.
- Workflow events emitted at existing transition points: `stage.selected`, `stage.reported`, `unit.parked`, `unit.revisited`, `gate.decided`, `intent.paused`, `intent.resumed`, `intent.abandoned` where practical.
- CLI events at the shared boundary carry `component: "cli"`; they do not pretend to be MCP tool calls.

Instrumentation must not duplicate or reinterpret audit events. Where an operation already emits a durable audit record, telemetry may reference the event type and a non-sensitive correlation id, but the audit log remains authoritative for the audited action.

### 3.6 MCP observability dashboard — product and interaction design

The dashboard is a first-class local product surface, not an unspecified chart page. It is an operator console for answering three questions quickly: **Is the MCP server healthy? What is happening in the AIDLC workflow? Where did a tool call fail or slow down?** It is explicitly read-only and is separate from both the assistant conversation and the durable AIDLC audit record.

#### Information architecture

Use one responsive page with four related views/regions, ordered by operational priority:

1. **Health strip:** telemetry status (recording/off/degraded), selected time range, last event time, active MCP process/session count when known, and a clear “coverage is partial” indicator if some events are missing or only MCP-level spans are available.
2. **Activity summary:** tool calls and outcomes, latency distribution (p50/p95, with sample count), error count/rate, and current/recent workflow-stage activity. Avoid vanity KPI tiles: each summary must link to or filter the underlying event rows.
3. **Live/recent event stream:** time-ordered event list with time, component/source (`MCP`, `shared operation`, `CLI`), tool/operation, stage/unit when present, duration, and outcome. Use text and icon/label as well as color for status.
4. **Selected event details:** a side panel on wide screens and inline disclosure on narrow screens. Show trace/session correlation, parent/child span relationship, event type, duration, outcome, safe error code/message, and permitted identifiers. Do not show prompt text, raw tool arguments, artifact contents, credentials, or unredacted paths.

#### Primary screen layout

Desktop: compact header and health strip, then a two-column workspace. The main column contains the activity summary and event stream; the secondary column contains filters and the selected-event detail. The timeline/list remains the central visual element. Mobile: stack health, summary, filters, and events; open event details inline so no side-scroll is required. Provide an “empty / no telemetry yet” view that explains how to launch MCP with telemetry enabled and how to run the monitor, without fabricating sample events as live records.

Initial annotated wireframe (content and hierarchy, not final styling):

```text
DESKTOP  ≥ 960 px
┌──────────────────────────────────────────────────────────────────────────┐
│ AIDLC MONITOR   Project: <safe label>   Local only   [15m ▾] [Refresh]   │
├──────────────────────────────────────────────────────────────────────────┤
│ ● COLLECTING   MCP: 1 session   Last event: 14:32:08   Coverage: PARTIAL │
├──────────────────────────────────────────────┬───────────────────────────┤
│ Activity · selected range                    │ Filters                   │
│ Calls 128 · errors 3/128 · blocked 4          │ source [all ▾]             │
│ p50 42 ms · p95 240 ms · n=124 completed     │ outcome [all ▾]            │
│ [latency distribution] [event volume timeline]│ tool/stage [search]        │
│                                              │ trace/session [________]   │
│ Events · newest first · 128 results          │ [Clear filters]            │
│ TIME       SOURCE OPERATION   DUR   OUTCOME  │                           │
│ 14:32:08   MCP    aidlc_next   42ms  OK       │ Selected event             │
│ 14:31:51   APP    stage.select 18ms  OK       │ trace / span / parent       │
│ 14:31:22   MCP    aidlc_report 86ms  BLOCKED  │ safe error + allowlisted    │
│ [older events…] [Load more]                  │ metadata only               │
└──────────────────────────────────────────────┴───────────────────────────┘
```

Mobile (≤ 680 px):

```text
┌─────────────────────────────┐
│ AIDLC MONITOR     [15m ▾]   │
│ Local only · ● collecting   │
│ MCP 1 · last 14:32 · partial│
├─────────────────────────────┤
│ Calls 128 · errors 3/128    │
│ p50 42 ms · p95 240 ms      │
│ [latency] [volume]          │
├─────────────────────────────┤
│ [Filters (2)] [Refresh]     │
│ 14:32 MCP aidlc_next 42ms OK│
│ 14:31 APP stage.select 18ms │
│ 14:31 MCP aidlc_report      │
│ BLOCKED · tap for details   │
├─────────────────────────────┤
│ Event detail (inline/open)  │
│ trace · span · safe error   │
└─────────────────────────────┘
```

Wireframe states: healthy data retains the above hierarchy; empty state replaces metrics/list with a concise explanation and launch instructions; degraded state keeps last-known events visible with a stale/degraded banner, skipped-record count and retry action; selected-event state opens the detail region without discarding filters or scroll position. These are design requirements, not claims that telemetry already exists.

#### Interaction model

- Time range selector with bounded presets (15 minutes, 1 hour, 24 hours, and all retained), displaying the actual start/end time and local timezone or UTC offset.
- Filters for source/component, tool or operation, outcome, stage, and session/trace id. Search only safe indexed metadata; never search prompt/body content. Provide visible sort order, matching event count, and a clear-filters action.
- Selecting a summary segment or error count filters the event stream to its underlying records. Selecting an event opens its detail panel. Trace navigation follows parent/child ids only when those records are present and within the selected range; report missing/expired parents honestly and provide a clear route back to the same filtered stream.
- Paginate event rows or virtualize a bounded window; never render the entire retained event corpus at once. The read API uses stable cursors tied to the selected query and time range. If the query changes, reset the cursor and selected event when it no longer matches.
- Define filter persistence explicitly: keep filters in page memory for the current monitor session; provide a visible reset. Do not put project paths, tokens, or sensitive values into URL query/hash state.
- A pause/resume control pauses only UI refresh, never collection. Show the last successful refresh time and a manual refresh action. Use a modest refresh interval with backoff on errors and pause polling when the page is hidden.
- Dashboard controls are local UI state only; they cannot call AIDLC mutations or alter approvals, project files, telemetry retention, or server settings.

#### Dashboard read model and event details

- Event-list row fields: timestamp (with timezone clarity), source (`mcp`, `application`, `cli`), event/tool/operation name, intent/stage/unit identifiers where allowed, duration with `ms` units, outcome (`ok`, `blocked`, `error`), and trace/session correlation id.
- Summary metrics show their time range and sample count. Latency percentiles are computed only for completed spans in the selected range, label p50/p95 and `ms`, and show “insufficient samples” instead of a misleading number below a documented minimum sample size. Error rate states its numerator and denominator.
- Detail panel includes event id, timestamps, component, event name, duration and units, outcome, safe error code/message, trace/span/parent ids, and allowlisted metadata. If a field was redacted, omitted, malformed, or truncated, say so without exposing the raw value.
- The API provides bounded time-window queries, allowlisted filters, deterministic order, cursor pagination, and stable errors (`invalid_filter`, `range_too_large`, `cursor_stale`, `store_unavailable`). It never accepts a path or filename from the browser and never returns raw JSONL lines.
- Retention and deletion disclosure states the configured maximum age/size and makes clear that expired records may leave gaps. No UI delete action in v1; retention is applied by the local process policy, not dashboard controls.

#### States and failure handling

Design and implement explicit states for: no telemetry directory yet, telemetry disabled, healthy data, partial/corrupt lines skipped, stale/no recent events, telemetry sink error, MCP process absent, query/file-size limit reached, and dashboard server shutting down. State messages say what the operator can do next. Preserve a last-known data view on refresh failures and mark it stale; never make a failed refresh look like a healthy empty system. A dashboard restart should not affect the MCP process or workflow.

#### Visual and accessibility design

- Use a restrained instrument-panel visual language tied to AIDLC operations: dense but legible event rows, clear typographic hierarchy, consistent time/duration alignment, and distinct semantic states for success, blocked, and error. Color is never the sole signal.
- Meet keyboard operation for filters, event selection, trace navigation, and refresh. Provide visible focus, semantic headings/landmarks, accessible labels, and screen-reader announcements for refresh/error status without announcing every incoming event noisily.
- Respect reduced motion; do not flash or animate incident state. Keep contrast readable in light and dark themes. Use responsive layouts and allow long identifiers/errors to wrap or scroll inside their own region rather than expanding the page.
- If visualizations are used, each must answer a concrete operator question. Keep a latency distribution and a compact time-series of event/outcome volume; link plotted values to filtered events. Always include a textual equivalent and exact counts/time range. Do not add decorative charts or imply statistical confidence for small samples.

#### Dashboard implementation and security boundary

- Add an optional `dtech-aidlc monitor --project <path>` command (name is provisional; settle after checking CLI naming and platform packaging).
- The monitor is a separate process from the stdio MCP server. It binds only to `127.0.0.1` on an ephemeral port by default, prints the URL, and does not open a browser automatically.
- The dashboard exposes read-only pages/API. No POST/PUT/PATCH/DELETE routes and no workflow tool invocation.
- Read only recognized event files below `.aidlc/telemetry/`; validate paths and cap file count, response size, event count, and query window. Do not traverse arbitrary user paths or follow symlinks.
- Serve inline/static bundled assets or embedded HTML; no remote fonts, analytics, or external JS for the initial local dashboard.
- Browser safety: loopback bind, strict Host/Origin validation, no permissive CORS, no mutating routes, and a per-run unguessable read token if the chosen browser/API design requires it. Do not put the token in telemetry or project files. Evaluate DNS-rebinding and local browser cross-origin access in the threat model.
- If the dashboard cannot start, report a clear local error; do not affect the MCP or CLI processes.

#### Dashboard detail-design component and state matrix

| Component | Healthy | Empty / disabled | Degraded / error | Interaction |
|---|---|---|---|---|
| Health strip | Recording state, last event, range, process/session count when known | Explain telemetry off or no first event | Stale/degraded label, last successful refresh, safe reason | Status details; never implies workflow approval/health from missing telemetry |
| Summary | Counts, p50/p95 with sample size, error numerator/denominator | “No data for range” without zero-as-healthy ambiguity | Preserve last-known summary and mark stale | Click metric to filter underlying rows |
| Filters | Current filters, result count, sort and range | Available but clearly returns no records | Keep filters; show failed query separately | Keyboard-operable, clear/reset, bounded presets |
| Event stream | Paginated newest-first rows | Instructions, no fabricated live rows | Show partial/corrupt-line count and stale marker | Select row, load more/cursor paging |
| Trace detail | Correlated parent/child links and safe metadata | Not shown until selection | Missing/expired parent is explicit | Open detail and return to same query/scroll |
| Refresh | Last-updated time and running state | Manual refresh can reveal first event | Retry with backoff; pause auto-refresh only | Pause is UI refresh only, not collection |
| Latency/time plots | Units, selected range, counts, text alternative | Empty-range caption | Suppress invalid metrics and explain why | Click bin/point to filter the stream |

#### Dashboard design deliverables

Before implementation, include a dashboard-specific design pack in this plan's execution artifacts (or a separate reviewed design doc):

1. Screen map and annotated desktop/mobile wireframes covering healthy, empty, degraded, and selected-event states (initial wireframes are above; review/refine them during detailed design).
2. Component inventory and interaction/state matrix (above) for health strip, filters, event stream, trace detail, latency view, and refresh behavior.
3. Visual tokens, typography, semantic status treatment, spacing, responsive breakpoints, and keyboard/accessibility behavior.
4. Field-level privacy map showing which telemetry fields may be displayed, redacted, or never collected.
5. Dashboard API/read-model contract, including event pagination, time-range limits, filter semantics, stale-data behavior, and stable error responses.
6. Acceptance review against the design deliverables before dashboard implementation begins; UI implementation tests verify the states and keyboard pathways.

### 3.7 Configuration and compatibility

- Default MCP and monitor behavior remains opt-in: installing the binary does not start a server or listener.
- Telemetry default should be decided before release. Proposed: on for local metadata-only telemetry, with `--telemetry off` and config override; document event fields and retention. If privacy review rejects that default, ship off by default and require opt-in.
- Do not change the phase graph, intent-state schema, gate rules, audit schema, or methodology outputs for the initial integration.
- Preserve CLI output, exit codes, command flags, and project write behavior with the existing golden test suite.
- Add telemetry directory ignore entries without weakening user `.gitignore` content; use the existing managed-block semantics.
- Version event schema independently (`schemaVersion`) and make the dashboard tolerant of unknown future event kinds/fields.

---

## 4. Implementation sequence

### Phase 0 — Architecture design and dependency spike

1. Produce and review an architecture decision record with system context, process/deployment view, trust boundaries, data flows, component responsibilities, telemetry-vs-audit distinction, threat model, and explicit non-goals.
2. Confirm local stdio transport and no remote MCP transport for v1.
3. Select a maintained Rust MCP SDK compatible with this crate's MSRV, async/runtime needs, offline build/test constraints, and transport framing. Record version/license and update `Cargo.lock`; do not hand-roll MCP protocol unless the SDK evaluation documents a concrete blocker.
4. Choose the first tool set and exact JSON schemas from existing CLI behavior.
5. Confirm telemetry default/retention and local dashboard security approach.
6. Record ADRs for approval identity limitation, and for the fact that audit and telemetry are distinct.
7. Create an architecture diagram showing the harness, MCP adapter, CLI, shared application layer, project data, audit, telemetry, and dashboard paths.

**Exit:** architecture review approves component boundaries, trust and data flows; dependency builds with `--locked`; minimal stdio handshake works in an isolated test; no existing CLI or project state changes.

### Phase 0b — Detailed design

1. Write detailed design for application façade APIs, MCP lifecycle/tool schemas, error mapping, project-root resolution, write preconditions, concurrency and shutdown behavior.
2. Finalize telemetry event schema, allowlist/redaction rules, file layout, retention/rotation and failure semantics.
3. Complete the dashboard design pack specified in §3.6: desktop/mobile annotated wireframes; healthy, empty, degraded and selected-event states; component and interaction/state matrix; tokens and responsive/accessibility rules; field-level privacy map; and the read-model/API contract.
4. Review dashboard loopback/browser-security choices and the phase-approval interaction against supported hosts.
5. Map each detail-design decision to tests and the later implementation phase.
6. Produce an architecture design and detailed design artifact in `plans/` (or maintain this plan's sections as those design records), with explicit review/approval before implementation.

**Exit:** detail design is approved before feature implementation; unresolved decisions are named and cannot silently become implementation assumptions.

### Phase 1 — Shared application façade

1. Extract the runtime graph/project context resolution currently owned by CLI routing into reusable library-level functions.
2. Move only the orchestration/status/queue operations required by the initial tools behind `application.rs` façade calls.
3. Refactor the existing CLI to call the same façade, preserving existing presentation and exit codes.
4. Add façade unit tests and reuse existing orchestration tests.

**Exit:** all existing tests/goldens pass unchanged; CLI still runs without MCP dependency behavior being activated.

### Phase 2 — MCP stdio vertical slice

1. Add `dtech-aidlc-mcp` binary and server lifecycle.
2. Implement initialize/capability negotiation and tool discovery/call through the selected SDK.
3. Implement `aidlc_status`, then `aidlc_queue`, then `aidlc_next`, then `aidlc_report`.
4. Validate inputs before any filesystem access; preserve project-root selection, safe path, concurrency, audit, and gate rules.
5. Add copy-ready manual host configuration documentation and process troubleshooting.

**Exit:** integration tests drive a real child process over stdin/stdout; stdout contains protocol frames only; invalid requests and process errors cannot write project state.

### Phase 3 — Telemetry foundation

1. Define the event schema and safe-field allowlist.
2. Implement per-process/session JSONL writer, exclusive file creation, size limits, retention, and graceful disable/failure behavior.
3. Add MCP spans and shared-operation lifecycle instrumentation, then add selected workflow transition events.
4. Update managed `.gitignore` behavior and tests for `.aidlc/telemetry/`.
5. Add privacy tests asserting prompts, details, artifact contents, environment values, and absolute paths are absent.

**Exit:** telemetry is distinct from audit; failed telemetry writes do not change command outcomes; concurrent CLI/MCP sessions create readable independent event files.

### Phase 4 — Read-only monitor dashboard

1. Add monitor command and loopback-only server.
2. Implement bounded event-file reader and read-only summary/event endpoints.
3. Build a locally bundled dashboard for tool activity, latency, errors, event coverage, and workflow timeline.
4. Add Host/Origin validation, no-CORS policy, response limits, and tests for path traversal/symlink rejection.
5. Document starting/stopping monitor and its local security boundary.

**Exit:** dashboard can display real sample events and live local events without access to workflow mutation routes or project artifact bodies.

### Phase 5 — Hardening, docs, and release

1. Run full test suite on Windows and POSIX; exercise Cargo install of both binaries.
2. Run protocol conformance and host-compatibility tests for supported clients.
3. Perform threat-model review for filesystem path handling, approval semantics, stdio leakage, dashboard loopback exposure, telemetry privacy, and retention.
4. Add README installation/usage/configuration sections, MCP tool reference, telemetry schema/retention, dashboard usage, known coverage limits, and troubleshooting.
5. Add changelog entry; cut a release only after acceptance criteria pass.

**Exit:** release notes state MCP, telemetry, and dashboard are local optional capabilities; user-facing docs accurately distinguish self-asserted approval names from authenticated identity.

---

## 5. Test and verification plan

### Unit tests

- Application façade maps requests to existing operations and preserves result/error semantics.
- Tool JSON schemas reject missing required fields, unknown fields, invalid enum values, oversized strings/lists, and malformed structured input.
- Gate fields are refused outside valid phase-gate states and existing human/AI gate rules remain intact.
- Telemetry schema serialization is stable and redaction allowlist rejects sensitive values.
- Telemetry rotation/retention handles malformed files, limits, and timestamps safely.
- Dashboard query/parser tolerates truncated final JSONL records and unknown event types.

### Integration tests

- Spawn MCP child process; exchange initialize, tool list, valid tool call, invalid tool call, and shutdown messages over stdio.
- Assert stdout has only protocol output; diagnostics appear only on stderr.
- Compare CLI and MCP calls on equivalent isolated fixtures for status, queue, next, and report semantic results and resulting project state.
- Verify every rejected mutation leaves state/audit unchanged, except permitted telemetry diagnostics.
- Verify successful mutation writes the same existing state/audit structures as CLI.
- Verify an explicit approval value follows current policy and self-asserted identity limitations are documented; no hidden assistant identity is treated as authentication.
- Verify CLI remains functional if telemetry is off, corrupt, full, or unwritable.
- Verify MCP shutdown/restart does not lose or migrate workflow state.
- Verify concurrent CLI and MCP runs do not corrupt intent state (existing optimistic concurrency checks) or interleave telemetry records.
- Verify dashboard binds to loopback only; rejects foreign Host/Origin, traversal, symlink files, large ranges, and mutation methods; cannot alter project state.

### Regression and release checks

- Run `cargo fmt --check`, `cargo test --locked`, and the repository's documented lint/validation commands.
- Run existing routing/golden tests without updating snapshots unless a separately reviewed intentional behavior change requires it.
- Run on Windows and at least one POSIX CI environment.
- Validate install/update/uninstall packaging for both binaries and verify the MCP server launches from an installed binary without a repository checkout.

---

## 6. Acceptance criteria

1. A supported MCP host can launch the local server and discover/call the initial tools.
2. MCP calls and CLI calls use shared application operations; no MCP subprocess-shell proxy or duplicate workflow engine exists.
3. Existing CLI tests/goldens and documented behavior remain intact.
4. MCP stdout is protocol-only, and the server opens no network listener.
5. State-changing tools preserve existing validation, path safety, concurrency, audit, and gate behavior.
6. Approval names remain explicitly self-asserted under the existing contract; the design does not claim identity authentication or introduce an assistant-only approval bypass.
7. Telemetry has versioned, bounded, privacy-minimized local events and is separate from `.aidlc/audit.jsonl`.
8. Telemetry failures never fail or alter workflow operations.
9. Dashboard is optional, read-only, local-only, and reports coverage limitations accurately.
10. Dashboard design is reviewed before implementation and covers desktop/mobile layouts, healthy/empty/degraded/selected-event states, filters, event details, metric semantics, privacy, accessibility, and bounded read APIs.
11. Telemetry directories are ignored by Git through safe managed `.gitignore` handling.
12. Both binaries install and run on supported Windows/POSIX platforms.
13. MCP server, telemetry, and dashboard are labeled proposed until each phase is implemented and released.

---

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| CLI and MCP behavior diverge | Put shared command semantics behind library façade and test semantic parity on equivalent fixtures. |
| MCP tool call bypasses gate or audit | Reuse existing orchestration operations; prohibit direct state-file writes in adapter; mutation integration tests. |
| Assistant fabricates approver name | Preserve self-asserted caveat, require explicit gate fields, host-side human confirmation where supported; do not claim authentication. |
| Stdio diagnostics corrupt MCP protocol | Reserve stdout exclusively for SDK protocol writes; send logs to stderr; assert in child-process tests. |
| Telemetry leaks prompt/artifact data | Allowlist event fields; never serialize raw arguments/results; privacy regression tests. |
| Dashboard becomes an attack surface | Separate optional process, loopback-only bind, strict host/origin policy, no CORS, read-only endpoints, bounded input/output. |
| MCP dependency conflicts with offline builds/MSRV | Complete dependency spike before implementation; lock versions; verify `--locked` and offline build requirements. |
| Local events mistaken for durable audit | Separate directories, schemas, naming, documentation, and dashboard labels; audit log remains authoritative. |
| Instrumentation coverage appears complete when it is not | Distinguish MCP spans from shared operation events; show gaps and bypassed legacy routes. |
| Telemetry file count/storage grows unbounded | Per-file and total retention caps; safe best-effort pruning; monitor enforces query bounds. |

---

## 8. Decisions to confirm before coding

These are implementation gates, not reasons to defer the architecture:

1. **Telemetry default:** enable local metadata-only telemetry by default with an opt-out, or disabled until opted in. Proposed default is enabled with a clear opt-out; privacy review may choose the stricter disabled default.
2. **Dashboard command name:** `dtech-aidlc monitor` is proposed; confirm it does not conflict with the intended CLI vocabulary.
3. **MCP SDK/dependency policy:** choose the Rust SDK only after verifying maintenance, license, MSRV, protocol version support, and locked/offline builds.
4. **Human approval UX:** determine which supported MCP hosts can present a true user confirmation interaction. Where unavailable, clearly fall back to the existing self-asserted CLI approval contract rather than imply authenticated approval.
5. **Dashboard authentication:** finalize whether loopback + Host/Origin restrictions suffice or a per-run token is required by the implementation and supported browsers.

No phase-graph or intent-state schema change is proposed for the initial release.
