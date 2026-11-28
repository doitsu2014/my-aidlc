# WU-016 Dashboard design pack: dtech-aidlc Monitor

- Unit: WU-016 (design unit, no code tests). Feeds WU-017 (IF-017 field names) and WU-019 (FN-036 assets, TS-030 manual checklist).
- Criteria served: AC-011, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022. Functions and contracts: FN-036, FN-042, FN-032; IF-012, IF-017, IF-018, IF-019; CFG-011, CFG-012. Decisions: ADR-021, ADR-029, ADR-030, ADR-031, ADR-038, ADR-039. Threats: T2-03, T2-14, T2-20, T2-24. Quality scenario: QS-10 (plus the architecture-design accessibility, privacy and percentile scenarios).
- Status: revised after the AC-022 AI design review, round 1. The reviewer rulings 1 to 12 are applied as ruled and issues 1 to 15 are fixed in this revision (section 7 lists where). Acceptance lives in the review record, not here. Nothing here amends an AC.
- Labels: **Observed** = read in the cited file in this tree; **Assumption** = believed, not checked; **Estimate** = a chosen number not measured against real sessions. All CFG-011 and CFG-012 numbers are Estimates (ADR-030).

## 0. Fixed constraints the design obeys

| Constraint | Source | Label |
|---|---|---|
| Three embedded assets only: `index.html`, `app.js`, `app.css`; routes `/`, `/app.js`, `/app.css`, plus `GET /api/events` | ADR-031, IF-018 | Observed |
| No remote asset of any kind; system font stacks; CSP `default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'` | ADR-031, IF-018, AC-011, T2-24 | Observed |
| GET only. The page sends only same-origin GET requests: the three static assets the HTML loads, plus `fetch('/api/events?...', {method: 'GET'})`. HEAD and OPTIONS get 405. The browser's automatic `GET /favicon.ico` is also a GET and gets 404 `not_found`; that adds no fourth asset (ADR-031 reopen trigger not met) | IF-018 row 4, ADR-039 reading 2, ADR-031 | Observed |
| No persistent or shared browser state: no localStorage, sessionStorage, cookies, IndexedDB, Cache API, service worker, `pushState`, window globals holding data | ADR-029, FN-036 | Observed |
| Token read once from `location.hash`, kept in a `const` inside an IIFE or a `type="module"` script (never a top-level `const` in a classic script, which is a global lexical binding), `history.replaceState(null, '', '/')` before the first fetch, sent only as `Authorization: Bearer <token>` on `/api/events` | ADR-029, FN-036, T2-03 | Observed |
| Data enters the DOM only through `textContent` (and `setAttribute` on non-URL attributes such as `aria-*`, `data-state`); no `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `eval`, `new Function`, inline handlers, string `setTimeout` | ADR-031, T2-14 | Observed |
| Filters live in memory only; the address stays `/` (no query string, no hash) | AC-019, ADR-029 | Observed |
| Poll 5 s; double up to 60 s only on 500 `internal_error`, 503 `busy` or 503 `store_unavailable`; never retry another 4xx (401 and 409 have their own states); pause while `document.hidden` and while older pages are loaded; stop on a network error or an unexpected response (manual retry only); stale after 15 s, only while auto-refresh is supposed to run; quiet after 15 min; liveness always "unknown"; setting always "unknown" | CFG-012, ADR-030, ADR-039 reading 1, T2-20 | Observed (values Estimate) |
| Page title `dtech-aidlc Monitor` | ADR-038 | Observed |

ADR-039 readings, as ruled in the AC-022 review:
- Reading 1 (accepted). With no trusted setting input (ADR-023), the header always shows "Setting: unknown". The page never shows "on" or "off", and an empty store says it "does not mean ... telemetry is off". This meets AC-001 scenario 2 and the AC-017 "off (trusted input only)" clause, which cannot fire in v1. AC-017 "shutting down" is met by the client-detected "Monitor stopped or unreachable" state. The privacy sentence "The MCP server records telemetry by default; start it with `--telemetry off`" states the documented default, not the effective setting, so it does not contradict reading 1.
- Reading 2 (accepted). Same-origin GET only, as in the table above. A same-origin request is never preflighted, whatever its headers (Observed against the Fetch same-origin rule), so the `Authorization` header raises no OPTIONS request.
- Reading 8 (accepted). A reload, a duplicated tab or a bare `/` shows "Token missing", makes no API call and keeps no filter state, because filters live only in memory and the address is always `/`. Reopening the printed address therefore starts with default filters; that is how AC-019 "Reload" is met (Met*, ADR-029). Back/forward-cache restoration is not a reload and is out of scope for this reading (Assumption: `Cache-Control: no-store` on `/` usually keeps the page out of bfcache).

Removed from the plan wireframes for v1 (FN-042, accepted): the charts themselves (latency distribution, events by time) and chart click-to-filter. No AC requires a chart or summary-click filtering, and the plan prefers a direct labelled value when it is clearer (Observed, mcp-server-detailed-design.md:314). Plain-text summary values meet AC-018 (denominators and n shown) with no new focus stops, no canvas or SVG text alternatives and no extra state. Summary values are not links or filter controls. Filtering stays in the Filters form plus "Show this trace" and "Show this session". Revisit only if a later intent adds an AC for charts.

## 1. Wireframes (ASCII)

Widths: "wide" is 960 px and up (two columns), "narrow" is below 960 px (one column). The 320 px drawings are the binding narrow layout (section 5).

### 1.1 Overview, wide (recent events, healthy)

```text
+--------------------------------------------------------------------------------------------+
| [Skip to events]                                                                            |
| dtech-aidlc Monitor   local project   (*) LOCAL ONLY   read-only        [Privacy and data v] |
| Setting: unknown   MCP process: unknown (not inferred from events)                          |
+--------------------------------------------------------------------------------------------+
| Range [Last 1 hour v]   14:05:12 to 15:05:12 (UTC+07:00)        [Refresh]                    |
| Status: Updated 15:05:12 . auto-refresh every 5 s                                           |
+--------------------------------------------------------------------------------------------+
| SUMMARY (whole filtered range, not only this page)                                          |
| Tool calls 128 | Blocked 4 of 128 | Errors 3 of 128 | Incomplete 1 (never counted as ok)    |
| Operations 131 (errors 2), separate population                                              |
| Gate-decided events 6 (telemetry, not audit; no approve/reject value)                       |
| Latency, completed tool calls: p50 42 ms . p95 240 ms . n = 128                             |
| Last event 15:04:58                                                                         |
| Coverage: files read 3 . skipped lines 0 . partial tails 0 . unreadable files 0 .           |
|           expired files 0 . Dropped events: not recorded . CLI writes no events .           |
|           kinds not observed: mcp.protocol.error                                            |
+--------------------------------------------------------------------------------------------+
| FILTERS                                                                                     |
| Source [Any v]  Name [Any v]  Outcome [Any v]  Stage [______]  Session [______]             |
| Trace [______________]                                         [Apply filters] [Clear]      |
+-----------------------------------------------------------+--------------------------------+
| EVENTS . newest first . 50 shown                          | EVENT DETAIL                   |
| Time (UTC+07:00) Src Event               Name  ms  Outcome | mcp.tool.completed . report    |
| > 15:04:58  MCP  mcp.tool.completed      next  42  v OK   | [/] Blocked                    |
| > 15:04:58  APP  workflow.stage_selected next  --  v OK   | (full detail: section 1.4)     |
| > 15:04:58  APP  operation.completed     next  38  v OK   |                                |
|#> 15:03:10  MCP  mcp.tool.completed      report 86 [/] Blocked                              |
| > 15:01:44  MCP  mcp.protocol.error      protocol -- x Error                                |
| ...                                                        |                                |
| [Load older events]                                        |                                |
+-----------------------------------------------------------+--------------------------------+
| dtech-aidlc monitor . loopback only . telemetry is not audit                                |
+--------------------------------------------------------------------------------------------+
```
Glyph key. The ASCII drawings use stand-ins; the page uses these exact code points, all with text presentation, each in a span with `aria-hidden="true"` and always followed by the word, which is the accessible name (never colour alone, never an emoji):

| Drawing | Code point | Character | Word |
|---|---|---|---|
| `v` | U+2713 | check mark | OK |
| `[/]` | U+2298 | circled division slash | Blocked |
| `x` | U+2715 | multiplication x | Error |
| `o` | U+25CB | white circle | Started (outcome null) |
| `!` | U+26A0 U+FE0E | warning sign, text variation selector | heading word of the banner |
| `i` | U+2139 U+FE0E | information source, text variation selector | heading word of the banner |

The no-entry sign U+1F6AB is not used: it renders as a colour emoji and ignores the status tokens and the computed contrast. Each row starts with a button (`>`), the only focus stop in the row. `#` marks the selected row: a 4 px left border in `--accent` plus `aria-current="true"` on its row button, on top of the `--surface-2` background (section 3.1).

