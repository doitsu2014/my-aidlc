# Recommended direction: Option A (intent 2 of 2)

- Purpose: state the solution direction for intent 2 (local MCP telemetry, the read-only loopback monitor and dashboard, and release hardening), say why the other options lost, and name what still belongs to the owner, so implementation-design can proceed without re-running Ideate.
- Status: Draft - pending solution-direction gate. The direction (Option A) and the Analyze decisions it carries were chosen by the project owner (User-provided, 2026-11-28). The technical details inside it are recommendations and are not approved until the gate passes. In AI orchestration mode the gate may be approved by "AI Orchestrator"; that is not an owner approval of the details.
- Stage: Ideate / direction-selection (decision-facilitator). Companions: [decision-record.md](decision-record.md) (ADR-019..ADR-039), [scope-boundary.md](scope-boundary.md), plus architecture-design.md and threat-model.md in this folder.
- Sources: [solution-options.md](../option-generation/solution-options.md), [trade-off-matrix.md](../option-generation/trade-off-matrix.md); Analyze baseline `../../analyze/problem-framing/*` and `../../analyze/context-and-risk/*`; intent-1 decision record (ADR-001..ADR-018) and knowledge record `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md`; `plans/mcp-server-enhancement/*` (the owner's proposal, not an approved design); spikes under `/private/tmp/claude-501/-Users-doitsu2014-workspaces-doitsu-technology-my-aidlc/dbbd0152-2f7e-4566-a23e-0dd698cbab3c/scratchpad/spike2/` (macOS only).
- Accepted decision records checked: intent 1's ADR-001..ADR-018 (Observed). Numbering continues at ADR-019. Changes to intent-1 ADRs are named in section 3.
- Provenance labels: Observed, User-provided, Assumption, Estimate, Open question.

## 1. Direction

**Option A** (solution-options s2), chosen by the owner on 2026-11-28 (User-provided; [ADR-019](decision-record.md#adr-019-direction-option-a-and-the-owner-decisions-it-carries)).

Each element of the direction is decided once, in the decision record; values (queue size, retention, caps, codes) and statuses live there and are not restated here.

- Telemetry writer: [ADR-021](decision-record.md#adr-021-background-writer-thread-failure-isolation-and-dropped-events)
- Storage, rotation and retention: [ADR-020](decision-record.md#adr-020-telemetry-storage-file-format-rotation-and-retention)
- Hook in the MCP adapter; tests pass `--telemetry off`: [ADR-022](decision-record.md#adr-022-operation-hook-in-the-mcp-adapter-tests-pass---telemetry-off)
- Opt-out flag: [ADR-023](decision-record.md#adr-023-opt-out-flag---telemetry-onoff)
- Event schema v1 and `workflow.gate_decided`: [ADR-024](decision-record.md#adr-024-event-vocabulary-and-schema-v1-including-workflowgate_decided)
- Outcome mapping and protocol-error hook: [ADR-025](decision-record.md#adr-025-outcome-and-error-mapping-and-the-protocol-error-hook)
- Ids, trace and parent-span rules: [ADR-026](decision-record.md#adr-026-ids-sessionid-from-the-os-random-source)
- HTTP server and error table: [ADR-027](decision-record.md#adr-027-http-server-std-only-zero-crates), [ADR-032](decision-record.md#adr-032-one-http-status-and-error-code-table)
- Monitor process and edge cases: [ADR-028](decision-record.md#adr-028-monitor-process-model-lifecycle-and-edge-cases)
- Monitor token: [ADR-029](decision-record.md#adr-029-monitor-token-source-delivery-page-storage-comparison)
- Query and cursor API: [ADR-030](decision-record.md#adr-030-query-and-cursor-api-read-caps-and-refresh-values)
- Dashboard assets: [ADR-031](decision-record.md#adr-031-dashboard-assets-embedded-as-three-static-files)
- Release 1.3.0 and CHANGELOG: [ADR-033](decision-record.md#adr-033-version-130-scoped-re-bless-and-changelog)
- README and the "proposed" label: [ADR-034](decision-record.md#adr-034-readme-update-and-the-proposed-label)
- Installers: [ADR-035](decision-record.md#adr-035-installers-install-both-binaries)
- Hardening and Studio mock: [ADR-036](decision-record.md#adr-036-flaky-test-fixes-and-the-missing-intent-1-mcp-tests), [ADR-037](decision-record.md#adr-037-deps-add-takes-the-state-lock), [ADR-038](decision-record.md#adr-038-studio-html-mock-update-and-title)
- AC readings for the release gate: [ADR-039](decision-record.md#adr-039-acceptance-criteria-readings-applied-at-the-release-gate)

Effort (Estimate, from trade-off-matrix.md): about 5-7 dev-days specific to A plus about 4-5 dev-days of common work, so about 9-12 dev-days. Analyze sized the intent at 8-12 work units (ASM9, Estimate).

## 2. Why the alternatives lost

The owner chose A. The reasons below are the trade-off-matrix evidence behind that choice, not a separate decision.

- **B, plan-literal (synchronous sink in the facade, supervisor with hidden child, typed token).** A synchronous std write has no timeout, so a hung disk or AV scan delays the tool reply and AC-028 scenario 4 is Partial (R3). The facade sink puts per-process state into `Project`, which fires ADR-006's reopen trigger. The supervisor adds an undocumented child entry point, an orphan window on SIGKILL (R13) and cleanup tests on three OSes, the weakest Windows rating in the matrix. It costs the most (about 6.5-8 dev-days specific). Its one real advantage, keeping the token out of every URL, is narrowed in A by the fragment (never sent on the wire or in Referer) plus `replaceState`.
- **C, lean (no token, browser-side filtering).** Cheapest live monitor (about 4-5.5 dev-days specific), but with no token another OS user on a shared machine can read telemetry through the loopback port that 0o600 files deny them; the AC-030 review would have to accept single-user hosts. AC-019 "stale cursor" becomes a client-only message (Partial), and whole-range bundles hit the size cap sooner.
- **D, static snapshot.** Smallest surface and cost, but fails AC-013 (no foreground server or address) and AC-017 (no refresh, stale or shutdown states), leaves AC-014/AC-015 vacuous, and writes an at-rest telemetry copy outside `.aidlc/telemetry/`, the ignore line and retention. Shipping it needs Analyze-owned AC changes.
- **Unbundled synchronous-writer combination** (trade-off-matrix "Unbundled"): no drops and no thread, but the same AC-028 scenario 4 cost as B. Not chosen.

## 3. Changes to accepted decisions and plan text

| Changed | How | Decision |
|---|---|---|
| ADR-002 (intent 1) | Amended: installers install both binaries (its "stay CLI-only" sentence superseded; its own reopen trigger fired); MCP flags become `--project` and `--telemetry`; its Cargo.lock no-diff check (`LOCK_SHA256`) takes the 1.3.0 value. The `--help`/`--version` text was already replaced by the intent-1 Develop contract (consistency note). | ADR-023, ADR-033, ADR-035 |
| ADR-005 (intent 1) | Amended: `serve` gains an error observer, reply bytes unchanged; rule 5 (no disk write in the `initialize`, `server/discover` and list handlers) relaxed for the telemetry write after an error reply. | ADR-025 |
| ADR-006 (intent 1) | Amended: the consequence "the facade is the intent-2 hook" no longer holds; the hook is the MCP adapter. Reopen trigger not fired. | ADR-022 |
| ADR-017 (intent 1) | Amended: the `## MCP server (proposed)` heading and "every statement labelled proposed" end once the Q10 evidence exists; telemetry, monitor and troubleshooting sections join it. Its reopen trigger fired. | ADR-034 |
| ADR-009 (intent 1) | Unchanged: `status` and `queue` keep `readOnlyHint: true` although calls now write telemetry; reason recorded. | ADR-022 |
| ADR-001 (intent 1) | Extended to the HTTP server; unchanged. | ADR-027 |
| ADR-018 (intent 1) | Fulfilled by the 1.3.0 entry; not superseded. | ADR-033 |
| Plan:305, :409, :456, :500; design s7.7 bullet 1 | Superseded: in-process monitor, no supervisor or hidden child. | ADR-028 |
| Plan:278, :309; design:312 | Amended for the token only: fragment, stripped on load. | ADR-029 |
| Design s6.3 operation-event producer | Amended: MCP adapter, not the facade. | ADR-022 |
| Design s6.2 enumeration | `cli`, `unit_parked`, `unit_revisited` removed from v1. | ADR-024 |
| Plan:119 MCP `--help`/`--version` | Already superseded by intent 1 (`interfaces-and-contracts.md:151`). | ADR-019 |

## 4. Conditions the gate must accept

- AC-004, AC-018 and AC-020 are Met* under ADR-021: met while the writer queue does not fill, with drops disclosed and not counted. A counted drop field needs an Analyze-owned AC-002 change.
- The AC readings in ADR-039 (AC-017 "off" never shown, AC-014 GET-only, AC-013 "unavailable", AC-004 pre-host rejections, AC-026 in-process launch, CON18, AC-019 "Reload" through "Token missing", AC-024 "local optional capabilities" in the checklist).
- Windows behaviour is Assumption until windows-latest CI and one manual desktop check: BCryptGenRandom link and run, delete-pending pruning, in-use upgrade, the batch prompt, and the loopback firewall prompt.
- The residual risks in threat-model.md (T2-03, T2-05, T2-06, T2-15, T2-22, including the untested browser-history residual of the fragment token) are dispositioned by the AC-030 AI threat review before release, not at this gate. T2-10 is covered by the owner's Q13 decision.

## 5. What still belongs to the owner

Non-blocking follow-ups, none needed to start implementation-design (scope-boundary.md section 4): the Studio mock title (proposed default "dtech-aidlc MCP Studio", ADR-038); Q21 (who supplies the live Claude Code `tools/call` and when); Q16 (PROJECT.md refresh); Q25 (codebase-map refresh). Any change to an AC (a drop counter, a gate decision field) is Analyze-owned and needs the owner.

Residual risks are not owner items: no threat-model residual needs the owner's written approval at this gate. The AC-030 AI threat review (from the Analyze gate [follow-up] under the AI orchestrator approval contract; not an owner decision) accepts each residual or requires a change before release, and records it; T2-10 is already covered by the owner's Q13 decision. The owner may still raise any residual at the gate.
