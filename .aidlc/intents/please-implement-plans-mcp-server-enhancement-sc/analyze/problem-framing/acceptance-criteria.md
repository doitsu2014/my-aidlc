# Acceptance criteria

- Purpose: observable, testable criteria for intent 1 (shared application layer plus MCP stdio server).
- Status: Draft - pending analysis-baseline gate
- Sources: plans/mcp-server-enhancement/{architecture s11, detailed-design s4-s5 and s9, implementation-plan s3, s5, s6}; problem-statement.md. Provenance: Observed in plans (all MCP items are Proposed), reworded in behaviour terms; open decisions are listed in problem-statement.md.

## AC-001 CLI behaviour unchanged
- Statement: The system shall produce the same command output, exit codes, project-file effects and audit rows for every existing CLI command as before the change.
- Rationale: The refactor must not alter the product people already use (plan Phase 1 exit, AC3).
- Source: implementation-plan s5, s6; Observed
- Story: US-001

### Scenario: existing suite passes
- Given the existing test suite and golden files in the agreed baseline environment
- When the full suite is run after the change
- Then every test that passed before still passes and no golden file differs

### Scenario: blocked outcome keeps its exit code
- Given a stage whose next step is blocked
- When the user runs the next-stage command from the CLI
- Then the printed report and the non-zero exit code match the pre-change behaviour

## AC-002 One shared behaviour path
- Statement: The system shall serve CLI and MCP requests for status, queue, next and report from the same operations, without launching the CLI as a child process and without a second workflow engine.
- Rationale: Prevents divergence between the two entry points (plan s6 AC2, risk "CLI/MCP divergence").
- Source: architecture s3.3; implementation-plan s3.1; Observed
- Story: US-002

### Scenario: parity on equivalent projects
- Given two isolated, identical projects, one driven by the CLI and one by the MCP server
- When the same operation and inputs are applied to each
- Then the returned semantic data, the changed project files and the audit rows are equivalent

### Scenario: engine selection honoured
- Given the external engine location setting is present
- When either entry point runs an operation
- Then both use the same engine and neither silently falls back to another

## AC-003 Server launches and exposes tools
- Statement: The system shall provide a separate MCP server executable that, when started by a host over standard input and output, completes the protocol handshake and lists exactly the tools status, queue, next and report, for each supported host: Claude Code and DeepSeek Harness. Both hosts must launch the server and list the four tools.
- Rationale: Core capability requested (plan s6 AC1); supported hosts decided by the owner (Q1).
- Source: architecture s3.1; design s4.1; Observed; hosts = Claude Code and DeepSeek Harness (User-provided, project owner, 2026-11-27); the exact protocol revision is pinned in Ideate
- Story: US-003

### Scenario: handshake and discovery
- Given the server executable is started by a test host over stdin/stdout
- When the host sends initialize and then a tool-list request
- Then the host receives a valid handshake response and a tool list containing the four tools, each with a strict input schema

### Scenario: supported hosts launch and discover
- Given the server is configured in Claude Code, and separately in DeepSeek Harness, using the documented manual configuration
- When each host launches the server and requests the tool list
- Then each host completes the handshake and lists status, queue, next and report

### Scenario: restart and shutdown preserve state
- Given a project with existing workflow state
- When the server is shut down and started again
- Then the workflow state and audit file are byte-identical to before the shutdown

## AC-004 Stdout carries only protocol frames
- Statement: The system shall write only protocol frames to standard output, send diagnostics to standard error, and open no network listener.
- Rationale: Stray output corrupts the protocol; local-only is a stated non-goal boundary (plan s6 AC4).
- Source: design s3.1, s4.1; Observed
- Story: US-003

### Scenario: clean stdout across a session
- Given a child-process session covering valid calls, schema errors and domain errors
- When all stdout bytes are captured
- Then every byte belongs to a protocol frame and no listening network socket exists for the process

