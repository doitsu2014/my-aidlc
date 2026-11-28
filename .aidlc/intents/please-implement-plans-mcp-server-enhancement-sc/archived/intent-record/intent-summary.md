# Intent summary: please-implement-plans-mcp-server-enhancement-sc

- Purpose: close-out record for intent 1 of 2 (shared application facade and MCP stdio server). A reader who did not run the intent should be able to tell what was asked, what was done, what the owner decided, how each gate was passed, what evidence exists, and what was left out.
- Status: Draft for owner review. Written by the intent-record stage on 2026-11-28. The knowledge-writeback stage has not run, and the project owner has not approved the intent close.
- Scope: solo. Depth: standard. Mode: ai. Gate mode: ai. Final stage: knowledge-writeback.
- Sources: `intent.md` (User-provided request); `state.json` (stage, gate and work-unit records); `analyze/`, `ideate/` and `develop/` artifacts, including the three `gate-recommendation.md` files; `develop/build-and-verify/green-verification-report.md`; `develop/build-and-verify/gate-recommendation.md`; `git log 69cab5a..HEAD` (Observed).
- Provenance labels: **Observed** = read from the intent folder, git history or a named check. **User-provided** = stated by the project owner, with the date given in the artifacts. Items labelled Assumption or Open question are carried as written in the source artifacts.

## 1. What the intent asked

User-provided (`intent.md`): "please implement @plans/mcp-server-enhancement". The owner split the plans into two intents. This intent (intent 1 of 2) covers the shared application facade (CLI refactor) and the MCP stdio server binary: plan phases 1-2 of `plans/mcp-server-enhancement/mcp-server-observability-implementation-plan.md`, with the related parts of `mcp-server-architecture.md` and `mcp-server-detailed-design.md`. Telemetry, the monitor dashboard and plan phases 3-5 are out of scope and belong to intent 2.

## 2. What was done

Observed (`state.json`, `git log`, `develop/build-and-verify/green-verification-report.md`):

- All 10 work units (WU-001 to WU-010) reached `done`. Commits run from `9697529` to `ca30a85` on branch `feature/please-implement-plans-mcp-server-enhancement-sc`, with one design-decision commit (`0487f49`) and a final record commit (`38ebf90`). The per-unit list is in `completed-scope.md`.
- The CLI now routes through a shared application facade. CLI goldens are unchanged, and the local sweep matches the baseline pass/fail set after the two baseline failures were fixed (see section 6).
- A second binary, `dtech-aidlc-mcp`, is declared in the same package. It speaks a hand-rolled JSON-RPC over stdio that supports both protocol eras (legacy `initialize` and the stateless flow). It exposes exactly four tools: `status`, `queue`, `next`, `report`.
- MCP guards: path redaction; no prerequisite shell checks run over MCP (`next` and `report` return pending prerequisites marked not executed); an approval without an explicit approver label is refused; state writes are serialised with an advisory file lock.
- Zero new crates. `Cargo.lock` and `tests/golden` have no diff against `69cab5a`. `Cargo.toml` gains the second binary target (6 lines). The version stays 1.2.2.
- A CI workflow (`.github/workflows/ci.yml`) runs build, test, installer and MCP transcript checks on `windows-latest` and `macos-latest`, with `ubuntu-latest` as best effort.
- `README.md` gained an MCP section that labels the server "proposed" and marks the DeepSeek Harness entry as not verified.
- The first-pass Analyze, Ideate and Develop artifacts, and the codebase map `.aidlc/knowledge/_codebase-map.md`, were written in the intent and project folders (see `artifact-index.md`).

## 3. Owner decisions taken along the way

