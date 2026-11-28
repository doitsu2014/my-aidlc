# Implementation plan (intent 2)

Direction: Option A (owner). A background telemetry writer thread with a bounded queue (drops disclosed, not counted), a telemetry hook in the MCP adapter (not the facade; the CLI writes no telemetry), MCP flag `--telemetry on|off` default on, an in-process `dtech-aidlc monitor` serving a std-only HTTP/1.1 loopback server with a per-run token and three embedded assets, zero new crates, version 1.3.0 prepared only. Design only in this stage: no source, test, Cargo, README, CHANGELOG, PROJECT.md, plans, scripts or CI file was changed. Detailed contracts are in the interfaces-and-contracts and functional-design artifacts (FN-023..FN-044, IF-012..IF-020, CFG-007..CFG-013).

## Scope
In scope: AC-001..AC-030 as read through the owner decisions (CON18: the CLI part of AC-006 and AC-012 is superseded; Q1), flake fixes, `deps add` lock, missing intent-1 MCP tests, both-binary installers, release preparation, Studio mock update with an AI dashboard design review, AI threat-model review.

Out of scope: any CLI telemetry; CI or clippy cleanup (Q12; ci.yml unchanged); creating the tag, publishing or pushing (Q5); `harness/` changes; refreshing PROJECT.md and the codebase map (Q16, Q25, owner follow-up); renaming the Studio mock title (until the owner confirms); any new crate; Linux as a required platform; auth beyond the loopback token; dropping "(proposed)" without evidence (Q10, WU-022).

## Environment and baseline steps (before any edit, per unit)
1. Run the baseline command from the test strategy with a unit-specific `TMPDIR` under `/private/tmp`. Expected (Observed, local macOS run): 545 passed, 0 failed on macOS at 327b02a; 547 on Windows (Observed, intent-1 CI run 46090623346) (the PROJECT.md "Known baseline failures" section is stale; Q16). Save the pass/fail name list.
2. `git status tests/golden` is clean at the start. After every unit `git diff --stat -- tests/golden` lists only that unit's allowed goldens.
3. Confirm known flakes by loop before WU-011 changes anything.

## Ordered tasks
Wave 1 in parallel worktrees (no shared files): WU-011 flake fixes, WU-012 deps lock, WU-013 telemetry core and `/telemetry/` ignore line, WU-016 design pack and Studio mock, WU-020 installers.
Wave 2: WU-014 MCP wiring (after WU-013); WU-017 query engine (after WU-013 and an accepted WU-016).
Wave 3: WU-015 missing MCP tests (after WU-014); WU-018 monitor server and CLI wiring (after WU-017).
Wave 4: WU-019 dashboard build (after WU-018 and an accepted WU-016).
Wave 5: WU-021 release candidate: README sections, AI threat review, installed monitor smoke, then last the 1.3.0 bump and the seven-golden re-bless.
Wave 6: WU-022 drop "(proposed)" only with green windows-latest and macos-latest CI and an owner-recorded live Claude Code `tools/call`.
Twelve units exceed the split threshold of 6; see work-units.md for the recommendation. Deviation from the implementation-planner default (recommend split or reduce/recombine): the plan recommends keeping all 12 units whole and names the split line; the gate should record this deviation.