### 1.2 Overview, 320 px

```text
+--------------------------------+
| [Skip to events]               |
| dtech-aidlc Monitor            |
| local project . (*) LOCAL ONLY |
| read-only                      |
| Setting: unknown               |
| MCP process: unknown           |
| [Privacy and data v]           |
+--------------------------------+
| Range [Last 1 hour      v]     |
| 14:05:12-15:05:12 UTC+07:00    |
| [Refresh]                      |
| Updated 15:05:12               |
+--------------------------------+
| SUMMARY (whole range)          |
| Tool calls      128            |
| Blocked         4 of 128       |
| Errors          3 of 128       |
| Incomplete      1              |
| Operations      131 (errors 2) |
|   separate population          |
| Gate-decided events 6          |
|   (telemetry, not audit; no    |
|   approve/reject value)        |
| Latency, completed tool calls: |
| p50 42 ms . p95 240 ms         |
| n = 128                        |
| Last event      15:04:58       |
| Coverage: files read 3 .       |
| skipped lines 0 . partial      |
| tails 0 . unreadable files 0 . |
| expired files 0 . Dropped      |
| events: not recorded . CLI     |
| writes no events . kinds not   |
| observed: mcp.protocol.error   |
+--------------------------------+
| [Filters (0) v]                |
+--------------------------------+
| (detail appears here when open)|
+--------------------------------+
| EVENTS . newest first . 50     |
| > 15:04:58  v OK               |
|   MCP mcp.tool.completed       |
|   next . 42 ms . a1b2c3d4..-17 |
| > 15:03:10  [/] Blocked        |
|   MCP mcp.tool.completed       |
|   report . 86 ms . a1b2..-12   |
| [Load older events]            |
+--------------------------------+
```
At narrow widths the filter form sits in a native `<details>` ("Filters (n)" shows the count of active filters). Each event row wraps to three lines; nothing scrolls sideways. The narrow layout carries the same content as the wide one, only wrapped: every summary value with its qualifier, and the full coverage line (files read, skipped lines, partial tails, unreadable files, expired files, "Dropped events: not recorded", "CLI writes no events", kinds not observed). Only the visual column headers are absent; the row button's accessible name gives the fields in a fixed order (section 5.2).

### 1.3 Sessions and events list with filters (filters open, trace filter active)

```text
+--------------------------------------------------------------------------------+
| FILTERS (2 active)                                                             |
| Source  [Any            v]   (Any | mcp | application)                         |
| Name    [Any            v]   (Any | status | queue | next | report |          |
|                               unknown | protocol)                              |
| Outcome [Any            v]   (Any | ok | blocked | error)                      |
| Stage   [_______________]    hint: lowercase letters, digits, '-', up to 64    |
| Session [a1b2c3d4e5f6...]    hint: 32 lowercase hex characters                 |
| Trace   [a1b2...-12_____]    hint: session id, '-', a number; up to 64         |
| [Apply filters] [Clear]                                                        |
| Active: session a1b2c3d4.. . trace a1b2c3d4..-12        (text, not chips)      |
+--------------------------------------------------------------------------------+
| EVENTS . newest first . 4 shown . trace a1b2c3d4..-12                          |
| > 15:03:10  MCP  mcp.tool.completed   report  86 ms [/] Blocked                |
| > 15:03:10  APP  operation.completed  report  80 ms [/] Blocked                |
| > 15:03:10  APP  operation.started    report  --    o Started                  |
| > 15:03:10  MCP  mcp.request.started  report  --    o Started                  |
| (end of results)                                                               |
+--------------------------------------------------------------------------------+
```
- Sessions: there is no separate sessions screen. Sessions are reached through the Session filter, and through "Show this session" in the detail (section 1.4). A list of observed sessions is not in v1 (skipped: IF-017 returns no session list; add `summary.sessions` only if the review asks for it).
- Apply runs one fresh query (no cursor). Clear resets every filter to Any/empty, keeps the range and runs one fresh query.
- Client-side check before any request, one rule per field, matching the Observed stored id charsets (rust-cli/mcp/tools.rs:73-74, IF-012), all subject to WU-017 confirming the same rules server-side:

  | Field | Rule | Inline error (also the hint) |
  |---|---|---|
  | Stage | `^[a-z0-9-]{1,64}$` | "Use lowercase letters, digits or '-', up to 64 characters" |
  | Session | `^[0-9a-f]{32}$` | "Use the 32 lowercase hex characters of a session id" |
  | Trace | `^[0-9a-f]{32}-[0-9]+$` and at most 64 characters | "Use a session id, '-', then a number, up to 64 characters" |

  A failing field shows its inline error, gets `aria-invalid="true"` and `aria-describedby` pointing at the error element, receives focus, and no request is sent. An empty field means "no filter". These rules also keep every query value free of characters that `URLSearchParams` would percent-encode, because the server rejects any `%` (IF-017). A value that passes here but fails on the server (400 `invalid_filter`) goes to the not-retried "Request failed" state (section 2), never into the retrying Error loop.
