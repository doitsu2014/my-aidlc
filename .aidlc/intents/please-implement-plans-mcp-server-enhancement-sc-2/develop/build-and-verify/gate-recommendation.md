# Gate recommendation: release-candidate (intent 2)

- Intent: please-implement-plans-mcp-server-enhancement-sc-2 (telemetry, monitor dashboard, 1.3.0 release prep)
- Gate: release-candidate (Develop), gateMode ai, decided by the AI Orchestrator conductor
- Status: APPROVED by AI Orchestrator, 2026-11-28, after the owner's written decisions below
- Candidate: branch feature/please-implement-plans-mcp-server-enhancement-sc, HEAD 483e65f plus the forced-colors CSS fix commit; version 1.3.0 prepared, not tagged.

## Researched options

### A. Approve now with follow-ups only
Trade-offs: For: all automated evidence green. Against: at research time four items lacked owner confirmation or evidence (an AI re-bless outside ADR-033's list, the ADR-025 relaxation of an intent-1 rule with no owner reply, no CI on HEAD, unexercised AC-013/017/019/021 scenarios). Approving over them would attach an AI approval to owner-protected rules.

### B. Block, get the owner's written decisions, close the evidence gaps (selected)
Trade-offs: For: every gap closed with owner words or Observed evidence. Against: one more round (done).

### C. Block and rework so no owner confirmation is needed
Trade-offs: For: no owner input. Against: reverting the monitor completion line or the handshake-error telemetry would weaken accepted behaviour for no safety gain.

### D. Pin the gate to human approval
Trade-offs: For: the owner decides everything. Against: not requested; the AI contract allows the decision once [written-approval] items are resolved.

## Selected direction and rationale

Option B, executed. The research workflow listed four [written-approval] items; all are now resolved:
1. Golden scope (AC-027): the owner confirmed in writing (AskUserQuestion, 2026-11-28) the one-line re-bless of `tests/golden/routing_golden/unsafe.json` (monitor completion entry) as covered by Q2/Q4; ADR-033's list is read as eight files.
2. ADR-025 vs intent-1 ADR-005 rule 5: the owner accepted in writing (2026-11-28) one `mcp.protocol.error` write after a handshake or list error reply, with telemetry on and `.aidlc` present.
3. CI on the release candidate (AC-029): the owner authorised the push of 483e65f ("push lên đi", 2026-11-28); CI run 46118325770 succeeded on windows-latest (693/0), macos-latest (702/0) and ubuntu-latest (702/0).
4. Unexercised scenarios: the owner chose the headless pass (2026-11-28). `rc-manual-pass.md` records AC-013 (SIGINT and SIGTERM stop, port freed, no stdout, no writes), AC-017 (empty, corrupt and partial counters, stale within one poll, token missing), AC-019 (filter, trace and session drill-down, Clear, Load older and Back to newest) and AC-021 (Tab order at 1280 and 320 px, visible focus in light, dark and forced colors, detail focus in and out, focus kept across auto-refresh, live region) as passed. Its one AC-021 defect (selected row not distinguishable in forced colors) was fixed with a test needle that was red first.

Develop exit criteria: all 12 units verified with per-unit artifacts; final sweep on the integrated tree 657 passed, 0 failed, 1 ignored (baseline 545/0); regression.json failures []; no new crate (Cargo.lock: version line only); golden changes limited to the owner-scoped reasons; the AI dashboard design review (AC-022) and the AI threat-model review (AC-030: 0 high, 0 medium, 23 low, required fixes done) are recorded; install, upgrade and rollback notes in implementation.md.

## Evidence trace

- Observed: sweep logs i2-sweep-suite.log (483e65f) and i2-sweep2-suite.log (after the CSS fix): 657/0/1 ignored, 39 result lines.
- Observed: CI runs 46117480020 (6aec542) and 46118325770 (483e65f), green on all three OSes; Windows ran the PowerShell installer tests, the in-use test and BCryptGenRandom for real.
- Observed: live Claude Code tools/call (WU-022 implementation.md).
- Observed: `WU-016/design-review.md` (accepted), `WU-021/threat-model-review.md` (with Resolution), `rc-manual-pass.md` (with Resolution), `WU-019/browser-shots/`.
- User-provided: owner decisions 1-4 above, Q1-Q5, Q10-Q13, Q15, direction A, Studio title.
- Observed: `git diff 327b02a..HEAD -- Cargo.lock` = the dtech-aidlc version line; 8 golden files changed, each for a scoped reason.

## Residual risks

- Same-OS-user exposure (T2-06) and the token in browser history and scrollback (T2-03) are accepted by design and disclosed in the README.
- The hand-rolled HTTP server and token compare are new attack surface without crate support (rated low).
- The forced-colors CSS fix has only the local sweep, not CI (one CSS rule plus one test needle).
- Real screen-reader output, real Windows High Contrast, the Windows desktop firewall and `.cmd` prompts, and DeepSeek Harness are not verified.
- Drops are disclosed, not counted; the retention cap is soft by about one part.

## Follow-ups

See Open items.

## Boundary

This approval accepts the 1.3.0 release candidate on the feature branch as reviewed. It does not authorise a deployment, merge, PR, tag, crates.io or GitHub release, any further push, host configuration changes, further golden re-blesses, new crates or AC amendments. Launch activities need their own owner approval of the concrete action.

## Open items

- [follow-up] DeepSeek Harness host check (owner deferral); AI-host launch of the Windows .exe, Windows firewall prompt and .cmd Ctrl+C prompt not run; README states them as unverified.
- [follow-up] Threat-review low findings still open: T2-20 client-side event shape check, T2-22b and T2-22d install.ps1 rollback-in-finally and path handling, T2-18 store .gitignore content, query whole-file re-read bound, impossible timestamps not counted as skipped (NEW-5), writer closed-set and id charset check, installer ownership matching and housekeeping.
- [follow-up] Accepted residuals the owner may overturn: T2-06 same-user boundary, T2-03 token in history and scrollback, T2-05 other OS users and Windows ACLs, T2-09 slot exhaustion, T2-15 writer drops, predictable install-prefix temp names, upgrade replacing lib/dtech-aidlc wholesale.
- [follow-up] Contract text drift: amend IF-018 and ADR-029 for the OWS stripping around Authorization and the wider bounded drain, update T2-03 to Observed, update ADR-033's golden list to eight files.
- [follow-up] Push the forced-colors CSS fix commit and watch CI when the owner next authorises a push.
- [follow-up] Manual checks: real screen reader on the dashboard, real Windows High Contrast, invalid-filter announcement.
- [follow-up] Q16/Q25: refresh PROJECT.md (version, CI, MSRV, test baseline) and the codebase map (monitor, telemetry, new test binaries).
- [follow-up] Process notes for the intent record: WU-021 edited earlier units' files for the AC-030 fixes; WU-022 edited the CHANGELOG; the live check exercised only status.
- [follow-up] Product limits to track: dashboard cramped at 960-1040 px with detail open; query cap can be exceeded by one rotated file; Estimates (queue 1024, flush 500 ms, retention, 16 slots, timeouts) to be measured in early use.
- [follow-up] Out of scope (Q12): pre-existing clippy -D warnings failure, actions/checkout Node 20 deprecation.
- [follow-up] Launch: a tag, publish, release, merge or PR for 1.3.0 each needs a separate owner approval.
