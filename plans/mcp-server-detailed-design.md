# dtech-aidlc MCP Server — Detailed Design

- **Status:** Proposed; this is an implementation-level design, not a description of shipped features.
- **Date:** 2026-11-27
- **Related documents:** [Architecture design](mcp-server-architecture.md) · [Implementation plan](mcp-server-observability-implementation-plan.md)
- **Source of truth:** Current shared Rust operations and existing project data formats remain authoritative.

---

## 1. Scope and design principles

This document specifies the first implementation slice for the optional MCP server, shared operation façade, local telemetry, and local dashboard.

Principles:

1. MCP adapts protocol calls to application APIs. It does not execute shell commands or proxy CLI argv/stdout.
2. CLI and MCP are peers over the same application operations.
3. Domain rules, safe-path behavior, concurrency checks, gate policy, persistence, and audit remain in existing code paths.
4. Read and mutation capabilities are explicit in tool contracts and UI.
5. Telemetry is metadata-only, best-effort, bounded, and separate from audit.
6. The dashboard is a read-only observability surface, not a workflow control surface.
7. No implicit identity guarantee: the existing `approved-by` value is caller supplied and is not an authenticated signature.

## 2. Proposed crate/module layout

```text
Cargo.toml
  [[bin]] dtech-aidlc       -> rust-cli/main.rs       (existing; add `monitor` subcommand)
  [[bin]] dtech-aidlc-mcp   -> rust-cli/mcp_main.rs   (proposed)

rust-cli/
  application.rs            shared request context and operation façade (new)
  telemetry.rs              event schema, sink, rotation, redaction (new)
  mcp/
    mod.rs                  server setup and protocol adapter (new)
    tools.rs                schemas and tool dispatch (new)
    errors.rs               domain-to-MCP-safe error mapping (new)
  monitor/
    mod.rs                  internal monitor-child mode and local read-only server (new)
    store.rs                bounded telemetry reader/query/pagination (new)
    ui.html                 bundled dashboard shell (new)
  main.rs                   existing CLI adapter; hidden monitor-child mode self-launches via current executable
```

The module names are proposed. Keep the package at two public binaries: `dtech-aidlc` and `dtech-aidlc-mcp`. The monitor subcommand launches the currently installed `dtech-aidlc` executable itself in a hidden internal monitor-child mode, resolving via the current executable path rather than `PATH` or the repository checkout. There is no third installable binary or sidecar requirement. Test the installed launcher from outside the checkout and verify child shutdown. Avoid an unnecessary web framework or runtime unless required by the selected MCP SDK and monitor implementation. The repository does not currently declare `rust-version`; decide and document an MSRV as part of dependency review. Validate dependency maintenance, licensing, transport support, Windows support, lockfile reproducibility, and whether the selected dependency set can build with `--locked`; treat offline-cache availability as a separate deployment constraint, not an existing guarantee.

## 3. Shared application façade

### 3.1 Context

`ProjectContext` contains immutable process configuration only:

```rust
pub struct ProjectContext {
    pub project_root: PathBuf,       // canonicalized/normalized absolute root
    pub graph: scope::Graph,
    pub engine: orchestrate::Engine,
    pub telemetry: Option<TelemetryHandle>,
    pub caller: CallerKind,          // Cli | Mcp
}
```

Do not cache policy-relevant `.aidlc/config.json` in this long-lived context. Reload and validate it using the same operation-specific behavior as the CLI on every call, especially immediately before gate checks, so a running MCP process cannot keep applying stale gate modes or approver allowlists. Do not expose an arbitrary path argument on each MCP tool. Resolve once from startup CWD or explicit `--project`; use the existing `orchestrate::resolve_like_node` and `audit::assert_safe_state_dir` behavior. Reproduce the CLI's embedded/external engine handling using `DTECH_AIDLC_HOME` exactly; do not silently select an alternate engine.

The graph currently loads as `scope::Graph` in the CLI and handlers generally consume `graph.value()`. Keep graph parsing and config validation consistent rather than loading separate MCP-specific copies.

