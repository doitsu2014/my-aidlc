# User stories

- Purpose: stories derived from the request and plans for intent 1, plus recorded out-of-scope stories for intent 2.
- Status: Draft - pending analysis-baseline gate
- Sources: intent.md; plans/mcp-server-enhancement/*; problem-statement.md; acceptance-criteria.md. Provenance: Observed in plans; roles are those the plans name.

## Users
- MCP-assistant driver: person who drives AIDLC through an MCP-capable assistant (plan header, primary users)
- Project owner / scripts: owner using the CLI directly, who also completes human-mode gate approvals (architecture s5; design s4.4)
- Assistant harness: the MCP host that launches the server and relays calls (plan s2.2)
- Maintainer: person who maintains the codebase; the plans name this role for dashboard use (plan header)

## Stories
### US-001 CLI stays the same
- Story: As a project owner, I want the CLI to behave exactly as before after the refactor, so that my scripts and goldens keep working
- Scope: in-scope
- Criteria: AC-001

### US-002 Shared operations
- Story: As a project owner, I want CLI and MCP to share one set of operations, so that behaviour cannot diverge
- Scope: in-scope
- Criteria: AC-002

### US-003 Launch and discover
- Story: As an assistant harness (Claude Code or DeepSeek Harness), I want to start a local stdio server and list its tools without protocol noise, so that I can offer them to the assistant
- Scope: in-scope
- Criteria: AC-003, AC-004

### US-004 Bounded, safe requests
- Story: As a project owner, I want invalid, oversized or unsafe requests refused before they touch the project, with safe errors and no automatic retries of changes, so that a model cannot damage workflow state
- Scope: in-scope
- Criteria: AC-005, AC-011, AC-012

### US-005 Read status and queue
- Story: As an MCP-assistant driver, I want read-only status and queue results without local absolute paths, so that I can see where the workflow stands safely
- Scope: in-scope
- Criteria: AC-006, AC-007

### US-006 Get the next stage
- Story: As an MCP-assistant driver, I want to fetch the next stage directive for the active intent without starting new intents or running configured commands implicitly, so that I can continue the workflow safely
- Scope: in-scope
- Criteria: AC-008

### US-007 Report stage results
- Story: As an MCP-assistant driver, I want to report a stage result with the same validation and audit as the CLI, so that governed progress is recorded
- Scope: in-scope
- Criteria: AC-009

### US-008 Approvals accept a label and obey gate policy
- Story: As a project owner, I want approvals over MCP to accept an approver label and obey the same gate policy as the CLI (AI-mode needs "AI Orchestrator" plus the recommendation file; human-mode refuses "AI Orchestrator"), so that an assistant cannot pass a gate the CLI would refuse, knowing the label is self-asserted (an approval with no label is refused: proposed Ideate default, Q18, not an owner decision)
- Scope: in-scope
- Criteria: AC-010

### US-009 Optional, installable, documented
- Story: As a project owner, I want the MCP server to be an optional second executable with manual configuration docs and honest labels, so that I can adopt it without changing my CLI or host settings unexpectedly
- Scope: in-scope
- Criteria: AC-013, AC-014

### US-010 Telemetry events (intent 2)
- Story: As a maintainer, I want metadata-only telemetry with retention, redaction and privacy tests, so that tool behaviour can be observed
- Scope: out-of-scope
- Criteria: none (intent 2)

### US-011 Workflow event instrumentation (intent 2)
- Story: As a maintainer, I want shared operations and workflow events instrumented, so that slow or failing steps are visible
- Scope: out-of-scope
- Criteria: none (intent 2)

### US-012 Monitor dashboard (intent 2)
- Story: As a maintainer, I want a loopback-only read-only monitor command and dashboard (including mcp-studio-server.html), so that I can inspect telemetry
- Scope: out-of-scope
- Criteria: none (intent 2)

### US-013 Hardening, README, release (intent 2)
- Story: As a maintainer, I want hardening, README and changelog work and a release, so that the feature ships (plan phases 3-5 beyond host-config docs)
- Scope: out-of-scope
- Criteria: none (intent 2)

## Revision notes
- 2026-11-27 (attempt 2): US-003 names both supported hosts (Q1); US-008 reworded from "approvals stay with people" to match Q6 (approver label accepted, same gate policy, label recorded). IDs unchanged. Source: owner decisions 2026-11-27 (User-provided, project owner, via the conductor AskUserQuestion after the first analysis-baseline submission was blocked).
