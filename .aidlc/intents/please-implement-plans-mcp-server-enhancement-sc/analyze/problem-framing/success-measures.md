# Success measures

- Purpose: state how success of intent 1 will be recognized.
- Status: Draft - pending analysis-baseline gate
- Sources: plan s5-s6 phase exits and acceptance list; architecture s11; problem-statement.md
- Note (Assumption): the docs give no numeric baseline or target; measures are pass/fail. Validate with the owner.

| # | Measure | Evidence of success | Provenance | Criteria |
|---|---------|---------------------|------------|----------|
| 1 | CLI unchanged | All existing tests and goldens pass unchanged in a defined environment; golden files not re-blessed | Observed (plan Phase 1 exit) | AC-001 |
| 2 | One behaviour path | CLI and MCP produce the same semantic results, project-file effects and audit rows on equivalent fixtures; no subprocess proxy or second engine | Observed (plan s5, s6 AC2) | AC-002, AC-009 |
| 3 | Real server works | A child-process test drives initialize, tool list, valid call, schema error, domain error, shutdown and restart over stdin/stdout; both supported hosts, Claude Code and DeepSeek Harness, launch the server and list the four tools | Observed (plan Phase 2 exit); hosts User-provided (project owner, 2026-11-27) | AC-003 |
| 4 | Protocol hygiene | Every stdout byte is a protocol frame; no network listener | Observed (plan s6 AC4) | AC-004 |
| 5 | Safe by construction | Invalid, oversized or unknown-field requests and every rejected mutation leave state and audit unchanged | Observed (design s9) | AC-005, AC-008, AC-009, AC-011, AC-012 |
| 6 | Gate integrity | MCP approvals follow exactly the CLI gate policy (AI-mode needs AI Orchestrator and the recommendation file; human-mode refuses AI Orchestrator); approver labels never described as authenticated | Observed (plan s6 AC6) | AC-010 |
| 7 | Optional and installable | CLI works without MCP; both binaries install, run and uninstall from outside the checkout | Observed (plan s5) | AC-013 |
| 8 | Honest labelling | Docs say proposed until released | Observed (plan s6 AC13) | AC-014 |

Estimate: none provided; effort sizing belongs to later stages.

## Revision note
- 2026-11-27 (attempt 2): measure 3 names both hosts (Q1); measure 6 reworded to match Q6. Source: owner decisions 2026-11-27 (User-provided, project owner, via the conductor AskUserQuestion after the first analysis-baseline submission was blocked).
