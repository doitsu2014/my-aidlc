# Interfaces and contracts (intent 2)

Registered here: IF-012 to IF-020, CFG-007 to CFG-013. Signatures are Rust-like prose. Intent-1 contracts are named in words ("the intent-1 ToolHost") and amended where stated. ADR and T2 labels are Ideate labels. All numeric limits are compiled-in constants and Estimates (not runtime configuration).

## IF-012 Telemetry line contract v1 and file layout
Traces: FN-023, FN-025, FN-027, FN-032. Serves AC-002, AC-003, AC-008, AC-009, AC-012 (ADR-020, ADR-024, ADR-026).

One JSON object per `\n`-terminated UTF-8 line, at most 16 KiB. Keys appear in this order and no others:

| Key | Type | Rule |
|---|---|---|
| schemaVersion | integer | always 1 |
| eventId | string | `<sessionId>-<n>`, unique |
| timestamp | string | `YYYY-MM-DDTHH:MM:SS.mmmZ` (`audit::format_iso_millis`) |
| component | string | `mcp` or `application` |
| caller | string | always `mcp` in v1 (`cli` is not in the v1 set) |
| processId | integer | OS pid of the MCP process |
| sessionId | string | 32 lowercase hex |
| traceId | string | `<sessionId>-<n>` |
| spanId | string | `<sessionId>-<n>` |
| parentSpanId | string or null | always present; null for a root |
| event | string | one of the 8 kinds below |
| name | string | see the name table |
| intentId | string | optional, omitted when absent or invalid (validated, at most 64 chars) |
| stageId | string | optional, same rule |
| unitId | string | optional, same rule |
| durationMs | integer or null | integer only on `operation.completed` and `mcp.tool.completed`; null otherwise |
| outcome | string or null | `ok`, `blocked` or `error`; null on `*.started` events |
| errorCode | string or null | null unless outcome is `error` |

Event kinds, component, and the meaning of `name`:

| event | component | name | outcome |
|---|---|---|---|
| mcp.request.started | mcp | `status`, `queue`, `next`, `report`, or `unknown` | null |
| mcp.tool.completed | mcp | same as its start | ok, blocked, error |
| mcp.protocol.error | mcp | `protocol` | error (errorCode = protocol code, IF-015) |
| operation.started | application | operation name (the tool name) | null |
| operation.completed | application | same | ok, blocked, error |
| workflow.stage_selected | application | `next` | ok |
| workflow.stage_reported | application | `report` | ok |
| workflow.gate_decided | application | `report` | ok (no approve/reject value in v1; telemetry is not audit) |

errorCode values on tool and operation events: `invalid_arguments`, `approver_required`, `state_conflict`, `operation_failed`, `unknown_tool`, `internal_error`. On `mcp.protocol.error` the code is one of the IF-015 protocol codes.

Trace rules (ADR-026): one `traceId` per `tools/call`; the tool span (`mcp.request.started` and `mcp.tool.completed`) is the root with `parentSpanId` null; the operation pair shares one span whose parent is the tool span; each workflow event has its own span whose parent is the operation span; each `mcp.protocol.error` is its own one-event trace (name `protocol`, parent null).

Never stored: `detail`, `approved_by`, approval note, messages, paths, argv, env, raw frames, the raw unknown tool name.

File layout:
```
.aidlc/telemetry/                      dir, Unix 0o700, created lazily by the first event
.aidlc/telemetry/.gitignore            content: "*\n"
.aidlc/telemetry/mcp-<yyyymmddThhmmssZ>-<pid>-<8hex>-<NNN>.jsonl   Unix 0o600, create_new
```
`<yyyymmddThhmmssZ>` is the process start time in UTC, `<8hex>` the first 8 characters of sessionId, `<NNN>` a 3-digit part from 000. On AlreadyExists the writer takes the next part. One strict pattern (`is_store_file`) is shared by the writer, prune and the reader. Rotation at 5 MiB (5,242,880 bytes) per part, prune by 7 days (mtime) then 50 MiB total, oldest first, touching only pattern-matching regular files and never the current file. The `.aidlc` directory is never created by telemetry.

## IF-013 os_random and hex
Traces: FN-024. Serves AC-010, AC-015 (ADR-029).
```
pub fn os_random(buf: &mut [u8]) -> std::io::Result<()>   // rust-cli/telemetry/mod.rs
pub fn hex(bytes: &[u8]) -> String                        // lowercase
```
Unix: `/dev/urandom` with `read_exact`. Windows: `BCryptGenRandom(null, buf, len, 0x2)`, negative NTSTATUS is `Err`. Fails closed, no fallback generator. Used for sessionId (16 bytes, 32 hex) and the monitor token (16 bytes, 32 hex).