- Range presets: Last 15 minutes (900), Last 1 hour (3600, default), Last 24 hours (86400), All retained, up to 7 days (604800). No free-form range in the UI, so `range_too_large` is unreachable from the page (it stays reachable over HTTP for AC-016).
- Paging: "Load older events" sends the same query plus `cursor=<nextCursor>` and appends. It is absent when `nextCursor` is null ("End of results"). While older pages are loaded, auto-refresh pauses (accepted: cursor paging freezes `to`, IF-017, and a list moving under the reader is worse). The page is then in the user-paused state: the status says "Auto-refresh paused while older events are shown" with a [Back to newest] button (one fresh query, page 1, auto-refresh resumes). This paused state takes the place of the time-based Stale banner: stale-by-age never fires while paused (section 2). Apply, Clear, Refresh and a Range change also leave the paused state, because each runs one fresh query.

### 1.4 Event detail (wide: right column; narrow: inline section above the list)

```text
+--------------------------------------------+
| EVENT DETAIL                       [Close] |
| mcp.tool.completed . next                  |
| [/] Blocked                                |
| Time      15:03:10.412 (UTC+07:00)         |
|           2026-11-28T08:03:10.412Z         |
| Source    mcp      Caller   mcp            |
| Duration  86 ms                            |
| Error     none (outcome blocked)           |
| Intent    please-implement-...-sc-2        |
| Stage     build-and-verify                 |
| Unit      not recorded                     |
| Event id  a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6-31 |
| Session   a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6 |
| Trace     a1b2c3d4...-12                   |
| Span      a1b2c3d4...-13                   |
| Parent    no parent (root span)            |
| Process   48211                            |
|                                            |
| [Show this trace] [Show this session]      |
| [Go to parent]  (hidden for a root span)   |
+--------------------------------------------+
```
- Error row on `outcome = error`: the code and its fixed copy, for example `state_conflict: Workflow state conflict` (section 4.3). Never a message or a source string.
- Parent navigation: [Go to parent] looks for an event with `spanId = parentSpanId` in the loaded events; if absent it runs one query with `trace=<traceId>`, `limit=200` and the current range. Found: the detail switches to the parent (prefers the `*.completed` event of that span, else the `*.started`). Not found: the Parent row reads "parent not in store (expired, skipped, dropped or outside the range)" and the button is removed. `parentSpanId = null`: "no parent (root span)". The trace lookup query changes only the detail panel: it does not change the active filters, the visible list, the paging state or the summary.
- Show this trace sets the Trace filter and clears an active Session filter; Show this session sets the Session filter and clears an active Trace filter. The two buttons therefore never combine into an empty result by accident. Other active filters (Source, Name, Outcome, Stage) stay as they are and are listed in the "Active:" line.
- Long ids wrap inside the detail (`overflow-wrap: anywhere` on the id and code elements only, not the whole body); full ids are always visible as text so they can be selected and copied without a clipboard API.
- Close (button or Escape while focus is inside the detail) hides the section and returns focus to the row button that opened it. Rows are keyed by eventId and reused across refreshes (section 2, list rendering), so that button survives auto-refresh; only if its event has aged out of the loaded page does focus go to the Events heading.

### 1.5 Privacy panel (native `<details>`, closed by default)

```text
+--------------------------------------------------------------+
| [Privacy and data ^]                                         |
| What this monitor shows                                      |
|  Setting: unknown. The monitor cannot see whether telemetry  |
|  is on; it only reads what is already stored.                |
|  Location: .aidlc/telemetry/ in the local project.           |
|  Retention: ... (full text in section 4.4)                   |
|  Fields kept: ...                                            |
|  Never kept: ...                                             |
|  Dropped events: not recorded                                |
+--------------------------------------------------------------+
```
The panel never shows an absolute path, a username, the port or the token. It deliberately shows the generic relative location `.aidlc/telemetry/` that AC-020 requires. Its full text is fixed copy in `index.html` (section 4.4), not data from the API.

### 1.6 State wireframes (narrow drawings; wide uses the same banner above the summary)

Token missing (also reload, duplicated tab or bare `/`; no API call ever made). The header lines are the same as in 1.2 (`read-only`, `Setting: unknown`, `MCP process: unknown`); the drawing below abbreviates them. The skip link is not rendered in this state. The banner text is not in the initial HTML: app.js inserts it into the existing `role="status"` region after load, so screen readers announce it (a live region that already holds text at load is often not announced):
```text
+--------------------------------+
| dtech-aidlc Monitor            |
| local project . LOCAL ONLY     |
| [Privacy and data v]           |
+--------------------------------+
| ! Token missing                |
| Reopen the address printed in  |
| the terminal where you ran     |
| dtech-aidlc monitor.           |
| Reloading this page removes    |
| the token on purpose.          |
+--------------------------------+
```
Token rejected (HTTP 401, for example a tab left open across a monitor restart): same layout, heading "! Token rejected", same instruction, polling stopped, previous data removed from view, skip link hidden.

Empty (200, zero events, no filters):
```text
| i No telemetry observed        |
| in the last 1 hour.            |
| This does not mean the MCP     |
| process is absent or that      |
| telemetry is off.              |
| Coverage: files read 0 ...     |
| Dropped events: not recorded   |
| ... (full line, as in 1.2)     |
```
Empty with filters: "i No events match these filters in the last 1 hour." plus [Clear filters].

Loading (first request only; later refreshes keep data on screen):
```text
| Loading telemetry...           |   (text only, no spinner)
| (summary and list not drawn)   |
```

Error, retrying (500 `internal_error`, 503 `busy`):
```text
| ! Request failed: the monitor  |
| is busy.                       |
| Retrying in 10 s.  [Retry now] |
| Last data from 15:04:58 shown, |
| marked STALE.                  |
```
First-load variant (no data yet): the last two lines read "No data loaded yet."

Request failed, not retried (any 4xx with a known IF-018 code other than 401 `unauthorized` and 409 `cursor_stale`, for example 400 `invalid_filter`, 400 `range_too_large`, 408 `request_timeout`). The same GET is deterministic, so the page does not resend it and does not keep sending the token:
```text
| ! Request failed: a filter     |
| value was rejected.            |
| Change the filters or press    |
| Refresh.                       |
| Last data from 15:04:58 shown, |
| marked STALE.                  |
```
First-load variant: the last two lines read "No data loaded yet." Auto-refresh stops until a user action (Apply, Clear, Refresh, Range change, Back to newest).

Unavailable (503 `store_unavailable`):
```text
| ! Telemetry store unavailable  |
| The .aidlc/telemetry folder is |
| unsafe or unreadable now.      |
| Last-known data shown, marked  |
| STALE. Retrying in 20 s.       |
```
First-load variant: "No data loaded yet. Retrying in 20 s."


Stale (no failure yet, but last success older than 15 s while auto-refresh is supposed to run, for example just after the tab becomes visible again). Never shown in the user-paused state:
```text
| ! STALE: last updated 15:04:58 |
| (42 s ago). Refreshing...      |
```

Paused by the user (older pages loaded, section 1.3; replaces the Stale banner):
```text
| i Auto-refresh paused while    |
| older events are shown.        |
| [Back to newest]               |
```