### 3.2 Operations

The façade should use explicit request/result types and call existing functions. Illustrative interface:

```rust
pub enum CallerKind { Cli, Mcp }

pub fn status(ctx: &ProjectContext, intent: Option<&str>) -> Result<Value, AppError>;
pub fn queue(ctx: &ProjectContext) -> Result<Value, AppError>;
pub fn next(ctx: &ProjectContext, request: NextRequest) -> Result<Value, AppError>;
pub fn report(ctx: &ProjectContext, request: ReportRequest) -> Result<Value, AppError>;
```

Mapping to current behavior:

- `status`: `status::status(root, graph, intent)`; the MCP projection must omit or relativize `intentDir` because the existing CLI payload contains an absolute path.
- `queue`: existing `unit_commands::run("orchestrate queue", root, graph, config, options)`; preserve and expose the non-persisted `migration: "pending"` preview marker on legacy state.
- `next`: preserve the CLI's knowledge preflight, fresh `validate_model_config`, `orchestrate::orchestrate_next(root, graph.value(), config, engine, options)`, and `orchestrate::finish_directive` for directive outcomes. Convert `NextOutcome::value()` without dropping blocked/busy/completion fields, but redact/relativize every absolute path in the structured directive before returning it through MCP.
- `report`: preserve the CLI's freshly loaded `load_config_unvalidated`, `orchestrate::orchestrate_report_with_engine(root, graph.value(), config, engine, options)`, and `orchestrate::finish_report` sequence. Return the same semantic result as the CLI after post-processing.
- The shared façade is not behaviorally complete until it reproduces these route-specific preflights/finalizers and preserves their ordering. Test knowledge-writeback completion and directive projection/writeback cases, not only the core orchestration return values.

Current CLI route setup in `main.rs` constructs these inputs in `run_read_only`, `run_orchestrate`, and `run_unit_queue`. Move operation setup/dispatch to the façade incrementally; preserve CLI-only help, parsing, formatting and process exit behavior. The MCP adapter must never synthesize argv, invoke the binary, or parse presentation output.

`AppError` should preserve a safe stable `code`, a user-actionable message, and an internal source for stderr diagnostics. Do not return backtraces, raw environment values, secrets, or unnecessary absolute paths to MCP clients.

## 4. MCP process and protocol

### 4.1 Binary lifecycle

- Binary: `dtech-aidlc-mcp`.
- Foreground child process launched by MCP host; process lifetime is host-controlled.
- Transport: stdio only for v1. No TCP/HTTP listener in this process.
- CLI arguments: `--project <path>`; telemetry toggle (final flag/config mechanism to decide); `--help`, `--version`.
- Stdout is exclusively MCP framing. All diagnostics, startup errors, and telemetry warnings go to stderr.
- Startup errors that prevent protocol initialization are emitted to stderr and exit non-zero; no partial workflow state is written.
- Graceful shutdown flushes best-effort telemetry and closes handles without changing workflow state.
- Configuration such as project root and transport is process-scoped, but gate mode/approver allowlist and other policy-relevant config are reloaded with the same operation-specific semantics as the CLI on each call, immediately before decisions. Do not retain a long-lived stale policy snapshot.

### 4.2 Protocol dependency

Use a maintained Rust MCP SDK only after the architecture spike validates its protocol support, stdio behavior, license, selected MSRV, async runtime footprint, Windows support, and `--locked` build reproducibility. Pin in `Cargo.lock`; validate offline-cache availability separately if offline installation is a requirement. Do not implement JSON-RPC framing by hand unless an ADR documents the SDK blocker and a conformance plan.

### 4.3 Tool set and schemas

Initial tools are intentionally narrow:

#### `aidlc_status` (read)

Input: optional `intent_id` only if the existing status operation supports selecting it. Otherwise use empty object with `additionalProperties: false`.

Output: structured status data using the existing semantic projection. Explicitly omit or relativize the `intentDir` field returned by `status::status`; apply the same path-redaction policy to every nested field in every MCP result, including directive paths from `aidlc_next`. Do not assume that an instruction to avoid paths removes them from the existing serialized value.