## IF-014 Sink API and fixed stderr lines
Traces: FN-026, FN-027. Serves AC-004, AC-010, AC-020, AC-028 (ADR-021).
```
pub struct Sink { /* SyncSender<String>, session: String, counter: AtomicU64, disabled: AtomicBool, dropped_once: AtomicBool */ }
impl Sink {
  pub fn start(root: &Path) -> Option<Sink>;
  pub fn spawn(session: String, consume: impl FnMut(String) -> io::Result<()> + Send + 'static) -> Sink;
  pub fn next_id(&self) -> String;                       // "<sessionId>-<n>"
  pub fn session(&self) -> &str;
  pub fn send(&self, build: impl FnOnce() -> Event);     // never blocks, never panics
  pub fn finish(self, wait: Duration);                   // drop sender, wait up to `wait` (500 ms)
}
```
Queue: `sync_channel(1024)`. Producer uses `try_send` only. Fixed pathless stderr lines, each printed at most once per process:
- `dtech-aidlc-mcp: telemetry events dropped (writer queue full)`
- `dtech-aidlc-mcp: telemetry disabled (unsafe telemetry location)`
- `dtech-aidlc-mcp: telemetry disabled (write failed)`
- `dtech-aidlc-mcp: telemetry disabled (random source unavailable)`
- `dtech-aidlc-mcp: telemetry disabled (internal error)`

Rules: no state lock, no stdout, no disk access on the producer; a disabled Sink drops silently; drops are disclosed, not counted.

## IF-015 ToolHost v2, protocol codes and traced_call
Traces: FN-030, FN-031. Serves AC-005, AC-028 (ADR-025). Amends the intent-1 ToolHost contract.
```
pub struct ToolHost<C> {
  pub list: fn() -> Value,
  pub call: fn(&C, &str, &Value) -> Option<Result<Value, Value>>,
  pub on_error: fn(&C, &'static str),
}
pub fn traced_call(ctx: &ToolContext, name: &str, args: &Value) -> Option<Result<Value, Value>>;
pub fn on_protocol_error(ctx: &ToolContext, code: &'static str);
fn check_call(ctx, name, args) -> Result<(), Value>;      // validation, internal
fn run_op(ctx, name, args) -> Result<Value, String>;      // operation, internal
```
`ToolContext` gains `pub telemetry: Option<telemetry::Sink>`. `on_error` is called once per error reply, after the reply was written and flushed. The table is implemented by a new private helper `fn error_code_name(code: i64) -> &'static str` in `protocol.rs` (it does not exist today; Observed: -32022 is a literal at `protocol.rs:182` and `:523`). Protocol code table: `line_too_long` (oversized frame), `parse_error` (-32700, invalid UTF-8 or JSON), `invalid_request` (-32600), `method_not_found` (-32601), `invalid_params` (-32602), `unsupported_protocol_version` (-32022), `internal_error` (-32603), `protocol_error` (any other). Notifications and successful replies make no call. Reply bytes and stdout are unchanged. A `tools/call` rejected by `serve` before `host.call` gives only an `invalid_params` protocol event (no tool span). An unknown tool gives one tool span (error, `unknown_tool`, name `unknown`) and one separate `invalid_params` protocol event. A panic in `traced_call` sends error/`internal_error`, then re-raises so the protocol layer still answers -32603.

## IF-016 MCP launch contract v2
Traces: FN-029. Serves AC-001 (ADR-023; CFG-007). Amends the intent-1 launch contract (the launch arguments).
```
dtech-aidlc-mcp [--project <dir>] [--telemetry on|off]     // any order, default on
```
Exit 1, empty stdout, stderr `dtech-aidlc-mcp: <message>` on: a duplicate flag, a missing value, a value other than `on` or `off` (exact lowercase), an unknown argument. Still no `--help` or `--version`. With `off`: no Sink, no file or directory created or changed. No environment variable.
```
fn parse_args(&[OsString]) -> Result<(Option<PathBuf>, bool /*telemetry*/), String>
```

