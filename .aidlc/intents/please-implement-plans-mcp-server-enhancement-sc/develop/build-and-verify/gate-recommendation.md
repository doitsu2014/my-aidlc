# Gate recommendation: release-candidate (attempt 2)

- Intent: please-implement-plans-mcp-server-enhancement-sc (intent 1 of 2: application facade + MCP stdio server)
- Gate: release-candidate (Develop final sweep), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-28
- History: attempt 1 was blocked on 2026-11-28 with one [written-approval] item, AC-003's DeepSeek Harness half (owner Q1 required both hosts). The owner answered in writing the same day: "không cần deepseek harness giờ đâu" (DeepSeek Harness is not needed now). That narrows Q1 for this release to Claude Code and defers DeepSeek Harness verification (recorded in scope-boundary.md). The sweep was re-selected and re-run: 545 passed, 0 failed.

## Researched options

### A. Block the gate: AC-003 DeepSeek Harness half is a [written-approval] item (recommended)

Write gate-recommendation.md in develop/build-and-verify/ with one [written-approval] item: AC-003 / TS-013, where DeepSeek Harness launch-and-list was not run and the Claude Code half is only partly evidenced. Report the final sweep `--result blocked` with that reason. Do not report `--approval approved`. Present the owner with two ways to clear it in writing: (i) run the DeepSeek Harness launch-and-list check (and, ideally, a Claude Code tools/list or tools/call in a live session) on a machine with `dsh`, then record the evidence in the sweep artifacts and present the gate again; or (ii) state in writing that the release candidate is accepted with the DeepSeek Harness half of AC-003 unverified, which keeps the TS-013 check as a tracked follow-up before intent close or release. Everything else is filed as [follow-up].

Trade-offs: For: this is the only option consistent with the owner's Q1 decision, AC-003 as written ("Both hosts must launch the server and list the four tools"), scope-boundary section 1 ("Manual host checks: Claude Code and DeepSeek Harness both launch..." is in scope) and F8 ("These are AC-003 checks before the intent closes"). It also matches the contract rule that a criterion not checked does not pass. The CLI refuses an AI approval while any [written-approval] item is open, so blocking is also the only route that works mechanically. Against: the gate waits on the owner. Route (i) needs a machine with `dsh` installed (Observed: not installed here), and Launch preparation cannot start until the owner acts.

### B. Approve now with the DeepSeek Harness half as [follow-up]

Treat the strong child-process evidence (handshake and discovery on three operating systems, 29 of 29 MCP transcript tests) plus the Claude Code `claude mcp list` Connected result as enough. Tag the DeepSeek Harness launch-and-list as [follow-up] and approve as AI Orchestrator.