#### `aidlc_queue` (read; may be a migration preview)

Input: empty object, `additionalProperties: false`.

Output: existing queue representation of ready/waiting/blocked/parked/done units. On legacy state, the current queue path may synthesize an in-memory migration preview marked `migration: "pending"`; it must not be presented as persisted state. Preserve and surface the preview marker clearly in MCP structured output and any UI that consumes it. Assert that queue reads write no project files.

#### `aidlc_next` (state-affecting; guarded; no implicit prerequisite execution)

Input: begin with no assistant-supplied gate identity or arbitrary root. The current `orchestrate_next` behavior can create a new intent when no active intent exists, even when no explicit intent text is supplied, and can run owner-configured prerequisite checks before returning a directive. Merely omitting an MCP `intent` field does not constrain those effects.

Before exposing this tool, add and test an enforceable application-level mode/guard that:

1. requires an existing active intent and refuses without creating or selecting a new intent when none exists; and
2. makes prerequisite execution explicit. The safer initial MCP contract is not to execute configured shell prerequisite checks implicitly. If prerequisites are needed, return their configured names/status without running their commands, or require a separately reviewed explicit opt-in that runs only owner-configured checks using the existing timeout/argument-safety behavior.

Output: full directive/complete/blocked/busy structure, including any non-executed prerequisite requirements. Document that selecting a directive may persist current stage, perform existing migration, and append existing audit events. Test no-active-intent, active-intent, blocked prerequisite, and explicit prerequisite opt-in/refusal cases before publishing this tool.

#### `aidlc_report` (state mutation)

Inputs map only to current `ReportOptions`: `stage`, `result`, `detail`, `approval`, `approved_by`, `approval_note`, `needs`, and `unit`, subject to the exact existing conditions. Strict schema; bounded detail/list/string sizes; `result` enum from current CLI contract. Do not add fields the domain operation cannot enforce. Completion artifacts/evidence are derived and verified by existing application logic; the current `ReportOptions` does not accept caller-supplied artifact/evidence fields.

Output: the existing structured report result or safe error.

### 4.4 Gate and approval handling

- Do not create a special tool that bypasses `orchestrate_report` or directly writes gate state.
- Preserve human/AI mode behavior, configured gate mode, allowed-approver checks, AI recommendation requirements and open-item checks.
- **MCP must not submit `approval: "approved"` for a human-mode gate in v1.** The current report path accepts a caller-supplied approver value and does not receive authenticated human-consent evidence. A host UI confirmation that is not conveyed and independently verified by the server does not close this gap.
- For human-mode gates, MCP may submit a report without approval where current rules permit; the project owner completes the approval through the existing direct CLI flow. If a supported host later provides verifiable, server-checkable consent, enable MCP human approvals only through a separately reviewed authorization design.
- AI-mode approvals may be submitted only through existing AI-orchestrator policy and recommendation/open-item validation. The `AI Orchestrator` name is a policy value, not proof of a person.
- Approver resolution in the CLI falls back from an explicit label to Git `user.name`, then OS username, then `unknown`; none is authenticated. An empty configured allowlist does not authenticate or restrict names. The MCP contract must not claim these names are caller-verified or authenticated.
- Neither MCP nor the dashboard may present a caller-supplied/resolved name as authenticated identity or proof of human approval.

## 5. Error handling and idempotency

- Separate protocol/transport errors, schema validation errors, domain validation errors, project I/O errors, concurrency conflicts, and telemetry errors.
- Domain and validation failures return safe, actionable MCP tool errors and do not write project state.
- Existing optimistic concurrency failures remain visible; do not automatically retry a state-changing tool because a blind retry could change which stage is selected.
- Do not promise exactly-once tool invocation. Document that hosts may retry after transport loss; results and operation IDs should be used where available. The existing operation semantics remain authoritative.
- Telemetry error is logged to stderr and counted in process health when possible, but does not change operation success/failure.