## IF-017 Query contract
Traces: FN-032. Serves AC-003, AC-016 to AC-019 (ADR-030, ADR-032). Response field names are provisional until the WU-016 AI design review (WU-016 may rename before WU-017 builds).
```
pub struct Cache(/* Mutex<HashMap<PathBuf, (u64 len, SystemTime mtime, Arc<Vec<Event>>)>> */);
pub struct QueryError { pub status: u16, pub code: &'static str }
pub fn query(root: &Path, raw_query: &str, now_ms: i64, cache: &Cache) -> Result<serde_json::Value, QueryError>
```
HTTP: `GET /api/events?range&component&name&outcome&stage&session&trace&limit&cursor`, all optional.

| Parameter | Rule |
|---|---|
| range | integer seconds 1..604800, default 3600; presets 900, 3600, 86400, 604800 |
| component | `mcp` or `application` |
| name | `status`, `queue`, `next`, `report`, `unknown`, `protocol` |
| outcome | `ok`, `blocked`, `error` |
| stage, session, trace | id charset and at most 64 chars |
| limit | integer 1..200, default 50 |
| cursor | opaque (below) |

Unknown, duplicate or malformed keys, any `%`, path-like keys, a bad enumeration value, limit out of range or a non-integer range give `invalid_filter`. Range above 604800 gives `range_too_large`.

200 response:
```
{ "schemaVersion": 1,
  "range": 3600, "to": "<ISO ms, frozen at first page>",
  "events": [ <IF-012 object, re-serialised from the allowlisted fields, newest first> ],
  "summary": { "toolCalls": n,
               "blocked": {"count": n, "of": n}, "errors": {"count": n, "of": n},
               "incomplete": n,
               "operations": {"count": n, "errors": n},
               "latency": {"n": n, "p50": ms, "p95": ms} | {"n": n, "insufficient": true},
               "lastEventAt": "<ISO ms>" | null },
  "coverage": { "filesRead": n, "skippedLines": n, "partialTails": n, "unreadableFiles": n, "expiredFiles": n },
  "nextCursor": "<hex>" | null,
  "limitReached": "query_limit_reached" }          // key present only when a cap was hit
```
Order: timestamp descending, tie-break eventId. Summary covers the whole filtered range, not only the page. Latency: nearest-rank p50 and p95 over completed tool calls only when n >= 20. `toolCalls` = distinct `mcp.tool.completed` eventIds. `incomplete` = a started tool span with no completion, never counted as success. Dedup by eventId. Caps: 512 files, 64 MiB, 250,000 events; newest files first; reaching one returns 200 with partial data.

Cursor: lowercase hex of the UTF-8 string `v1|<to_ms>|<qhash16>|<last_ts_ms>|<last_eventId>`, where `qhash16` is the first 16 hex characters of the sha256 of the normalised query (filters and range, without cursor and limit, keys sorted). Not a secret. A decode failure, a hash mismatch or a `to` mismatch gives `409 cursor_stale`. Events newer than the frozen `to` appear on a fresh query.

Reader tolerance: only `is_store_file` regular files (lstat) are read; symlinks, `.gitignore` and subdirectories are ignored; malformed, oversized (over 16 KiB), unknown-kind or unknown-field lines are counted in `skippedLines`; an unterminated last line is a `partialTails` count; a file older than 7 days (mtime) is counted in `expiredFiles` and not shown; an unreadable file is counted in `unreadableFiles`; a file deleted between list and open is skipped silently.

## IF-018 Monitor HTTP contract
Traces: FN-033, FN-034. Serves AC-014, AC-015, AC-016, AC-030 (ADR-027, ADR-029, ADR-032).

Routes (exact raw target, no percent-decoding): `GET /`, `GET /app.js`, `GET /app.css` (no token), `GET /api/events` (query allowed only here; bearer required). One request per connection. Authorization is exactly `Authorization: Bearer <32 lowercase hex>` (header name case-insensitive; the scheme word is matched case-sensitively as `Bearer` with exactly one space; no trimming or case folding of the token). Missing, malformed, duplicate or wrong gives the same 401.

Checks in this order. Every error body is `{"error":"<code>"}` with `Content-Type: application/json`, never echoing input.