Monitor stopped (network error; auto-refresh stops for good):
```text
| x Monitor stopped or           |
| unreachable.                   |
| Auto-refresh has stopped.      |
| [Retry once]                   |
| Last data from 15:04:58 shown, |
| marked STALE.                  |
```
First-load variant: the last two lines read "No data loaded yet."

Unexpected response (wrong content type, unparseable body, `schemaVersion` other than 1, or an error code the page does not know, for example an HTML 404 from a different process now on the same port, T2-20). Handled like Monitor stopped: auto-refresh stops, manual retry only, so the token is not sent again on its own:
```text
| x Unexpected response from     |
| the monitor address.           |
| Auto-refresh has stopped.      |
| [Retry once]                   |
| Last data from 15:04:58 shown, |
| marked STALE.                  |
```
First-load variant: the last two lines read "No data loaded yet."


Quiet (200 with data, `lastEventAt` older than 15 min before `to`):
```text
| i Quiet: no event in the last  |
| 15 min (last 14:41:03).        |
| Not a liveness claim.          |
```

Limit reached (200 with `limitReached`):
```text
| ! Partial result: a read limit |
| was reached (512 files, 64 MiB |
| or 250,000 events). Newest     |
| data shown; summary is partial.|
| Narrow the range or filters.   |
```

Stale cursor (409 `cursor_stale` on Load older):
```text
| i Results changed since this   |
| page was loaded. Showing the   |
| newest events again.           |
```

Corrupt records (200, `coverage.skippedLines > 0` or `partialTails > 0`): no banner; the coverage line marks the count with `!` and the words "skipped lines 3 (malformed or unknown records not shown)". Valid rows stay visible. No liveness claim.

## 2. QS-10 state matrix

Precedence when several hold (one banner, the highest wins; data qualifiers combine below it): token missing / token rejected > monitor stopped / unexpected response > unavailable > error (retrying) / request failed (not retried) > paused by the user > stale > loading. Qualifiers that can show together with any data state: limit reached, corrupt records, quiet, empty. Setting unknown and liveness unknown are always shown in the header. Stale by age applies only while auto-refresh is supposed to run; in the paused, stopped and not-retried states the banner of that state is shown instead, and the data is marked stale only where that state's row says so.

Focus rule: the page never moves focus on its own for a timer-driven change. It moves focus only after a user action (open detail, close detail, Apply, Clear, Load older, Back to newest, Retry, a blocked filter, a filter rejected by the server after Apply).

Announcement rule (AC-021 "announced once"): one `role="status"` region (`aria-live="polite"`) speaks only (a) the result of a manual Refresh, Apply, Clear, Range change, Load older or Back to newest, for example "Updated 15:05:12, 50 events", and (b) a state transition, such as entering stale, paused, error, request failed, unavailable, monitor stopped, unexpected response, quiet, empty, limit reached or stale cursor. An auto-refresh that only changes the event count or the summary is never announced, so an active session does not speak every 5 s.

List rendering rule (AC-021 keyboard operability): row elements are keyed by eventId. A refresh reuses the existing node for every eventId still in the page, inserts new rows before them, and removes only rows whose events left the page; it never rebuilds the `<ol>` and never replaces the focused element. If the focused row button is removed (its event aged out of the page), focus moves to the Events heading (`tabindex="-1"`). The selected row keeps `aria-current="true"` across refreshes.

| State | Trigger | What the page shows (text, glyph) | API call | Polling | Keyboard focus | Announced |
|---|---|---|---|---|---|---|
| Token missing | No `#token=<32 hex>` at load (first load without it, reload, duplicated tab, bare `/`, malformed) | `!` "Token missing" with the reopen instruction; header and privacy panel only; no skip link, filters, summary or list | None, ever | Never starts | Initial focus stays at document start; the skip link is not rendered, so the first Tab reaches the Privacy and data summary | Yes, once: app.js inserts the text into the empty status region after load |
| Token rejected | 401 `unauthorized` | `!` "Token rejected" with the reopen instruction; previous data removed from view; skip link hidden | The failed GET only | Stops | Unchanged (timer) or on the triggering button | Yes, once |
| Loading | First GET in flight | "Loading telemetry..." text; `aria-busy="true"` on the main region | `GET /api/events?range=3600&limit=50` | Next poll scheduled after the response | Start of document | No (the result is announced) |
| Recent events | 200 with events | Summary, coverage line, list newest first, last event time | One GET per poll, first page, current filters | Every 5 s while visible and not paused | Unchanged: rows keyed by eventId and reused, focused row kept (list rendering rule); open detail keeps focus | Only manual results and state transitions (announcement rule); never a count-only auto-refresh |
| Empty | 200, `events` empty and `summary.lastEventAt` null | `i` "No telemetry observed in the last <range>" plus "This does not mean the MCP process is absent or that telemetry is off"; with filters: "No events match these filters" plus [Clear filters] | Polls continue | Every 5 s | Unchanged | Yes, once on entering |
| Limit reached | 200 with `limitReached: "query_limit_reached"` | `!` "Partial result" banner; summary values, including `eventKinds` counts, suffixed "(partial)" | As normal | Every 5 s | Unchanged | Yes, once on entering |
| Corrupt records | 200 with `skippedLines > 0` or `partialTails > 0` | Coverage counts marked `!` with "not shown" wording; valid rows kept | As normal | Every 5 s | Unchanged | No (visible text only) |
| Paused by the user | Older pages loaded (Load older succeeded) | `i` "Auto-refresh paused while older events are shown" plus [Back to newest]; replaces the Stale banner; data not marked stale | Only user-triggered GETs (Load older, Back to newest, Refresh, Apply, Clear, Range change) | Paused; any fresh query resumes 5 s polling | Unchanged; Load older moves focus to the first new row | Yes, once on entering |
| Stale | Last success older than 15 s while auto-refresh is supposed to run (not paused, stopped or waiting for a user action), for example just after the tab becomes visible | `!` "STALE: last updated <time> (<n> s ago)"; data kept | Next scheduled GET (immediately when the tab becomes visible) | Continues per its state | Unchanged | Yes, once on entering |
| Error (retrying) | 500 `internal_error`, 503 `busy` | `!` "Request failed: <fixed copy of code>" plus "Retrying in <n> s" and [Retry now]; last data kept and marked stale, or "No data loaded yet" on first load | Retries the same GET | Interval doubles 5, 10, 20, 40, 60 s (cap); a success resets to 5 s | Unchanged; [Retry now] keeps focus when used | Yes, once per error code change |
| Request failed (not retried) | Any 4xx with a known IF-018 code other than 401 and 409, for example 400 `invalid_filter`, 400 `range_too_large`, 408 `request_timeout` | `!` "Request failed: <fixed copy>", for example "a filter value was rejected", plus "Change the filters or press Refresh"; last data kept and marked stale, or "No data loaded yet" on first load | None until a user action | Stopped until Apply, Clear, Refresh, Range change or Back to newest | On the filter form only if a user action (Apply) caused it; otherwise unchanged | Yes, once |
| Unavailable | 503 `store_unavailable` | `!` "Telemetry store unavailable" (unsafe or unreadable); last-known data marked stale, or "No data loaded yet" on first load | Retries the same GET | Backoff as Error (retrying) | Unchanged | Yes, once |
| Stale cursor | 409 `cursor_stale` on Load older | `i` "Results changed since this page was loaded. Showing the newest events again." | One fresh GET without cursor | Resumes at 5 s | Events heading (user had pressed Load older) | Yes, once |
| Monitor stopped | `fetch` rejects (network error) | `x` "Monitor stopped or unreachable. Auto-refresh has stopped." [Retry once]; data marked stale, or "No data loaded yet" on first load | None until [Retry once]; that sends one GET | Stopped (T2-20) | Unchanged; [Retry once] keeps focus | Yes, once |
| Unexpected response | Wrong content type, unparseable body, `schemaVersion` other than 1, or an unknown error code (for example an HTML 404 from another process now on the port after the tab was hidden) | `x` "Unexpected response from the monitor address. Auto-refresh has stopped." [Retry once]; data marked stale, or "No data loaded yet" on first load | None until [Retry once]; that sends one GET | Stopped, as Monitor stopped (T2-20) | Unchanged; [Retry once] keeps focus | Yes, once |
| Quiet | 200, `lastEventAt` not null and more than 15 min before `to` (server clock, not browser clock) | `i` "Quiet: no event in the last 15 min (last <time>). Not a liveness claim." | As normal | Every 5 s | Unchanged | Yes, once on entering |
| Hidden tab | `document.hidden` true | Nothing new (not visible) | None | Paused; on `visibilitychange` to visible, one GET at once (unless paused by the user or stopped); its reply goes through the same handling, so a foreign reply lands in Unexpected response | Unchanged | No |
| Setting unknown | Always | Header "Setting: unknown" (never "on" or "off", ADR-039 reading 1) | n/a | n/a | n/a | No |
| Liveness unknown | Always | Header "MCP process: unknown (not inferred from events)" | n/a | n/a | n/a | No |