### Scenario: startup failure
- Given an invalid project path argument
- When the server is started
- Then it prints a message to standard error, exits non-zero, and writes no project state

## AC-005 Requests validated before project access
- Statement: The system shall reject requests that have missing required fields, unknown fields, invalid enumerated values or oversized strings and lists, before reading or writing any project file.
- Rationale: Bounded, schema-validated calls (plan Goal 1, Phase 2 step 4).
- Source: design s4.1, s9; Observed
- Story: US-004

### Scenario: unknown field
- Given a running server and a project
- When a tool call includes a field not in the schema
- Then the call returns an actionable validation error and project files and audit are unchanged

### Scenario: oversized input
- Given a report call whose detail text exceeds the documented size limit
- When the call is sent
- Then it is rejected with a bounded error and nothing is written

## AC-006 Status is read-only and path-safe
- Statement: The system shall return the workflow status through MCP without writing any project file and without exposing absolute filesystem paths.
- Rationale: Hosts should not learn local directory layout unnecessarily (design s4.3).
- Source: design s4.3; implementation-plan s3.3; Observed (status output contains an absolute intent directory today)
- Story: US-005

### Scenario: status call
- Given a project with an active intent
- When the status tool is called
- Then the result shows the same semantic status as the CLI, contains no absolute path at any nesting level, and no project file changed

### Scenario: unknown intent selector
- Given an intent identifier that does not exist
- When the status tool is called with it
- Then a safe domain error is returned and nothing is written

## AC-007 Queue is a read-only preview
- Statement: The system shall return the ready, waiting, blocked, parked and done work through MCP without writing any file, and shall mark legacy-format state as a pending migration that is not persisted.
- Rationale: Preview must not mutate or imply persisted migration (design s4.3).
- Source: design s4.3; Observed; park and revisit are not exposed in v1
- Story: US-005

### Scenario: queue on current-format state
- Given a project with work units in several states
- When the queue tool is called
- Then the result groups them by state and the project directory contents are unchanged

### Scenario: queue on legacy-format state
- Given a project whose state is in a legacy format
- When the queue tool is called
- Then the result carries a visible pending-migration marker and the state on disk is unchanged

## AC-008 Next-stage guard
- Statement: The system shall let the MCP next tool run only when an active intent already exists, shall not create or select an intent, and shall never execute configured prerequisite shell checks; configured prerequisites are returned as pending and marked not executed.
- Rationale: A model-driven call must not start work or run owner-configured commands (design s4.3; plan s3.3).
- Source: design s4.3; Observed: the CLI next command runs configured prerequisite checks today and has no no-execute mode; decided: never from MCP, with the guard off for the CLI so CLI behaviour stays byte-identical (Q5, User-provided, project owner, 2026-11-27). Implementation detail is in constraints 10 and 17 and the evidence log (E8, Corrections).
- Story: US-006

### Scenario: active intent
- Given a project with an active intent and a ready stage with no configured prerequisite
- When the next tool is called
- Then the result carries the directive, complete, blocked or busy outcome with paths relativized or omitted, and the audit file records the same events as the CLI

### Scenario: no active intent
- Given a project with no active intent
- When the next tool is called
- Then it is refused with a clear error, and no intent is created or selected and no file changes

### Scenario: configured prerequisite
- Given a stage with a configured prerequisite check
- When the next tool is called
- Then the configured prerequisite names are returned marked not executed and no check command runs

## AC-009 Report keeps validation, audit and concurrency
- Statement: The system shall accept reports through MCP only with the documented fields (stage, result, detail, approval, approver label, approval note, needs, unit), shall apply the same validation, path-safety, concurrency, gate and audit rules as the CLI, and shall never execute configured prerequisite shell checks; prerequisites that would run when advancing are returned as pending and marked not executed.
- Rationale: MCP must not bypass governance (plan s6 AC5) and, like next, must not run owner-configured commands (Q5).
- Source: design s4.3; implementation-plan s3.3; Observed; approver label accepted (Q6) and no prerequisite execution (Q5), both User-provided, project owner, 2026-11-27. Observed: the CLI report command does not run prerequisite checks (it only checks that named needs are known), so this guarantee does not differ from the CLI on report; see the evidence log Corrections.
- Story: US-007