## 6. Telemetry detailed design

### 6.1 Storage and lifecycle

Proposed storage: `.aidlc/telemetry/`, created only after safe directory validation. Add it through the existing managed `.gitignore` block without overwriting user text. Store one JSONL file per process/session with exclusive creation to avoid concurrent append interleaving. Use restrictive file permissions on POSIX consistent with the audit writer's approach; verify Windows behavior.

Proposed limits (confirm in privacy/operations review): 5 MiB per file, seven days maximum age, 50 MiB aggregate maximum. Cleanup only recognized telemetry files beneath the validated directory; never recurse elsewhere or follow symlinks. A malformed trailing line is skipped by the dashboard. Telemetry off/unwritable/full means event loss with a diagnostic, not workflow failure.

### 6.2 Event schema

Version the schema independently from AIDLC state:

```json
{
  "schemaVersion": 1,
  "eventId": "opaque-id",
  "timestamp": "RFC3339 UTC",
  "component": "mcp|application|cli",
  "caller": "mcp|cli; omitted if unknown",
  "processId": 1234,
  "sessionId": "opaque-process-session-id",
  "traceId": "opaque-correlation-id",
  "spanId": "opaque-span-id",
  "parentSpanId": null,
  "event": "mcp.request.started|mcp.tool.completed|mcp.protocol.error|operation.started|operation.completed|workflow.stage_selected|workflow.stage_reported|workflow.unit_parked|workflow.unit_revisited|workflow.gate_decided",
  "name": "aidlc_next",
  "intentId": "optional validated id",
  "stageId": "optional graph id",
  "unitId": "optional validated id",
  "durationMs": 42,
  "outcome": "ok|blocked|error",
  "errorCode": null
}
```

The final schema must be an explicit allowlist. Do not serialize arbitrary `Value` arguments/results or error source strings. Exclude prompts, `detail`, artifact content, registered doc content, credentials, env vars, raw filesystem paths, and free-form user text. If a safe error message is needed in the dashboard, map from stable error codes to fixed local copy rather than recording unbounded error text.

### 6.3 Event names, producer ownership, and correlation

These are proposed telemetry events, not a promise that existing audit rows automatically become events. Instrument each event explicitly and test its producer. Emit no event by scraping `.aidlc/audit.jsonl`.

| Event | Producer/hook | Coverage and meaning |
|---|---|---|
| `mcp.request.started` / `mcp.tool.completed` | MCP transport adapter | One start and one terminal completion per tool invocation when the process survives; includes outcome `ok`, `blocked`, or `error`. Missing completion after crash is unknown/incomplete, never success. |
| `operation.started` / `operation.completed` | Shared application façade around each public operation | One operation pair for calls that enter the façade. It does not imply every inner stage transition was emitted. |
| `workflow.stage_selected` | Explicit instrumentation after successful `orchestrate_next` selection/finalization | Indicates a directive was selected, not that work started or completed. |
| `workflow.stage_reported` | Explicit instrumentation after successful report finalization | Indicates report operation returned accepted; does not establish truth of reported evidence. |
| `workflow.unit_parked` / `workflow.unit_revisited` | Explicit instrumentation at successful queue mutation result | Only if those operations are exposed through the façade/MCP scope. |
| `workflow.gate_decided` | Explicit instrumentation after the existing report path has persisted state and audit successfully | Descriptive event only; not proof of approver identity, consent, or audit-log integrity. Human-mode approval is not accepted through MCP v1. |
| `mcp.protocol.error` | MCP transport adapter | Bounded protocol/dispatch failures only; never include raw protocol frames or tool arguments. |
| `workflow.intent_paused` / `workflow.intent_resumed` / `workflow.intent_abandoned` | Only if those intent operations are later moved into the shared façade and explicitly instrumented | Not covered merely because current CLI commands exist. Exclude from v1 event claims until wired and tested. |

