# User stories: intent 2

Purpose: user stories for the intent-2 scope, each linked to acceptance criteria. Only roles named by the plans or the project are used.
Status: Draft - pending analysis-baseline gate
Sources: plan s1; design s7.1; `PROJECT.md`; `problem-statement.md`; `stakeholder-map.md`; `acceptance-criteria.md`.
Provenance labels: Observed (roles from plan s1, design s7.1, PROJECT.md); User-provided (scope, owner 2026-11-28); Open question for items needing an owner decision.

## Users
- Project owner: the human who decides and approves gates (Observed, PROJECT.md).
- MCP user: a person who drives AIDLC through an MCP-capable assistant (Observed, plan s1).
- Maintainer / local operator: the person who watches tool-call and workflow progress locally (Observed, plan s1, design s7.1).
- CLI user: an existing user of `dtech-aidlc` whose commands must not change (Observed, plan s3.7).

## Stories
### US-001 Privacy-minimal, controllable telemetry
- Story: As an MCP user, I want telemetry to collect only metadata and to be controllable, so that my prompts and project content are never recorded and I know whether collection is on.
- Scope: in-scope
- Criteria: AC-001, AC-002

### US-002 See what my tools and workflow did
- Story: As a maintainer, I want versioned events for tool calls, shared operations, protocol errors and workflow transitions, so that I can see what ran and in what order.
- Scope: in-scope
- Criteria: AC-003, AC-004, AC-005, AC-006, AC-007

### US-003 Bounded, separate local storage
- Story: As a maintainer, I want telemetry stored in separate per-process files that are git-ignored and size and age bounded, so that audit evidence stays untouched and disk and commits stay clean.
- Scope: in-scope
- Criteria: AC-008, AC-009, AC-012

### US-004 Telemetry never harms the workflow
- Story: As an MCP user, I want telemetry failures and the absence of network access to be harmless, so that my operations behave identically and nothing leaves my machine.
- Scope: in-scope
- Criteria: AC-010, AC-011

### US-005 Start a safe local monitor
- Story: As a maintainer, I want to start and stop a read-only, loopback-only monitor on demand, so that I can view activity without exposing controls or data.
- Scope: in-scope
- Criteria: AC-013, AC-014, AC-015, AC-016

### US-006 Understand activity from the dashboard
- Story: As a maintainer, I want a dashboard with honest states, defined metrics, filters, event detail and a privacy disclosure, so that I can tell what is slow, blocked or failing and what led to a failure.
- Scope: in-scope
- Criteria: AC-017, AC-018, AC-019, AC-020

### US-007 Accessible, reviewed dashboard design
- Story: As a maintainer, I want the dashboard to be accessible and its design accepted before it is built, so that it is usable by keyboard and screen reader and matches the privacy map.
- Scope: in-scope
- Criteria: AC-021, AC-022

### US-008 Released, documented version
- Story: As a CLI user, I want a consistent version, a CHANGELOG entry and an accurate README, so that I know what changed and how to use the new capabilities.
- Scope: in-scope
- Criteria: AC-023, AC-024, AC-025

### US-009 Install both binaries
- Story: As a CLI user, I want the install scripts to install, update and uninstall both binaries on Windows and POSIX, so that the MCP server and monitor work from an installed copy.
- Scope: in-scope
- Criteria: AC-026

### US-010 Nothing existing breaks
- Story: As a CLI user, I want existing commands, MCP stdout and CI to stay unchanged except for owner-approved differences, so that upgrading is safe.
- Scope: in-scope
- Criteria: AC-027, AC-028, AC-029

### US-011 Release gated on security review
- Story: As the project owner, I want a threat-model review and honest identity wording before release, so that I can approve the release knowing the loopback, token and privacy risks are addressed.
- Scope: in-scope
- Criteria: AC-030

### US-012 DeepSeek Harness verification
- Story: As the project owner, I want DeepSeek Harness launch-and-list verified, so that the second host is confirmed.
- Scope: out-of-scope (owner deferred, 2026-11-28; plans do not require it for intent 2)
- Criteria: none

### US-013 Inherited test and CI follow-ups
- Story: As the project owner, I want the intent-1 test gaps, timing flakes and CI runtime upgrade handled, so that the release gate is quiet.
- Scope: out-of-scope unless the owner adds it (Open question Q12; not named in the intent text)
- Criteria: none
