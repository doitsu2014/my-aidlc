# Completed scope: please-implement-plans-mcp-server-enhancement-sc-2

## 1. Stages (from `state.json`, Observed)

| Phase | Stage | Result |
|---|---|---|
| Analyze | problem-framing | complete |
| Analyze | context-and-risk | complete; gate analysis-baseline approved |
| Ideate | option-generation | complete |
| Ideate | direction-selection | complete; gate solution-direction approved |
| Develop | implementation-design | complete (12 work units) |
| Develop | red-tests / build-and-verify | complete per unit; final sweep complete; gate release-candidate approved |
| Launch, Curate | - | outside the solo scope |
| Archived | intent-record, knowledge-writeback | this stage and the next |

## 2. Work units (all `done`)

| Unit | Title | Commit | Key tests |
|---|---|---|---|
| WU-011 | Flaky-test fixes | c23b4a8 | TS-032 loop evidence |
| WU-012 | `deps add` lock | 695d62f | TS-031 |
| WU-013 | Telemetry core plus managed ignore line | 2f9a33e | TS-021..TS-025 |
| WU-020 | Installers for both binaries | cba3669 | TS-034 |
| WU-016 | Dashboard design pack, AI design review, Studio mock | db30d93 | design review record |
| WU-014 | MCP telemetry wiring | 164f09c | TS-026, TS-027 |
| WU-017 | Query engine | d3f0312 | TS-028 |
| WU-015 | Missing intent-1 MCP tests | ddf97ca | TS-033 |
| WU-018 | Monitor server and CLI wiring | ed1b131 | TS-029, TS-037 |
| WU-019 | Dashboard build | 63852db (+ 372b60f forced-colors fix) | TS-030 |
| WU-021 | Release candidate: docs, AI threat review, 1.3.0 prep | 6aec542 | TS-034, TS-035, TS-036, threat review |
| WU-022 | Drop the proposed label | 483e65f | repo_content needles; CI + live check preconditions |

## 3. Release-candidate follow-ups (11, from `develop/build-and-verify/gate-recommendation.md`)

1. DeepSeek Harness host check; AI-host launch of the Windows .exe; Windows firewall and .cmd Ctrl+C prompts.
2. Threat-review low findings: T2-20, T2-22b, T2-22d, T2-18, query re-read bound, NEW-5 timestamps, writer closed-set check, installer ownership matching and housekeeping.
3. Accepted residuals the owner may overturn: T2-06, T2-03, T2-05, T2-09, T2-15, install-prefix temp names, lib/dtech-aidlc replaced wholesale.
4. Contract text drift: IF-018 and ADR-029 (OWS stripping, wider drain), T2-03 to Observed, ADR-033 golden list to eight files.
5. Push `372b60f` and watch CI when next authorised.
6. Manual checks: real screen reader, real Windows High Contrast, invalid-filter announcement.
7. Q16/Q25: refresh PROJECT.md and the codebase map.
8. Process notes: WU-021 edited earlier units' files for AC-030 fixes; WU-022 edited the CHANGELOG; the live check exercised only `status`.
9. Product limits: dashboard cramped at 960-1040 px with detail open; query cap can be exceeded by one rotated file; measure the Estimates in early use.
10. Out of scope (Q12): clippy -D warnings failure, actions/checkout Node 20.
11. Launch: tag, publish, release, merge or PR each need a separate owner approval.

## 4. Not done

Launch and Curate (scope); DeepSeek verification (deferred); tag/publish/merge (not authorised).
