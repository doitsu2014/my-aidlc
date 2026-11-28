# Functional design (intent 2)

Registered here: FN-023 to FN-044, IF-012 to IF-020, CFG-007 to CFG-013 (see ids.json). Intent-1 identifiers are cited only in words ("intent-1 facade"), never by number. ADR-019 to ADR-039 and T2-01 to T2-24 are Ideate labels.

## 1. Purpose and trace
- Approved direction: Option A (ADR-019). A background telemetry writer thread with a bounded queue (ADR-021), a telemetry hook in the MCP adapter and not in the facade (ADR-022, owner Q1: the CLI writes no telemetry), the flag `--telemetry on|off` default on (ADR-023), an in-process `dtech-aidlc monitor` with a std-only HTTP/1.1 loopback server (ADR-027, ADR-028), a per-run token in the URL fragment (ADR-029), server-side query and cursor (ADR-030), three embedded static assets (ADR-031), one status table (ADR-032), zero new crates (Q3), and release 1.3.0 prepared but not tagged (ADR-033).
- Acceptance criteria covered: AC-001 to AC-030. Owner Q1 supersedes the CLI parts of AC-006 and AC-012 (CON18): they are read as MCP-caller only, and the dashboard coverage line says so.
- This stage is design only. No source, test, Cargo, README, CHANGELOG, PROJECT.md, plans/, scripts/ or .github file is changed here.

## 2. Context
- Actors: an AI host talking to `dtech-aidlc-mcp` over stdio; the owner at a terminal running `dtech-aidlc monitor` and a browser on the same machine; CI runners (windows-latest, macos-latest required, ubuntu best-effort); the installer user.
- In scope: telemetry writer and store, adapter instrumentation, `monitor` command and server, dashboard assets, `deps add` lock, three flaky-test fixes, both-binary installers, 1.3.0 prepare, README and Studio-mock updates, the conditional "proposed" removal, an AI dashboard design review (AC-022) and an AI threat-model review (AC-030).
- Out of scope: any CLI telemetry (Q1); a tag, publish or push; CI/clippy cleanup (Q12); new crates; AC amendments; DeepSeek verification; approve/reject counts (ADR-024); a browser test runner.
- Existing components and conventions affected: `rust-cli/mcp/{mod,protocol,tools}.rs`, `rust-cli/knowledge.rs` (IGNORE_BLOCK), `rust-cli/args.rs`, `main.rs`, `help.txt`, `completion.zsh`, `lib.rs`, `dependencies.rs`, test modules in `artifacts.rs`, `onboarding/projection.rs`, `scope.rs`, `prerequisites.rs`, `scripts/rust-install/*`, `Cargo.toml`, `Cargo.lock`, `tests/mcp_packaging.rs`, `tests/repo_content.rs`, goldens. Conventions kept: library functions return `Result<_, String>`; the binary owns printing and exit codes; stdout of the MCP binary carries protocol frames only (AC-028); no `println!` in the library; tests are Rust integration tests under `tests/`; goldens move only for the three allowed reasons.
- New modules: `rust-cli/telemetry/{mod,sink,query}.rs`, `rust-cli/monitor/{mod,http}.rs`, `rust-cli/monitor/assets/{index.html,app.js,app.css}`. Reader and writer share one schema type (AC-003).
- Baseline for every unit: macOS suite 545 passed, 0 failed (intent-1 evidence E15); PROJECT.md "Known baseline failures" is stale (owner Q16).

## 3. Functional decomposition

### FN-023 Telemetry schema v1
- Traces to: AC-002, AC-003, AC-005 (ADR-024, ADR-026; T2-10, T2-14)
- Actor: the MCP adapter (producer), the query engine (reader).
- Preconditions: none.
- Main flow: `Event` is built field by field, never from a generic `Value`. `EventKind` has exactly the 8 v1 kinds. `Event::to_line` emits `schemaVersion: 1` and exactly the IF-012 keys in IF-012 order, then no trailing newline (the writer adds it). Timestamps use `audit::format_iso_millis`. `parse_line` re-validates every field: key set exact (no unknown, no missing required), kind in the enumeration, `component`/`caller`/`outcome`/`errorCode` in their closed sets, id fields matching the charset and 64-char cap, ids `<32 hex>-<n>` shaped.
- Alternate and error flows: `parse_line` returns `None` for an unknown kind, an extra field, a missing field, a bad value or a line over 16 KiB; the caller counts it (FN-032). Never panics.
- Postconditions: a line written by this crate always parses back to an equal `Event`.

