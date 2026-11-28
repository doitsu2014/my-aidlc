# Open questions: intent 2

Q1..Q15 are carried from problem-framing; Q16..Q25 are new. Decisions are User-provided (project owner, 2026-11-28, AskUserQuestion in the conductor session before context-and-risk). The same decision status was also noted in `problem-framing/problem-statement.md` (conductor-authorised; see CON17 for the hash mismatch with `state.json`). Ideate research decisions carry a safe default, an Assumption until Ideate confirms.
Status: Draft - pending analysis-baseline gate

## Decided
| Q | Decision (User-provided, owner, 2026-11-28) | Criteria |
|---|---|---|
| Q1 | Telemetry: the MCP server only records telemetry; default ON with an opt-out flag; the CLI writes no telemetry (CLI goldens unchanged). Flag name and any env var are Ideate details. | AC-001, AC-006, AC-027 |
| Q2 | Scoped re-bless allowed ONLY for goldens that change because of the version bump, the `monitor` command in help/completion/validation, and the managed .gitignore block line; every diff reviewed; every other golden byte-identical. | AC-027, AC-023 |
| Q3 | Still zero new crates; loopback HTTP server, id generation and token are hand-rolled with std (recorded as an ADR in Ideate). | AC-029, AC-011 |
| Q4 | `monitor` is listed in help (same scoped re-bless as Q2). | AC-025, AC-027 |
| Q5 | Version 1.3.0, prepare only (Cargo.toml, Cargo.lock, badge, CHANGELOG, goldens); no tag, no publish. | AC-023, AC-024 |
| Q10 | Remove "proposed" once CI is green on Windows+macOS AND a live Claude Code tools/call is observed; DeepSeek entry stays "not verified"; README platform sentence updated in the same unit. | AC-025 |
| Q11 | `scripts/rust-install` install/upgrade/uninstall both binaries by default, tested in sh and PowerShell. | AC-026 |
| Q12 | In scope: fix timing-flaky tests (`prerequisites::tests::check_timeout_is_reported`, `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`); add missing intent-1 MCP tests (AI-mode gate over MCP, unsafe layout, CLI+MCP interleaving, --help/--version at install); route `deps add` through `with_state_lock`. Out: checkout Node 20, actionlint, `orchestration_golden.rs:1108` warning. US-013 becomes in scope for the in-scope items. | AC-027, AC-029 |
| Q13 | Telemetry may store validated ids (intent/stage/unit), no free-form text. | AC-002, AC-020 |
| Q15 | Update `mcp-studio-server.html` to match the design (correct tool names, no remote fonts, correct labels); the AI reviews the dashboard design (no human design gate). | AC-022, AC-011 |
| - | DeepSeek Harness verification stays deferred (owner, 2026-11-28). | US-012 |

## Ideate research decisions (safe default, Assumption until confirmed)
The owner here is a stage (Ideate), not a person. [follow-up] Q6, Q7, Q8, Q9 and Q14 are follow-ups for Ideate; no Analyze-gate decision depends on them.
| Q | Question | Owner | Safe default |
|---|---|---|---|
| Q6 | Retention numbers, rotation behaviour, pruning moment, Windows permissions | Ideate | 5 MiB per file, 7 days, 50 MiB total; new suffixed file at cap; prune at start and rollover only; ignore delete errors; Unix 0o600, Windows inherited ACLs documented |
| Q7 | Monitor token mechanism (needed at all?) | Ideate + AI threat review | Loopback bind plus exact Host/Origin; 128-bit token (source to weigh in Ideate: `RandomState` plus sha2, or an OS RNG read such as `/dev/urandom` or a Win32 call; std gives no cryptographic guarantee for `RandomState`), delivered as URL fragment printed only to the foreground terminal; validate on three OSes |
| Q8 | Which events ship; park/revisit bypass the facade | Ideate | Park, revisit and pause events dropped from v1 (CLI writes none under Q1); dashboard labels the coverage gap |
| Q9 | Error and outcome mapping | Ideate | directive/complete to ok, blocked/busy to blocked, Err to error with existing four codes plus protocol codes; no typed-error work (ADR-008); optional protocol observer |
| Q14 | Latency sample minimum, liveness, shutdown, maximum query range | Ideate | Percentile only at n>=20, n shown; no liveness ("unknown"); in-process foreground monitor, Ctrl+C default; a stated maximum range chosen in Ideate |

