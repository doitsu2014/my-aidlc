# Test strategy (intent 2)

Framework: Rust built-in `cargo test` (unit tests in `rust-cli/**` modules, integration tests in `tests/*.rs`, golden suites through `tests/common/golden.rs`). Zero new crates, so no test helper crate: raw `std::net::TcpStream`, `Command` and `std::fs` only. Test function names start with the lowercase TS id, for example `ts029_...` for TS-029, so a failure is attributed by name. The numbers ts021..ts037 do not clash with the intent-1 prefixes ts001..ts020 and ts042.

## Levels
### Unit
- Applies: yes
- Why: schema, os_random, sink, FileStore, `on_error` table, query engine, `ct_eq` and flag parsing are pure or near-pure code with tight contracts (TS-021..TS-026, TS-028).
### Integration
- Applies: yes
- Why: the MCP adapter, the monitor HTTP layer, deps lock and installers only show their behaviour across processes (TS-027, TS-029, TS-031, TS-033, TS-034, TS-037).
### End to end
- Applies: yes, limited
- Why: the installed CLI starting the monitor from its own location and the installed MCP binary answering initialize and tools/list (TS-034). TS-034 has two owners: WU-020 adds the installer round trip (install, upgrade, uninstall of both binaries, installed MCP binary answering initialize and tools/list); WU-021 adds the installed-monitor smoke `ts034_installed_cli_starts_monitor_from_its_own_location` in `tests/rust_cargo_install.rs`, because the monitor exists only after WU-018 and WU-020 runs in wave 1 without it. Rendered dashboard states and keyboard use need a browser and are a recorded manual checklist plus the AI design review, because there is no JS runner and no new crate.
### Performance
- Applies: yes, bounded
- Why: only what the ADRs bound: producer never blocked past 1 s by a stalled writer for 2,000 sends (TS-023), rotation at 5 MiB, read caps via injected values (TS-028). Parse-cache memory, queue headroom, timeouts and drain bound are measured and recorded against ADR reopen triggers; they are Estimates until then.
### Security
- Applies: yes
- Why: the dominant risk. Test-first for the ADR-032 status and header table, Host/Origin/Sec-Fetch-Site, token compare, traversal and filter rejection, sentinel-secret grep of the store, symlink refusal in writer and reader, and the installer uninstall-only-recognised rule. The AC-030 AI threat-model review is a release gate, not a test.

## Criterion to level
| Test | Criterion | Function | Level |
| TS-021 | AC-002, AC-003 | FN-023 | unit |
| TS-022 | AC-010, AC-015 | FN-024 | unit |
| TS-023 | AC-006, AC-010, AC-012, AC-028 | FN-026 | unit, plus static guard and CLI integration (Q1) |
| TS-024 | AC-008, AC-009, AC-012 | FN-025, FN-027 | unit |
| TS-025 | AC-008, AC-027 | FN-028 | integration (golden) |
| TS-026 | AC-005 | FN-030 | unit |
| TS-027 | AC-001, AC-002, AC-004..AC-008, AC-010, AC-012, AC-028 | FN-029, FN-031 | integration |
| TS-028 | AC-003, AC-016..AC-019 | FN-032 | unit |
| TS-029 | AC-013..AC-016, AC-030 | FN-033, FN-034 | integration, security |
| TS-030 | AC-011, AC-016, AC-017, AC-019, AC-020, AC-021, AC-022 | FN-036, FN-042 | static check plus manual |
| TS-031 | AC-027 | FN-037 | integration |
| TS-032 | AC-029 | FN-038 | evidence record |
| TS-033 | AC-027, AC-029 | FN-044 | integration |
| TS-034 | AC-026 | FN-039 | integration, end to end |
| TS-035 | AC-023, AC-024, AC-027 | FN-040 | integration (release consistency) |
| TS-036 | AC-020, AC-025, AC-030 | FN-041 | static check |
| TS-037 | AC-025, AC-027 | FN-035 | integration (golden and parity) |

