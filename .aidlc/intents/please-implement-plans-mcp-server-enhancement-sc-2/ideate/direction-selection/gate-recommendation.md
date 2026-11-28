# Gate recommendation: solution-direction (intent 2)

- Intent: please-implement-plans-mcp-server-enhancement-sc-2 (intent 2 of 2: telemetry, monitor dashboard, release 1.3.0 prep)
- Gate: solution-direction (Ideate), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-28
- Owner decisions folded in (User-provided, project owner, 2026-11-28): direction Option A; Analyze decisions Q1-Q5, Q10-Q13, Q15; DeepSeek Harness deferred. Recorded in decision-record.md ADR-019.

## Researched options

### 1. Approve now, as written

Approve the five direction-selection artifacts unchanged, with only [follow-up] items.

Trade-offs: For: fastest; every open question the direction depends on is answered by the owner or decided with evidence (ADR-020..ADR-039). Against: scope-boundary.md, recommended-direction.md and architecture-design.md credited the AI threat-model review (AC-030) to owner decision Q15. Q15 only covers the dashboard design review and the Studio mock (problem-statement.md:48). Approving would attach the gate to records that claim an owner decision the owner did not make.

### 2. Fix the provenance wording, then approve (selected)

Correct the Q15 overclaim in the stage's own outputs, then approve with only [follow-up] items.

Trade-offs: For: the approval attaches to accurate records; same pattern as the Analyze gate (option B). It needs no owner input, because it only removes an overclaim; direction, scope and every ADR stay the same. Against: one more short edit pass after drafting (hashes are recorded at report time, after the edit).

### 3. Block for owner written approval

List as [written-approval]: the ADR-025 amendment of intent-1 ADR-005 rule 5, the Met* readings of AC-004/AC-018/AC-020 under writer drops, AC-030's "when the owner checks the gate" wording, and the mock title.

Trade-offs: For: most conservative. Against: none changes the direction. ADR-025 follows from owner Q1 plus the accepted AC-005, changes no reply bytes or stdout, and has a fallback that keeps rule 5 literal. The Met* readings were disclosed in the Option A the owner chose (solution-options.md:109, :190, :204, :206). The AC-030 gate wording is a release-gate matter the owner controls with `config gate-mode`. The mock title is non-blocking by the owner's own framing.

## Selected direction and rationale

Option 2, executed. The conductor changed scope-boundary.md (row "AI dashboard design review ... AI threat-model review"), recommended-direction.md (residual-risk paragraph) and architecture-design.md (section 2 constraints) so that the AI dashboard design review (AC-022) is credited to owner decision Q15, and the AI threat-model review (AC-030) to the Analyze gate [follow-up] under the AI orchestrator approval contract. It also corrected the decision-record.md citation of `run_mcp_env` to tests/fixtures/mcp/common.rs:159.

Ideate exit criteria, checked:

1. Feasible options and real alternatives are documented: options A-D plus an unbundled combination in solution-options.md and trade-off-matrix.md, each with cost, risk and AC coverage (Observed).
2. Trade-offs, scope and the recommended direction are explicit: recommended-direction.md s1-s2 says why B, C and D lost; scope-boundary.md lists in, out and deferred items with triggers and owner-owned items (Observed).
3. The direction is chosen by the owner: Option A (User-provided, ADR-019). The decision record keeps the AI recommendations (ADR-020..ADR-039) separate from the owner decisions (status lines "Owner-decided" vs "Recommended").
4. Architecture-design follows the arc42 template (s1-s12) and threat-model follows the template (assets, trust boundaries, entry points, STRIDE threats T2-01..), with residuals routed to the AC-030 review (Observed).
5. Open questions the direction depends on are settled: Q24 by the owner's "tests pass --telemetry off" (ADR-022 changes three spawn sites and leaves common.rs:244 alone); Q6-Q9, Q14, Q17-Q20, Q22 decided with evidence; Q16, Q21, Q25 are owner items the direction does not depend on, carried as follow-ups with defaults.
6. No Develop identifiers (FN-, IF-, TS-, WU-, CFG-) appear in the stage artifacts (Observed: grep, none).

## Evidence trace