| Order | Condition | Status | Code |
|---|---|---|---|
| 0 | 16 connections in flight (at accept, nothing read) | 503 | `busy` |
| 1 | Head not complete within 5 s | 408 | `request_timeout` |
| 2 | Head over 8 KiB | 431 | `header_too_large` |
| 3 | Malformed request line, non-ASCII or control bytes, target not starting with `/`, version other than HTTP/1.0 or 1.1 | 400 | `bad_request` |
| 4 | Any method other than GET (including HEAD, OPTIONS); header `Allow: GET` | 405 | `method_not_allowed` |
| 5 | Duplicate Host, Content-Length other than 0, or any Transfer-Encoding | 400 | `bad_request` |
| 6 | Host missing or not exactly `127.0.0.1:<port>` | 421 | `host_rejected` |
| 7 | Origin present and not exactly `http://127.0.0.1:<port>` (including `null`), or Sec-Fetch-Site present and not `same-origin` or `none` | 403 | `origin_rejected` |
| 8 | Target not exactly one of the four routes (query only on `/api/events`) | 404 | `not_found` |
| 9 | `/api/*` with missing, malformed or wrong bearer token; header `WWW-Authenticate: Bearer` | 401 | `unauthorized` |
| 10 | Bad filter (see IF-017) | 400 | `invalid_filter` |
| 11 | Range above 604800 | 400 | `range_too_large` |
| 12 | Cursor mismatch or undecodable | 409 | `cursor_stale` |
| 13 | `.aidlc` or `.aidlc/telemetry` unsafe or unreadable at query time | 503 | `store_unavailable` |
| 14 | Handler panic | 500 | `internal_error` |
| - | A read cap reached | 200 | no error; `limitReached` in the body |

Method is checked before body headers so a POST with a body still receives its 405. Row 0 is decided at accept and nothing is read. Row 14 is tested through a unit test of the panic guard function around a handler closure, not through a route (no test-only route exists).

