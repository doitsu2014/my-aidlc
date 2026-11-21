Implementation plan

Phase 1: AI Orchestrator Gates mode

Design choices locked in:
- Mode selection: Hybrid — config default (ai or human) + per-intent override via --mode flag
- AI approval rigor: Research + recommendation with traced rationale
- Gate restrictions: All gates allowed (no blocking)
- Sequence: AI orchestrator first, then question tool integration

Files to change:

1. core/data/phase-graph.json — bump schema to 1.5.0, add orchestrationMode field
2. core/tools/cli-spec.mjs — add mode: 'string' to orchestrate next and config flags
3. core/tools/dtech-aidlc.mjs — add config mode [ai|human] command, accept --mode on orchestrate next
4. core/tools/workflow.mjs — orchestrateNext returns orchestrationMode: "ai"|"human" in directive; AI mode gates produce a gate-recommendation artifact before auto-approving
5. core/methodology/README.md — document AI orchestrator mode, its evidence requirements, and the hybrid selection model
6. core/methodology/contracts.md — add AI gate approval contract: must produce gate-recommendation.md with researched options, selected direction, evidence trace, and approval rationale
7. Agent files (all 12) — add AI orchestrator instructions: "When orchestrationMode: 'ai', research the gate decision, write gate-recommendation.md, then report with --approval approved --approved-by 'AI Orchestrator'"
8. harness/claude/skills/aidlc/SKILL.md and harness/deepseek/skills/aidlc/SKILL.md — update to handle AI mode directive, explain when to call AskUserQuestion (phase 2), and how to produce gate-recommendation artifacts
9. guidelines/README.md — add section explaining AI vs human mode, when to use each, and the traced-rationale requirement
10. README.md — mention AI orchestrator mode in the "How AIDLC flows" section
11. Tests: tests/orchestration-mode.test.mjs — validate config storage, directive emission, AI gate auto-approval, and artifact creation

Phase 2: AskUserQuestion integration (next)

Design choice: Direct inline tool calls by agents, no intermediate artifact file.

What changes:
- Agent methodology files get guidance on constructing AskUserQuestion calls with clear options and descriptions
- Harness skills get examples of when/how to call AskUserQuestion for material unknowns during stage execution (not just gates)
- Guidelines document when agents should pause with a question vs. making a working assumption

---