### FN-024 os_random and hex
- Traces to: AC-010, AC-015 (ADR-026, ADR-029; T2-19)
- Actor: Sink start (sessionId), monitor start (token).
- Preconditions: none.
- Main flow: Unix reads `/dev/urandom` with `read_exact`. Windows calls `BCryptGenRandom(NULL, buf, len, 0x2)` through a plain `#[link(name = "bcrypt")] extern "system"` block, with a SAFETY comment, modelled on the existing kernel32 FFI in `dependencies.rs`. `hex` returns lowercase hex.
- Alternate and error flows: any failure, or a negative NTSTATUS, is `Err`. There is no fallback generator (fail closed). If the Windows link fails in CI, switch the block to `raw-dylib` (ADR-029 reopen trigger).
- Postconditions: the buffer is fully filled on `Ok`.

### FN-025 Store location, StoreState and file-name pattern
- Traces to: AC-008, AC-012, AC-016 (ADR-020, ADR-028 edge cases 1-4; T2-13)
- Actor: the writer thread and the query engine (both call it).
- Main flow: `store_state(root)` runs `audit::assert_safe_state_dir` for `.aidlc`; failure or a non-directory gives `Unsafe`; a missing `.aidlc` gives `Absent`. Then lstat of `.aidlc/telemetry`: missing gives `Absent`, a real directory gives `Dir`, a symlink or non-directory gives `Unsafe`. `file_name` builds `mcp-<yyyymmddThhmmssZ>-<pid>-<8hex>-<NNN>.jsonl`. `is_store_file(name)` applies the same strict pattern.
- Alternate and error flows: an unreadable `.aidlc` is `Unsafe` for the monitor and `Absent`-and-silent for the writer only when `.aidlc` itself is missing; the two callers map `Unsafe` differently (writer disables telemetry, monitor answers 503 or exits 1).
- Postconditions: no function here writes.

### FN-026 Sink producer
- Traces to: AC-004, AC-010, AC-018, AC-020, AC-028 (ADR-021, ADR-026; T2-15)
- Actor: the MCP adapter on the request thread.
- Preconditions: telemetry on and a project root.
- Main flow: `Sink::start(root)` fills a 16-byte `os_random` buffer, hex-encodes it as `sessionId`, creates `sync_channel(1024)`, spawns the writer thread (FN-027) and returns the Sink. `Sink::spawn(session, consume)` is the same with an injected consumer closure (used by the stall test, no runtime switch). `next_id()` returns `<sessionId>-<n>` from one `AtomicU64`. `send(build)` runs `build` and `try_send` inside `catch_unwind`. `finish(wait)` drops the sender and waits up to 500 ms on a done channel.
- Alternate and error flows: a full queue drops the event and prints the first-drop line once (IF-014); drops are disclosed, not counted. A disconnected channel or a panic disables telemetry with one fixed pathless stderr line. `os_random` failure at start prints one fixed line and returns `None`; the server keeps working. The producer never touches the disk and never takes the state lock (CON12).
- Postconditions: a request never waits on telemetry I/O; replies and stdout bytes are identical with telemetry on or off.

### FN-027 FileStore writer
- Traces to: AC-008, AC-009, AC-012 (ADR-020; T2-13, T2-16, T2-18)
- Actor: the writer thread.
- Main flow: on the first event, re-check `store_state`. `Absent` with no `.aidlc` drops events silently and never creates `.aidlc`. Otherwise create `.aidlc/telemetry` (Unix mode 0o700), write `.aidlc/telemetry/.gitignore` containing `*`, then prune. Open the file with `create_new` (Unix mode 0o600); on `AlreadyExists` try part+1. Append each line plus `\n`. When the next line would exceed 5 MiB, rotate to the next part and prune again. `prune(dir, current, now)` considers only pattern-matching regular files (lstat), never the current file; it deletes files older than 7 days by mtime first, then oldest first until the total is at most 50 MiB.
- Alternate and error flows: `Unsafe` disables telemetry with a pathless stderr line and writes nothing through the path. A write or open error disables telemetry with one stderr line; delete errors in prune are ignored (Windows delete-pending must not loop). The writer never deletes unrelated files.
- Postconditions: the store holds only IF-012 lines; a corrupt or truncated existing file is never modified.

