# Evidence log: intent 2

Purpose: one row per consulted source. All rows are Observed at commit 327b02a unless stated. Registered sources: PROJECT.md and README.md were consulted.
Status: Draft - pending analysis-baseline gate

| Source id | Path | What was taken | Label | Commit |
|---|---|---|---|---|
| E1 | `PROJECT.md` | Project identity, owner role, stage graph (13 stages at `standard`), version 1.2.2 (lines 235-240), projection-history rule (line 241). Stale text: says no CI, no MSRV (repo disagrees). Not editable here. | Observed | 327b02a |
| E2 | `README.md` | MCP section heading "proposed"; line ~380 says installers are CLI-only; "launch-and-list not yet run" and platform sentences to change under Q10; "What to commit" for `.aidlc/.gitignore`. | Observed | 327b02a |
| E3 | `.aidlc/knowledge/_codebase-map.md` | Component and test baseline; many sections stale (corrections below). | Observed | 327b02a |
| E4 | `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md` | Intent-1 record: ADR-001..018 summary, baseline 545/0, CI runs, known limits, lessons. | Observed | 327b02a |
| E5 | `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/archived/knowledge-writeback/next-intent-notes.md` | Hook point notes (suggests `application.rs`, conflicts with Q1), inherited follow-ups. | Observed | 327b02a |
| E6 | `.aidlc/intents/please-implement-plans-mcp-server-enhancement-sc/ideate/direction-selection/decision-record.md` | Full ADR text (read via the knowledge record references). | Observed | 327b02a |
| E7 | `.aidlc/intents/...-2/analyze/problem-framing/{problem-statement,success-measures,stakeholder-map,acceptance-criteria,user-stories}.md` | AC-001..AC-030, US-001..US-013, Q1..Q15. | Observed | 327b02a |
| E8 | `Cargo.toml`, `Cargo.lock`, `.github/badges/version.svg`, `CHANGELOG.md` | Version 1.2.2 (badge text in two places); deps serde_json, sha2; rust-version 1.100; two bins. | Observed | 327b02a |
| E9 | `rust-cli/application.rs`, `rust-cli/mcp/{mod,protocol,redact,tools}.rs`, `rust-cli/mcp_main.rs`, `rust-cli/main.rs` | Facade (status, queue, next, report) with `Caller`; ToolHost fn pointers and ToolContext; catch_unwind at `protocol.rs:231`; serve is sole stdout writer; park/revisit bypass facade. | Observed | 327b02a |
| E10 | `rust-cli/orchestrate.rs`, `rust-cli/intent_commands.rs`, `rust-cli/dependencies.rs`, `rust-cli/knowledge.rs`, `rust-cli/audit.rs`, `rust-cli/args.rs`, `rust-cli/help.txt`, `rust-cli/completion.zsh` | `with_state_lock` (1229-1340); `deps add` unlocked (`dependencies.rs:1264-1306`); managed block marker `v1` (`knowledge.rs:22`); `ensure_ignore_file` runs from `finish_directive` only; no `monitor` text yet. | Observed | 327b02a |
| E11 | `tests/` (repo_content, mcp_packaging, mcp_stdio, mcp_parity, fixtures/mcp/common.rs, rust_cargo_install, goldens) | Pins on README, CI, lock hash, help/flag invariants; snapshot walkers; installer fixtures. | Observed | 327b02a |
| E12 | `scripts/rust-install/install.sh`, `install.ps1` | CLI-only; marker `.dtech-aidlc-rust-install`; `.cmd` wrapper at `install.ps1:75`. | Observed | 327b02a |
| E13 | `.github/workflows/ci.yml`, `build.rs` | Windows/macOS required legs; installer-not-run pin; build.rs embeds core/ and harness/. | Observed | 327b02a |
| E14 | `plans/mcp-server-enhancement/` (architecture, detailed design, observability plan, `mcp-studio-server.html`) | Plan phases 3-5; design s6, s7.7, s10 (partly skimmed); HTML loads remote fonts, uses `aidlc_*` names. | Observed (partly read) | 327b02a |
| E15 | Baseline run: `cargo build`; `TMPDIR=/private/tmp/aidlc-test PATH=target/debug:$PATH cargo test --no-fail-fast` (macOS, cargo 1.100.0) | 545 passed, 0 failed, 0 ignored; lib 246, repo_content 36, rust_unit_scheduling 77 (slowest, ~20 s); none of the three flakes tripped this run. | Observed | 327b02a |
| E16 | CI run 46090623346 | Green: Windows 547/0, macOS 545/0, Ubuntu 545/0. Taken from the knowledge record (Observed that the record says so), not checked on GitHub. | Observed (in docs) | 327b02a |
| E18 | `.aidlc/shared/project-profile.md`, `.aidlc/shared/sources-snapshot.json`, `.aidlc/config.json`, `.aidlc/knowledge/_index.md` | Directive items `profile`, `repos`, `_index`. Profile: brownfield, 71 source files, rust and shell, architecture none detected; sources are PROJECT.md and README.md only (matches the registered-sources note). Config: mode ai, harness claude, scope solo. Index: one record (intent 1, closed). `repos`: no repos configuration exists under `.aidlc/` (listing shows engine, intents, knowledge, shared only), so nothing to open; `deps` and `repos add` behaviour in code is covered by E10. Profile adds nothing beyond E1-E3. | Observed | 327b02a |
| E17 | CI run 46093693872 | Ubuntu failed on `prerequisites::tests::check_timeout_is_reported`. Run id and result come from the owner decision text (Q12) only; a repo-wide grep at 327b02a finds the id in no repo file, and the knowledge record (line 60) names the three flakes without a run id. Not checked on GitHub. | User-provided (owner, 2026-11-28) | n/a |