For every event, set `component` to the emitting boundary (`mcp`, `application`, or `cli`) and `caller` to `mcp` or `cli` when an originating interface is known. The monitor emits no telemetry events in v1; it is a read-only consumer. MCP request spans use `component: mcp`; façade operations use `component: application` and preserve caller provenance; CLI-only adapter events use `component: cli`. MCP request spans cover MCP only. Shared-operation events cover only invocations that pass through instrumented façade hooks. Legacy CLI routes bypassing the façade remain invisible to these events. Dashboard labels coverage as observed event types and source; never claim complete workflow/audit coverage from a generic “shared events available” flag.

Propagate a generated `traceId`/parent span through one MCP call into the application operation. For direct CLI calls, generate a trace root with no MCP parent. Event IDs are deduplication keys only, and trace/session IDs are operational metadata, not authentication credentials.

## 7. Dashboard detailed design

### 7.1 Product purpose and users

A local operator/maintainer uses the dashboard to answer:

1. What telemetry events have been observed, and how fresh is the last observation?
2. Which tools/operations are slow, blocked, or failing in the selected range?
3. What event sequence led to a specific failure or stage transition?

The dashboard does not claim to establish collector or MCP-process liveness from the telemetry event stream. It may report a trusted launch configuration, but process presence requires a separate bounded heartbeat/status mechanism.

The dashboard is a separately started, read-only local app. It cannot call MCP tools, mutate workflow state, approve gates, replay reports, change telemetry settings, or delete logs.

### 7.2 Navigation and screen layout

**Privacy notice:** The dashboard header/help disclosure names the effective telemetry setting (or “unknown”), storage location (`.aidlc/telemetry/`), retention limits, and fields that may be retained (tool/operation names, timestamps, outcomes, durations, and selected intent/stage/unit/session/trace identifiers). It states that prompts, raw arguments/results, artifacts, credentials, and full project paths are excluded. The dashboard is read-only; the user changes collection settings through documented local configuration/launch options, not from dashboard controls.

One responsive screen for v1, with a header and four regions:

1. **Header:** “AIDLC Monitor”, generic “local project” label, local-only indicator, effective collection/retention disclosure, bounded time-range picker, refresh status.
2. **Health strip:** last observed event, selected interval, and event coverage (`MCP spans`, `shared operation events`, `partial`, `stale`, `unknown`). Do not infer that a process is live, stopped, or telemetry is disabled from silence in the same best-effort event stream. Do not show active-session counts unless backed by a separately specified, bounded-lifecycle heartbeat/status signal; otherwise show sessions observed within the selected range.
3. **Activity summary:** count MCP tool-completion spans as tool calls, count shared operations separately, report tool errors/blocked results only from MCP tool-completion spans, and calculate latency percentiles only from completed MCP tool spans. Never add operation spans to tool-call denominators or latency distributions, because one request may produce multiple operations. Show shared operation event counts in a separately labeled measure. Each metric filters its own event population. Below a defined minimum sample count show “insufficient samples”.
4. **Event workspace:** central newest-first event list with filters; selected event details in a right panel on desktop and inline disclosure on mobile.