- Observed: contracts.md AI orchestrator approval contract; rust-cli/gate.rs:64-70, :146-148 (Open items must exist, every item tagged).
- Observed: problem-statement.md:48 and open-questions.md Q15 (scope of Q15); analyze/context-and-risk/gate-recommendation.md Open items (source of the AC-030 AI threat review).
- Observed: acceptance-criteria.md:34 (AC-002 fixes the exact field list, so writer drops are disclosed, not counted, ADR-021).
- Observed: rust-cli/orchestrate.rs:3245 (approval only accepted on gate stages, so ADR-024's gate_decided event cannot misreport a gate).
- Observed: rust-cli/dependencies.rs:338-361 and rust-cli/knowledge.rs:118-160 (raw Win32 FFI precedent with no crate, supporting ADR-020 BCryptGenRandom).
- Observed (rustc 1.100.0, macOS arm64): `std::random` is unstable (E0658, issue #130703); 100,000 tokens unique; ct_eq shows no early exit at 5M compares; HTTP spike with Host check, traversal refusal and timeouts; writer stall does not block producers.
- Observed: Cargo.toml dependencies are serde_json and sha2 only; no new crate is needed by any ADR.
- User-provided: owner decisions listed in the header (ADR-019).
- Not run at this gate: cargo test, CI, a browser check, any Windows build.

## Residual risks

- Windows is unverified: the BCryptGenRandom block has never been compiled; delete-pending pruning, in-use upgrade preflight, read-timeout error kind and the firewall prompt are Assumptions until windows-latest CI and a manual check.
- Spike evidence is macOS-only and lives in an ephemeral scratchpad until implementation-design turns it into repo tests.
- The hand-rolled HTTP parser, Host/Origin/Sec-Fetch-Site checks and token compare are new attack surface without crate support; black_box constant-time behaviour is a compiler hint, not a guarantee.
- The fragment token may stay in browser history before replaceState and in terminal scrollback (T2-03); untested in a browser.
- AC-004, AC-018 and AC-020 hold only while the writer queue does not fill; drops are disclosed, not counted.
- Numeric limits (queue 1024, flush 500 ms, 5 MiB / 7 days / 50 MiB retention, 16 connections, 5 s timeouts, n>=20) are Estimates.
- The README "(proposed)" label stays until the Q21 live Claude Code observation happens.
- Effort estimate about 9-12 dev-days against 8-12 work units; the test baseline stays unstable until the flake fixes land.

## Follow-ups

See Open items. Every technical decision ADR-020..ADR-039 is an AI-approved recommendation for implementation-design, refinable within each ADR's reopen triggers.

## Boundary

This approval accepts Option A as the intent-2 solution direction and the five direction-selection artifacts (as corrected) as the basis for Develop. ADR-020..ADR-039 are approved by the AI Orchestrator as recommendations, not owner approvals; the owner decisions remain User-provided and are not re-approved or altered. It does not authorize: source, test, golden, README, CHANGELOG, Cargo, installer, plans/ (including the Studio mock title), PROJECT.md or codebase-map edits outside Develop work units; any golden re-bless beyond the owner-scoped reasons; any new crate; any AC amendment; a tag, publish, push, release or CI change; removing "(proposed)" without green Windows and macOS CI and a live Claude Code tools/call; DeepSeek verification; or accepting any threat-model residual (the AC-030 review does that before release). It authorizes nothing at later gates.

## Open items

- [follow-up] Owner, non-blocking: Studio mock title 'Orca MCP Studio' (plans/mcp-server-enhancement/mcp-studio-server.html); proposed default 'dtech-aidlc MCP Studio' (ADR-038); the Studio unit keeps the current title until the owner confirms.
- [follow-up] Owner, Q21: live Claude Code tools/call observation after Windows and macOS CI are green; without it the README '(proposed)' heading and its test needle stay and the release notes say so (ADR-034, AC-025).
- [follow-up] Owner, Q16 and Q25: refresh PROJECT.md (version, CI, MSRV, test baseline) and .aidlc/knowledge/_codebase-map.md after the release unit.
- [follow-up] Owner notice: ADR-025 amends intent-1 ADR-005 rule 5 (one mcp.protocol.error write after a handshake/list error reply, reply bytes unchanged); fallback if the owner objects: suppress on_error until the first tools/call.
- [follow-up] Implementation-design: mint FN-/IF-/TS-/WU-/CFG- ids for ADR-020..ADR-039 (storage and retention, writer queue and flush, drop disclosure, traced_call, --telemetry parsing, v1 event set, gate_decided, on_error codes, HTTP limits and ADR-032 status table, cursor, read caps, embedded assets, installer marker and preflight).
- [follow-up] Implementation-design: turn the scratchpad spike checks into repo tests (raw-TcpStream contract tests per ADR-032 code, writer-stall test, token compare tests, rotation and prune test, deps-add concurrent test).
- [follow-up] Windows verification on windows-latest CI (AC-029): BCryptGenRandom link and run (raw-dylib fallback), TimedOut vs WouldBlock, half-close drain, delete-pending pruning, in-use upgrade preflight, installed MCP exe spawn; plus a manual firewall-prompt and Ctrl+C batch-prompt check.
- [follow-up] AI dashboard design review (AC-022) before dashboard build: rule on ADR-039 readings (no 'off' state, GET-only, Reload via 'Token missing'), the drops coverage line, gate decisions without approve/reject counts, chart click-to-filter removal, 320 px vs 360 px width target.
- [follow-up] AI threat-model review (AC-030) before release against the built code: T2-03, T2-05, T2-06, T2-15, T2-22; any unresolved high finding blocks release; the owner may pin the release gate to human with config gate-mode.
- [follow-up] First flake unit (Q23): confirm root causes with repeated local loops before fixing; keep the extra rust-cli/scope.rs temp() counter fix only with loop evidence.
- [follow-up] Measure the Estimates (queue headroom, parse cache, connection cap, timeouts, retention overshoot) in tests or early use and act on the ADR reopen triggers.
- [follow-up] Any AC amendment (drop counter in AC-002, a gate decision field) is owned by Analyze and the owner; none is proposed for v1.
