# Acceptance criteria: intent 2, telemetry, monitor and release

Purpose: testable criteria for the intent-2 scope. Identifiers are per intent. Behavior is stated without implementation names.
Status: Draft - pending analysis-baseline gate
Sources: `problem-statement.md`; plan s3, s5-s6; arch s11; design s6-s9, s11; intent-1 ADRs; `tests/repo_content.rs`.
Provenance labels: Observed, User-provided, Assumption, Estimate, Open question. Values marked Open question (Q#) refer to `problem-statement.md`; they are not decided here.

## AC-001 Telemetry setting is explicit and controllable
- Statement: Telemetry shall have an owner-confirmed default and a documented way to turn it on or off, and the dashboard and docs shall state the effective setting or say 'unknown'.
- Rationale: Plan s3.7 leaves the default as an owner privacy decision (Q1); users must know whether data is collected
- Source: Owner; plan s3.7, s8.1
- Story: US-001

### Scenario: Telemetry is switched off through the documented control
- Given a project with telemetry switched off
- When a tool call and a CLI status are run
- Then no telemetry file is created or changed and both results are identical to a run without the feature

### Scenario: No trusted setting input
- Given no trusted setting input exists for the monitor
- When the dashboard is loaded
- Then it shows the telemetry setting as 'unknown' and does not infer on or off from missing files
- Note: the default value itself is blocked pending Q1; implementation is held at the design gate until the owner records it (gate recommendation in `problem-statement.md`)

## AC-002 Events contain metadata only
- Statement: Every stored event shall contain only the documented allowlisted fields.
- Rationale: The privacy promise is the core of the plan
- Source: Plan s3.4; design s6.2
- Story: US-001

### Scenario: Allowed fields only
- Given a tool call whose arguments include a free-text detail, an approver label and a long artifact reference
- When the call completes
- Then the stored event has exactly the fields of the design s6.2 allowlist (normative) and no others: schemaVersion, eventId, timestamp, component, caller (omitted if unknown), processId, sessionId, traceId, spanId, parentSpanId, event, name, intentId, stageId, unitId (the last three optional and validated), durationMs, outcome, errorCode; a test checks field-for-field (plan Phase 3 step 2)

### Scenario: Sensitive input never stored
- Given inputs containing a credential-like string, an environment value and an absolute path
- When the call is made and telemetry is on
- Then searching the whole store finds none of those strings

## AC-003 Event schema is versioned and stable
- Statement: Every event shall carry a schema version and the field set documented for that version.
- Rationale: Readers must tolerate change
- Source: Plan s3.4, s5; design s6.2
- Story: US-002

### Scenario: Schema consistency
- Given the documented schema and the produced events
- When a sample of every event kind is produced
- Then each event matches the documented field names, types and enumerations exactly

### Scenario: Unknown future data
- Given a store containing an unknown event kind, an extra field and a truncated final line
- When the dashboard or reader loads it
- Then known events display, unknown ones are shown as unknown or skipped, and the skipped count is reported

## AC-004 MCP tool calls are observed
- Statement: Each tool call that reaches the server shall produce one start and one completion event with duration and outcome.
- Rationale: Maintainers need call visibility
- Source: Plan s3.5; design s6.3
- Story: US-002

### Scenario: Call completes
- Given the MCP server running with telemetry on
- When status, queue, next and report are each called once
- Then there is one start and one completion per call, named with the shipped tool names, with a duration and an outcome of ok, blocked or error

### Scenario: Invalid arguments
- Given a call to a known tool with invalid arguments
- When the call is made
- Then events are produced with an error outcome

### Scenario: Unknown tool
- Given a call to a tool name that does not exist
- When the call is made
- Then events are produced with an error outcome

### Scenario: Process dies mid-call
- Given a process that stops after a start event and before completion
- When the store is read
- Then the missing completion is shown as incomplete, never as success

## AC-005 Protocol errors are observed without raw content
- Statement: Protocol-level failures shall produce a protocol-error event carrying only a stable error code.
- Rationale: Plan requires a producer for every event kind
- Source: Plan s3.5, s5
- Story: US-002

### Scenario: Unparseable line
- Given telemetry on
- When the server receives an unparseable line
- Then a protocol-error event with a stable code is stored and the host receives the same error reply as before

### Scenario: Oversized line
- Given telemetry on
- When the server receives an oversized line
- Then a protocol-error event with a stable code is stored and the host receives the same error reply as before

### Scenario: Content excluded
- Given the same malformed input contains secrets
- When the event is stored
- Then the event contains none of the raw frame text

## AC-006 Shared operations are observed with caller
- Statement: Each shared operation shall produce start and completion events naming whether the CLI or MCP made the call.
- Rationale: Operation-level visibility, separate from tool spans
- Source: Plan s3.5; design s6.3
- Story: US-002

### Scenario: Both callers
- Given telemetry on
- When the CLI runs status and the MCP tool runs status
- Then two operation events record caller cli and caller mcp, and the MCP call links to its tool span by trace

### Scenario: Coverage gap shown
- Given a CLI command that does not pass through the shared operations, such as park or revisit
- When it is run
- Then no event is claimed for it and the dashboard coverage label says operation events cover only calls through the shared operations

## AC-007 Workflow events only after the change is durable
- Statement: Stage-selected, stage-reported and gate-decided events shall be written only after the state change (and for gates the audit record) is persisted.
- Rationale: Telemetry must not claim what did not happen
- Source: Plan s3.5; design s6.3
- Story: US-002

### Scenario: Successful report
- Given telemetry on
- When a report finishes successfully
- Then a stage-reported event exists, and for a gate decision a gate-decided event exists only after state and audit are written

### Scenario: Failed or conflicting report
- Given a report that fails with a state conflict
- When it returns
- Then no stage-reported or gate-decided event exists and the operation event shows error

## AC-008 Telemetry is stored separately and ignored by git
- Statement: Events shall be written to files under a dedicated telemetry directory, one file per process, never in the audit log, and that directory shall be git-ignored through the existing managed ignore block.
- Rationale: Separation from audit; avoid accidental commits
- Source: Plan s3.4, AC 7, AC 11
- Story: US-003

### Scenario: Fresh project
- Given a project with the managed ignore block and telemetry on
- When an operation runs
- Then a new per-process file exists under the telemetry directory, the audit log has no new telemetry row, and git status shows no telemetry file

### Scenario: Symlinked telemetry directory
- Given the telemetry directory is a symlink
- When an operation runs
- Then no event is written through the symlink, the operation succeeds and a diagnostic goes to stderr only

### Scenario: Ignore file with user text
- Given the ignore file contains user text outside the managed block
- When an operation runs
- Then the user text in the ignore file is untouched and the operation succeeds

## AC-009 Retention is bounded
- Statement: The store shall stay within the confirmed per-file size, maximum age and total size, pruning only recognized telemetry files.
- Rationale: Unbounded local data is a privacy and disk risk
- Source: Plan s3.4; Open question Q6
- Story: US-003

### Scenario: Old and oversized data
- Given files older than the age limit and a store above the total limit
- When the next process starts or writes
- Then old files are removed, the total returns under the limit, and unrelated files in the directory remain

### Scenario: File reaches its size cap
- Given a file at the per-file limit
- When one more event is written
- Then the operation outcome is unchanged; the file-cap behavior (new file or stop) is blocked pending Q6 and not testable as written

## AC-010 Telemetry failure never changes the workflow
- Statement: An unwritable, full, corrupt or absent telemetry store shall not change any operation result, exit code, stdout or workflow state.
- Rationale: Plan exit criterion
- Source: Plan s2.4, Phase 3 exit; design s5
- Story: US-004

### Scenario: Store unwritable
- Given a read-only telemetry location
- When a tool call and CLI commands run
- Then results, exit codes and stdout equal the telemetry-off results, and a diagnostic appears on stderr only

### Scenario: Store corrupt
- Given an existing file with garbage and a truncated line
- When operations run
- Then operations succeed and the garbage is neither repaired nor propagated

## AC-011 Nothing leaves the machine
- Statement: Telemetry and the dashboard shall make no outbound network connection and the dashboard shall load no remote asset.
- Rationale: Local-only is the product promise
- Source: Plan s1; arch s4
- Story: US-004

### Scenario: Offline run
- Given a machine with outbound traffic blocked
- When telemetry and the monitor run through a full session
- Then all features work and no outbound connection attempt is observed

### Scenario: Page assets
- Given the dashboard page is loaded
- When its resources are listed
- Then every resource is served by the local monitor, with no remote font, script or analytics

## AC-012 Concurrent sessions stay readable
- Statement: Concurrent CLI and MCP processes shall write independent files without interleaving.
- Rationale: Plan Phase 3 exit
- Source: Plan Phase 3 exit, s5
- Story: US-003

### Scenario: Parallel writers
- Given an MCP session and several CLI runs at the same time
- When they all complete
- Then each file is complete, parseable line by line, and no event appears in two files

### Scenario: Name collision
- Given two processes start in the same instant
- When both create files
- Then both get distinct files and neither fails

## AC-013 Monitor starts and stops cleanly
- Statement: The monitor command shall start a local server in the foreground, print its address, stay attached until interrupted, and leave no orphan process or browser window.
- Rationale: Optional, explicit launch
- Source: Plan s3.6; design s7.7
- Story: US-005

### Scenario: Start and stop
- Given an installed CLI outside the checkout
- When the owner starts the monitor and interrupts it
- Then the address is printed, the page loads, no browser is opened, and after interruption no monitor process remains

### Scenario: Cannot start
- Given the port or the telemetry location is unavailable
- When the monitor starts
- Then it exits with a clear message and a nonzero code, and MCP and other CLI commands are unaffected

## AC-014 Monitor is loopback-only and read-only
- Statement: The monitor shall accept connections only on the loopback address and only read requests, and offer no route that changes workflow, telemetry or configuration.
- Rationale: Dashboard is not a control panel
- Source: Plan s3.6, s1; arch s5
- Story: US-005

### Scenario: Allowed read
- Given the monitor running
- When a read request comes from the local machine
- Then a response is returned

### Scenario: Write methods refused
- Given the monitor running
- When a POST, PUT, PATCH or DELETE arrives
- Then it is refused with a method error and no state is changed

### Scenario: Other interface
- Given the monitor running
- When a connection is attempted from another network interface
- Then the connection cannot be made

## AC-015 Browser-origin safety
- Statement: Requests with an unexpected Host or Origin header shall be refused and no cross-origin permission shall be granted.
- Rationale: DNS-rebinding and drive-by page risk
- Source: Plan s3.6, s8.6; design s7.7
- Story: US-005

### Scenario: Expected headers
- Given a request with the monitor's own host and origin
- When it is made
- Then it succeeds

### Scenario: Foreign headers
- Given a request with a different Host or a foreign Origin
- When it is made
- Then it is refused and the response grants no cross-origin access

## AC-016 Monitor reads telemetry only
- Statement: The monitor shall read only recognized telemetry files and shall never accept a path from the browser, return raw lines, or read workflow state, audit or artifact content.
- Rationale: Limits blast radius
- Source: Plan s3.6; design s7.7
- Story: US-005

### Scenario: Normal query
- Given telemetry files and workflow files present
- When a query is made
- Then the response includes only parsed allowlisted fields from telemetry files

### Scenario: Path from the browser
- Given a request naming a path
- When it is made
- Then it is rejected with a stable error code and no file outside the telemetry store is read

### Scenario: Symlinked telemetry file
- Given a symlinked telemetry file
- When a query is made
- Then the file is not read and no file outside the telemetry store is read

### Scenario: Range above the maximum
- Given a request for a range above the maximum (blocked pending Q14 / contradiction 15: no maximum value exists, so not testable as written)
- When it is made
- Then it is rejected with a stable error code

## AC-017 Dashboard shows every defined state
- Statement: The dashboard shall present distinct, honest states for empty, off (trusted input only), recent events, partial or corrupt records, stale, store unreadable, liveness unknown, limit reached and shutting down.
- Rationale: Plan s3.6 state list
- Source: Plan s3.6; design s7.5
- Story: US-006

### Scenario: Healthy and empty
- Given the store has recent events, then is empty
- When the page loads
- Then the first shows events newest first with last-event time, the second says no telemetry observed and does not say the process is absent

### Scenario: Corrupt lines
- Given a store with corrupt lines
- When the page loads
- Then the skipped count is shown, with no liveness claim

### Scenario: Unreadable store
- Given an unreadable store
- When the page loads
- Then an unavailable message appears with last-known data marked stale, with no liveness claim

### Scenario: Stale data
- Given data older than the stale threshold (threshold value blocked pending Q14 / design s7.5)
- When the page loads
- Then a stale label appears, with no liveness claim

## AC-018 Metrics are defined and honest
- Statement: Counts, error rates and latency shall use stated populations, show denominators and sample sizes, and deduplicate by event identity.
- Rationale: Misleading metrics defeat the purpose
- Source: Plan s3.6; design s7.3; Open question Q14
- Story: US-006

### Scenario: Summary
- Given completed tool calls with mixed outcomes and a retried duplicate record
- When the summary is shown
- Then tool-call count, blocked and error ratios use that population, operation counts are shown separately, and duplicates are counted once

### Scenario: Too few samples
- Given fewer completed calls than the minimum sample size (blocked pending Q14: no value confirmed, not testable as written)
- When latency is shown
- Then it says insufficient samples and shows no percentile

## AC-019 Dashboard filters and detail without persistence
- Statement: The dashboard shall filter by range, source, name, outcome, stage and session or trace, page through results, and show event detail, keeping filter state only in memory.
- Rationale: Answer the sequence question without leaking state
- Source: Plan s3.6; design s7.4
- Story: US-006

### Scenario: Filter and drill down
- Given events from two traces
- When the user filters to one trace and opens an event
- Then only that trace's events show, detail shows allowlisted fields and parent navigation, and the address contains no filter data

### Scenario: Stale cursor
- Given a page cursor from before a filter change
- When the old cursor is used
- Then a stale-cursor message appears

### Scenario: Reload
- Given filters set in the page
- When the page reloads
- Then filters are back to defaults

## AC-020 Privacy disclosure on the dashboard
- Statement: The dashboard header or help shall disclose the effective setting, storage location in generic form, retention limits, retained fields and exclusions.
- Rationale: Informed local use
- Source: Plan s3.6; design s7.1
- Story: US-006

### Scenario: Disclosure visible
- Given the dashboard is open
- When the privacy help is opened
- Then it lists setting (or unknown), location without the absolute path, limits, fields and exclusions

### Scenario: Project label
- Given a project path containing a username
- When the header renders
- Then the header shows a generic local-project label, never the path

## AC-021 Dashboard accessibility basics
- Statement: The dashboard shall be fully operable by keyboard, show visible focus, use landmarks and accessible names, not rely on color alone, respect reduced motion, and avoid horizontal page scroll at narrow width.
- Rationale: Design s7.6 requirements
- Source: Plan s3.6; design s7.6
- Story: US-007

### Scenario: Keyboard and screen reader
- Given the dashboard open
- When the user uses only the keyboard through range, filters, rows, trace links, refresh and detail close
- Then every control is reachable and operable, focus is visible, focus returns after closing detail, and refresh results are announced once

### Scenario: Narrow and degraded
- Given a 360-pixel-wide window and an error state
- When the page renders
- Then there is no horizontal page scroll, detail opens inline, and status is shown with text and a shape or icon as well as color

## AC-022 Dashboard design is reviewed before it is built
- Statement: The wireframes, state matrix, tokens, privacy map and read-interface contract shall be accepted in writing before dashboard implementation starts.
- Rationale: Plan AC 10 and s3.6 deliverable
- Source: Plan s3.6, s6 item 10; Open question Q15
- Story: US-007

### Scenario: Review before build
- Given the design pack exists
- When dashboard work is about to start
- Then a recorded acceptance by the named reviewer exists and covers layouts, states, filters, details, metrics, privacy, accessibility and bounded reads

### Scenario: Not accepted
- Given the reviewer rejects the pack
- When the owner checks readiness
- Then dashboard implementation does not start until the pack is revised and accepted

## AC-023 Version bump is consistent
- Statement: The release shall set one new version everywhere the project ties it together: package manifest, lock file, README badge, CHANGELOG heading and version-bearing golden records.
- Rationale: Mismatches fail content checks and mislead users
- Source: ADR-018; `tests/repo_content.rs`; Open question Q5
- Story: US-008

### Scenario: Bump applied
- Given the owner-confirmed version
- When the full test suite runs
- Then the version output, badge, CHANGELOG heading and lock file all show the same version and the suite passes

### Scenario: Partial bump
- Given a change that updates the manifest but not the badge
- When the suite runs
- Then a version-consistency check fails and names the mismatch

## AC-024 CHANGELOG entry covers user-visible changes
- Statement: The CHANGELOG shall list, under the new version, the second binary, the toolchain minimum, telemetry, the monitor command, any approved golden change, and that these are local optional capabilities.
- Rationale: Each release lists behavior a user can notice
- Source: ADR-018; plan Phase 5 exit
- Story: US-008

### Scenario: Entry present
- Given the release candidate
- When the CHANGELOG is read
- Then the top heading is the new version and each listed change is present

### Scenario: Missing item
- Given an entry that omits the monitor command or the toolchain minimum
- When the review checklist is applied
- Then the omission is reported and the release is not approved

## AC-025 README is accurate and the proposed label is resolved
- Statement: The README shall describe install, usage, host configuration, tools, telemetry schema and retention, dashboard use and security boundary, coverage limits and troubleshooting, and drop the proposed label only per the owner's evidence decision (Q10). The owner-edited platform sentence is part of the README updates the owner named, tied to Q10 (who edits it, and the 'not yet run' sentence); other inherited intent-1 follow-ups stay under Q12.
- Rationale: Plan AC 13; Open question Q10
- Source: Plan Phase 5; next-intent-notes (platform sentence: via Q10)
- Story: US-008

### Scenario: Docs complete
- Given the release candidate
- When the README is read and every shell example is run against the help output
- Then each documented command exists, the section heading no longer says proposed (blocked pending Q10: only if the owner approves, not testable as written), and unverified hosts are stated as unverified

### Scenario: Evidence not accepted
- Given the owner decides the evidence is insufficient
- When the README is reviewed
- Then the proposed wording remains and the release notes say which phases are released

## AC-026 Install scripts install both binaries
- Statement: The POSIX and Windows install scripts shall install, update and uninstall both public binaries, recognize only their own files, and the installed CLI shall launch the monitor from its own location.
- Rationale: Plan AC 12
- Source: Plan Phase 5, AC 12; Open question Q11
- Story: US-009

### Scenario: Install from outside the checkout
- Given a clean prefix on each platform
- When install, run both binaries' smoke checks, update, then uninstall
- Then both binaries and launchers work, the monitor starts from the installed CLI, and uninstall removes only recognized files

### Scenario: Existing CLI-only install
- Given a prefix from the previous CLI-only installer
- When the new script runs
- Then it upgrades to both binaries without deleting unrecognized files, or stops with a clear message if it cannot

## AC-027 Existing CLI behavior is unchanged
- Statement: All existing commands shall keep their output, exit codes and file effects, except changes the owner explicitly scopes (managed ignore line, new command in help, version strings).
- Rationale: Plan s3.7; ADR-006
- Source: Plan s3.7; next-intent-notes; Open question Q2, Q4
- Story: US-010

### Scenario: Golden comparison
- Given the existing recorded outputs
- When the suite runs
- Then all records match except the listed owner-approved files, each with a reviewed diff

### Scenario: Unapproved difference
- Given a change that alters any other recorded output
- When the suite runs
- Then the suite fails and the change is not accepted by re-recording

## AC-028 MCP stdout carries only protocol frames
- Statement: The MCP server's standard output shall contain only protocol frames in every telemetry state, including failure.
- Rationale: Hosts break on stray output
- Source: next-intent-notes; ADR-005
- Story: US-010

### Scenario: Telemetry on or off
- Given a scripted session
- When it runs with telemetry on, off and failing
- Then standard output parses as protocol frames only and diagnostics are on standard error

### Scenario: Telemetry write errors
- Given a telemetry write that returns an error
- When a call is made
- Then the host still receives its normal reply, never an internal-error substitute

### Scenario: Telemetry write panics
- Given a telemetry write that panics
- When a call is made
- Then the host still receives its normal reply, never an internal-error substitute, and the process keeps serving

### Scenario: Telemetry write stalls
- Given a telemetry write that blocks beyond the time the owner accepts (blocked pending Q14 / design for the bound; no value exists)
- When a call is made
- Then the host receives its normal reply without waiting for the write

## AC-029 Required CI stays green
- Statement: The test suite shall pass on Windows and macOS (and not regress on Ubuntu), with no new network or unsafe-skip behavior, and no new crate unless the owner approved.
- Rationale: Platform support is a stated requirement
- Source: Knowledge record; `.github/workflows/ci.yml`; Open question Q3
- Story: US-010

### Scenario: Both platforms
- Given the release candidate pushed
- When the CI runs
- Then Windows and macOS report zero failures, installer tests do not skip, and the dependency set equals the approved set

### Scenario: Platform-specific failure
- Given a test that passes on macOS but fails on Windows
- When CI runs
- Then the build is red and the release is not approved

## AC-030 Threat-model review and honest identity wording precede release
- Statement: A written review of paths, approval semantics, stdout leakage, loopback exposure, telemetry privacy and retention shall be accepted before release, and documents shall say approver names are self-asserted and telemetry is not audit.
- Rationale: Plan Phase 5 exit
- Source: Plan Phase 5; ADR-013
- Story: US-011

### Scenario: Review done
- Given the release candidate
- When the owner checks the gate
- Then a recorded review covers all six topics, including the token decision (Q7), and the docs and release notes carry both statements

### Scenario: Review open
- Given an unresolved high finding
- When approval is requested
- Then the release is not approved and the finding is listed as blocking
