# Known limitations: release candidate 1.3.0 (final sweep)

Labels: Observed, Assumption, Open question. All reviews named here are AI reviews, not owner approvals.

## Verification gaps

1. CI ran on 483e65f (run 46118325770, green on all three OSes). The forced-colors CSS fix committed after it (one CSS rule, one test needle) has only the local sweep (657 / 0); it is not pushed.
2. DeepSeek Harness not run against `dtech-aidlc-mcp` (owner deferral). The README states it as unverified. A host launch of the Windows `.exe` by an AI host was not run either (CI ran the binaries directly).
3. The live Claude Code check was run by the conductor with the owner's normal login and `--strict-mcp-config`, one tool (`status`) called; `queue`, `next` and `report` were not exercised from a live host. Claude Code keeps that session transcript in its usual place.
4. Manual checks not done: Windows desktop firewall prompt and `.cmd` Ctrl+C prompt; dashboard keyboard flow, focus survival across auto-refresh, screen-reader announcements, dark mode, forced-colors, widths between 360 and 960 px other than those captured, Windows browsers. Contrast ratios are computed, not measured.
5. Dashboard states not driven in a browser: empty, stale, quiet, paused, limit reached, stale cursor, filter and drill-down (AC-017, AC-019). They are covered by query tests and static asset checks.
6. SIGINT and SIGTERM are not driven against the monitor; stop is checked with `Child::kill`. ADR-032 row 14 (500 internal_error) is covered by the panic-guard unit test, not over a raw socket.
7. Windows: timing-sensitive pieces (TimedOut and WouldBlock, 408 timing, flaky-test `ping -n 6` change) were verified only by CI on 6aec542. The extended Windows in-use test asserts before killing its started process, so a failure there can leak a process on the runner.
8. `cargo clippy --all-targets -- -D warnings` is a known pre-existing failure (Q12), not run in the sweep.

## Product limitations that still apply

- Retention cap is soft by up to one open part (Observed 54.96 MiB against 50 MiB); queue headroom is a lower bound (no drop at 200k events/s).
- Drops are disclosed ("Dropped events: not recorded"), not counted.
- Query caps: the event cap is checked after a whole file is added, so a partial result can exceed 250,000 events by up to one rotated file; under a cap hit `incomplete` can over-count and the page flags `(partial)`. Parse cache is about the 64 MiB byte cap (Observed RSS +79 MiB at 62.9 MB).
- Stage filter matches workflow events only (tool and operation events carry no ids), so toolCalls, latency and incomplete are 0 under a stage filter; the page labels it "workflow events only".
- Latency population: completed `mcp.tool.completed` events with a non-null duration; n equals `toolCalls` except for null durations.
- With telemetry on and `.aidlc` present, an error reply in initialize, server/discover or a list handler leads to one `mcp.protocol.error` write (ADR-025 owner notice; no objection received). Reply bytes and stdout are unchanged.
- The `traced_call` panic path is covered by the protocol-layer host-panic case, not by an integration test in the real tools.
- Telemetry setting shown as "unknown" (no trusted input); MCP process liveness "unknown".
- T2-13 stays check-then-open per write (narrowed, same user only). The EACCES test self-skips when mode bits are not enforced (root).
- `with_state_lock` is a no-op when `.aidlc` does not exist (intent-1 behaviour).
- Installers: `install.ps1` recognises `bin\dtech-aidlc-mcp.exe` by hash against the installed copy, so after a v1 upgrade a pre-existing foreign MCP exe is refused; upgrade replaces `lib/dtech-aidlc` wholesale; the 1.2.2 installer does not recognise the v2 marker (Assumption).
- Dashboard cramped near the 60rem breakpoint (960-1040 px with the detail panel open; event column 47-127 px). Option: raise the breakpoint to about 66rem.
- Some static asset assertions match exact spellings; they guard the fixes as written, not every equivalent regression.
- Test helpers can hang instead of failing on a stuck server (`Session::call` and `mcp_parity` racer block on `read_line`/barrier without a watchdog).
- Unix-only tests: symlink cases, the measurement test (ignored by default).
- `.claude/skills/dtech-aidlc/.projection.json` stays at 1.2.2 on purpose.

## IF-018 / CFG-010 deviations (AI conductor decisions, WU-018; for the release-gate record)

1. `routing_golden/unsafe.json` is an eighth golden (one monitor completion line). ADR-033's count of seven missed it.
2. Optional whitespace around the Authorization field value is stripped per RFC 9110; the `Bearer ` scheme and the token stay exact and case-sensitive.
3. The bounded drain (1 MiB or 1 s) runs before every error reply except 408 and 503, not only after a refused method, so 400/403/404/421/431 replies are delivered when request bytes are pending. This adds up to 1 s per error reply (T2-09 notes it).

## Threat-review low findings carried as follow-ups (23 low, 0 high, 0 medium; not blocking; the findings table of `WU-021/threat-model-review.md` is the full list, and its follow-up findings without a T2 id are described here by title)

- Fixed in WU-021: T2-10, T2-06 gate item and README disclosures, T2-03 and the token-in-AI-transcripts token disclosures (README), T2-05 Windows line, T2-13, T2-22a, the Trusted Types CSP finding, T2-22c (extended test, verified only by CI).
- Open follow-ups: T2-20 client-side event shape check (stale-tab impostor on a freed port); T2-22b and T2-22d `install.ps1` rollback in `finally` and path handling; T2-18 existing store `.gitignore` content; the query whole-file re-read bound (finding on the 64 MiB cap); impossible timestamps accepted by `parse_line` and not counted as skipped; the writer does not enforce the closed name and errorCode sets and its id charset is broader than the ADR-024 validators; loose installer ownership matching (launchers matched by substring, marker content never read); installer housekeeping (orphan launchers kept with a misleading message, partial leftover sweep); T2-04 doc amendment for the OWS deviation (IF-018 and ADR-029); threat-model and ADR-029 text updates (T2-03 is now Observed).
- Residuals accepted by the AI review, which the owner may overturn: T2-06 same-OS-user boundary; T2-03 token in browser history; T2-05 other OS users reach static assets; T2-09 slot exhaustion by a local client (recovers on its own); T2-15 writer drops make metrics incomplete (disclosed; events still queued when the 500 ms EOF drain expires are one more silent path); predictable `.$$` temp and stage names in the install prefix (needs another principal with write access to a sticky world-writable prefix); upgrade and uninstall delete user files inside `lib/dtech-aidlc` (only partly covered by "replaces `lib/dtech-aidlc` wholesale" above; README states it).
- Windows items of the threat review are an Assumption except where CI on 6aec542 ran them (T2-19 BCryptGenRandom, TS-022, TS-034 PowerShell, in-use test).

## Process and records

- AI conductor decisions are not owner approvals: unsafe.json golden, IF-018 deviations, WU-021 edits to earlier units' files, CHANGELOG edit in WU-022, the design and threat reviews, and the live check run by the conductor.
- `PROJECT.md` and the codebase map are not refreshed for intent 2 (Q16/Q25 follow-ups).
- README keeps the intent-1 follow-ups under Q12 untouched.
- Golden count: the conductor note says 9 changed golden files; `git diff` from 327b02a to HEAD lists 8 (Open question, see `regression-report.md`).
- Release: 1.3.0 prepared only. Nothing tagged, published or merged; no PR exists.