## File-level change list
| File | Change | Unit |
| rust-cli/artifacts.rs, onboarding/projection.rs, scope.rs, prerequisites.rs | test-module temp-dir counter; wider timeout test | WU-011 |
| rust-cli/dependencies.rs | `deps add` under `with_state_lock` | WU-012 |
| tests/deps_lock.rs | new, 3 concurrent processes | WU-012 |
| rust-cli/telemetry/mod.rs, sink.rs | new: schema, os_random, store state, sink, writer | WU-013 |
| tests/telemetry_guards.rs | new: TS-023 Q1 guards (Sink only under mcp/, stdout guard, CLI run creates no telemetry) | WU-013 |
| rust-cli/lib.rs | `pub mod telemetry;` then `pub mod monitor;` | WU-013, WU-018 |
| rust-cli/knowledge.rs | `/telemetry/` line in IGNORE_BLOCK | WU-013 |
| tests/golden/routing_golden/lifecycle.json, lifecycle-embedded.json | re-bless: `/telemetry/` line only | WU-013 |
| rust-cli/mcp/mod.rs, protocol.rs, tools.rs | flag parsing, `on_error`, `traced_call` | WU-014 |
| tests/fixtures/mcp/common.rs, mcp_stdio.rs, mcp_parity.rs | `--telemetry off` at spawn sites; refusal cases | WU-014 |
| tests/mcp_telemetry.rs | new | WU-014 |
| tests/mcp_followups.rs | new | WU-015 |
| plans/mcp-server-enhancement/mcp-studio-server.html | system fonts, shipped tool names, telemetry and monitor wording | WU-016, WU-022 |
| rust-cli/telemetry/query.rs | new; one `pub mod query;` line in telemetry/mod.rs | WU-017 |
| rust-cli/monitor/mod.rs, http.rs, assets stubs | new | WU-018 |
| rust-cli/args.rs, main.rs, help.txt, completion.zsh | `monitor` command | WU-018 |
| tests/repo_content.rs, tests/routing_golden.rs | COMMAND_KEYS gains monitor | WU-018 |
| tests/golden/utility_golden/help-and-completion.json, routing_golden/validation.json | re-bless: monitor lines and two new steps only | WU-018 |
| tests/monitor_http.rs | new | WU-018 |
| rust-cli/monitor/assets/*, tests/monitor_assets.rs | full dashboard; static checks | WU-019 |
| scripts/rust-install/install.sh, install.ps1, tests/rust_cargo_install.rs, README.md line 380 | both binaries, marker v2; TS-034 installer round trip | WU-020 |
| README.md, CHANGELOG.md, Cargo.toml, Cargo.lock, .github/badges/version.svg, tests/mcp_packaging.rs, seven goldens, tests/repo_content.rs, tests/rust_cargo_install.rs | release candidate; TS-034 installed-monitor smoke (AC-026) | WU-021 |
| README.md, tests/repo_content.rs (every needle referencing the proposed heading), mcp-studio-server.html | drop "(proposed)" | WU-022 (conditional) |

## How CLI byte-parity is protected
- The CLI never constructs a Sink and writes no telemetry; the facade is untouched. This is registered as TS-023 (owner WU-013, `tests/telemetry_guards.rs`): `ts023_sink_is_built_only_under_mcp`, `ts023_no_stdout_writes_outside_main_and_mcp_handoff` (no `println!`, `print!(` or `io::stdout` under `rust-cli/` except `main.rs` and `mcp/mod.rs`, once each; Observed today at `main.rs:399` and `mcp/mod.rs:73`), which also keeps the monitor on stderr only, and `ts023_cli_run_creates_no_telemetry_dir`.
- Every golden except seven is byte-identical; the seven are re-blessed in three scoped steps, each with one reason: WU-013 `/telemetry/` ignore line (2 goldens), WU-018 `monitor` in help, completion and validation (2 goldens), WU-021 version 1.2.2 to 1.3.0 (all 7, filtered diff must be empty). `routing_golden/unsafe.json` is not in the set; monitor unsafe start is tested in TS-029.
- Bless only with the baseline environment and only on the producing targets; any other changed golden stops the unit (ADR-033 reopen trigger). The `deps add` golden must stay byte-identical (ADR-037 reopen trigger).
- MCP transcript snapshots are unchanged because snapshot spawns pass `--telemetry off`; a test with telemetry on proves stdout bytes equal the off run.

## Risks
| Risk | Mitigation |
| Telemetry harms the MCP workflow | queue never blocks, writer panic or error disables telemetry silently, stdout identity test, never touches the state lock |
| Secrets or paths in the store | field-by-field constructor, validated ids only, sentinel-secret grep test, AI threat review |
| Monitor reachable from a browser or LAN | loopback bind, Host, Origin, Sec-Fetch-Site, bearer token with constant-time compare, CSP, LAN-address refusal test |
| Windows code never compiled (BCryptGenRandom, timeouts, drain, installer preflight) | windows-latest CI is the first proof; raw-dylib fallback; installer behaviour stays an Assumption until then |
| Golden drift hides a regression | per-reason filtered diffs, three scoped steps |
| New tests flake | 20x loops, no weakened assertions, grouped slow cases |
| Dashboard built before the design is accepted | WU-017 and WU-019 depend on WU-016 acceptance |
| `with_state_lock` thread semantics unknown | TS-031 uses processes |
| Queue headroom, timeouts, retention overshoot, parse-cache memory are Estimates | measured in WU-013, WU-017, WU-018 and recorded against the ADR reopen triggers |
| Q21 live `tools/call` not observed | WU-022 parked; README label stays; release notes name the released phases |

## Rollback
No unit publishes anything. Each unit lands as its own commit set on the feature branch and reverts independently with `git revert`, except dependencies: reverting WU-013 requires reverting WU-014, WU-017, WU-018 and WU-019. A mistaken golden re-bless is reverted by restoring the file from HEAD. Existing telemetry directories are ignorable and deletable by the user at any time; the reader never deletes. The release bump (WU-021) is the last edit, so it can be dropped without touching functional units. No tag exists to remove.

## Windows verification via CI
No `ci.yml` change. New tests run in the existing `cargo test --locked --no-fail-fast` step on windows-latest and macos-latest. Owner pushes and reads the run; this stage cannot. Windows-only expectations are listed in the test strategy. Manual desktop checks (firewall prompt, Ctrl+C batch prompt) are recorded in WU-021.

## Release steps (prepare only; no tag, no push)
1. All of WU-011..WU-020 done; baseline green locally.
2. WU-021: README telemetry, monitor and troubleshooting sections; AI threat-model review (no unresolved high finding); installed monitor smoke test (`ts034_installed_cli_starts_monitor_from_its_own_location`, AC-026).
3. Last: `Cargo.toml` to 1.3.0; `cargo build --locked` changes only the dtech-aidlc line of `Cargo.lock`; badge; LOCK_SHA256 recomputed from the CR-stripped lock; CHANGELOG `## 1.3.0` entry covering intents 1 and 2 with the AC-024 checklist (local optional capabilities, self-asserted approver names, telemetry is not audit); re-bless seven goldens and verify each filtered diff is empty. Do not touch `.claude/skills/dtech-aidlc/.projection.json` (tracked, `engineVersion` 1.2.2): it is not test-pinned and is written by onboarding, not by a release, so it is deliberately left at 1.2.2 and must not be re-blessed or hand-edited by this step.
4. Hand the owner: a green local baseline, the CI instruction, the manual checklist, and the WU-022 condition. Tagging and publishing stay with the owner.