Mapping to Ideate labels: T2-10, T2-11, T2-12, T2-15 to TS-021, TS-023, TS-026, TS-027; T2-13, T2-16, T2-18 to TS-024, TS-025; T2-14 to TS-028, TS-030; T2-01, T2-02, T2-04, T2-07, T2-08, T2-09, T2-21, T2-23 to TS-029; T2-03, T2-20, T2-24 to TS-030; T2-19 to TS-022; T2-22 to TS-034.

## Work unit to test mapping
| Work unit | Tests |
| WU-011 | TS-032 |
| WU-012 | TS-031 |
| WU-013 | TS-021, TS-022, TS-023, TS-024, TS-025 |
| WU-014 | TS-026, TS-027 |
| WU-015 | TS-033 |
| WU-016 | none (design gate; evidence consumed by TS-030) |
| WU-017 | TS-028 |
| WU-018 | TS-029, TS-037 |
| WU-019 | TS-030 |
| WU-020 | TS-034 (installer round trip) |
| WU-021 | TS-034 (installed-monitor smoke only), TS-035, TS-036 |
| WU-022 | none (conditional; re-runs the existing repo_content suite) |

## Test-first versus alongside
| Unit | Approach | Reason |
| WU-011 | measure first, then fix | Loops A, B, C confirm each root cause before any edit; measured beats argued. |
| WU-012 | test-first | Observed (spike, `<spike2>/release-hardening/deps-lock.diff`, cited in decision-record ADR-037): 3 parallel adds lost edges in 6 of 6 runs before the fix and kept all edges in 6 of 6 after; the red result proves the test. |
| WU-013 | test-first for schema allowlist, writer stall/panic isolation, symlink refusal, prune safety and the TS-023 Q1 guards; alongside for rotation mechanics | Privacy and never-harm-the-workflow contracts are fixed by the ADRs; rotation numbers are tuned while building. |
| WU-014 | test-first for `on_error` table, flag refusals, stdout identity on vs off, sentinel grep; alongside for the trace shape | Public contract and security; trace details follow the adapter. |
| WU-015 | alongside (characterisation) | Expected green on arrival; red means a real bug. |
| WU-016 | n/a | Design gate. |
| WU-017 | test-first for parameter rejection (`invalid_filter`, `%` and path-like keys and values, out-of-range `range` and `limit`, unknown or duplicate keys) and cursor decode and mismatch, and for the caps; alongside for summary and counters | Query parameters are a trust boundary: the rejection and cursor `ts028_` tests must be red before `query.rs` exists so they show a behavioural red. WU-018 re-checks the same cases over HTTP only for the status mapping. Summary shape is defined while building; caps are injected. |
| WU-018 | test-first for the status table, headers, Host/Origin, `ct_eq` and asset-path traversal; the filter cases over HTTP check only the status mapping of errors already proven red in WU-017; alongside for lifecycle | Security contract fixed by ADR-032. |
| WU-019 | alongside, after the accepted design review | The design review defines the states. |
| WU-020 | test-first for uninstall-recognised-only and in-use preflight | Uninstall is destructive. |
| WU-021 | last; needles with the docs | Version bump and re-bless are done last, after every other golden change. |
| WU-022 | after evidence only | Gated on CI and the live `tools/call`. |

## Commands
- Build: `cargo build --locked` (builds both binaries)
- Format: `cargo fmt --check`
- Lint: `cargo clippy` is informational only; `-D warnings` already fails on `tests/orchestration_golden.rs:1108` and CI cleanup is out of scope (Q12).
- Test: `cargo test --locked --no-fail-fast`; focused: `cargo test --locked --lib <module>::`, `cargo test --locked --test <name>`.
- Baseline (whole existing suite): `mkdir -p /private/tmp/aidlc-test-<wu> && cargo build --locked && TMPDIR=/private/tmp/aidlc-test-<wu> PATH="$PWD/target/debug:$PATH" cargo test --locked --no-fail-fast`. Observed (local run on macOS at 327b02a): 545 passed, 0 failed, 0 ignored. Partial breakdown only, not the whole suite (these suites sum to 410): lib 246, repo_content 36, routing_golden 11, rust_cargo_install 6, mcp_stdio 13, mcp_parity 8, mcp_redaction 8, mcp_packaging 5, rust_unit_scheduling 77. Observed (static count of `#[test]`): 552 in total, lib 253 including cfg-gated tests, which is consistent with 545 run on macOS and 547 on Windows. Observed (intent-1 evidence, CI run 46090623346, `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md`): windows-latest 547/0, macos-latest 545/0. Each unit records its own before and after counts. The new baseline grows with each unit's tests.
- Golden re-bless (only WU-013, WU-018, WU-021): same environment plus `DTECH_AIDLC_BLESS=1`, only on the producing targets, then `git diff --stat -- tests/golden` must list exactly the unit's allowed files.