### Scenario: valid report
- Given a stage with its required artifacts present and no pending prerequisite
- When a report is submitted through MCP
- Then the state and audit rows written equal those of the same report via the CLI

### Scenario: report with a configured prerequisite
- Given a report that would advance to a stage with a configured prerequisite check
- When the report is submitted through MCP
- Then the pending prerequisite names are returned marked not executed, no check command runs, and the report is validated and audited as the CLI would

### Scenario: rejected report
- Given a stage whose artifacts are missing or invalid, or whose state changed since it was read
- When the report is submitted
- Then the tool returns the application error and does not claim success, and state and audit are unchanged

### Scenario: caller-supplied evidence
- Given a report call that includes an artifact or evidence field
- When the call is sent
- Then it is rejected as an unknown field

## AC-010 Approvals over MCP follow the CLI gate policy
- Statement: The system shall accept an approver label on an MCP report and apply exactly the CLI gate policy to it: an AI-mode gate is approved only with the approver "AI Orchestrator" and a valid gate-recommendation file; a human-mode gate refuses the approver "AI Orchestrator". It shall offer no separate approve-gate tool, and shall describe approver labels as self-asserted rather than authenticated.
- Rationale: The owner accepted that the label is self-asserted (Q6); MCP must not weaken the CLI policy. A host confirmation is not verifiable by the server (plan s6 AC6; design s4.4).
- Source: design s4.4; plan s3.3; Observed; approver label accepted, same gate policy as CLI (Q6, User-provided, project owner, 2026-11-27, chosen over the recommended AI-mode-only option). Open (Ideate, Q18): omitted-label handling, refusal error code, and whether the audit can record that the approval came via MCP; any such record must not change the audit schema (carried non-goal) and must not break the audit-row equality in AC-002 and AC-009, so it is not required by this criterion.
- Story: US-008

### Scenario: AI-mode gate approved
- Given a gate configured for AI approval with a valid recommendation file
- When a report with approval "approved" and approver "AI Orchestrator" is submitted through MCP
- Then the gate is recorded approved with approver "AI Orchestrator" and one gate decision audit row is written

### Scenario: tool documentation states the label is self-asserted
- Given the MCP report tool documentation
- When a reviewer reads the approver field description
- Then it states the label is self-asserted and not authentication

### Scenario: AI-mode gate without recommendation
- Given a gate configured for AI approval and no valid recommendation file
- When a report with approval "approved" and approver "AI Orchestrator" is submitted through MCP
- Then it is refused as the CLI refuses it, and no gate decision is audited

### Scenario: human-mode gate refuses the AI label
- Given a gate configured for human approval
- When a report with approval "approved" and approver "AI Orchestrator" is submitted through MCP
- Then it is refused, the gate stays undecided, and no gate decision is audited

### Scenario: human-mode gate with a named approver
- Given a gate configured for human approval
- When a report with approval "approved" and a non-AI approver label is submitted through MCP
- Then it is handled exactly as the CLI handles that label (self-asserted, recorded as given), and the tool documentation states it is not authentication

### Scenario: approval without an explicit label (pending the Ideate decision)
- Status: pending the Ideate decision (proposed conservative default); the owner has not decided this
- Given a gate awaiting approval, of either mode
- When a report with approval "approved" and no approver label is submitted through MCP
- Then it is refused, the gate stays undecided, no gate decision is audited, and no git or OS user identity is substituted

### Scenario: policy edited between calls
- Given the gate mode is changed in project configuration while the server is running
- When the next report is submitted
- Then the new configuration governs that call

