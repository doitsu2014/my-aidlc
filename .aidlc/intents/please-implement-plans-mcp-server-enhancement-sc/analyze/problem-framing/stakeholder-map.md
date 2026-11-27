# Stakeholder map

- Purpose: name the people and systems affected by intent 1, with their interest.
- Status: Draft - pending analysis-baseline gate
- Sources: plan header and s2.2; architecture s5; design s4.4, s7.1. Only roles the docs name are listed.

| Stakeholder | Role | Interest / concern | Provenance |
|-------------|------|--------------------|------------|
| People who drive AIDLC through an MCP-capable assistant | Primary user | Structured discovery and calls; safe state changes | Observed (plan header) |
| Project owner / scripts on the CLI path | User and approver | CLI unchanged; owner completes human-mode gate approvals via CLI or, under Q6, over MCP with a self-asserted approver label; owner decides open questions | Observed (architecture s5; design s4.4); owner is User-provided scope author |
| Assistant harness (MCP host) | Integration peer | Launches the stdio process and relays tool calls; not an authorization authority | Observed (plan s2.2) |
| Maintainers | Maintain codebase and release | Named for the dashboard; for this intent only as reviewers of CLI refactor and dependency choice | Observed (plan header); relevance to intent 1 is an Assumption (validate with owner) |
| Local operator / maintainer viewing telemetry | Dashboard user | Out of scope (intent 2) | Observed (design s7.1) |

Supported hosts: Claude Code and DeepSeek Harness (User-provided, project owner, 2026-11-27, Q1); the plan names them only in a non-goal (plan s1). Decision authority: the project owner; approvals are self-asserted, not authenticated (Observed, identity.rs); an MCP approver label is accepted on that basis (User-provided, project owner, 2026-11-27, Q6).
