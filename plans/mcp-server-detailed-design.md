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
  [[bin]] dtech-aidlc       -> rust-cli/main.rs       (existing)
  [[bin]] dtech-aidlc-mcp   -> rust-cli/mcp_main.rs   (proposed)
  [[bin]] dtech-aidlc-monitor -> rust-cli/monitor_main.rs (proposed; verify command naming)

rust-cli/
  application.rs            shared request context and operation façade (new)
  telemetry.rs              event schema, sink, rotation, redaction (new)
  mcp/
    mod.rs                  server setup and protocol adapter (new)
    tools.rs                schemas and tool dispatch (new)
    errors.rs               domain-to-MCP-safe error mapping (new)
  monitor/
    mod.rs                  local server/read-only routes (new)
    store.rs                bounded telemetry reader/query/pagination (new)
    ui.html                 bundled dashboard shell (new)
  main.rs                   existing CLI adapter; refactor incrementally
```

The module names are proposed. Avoid an unnecessary web framework or runtime unless required by the selected MCP SDK and monitor implementation. Validate dependency maintenance, licensing, MSRV, transport support, offline build constraints and lockfile reproducibility in a spike before selecting crates.

## 3. Shared application façade

### 3.1 Context

Proposed `ProjectContext` is immutable process configuration:

```rust
pub struct ProjectContext {
    pub project_root: PathBuf,       // canonicalized/normalized absolute root
    pub graph: scope::Graph,
    pub config: Value,
    pub engine: orchestrate::Engine,
    pub telemetry: Option<TelemetryHandle>,
    pub caller: CallerKind,          // Cli | Mcp
}
```

Do not expose an arbitrary path argument on each MCP tool. Resolve once from startup CWD or explicit `--project`; use the existing `orchestrate::resolve_like_node` and `audit::assert_safe_state_dir` behavior. Reproduce the CLI's embedded/external engine handling using `DTECH_AIDLC_HOME` exactly; do not silently select an alternate engine.

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

- `status`: `status::status(root, graph, intent)`.
- `queue`: existing `unit_commands::run("orchestrate queue", root, graph, config, options)`.
- `next`: existing `orchestrate::orchestrate_next(root, graph.value(), config, engine, options)` and convert `NextOutcome::value()` without dropping blocked/busy/completion fields.
- `report`: existing `orchestrate::orchestrate_report_with_engine(root, graph.value(), config, engine, options)`.

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

### 4.2 Protocol dependency

Use a maintained Rust MCP SDK only after the architecture spike validates its protocol support, stdio behavior, license, MSRV, async runtime footprint, Windows support, and locked/offline build compatibility. Pin in `Cargo.lock`. Do not implement JSON-RPC framing by hand unless an ADR documents the SDK blocker and a conformance plan.

### 4.3 Tool set and schemas

Initial tools are intentionally narrow:

#### `aidlc_status` (read)

Input: optional `intent_id` only if the existing status operation supports selecting it. Otherwise use empty object with `additionalProperties: false`.

Output: existing status projection as structured data. Avoid leaking full project absolute paths.

#### `aidlc_queue` (read)

Input: empty object, `additionalProperties: false`.

Output: existing queue representation of ready/waiting/blocked/parked/done units. This is not a state mutation; preserve the existing in-memory migration-preview behavior.

#### `aidlc_next` (state-affecting)

Input: begin with no assistant-supplied gate identity or arbitrary root. The narrowest MVP should only select the next step on an already onboarded project; creating a new intent through `next` is a separate scope decision because it accepts intent/scope/depth/mode and may create workflow state. If new-intent creation is included, make those fields explicit, bounded, and require the same existing validation as CLI.

Output: full directive/complete/blocked/busy structure. Document that selecting a directive may persist current stage, perform existing migration, and append existing audit events.

#### `aidlc_report` (state mutation)

Inputs map only to current `ReportOptions`: `stage`, `result`, `detail`, `approval`, `approved_by`, `approval_note`, `needs`, and `unit`, subject to the exact existing conditions. Strict schema; bounded detail/list/string sizes; `result` enum from current CLI contract. Do not add fields the domain operation cannot enforce.

Output: the existing structured report result or safe error.

### 4.4 Gate and approval handling

- Do not create a special tool that bypasses `orchestrate_report` or directly writes gate state.
- Gate decision fields may be passed only through the same report path and only where existing gate rules accept them.
- Preserve human/AI mode behavior, configured gate mode, allowed-approver checks, AI recommendation requirements and open-item checks.
- `approved_by` is a supplied label, not authenticated identity. Neither MCP nor the dashboard may present it as proof of human approval.
- MCP host user-confirmation UI may be recommended where available but is not guaranteed by protocol. Documentation must state this boundary.

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
  "processId": 1234,
  "sessionId": "opaque-process-session-id",
  "traceId": "opaque-correlation-id",
  "spanId": "opaque-span-id",
  "parentSpanId": null,
  "component": "mcp|application|cli|monitor",
  "event": "mcp.tool.completed|operation.completed|stage.selected|gate.decided",
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

### 6.3 Event names and correlation

- `process.started`, `process.stopped`.
- `mcp.request.started`, `mcp.tool.completed`, `mcp.protocol.error`.
- `operation.started`, `operation.completed`.
- Selected existing lifecycle transitions: `stage.selected`, `stage.reported`, `unit.parked`, `unit.revisited`, `gate.decided`, `intent.paused`, `intent.resumed`, `intent.abandoned`.
- Preserve `component` and `caller` distinction so a CLI-originated operation does not look like an MCP tool call.
- Propagate a generated `traceId`/parent span through one MCP call into the application operation. For direct CLI calls, generate a trace root with no MCP parent.
- Correlation IDs are operational metadata, not authentication credentials.

## 7. Dashboard detailed design

### 7.1 Product purpose and users

A local operator/maintainer uses the dashboard to answer:

1. Is telemetry collecting, disabled, degraded, or stale?
2. Which tools/operations are active, slow, blocked, or failing in the selected range?
3. What event sequence led to a specific failure or stage transition?

The dashboard is a separately started, read-only local app. It cannot call MCP tools, mutate workflow state, approve gates, replay reports, change telemetry settings, or delete logs.

### 7.2 Navigation and screen layout

One responsive screen for v1, with a header and four regions:

1. **Header:** “AIDLC Monitor”, selected safe project label (not full path), local-only indicator, bounded time-range picker, refresh button/status.
2. **Health strip:** telemetry collection state, last event, selected interval, known MCP session count, and an explicit coverage status (`MCP-only`, `shared events available`, `partial`, `stale`, etc.). Do not infer server health merely from the absence of errors/events.
3. **Activity summary:** request count, error numerator/denominator, blocked count, latency p50/p95 with units/sample count, and a compact event/outcome time series. Each metric is clickable and filters the event stream. Below a defined minimum sample count show “insufficient samples”.
4. **Event workspace:** central newest-first event list with filters; selected event details in a right panel on desktop and inline disclosure on mobile.

Desktop wireframe:

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ AIDLC MONITOR  project label  LOCAL ONLY  [15m ▾]  [Refresh]             │
├──────────────────────────────────────────────────────────────────────────┤
│ ● RECORDING  last event 14:32:08  MCP sessions 1  coverage: PARTIAL      │
├───────────────────────────────────────────────┬──────────────────────────┤
│ Calls 128 · errors 3/128 · blocked 4          │ FILTERS                  │
│ p50 42 ms · p95 240 ms · n=124 completed      │ source / outcome          │
│ [latency distribution] [events by time]        │ tool / stage / trace      │
│                                               │ [clear filters]           │
│ EVENTS · newest first · 128 matches           ├──────────────────────────┤
│ time       source  operation     duration status│ SELECTED EVENT           │
│ 14:32:08   MCP     aidlc_next    42 ms    OK    │ trace/span/parent         │
│ 14:31:51   APP     stage.selected 18 ms  OK    │ safe code + safe fields   │
│ 14:31:22   MCP     aidlc_report  86 ms BLOCKED │ return to filtered list   │
│ [Load more]                                    │                          │
└───────────────────────────────────────────────┴──────────────────────────┘
```