## New questions found in analysis
| Q | Question | Owner | Why |
|---|---|---|---|
| Q16 | Who refreshes PROJECT.md (1.2.2, "no CI", no MSRV) and the README/PROJECT.md baseline of 454 passed and 2 failed? This workflow cannot edit PROJECT.md. | Owner | Stale facts persist after release (R26). |
| Q17 | Exact list of missing intent-1 MCP tests; intent-1 AC numbers collide with intent-2 numbering. | Ideate (technical-spec stage) | Scope of Q12 test work (R19). |
| Q18 | Can the `DTECH_AIDLC_ASSETS` wrapper env var be dropped (no reader found), and is the CLI `.cmd` wrapper kept? | Ideate | Installer design (R14). |
| Q19 | Monitor in-process vs supervisor and hidden child (ASM3). | Ideate | Orphan risk, flag-table rule, installed launch (R13). |
| Q20 | How a trusted "configured off" setting reaches the monitor when the CLI writes no settings file; flag name and optional env var for the opt-out. | Ideate | AC-001 "unknown" state; Q1 detail. |
| Q21 | Who supplies the live Claude Code tools/call observation and when; if absent, the label stays and release notes say so. | Owner | Gates Q10 (R25). |
| Q22 | Conflict: intent-1 notes put the hook in `rust-cli/application.rs`; Q1 says the CLI writes nothing. Confirm hook at the MCP adapter (`rust-cli/mcp/tools.rs`) or a `Caller::Mcp` branch that is a no-op for the CLI. | Ideate | CON4. |
| Q23 | Confirm flake root causes before fixing, from CI logs for run 46093693872 (id User-provided, Q12; not found in the repo) and from repeated local loops; the `artifacts` flake already reproduced locally in 2 of 4 runs (reviewer). No source names other failing runs. | Develop (first flake-fix unit) | ASM4. |
| Q24 | May the shared MCP test snapshot (`tests/fixtures/mcp/common.rs:244`) skip `.aidlc/telemetry`? It changes what `tests/mcp_parity.rs:96` onward and `tests/mcp_stdio.rs:548-549` compare, and Q2/Q4 allows no change beyond three golden re-bless reasons. Alternatives: telemetry only writes when `.aidlc` exists (then tests that pre-create `.aidlc` need an opt-out arg), or tests pass the opt-out flag. | Owner | R1, ASM5. |
| Q25 | Who refreshes `.aidlc/knowledge/_codebase-map.md` for the 11 corrections in evidence-log.md (stale 450/6 baseline, "No MCP code exists", "no file lock exists", and others)? Analyze exit needs the map reviewed by the owner. This stage must not edit it. | Owner | R26; map stays stale until assigned. |

## Test-source edits the decisions imply (not golden re-blesses; for owner review)
Distinct from the Q2/Q4 golden re-bless list. Each is a hand edit to test or code-test source, Observed at 327b02a.
| Edit | Driven by | Cited in |
|---|---|---|
| `tests/mcp_packaging.rs:6` `LOCK_SHA256` recomputed | Q5 version bump changes the Cargo.lock package line | CON2, R20 |
| `tests/repo_content.rs` ~1941-1946 heading `## MCP server (proposed)` | Q10, only if the live observation happens | CON6, R25 |
| `tests/repo_content.rs` pins that installers are CLI-only and CI must not run them, and `README.md:378-380` text | Q11 | CON7, R15 |
| `rust-cli/mcp/mod.rs:86` unit test `parse_args_accepts_only_project` | Q1 opt-out flag | CON4 |
| `tests/repo_content.rs:1462-1492` CHANGELOG heading and badge ties (new `## 1.3.0` entry, no test edit expected) | Q5 | CON2 |
| Flake fixes in `prerequisites.rs`, `artifacts.rs`, `onboarding/projection.rs` tests; new intent-1 MCP tests | Q12 | CON5, R18, R19 |
| Possible `tests/fixtures/mcp/common.rs:244` snapshot helper change | Only if the owner answers yes to Q24 | R1 |