| Date | Decision | Provenance |
|---|---|---|
| 2026-11-27 | Q1: supported hosts are Claude Code and DeepSeek Harness; both must pass launch-and-discover. Narrowed for this release on 2026-11-28 (see below). | User-provided (Analyze open question Q1, recorded in `evidence-log.md` E18) |
| 2026-11-27 | Q3: dependency policy "MCP binary only". At most one reviewed SDK tree, used only by the MCP binary. CLI keeps only `serde_json` and `sha2`. `Cargo.lock` committed. `--locked` and offline install keep working. MSRV pinned at 1.100. | User-provided (Q3, E18; MSRV 1.100 in the solution-direction gate) |
| 2026-11-27 | Q5: MCP `next` and `report` never run owner-configured prerequisite shell checks. The guard is a facade option that is off for the CLI. | User-provided (Q5, E18) |
| 2026-11-27 | Q6: MCP `report` accepts an approver label, under the same gate policy as the CLI. The label is self-asserted, and the owner accepted that. | User-provided (Q6, E18) |
| 2026-11-27 | Solution direction Option A (hand-rolled, zero new crates) chosen over Option B (SDK isolated). Dual-era protocol in v1, overriding the stage's legacy-only recommendation. | User-provided (solution-direction gate recommendation) |
| 2026-11-27 | Q18: an MCP approval without an explicit label is refused. No git or OS identity fallback over MCP. | User-provided (solution-direction gate recommendation) |
| 2026-11-27 | N2: install scripts stay CLI-only. | User-provided (solution-direction gate recommendation) |
| 2026-11-27 | N5: fix the two pre-existing baseline failures in this intent (WU-010). The solution-direction gate summarises the N5/Q14 "unchanged" definition as the same pass/fail set in the defined environment, with pre-existing failures out of scope. The two statements are not fully consistent; the WU-010 decision is the one that was applied. | User-provided (`develop/baseline.md` line 16); Observed (gate summary) |
| 2026-11-27 | Q14: "unchanged" means the same pass/fail set before and after, with `TMPDIR` under `/private/tmp` and `target/debug` first on `PATH`, and no golden re-blessed. | User-provided (solution-direction gate recommendation) |
| 2026-11-27 | AC-012 / Q10 (concurrency): accept the documented compare-then-rename window for v1, with no lock. | User-provided (solution-direction gate recommendation; superseded on 2026-11-28) |
| 2026-11-27 | Codebase-map review: the conductor's review is accepted in place of an owner review. | User-provided (solution-direction gate recommendation) |
| 2026-11-27 (after the solution-direction gate) | Support Windows and macOS. Verify through a GitHub Actions CI workflow on `windows-latest` and `macos-latest`. | User-provided (`scope-boundary.md`, amendment after the solution-direction gate) |
| 2026-11-28 | Reopen the concurrency decision: add an advisory file lock around the state check-and-write, so the loser gets the retryable `state_conflict`. Recorded in `decision-record.md` as a post-approval amendment. | User-provided (`solution-direction/gate-recommendation.md`, post-approval amendment 2) |
| 2026-11-28 | DeepSeek Harness is deferred: "không cần deepseek harness giờ đâu" (English: DeepSeek Harness is not needed now). Q1 is narrowed for this release to Claude Code. | User-provided (`scope-boundary.md`, amendment at the release-candidate gate) |
| Date not recorded | Owner-authorised push of the branch, so CI could run. | User-provided (`develop/build-and-verify/gate-recommendation.md`) |

Note: owner decisions appear only in the conductor record and the artifacts. `audit.jsonl` has no row for them (Observed, `analyze/context-and-risk/gate-recommendation.md` and `solution-direction/gate-recommendation.md`).

## 4. The three approved gates

All three gates were approved with the approver label "AI Orchestrator" (`gateMode: ai`). No gate was approved by the owner personally. The owner's decisions are the User-provided items in section 3.

| Gate | Stage | Attempts | Decision and time (UTC, from `state.json`) | Open items at approval |
|---|---|---|---|---|
| analysis-baseline | analyze / context-and-risk | 2 (attempt 1 blocked) | Approved 2026-11-27 14:57:43 | 0 written-approval, 8 follow-up |
| solution-direction | ideate / direction-selection | 1 recorded (no blocked report in `state.json`) | Approved 2026-11-27 16:22:52 | 0 written-approval, 16 follow-up |
| release-candidate | develop / build-and-verify | 2 (attempt 1 blocked) | Approved 2026-11-28 01:12:52 | 0 written-approval, 14 follow-up |

## 5. Blocked attempts and the owner's written answers