Mobile wireframe (no horizontal page scroll):

```text
┌────────────────────────────┐
│ AIDLC MONITOR   [15m ▾]   │
│ LOCAL ONLY · ● RECORDING  │
│ last event 14:32 · PARTIAL│
├────────────────────────────┤
│ calls 128 · errors 3/128  │
│ p50 42 ms · p95 240 ms    │
│ [latency] [event volume]  │
├────────────────────────────┤
│ [Filters (2)] [Refresh]   │
│ 14:32 MCP aidlc_next 42ms│
│ 14:31 APP stage.selected │
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
- Source/component (`MCP`, `APP`, `CLI`, `MONITOR`).
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

### 7.5 Dashboard states

| State | Display behavior | Operator guidance |
|---|---|---|
| First run/no telemetry directory | Purposeful empty state; no invented events or zero-as-healthy metrics | Launch MCP with telemetry enabled and perform a tool call; run monitor in selected project. |
| Telemetry disabled | Explicit disabled badge, no claim of server health | Enable telemetry in documented local configuration if desired. |
| Healthy data | Freshness, range, coverage, summary and event rows | Use filters/details. |
| Partial/corrupt records | Keep valid rows; count skipped records and mark partial | Explain records were skipped; do not hide valid rows. |
| Stale data/no recent event | Preserve last-known data with stale banner and timestamp | Refresh or check process/collection state; lack of events is not proof of failure. |
| Store unreadable/sink failure | Distinguish monitor read failure from collector write failure | Give safe local action, keep workflow unaffected. |
| MCP process absent | State “no active MCP session observed”; do not call this workflow failure | Launch/reconnect host if an MCP call is expected. |
| Query/size limit reached | Show truncated/limited result state and applied limit | Narrow range/filters or load next page. |
| Dashboard shutting down | Stop refresh and show shutdown state | No workflow action implied. |

### 7.6 Visual, responsive and accessibility design

- Visual direction: restrained local operations instrument, with dense but legible event rows, strong alignment for timestamps/durations, calm surfaces, and clear success/blocked/error semantics.
- Use semantic labels and icons as well as color; consistent status treatment across summary, rows and detail.
- Keyboard access for range picker, filters, table/list rows, trace navigation, refresh and detail close/return. Visible focus and logical tab order.
- Semantic page landmarks, heading hierarchy, accessible control names, and list/table semantics. Announce refresh completion/errors through a restrained live region; do not announce every new event.
- Light/dark contrast; respect reduced-motion. No flashing alerts or continuous animated charts.
- Mobile stacks regions and uses inline details; long IDs/errors wrap or scroll within their own region. Body never scrolls horizontally.
- Metrics state units, time range, sample counts and denominators. Do not imply confidence in small samples.

### 7.7 Dashboard process, API, and security

- Separate proposed command: `dtech-aidlc monitor --project <path>`; final name requires CLI naming review.
- Bind to `127.0.0.1` on an ephemeral port by default; print URL but do not open browser automatically. No `0.0.0.0` binding in v1.
- Read-only GET endpoints only. No POST/PUT/PATCH/DELETE, no tool invocation, no workflow state file access except a safe project label if separately approved; prefer telemetry-only reads.
- Query interface accepts bounded time range, allowlisted filter values, sort and opaque cursor. It never accepts a filesystem path or filename from the browser and never returns raw JSONL lines.
- Stable errors: `invalid_filter`, `range_too_large`, `cursor_stale`, `store_unavailable`, `query_limit_reached`.
- Strict Host/Origin handling, no permissive CORS, bounded response sizes and event counts, loopback-only bind. Decide whether a per-run unguessable read token is required after browser security review; never store it in telemetry or project files.
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
- `cargo install`/packaging smoke test for CLI, MCP, and monitor binaries from outside checkout.

## 10. Open decisions before implementation

1. Rust MCP SDK and required async runtime, after dependency/offline/MSRV spike.
2. Whether `aidlc_next` can create a new intent over MCP in v1 or only select next stage for an existing intent.
3. Exact gate-report tool UX and whether supported hosts can reliably require a user confirmation UI.
4. Telemetry default (enabled metadata-only with opt-out vs opt-in only).
5. Telemetry retention/file size values and whether Windows file permissions need additional handling.
6. Monitor command naming and whether local static UI can avoid a framework/runtime dependency.
7. Loopback dashboard browser access: Host/Origin protections alone or a per-run token.
8. Dashboard latency percentile minimum sample size and exact metric definitions.

## 11. Detailed design acceptance

- Every component and data field has an owner and explicit trust boundary.
- Tool schemas and side effects match existing operation behavior; no generic shell/filesystem tool.
- Dashboard desktop/mobile layouts and all operational states are specified above and reviewed before implementation.
- Dashboard read API is bounded and read-only; telemetry privacy is defined by an allowlist.
- MCP, telemetry, monitor, event schema and host configuration are clearly labeled proposed until implemented and tested.