## Environment prerequisites
- Rust toolchain at or above the crate's `rust-version`: `rustc --version`; fix with `rustup update`.
- `TMPDIR` under `/private/tmp` and `target/debug` on `PATH`: without them the environment-caused failures return (rust_doctor 8, routing_golden 4, rust_orchestration_assets 3). Fix by using the baseline command above.
- `git`: `git --version`; the `git check-ignore` test skips visibly when missing.
- `sh` and `pwsh` for installer tests: `command -v pwsh`; the tests print the exact existing skip strings ('skipped: pwsh unavailable', 'skipped: POSIX shell unavailable') so the CI grep catches skips.
- Symlink permission on Windows: tests use the existing 'skipping: symlink creation not permitted' pattern.

## Environments and data
- Every telemetry-on or monitor test passes `--project <Tmp>`, never `None` or the repository root, so a test never writes `.aidlc/telemetry` into this repository. Every existing snapshot-comparing MCP spawn passes `--telemetry off`; the existing snapshots then also prove that off writes nothing (AC-001).
- Fixture event lines for query and monitor tests are produced with the same `Event::to_line` the writer uses.
- CI: `.github/workflows/ci.yml` is unchanged. New test files run in the existing `cargo test --locked --no-fail-fast` step on windows-latest and macos-latest (the required legs); Ubuntu stays best effort. The named "MCP transcript tests" step lists only three files, so `mcp_telemetry` and `monitor_http` run in the full-suite step; adding them needs a ci.yml edit plus a pinned-needle edit and is not proposed.
- Windows-only proof on windows-latest: BCryptGenRandom link and run (raw-dylib is the fallback), 408 slowloris (TimedOut versus WouldBlock), 2 MiB POST still receiving 405 (half-close and drain), prune with an open file, installed MCP exe spawn, in-use `-Upgrade` preflight, monitor kill and port refusal. Unix-only through `cfg(unix)`: modes 0o600 and 0o700, read-only directory (skip when euid is 0), SIGTERM exit signal.