### FN-028 Managed .gitignore line
- Traces to: AC-008 (ADR-020; T2-18)
- Actor: `knowledge::ensure_ignore_file` (unchanged function).
- Main flow: one `/telemetry/` line is added to `IGNORE_BLOCK` after `/repos/`. The marker stays `v1` (CON3).
- Postconditions: the only golden diffs are the `/telemetry/` line in the lifecycle goldens (Q2/Q4 reason c).

### FN-029 MCP launch v2
- Traces to: AC-001 (ADR-023; CFG-007)
- Actor: the AI host starting `dtech-aidlc-mcp`.
- Main flow: an order-independent loop parses `--project <dir>` and `--telemetry on|off`. With telemetry on, `Sink::start(&root)`; `ToolContext` gains `telemetry: Option<Sink>`. `ToolHost` becomes `{ list, call: traced_call, on_error: on_protocol_error }`. After `serve` returns (stdin EOF), `sink.finish(500 ms)`.
- Alternate and error flows: a duplicate flag, a missing value, a value other than `on`/`off` or an unknown argument exits 1 with `dtech-aidlc-mcp: <message>` and empty stdout. Still no `--help` or `--version`. With `off` the Sink is never constructed and no file or directory is created or changed.
- Postconditions: default is on (AC-001); the setting is not exposed to the page (the dashboard shows "unknown").

### FN-030 Protocol-error observer
- Traces to: AC-005, AC-028 (ADR-025; T2-11, T2-12)
- Actor: `protocol::serve`.
- Main flow: `ToolHost<C>` gains `on_error: fn(&C, &'static str)`. After a successful write and flush of a reply that carries `error.code`, `serve` calls `on_error(ctx, code)`: `Frame::Oversized` gives `line_too_long`; otherwise a new private helper `error_code_name` (IF-015; it does not exist today, Observed: -32022 is a literal at `protocol.rs:182` and `:523`) maps -32700 `parse_error`, -32600 `invalid_request`, -32601 `method_not_found`, -32602 `invalid_params`, -32022 `unsupported_protocol_version`, -32603 `internal_error`, anything else `protocol_error`.
- Alternate and error flows: notifications get no reply and so no call; a successful `initialize`, `server/discover`, `tools/list` or `ping` makes no call. A failed write makes no call.
- Postconditions: reply bytes are unchanged; no frame text reaches telemetry (`&'static str`). Fallback if the owner objects to the rule-5 amendment: suppress `on_error` until the first `tools/call`.

### FN-031 traced_call adapter
- Traces to: AC-002, AC-004, AC-006, AC-007 (ADR-022, ADR-024, ADR-025, ADR-026; T2-10, T2-17)
- Actor: `ToolHost.call`.
- Main flow: `call_tool` is split into `check_call` (validation) and `run_op` (the operation closure); `call_tool` stays `check_call` then `finish(ctx, run_op(..))`, so existing tests hold. `traced_call` with no Sink returns `call_tool`. With a Sink: mint `traceId`; send `mcp.request.started` (tool span, parent null); unknown tool gives `mcp.tool.completed` error/`unknown_tool`, name `unknown`, then `None`; failed `check_call` gives completed error/`invalid_arguments`; otherwise `operation.started` (component `application`, parent = tool span), `run_op`, `operation.completed` with `durationMs` and outcome. After Ok only: `workflow.stage_selected` for a `next` directive, `workflow.stage_reported` for a successful `report`, plus `workflow.gate_decided` when the arguments carried `approval` (no decision value). Finally `mcp.tool.completed`: directive, complete, status and queue map to `ok`; blocked and busy to `blocked`; an error result to `error` with the existing four codes.
- Alternate and error flows: the whole body runs under `catch_unwind`; a panic sends error/`internal_error`, then `resume_unwind`, so the protocol layer still answers -32603. `on_protocol_error(ctx, code)` sends a one-event trace, name `protocol`. Ids (`intentId`, `stageId`, `unitId`) are stored only if they pass the existing validators and the 64-char cap, otherwise omitted (Q13). `detail`, `approved_by`, approval notes, paths, argv, env and raw frames are never stored. The `readOnlyHint` stays unchanged.
- Postconditions: workflow events appear only after the state and audit write succeeded; a conflicting report gives only an error `operation.completed`.