Every row is distinguishable without colour: each banner has a glyph and a heading word. Every API call in the matrix is the same GET route; there is no other request.

## 3. Design tokens

All tokens are CSS custom properties on `:root` in `app.css`, redefined under `@media (prefers-color-scheme: dark)`. No theme toggle (it would need storage to persist and adds nothing). Contrast ratios are computed with the WCAG 2.x relative-luminance formula in this session (Observed, scratch computation, not a tool audit). Target: WCAG AA, 4.5:1 for text, 3:1 for control borders and the focus ring. Stated as a design target, not a conformance claim (quality requirements, Accessibility).

### 3.1 Colour

| Token | Use | Light | Dark | Contrast light (on bg / surface / surface-2) | Contrast dark (same) |
|---|---|---|---|---|---|
| `--bg` | page background | `#f6f8f8` | `#0f1c22` | n/a | n/a |
| `--surface` | panels, detail | `#ffffff` | `#15262d` | n/a | n/a |
| `--surface-2` | header band, selected row | `#eaf0ef` | `#1d3139` | n/a | n/a |
| `--ink` | body text | `#14262d` | `#e4efed` | 14.65 / 15.61 / 13.54 | 14.77 / 13.27 / 11.52 |
| `--muted` | secondary text, hints | `#4a5f66` | `#a1b8bb` | 6.31 / 6.73 / 5.84 | 8.34 / 7.49 / 6.51 |
| `--line` | control borders, dividers | `#6f8488` | `#627d83` | 3.69 / 3.94 / 3.42 | 3.95 / 3.55 / 3.08 |
| `--accent` | buttons, links, headings marks | `#006b66` | `#53d3c2` | 5.98 / 6.37 / 5.53 | 9.48 / 8.51 / 7.39 |
| `--on-accent` | text on accent buttons (= `--bg`) | `#f6f8f8` | `#0f1c22` | 5.98 on accent | 9.48 on accent |
| `--ok` | OK glyph and word | `#1d6b33` | `#6fd08c` | 6.15 / 6.56 / 5.68 | 9.16 / 8.22 / 7.14 |
| `--blocked` | Blocked glyph and word | `#8a4b00` | `#f0b45a` | 6.38 / 6.80 / 5.90 | 9.41 / 8.45 / 7.34 |
| `--error` | Error glyph and word, invalid field | `#b3261e` | `#ff8a80` | 6.13 / 6.54 / 5.67 | 7.61 / 6.83 / 5.93 |
| `--focus` | focus ring | `#005fcc` | `#7cb7ff` | 5.61 / 5.98 / 5.19 | 8.33 / 7.48 / 6.50 |

Every text pair is at least 4.5:1 (lowest 5.19:1) and every border or ring at least 3:1 (lowest 3.08:1) in both schemes; the reviewer recomputed every ratio with the WCAG 2.x formula and they match (Observed, AC-022 review ruling 12). Status colour is always paired with a glyph and a word (AC-021). Banners use `--surface` with a 4 px left border in the status colour plus the glyph; no filled red or yellow backgrounds.

Non-colour cues (AC-021, colour never alone):
- Selected row: `--surface-2` background plus a 4 px left border in `--accent` (5.53:1 light, 7.39:1 dark against `--surface-2`) plus `aria-current="true"` on its row button. Only one row carries it at a time.
- Invalid field: 2 px border in `--error`, `aria-invalid="true"`, and `aria-describedby` pointing at the inline error text, which starts with the U+2715 glyph (`aria-hidden="true"`) followed by the words.
- Status glyphs: the exact code points in section 1.1, text presentation only, so they take the status colour tokens and the contrast above.

### 3.2 Type (system fonts only)

- `--font-ui: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`
- `--font-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace` (ids, codes)
- Times and durations use `font-variant-numeric: tabular-nums` so columns align.

| Step | Size | Line height | Use |
|---|---|---|---|
| `--fs-xs` | 0.75rem (12 px) | 1.4 | coverage line, hints |
| `--fs-sm` | 0.875rem (14 px) | 1.45 | rows, detail labels |
| `--fs-md` | 1rem (16 px) | 1.5 | body, controls (16 px avoids iOS zoom on focus, Assumption) |
| `--fs-lg` | 1.25rem (20 px) | 1.3 | section headings (h2) |
| `--fs-xl` | 1.5rem (24 px) | 1.2 | page title (h1) |

Weights: 400 and 600 only. Sizes in rem so browser zoom and text size settings apply.

### 3.3 Spacing, shape, motion

- Spacing scale: `--sp-1` 4 px, `--sp-2` 8 px, `--sp-3` 12 px, `--sp-4` 16 px, `--sp-5` 24 px, `--sp-6` 32 px.
- Side gutter: 16 px at every width. Max content width 1200 px.
- Radius `--radius` 4 px. Border 1 px `--line`.
- Focus: `:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }` on every interactive element; never `outline: none` without this replacement.
- Hit target: buttons and selects at least 44 px tall at narrow width (Estimate, platform guidance).
- Motion: none. No spinner, no transition, no animated chart. `@media (prefers-reduced-motion: reduce)` sets `scroll-behavior: auto` as a guard, and `scrollIntoView` is called with `behavior: 'auto'`.