## Flake handling
- Known flakes: `prerequisites::tests::check_timeout_is_reported`, `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `artifacts::tests::validates_work_units_and_registry_json`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. Cause for the temp-dir ones: pid plus nanosecond names collide at microsecond clock resolution (Observed, spike `<spike2>/impl-test-mapper/nanos.rs`: 91,662 of 100,000 consecutive calls equal on macOS); the timeout test races `sleep 1` against a 1 s timeout. WU-011 fixes both.
- Loop A (before fix, then after, 40 runs): `export TMPDIR=/private/tmp/aidlc-test-flake PATH=$PWD/target/debug:$PATH; for i in $(seq 40); do cargo test --locked --lib -- artifacts::tests onboarding::projection::tests scope::tests prerequisites::tests::check_timeout_is_reported 2>&1 | grep -E '^test .* FAILED'; done | sort | uniq -c`. Loop B: same with `-- --test-threads=1` as the control. Loop C: `check_timeout_is_reported` 40 times with every core loaded.
- New integration tests (`mcp_telemetry`, `monitor_http`, `deps_lock`) run a 20x local loop (10x for deps_lock) before the unit closes. If a telemetry test flakes because the writer is joined for at most 500 ms at EOF, revisit the flush wait Estimate; never weaken the assertion.
- Group all 5 s wait cases (408, 503) in one test function so the suite pays the wait once.
- A failure outside the owning unit's tests is not hidden by retry: rerun once, record it, and compare with the baseline list.

## Regression
- Byte parity of the CLI is guarded by all golden suites (cli_golden, utility_golden, routing_golden, onboard_golden, orchestration_golden, scope_golden, status_golden and the golden-backed rust_* suites, including the deps add golden) plus `mcp_parity` and the `tests/repo_content.rs` invariants. Only the seven goldens in ADR-033 may change, each for exactly one allowed reason: version bump, `monitor` in help/completion/validation, the `/telemetry/` ignore line. TS-025, TS-037 and TS-035 prove each reason with a filtered diff.
- Owner decision Q1 (the CLI writes no telemetry) is guarded by TS-023, owner WU-013, new file `tests/telemetry_guards.rs`, written test-first: `ts023_sink_is_built_only_under_mcp` (static scan: `Sink::start`, `Sink::spawn` and `telemetry::Sink` appear only under `rust-cli/telemetry/` and `rust-cli/mcp/`); `ts023_no_stdout_writes_outside_main_and_mcp_handoff` (static scan of `rust-cli/**/*.rs`: no `println!`, `print!(` or `io::stdout` outside `rust-cli/main.rs` and `rust-cli/mcp/mod.rs`, each allowed exactly once, matched by file and count, not line number; `eprintln!` is not a hit; Observed today: only `main.rs:399` and `mcp/mod.rs:73`); `ts023_cli_run_creates_no_telemetry_dir` (runs the CLI binary from `target/debug` with `--project <Tmp>` through a short lifecycle sequence taken from the lifecycle routing golden, then asserts `<Tmp>/.aidlc/telemetry` does not exist). The CLI goldens give only partial, indirect protection, so these three tests are the registered Q1 evidence.
- `regression.json` conventions: attributed regression follows the `tests` lists in `work-units.json`. A failing test owned by an unfinished unit is deferred to it; function names carry their `tsNNN_` prefix so attribution works by name. Tests that must keep failing deterministically are never added to a unit's `tests` list. Because TS-033 characterisation tests may reveal a real bug, a red `ts033_` test is a finding, not a flake.
- Golden failures must be attributed explicitly. The existing golden tests (for example `option_and_argument_validation_matches_the_golden_for_every_command`, `lifecycle`) have no `tsNNN_` prefix, and TS-025, TS-037 and TS-035 are filtered-diff procedures, not named functions, so name inference cannot attribute them; an unowned failure is only recorded and blocks just the final sweep. Rule: any golden failure in a unit's `regression.json` carries an explicit `ts` and `owner` (WU-013: TS-025; WU-018: TS-037; WU-021: TS-035), or the unit reports blocked, not complete. In any other unit a golden failure is a stop (ADR-033 reopen trigger), never an unowned record.
- The TS-023 Q1 guards (`tests/telemetry_guards.rs`, owner WU-013) scan the whole `rust-cli/` tree, so a later unit (WU-014, WU-018) that adds a stdout write or a Sink construction outside `mcp/` turns them red; that unit fixes its own code, the guard is never widened without a recorded reason.
- Existing tests expected to be affected: the `parse_args_accepts_only_project` unit test (replaced), the MCP spawn helpers (flag added), `command_table_matches_cli_spec` and `help_completion_and_the_command_table_stay_in_step` (monitor), `mcp_packaging` LOCK_SHA256 and the changelog and badge test (release), the installer fixture in `rust_cargo_install` (real binaries).

## Manual checks (recorded, not automated)
- Browser pass over states, keyboard flow and narrow width, the stale-tab stop-polling and the history residual (T2-03, T2-20); offline outbound-traffic observation (AC-011); interactive Ctrl+C exit 130; Windows firewall prompt on first bind; Windows "Terminate batch job" prompt. Recorded in WU-019 and WU-021.

## Unverified
- All Windows behaviour until windows-latest CI runs; pushing to CI is an owner action.
- ADR-032 row 14 (500 internal_error) is tested through a unit test of the panic guard, not an HTTP route, so the "slot released on panic" claim is covered only at unit level.
- Production read-cap constants (64 MiB, 250,000 events) are checked by constant assertion plus injected-cap unit tests, and one 513-file integration case.
- ACs worded "not testable as written" (AC-009 scenario 2, AC-016 range maximum, AC-017 stale threshold, AC-018 sample size, AC-028 scenario 4) are tested against the ADR-020/021/030 values and ADR-039 readings.
- The Q21 live Claude Code `tools/call` is a manual owner observation.
