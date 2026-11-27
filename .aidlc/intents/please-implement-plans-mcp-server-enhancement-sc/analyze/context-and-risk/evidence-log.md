# Evidence log

- Purpose: one row per source consulted for the analysis baseline of intent 1 of 2.
- Status: Draft - pending analysis-baseline gate
- Sources: listed below. All rows are Observed at git commit 69cab5a unless stated. Statements about lines that were read by a parallel research reader and not re-opened by this writer are marked "(via research)".

| ID | Path | What was taken | Label | Commit |
|----|------|----------------|-------|--------|
| E1 | PROJECT.md (registered source `project`; directive context.knowledge.records is empty, and no `profile` or `repos` context entries were supplied, so those listed inputs were absent, not skipped) | Purpose, glossary, conventions, non-negotiables, known baseline failures (17 default env, 2 with fixed TMPDIR), toolchain note, golden/bless rules | Observed | 69cab5a |
| E2 | README.md (registered source `README.md`) | Install commands using `--bin dtech-aidlc`, required sections, README sh-block rules, no MCP mention (grep) | Observed | 69cab5a |
| E3 | .aidlc/knowledge/_codebase-map.md | Components, entry points, test run (28 targets, 450 passed, 6 failed, all in baseline), gaps | Observed | 69cab5a |
| E4 | .aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/intent.md and analyze/problem-framing/{problem-statement,success-measures,stakeholder-map,acceptance-criteria,user-stories}.md | Scope, AC-001..AC-014, US-001..US-013, 15 open questions, 12 contradictions | Observed (User-provided scope) | 69cab5a |
| E5 | plans/mcp-server-enhancement/mcp-server-architecture.md, mcp-server-detailed-design.md, mcp-server-observability-implementation-plan.md | Proposed design, phases 1-2, non-goals, SDK/offline notes (via problem-framing and research; the observability plan was not read in full; plan sections cited in constraints are second-hand via problem-statement.md) | Observed (all MCP items Proposed) | 69cab5a |
| E6 | Cargo.toml, Cargo.lock | Two direct deps, preserve_order, one lib and one bin, 25 locked packages, no rust-version | Observed | 69cab5a |
| E7 | rust-cli/main.rs | Logic stranded in the binary: validation, root resolution, exit mapping, print_json, DTECH_AIDLC_HOME, stdin intent file | Observed | 69cab5a |
| E8 | rust-cli/orchestrate.rs | BLOCKED_EXIT_CODE 3, ensure_unchanged (1071-1088), prerequisite checks run only in orchestrate_next (run_prerequisites at 1470, 1544, 1694; function starts 1142); force_resume overrides the hold only (1699, 1745), lexical resolve_like_node, absolute paths in directives | Observed | 69cab5a |
| E9 | rust-cli/gate.rs, rust-cli/identity.rs | Approver rules and git/OS fallback; resolve_approver call sites not fully traced | Observed | 69cab5a |
| E10 | rust-cli/prerequisites.rs | Shell execution; stdout/stderr piped; no stdin setting found by grep | Observed | 69cab5a |
| E11 | rust-cli/status.rs, audit.rs, workflow/mod.rs | status takes optional intent id; absolute intentDir in output; append-only audit; atomic temp+rename writes; assert_safe_state_dir | Observed (partly via research) | 69cab5a |
| E12 | tests/rust_cargo_install.rs, scripts/rust-install/{install.sh,install.ps1} | Offline locked install test; single-binary scripts and fixtures | Observed (via research) | 69cab5a |
| E13 | tests/repo_content.rs | README sh-block matcher (strip_prefix "dtech-aidlc" then whitespace), help lockstep, changelog and README section rules | Observed | 69cab5a |
| E14 | Test run: `cargo test --no-fail-fast` (map agent, 2026-11-27, rustc 1.100.0, macOS) | 450 passed, 6 failed (2 real, 4 TMPDIR), no new failures; rust_doctor 10/10 | Observed | 69cab5a |
| E15 | .github/ listing | Only badges/; no workflows | Observed | 69cab5a |
| E16 | aidlc.knowledge.json | Registered sources: project, README.md only | Observed | 69cab5a |
| E17 | rust-cli/unit_commands.rs | `queue` is read-only; unmigrated state marked `migration: pending`, not persisted (lines 7-8, 182). Baseline for AC-007 already met | Observed | 69cab5a |
| E18 | Owner decisions Q3, Q5, Q6, Q1 (source: conductor AskUserQuestion, project owner, 2026-11-27, after the first analysis-baseline submission was blocked) | Q3 MCP-binary-only dependency policy; Q5 never run prerequisites from MCP; Q6 accept approver label, same gate policy; Q1 hosts Claude Code and DeepSeek Harness | User-provided | n/a |

## Registered sources not consulted
None. Both registered sources (project, README.md) were read.

## Corrections to the map
- Observed: the map says it did not verify absence of locking; research grep found none (still Assumption A1).
- Observed: DTECH_AIDLC_HOME is read in several lib modules besides main.rs (config_commands.rs:67, doctor.rs:119, intent_commands.rs:200, ids.rs:567, knowledge.rs:895; via research), which strengthens R13, so "private to the binary" in problem-statement Q15 is partly wrong.
- Observed (review fix, commit 69cab5a): earlier text said the CLI report path runs prerequisite checks and that checks run "unless --force-resume". Both are wrong. run_prerequisites is called only at orchestrate.rs:1470, 1544 and 1694, all inside orchestrate_next (starts line 1142); orchestrate_report and finish_report only check that needs names are known (require_known_prerequisites, orchestrate.rs:2986), and main.rs:437-457 runs no checks. force_resume does not gate execution; it only overrides the hold (lines 1699, 1745), so it cannot be reused as the Q5 no-exec guard. AC-008, AC-009, constraint 10, problem-statement contradiction 6 and R3 corrected. Q5 stands as decided.

## Not run
Release build, fmt, clippy, a /private TMPDIR run, any SDK spike, Windows paths, size or build-time measurement.

## Revision note
- 2026-11-27 (attempt 2): E18 added; E16/E17 put in ID order; duplicated parenthetical in "Corrections to the map" removed.
- 2026-11-27 (review fixes): Corrections entry and E8 updated for the prerequisite call sites.
