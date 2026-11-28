# Artifact index: please-implement-plans-mcp-server-enhancement-sc-2

All paths are under `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc-2/` and exist (Observed).

## Task and progress
- `intent.md`, `state.json`

## Analyze
- `analyze/problem-framing/`: problem-statement.md, success-measures.md, stakeholder-map.md, acceptance-criteria.md (AC-001..AC-030), user-stories.md (US-001..US-013)
- `analyze/context-and-risk/`: constraints-and-assumptions.md, evidence-log.md, risk-register.md, open-questions.md, quality-requirements.md, gate-recommendation.md

## Ideate
- `ideate/option-generation/`: solution-options.md, trade-off-matrix.md
- `ideate/direction-selection/`: recommended-direction.md, decision-record.md (ADR-019..ADR-039), scope-boundary.md, architecture-design.md, threat-model.md (T2-01..T2-24), gate-recommendation.md

## Develop
- `develop/baseline.md`
- `develop/implementation-design/`: implementation-plan.md, functional-design.md, interfaces-and-contracts.md, test-strategy.md, work-units.md, work-units.json, ids.json
- `develop/red-tests/WU-0xx/`: failing-tests.md, red-evidence.md (WU-011..WU-015, WU-017..WU-021; WU-016 and WU-022 skipped: no owned tests)
- `develop/build-and-verify/WU-0xx/`: implementation.md, green-verification-report.md, regression-report.md, known-limitations.md, regression.json (all 12 units); extras: WU-016/dashboard-design-pack.md, WU-016/design-review.md, WU-019/browser-shots/*.png, WU-021/threat-model-review.md
- `develop/build-and-verify/` (final sweep): implementation.md, green-verification-report.md, regression-report.md, known-limitations.md, regression.json, rc-manual-pass.md, gate-recommendation.md

## Archived
- `archived/intent-record/`: intent-summary.md, completed-scope.md, artifact-index.md

## Product changes (repository, Observed in `git log 327b02a..HEAD`)
- `rust-cli/telemetry/{mod.rs,sink.rs,query.rs}`, `rust-cli/monitor/{mod.rs,http.rs,assets/}`, `rust-cli/mcp/*`, `scripts/rust-install/{install.sh,install.ps1}`, `README.md`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `.github/badges/version.svg`, tests (`tests/{deps_lock,telemetry_guards,mcp_telemetry,mcp_followups,monitor_http,monitor_assets}.rs` new, others extended), eight goldens, `plans/mcp-server-enhancement/mcp-studio-server.html`.