## 4. Privacy map

### 4.1 Field map (IF-012 field to page)

The API returns only IF-012 objects re-serialised from allowlisted fields (IF-017, AC-016); the page adds no other data source. "Shown" = where the field appears.

| IF-012 field | Shown | Display form | Redaction / handling |
|---|---|---|---|
| schemaVersion | Not shown | n/a | Always 1; the page ignores events whose value is not 1 (defence in depth; the server already filters) |
| eventId | Detail | full text, mono | Validated id `<sessionId>-<n>` (server); no further redaction |
| timestamp | List (local `HH:MM:SS`), detail (local with ms and offset, plus the exact UTC string) | `Date` parse for local display | None needed |
| component | List (`MCP` or `APP`), detail (`mcp` / `application`), Source filter | fixed label map | Closed set; unknown value never reaches the page (server) |
| caller | Detail | `mcp` | Always `mcp` in v1 |
| processId | Detail only | integer | OS pid; not identifying beyond this machine (accepted in architecture-design Privacy, Observed) |
| sessionId | Detail full; list never; Session filter | mono, wraps | Random 32 hex; no redaction |
| traceId | List shortened (`first 8 hex..-n`), detail full; Trace filter | mono | Random-derived; shortening is visual only |
| spanId | Detail | mono | as traceId |
| parentSpanId | Detail ("no parent (root span)" when null; "parent not in store ..." when not found) | mono | as traceId |
| event | List, detail heading | literal kind string | Closed set of 8 |
| name | List, detail heading, Name filter | literal (`status`, `queue`, `next`, `report`, `unknown`, `protocol`) | Closed set; the raw unknown tool name is never stored (IF-012), the page shows `unknown` |
| intentId | Detail ("not recorded" when absent) | literal, wraps | Validated slug, at most 64 chars; slugs of user text, owner-accepted (T2-10, owner-accepted). Shown, not redacted |
| stageId | Detail; Stage filter | literal | same |
| unitId | Detail ("not recorded" when absent) | literal | same |
| durationMs | List and detail (`42 ms`; `--` when null) | integer + unit | No fabricated duration for null (point events) |
| outcome | List and detail: glyph + word (`OK`, `Blocked`, `Error`; `Started` for null) | fixed label map | Closed set |
| errorCode | Detail when outcome is `error` | code plus fixed copy (4.3) | Closed set; never a message; an unrecognised code shows "unrecognised code" plus the code as text |

Also shown, from IF-017 not IF-012: `range`, `to`, every `summary` and `coverage` counter, `nextCursor` (never displayed, only sent back), `limitReached`.

Never shown and never available to the page: project path, username, hostname, port (beyond what the browser address bar shows), token, file names, raw lines, `detail`, `approved_by`, approval notes, messages, argv, env, frames (IF-012 "Never stored", IF-018). The header label is the fixed text "local project" (AC-020 "Project label").

### 4.2 Token handling (page side, T2-03)

```js
// app.js, top of the module closure (shape for WU-019, not final code)
const TOKEN = (() => {
  const m = /^#token=([0-9a-f]{32})$/.exec(location.hash);
  history.replaceState(null, '', '/');   // always, even when malformed
  return m ? m[1] : null;
})();
```
- The fragment is stripped on every load, including a malformed one. `TOKEN` is never written to the DOM, a global, storage or the console.
- Fetch options: `method: 'GET'`, `headers: {Authorization: 'Bearer ' + TOKEN}`, `cache: 'no-store'`, `credentials: 'omit'`, `redirect: 'error'`, `referrerPolicy: 'no-referrer'`; relative URL `/api/events?...` only. Static assets are loaded by the HTML with no token.
- Residual (T2-03, Assumption, untested in a browser): global history may keep the original `#token=` URL; the AC-030 review rates it. The page cannot remove it.

### 4.3 Error code fixed copy

| Code | Copy |
|---|---|
| `invalid_arguments` | Arguments were rejected |
| `approver_required` | An approver name was required |
| `state_conflict` | Workflow state conflict |
| `operation_failed` | Operation failed |
| `unknown_tool` | Unknown tool requested |
| `internal_error` | Internal error |
| `line_too_long` | Request line too long |
| `parse_error` | Request was not valid JSON |
| `invalid_request` | Invalid request |
| `method_not_found` | Method not found |
| `invalid_params` | Invalid parameters |
| `unsupported_protocol_version` | Unsupported protocol version |
| `protocol_error` | Protocol error |

HTTP error codes (IF-018) for the banner, with the state each one enters (section 2):

| Code | Copy | State |
|---|---|---|
| `unauthorized` (401) | Token rejected | Token rejected |
| `cursor_stale` (409) | Results changed since this page was loaded | Stale cursor |
| `store_unavailable` (503) | Telemetry store unavailable | Unavailable (retrying) |
| `busy` (503) | the monitor is busy | Error (retrying) |
| `internal_error` (500) | monitor internal error | Error (retrying) |
| `invalid_filter` (400) | a filter value was rejected | Request failed (not retried) |
| `range_too_large` (400) | range too large | Request failed (not retried) |
| `request_timeout` (408) | request timed out | Request failed (not retried) |
| `bad_request`, `header_too_large`, `method_not_allowed`, `host_rejected`, `origin_rejected`, `not_found` (4xx) | bad request | Request failed (not retried); none is reachable from a correct page |
| any other code, a non-JSON body, a content type other than `application/json`, or a 200 whose `schemaVersion` is not 1 | unexpected response | Unexpected response (stops, manual retry only) |

The body is read only as JSON with a known `error` value; anything else is "unexpected response".

### 4.4 AC-020 disclosure text (fixed copy in `index.html`, privacy panel)

> **Privacy and data**
>
> **Setting: unknown.** This monitor only reads what is already stored. It cannot tell whether telemetry is on or off for your AI host. The MCP server records telemetry by default; start it with `--telemetry off` to record nothing. The command-line tool (CLI) writes no telemetry.
>
> **Location:** `.aidlc/telemetry/` inside the local project. The folder is ignored by git. On macOS and Linux the files are readable only by your user account; on Windows they inherit the folder's permissions.
>
> **Retention:** each file is at most 5 MiB, then a new file starts. Files older than 7 days are deleted, then the oldest files are deleted until the folder is under 50 MiB in total. Deletion happens only when an MCP server with telemetry on starts writing or starts a new file; this monitor never deletes anything. Expired files still present are counted but not shown.
>
> **Fields kept per event:** schema version, event id, time, source (mcp or application), caller (mcp), process id, session id, trace id, span id, parent span id, event kind, tool or operation name (status, queue, next, report, unknown, protocol), intent id, stage id and unit id when valid, duration in milliseconds, outcome (ok, blocked, error) and a fixed error code.
>
> **Never kept:** prompts, tool arguments and results, artifact or document content, the `detail` text, approver names and approval notes, messages, file paths including the project path, command lines, environment variables, credentials, raw protocol frames, and unknown tool names. Gate events record that a gate decision happened, not who approved it or whether it was approved: telemetry is not audit, and approver names are self-asserted labels, not authentication.
>
> **Dropped events: not recorded**
>
> If the disk is slow the MCP server may drop events instead of slowing your assistant. Drops are not written to the store, so counts can be lower than the real number of calls. A call whose completion was dropped shows as incomplete, never as successful.
>
> **What this page cannot show:** whether an MCP process is running now, events from the CLI, events that were dropped, skipped (malformed) records, and events older than 7 days.
>
> This page runs only on 127.0.0.1, reads only, sends nothing off this machine, and stores nothing in your browser. Reloading the page removes its access token; reopen the address printed in the terminal.

