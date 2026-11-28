# Implementation: release-candidate summary (final sweep, no unit)

- Purpose: what the intent built, per work unit, for the release-candidate gate.
- Status: Complete for the gate, with open items listed in `known-limitations.md` (2026-11-28). Branch `feature/please-implement-plans-mcp-server-enhancement-sc`, HEAD `ca30a85`.
- Cites WU-001..WU-010, FN-001..FN-022, IF-001..IF-011, CFG-001..CFG-006, AC-001..AC-014, US-001..US-009.
- Kind: code manifest and summary. Per-unit detail is in `WU-xxx/implementation.md`.

## What was built, per unit (commit order on the branch)

| Unit | Commit | What it delivers | Files touched (code) |
|---|---|---|---|
| WU-010 Fix pre-existing Unix test bugs | `919e0b9` | Two baseline test failures fixed; test code only, no product change. Windows assertion kept under `#[cfg(windows)]`. | `rust-cli/onboarding/mod.rs` (+4), `rust-cli/onboarding/projection.rs` (-1) |
| WU-001 Scaffold and packaging | `9697529` | Second binary `dtech-aidlc-mcp` (CFG-001), `rust-version = "1.100"` (CFG-002), `default-run = "dtech-aidlc"`, `application` and `mcp` modules registered, no new crates. | `Cargo.toml`, `rust-cli/lib.rs`, `rust-cli/application.rs`, `rust-cli/mcp/mod.rs`, `rust-cli/mcp_main.rs`, `tests/mcp_packaging.rs` |
| WU-002 Prerequisite guard, error constant, pending preview | `4bc3ec2` | `STATE_CHANGED` shared constant (IF-002); `check_prerequisites` with a skip mode that spawns nothing and fails closed (IF-003); read-only `pending_prerequisites` (IF-004). CLI path byte-identical. | `rust-cli/orchestrate.rs`, `rust-cli/prerequisites.rs`, `rust-cli/intent_commands.rs`, `rust-cli/main.rs` (1 line) |
| WU-004 MCP protocol layer | `6b60591` | Hand-rolled dual-era JSON-RPC 2.0 over stdio (serde_json and std only): bounded 1 MiB framing, era detection, `server/discover`, panic catch, single stdout writer (IF-005). | `rust-cli/mcp/protocol.rs` (new, 711 lines incl. tests), `rust-cli/mcp/mod.rs` |
| WU-006 Path redaction | `a986c73` | `Redactor` (IF-007): project root, `DTECH_AIDLC_HOME` and `HOME` prefixes, macOS `/var` and `/private/var` twins, Windows forms; unregistered absolute paths never survive in messages. | `rust-cli/mcp/redact.rs` (new), `rust-cli/mcp/mod.rs` (1 line) |
| WU-003 Facade and CLI routing | `e720b5f` | `application` facade (IF-001): `Caller {Cli, Mcp}`, `open`, `status`, `queue`, `next`, `report`, `external_engine`. MCP `next` cannot create an intent or run shell checks; MCP `report` refuses an approval without an explicit approver label. CLI `status`, `queue`, `next`, `report` route through it; exit code 3 stays in `main.rs`. | `rust-cli/application.rs`, `rust-cli/main.rs` |
| WU-005 MCP tool adapter | `bbd9635` | Exactly four tools: `status`, `queue`, `next`, `report` (no doctor or config tool). Schema-strict arguments validated before the project opens; every value, key and error redacted; errors classified by exact equality (`state_conflict` retryable, `approver_required`, `invalid_arguments`, `operation_failed`); a Blocked `next` is a result, not an error. Startup wiring and `DTECH_AIDLC_HOME` stderr warning. | `rust-cli/mcp/tools.rs` (new), `rust-cli/mcp/mod.rs`, `rust-cli/prerequisites.rs` (test-only flake fix) |
| WU-009 README docs | `b2c4166` | `## MCP server (proposed)` section: install, Claude Code and DeepSeek Harness configuration, troubleshooting, Windows and macOS notes; states what was and was not verified. | `README.md` (+66) |
| WU-007 Integration, parity, install tests and state write lock | `bab236e` | State write lock (FN-022, IF-011): `with_state_lock` around the check-and-write at six sites, ephemeral `.aidlc/state.lock`, 5 s bounded wait to `STATE_CHANGED`. Integration suites: child-process sessions, CLI-versus-MCP parity, redaction end to end, offline two-binary install. This unit was reworked after the owner reopened the earlier concurrency decision on 2026-11-28. | `rust-cli/orchestrate.rs`, `rust-cli/intent_commands.rs`, `tests/mcp_stdio.rs`, `tests/mcp_parity.rs`, `tests/mcp_redaction.rs`, `tests/rust_cargo_install.rs`, `tests/fixtures/mcp/*` |
| WU-008 CI workflow | `ca30a85` | `.github/workflows/ci.yml` (CFG-003, IF-010): `windows-latest` and `macos-latest` required, `ubuntu-latest` best effort; locked build and test, "Installer tests must not skip", "MCP transcript tests". | `.github/workflows/ci.yml` (new), `tests/repo_content.rs` |

Every unit also updated its `build-and-verify` and `red-tests` artifacts, and WU-001, 003, 004, 005, 007, 008 updated `.aidlc/knowledge/_codebase-map.md`.

## Whole-intent footprint (Observed)
- No new crate: `git diff 69cab5a..HEAD -- Cargo.lock tests/golden` is empty. Dependencies remain exactly `serde_json` and `sha2`.
- No golden file changed. Version stays 1.2.2; no CHANGELOG entry (release work is intent 2).
- CLI behaviour unchanged without contention (all golden suites pass unchanged). With contention a losing writer now gets `STATE_CHANGED` instead of silently losing its update (FN-022).

## Deployment and install notes
- Observed: `cargo install --path . --locked` installs both `dtech-aidlc` and `dtech-aidlc-mcp`; one `cargo uninstall dtech-aidlc` removes both (offline, `--locked`, covered by `ts009_cargo_install_and_uninstall_both_binaries`, passed on macOS and Windows CI).
- Observed: the prebuilt-binary installers in `scripts/rust-install/` stay CLI-only (owner decision); they were not changed.
- Observed: the server is launched by the host as a stdio child: `dtech-aidlc-mcp --project <dir>` (cwd if omitted). Root is fixed at launch and never taken from a call. Diagnostics go to stderr only.
- Observed: the install writes nothing to any host configuration; hosts are configured manually from the README.
- Assumption: nothing is deployed or published by this intent. No PR was opened and nothing merged; the owner authorised only the push that produced CI run 46090623346.

## Rollback note
- Each unit is one commit, plus `0487f49` (the WU-007 redesign record, artifacts only); revert in reverse order (WU-008, WU-007, WU-009, WU-005, WU-003, WU-006, WU-004, WU-002, WU-001, WU-010), or revert the range `d765edd..ca30a85` (equivalently `919e0b9^..ca30a85`; a git A..B range excludes A, so `919e0b9..` would leave WU-010 in place) to return to the pre-intent product code (the `.aidlc` artifacts would revert with it).
- Cheapest partial rollbacks (Observed from the unit notes): delete `.github/workflows/ci.yml` and the `ts015` test (WU-008); revert WU-007 to remove the state lock and the integration tests; revert WU-009 for the README only.
- Users who installed the server: `cargo uninstall dtech-aidlc` removes both binaries; no host configuration or user-level setting was written by the install.
