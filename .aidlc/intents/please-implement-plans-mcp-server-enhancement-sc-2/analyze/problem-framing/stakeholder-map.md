# Stakeholder map: intent 2

Purpose: name the people and roles affected. Only roles named by the plans or the project are listed; none are invented.
Status: Draft - pending analysis-baseline gate
Sources: plan s1 ("people who drive AIDLC through an MCP-capable assistant, and maintainers"); design s7.1 ("a local operator/maintainer"); `PROJECT.md` (project owner, assistant, CLI); `README.md`; intent-1 knowledge record.
Provenance labels: Observed, User-provided, Assumption, Open question.

| Stakeholder | Interest | Influence | Provenance |
|---|---|---|---|
| Project owner (human) | Decides open questions Q1-Q15; approves gates and the release; privacy decision on telemetry default | Decides | Observed (PROJECT.md); User-provided (scope decision, 2026-11-28) |
| MCP user: person driving AIDLC through an MCP-capable assistant (Claude Code is the required host) | Wants to see tool-call activity and progress locally; must not have prompts or content captured | Consulted | Observed (plan s1) |
| Maintainer / local operator | Wants slow, blocked and failing operations and event sequences; runs the monitor | Consulted | Observed (plan s1, design s7.1) |
| Assistant (AI agent) | Calls the MCP tools; telemetry must not alter its results or stdout | Informed | Observed (PROJECT.md) |
| CLI user (existing users of `dtech-aidlc`) | Needs unchanged commands, output, exit codes and file effects; upgrades via install scripts | Affected | Observed (plan s3.7, ADR-006) |
| Release installer user | Installs both binaries via the scripts or cargo on Windows or POSIX | Affected | Observed (plan AC 12) |
| Security / threat-model reviewer | Reviews loopback exposure, token, Host/Origin, paths, retention, privacy | Gate input; the role is needed, owner unnamed | Observed (plan Phase 5); Open question (owner names the reviewer) |
| Design reviewer for dashboard pack | Accepts wireframes, states, privacy map, API contract before implementation | Gate input; owner unnamed | Observed (plan s3.6); Open question Q15 |

Deferred (not a stakeholder for this intent): DeepSeek Harness hosts, owner-deferred 2026-11-28 (User-provided).
Not named by any source, so not listed: remote or hosted telemetry consumers (explicit non-goal), authenticated identity systems (approver names stay self-asserted, ADR-013).