Facts behind the text: retention CFG-008 and ADR-020; drops ADR-021; CLI writes none, owner decision guarded by TS-023; fields IF-012; gate events IF-012 and T2-17; Windows ACLs ADR-020 (Assumption until windows-latest CI). The exact line `Dropped events: not recorded` appears in this panel and in the coverage line of every data state (FN-036, ADR-021); the panel explanation is accepted as the AC-020 drops disclosure (AC-022 review ruling 6).

## 5. Narrow-width layout and keyboard flow

### 5.1 Width decision (binding: 320 px)

- Binding target, as ruled: no horizontal page scroll down to **320 px** CSS width. Reasons: (1) the quality requirements (Accessibility), QS-14, FN-036 and architecture-design s5 all say 320; (2) AC-021's 360 px scenario is a lower bar, so a layout with no horizontal scroll at 320 also has none at 360, and 320 meets AC-021 in full without amending it; (3) 320 CSS px is the standard reflow width (1280 px at 400% zoom), which the zoom note below relies on.
- Breakpoint: one, at 960 px (`@media (min-width: 60rem)`). Below it: one column, filters inside `<details>`, rows wrap to three lines, detail is an inline section placed above the list in DOM and flow. At and above it: summary in a 4-column grid, list plus a 360 px sticky detail column, and the Filters `<details>` is open (section 5.4) so no filter is hidden.
- Rules that guarantee no horizontal scroll: `overflow-wrap: anywhere` on id and code elements only (not the whole body); no fixed widths above 288 px (320 minus two 16 px gutters) at narrow width; form controls `width: 100%` at narrow width; the summary is a `<dl>` that wraps; no `<table>` (the list is an ordered list of row buttons, so no table can force a minimum width); `min-width: 0` on grid children.
- Zoom: the 320 px layout is also what a 1280 px window shows at 400% zoom, so reflow is covered by the same rules (Assumption, standard CSS px behaviour).

### 5.2 Landmarks and names

`<header>` (banner: title, project label, setting, liveness, privacy `<details>`), `<main>` with `<h1>`-less sections each with an `<h2>`: "Range and refresh" (`<section aria-labelledby>`), "Summary" (`<dl>`), "Filters" (`<form aria-label="Event filters">`, every control has a `<label>`), "Event detail" (`<section aria-labelledby>`, hidden when empty), "Events" (`<ol aria-label="Events, newest first">`), `<footer>`. One `role="status"` region at the top of `<main>`, present in every state including Token missing and Token rejected, and empty in the initial HTML (the Status line drawn in the Range band of 1.1 is this region). The `<h1>` is the page title in the header.

Row button accessible name. The list is not a `<table>`, so the visual column headers carry no semantics. Each row button therefore gets an `aria-label` (a non-URL attribute set with `setAttribute`) built from the event's values in this fixed order, comma-separated: time, source, event, name, duration, outcome. Example: "15:03:10, MCP, mcp.tool.completed, report, 86 ms, Blocked". A null duration reads "no duration"; a null outcome reads "Started". The order is the same at every width, whatever the visual wrapping; the glyph spans are `aria-hidden="true"`, and the selected row adds `aria-current="true"`.

### 5.3 Keyboard flow (Tab order equals DOM order)

