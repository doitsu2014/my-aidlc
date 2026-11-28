# Work units

Twelve units exceed the default split threshold of 6, and they touch unrelated components (telemetry, monitor, dashboard, installers, release). Deviation from the implementation-planner default (which is to recommend that the owner split the intent or reduce/recombine units), disclosed here for the gate to note: the recommendation to the owner is to keep the intent whole, as the owner did for intent 1, because the units have disjoint owned files and are listed in dependency order; if the owner prefers to split, the natural cut is {WU-011, WU-012, WU-015, WU-020} (inherited follow-ups and installers) versus {WU-013, WU-014, WU-016..WU-019} (telemetry and monitor) with WU-021 and WU-022 last.

Waves: W1 WU-011, WU-012, WU-013, WU-016, WU-020 (parallel worktrees). W2 WU-014 (after WU-013) and WU-017 (after WU-013 and an accepted WU-016). W3 WU-015 (after WU-014) and WU-018 (after WU-017). W4 WU-019 (after WU-018 and an accepted WU-016). W5 WU-021. W6 WU-022 (conditional). Critical path: WU-013 and WU-016, WU-017, WU-018, WU-019, WU-021.

Sequential overlaps (never in the same wave): `rust-cli/lib.rs` (WU-013 then WU-018), `rust-cli/telemetry/mod.rs` (WU-013 then one `pub mod query;` line in WU-017), `rust-cli/monitor/assets/*` (WU-018 stubs then WU-019 content), `README.md` (WU-020 line 380, WU-021 new sections, WU-022 label), `tests/repo_content.rs` (WU-018 COMMAND_KEYS, WU-021 new test fn, WU-022 every needle that references the proposed heading, including any in the WU-021 test fn), `tests/rust_cargo_install.rs` (WU-020 then one WU-021 smoke test; both own TS-034), `plans/mcp-server-enhancement/mcp-studio-server.html` (WU-016 then WU-022).

Every unit ends with the baseline suite (see test-strategy) and `git diff --stat -- tests/golden` showing only the goldens the unit is allowed to re-bless. Any other golden change stops the unit (ADR-033 reopen trigger). Hard limits for all units: zero new crates, `Cargo.lock` byte-identical except WU-021's version line, no commit tag, no push.

Provenance: file and line anchors are Observed in the working tree at 327b02a; spike results are Observed (ephemeral, macOS, under `<spike2>` = the session scratchpad `spike2/`) and cited where used; Windows behaviour is an Assumption until windows-latest CI runs; queue headroom, rotation peak, parse-cache memory, timeouts and the drain bound are Estimates (basis: ADR values) until the owning unit measures them.

## WU-011 Flaky-test fixes
- Stories: US-013, US-010
- Criteria: AC-027, AC-029
- Depends on:
- Tests: TS-032
- Owned files: `rust-cli/artifacts.rs`, `rust-cli/onboarding/projection.rs`, `rust-cli/scope.rs`, `rust-cli/prerequisites.rs` (all `#[cfg(test)]` code only). FN-038, ADR-036.
- Focused checks: record loops A, B and C from the test strategy before any edit (the collision cause is Observed in a spike, `<spike2>/impl-test-mapper/nanos.rs`; the loops make it Observed in this repository); after the fix `cargo test --locked --lib -- artifacts::tests onboarding::projection::tests scope::tests prerequisites::tests::check_timeout_is_reported` is 40/40 green; full baseline 3 times.
- Done when: the before and after counts per test are recorded in the unit evidence; the scope.rs change is kept only if loop A or B showed it failing; no production line changed.

## WU-012 deps add lock
- Stories: US-013
- Criteria: AC-027
- Depends on:
- Tests: TS-031
- Owned files: `rust-cli/dependencies.rs`, new `tests/deps_lock.rs`. FN-037, ADR-037.
- Focused checks: write `ts031_` test first (3 concurrent `deps add` processes, 3 edges kept); confirm red (Observed in the spike `<spike2>/release-hardening/deps-lock.diff`: red 6 of 6 before, green 6 of 6 after), then add `with_state_lock` around the add path; loop 10 times; `cargo test --locked --test rust_repos_deps_ids` unchanged.
- Done when: the test is green 10/10, the deps add golden is byte-identical, and the red-before result is recorded. A golden change stops the unit.

