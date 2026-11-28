# Work units

Ten units exceed the default split threshold of 6. Owner decision (project owner, 2026-11-27): keep all nine existing units and accept the count; the tenth, WU-010, fixes two pre-existing Unix test bugs. The units are small vertical slices with disjoint owned files and are listed in dependency order; execute as written. Waves are: W0 WU-001; W1 WU-002, WU-004, WU-006, WU-010 (parallel; WU-010 has no dependencies and may run in any wave before WU-008); W2 WU-003; W3 WU-005, plus WU-009 (draft can start in W1, final after WU-005); W4 WU-007 then WU-008 (WU-008 also waits for WU-010).

Every unit that edits `rust-cli/*.rs` outside `rust-cli/mcp/` must end with the TS-011 gate (full suite on the TS-017 baseline, goldens untouched).

## WU-001 Scaffold and packaging
- Stories: US-009
- Criteria: AC-013
- Depends on:
- Tests: TS-016, TS-017
- Owned files: `Cargo.toml` (CFG-001, CFG-002), `rust-cli/lib.rs`, `rust-cli/mcp_main.rs`, `rust-cli/application.rs` (doc comment only), `rust-cli/mcp/mod.rs` (stub `run`), new `tests/mcp_packaging.rs` (TS-016: dependency set, rust-version, Cargo.lock bytes).
- Focused checks: `cargo build` builds both bins; `cargo test --no-fail-fast` equals the TS-017 baseline; `Cargo.lock` byte-identical.
- Done when: both binaries build, the baseline pass/fail list is captured before the first edit and unchanged after, version and CHANGELOG untouched.

## WU-010 Fix pre-existing Unix test bugs
- Stories: US-001
- Criteria: AC-001
- Depends on: none
- Tests: TS-019
- Owned files: the `#[cfg(test)]` modules only of `rust-cli/onboarding/mod.rs` and `rust-cli/onboarding/projection.rs`. No product code. No other unit owns these files.
- Fix 1, `onboarding::tests::rejects_traversal_and_absolute_paths` (mod.rs ~:1597): `normalize_source_path("C:/escape")` is an error only on Windows, where `Path::is_absolute` accepts a drive prefix; on Unix `C:` is a normal component and the call returns `Ok("C:/escape")`. Keep `../escape` and `./docs/` assertions. Keep the `C:/escape` `is_err()` assertion under `#[cfg(windows)]` (unweakened), add `#[cfg(not(windows))]` asserting `normalize_source_path("C:/escape").unwrap() == "C:/escape"`, and add an all-platform `assert!(normalize_source_path("/escape").is_err())` so the Unix absolute-path case is covered.
- Fix 2, `onboarding::projection::tests::refuses_symlink_targets` (projection.rs ~:463, `#[cfg(unix)]`): `setup()` creates the skill directory and the source file but never `.claude/skills/demo/SKILL.md`, so `fs::remove_file` panics. Fix inside the test, not the shared `setup()` (other tests rely on the target being absent, for example the missing-projection test): delete the `remove_file` line, or create the file first with `fs::write`; then the symlink is created over an absent or fresh path and `projection_status(...).is_err()` is asserted as before. The implementer confirms that assertion is reached and holds.
- Focused checks: `cargo test --lib onboarding`; `git diff --stat` shows only the two test modules.
- Done when: both tests pass on macOS, the Windows assertion is preserved under `#[cfg(windows)]`, the lib baseline is 0 failures (193 passed), no product code or golden changed.

## WU-002 Prerequisite guard, error constant and pending preview
- Stories: US-001, US-006, US-007
- Criteria: AC-001, AC-008, AC-009, AC-011, AC-012
- Depends on: WU-001
- Tests: TS-007, TS-010, TS-011
- Owned files: `rust-cli/orchestrate.rs`, `rust-cli/prerequisites.rs`, `rust-cli/intent_commands.rs`, and the single `skip_prerequisite_checks: false` line at `rust-cli/main.rs:498`.
- Focused checks: `cargo test --lib prerequisites orchestrate`; `cargo test --test orchestration_golden --test routing_golden --test rust_unit_scheduling --test rust_intent_commands`.
- Done when: IF-002, IF-003, IF-004 exist; all three `run_prerequisites` sites in `orchestrate_next` use `check_prerequisites`; the `needed` extraction is purely mechanical; a stale-load test asserts equality with `STATE_CHANGED`; no golden differs.

## WU-004 MCP protocol layer
- Stories: US-003, US-004
- Criteria: AC-003, AC-004, AC-005
- Depends on: WU-001
- Tests: TS-001, TS-002, TS-003
- Owned files: `rust-cli/mcp/mod.rs` (replaces the stub, CFG-005, CFG-006 line cap), `rust-cli/mcp/protocol.rs`.
- Focused checks: `cargo test --lib mcp::protocol`. Tests are written first, with a fake `ToolHost`, and run on every OS.
- Done when: both eras and all shared cases pass as exact `serde_json::Value` frames, including CRLF, oversized, invalid UTF-8, panic recovery, empty stdin; the modern field names were checked once against the saved 2026-07-28 schema.

## WU-006 Path redaction
- Stories: US-005, US-006
- Criteria: AC-006, AC-008, AC-011
- Depends on: WU-001
- Tests: TS-008
- Owned files: `rust-cli/mcp/redact.rs`.
- Focused checks: `cargo test --lib mcp::redact`. The pure `rewrite` runs with Windows-form strings on any OS.
- Done when: the longest-prefix and boundary rules pass (`/a/b` does not match `/a/bc`), macOS `/var` and `/private/var` are both covered, and `cfg!(windows)` appears only here.