1. Skip link "Skip to events" (first focusable; jumps to the Events heading, `tabindex="-1"`). Rendered only when the Events heading is rendered; in Token missing and Token rejected it is hidden (`hidden` property), so the first Tab reaches the Privacy and data summary.
2. Privacy and data `<summary>` (Enter or Space toggles; native).
3. Range `<select>` (arrow keys choose; change runs one fresh query; the page does not move focus).
4. Refresh button (one GET; result announced once; focus stays).
5. Retry now / Retry once / Back to newest (only present in their states).
6. Filters `<summary>` at narrow width; then Source, Name, Outcome selects, Stage, Session, Trace inputs, Apply filters (Enter in any input also applies, through the form's `submit` event), Clear.
7. Event detail controls when open: Close, Show this trace, Show this session, Go to parent.
8. Row buttons, one per event, in list order; Enter or Space opens the detail and moves focus to the detail heading. Auto-refresh keeps the focused row button in place (list rendering rule, section 2); if its event ages out of the page, focus moves to the Events heading.
9. Load older events (after append, focus moves to the first new row button).
10. Footer has no focusable element.

- Escape inside the detail closes it and returns focus to the opening row button (AC-021 "focus returns after closing detail").
- Show this trace / Show this session set their filter (clearing the other one, section 1.4), run one fresh query, keep the detail open on the same event, and move focus to the Events heading with an announcement "Filtered to trace ..., 4 events".
- No keyboard trap: the detail is not modal; Tab leaves it normally.
- Manual browser checklist for WU-019 (TS-030): every step above with keyboard only, at 320 px, at 360 px with an error state shown (the literal AC-021 scenario) and at 1280 px, in light and dark schemes, in a forced-colors mode (for example Windows High Contrast or the browser's forced-colors emulation: borders, focus ring, selected-row border and glyphs stay visible), with `prefers-reduced-motion: reduce`, plus:
  - keyboard focus on a row button survives 3 auto-refreshes in an active session, and the status region speaks nothing during them;
  - each QS-10 row forced: token missing (open `/`; the first Tab reaches the Privacy summary; the banner is announced); monitor stopped (kill the monitor; also on first load); unexpected response (stop the monitor while the tab is hidden, serve a non-JSON 404 on the same port, show the tab); unavailable (make `.aidlc/telemetry` unreadable; also on first load); request failed (send a stage value that passes the client check but the server rejects, if any, or force a 400 with a test server; no retry follows); paused (Load older, wait more than 15 s; no Stale banner); limit reached (the 513-file fixture); empty (empty store); quiet (fixture with a 20-minute-old event); corrupt (fixture with a malformed line).

### 5.4 Implementation notes for WU-019

- Keep `TOKEN` inside an IIFE or a `type="module"` script; a top-level `const` in a classic script is a global lexical binding.
- Handle Apply through a `submit` listener on the form that calls `preventDefault()`; the CSP `form-action 'none'` blocks native submission.
- Never use `setAttribute('style', ...)` or inline `style` attributes, which `style-src 'self'` blocks; toggle classes or the `hidden` property instead.
- Scope `overflow-wrap: anywhere` to id and code elements, not the whole body.
- The Filters `<details>` carries `open` in `index.html`; at load, app.js closes it only when `matchMedia('(min-width: 60rem)')` does not match, and reopens it when the query starts matching. Content is never hidden at 960 px and above, and a script failure leaves the form open.
- Render the list keyed by eventId (section 2, list rendering rule); never replace the `<ol>` or the focused node.
- Insert the Token missing text into the status region after load, not in the initial HTML.
- Response ordering (acceptance-review nit): give every `/api/events` request a sequence number and abort the previous in-flight request with `AbortController` when Apply, Clear, Range change, Load older, Back to newest or Refresh starts a new one; ignore any reply whose sequence number is not the latest, so a late auto-refresh can never overwrite a newer result or leave the paused state with a page-1 list.

## 6. IF-017 response field names (final list for WU-017)

As ruled in the AC-022 review: every provisional IF-017 name is kept, and `summary.eventKinds` is added. Field names only; types, rules and status codes stay as IF-017 states.

| Field | Status | Page use |
|---|---|---|
| `schemaVersion` | keep | page refuses to render a response whose value is not 1 ("unexpected response") |
| `range` | keep | echo check; range label |
| `to` | keep | "from - to" label (from = `to` minus `range`); quiet test uses `to`, not the browser clock |
| `events[]` (IF-012 objects, optional `intentId`, `stageId`, `unitId` omitted when absent) | keep | list and detail |
| `summary.toolCalls` | keep | Tool calls |
| `summary.blocked.count`, `summary.blocked.of` | keep | "Blocked n of m" |
| `summary.errors.count`, `summary.errors.of` | keep | "Errors n of m" |
| `summary.incomplete` | keep | Incomplete |
| `summary.operations.count`, `summary.operations.errors` | keep | Operations (separate population) |
| `summary.latency` as `{n, p50, p95}` or `{n, insufficient: true}` | keep | "p50 . p95 . n" or "insufficient samples (n = 7, needs 20)" (n >= 20 rule, CFG-011) |
| `summary.lastEventAt` (ISO ms or null) | keep | Last event; empty and quiet tests |
| `summary.eventKinds` | **added (accepted)** | object whose keys are exactly the 8 IF-012 event kind strings, each always present, each a count of distinct eventIds of that kind in the filtered range (zeros included; "(partial)" applies under `limitReached`). Gives "Gate-decided events n (telemetry, not audit)" from `workflow.gate_decided`, with no approve/reject split because IF-012 stores none (T2-17, FN-042), and the coverage line "kinds not observed" (FN-036). Needed because the page sees only one page of events, so neither value can be computed client-side from the other fields. The kinds-not-observed text is shown only when no filter other than range is active, because a filter makes other kinds zero by construction |
| `coverage.filesRead`, `.skippedLines`, `.partialTails`, `.unreadableFiles`, `.expiredFiles` | keep | coverage line |
| `nextCursor` (hex or null) | keep | Load older; null means end of results |
| `limitReached` (key present only when a cap was hit, value `query_limit_reached`) | keep | Partial result banner |

Error bodies stay `{"error":"<code>"}` (IF-018). The page needs no other route, no session list and no per-file data.

Latency population (WU-017 must state it in IF-017). IF-017 and the plan (Observed, mcp-server-detailed-design.md:238 and :315) define latency over completed MCP tool calls, and `toolCalls` counts distinct `mcp.tool.completed` eventIds. The pack therefore takes `summary.latency.n` to equal `summary.toolCalls` (all completed tool calls, every outcome; Assumption). The wireframes now show n = 128 against Tool calls 128; the earlier n = 124 implied another population. If WU-017 excludes any outcome or any completion without a duration, it names the excluded outcomes in IF-017 and the page label changes to match (Assumption until WU-017 records it).

Id filter rules for WU-017 to confirm server-side (the page applies them client-side, section 1.3): stage `^[a-z0-9-]{1,64}$`, session `^[0-9a-f]{32}$`, trace `^[0-9a-f]{32}-[0-9]+$` with at most 64 characters (Observed charsets: rust-cli/mcp/tools.rs:73-74, IF-012).

## 7. Review round 1: where each item landed, and what stays open

Rulings applied: 1, 2, 3 (ADR-039 readings) in section 0; 4 (320 px binding) in 5.1 and the TS-030 checklist in 5.3; 5 (IF-017 final list, `summary.eventKinds`, latency population) in section 6 and the wireframes; 6 (drops line) in 1.1, 1.2, 1.5 and 4.4; 7 (gate-decided events, qualifier at both widths) in 1.1, 1.2 and 6; 8 (no charts, no click-to-filter) in section 0; 9 (pause while paging) in 1.3, 1.6 and 2; 10 (per-field id checks) in 1.3 and 6; 11 (privacy text, optional "approver names are self-asserted" added) in 4.4; 12 (tokens) in 3.1.

Issues fixed: 1 announcement rule (section 2); 2 list rendering rule and TS-030 focus check (2, 5.3); 3 paused state in the matrix and precedence (1.6, 2); 4 not-retried Request failed state and per-field checks and hints (1.3, 1.6, 2, 4.3); 5 Studio mock (below); 6 first-load variants (1.6, 2); 7 Unexpected response state (1.6, 2, 4.3); 8 selected-row border plus `aria-current`, `aria-describedby` on invalid fields (1.1, 1.3, 3.1); 9 exact glyph code points (1.1); 10 skip link hidden and status text inserted after load (1.6, 2, 5.3); 11 narrow wireframe content (1.2); 12 row accessible name order (5.2); 13 parent lookup scope and trace/session filter exclusivity (1.4); 14 implementation notes and forced-colors check (5.3, 5.4); 15 "absolute path" (1.5).

Studio mock (issue 5, FN-042, ADR-038): made in this unit run in `plans/mcp-server-enhancement/mcp-studio-server.html` (Observed in the working tree, not committed): shipped tool names `status`, `queue`, `next`, `report` in shipped order (tool list, guardrail heading, diagram label, plan reference); "TELEMETRY MCP-ONLY · DEFAULT ON · OPT-OUT" in the diagram frame; operation events labelled "MCP-caller op events" and the thesis, diagram description and figcaption saying the CLI writes no telemetry; the guardrail rewritten to "Monitor runs in-process, in the foreground" (IF-019 shape); principles stating the fragment token and the 5 MiB / 7 days / 50 MiB retention and the default-on, opt-out setting; the approval guardrail and `report` text rewritten to ADR-013 wording (explicit self-asserted `approved_by` label required, no Git or OS fallback over MCP, not authentication). "Proposed" is kept (footer, figure caption, diagram description); WU-022 owns that label. The remote-URL check still passes: no `http(s)://`, `googleapis`, `gstatic` or "Orca"; the only `url()` uses are internal SVG marker fragments such as `url(#arrow-ink)`.

Still open, owned elsewhere:
- WU-017: confirm the id filter rules server-side and state the latency population in IF-017 (section 6).
- WU-019: build to sections 1 to 5 and run the TS-030 checklist in 5.3.
