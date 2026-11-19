# my-aidlc for DeepSeek Harness

This project uses my-aidlc, an AI-Driven Development Life Cycle workflow engine. The CLI selects the next stage and owns workflow state. DeepSeek Harness performs only the directed work and preserves the required human phase approvals.

## Start

Invoke `/aidlc` and describe the work, for example:

```text
/aidlc Build a REST API for inventory management
```

On first use, `/aidlc` asks for a workflow scope (`solo`, `team`, or `enterprise`) if one is not configured, initializes the project, then starts the requested intent. Use `/aidlc` to continue an active intent. Check progress with `my-aidlc status`; validate setup with `my-aidlc doctor`.

## Human decisions

Review each phase's artifacts at the paths shown by the orchestrator. Every phase has an explicit approval gate. The workflow pauses for your decision; approval is never inferred from silence, tool output, or a model recommendation. Requested changes are revised and checked before the gate is presented again.

## Tool availability

Only capabilities available in the current DeepSeek Harness session may be used. This setup does not promise a `Workflow` tool, subagents, or delegation. If these are unavailable, the stage is handled directly rather than simulating a missing capability.

## Installed files

- `.dsh/skills/aidlc/SKILL.md` — `/aidlc` orchestrator instructions
- `.dsh/tools/` — my-aidlc CLI
- `.aidlc/methodology/` — project-local copy of the human-readable methodology; treat as read-only
- The CLI's `core/data/phase-graph.json` is its machine-readable workflow index; use `my-aidlc scope` and `my-aidlc doctor` to inspect it.
- `.aidlc/` — project configuration, state, audit log, and generated artifacts
