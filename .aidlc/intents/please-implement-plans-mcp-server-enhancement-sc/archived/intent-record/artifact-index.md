# Artifact index: please-implement-plans-mcp-server-enhancement-sc

- Purpose: list every artifact file that exists in the intent folder, grouped by phase and stage, and the project-level files changed or added for this intent. Use it to find a file without opening every stage.
- Status: Draft for owner review (intent-record stage, 2026-11-28).
- Sources: `find` on `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/` (105 files, Observed); `state.json` stage records (Observed); `git diff --name-only 69cab5a..HEAD` for project files (Observed).
- Scope note: the solo scope has no Launch or Curate phase. No outputs for those phases exist, and none are listed. The intent folder has no `launch/` or `curate/` directory.
- Paths are relative to the project root, `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/`, unless shown in full.

## 1. Intent folder artifacts (105 files)

| Phase / stage | Folder | Files |
|---|---|---|
| Intent | `./` | `intent.md` (request, User-provided); `state.json` (progress record, modified and uncommitted at intent-record) |
| Analyze / problem-framing | `analyze/problem-framing/` | `problem-statement.md`, `success-measures.md`, `stakeholder-map.md`, `acceptance-criteria.md`, `user-stories.md` |
| Analyze / context-and-risk | `analyze/context-and-risk/` | `constraints-and-assumptions.md`, `evidence-log.md`, `gate-recommendation.md` (analysis-baseline, attempt 2), `open-questions.md`, `quality-requirements.md`, `risk-register.md` |
| Ideate / option-generation | `ideate/option-generation/` | `solution-options.md`, `trade-off-matrix.md` |
| Ideate / direction-selection | `ideate/direction-selection/` | `architecture-design.md`, `decision-record.md`, `gate-recommendation.md` (solution-direction), `recommended-direction.md`, `scope-boundary.md`, `threat-model.md` |
| Develop / implementation-design | `develop/implementation-design/` | `functional-design.md`, `ids.json` (identifier registry), `implementation-plan.md`, `interfaces-and-contracts.md`, `test-strategy.md`, `work-units.json`, `work-units.md` |
| Develop / baseline | `develop/` | `baseline.md` |
| Develop / red-tests | `develop/red-tests/WU-001` to `WU-010` | For each unit: `failing-tests.md`, `red-evidence.md` (20 files) |
| Develop / build-and-verify (final sweep) | `develop/build-and-verify/` | `gate-recommendation.md` (release-candidate, attempt 2), `green-verification-report.md`, `implementation.md`, `known-limitations.md`, `regression-report.md`, `regression.json` |
| Develop / build-and-verify (per unit) | `develop/build-and-verify/WU-001` to `WU-010` | For each unit: `implementation.md`, `green-verification-report.md`, `known-limitations.md`, `regression-report.md`, `regression.json` (50 files) |
| Archived / intent-record (this stage) | `archived/intent-record/` | `intent-summary.md`, `completed-scope.md`, `artifact-index.md` |

Counts: 105 files existed in the intent folder before this stage, plus the three files written here.

## 2. Project-level files changed or added (git, `69cab5a..HEAD`)

Observed. These sit outside the intent folder. Source and test files are grouped; the full set is listed in the `git diff --name-only` output.

| Area | Files | Notes |
|---|---|---|
| Codebase knowledge | `.aidlc/knowledge/_codebase-map.md` (added) | Status draft at analyze. Owner review was replaced by the conductor's review (solution-direction gate). |
| Project guide | `PROJECT.md` (added, 333 lines) | Project guide written during onboarding. |
| Documentation | `README.md` (68 lines added) | MCP section labelled "proposed". DeepSeek Harness entry labelled not verified. |
| CI | `.github/workflows/ci.yml` (added) | Windows, macOS and Ubuntu (best effort) build, test and transcript steps. WU-008. |
| Build manifest | `Cargo.toml` (6 lines added) | Second binary target. Version 1.2.2 unchanged. `Cargo.lock` unchanged. |
| Onboarding and tooling | `.aidlc/.gitignore`, `.aidlc/request.md`, `aidlc.knowledge.json`, `.claude/skills/dtech-aidlc/SKILL.md`, `.claude/skills/dtech-aidlc/onboarding.md`, `.claude/skills/dtech-aidlc/.projection.json`, `.gitignore` (2 lines removed) | Added or changed in commit `d765edd` (onboarding). |
| Facade and CLI | `rust-cli/application.rs` (added), `rust-cli/intent_commands.rs`, `rust-cli/lib.rs`, `rust-cli/main.rs`, `rust-cli/orchestrate.rs`, `rust-cli/prerequisites.rs`, `rust-cli/onboarding/mod.rs`, `rust-cli/onboarding/projection.rs` | Shared application facade and CLI routing (WU-002, WU-003). |
| MCP server | `rust-cli/mcp_main.rs` (added), `rust-cli/mcp/mod.rs`, `rust-cli/mcp/protocol.rs`, `rust-cli/mcp/redact.rs`, `rust-cli/mcp/tools.rs` (all added) | Binary entry, dual-era protocol layer, path redaction and tool adapter (WU-001, WU-004, WU-005, WU-006). |
| Tests | `tests/mcp_stdio.rs`, `tests/mcp_parity.rs`, `tests/mcp_redaction.rs`, `tests/mcp_packaging.rs`, `tests/rust_cargo_install.rs`, `tests/repo_content.rs` (added or changed); `tests/fixtures/mcp/common.rs`, `tests/fixtures/mcp/legacy_session.jsonl`, `tests/fixtures/mcp/modern_session.jsonl` (added) | MCP transcript, parity, redaction, packaging and install tests (WU-007, WU-010). Golden fixtures unchanged. |

Not changed by this intent (Observed): `Cargo.lock`, `tests/golden/`.

## 3. Not yet written (next stage, not an artifact of this stage)

- `.aidlc/knowledge/<intent-id>.md`: the knowledge record, to be written by knowledge-writeback. It does not exist yet.