Trade-offs: Rejected. A [follow-up] is defined as tracked work that does not affect whether the candidate meets the approved scope. AC-003 does depend on it, because the owner explicitly made both hosts a requirement (Q1). Calling it a follow-up would let the AI Orchestrator quietly drop part of an owner decision, which breaks the change-and-scope contract (no silent narrowing without the owner's decision). It would also overstate the evidence: 'Connected' shows launch and handshake, but does not show that the host listed the four tools (Assumption).

### C. Owner amends scope: DeepSeek Harness verification moves to intent 2, or AC-003 is narrowed

The owner changes AC-003 or the scope boundary (for example, DeepSeek Harness launch-and-list becomes an intent-2 release check). The conductor records the amendment the same way the 2026-11-27 amendments were recorded, revises the sweep artifacts, and presents the gate again with no [written-approval] item.

Trade-offs: For: the approved scope then matches the evidence exactly, and the gate can be AI-approved afterwards. Against: it is still a written owner decision, so it is a variant of A(ii), not a way around A. It changes an Analyze acceptance criterion after the fact, which needs a recorded amendment, and it leaves the README's DeepSeek Harness configuration untested until intent 2.

### D. Pin the release-candidate gate to human approval

The owner runs `dtech-aidlc config gate-mode release-candidate human` and decides the gate personally.

Trade-offs: For: puts a decision-heavy gate with the person who made Q1. Against: the pin applies to every intent, not just this one, and only the owner can choose it. Either way, the AC-003 gap still has to be disclosed and decided. This changes who decides, not what has to be decided.

## Selected direction and rationale

Approve the release candidate. Attempt 1's recommended disposition (option A, block on AC-003) is resolved by the owner's written decision, which is option C (scope narrowed: DeepSeek Harness verification moves to a later intent). With that, every acceptance criterion in the approved scope has evidence: the local sweep (545/0, no golden or Cargo.lock change), CI green on windows-latest (547/0) and macos-latest (545/0) with no installer skips and 29/29 MCP transcript tests on each, and Claude Code connecting to the real binary. The partial rows in green-verification-report.md (AC-003 live-session call, AC-010, AC-011, AC-012 scenario gaps) rest on the shared facade and lock code paths and are tracked as follow-ups; none is a regression outside the baseline.

Attempt-1 rationale (for the record):

Develop exit criteria (core/methodology/phases/develop.md): (1) the implementation matches the approved scope and contracts; (2) verification evidence, known limitations and deployment/rollback notes are recorded; (3) the project owner approves the release candidate before launch activities; (4) the existing suite was run and compared with baseline.

Criterion 4 is met (Observed). The sweep gave 545 passed / 0 failed against the intent baseline of 454 / 2. Both baseline failures were fixed by WU-010 under an owner decision. regression.json has `unit: sweep`, `scope: full`, `failures: []`. Cargo.lock and tests/golden show no diff. CI run 46090623346 is green on windows-latest (547/0) and macos-latest (545/0) with installer 6/6 and 0 skips and MCP 29/29, which satisfies the 2026-11-27 Windows+macOS amendment (TS-012).

Criterion 2 is met (Observed). implementation.md has the deployment/install notes and a rollback note: revert d765edd..ca30a85 or revert each unit in reverse order, and `cargo uninstall dtech-aidlc` removes both binaries. known-limitations.md lists every gap with Observed/Assumption/Open-question labels, and the green report has a 14-row trace table.

Criterion 1 is NOT fully met. AC-003 (US-003) states "for each supported host: Claude Code and DeepSeek Harness. Both hosts must launch the server and list the four tools", sourced to the owner's Q1 decision of 2026-11-27. Scope-boundary section 1 lists both manual host checks as in scope, and F8 says "These are AC-003 checks before the intent closes." Observed: DeepSeek Harness was never launched (`dsh` not installed). Observed: Claude Code 2.1.332 `claude mcp list` reported Connected with an isolated config, and the project dir and ~/.claude.json were untouched. Assumption: Connected shows launch and handshake, not that the host listed status/queue/next/report. The four-tool list is proven only by the child-process tests (ts018_exactly_four_tools_with_contract_annotations, TS-001, TS-002, TS-005).

The contracts are explicit: "A check that was not run is not a passing check." An owner-required criterion with no evidence for one host therefore cannot become a follow-up. The owner must verify it, or accept in writing that the candidate goes forward with it unverified (or amend the scope, which is the same written decision). The AI orchestrator contract makes any [written-approval] item a blocked gate, and the CLI refuses `--approval approved` while one exists. So the correct action is a blocked report with the gate-recommendation written.

Every other gap is tracked work that does not change whether the candidate meets the approved scope. Those gaps are: AC-010 AI-mode MCP test gap, AC-011 unsafe-layout test, AC-012 mixed CLI/MCP writer test, AC-013 install-smoke breadth, the AC-003 restart scenario's lack of a dedicated test, Windows host launch, model-driven tools/call, the performance bound not measured, flakes, Node 20 checkout, and the README sentences. In each case either the behaviour is implemented on the shared facade path (AC-002, passed) with the missing piece being test depth, or the item is documentation or CI hygiene. The restart/shutdown scenario could be folded into the written-approval item. I keep it as a follow-up because the server opens the project per call and holds no state (ADR-006), and ts005_empty_stdin_exits_zero_and_writes_nothing and the ts006 read-only byte checks bound it. The owner may choose to require it.

README platform sentence (README.md:191, "macOS and Linux builds have not been verified yet"): this is a [follow-up] for the owner. WU-009's design hands the update to the owner once CI is green on both OSes, and that condition is now true. The sentence understates rather than overclaims, so it is not a release blocker. No repo_content guard pins it (Observed: grep). README.md:421 ("A launch-and-list check in each host is a manual release check, not yet run") is still accurate for DeepSeek Harness. Update it only together with the outcome of the written-approval item, and keep the DeepSeek Harness 'not verified' wording unless route A(i) produces evidence. Re-run `cargo test --test repo_content` after either edit.

## Evidence trace

- Observed: core/methodology/contracts.md:52 says approval of a release candidate does not automatically authorize deployment. contracts.md:54-66 is the AI orchestrator contract: gate-recommendation.md needs options, an evidence trace, rationale, a boundary statement and `## Open items` with [written-approval]/[follow-up] tags, and any [written-approval] item means the gate is blocked and the CLI refuses approval. contracts.md:72 says a check that was not run is not a passing check.
- Observed: core/methodology/phases/develop.md:23-30 gives the Develop exit criteria. develop.md:65 says only a sweep without failures outside the baseline can pass the release-candidate gate.
- Observed: core/methodology/agents/builder-verifier.md:55-59 says the final sweep closes Develop. In AI mode it writes gate-recommendation.md and approves only as the contract allows.
- Observed: develop/build-and-verify/regression.json is {schemaVersion:1, unit:sweep, scope:full, failures:[]}. regression-report.md records 545/0 against baseline 454/2 (develop/baseline.md), and both baseline failures were fixed by WU-010 (TS-019).
- Observed (conductor): sweep-suite.log at /private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/sweep-suite.log. fmt is clean. The only clippy warning is the pre-existing one at tests/orchestration_golden.rs:1108. `git diff 69cab5a..HEAD -- Cargo.lock tests/golden` is empty.
- Observed (conductor): CI run 46090623346 succeeded. windows-latest 547/0, macos-latest 545/0, ubuntu-latest 545/0. Installer 6/6 with 0 skips and MCP transcript 29/29 on each OS. Log at /private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/ci-46090623346.log. This meets TS-012 and the scope-boundary amendment of 2026-11-27.
- Observed: analyze/problem-framing/acceptance-criteria.md:39-58. AC-003 requires both Claude Code and DeepSeek Harness to launch and list the four tools (owner Q1). It also has a scenario 'restart and shutdown preserve state'.
- Observed: analyze/context-and-risk/open-questions.md, row Q1, class Decided (User-provided, project owner, 2026-11-27): 'both must pass launch-and-discover'.
- Observed: ideate/direction-selection/scope-boundary.md section 1 puts manual host checks for both hosts in scope. Section 6 F8 says 'These are AC-003 checks before the intent closes.' The amendment adds Windows+macOS via CI.
- Observed: develop/implementation-design/ids.json registers TS-013, 'Manual host launch checks (Claude Code, DeepSeek Harness) on macOS and Windows', which traces AC-003, AC-014, FN-016 and FN-021.
- Observed (conductor): Claude Code 2.1.332 `claude mcp list` with an isolated CLAUDE_CONFIG_DIR against target/debug/dtech-aidlc-mcp reported Connected. The project dir stayed empty and ~/.claude.json was not modified. Not run: a model-driven tools/call, and DeepSeek Harness (dsh not installed).
- Assumption: 'Connected' from `claude mcp list` shows launch and handshake but not that the host listed status/queue/next/report. The four tools are evidenced only by the child-process tests ts018_exactly_four_tools_with_contract_annotations, TS-001, TS-002 and TS-005.
- Observed: green-verification-report.md trace table. 12 of 14 criteria passed (AC-011..AC-014 with caveats). AC-003 and AC-010 are partial and none failed. known-limitations.md has open items 1-8.
- Observed: a grep for 'restart' in tests/mcp_*.rs and in develop/implementation-design/test-strategy.md finds no dedicated test for the AC-003 restart scenario. Assumption: it is bounded by the per-call project open (ADR-006), ts005_empty_stdin_exits_zero_and_writes_nothing and the ts006 read-only byte checks.
- Observed: develop/build-and-verify/implementation.md has the deployment/install notes and the rollback note (revert d765edd..ca30a85 or each unit in reverse order, then `cargo uninstall dtech-aidlc`). Nothing was deployed, published or merged. Only the owner-authorised push happened.
- Observed: README.md:191 still says 'macOS and Linux builds have not been verified yet'. README.md:421 says the launch-and-list check in each host is 'not yet run'. tests/repo_content.rs has no guard on either sentence (grep). WU-009/known-limitations.md says the owner updates the platform sentence once CI is green on Windows and macOS.
- Observed: Cargo.toml version 1.2.2 is unchanged and there is no CHANGELOG entry. Under ADR-018 and the scope boundary, release work belongs to intent 2.
- Observed: develop/build-and-verify/gate-recommendation.md does not exist yet. The conductor must write it before reporting, because I did not write files.
- Open question: why Windows CI runs 547 tests against 545 on macOS (assumed to be platform-gated tests; not investigated).
- User-provided (project owner, 2026-11-28): "không cần deepseek harness giờ đâu"; recorded in ideate/direction-selection/scope-boundary.md.
- Observed (re-run after the re-selection, 2026-11-28): `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 33 targets, 545 passed, 0 failed; sweep regression.json rewritten with failures [].

## Residual risks

- DeepSeek Harness may fail to launch or list the server in practice. Its scrubbed environment may drop DTECH_AIDLC_HOME (Assumption), and the `insert` entry shape comes only from the dsh-mcp-client documentation. The README's DeepSeek Harness configuration is untested.
- Claude Code's reaction to a modern-era response in a live, model-driven session is not observed. 'Connected' may come from a handshake that does not exercise tools/list (Assumption).
- No host launch on Windows was observed. Only the CI build and tests ran there.
- Several AC scenarios (AC-003 restart, AC-010 AI-mode, AC-011 unsafe layout, AC-012 mixed writers) depend on the Assumption that the shared code path covers them. There is no MCP-level test for any of them.
- ts007_concurrent_reports_never_lose_an_update is timing-based. A runner could report zero conflicts and fail spuriously (it passed on all CI runners).
- The approver label is self-asserted, not authenticated. MCP approvals cannot be told apart from CLI approvals in audit.jsonl (F7).

## Boundary statement

Scope of this research: it informs the release-candidate gate decision only. Recommended outcome: the gate is blocked pending the owner's written decision on AC-003, so there is no AI approval to give yet.

Even after the gate is later approved, by the AI Orchestrator once the written-approval item is resolved, or by the owner, the approval does NOT authorize any of the following: deployment or publishing of any kind; opening, merging or approving a pull request; merging into main; creating a release, tag or GitHub release; a version bump (Cargo.toml stays 1.2.2), a CHANGELOG entry or removing the 'proposed' README label (all intent 2); any further push beyond the one the owner already authorised; changes to the prebuilt install scripts; writing or changing any host configuration (Claude Code, DeepSeek Harness, ~/.claude.json, user-level settings) on any machine; running dtech-aidlc or reporting the gate on the owner's behalf; or starting Launch activities. Those each need the owner's separate, explicit approval of the concrete action, target and timing. Launch also needs its own gate.

This research made no file writes, no source or test edits, no state-changing git commands and no network calls. The conductor must write develop/build-and-verify/gate-recommendation.md, with the options, evidence trace, rationale, this boundary statement and an `## Open items` section listing the [written-approval] item and the [follow-up] items above, before reporting the sweep as blocked.

## Open items

- [follow-up] AC-003 / TS-013 DeepSeek Harness half: launch-and-list of dtech-aidlc-mcp in DeepSeek Harness is deferred by the owner ("không cần deepseek harness giờ đâu", User-provided, 2026-11-28; scope-boundary.md "Amendment at the release-candidate gate"). Claude Code is the required host for this release (claude mcp list: Connected with the real binary). The README keeps the DeepSeek Harness entry labelled proposed and not verified; verify it in a later intent.
- [follow-up] README.md:191 platform sentence ('macOS and Linux builds have not been verified yet'). The owner updates it now that CI run 46090623346 is green on windows-latest and macos-latest (WU-009 design). Linux stays best effort. It understates rather than overclaims, so it does not block. Re-run `cargo test --test repo_content` afterwards.
- [follow-up] README.md:421 ('A launch-and-list check in each host is a manual release check, not yet run'). The owner updates it to match the outcome of the AC-003 written-approval item. Keep the DeepSeek Harness 'not verified' wording unless that check is actually run.
- [follow-up] TS-013 model-driven tools/list and tools/call in a live Claude Code session (both eras where possible), plus a Windows host launch of dtech-aidlc-mcp.exe. The CI build and tests on Windows are the evidence agreed in the 2026-11-27 amendment; a Windows host launch is extra.
- [follow-up] AC-003 restart and shutdown scenario: add a test that state.json and audit.jsonl are byte-identical across a server shutdown and restart (currently bounded only by ts005_empty_stdin_exits_zero_and_writes_nothing, the ts006 read-only checks and the per-call open).
- [follow-up] AC-010: add MCP-level tests for an AI-mode gate approved with 'AI Orchestrator' and a valid recommendation, an AI-mode gate refused without one, a human-mode gate refusing 'AI Orchestrator', and the policy edited between calls. For now these rely on the shared facade (AC-002, passed).
- [follow-up] AC-011: an MCP-level test for an unsafe state dir or symlink layout (refused, nothing written). It is neither planned in test-strategy.md nor present.
- [follow-up] AC-012: a test that mixes a CLI writer and an MCP writer under with_state_lock (FN-022). The current races are MCP against MCP and in-process.
- [follow-up] AC-013 and AC-014: extend ts009_cargo_install_and_uninstall_both_binaries (TS-009) to check MCP --help and --version and to assert that the host settings files are byte-unchanged.
- [follow-up] Performance: the CLI or MCP latency bound was never measured (N6). Record a measurement or accept the estimate.
- [follow-up] Pre-existing timing flakes (not from this intent): prerequisites::tests::check_timeout_is_reported, artifacts::tests::verifies_empty_missing_and_not_applicable_reason, onboarding::projection::tests::keeps_user_edits_unless_force_then_backups. Give them timing margin.
- [follow-up] CI hygiene (CFG-003, WU-008): upgrade actions/checkout off the deprecated Node 20 runtime and run actionlint. The pre-existing clippy warning at tests/orchestration_golden.rs:1108 is a separate item.
- [follow-up] Redaction value-space limitation (FN-015), the state lock reopen ceiling (FN-022, marked with a ponytail comment), the `deps add` path without the lock (pre-existing, not reachable over MCP), and the wording gaps listed in known-limitations.md.
- [follow-up] Release work stays in intent 2 per ADR-018 and the scope boundary: CHANGELOG entry, version bump from 1.2.2, removing the 'proposed' label, install-script support (N2).