Desktop wireframe:

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ AIDLC MONITOR  local project  LOCAL ONLY  [15m ▾]  [Refresh]             │
├──────────────────────────────────────────────────────────────────────────┤
│ last MCP event 14:32:08 · 1 session observed in range · coverage partial│
├───────────────────────────────────────────────┬──────────────────────────┤
│ MCP tools 128 · errors 3/128 · blocked 4      │ FILTERS                  │
│ tool p50 42 ms · p95 240 ms · n=124           │ source / outcome          │
│ [latency distribution] [events by time]        │ tool / stage / trace      │
│                                               │ [clear filters]           │
│ EVENTS · newest first · 128 matches           ├──────────────────────────┤
│ time       source  operation     duration status│ SELECTED EVENT           │
│ 14:32:08   MCP     aidlc_next    42 ms    OK    │ trace/span/parent         │
│ 14:31:51   APP     workflow.stage_selected 18 ms  OK │ safe code + safe fields │
│ 14:31:22   MCP     aidlc_report  86 ms BLOCKED │ return to filtered list   │
│ [Load more]                                    │                          │
└───────────────────────────────────────────────┴──────────────────────────┘
```

Mobile wireframe (no horizontal page scroll):

```text
┌────────────────────────────┐
│ AIDLC MONITOR   [15m ▾]   │
│ LOCAL ONLY · status unknown│
│ last MCP event observed 14:32│
├────────────────────────────┤
│ MCP tools 128 · errors 3/128  │
│ tool p50 42 ms · p95 240 ms  │
│ [latency] [event volume]  │
├────────────────────────────┤
│ [Filters (2)] [Refresh]   │
│ 14:32 MCP aidlc_next 42ms│
│ 14:31 APP workflow.stage_selected │
│ 14:31 MCP aidlc_report   │
│ BLOCKED · tap for details│
├────────────────────────────┤
│ Event detail (inline)     │
│ trace/span · safe fields  │
└────────────────────────────┘
```

### 7.3 Event list and event detail

Event row fields:

- Timestamp in local time with timezone/UTC offset; accessible exact timestamp available.
- Source/component (`MCP`, `APP`, `CLI`). The read-only monitor is not a telemetry event producer in v1.
- Tool or operation/event name.
- Optional validated intent/stage/unit identifiers.
- Duration in milliseconds for span events; no fabricated duration for point events.
- Outcome text and icon/shape; color is supplementary.
- Trace/session correlation id, shortened visually but copyable through a user action.

Detail panel fields:

- Event id, time, source/component, name, outcome, duration and unit.
- Trace/span/parent IDs with navigation to present related records.
- Validated intent/stage/unit identifiers if allowed by privacy map.
- Stable safe error code and fixed descriptive copy.
- Allowlisted metadata only. If missing, truncated, unknown, malformed or expired, state that plainly; do not reveal raw record bytes or error sources.
- Return-to-list action preserves filters, selected range, and scroll position.

### 7.4 Filters, time, sorting, paging, refresh

- Bounded ranges: 15 minutes, 1 hour, 24 hours, all retained. Display actual range and timezone/UTC offset. Reject custom ranges above a configured maximum.
- Filters: source/component, tool/operation/event type, outcome, stage, session/trace id. Search only safe indexed metadata; never search prompts, arguments, artifacts, or free text.
- Default order newest first, with documented stable tie-break (timestamp then event id). Show count and active filters; provide clear/reset.
- Use cursor pagination or a bounded virtualized window. Never render all retained events at once. Cursor is bound to query/range; changing filters resets pagination.
- Keep filter state in memory for the current monitor session. Do not place project path, token, or other sensitive data in URL query/hash or persistent browser storage.
- Manual refresh plus modest auto-refresh. Pause/resume affects only UI refresh, not collection. Show last successful refresh, retain last-known data on errors with stale label, apply backoff, and suspend polling when page is hidden.
- Clicking summary values filters the underlying list; charts provide text alternatives and exact range/counts. Avoid charts when the same value is clearer as a direct labeled summary.
- Metric contract: `MCP tool calls` counts distinct `mcp.tool.completed` event IDs; errors and blocked counts use that same completed-tool population and show numerator/denominator. Tool latency uses only completed MCP tool durations, excluding protocol setup and shared-operation spans. Shared-operation counts/latency, if shown, have separate labels and populations; never sum them into tool-call counts because one request may produce multiple operations. Deduplicate by event ID. Exclude unfinished/missing-completion spans from success denominators; expose them as incomplete/unknown where a start event exists.

### 7.5 Dashboard states

| State | Display behavior | Operator guidance |
|---|---|---|
| No telemetry observed | Purposeful empty/unknown state; no invented events or zero-as-healthy metrics | Check the configured collection setting and make a tool call if telemetry is expected. |
| Telemetry configured off | Show disabled only when the setting is supplied through a trusted monitor launch/config input, not inferred from missing files | Enable using documented local configuration if desired. |
| Recent events observed | Show event freshness, range, coverage, summary and rows; do not equate recent events with process liveness | Use filters/details. |
| Partial/corrupt records | Keep valid rows; count skipped records and mark partial | Explain records were skipped; do not hide valid rows. |
| Stale/no recent event | Preserve last-known data with stale banner and timestamp | Refresh or check process/collection state; lack of events is not proof of failure. |
| Store unreadable/sink failure | Distinguish monitor read failure from collector write failure only when an independent signal exists; otherwise status is unknown | Give safe local action, keep workflow unaffected. |
| MCP process liveness unknown | Show “no recent MCP events observed,” not “process absent.” A positive live-session status requires a separate heartbeat/status signal with bounded expiry semantics | Launch/reconnect host if a new MCP call is expected. |
| Query/size limit reached | Show truncated/limited result state and applied limit | Narrow range/filters or load next page. |
| Dashboard shutting down | Stop refresh and show shutdown state | No workflow action implied. |

The best-effort event store cannot prove current process liveness, collection being disabled, or complete event capture. If the monitor receives a trusted telemetry-enabled setting at launch, it may report that configuration; otherwise show “status unknown.” A positive live-session count requires a separate local heartbeat/status mechanism with bounded lease/expiry and its own path, permission, and security review.

### 7.6 Visual, responsive and accessibility design

- Visual direction: restrained local operations instrument, with dense but legible event rows, strong alignment for timestamps/durations, calm surfaces, and clear success/blocked/error semantics.
- Use semantic labels and icons as well as color; consistent status treatment across summary, rows and detail.
- Keyboard access for range picker, filters, table/list rows, trace navigation, refresh and detail close/return. Visible focus and logical tab order.
- Semantic page landmarks, heading hierarchy, accessible control names, and list/table semantics. Announce refresh completion/errors through a restrained live region; do not announce every new event.
- Light/dark contrast; respect reduced-motion. No flashing alerts or continuous animated charts.
- Mobile stacks regions and uses inline details; long IDs/errors wrap or scroll within their own region. Body never scrolls horizontally.
- Metrics state units, time range, sample counts and denominators. Do not imply confidence in small samples.

### 7.7 Dashboard process, API, and security

- Separate monitor process, launched by the `dtech-aidlc monitor --project <path>` CLI subcommand. The CLI is a foreground supervisor: it starts the currently installed `dtech-aidlc` executable with a hidden internal monitor-child mode using the current executable path, prints the URL, and remains attached until the user interrupts/stops it; it does not daemonize or leave an orphan. The monitor remains separate from the MCP stdio process. There is no third public binary or repository-relative child lookup. Test installed-package launch from outside the checkout and clean child shutdown.
- Bind to `127.0.0.1` on an ephemeral port by default; print URL but do not open browser automatically. No `0.0.0.0` binding in v1.
- Read-only GET endpoints only. No POST/PUT/PATCH/DELETE, no tool invocation, and no workflow-state or audit-log reads. Prefer a generic “local project” label; the dashboard reads telemetry files only.
- Query interface accepts bounded time range, allowlisted filter values, sort and opaque cursor. It never accepts a filesystem path or filename from the browser and never returns raw JSONL lines.
- Stable errors: `invalid_filter`, `range_too_large`, `cursor_stale`, `store_unavailable`, `query_limit_reached`.
- Access control: bind loopback only and enforce strict Host/Origin allowlisting, no permissive CORS, and bounded responses. A per-run token remains an open decision: assess how a same-user local process could read command output and whether browser-origin/bootstrap can securely deliver the token without putting it in URLs, project files, or telemetry. Document the chosen threat model and review local cross-process access and DNS rebinding before release.
- Static UI is bundled/embedded. No remote fonts, analytics, scripts, or external services.
- Read only recognized telemetry files; reject symlinks/traversal and enforce retention/query bounds. Ignore unknown schema fields safely.

## 8. Configuration and persistence

Proposed flags/config (exact public names require owner review):

- MCP: `--project`, telemetry on/off, help/version.
- Monitor: `--project`, bind remains loopback-only, optional bounded time window.
- Default telemetry is a release decision. Proposal is local metadata-only enabled with opt-out; disabled-by-default is a valid stricter privacy alternative. Do not finalize by implication.
- `.aidlc/telemetry/` is added to the existing managed ignore block using the safe ignore-file maintenance path. Never replace user-authored ignore content.
- No change to `.aidlc/state.json`, intent `state.json`, phase graph, methodology outputs, or `.aidlc/audit.jsonl` schema in v1.

## 9. Test design

### Application parity

- Use equivalent isolated project fixtures to call CLI and MCP for status, queue, next and report; compare semantic output and all project-file effects.
- Existing CLI golden snapshots remain unchanged unless a separately approved behavior change requires a reviewed snapshot update.
- Verify `NextOutcome` variants and complete/blocked/busy semantics are represented faithfully.

### MCP protocol

- Child-process stdio tests for initialize, tool list, valid calls, schema errors, domain errors, shutdown, and restart.
- Assert all stdout bytes are protocol frames; diagnostics only on stderr.
- Host retries/transport disconnect cases do not trigger unsafe automatic mutation retries.

### Safety/gates

- Invalid/oversized/unknown fields rejected before project access or writes.
- Unsafe `.aidlc` paths/symlinks, invalid project state and concurrency conflicts remain refused.
- Gate approval fields follow the exact existing mode/state restrictions; no assistant identity or special MCP path bypasses checks.
- Audit rows and state mutations match the CLI path for accepted operations.

### Telemetry

- Sensitive data exclusion tests for prompts, raw args/results, details, artifact content, environment values and full project paths.
- Concurrent processes create separate files; no JSONL interleaving; rotation/retention bounded.
- Off, full, corrupt, permission-denied telemetry never changes command result/state.
- Dashboard reader handles malformed tail, unknown schema fields, expired records and symlink attacks safely.

### Dashboard

- Contract tests for range/filter validation, stable cursor pagination, deterministic ordering, stale cursor and size bounds.
- UI tests for each state in §7.5, filters, metric-to-row filtering, selected-event trace navigation, refresh failure/stale retention, and narrow layout.
- Keyboard-only test for all operations and visible focus; screen-reader labels/live region checked; non-color status cues checked.
- Security tests verify loopback bind, Host/Origin policy, no CORS, GET-only surface, traversal/symlink rejection, no mutation route, and no access to arbitrary project files.

### Platform/release

- `cargo fmt --check`, `cargo test --locked`, documented repository checks.
- Windows and at least one POSIX CI environment.
- Install/package smoke test for the two public binaries (`dtech-aidlc`, `dtech-aidlc-mcp`) from outside the checkout; run `dtech-aidlc monitor` and verify its hidden self-launched child serves the UI and terminates cleanly with the supervisor.

## 10. Open decisions before implementation

1. Rust MCP SDK, selected MSRV, and required async runtime, after the dependency/locked-build spike; assess offline cache availability separately if required.
2. Prerequisite handling if configured checks are needed for `aidlc_next`; default is no implicit command execution.
3. Exact AI-mode gate-report UX; human-mode approved decisions are excluded from MCP v1.
4. Telemetry default (enabled metadata-only with opt-out vs opt-in only).
5. Telemetry retention/file size values and whether Windows file permissions need additional handling.
6. Whether the local static UI can avoid a framework/runtime dependency; monitor launch contract is the `dtech-aidlc monitor` subcommand per §7.7.
7. Dashboard token/bootstrap threat model and local cross-process access; Host/Origin checks and loopback binding are required regardless.
8. Dashboard latency percentile minimum sample size and exact metric definitions.

## 11. Detailed design acceptance

- Every component and data field has an owner and explicit trust boundary.
- Tool schemas and side effects match existing operation behavior; no generic shell/filesystem tool.
- Dashboard desktop/mobile layouts and all operational states are specified above and reviewed before implementation.
- Dashboard read API is bounded and read-only; telemetry privacy is defined by an allowlist.
- MCP, telemetry, monitor, event schema and host configuration are clearly labeled proposed until implemented and tested.