### FN-032 Query engine
- Traces to: AC-003, AC-016, AC-017, AC-018, AC-019 (ADR-028 edge cases 3-8, ADR-030, ADR-032; T2-07, T2-13, T2-14)
- Actor: the monitor `/api/events` handler (read-only).
- Main flow: `query(root, raw_query, now_ms, cache)` validates parameters (IF-017), checks `store_state`, lists only `is_store_file` regular files (lstat) newest first, reads within the caps (512 files, 64 MiB, 250,000 events), dedups by `eventId`, sorts events newest first (timestamp, then eventId), applies filters, builds the summary over the whole filtered range, pages with the cursor and returns the IF-017 object.
- Alternate and error flows: counted, not fatal: malformed, oversized and unknown lines, partial tails, unreadable files, expired files (older than 7 days). Errors by ADR-032 rows 10-13 (`invalid_filter`, `range_too_large`, `cursor_stale`, `store_unavailable`). A file deleted between list and open is skipped silently. A cap reached sets `limitReached: "query_limit_reached"` with a 200.
- Postconditions: never writes, never deletes; a parse cache keyed by (path, len, mtime) holds memory up to about the caps (Estimate, unmeasured).
- Metrics: tool calls = distinct `mcp.tool.completed` eventIds; blocked and error ratios carry numerator and denominator; `incomplete` counts a start with no completion and is never counted as success; operation counts are separate; p50 and p95 by nearest rank only at n >= 20, otherwise `insufficient`.

### FN-033 Monitor server
- Traces to: AC-013, AC-014, AC-016 (ADR-027, ADR-028, ADR-029 delivery, ADR-031; T2-09, T2-20, T2-21)
- Actor: the owner running `dtech-aidlc monitor`.
- Main flow: `run(root)` checks `store_state` (Unsafe is an error), checks the telemetry directory is readable when present, creates a 16-byte token with `os_random`, binds `127.0.0.1:0`, prints one stderr line (IF-019) and nothing to stdout, then loops on `accept`. An `AtomicUsize` slot count (16) with a Drop guard admits connections; the 17th gets `503 busy` immediately. Each admitted connection runs on its own thread under `catch_unwind`. The route table serves `/`, `/app.js`, `/app.css` from `include_str!` without a token, and `/api/events` through FN-032.
- Alternate and error flows: start failures (unsafe path, unreadable store, random failure, bind failure) return a fixed pathless `Err`, which the binary turns into exit 1. A handler panic answers `500 internal_error` and releases the slot. No per-request logging. The loop never returns; the process ends on a signal (130 for SIGINT, 143 for SIGTERM on Unix; no path exits 0).
- Postconditions: opens no browser, does not daemonize, never creates `.aidlc` or `.aidlc/telemetry`.

### FN-034 HTTP layer
- Traces to: AC-014, AC-015, AC-030 (ADR-027, ADR-029 compare, ADR-032; T2-01, T2-02, T2-04, T2-07, T2-08, T2-23)
- Actor: each connection thread.
- Main flow: `read_head` enforces a 5 s total deadline recomputed before each read (matching both `TimedOut` and `WouldBlock`), an 8 KiB cap and a 5 s write timeout. Then checks 3 to 9 of the ADR-032 table in order (IF-018). `ct_eq` compares lengths first, then XOR-folds every byte through `std::hint::black_box`. `respond` writes the fixed headers and never an `Access-Control-*` header. A refused method half-closes the write side and drains the request body up to the drain bound (CFG-010), so a POST with a large body still reads its 405.
- Alternate and error flows: every error body is `{"error":"<code>"}` and never echoes input or a path. One request per connection.
- Postconditions: token comparison does not trim or case-fold; the `Bearer` scheme word is matched exactly (IF-018).