## WU-003 Facade and CLI routing
- Stories: US-001, US-002, US-006, US-007, US-008
- Criteria: AC-001, AC-002, AC-008, AC-009, AC-010
- Depends on: WU-002
- Tests: TS-010, TS-011
- Owned files: `rust-cli/application.rs`, `rust-cli/main.rs`.
- Focused checks: `cargo test --lib application`; all golden suites; `cargo test --test rust_orchestrate_engine --test rust_orchestration_report_validation --test rust_orchestration_policy`.
- Done when: `application` implements IF-001 and FN-001..FN-006; `main.rs` routes status, queue, next and report through it with the exit code 3 mapping kept in `main.rs`; the Q18 guard (whitespace-trimmed, before identity lookup) is test-first; CLI output and goldens are unchanged.

## WU-005 MCP tool adapter
- Stories: US-003, US-004, US-005, US-006, US-007, US-008
- Criteria: AC-003, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011
- Depends on: WU-003, WU-004, WU-006
- Tests: TS-004, TS-018
- Owned files: `rust-cli/mcp/tools.rs`, plus the one-line wiring edit in `rust-cli/mcp/mod.rs` (the only edit allowed there after WU-004).
- Focused checks: `cargo test --lib mcp::tools`.
- Done when: four tools only (no doctor or config tool), validation runs before the project opens, error classification is by exact equality, redaction is applied to values and keys, the `report` description says self-asserted and not authentication, and the CLI stage and result enums were read once from the CLI parser.

## WU-007 Integration, parity, install tests and state write lock
- Stories: US-001, US-002, US-003, US-004, US-006, US-007, US-008, US-009
- Criteria: AC-001, AC-002, AC-003, AC-004, AC-005, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013
- Depends on: WU-003, WU-005, WU-006
- Tests: TS-005, TS-006, TS-007, TS-008, TS-009, TS-014, TS-020
- Owned files: `tests/mcp_stdio.rs`, `tests/mcp_parity.rs`, `tests/mcp_redaction.rs`, `tests/fixtures/mcp/*`, the MCP additions in `tests/rust_cargo_install.rs` (TS-009 two-binary install only; TS-016 lives solely in WU-001's `tests/mcp_packaging.rs`). Owner decision 2026-11-28 (ADR-015 superseded) adds the lock implementation (FN-022, IF-011): `rust-cli/orchestrate.rs` (`with_state_lock`, `with_state_lock_for`, two constants, wrap `persist_active` and `persist_new`, TS-020 tests in its test module) and `rust-cli/intent_commands.rs` (wrap the compare-then-write in `persist_workflow`, `pause`, `resume`, `abandon`). No other source file; no golden, `Cargo.toml` or `Cargo.lock` change.
- Focused checks: `cargo test --test mcp_stdio --test mcp_parity --test mcp_redaction --test rust_cargo_install` and `cargo test --lib ts020`; then the TS-011 gate (full suite, goldens untouched) because persist paths changed. Projects use the CFG-004 canonical base.
- Done when: the TS-006 queue case asserts `state.json` and `audit.jsonl` bytes are unchanged after `queue` (AC-007, read-only preview); child-process sessions per era are stdout-pure, CLI and MCP parity holds with no prerequisite executed, the interleaving test yields exactly one `state_conflict`, two-binary install and uninstall pass, no existing golden changed. Lock: TS-020 is written first and fails without the lock; `ts007_concurrent_reports_never_lose_an_update` is unchanged apart from one added `assert!(conflicts >= 1)` after its 4-round loop (8 MiB padding widens the race window; under the lock a conflict happens only when the second racer loads before the first one's save lands, likely but not guaranteed on a fast CI runner, so if it flakes raise the round count or padding rather than drop the assertion); the loser gets exactly `STATE_CHANGED` and no audit row; `.aidlc/state.lock` never exists after a command and appears in no golden; no new crate; CLI output without contention is byte-identical.

## WU-008 CI workflow
- Stories: US-001, US-009
- Criteria: AC-001, AC-003, AC-013
- Depends on: WU-007, WU-010
- Tests: TS-012, TS-015
- Owned files: `.github/workflows/ci.yml` (new), plus one text-check test added to `tests/repo_content.rs`.
- Focused checks: YAML parse (and `actionlint` when installed); `cargo test --test repo_content`.
- Done when: the workflow runs build, full test and the named `MCP transcript tests` step on windows-latest and macos-latest with the macOS TMPDIR and PATH setup; no allowlist or skip list exists, so the macOS leg is genuinely green once WU-010 is in; no push was made. TS-012 passes only after an owner-authorized CI run; build-and-verify treats TS-012 and TS-013 as deferred open items, not failures, and the release-candidate gate must list them as open.

## WU-009 README docs
- Stories: US-009
- Criteria: AC-014
- Depends on: WU-004, WU-005
- Tests: TS-013, TS-015
- Owned files: `README.md` only.
- Focused checks: `cargo test --test repo_content`.
- Done when: a labelled-proposed `## MCP server (proposed)` section covers install, Claude Code and DeepSeek Harness configuration, Windows and macOS notes, and the self-asserted approver and state_conflict notes; sh-block lines are `dtech-aidlc-mcp` or non-command lines; the old platform sentence (README line 191) is not changed in Develop; the owner updates it after TS-012 is green on both OSes. TS-013 is a manual release check.
