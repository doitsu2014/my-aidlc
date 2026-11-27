# Implementation plan

Direction: Option A. A thin `application` facade in the library, a second `[[bin]]` `dtech-aidlc-mcp` in the same package, hand-rolled dual-era JSON-RPC over stdio, zero new crates, `rust-version = "1.100"`. Post-gate amendment: Windows and macOS are supported and CI-checked; Linux is best-effort. Design only in this stage; no source was changed.

## Environment and baseline steps (before any edit)
1. `cargo build && mkdir -p /private/tmp/aidlc-test && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast`; save the pass/fail names list (TS-017). Expected before WU-010: 454 passed, 2 failed (the two known lib tests). After WU-010 the macOS baseline is 0 lib failures (456 passed, 0 failed with the same environment).
2. Check `git status tests/golden` is clean; keep it clean after every unit (TS-011).

## Ordered tasks
1. WU-001 Scaffold: second bin, rust-version, `pub mod application; pub mod mcp;`, stub `run`, TS-016 test. Cargo.lock unchanged.
2. Wave 1 in parallel: WU-002 (guard, `STATE_CHANGED`, `pending_prerequisites`), WU-004 (protocol), WU-006 (redaction), WU-010 (test-only fix of two Unix lib tests, independent), WU-009 draft.
3. WU-003 facade and CLI routing, after WU-002.
4. WU-005 tool adapter, after WU-003, WU-004, WU-006.
5. WU-007 integration, parity and install tests.
6. WU-008 CI workflow (after WU-007 and WU-010). WU-009 finalised after WU-005.

## File-level change list
| File | Change | Unit |
| Cargo.toml | second `[[bin]]`, `rust-version = "1.100"` | WU-001 |
| rust-cli/lib.rs | two `pub mod` lines | WU-001 |
| rust-cli/mcp_main.rs | new, at most 40 lines | WU-001 |
| rust-cli/orchestrate.rs | `STATE_CHANGED` const, `skip_prerequisite_checks`, three call-site swaps, `needed_prerequisites` extraction, `pending_prerequisites`, test literals updated | WU-002 |
| rust-cli/prerequisites.rs | `check_prerequisites` | WU-002 |
| rust-cli/intent_commands.rs | use `orchestrate::STATE_CHANGED` | WU-002 |
| rust-cli/main.rs | line 498 flag (WU-002); routing through `application` (WU-003) | WU-002, WU-003 |
| rust-cli/application.rs | new facade | WU-003 |
| rust-cli/mcp/mod.rs | `run`; one wiring line later | WU-004, WU-005 |
| rust-cli/mcp/protocol.rs | new | WU-004 |
| rust-cli/mcp/tools.rs | new | WU-005 |
| rust-cli/mcp/redact.rs | new | WU-006 |
| tests/mcp_packaging.rs, mcp_stdio.rs, mcp_parity.rs, mcp_redaction.rs, tests/fixtures/mcp/*, tests/rust_cargo_install.rs | new or extended | WU-001, WU-007 |
| .github/workflows/ci.yml, tests/repo_content.rs | new workflow; one text check | WU-008 |
| rust-cli/onboarding/mod.rs, rust-cli/onboarding/projection.rs | `#[cfg(test)]` modules only: two pre-existing Unix test bugs fixed, no product code | WU-010 |
| README.md | proposed MCP section (WU-009); line 191 platform sentence is not edited in Develop | WU-009 |

Untouched: CHANGELOG, version, PROJECT.md, help.txt, completion.zsh, COMMAND_KEYS, args.rs, scripts/rust-install/*, `.github/badges`, every existing golden.

## How CLI parity is protected
- The facade extraction is mechanical and keeps the CLI order: resolve, assert safe state dir, external engine, read graph, engine, then validate_model_config, orchestrate_next, finish_directive. Exit code 3 and printing stay in `main.rs`.
- The prerequisite guard defaults to off, so the CLI path delegates untouched.
- The existing suites (status_golden, cli_golden, routing_golden, orchestration_golden, rust_unit_scheduling and others) run unchanged after WU-002 and WU-003; any diff blocks the unit. No version bump (it would force re-blessing 8 goldens).
- doctor and config prerequisite checks (`doctor.rs`, `config_commands.rs`) are not exposed over MCP; TS-018 asserts no such tool.

## Gate handling and open items
- WU-008 and WU-009 cannot reach done inside Develop: TS-012 needs an owner-authorized push and TS-013 is manual. Build-and-verify treats TS-012 and TS-013 as deferred, not passed: they are recorded as open items with the reason, and do not block build-and-verify when all other tests pass. The release-candidate gate must list them as open and may not claim Windows or macOS verified until they pass.
- macOS baseline policy (owner decision, 2026-11-27): WU-010 fixes the two pre-existing Unix lib test bugs (tests only), superseding N5 for these two tests only (goldens stay un-blessed). There is no allowlist file and no skip list; WU-008 depends on WU-010 and the macOS CI leg must be genuinely green.
- README line 191 ("macOS and Linux builds have not been verified yet"): the owner (or a one-line follow-up edit to WU-009's README after TS-012 is green on both OSes, in the release-candidate step) updates it. It is unchanged in Develop. The Platforms badge is untouched.
- Clippy is not run in CI (the workflow contract IF-010 has no clippy step) because `-D warnings` already fails at `tests/orchestration_golden.rs:1108`.

## Risks
- Highest: the `needed` extraction and three call-site swaps in `orchestrate.rs` (golden risk). Keep mechanical; review line by line.
- `pending_prerequisites` is new, unspiked, and a preview that can disagree with `next`.
- Windows is unmeasured locally; first CI run is the baseline. Rename over a held `state.json` can surface as non-retryable `operation_failed` (documented in the README).
- macOS is green only after WU-010; if WU-010 is skipped the CI macOS leg fails on the two known lib tests (no allowlist exists).
- Estimates (1 MiB line cap, field caps, `result` enum) to confirm in WU-004 and WU-005.
- Ten units exceed the split threshold of 6. Owner accepted this on 2026-11-27: all nine existing units are kept, plus WU-010. No recombination.

## Rollback
All work is additive except `main.rs` routing, `orchestrate.rs` and `prerequisites.rs`. Roll back per unit by reverting its commit; removing WU-003 restores the old `main.rs` inline path, and removing the second `[[bin]]` returns the package to CLI only. The Cargo.lock is unchanged, so no dependency cleanup is needed.

## Out of scope (intent 2 and others)
Telemetry events, workflow event instrumentation, monitor dashboard, hardening and release work (US-010..US-013); any CLI `mcp` subcommand or help change; install scripts for the MCP binary (N2); version bump and CHANGELOG; the clippy warning (separate owner decision); the two known lib failures are fixed by WU-010, not out of scope; pushing to run CI (needs owner authorization); real authentication of the approver label.