### FN-035 monitor command wiring
- Traces to: AC-013, AC-025, AC-027 (ADR-028, ADR-033 reason b)
- Actor: the CLI dispatcher.
- Main flow: `monitor` is added to `command_flags` (no flags), `command_max_args` (0), `COMMANDS`, the args spec unit test, `main.rs` dispatch (after `assert_safe_state_dir`), `help.txt`, `completion.zsh`, and `COMMAND_KEYS` in `tests/repo_content.rs` and `tests/routing_golden.rs`.
- Alternate and error flows: any extra flag or positional argument is rejected by the existing validation path (the goldens gain `monitor --no-such-option` and `monitor extra0`). `monitor` is not added to the `unsafe` golden list; its unsafe start is a process test.
- Postconditions: only the monitor lines change in `utility_golden/help-and-completion.json` and `routing_golden/validation.json`.

### FN-036 Dashboard assets
- Traces to: AC-011, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021 (ADR-029 page storage, ADR-030 refresh, ADR-031, ADR-039; T2-03, T2-14, T2-20, T2-24)
- Actor: the owner in a browser.
- Main flow: `app.js` reads `location.hash` once into a closure-scoped constant, calls `history.replaceState(null, '', '/')` before the first fetch, and sends `Authorization: Bearer` only on `/api/events`. It renders only through `textContent`. States: loading, empty ("No telemetry observed"), partial (limit reached), stale (last success older than 15 s), error, unavailable (503), token missing, monitor stopped (network error stops polling), quiet (no event for 15 min). The setting shows "unknown"; liveness is always "unknown". The coverage line says "Dropped events: not recorded", "CLI writes no events" and lists the kinds not observed. Polling is every 5 s, doubling to 60 s on an HTTP error, paused while hidden. Filters live in memory only. Event detail has parent navigation, "no parent" and "parent not in store". A privacy panel says "local project" and never shows a path. `app.css` uses system fonts, a visible focus ring, `prefers-reduced-motion` and no horizontal scroll at the width chosen in the design review (recommended 320 px).
- Alternate and error flows: no token means no API call. No storage, cookie, IndexedDB, `pushState`, `innerHTML`, `eval` or remote URL.
- Postconditions: the full content lands only after the design pack is accepted in writing (AC-022).

### FN-037 deps add under the state lock
- Traces to: AC-027 (ADR-037; Q12)
- Actor: the CLI `deps add`.
- Main flow: after `assert_safe_state_dir`, `run_deps` for action `add` wraps a new `deps_unlocked` in the existing `orchestrate::with_state_lock`. Other actions are unchanged.
- Alternate and error flows: lock busy follows the existing lock behaviour. If any `rust_repos_deps_ids` golden byte changes, stop the unit (ADR-037 reopen trigger).
- Postconditions: three concurrent `deps add` processes keep three edges.

### FN-038 Flaky-test fixes
- Traces to: AC-027, AC-029 (ADR-036)
- Actor: the test suite.
- Main flow: confirm each root cause with a 40-run loop before and after. Add a static `AtomicUsize` counter to the temp-directory name in `artifacts.rs`, `onboarding/projection.rs`, and `scope.rs` (kept only if a loop shows it failing). In `prerequisites.rs`, `check_timeout_is_reported` uses `sleep 5` and `ping -n 6` instead of `sleep 1` and `ping -n 3`.
- Alternate and error flows: a cause not confirmed by the loop is reported, not fixed blindly.
- Postconditions: only `#[cfg(test)]` code changes; production bytes are untouched.