Fixed response headers on every response, including errors: `Content-Type`, `Content-Length`, `Connection: close`, `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `Cross-Origin-Resource-Policy: same-origin`, `Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`. Never any `Access-Control-*` header. Content types: `/` is `text/html; charset=utf-8`, `/app.js` is `text/javascript; charset=utf-8`, `/app.css` is `text/css; charset=utf-8`, `/api/events` is `application/json`.

Timeouts and limits (CFG-010): head deadline 5 s total (recomputed per read; both `TimedOut` and `WouldBlock` count), 8 KiB head cap, 5 s write timeout, after a refused method the server half-closes the write side and drains the request body up to 1 MiB or 1 s (Estimate, measured in WU-018).

Token compare: `fn ct_eq(a: &[u8], b: &[u8]) -> bool`: lengths first, then XOR fold with each byte passed through `std::hint::black_box`, true only when the fold is 0.

## IF-019 Monitor CLI contract
Traces: FN-033, FN-035. Serves AC-013, AC-014, AC-025 (ADR-028).
```
dtech-aidlc [--project <p>] monitor            // no command flags, no positional arguments
pub fn run(root: &Path) -> Result<u8, String>  // rust-cli/monitor/mod.rs; never returns Ok once serving
```
- Startup: exactly one stderr line `monitor: http://127.0.0.1:<port>/#token=<32 lowercase hex>  (Ctrl+C to stop)` (two spaces before the parenthesis). Stdout is empty. No browser is opened, no daemon.
- Start failure (the binary's existing Err path prints the message to stderr and exits 1, no path in any message): `Refusing unsafe .aidlc/telemetry path` (the existing `assert_safe_state_dir` message applies to `.aidlc`), `Cannot read the telemetry directory`, `Cannot create a monitor token`, `Cannot bind a loopback port`.
- Absent `.aidlc` or `.aidlc/telemetry`: starts, creates nothing, queries return an empty result.
- Extra flags or arguments: the existing validation error, exit 1.
- Stop: a signal only (SIGINT is shell status 130, SIGTERM 143 on Unix; `Child::kill` on any OS). No path exits 0. The token appears only in the startup line.
- Windows: the `.cmd` launcher may show "Terminate batch job (Y/N)?" on Ctrl+C (documented).

## IF-020 Installer contract v2
Traces: FN-039. Serves AC-026 (ADR-035; T2-22). Amends the intent-1 packaging statement that the scripts stay CLI-only.

`scripts/rust-install/install.sh --from <dir> --prefix <dir> [--upgrade|--uninstall]`, `install.ps1 -From <dir> -Prefix <dir> [-Upgrade|-Uninstall]` (existing flag spellings kept). Layout:
```
<prefix>/lib/dtech-aidlc/{dtech-aidlc[.exe], dtech-aidlc-mcp[.exe], assets/, .dtech-aidlc-rust-install}
POSIX:   <prefix>/bin/dtech-aidlc, <prefix>/bin/dtech-aidlc-mcp      exec launchers
Windows: <prefix>\bin\dtech-aidlc.cmd (unchanged), <prefix>\bin\dtech-aidlc-mcp.exe (real exe)
```
Marker line `dtech-aidlc rust installer v2` (the recogniser accepts v1 and v2). Install requires both built binaries in `<from>/target/release`. Upgrade from a v1 prefix adds the MCP binary and keeps unrecognised files. Uninstall removes only recognised files and the install root. Each launcher has its own recognition check and its own backup and rollback. Windows preflight before any change: processes whose path is under `lib\dtech-aidlc\` or equals `bin\dtech-aidlc-mcp.exe` give exit 1, no change, message `dtech-aidlc files are in use: stop the AI host and any dtech-aidlc monitor, then re-run`. After the commit point, removing the old directory and backups is best-effort and outside the try block; leftovers `.dtech-aidlc-rust.old.*` and MCP backups are removed at the start of the next run. `DTECH_AIDLC_ASSETS` stays a no-op. Installed MCP binary refuses `--version` and `--help` with exit 1 and leaves user settings bytes unchanged.

## Configuration keys
- CFG-007: `--telemetry on|off`, MCP flag only, default on, no env var (IF-016).
- CFG-008: storage and retention: `.aidlc/telemetry/`, file pattern (IF-012), 5 MiB part, 7 days by mtime, 50 MiB total, 16 KiB line cap, Unix 0o700 and 0o600, managed ignore line `/telemetry/`, self `.gitignore` `*`.
- CFG-009: writer: queue 1024, EOF wait 500 ms.
- CFG-010: monitor server: bind `127.0.0.1:0`, 16 connections, 5 s head deadline, 8 KiB head cap, 5 s write timeout, drain bound 1 MiB or 1 s.
- CFG-011: query: range 1..604800 default 3600, limit 1..200 default 50, caps 512 files, 64 MiB, 250,000 events, percentiles at n >= 20.
- CFG-012: dashboard: poll 5 s, backoff up to 60 s, stale after 15 s, quiet after 15 min, paused while hidden.
- CFG-013: release identifiers: crate version 1.3.0, installer marker v2.

## Release and golden contract
Traces: FN-028, FN-035, FN-040. Serves AC-023, AC-024, AC-027 (ADR-033; owner Q2, Q4, Q5).

Allowed re-bless reasons, exactly three: (a) the version bump; (b) `monitor` in help, completion and validation; (c) the managed `.gitignore` line `/telemetry/`. Exactly seven goldens may change. Bless only with the baseline environment (TMPDIR under /private/tmp, `target/debug` on PATH) and only `DTECH_AIDLC_BLESS=1 cargo test --locked --test cli_golden --test utility_golden --test routing_golden`. Afterwards `git diff --stat -- tests/golden` must list only these files; any other file stops the unit (ADR-033 reopen trigger).

| Golden | Reasons | Unit and diff content |
|---|---|---|
| tests/golden/routing_golden/lifecycle.json | (c), (a) | WU-013: only the `/telemetry/` line; WU-021: version |
| tests/golden/routing_golden/lifecycle-embedded.json | (c), (a) | same |
| tests/golden/utility_golden/help-and-completion.json | (b), (a) | WU-018: only monitor lines (help and completion); WU-021: version |
| tests/golden/routing_golden/validation.json | (b), (a) | WU-018: the help monitor line plus new steps `monitor --no-such-option` and `monitor extra0`; WU-021: version |
| tests/golden/cli_golden/version.json | (a) | WU-021 |
| tests/golden/routing_golden/harness.json | (a) | WU-021 |
| tests/golden/routing_golden/read-only.json | (a) | WU-021 |

Review: for each golden `diff <(git show HEAD:<f> | sed 's/1\.2\.2/1.3.0/g') <f>`; at WU-021 the result must be empty because (b) and (c) are already blessed. `routing_golden/unsafe.json` and every deps-add golden stay byte-identical; `monitor` is not added to the unsafe list. `LOCK_SHA256` in `tests/mcp_packaging.rs` is recomputed from the CR-stripped bumped `Cargo.lock`; the lock diff is the `dtech-aidlc` version line only (zero new crates).

CHANGELOG `## 1.3.0` checklist (one entry for intents 1 and 2, AC-024, ADR-039 reading 9): the `dtech-aidlc-mcp` binary; Rust 1.100 minimum; MCP telemetry default on, `--telemetry off`, `.aidlc/telemetry`, retention 5 MiB per file, 7 days, 50 MiB, CLI writes none, drops not recorded; the `monitor` command (loopback, read-only, token); the approved golden changes; installers for both binaries; the `deps add` lock; "local optional capabilities"; "approver names are self-asserted"; "telemetry is not audit". Release notes also state which phases are released when the proposed label stays.