- **analysis-baseline, attempt 1, blocked 2026-11-27 14:44:50** (`state.json`, Observed). The gate needed the owner's written decisions on Q1, Q3, Q5 and Q6 before AI approval. The owner answered all four the same day (User-provided; `evidence-log.md` E18). Attempt 2 revised the Analyze artifacts and was approved at 14:57:43.
- **solution-direction, no blocked report recorded.** The gate research recommended pausing on one written question: AC-012 concurrency sufficiency (Q10). The owner answered "Accept window for v1" before approval (User-provided). The approval is recorded at 16:22:52.
- **WU-007 red-tests, blocked 2026-11-28 00:24:35** (`state.json`, Observed; park kind `design-defect`). The test `ts007_concurrent_reports_never_lose_an_update` exposed a lost update under concurrent state writers. The owner decided the same day to add an advisory file lock. WU-007 later reached `done` (commit `bab236e`, with design-decision commit `0487f49`).
- **release-candidate, attempt 1, blocked 2026-11-28 01:11:25** (`state.json`, Observed). The one [written-approval] item was AC-003's DeepSeek Harness half. Owner Q1 required both hosts, and `dsh` was not installed, so DeepSeek Harness launch-and-list was not run. The owner answered the same day in writing: "không cần deepseek harness giờ đâu" (User-provided). Q1 is narrowed for this release: Claude Code is the required host, and DeepSeek Harness verification is deferred. The sweep was re-run (545 passed, 0 failed) and attempt 2 was approved at 01:12:52.

## 6. Final evidence

Observed unless noted.

- **Local sweep:** `cargo test --no-fail-fast` across 33 targets gives 545 passed, 0 failed, against the intent baseline of 454 passed and 2 failed. Both baseline failures were fixed by WU-010. `cargo fmt --check` is clean. Clippy shows only the pre-existing warning at `tests/orchestration_golden.rs:1108`. `Cargo.lock` and `tests/golden` have no diff against `69cab5a`. Recorded in `develop/build-and-verify/regression.json` (`failures: []`).
- **CI run 46090623346:** conclusion success. `windows-latest`: 547 passed, 0 failed. `macos-latest`: 545 passed, 0 failed. `ubuntu-latest` (best effort): 545 passed, 0 failed. On each OS: installer tests 6 of 6 with 0 skips, and MCP transcript tests 29 of 29. Only warning: `actions/checkout@v4` on the deprecated Node 20 runtime.
- **Claude Code host check:** Claude Code 2.1.332, `claude mcp list` with an isolated `CLAUDE_CONFIG_DIR` and the real `dtech-aidlc-mcp` binary, reports "Connected". The project directory stayed empty and `~/.claude.json` was not modified. A model-driven `tools/call` was not run.
- **Acceptance criteria:** 12 of 14 pass (AC-011 to AC-014 with caveats). AC-003 and AC-010 are partial. None failed. Full statuses are in `completed-scope.md`.
- **Not verified:** DeepSeek Harness launch-and-list (deferred by the owner); a model-driven `tools/call` in a live Claude Code session; any Windows host launch (only the CI build and tests ran there).

## 7. What was skipped because of scope

- **Launch and Curate phases:** not in the solo scope. No Launch or Curate artifacts exist, and none are indexed.
- **Telemetry, monitor dashboard, plan phases 3-5:** intent 2, by owner decision (`intent.md`).
- **Release work:** version bump from 1.2.2, CHANGELOG entry, removing the "proposed" README label, and install-script support (N2): intent 2 (`scope-boundary.md`).
- **DeepSeek Harness verification:** deferred by the owner on 2026-11-28. Tracked as a follow-up.
- **Deployment, publication and merging:** not done. The gate recommendation records "Nothing was deployed, published or merged. Only the owner-authorised push happened" (Observed). No pull request was opened in these artifacts.

## 8. Open items

- No [written-approval] item is open at any approved gate (`state.json` `openItems`).
- The follow-up items per gate are listed in `completed-scope.md`, section 4 (release-candidate).
- The release-candidate gate lists 14 follow-ups. The solution-direction and analysis-baseline gates list 16 and 8. The full list of the earlier two is in their `gate-recommendation.md` files.