### FN-039 Installers for both binaries
- Traces to: AC-026 (ADR-035; T2-22)
- Actor: the installer user.
- Main flow: see IF-020. `install.sh` requires `dtech-aidlc-mcp` next to the CLI binary, stages both, writes marker v2, and adds a second exec launcher with its own backup, rollback and recognition check. `install.ps1` copies the MCP exe into `lib` and a real `bin\dtech-aidlc-mcp.exe` through a temp and backup file. A preflight finds running processes under the prefix before any change. The post-commit removal of the old directory and backups becomes best-effort outside the try block (this fixes the rollback bug). Leftover `.dtech-aidlc-rust.old.*` and MCP backups are cleaned at the next run. The README sentence "stay CLI-only" changes in the same unit.
- Alternate and error flows: files in use give exit 1 with the fixed message and no change. Upgrade from a v1 prefix keeps unrecognised files. Uninstall removes only recognised files.
- Postconditions: the CLI keeps its `.cmd` launcher; `DTECH_AIDLC_ASSETS` remains a no-op.

### FN-040 Release 1.3.0 preparation
- Traces to: AC-023, AC-024, AC-027 (ADR-033)
- Actor: the release unit, last.
- Main flow: `Cargo.toml` version 1.3.0; the `Cargo.lock` package line (the only lock diff, regenerated with `cargo build --locked`); `.github/badges/version.svg`; `LOCK_SHA256` in `tests/mcp_packaging.rs` recomputed from the CR-stripped lock; one `## 1.3.0` CHANGELOG entry covering intents 1 and 2 (contents checklist in the "Release and golden contract" section of interfaces-and-contracts.md); the 7 goldens re-blessed for the version only.
- Alternate and error flows: any golden outside the list changing stops the unit. No tag, no publish, no push. `harness/` is untouched (CON14).
- Postconditions: `diff <(sed 's/1\.2\.2/1.3.0/g' old) new` is empty for each of the 7 goldens because the monitor and `/telemetry/` lines were blessed earlier.

### FN-041 README docs
- Traces to: AC-020, AC-025, AC-030 (ADR-034)
- Actor: a reader of the README.
- Main flow: new sections beside `## MCP server (proposed)`: telemetry (fields, kinds, meaning of `name`, `--telemetry off`, location, retention, "drops not recorded", "CLI writes no events", Windows ACL note); monitor (command, URL and token on stderr, loopback, read-only, same-user boundary, coverage limits); troubleshooting (forward SIGTERM in wrappers, the Windows "Terminate batch job" prompt, ACLs); the two AC-030 statements ("approver names are self-asserted", "telemetry is not audit"). DeepSeek stays "not verified".
- Alternate and error flows: every `dtech-aidlc` line in an `sh` block must be a command in help (existing test). The heading and the sentences about "proposed", platform and hosts stay until FN-043.
- Postconditions: new needles are checked by the new repo_content test.

### FN-042 Studio mock update and design pack
- Traces to: AC-011, AC-022 (ADR-038, ADR-039; Q15)
- Actor: the AI design reviewer, then the dashboard builder.
- Main flow: produce wireframes, the state matrix, tokens, a privacy map, the IF-017 read contract, rulings on ADR-039 readings 1, 2 and 8, the drops coverage line, gate counts without approve/reject, removal of chart click-to-filter, and the 320 versus 360 px width decision. Update `plans/mcp-server-enhancement/mcp-studio-server.html`: no Google Fonts, shipped tool names, "TELEMETRY MCP-ONLY - DEFAULT ON - OPT-OUT", operation events labelled MCP-caller only, in-process monitor guardrail, fragment token, retention values, ADR-013 approval wording, "proposed" kept.
- Alternate and error flows: none for the title. Owner decision (User-provided, 2026-11-28): rename "Orca MCP Studio" to "dtech-aidlc MCP Studio" in the `<title>` (line 2) and the header `<h1>` (line 130, keeping its `<br>` line break: "dtech-aidlc MCP<br>Studio"), confirming the ADR-038 default.
- Postconditions: written acceptance gates the query engine contract and the dashboard build.