## WU-013 Telemetry core plus managed ignore line
- Stories: US-001, US-003, US-004
- Criteria: AC-002, AC-003, AC-005, AC-008, AC-009, AC-010, AC-012, AC-027, AC-028
- Depends on:
- Tests: TS-021, TS-022, TS-023, TS-024, TS-025
- Owned files: new `rust-cli/telemetry/mod.rs` and `rust-cli/telemetry/sink.rs`, new `tests/telemetry_guards.rs` (TS-023 Q1 guards), `rust-cli/lib.rs` (`pub mod telemetry;`), `rust-cli/knowledge.rs` (`/telemetry/` line in IGNORE_BLOCK only), `tests/golden/routing_golden/lifecycle.json`, `tests/golden/routing_golden/lifecycle-embedded.json`. FN-023..FN-028; IF-012, IF-013, IF-014; CFG-008, CFG-009.
- Focused checks: `cargo test --locked --lib telemetry::`; `cargo test --locked --test telemetry_guards` (written first: Sink only under `rust-cli/telemetry/` and `rust-cli/mcp/`, no stdout writes outside `main.rs` and `mcp/mod.rs`, a CLI run with `--project <Tmp>` creates no `.aidlc/telemetry`); re-bless only with the baseline environment and `DTECH_AIDLC_BLESS=1 cargo test --locked --test routing_golden`; `git diff -- tests/golden` shows only the added `/telemetry/` line in the two lifecycle goldens; measure queue headroom and rotation peak and record them against the ADR reopen triggers.
- Done when: schema, os_random, sink and FileStore tests are green, the stall test proves a 10 s stalled writer never blocks the producer for 1 s, the three `ts023_` Q1 guard tests are green (the CLI never builds a Sink and writes no telemetry), and the golden diff is exactly the `/telemetry/` line.

## WU-014 MCP telemetry wiring
- Stories: US-001, US-002, US-004
- Criteria: AC-001, AC-002, AC-004, AC-005, AC-006, AC-007, AC-008, AC-010, AC-012, AC-028
- Depends on: WU-013
- Tests: TS-026, TS-027
- Owned files: `rust-cli/mcp/mod.rs`, `rust-cli/mcp/protocol.rs`, `rust-cli/mcp/tools.rs`, `tests/fixtures/mcp/common.rs` (`run_mcp_env` passes `--telemetry off`, new helper taking extra args), `tests/mcp_stdio.rs`, `tests/mcp_parity.rs` (spawn sites pass `--telemetry off`; refusal loop gains the new cases), new `tests/mcp_telemetry.rs`. FN-029, FN-030, FN-031; IF-015, IF-016; CFG-007.
- Focused checks: write the `on_error` table test, the flag-refusal cases and the stdout-identity test (on equals off) first; `cargo test --locked --test mcp_stdio --test mcp_parity --test mcp_redaction --test mcp_telemetry`; 20x loop of `mcp_telemetry`; confirm the `stage_selected` id keys in the next-directive JSON (gap from part 1) and validate every stored id with the existing validators.
- Done when: every existing MCP transcript test is unchanged except for the `--telemetry off` argument, reply bytes with telemetry on equal off, sentinel secrets have zero hits in the store, and no workflow event is written without a durable Ok.

## WU-015 Missing intent-1 MCP tests
- Stories: US-013, US-010
- Criteria: AC-027, AC-029
- Depends on: WU-014
- Tests: TS-033
- Owned files: new `tests/mcp_followups.rs` only. FN-044, ADR-036.
- Focused checks: `cargo test --locked --test mcp_followups`; every spawn passes `--telemetry off` and `--project <Tmp>`. These are characterisation tests and are expected green on arrival.
- Done when: AI-mode gate with policy edited between calls, unsafe layout at launch and mid-session, CLI and MCP interleaving (sequential and concurrent), restart and shutdown mid-session are covered. A red result is a real bug: a small fix in the unit's own scope is made here; a fix that lands in a WU-014 file parks the unit as dep-unit and the owner decides.