## AC-011 Errors are safe and not retried
- Statement: The system shall return domain, validation, I/O and concurrency failures as bounded errors with a stable code and actionable text, with no stack traces, secrets or file contents, and shall never retry a state-changing call automatically.
- Rationale: Hosts may resend; exactly-once is not promised (design s5).
- Source: design s5; Observed
- Story: US-004

### Scenario: concurrency conflict
- Given project state changed between a call's read and its write
- When the state-changing tool call is made
- Then the conflict error is returned once, the call is not retried, and no partial state exists

### Scenario: unsafe project layout
- Given the project state directory is an unsafe path or symlink
- When any tool is called
- Then the call is refused and nothing is written

## AC-012 Root fixed at launch; concurrent use safe
- Statement: The system shall take the project root once at launch (from a launch argument or the startup directory), shall not accept a root in any tool call, and shall not corrupt intent state when a CLI run and the server write the same project.
- Rationale: Limits what a model can reach and protects state (design s3.1).
- Source: design s3.1, s9; Observed; sufficiency of current concurrency check is an Open question (owner)
- Story: US-004

### Scenario: root in a tool call
- Given a running server bound to one project
- When a call includes a project path field
- Then it is rejected as an unknown field

### Scenario: simultaneous CLI and MCP writes
- Given the CLI and the server issue conflicting writes to the same project
- When both run
- Then at most one succeeds per conflicting write, the other reports a conflict, and the state remains valid

## AC-013 CLI optional and binaries install cleanly
- Statement: The system shall keep the existing CLI executable name and function without the MCP server, and shall install, run and uninstall both executables from outside the source checkout.
- Rationale: MCP is additive (architecture s3.2; plan s5).
- Source: plan s5, Phase 2; Observed
- Story: US-009

### Scenario: CLI only
- Given only the CLI executable is installed
- When any existing command is run
- Then it behaves as before

### Scenario: install smoke
- Given a clean location outside the checkout
- When both executables are installed, started with help and version, and removed
- Then they run, print help and version, and leave no background process or user-level assistant settings changed

## AC-014 Manual host configuration and honest labels
- Statement: The system shall document manual host-configuration examples and troubleshooting, shall not modify user-level assistant settings itself, and shall label MCP features as proposed until released.
- Rationale: Avoid hidden host changes and overclaiming (plan s1, s6 AC13).
- Source: plan Phase 2 step 5, s6; Observed; Phase 2 versus Phase 5 timing is an Open question (owner)
- Story: US-009

### Scenario: docs present
- Given the delivered change
- When a reviewer reads the documentation
- Then it contains copy-ready launch examples and troubleshooting, and every MCP statement is labelled proposed until a release is recorded

### Scenario: install writes nothing to host
- Given a user-level assistant settings file
- When the executables are installed and run
- Then the settings file is byte-identical

## Revision history
- 2026-11-27 (attempt 2): revised after owner decisions 2026-11-27 (User-provided, project owner, via the conductor AskUserQuestion after the first analysis-baseline submission was blocked).
  - AC-003: hosts are Claude Code and DeepSeek Harness; both must launch and list the four tools; scenario added (Q1).
  - AC-008: prerequisites are never executed from MCP next; conditionality on Q5 removed (Q5).
  - AC-009: approver label accepted; "same behaviour as the CLI" narrowed so it no longer covers prerequisite execution; report never runs prerequisite checks, pending ones are returned marked not executed; scenario added (Q5, Q6). This resolves the AC-009 versus Q5/Q6 contradiction in the blocked gate recommendation.
  - Review fixes (same date): AC-008 and AC-009 corrected to the observed CLI behaviour (report runs no prerequisite checks); AC-010 audit-provenance wording moved to Source/Open (Q18) and the AI-mode scenario given a specific Then.
  - AC-010: renamed from "No assistant-only approval path" to "Approvals over MCP follow the CLI gate policy"; AI-mode and human-mode scenarios rewritten; the omitted-label refusal scenario added and marked pending the Ideate decision (Q6).