### FN-043 Drop the proposed label
- Traces to: AC-025 (ADR-034; Q10, Q21)
- Actor: the owner, with evidence.
- Preconditions: both windows-latest and macos-latest CI legs green and the owner records a live Claude Code `tools/call`.
- Main flow: in one unit, change the README heading and the "proposed" paragraph, the platform and host sentences, every `tests/repo_content.rs` needle that references the proposed heading (including any added by WU-021), and the Studio-mock label.
- Alternate and error flows: without the evidence the unit is parked as owner-decision and the release notes say which phases are released.
- Postconditions: DeepSeek stays "not verified".

### FN-044 Intent-1 MCP follow-up characterisation tests
- Traces to: AC-027, AC-029 (ADR-036; Q12, Q17)
- Actor: the test suite (WU-015).
- Main flow: new `tests/mcp_followups.rs` covers the intent-1 MCP behaviour that had no test: AI-mode gate with the policy edited between calls, unsafe layout at launch and mid-session, CLI and MCP interleaving (sequential and concurrent), restart and shutdown mid-session. Every spawn passes `--telemetry off` and `--project <Tmp>`.
- Alternate and error flows: these are characterisation tests, expected green on arrival; a red result is a real bug, fixed in the unit only when the fix stays in the test file's scope, otherwise the unit parks as dep-unit for the owner.
- Postconditions: no production file changes in this function; it is separate from FN-038 (flaky-test fixes).

## 4. Data
- Entity: telemetry event (IF-012). Fields: schemaVersion, eventId, timestamp, component, caller, processId, sessionId, traceId, spanId, parentSpanId, event, name, intentId, stageId, unitId, durationMs, outcome, errorCode. Validation: closed enumerations, id charset and 64-char cap, line at most 16 KiB. Retention: 5 MiB per part, 7 days, 50 MiB total (CFG-008).
- Entity: store file `mcp-<start>-<pid>-<session8>-<part>.jsonl` under `.aidlc/telemetry/`. Retention as above; the folder is git-ignored twice (managed block and a self `.gitignore`).
- Entity: query result (IF-017), derived in memory only; the parse cache is keyed by (path, length, mtime) and never persisted.
- Entity: monitor token, 16 random bytes as 32 lowercase hex, kept in process memory and the one stderr line, never in argv, env, files or telemetry.
- Ids: sessionId from the OS random source; every eventId, traceId, spanId is `<sessionId>-<n>`.

## 5. Interfaces
Each IF has its exact contract in interfaces-and-contracts.md: IF-012 (event line and file layout), IF-013 (os_random), IF-014 (Sink), IF-015 (ToolHost v2 and traced_call), IF-016 (MCP launch), IF-017 (query), IF-018 (monitor HTTP), IF-019 (monitor CLI), IF-020 (installers). Errors the actor can observe: exit 1 with a fixed message (launch, monitor start, installer preflight), the ADR-032 status and code, a single fixed stderr line when telemetry disables itself.

## 6. Interaction
- MCP call: host to `serve` to `traced_call` to Sink (try_send) to writer thread to file; the reply goes to stdout first and independently.
- Protocol error: `serve` writes the reply, then calls `on_error`, which sends one event.
- Dashboard: browser to `monitor` to `query` to files; no writes anywhere on this path.
- Writer and reader never coordinate; the reader tolerates partial tails and prunes by skipping missing files.

## 7. State
- Sink: Active, Disabled (random failure, unsafe store, write failure, panic, disconnect). Active to Disabled only; no re-enable in the process.
- Writer: NotStarted (lazy), Open(part n), Rotating, Disabled.
- Monitor connection slot: Free, Held, released by Drop on any exit including a panic.
- Dashboard: the AC-017 state set (FN-036). Forbidden: a Disabled Sink changing replies or exit code; a monitor handler writing or deleting under `.aidlc`; the CLI constructing a Sink.

## 8. Dependencies
- std only; `serde_json` and `sha2` already present (cursor query hash). Zero new crates; `Cargo.lock` unchanged except the version line.
- bcrypt (Windows, FFI): failure is an error at start (Sink: telemetry off with one line; monitor: exit 1).
- Filesystem: write failure disables telemetry; read failure counts the file as unreadable.
- Git: absent means the git-ignore check test skips visibly.
- CI: windows-latest and macos-latest prove every Windows assumption (Assumption until then).