## WU-016 Dashboard design pack, AI design review, Studio mock
- Stories: US-007
- Criteria: AC-011, AC-022
- Depends on:
- Tests:
- Owned files: `plans/mcp-server-enhancement/mcp-studio-server.html` (rename the title "Orca MCP Studio" to "dtech-aidlc MCP Studio" in `<title>` and the header `<h1>`, owner decision 2026-11-28, User-provided), plus the design pack and AI review artifacts in this unit's own develop folder. FN-042, ADR-038, ADR-039.
- Focused checks: the mock has no Google Fonts link and no remote URL; the review record rules on ADR-039 readings 1, 2 and 8, picks 320 or 360 px (recommend 320), fixes the IF-017 field names and states the drops coverage line; the design pack contains wireframes, the QS-10 state matrix, tokens and a privacy map.
- Done when: the AI design review is accepted in writing. WU-017 and WU-019 do not start before that. If the gate decides the query API is not dashboard implementation, WU-017 may start after WU-013 alone.

## WU-017 Query engine
- Stories: US-006, US-003
- Criteria: AC-003, AC-016, AC-017, AC-018, AC-019
- Depends on: WU-013, WU-016
- Tests: TS-028
- Owned files: new `rust-cli/telemetry/query.rs`, one `pub mod query;` line in `rust-cli/telemetry/mod.rs`. FN-032; IF-017; CFG-011.
- Focused checks: write the parameter-rejection tests (`invalid_filter`, `%` and path-like keys and values, out-of-range `range` and `limit`, unknown or duplicate keys) and the cursor decode and mismatch tests first and confirm they are red before `query.rs` has behaviour (Observed gap: these were planned only in WU-018, where the code already exists); `cargo test --locked --lib telemetry::query`; caps are injected into the reader so the 64 MiB and 250,000-event paths run in unit tests; measure parse-cache memory and record it.
- Done when: the response shape matches the contract as ruled in WU-016, the reader never writes or deletes, tolerance counters and cursor behave as specified, and incomplete is never counted as success.

## WU-018 Monitor server and CLI wiring
- Stories: US-005
- Criteria: AC-013, AC-014, AC-015, AC-016, AC-025, AC-027, AC-030
- Depends on: WU-017
- Tests: TS-029, TS-037
- Owned files: new `rust-cli/monitor/mod.rs` and `rust-cli/monitor/http.rs`, stub `rust-cli/monitor/assets/{index.html,app.js,app.css}`, `rust-cli/lib.rs` (`pub mod monitor;`), `rust-cli/args.rs`, `rust-cli/main.rs`, `rust-cli/help.txt`, `rust-cli/completion.zsh`, `tests/repo_content.rs` (COMMAND_KEYS only), `tests/routing_golden.rs` (COMMAND_KEYS only), `tests/golden/utility_golden/help-and-completion.json`, `tests/golden/routing_golden/validation.json`, new `tests/monitor_http.rs`. FN-033, FN-034, FN-035; IF-018, IF-019; CFG-010.
- Focused checks: write the ADR-032 table, header, Host/Origin, `ct_eq` and asset-traversal tests first (filter and cursor rejection is already test-first in WU-017; here only its HTTP status mapping is checked); `cargo test --locked --test monitor_http --test repo_content --test routing_golden --test utility_golden`; re-bless with the baseline environment and check that the golden diff contains only `monitor` lines and the two new validation steps; `routing_golden/unsafe.json` unchanged; measure the drain bound and the slot behaviour.
- Done when: every ADR-032 status code row except row 14 passes over a raw TcpStream, row 14 (500 `internal_error`) passes as the panic-guard unit test (documented deviation from ADR-032, see test-strategy Unverified), the startup line appears once on stderr with empty stdout, the token appears nowhere else, the module writes no stdout, and only the monitor-line golden diffs exist.

## WU-019 Dashboard build
- Stories: US-006, US-007
- Criteria: AC-011, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021
- Depends on: WU-016, WU-018
- Tests: TS-030
- Owned files: `rust-cli/monitor/assets/{index.html,app.js,app.css}` (full content), new `tests/monitor_assets.rs`. FN-036; CFG-012.
- Focused checks: `cargo test --locked --test monitor_assets --test monitor_http`; static greps from the test strategy; a manual browser pass against the WU-016 state matrix, keyboard flow and the chosen narrow width, recorded as evidence.
- Done when: the assets follow the accepted design, no forbidden API appears in `app.js`, the token is read once from the fragment and removed with `history.replaceState`, and the browser checklist is recorded (automation cannot cover it).