## Registered sources not consulted
None beyond the list above; no other registered source was reported.

## Not run
`cargo fmt --check`, `cargo clippy`, `cargo build --release --locked`, pwsh installer tests, any golden with telemetry on, CI logs of both runs (E16, E17).

## Local flake reproduction (Observed by the reviewer, macOS, 327b02a; not re-run by the author)
A reviewer run of the suite gave 544 passed, 1 failed: `artifacts::tests::verifies_empty_missing_and_not_applicable_reason` (`artifacts.rs:423`). Three `--lib` reruns gave FAILED, ok, ok, so the flake failed 2 of 4 runs. `temp()` at `artifacts.rs:371` uses pid plus nanos, so parallel tests can share a directory. E15 (545/0) is a single honest run in which none of the flakes tripped; the 545 baseline is therefore not stable.

## Codebase map corrections needed (map not edited)
1. Header and section 1: "Mapped at commit 69cab5a", "intent 1 of 2 ... Telemetry/monitor are out of scope" are stale; map now serves intent 2 at 327b02a. "No MCP code exists" is false (`application.rs` 266 lines; `mcp/{mod 111, protocol 711, redact 414, tools 490}.rs`; `mcp_main.rs`). "28 pub mods, ~27.5k lines" not re-checked.
2. Section 1/4: "new-dependency decision" and hand-roll assumption resolved by ADR-001/002 and re-confirmed (Q3). Section 4 "no HTTP, no network" needs a new row for the loopback listener.
3. Section 2: no rows for `application.rs`, `mcp/*`, `mcp_main.rs`; install scripts row says "(not read)" and should say CLI-only today, `--from --prefix --upgrade|--uninstall`, marker under `<prefix>/lib/dtech-aidlc` (Q11 changes it).
4. Section 3: "no public facade" and "logic stranded in binary" stale; only status and `orchestrate queue|next|report` use the facade. Telemetry hook is the MCP adapter, not the shared facade (conflict with E5, see open question Q16-adjacent note in open-questions.md).
5. Section 5: "no file lock exists" is wrong (`with_state_lock`, `.aidlc/state.lock`). Remaining gap is `deps add`. Telemetry location and retention absent.
6. Section 7: baseline 450/6 with 28 targets is stale; now 545/0 across 34 targets. Corrected: lib 246, repo_content 36, routing_golden 11, rust_cargo_install 6, rust_orchestration_assets 7; added mcp_packaging 5, mcp_stdio 13, mcp_parity 8, mcp_redaction 8. Six "known baseline failures" no longer fail. Add the three timing flakes. "No MCP tests" and "AC-001..AC-014" are stale (intent 2 uses AC-001..AC-030).
7. Section 8: "no CI workflows found" contradicts the CI row; `ci.yml` exists and its "not yet run" note is outdated (runs E16, E17).
8. Section 9: item 1 partly resolved; item 5 resolved; item 6 (clippy warning) stays but is out of scope (Q12). Add intent-2 risks (see risk-register.md).
9. Section 10: plans directory now has four files; managed .gitignore generator is `ensure_ignore_file` via `finish_directive` in `rust-cli/orchestrate.rs`, block label in `rust-cli/knowledge.rs:22` (confirmed by the risk research; `grep gitignore` in `rust-cli/onboarding/` finds nothing).
10. Missing: untracked `.claude/worktrees/`, the intent-2 record, the intent-1 knowledge record.
11. Observed: `intent_commands.rs:41` imports `with_state_lock` and `persist_workflow` calls it (`:317`; also `:786`, `:834`, `:883`). Not verified: `repos add` uses a separate lock in `dependencies.rs:272` (`deps.lock`); whether it covers the same files is unchecked.