## WU-020 Installers for both binaries
- Stories: US-009
- Criteria: AC-026
- Depends on:
- Tests: TS-034
- Owned files: `scripts/rust-install/install.sh`, `scripts/rust-install/install.ps1`, `tests/rust_cargo_install.rs`, `README.md` line 380 only. FN-039; IF-020; CFG-013 (marker). ADR-035.
- Focused checks: rewrite the ScriptFixture to copy the real binaries; `cargo test --locked --test rust_cargo_install`; the exact existing skip strings are printed so the CI skip grep covers the new cases; the intent-1 settings-bytes check around install, upgrade and uninstall.
- Done when: install, upgrade (including from a CLI-only v1 prefix, keeping unrecognised files) and uninstall work for both binaries through sh and pwsh, the in-use preflight and rollback fix are in place, and Windows behaviour is recorded as an Assumption until windows-latest CI.

## WU-021 Release candidate: docs, AI threat review, 1.3.0 prep
- Stories: US-008, US-011, US-012
- Criteria: AC-020, AC-023, AC-024, AC-025, AC-026, AC-027, AC-029, AC-030
- Depends on: WU-011, WU-012, WU-013, WU-014, WU-015, WU-016, WU-017, WU-018, WU-019, WU-020
- Tests: TS-034, TS-035, TS-036
- Owned files: `README.md` (new sections; heading and the "proposed" sentences stay), `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `.github/badges/version.svg`, `tests/mcp_packaging.rs` (LOCK_SHA256), `tests/repo_content.rs` (one new test fn), `tests/rust_cargo_install.rs` (one installed-monitor smoke test, `ts034_installed_cli_starts_monitor_from_its_own_location`, TS-034 shared with WU-020; it covers the AC-026 clause "the installed CLI shall launch the monitor from its own location"), the seven version goldens, plus the AI threat-model review artifact in this unit's develop folder. FN-040, FN-041; CFG-013. ADR-033, ADR-034.
- Focused checks: order inside the unit is docs, threat review, installed monitor smoke, then last the version bump; `cargo build --locked`; the lock diff is the single dtech-aidlc version line; LOCK_SHA256 recomputed from the CR-stripped lock; re-bless the seven goldens and check each with `diff <(git show HEAD:<f> | sed 's/1\.2\.2/1.3.0/g') <f>` which must be empty; full baseline.
- Done when: the threat review covers the six AC-030 topics and residuals T2-03, T2-05, T2-06, T2-15 and T2-22 with no unresolved high finding; the CHANGELOG 1.3.0 checklist is satisfied; the installed-monitor `ts034_` smoke test is green; the manual Windows and browser checklist is recorded; `.claude/skills/dtech-aidlc/.projection.json` is left at `engineVersion` 1.2.2 on purpose (not test-pinned, changed by onboarding, not by a release; not re-blessed); nothing is tagged or pushed.

## WU-022 Drop the proposed label (conditional)
- Stories: US-008
- Criteria: AC-025
- Depends on: WU-021
- Tests:
- Owned files: `README.md` (heading, the proposed paragraph, platform and host sentences; re-read before editing), `tests/repo_content.rs` (every needle that references the `## MCP server (proposed)` heading or the proposed wording: the existing one at `repo_content.rs:1941-1946` (Observed) and any added by WU-021's new test fn, which may locate the telemetry and monitor sections through that heading), `plans/mcp-server-enhancement/mcp-studio-server.html` (proposed label, title only if the owner confirmed). FN-043.
- Focused checks: runs only when windows-latest and macos-latest CI legs are green and the owner records a live Claude Code `tools/call` (Q21); `cargo test --locked --test repo_content`.
- Done when: all four README spots, every proposed-heading needle and the mock label flip together; `grep -n '(proposed)' tests/repo_content.rs` returns nothing. Without the evidence the unit is parked as owner-decision and the release notes say which phases are released.
