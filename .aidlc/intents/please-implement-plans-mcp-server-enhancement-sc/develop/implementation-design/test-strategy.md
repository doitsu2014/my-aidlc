# Test strategy

Framework: Rust built-in `cargo test` (lib unit tests, integration tests under `tests/`, goldens under `tests/golden/<test-binary>/` via `tests/common/golden.rs`). No new crates.

## Levels
### Unit
- Applies: yes
- Why: protocol framing and era detection (FN-010..FN-012), validators, redaction (FN-015, pure `rewrite`), prerequisite guard and facade rules are pure or in-process testable with a fake `ToolHost` and `serve<R, W>`.
### Integration
- Applies: yes
- Why: CLI versus MCP parity on identical projects (TS-006), state conflict interleaving (TS-007), library guards at the three `orchestrate_next` sites (TS-010), offline two-binary install (TS-009).
### End to end
- Applies: yes
- Why: child-process sessions of `dtech-aidlc-mcp` per era via `CARGO_BIN_EXE_dtech-aidlc-mcp` (TS-005, TS-014); CI on windows-latest and macos-latest (TS-012); manual host launches (TS-013).
### Performance
- Applies: no
- Why: no throughput requirement in quality-requirements; the only limits are size caps, tested as hostile input (TS-014). Whole-suite wall time is watched but not gated.
### Security
- Applies: yes
- Why: threat model drives test-first items: no prerequisite execution (TH-13), approver refused before identity lookup (TH-02), path leakage (TH-11, TH-12), stdout purity (TH-14), size and UTF-8 limits (TH-07..TH-09), panic recovery (TH-21), zero new crates (TH-18).

## Criterion to level
| Test | Criterion | Function | Level |
| TS-001 | AC-003, AC-004 | FN-011 | unit |
| TS-002 | AC-003 | FN-011 | unit |
| TS-003 | AC-004, AC-011 | FN-010, FN-011, FN-012 | unit |
| TS-004 | AC-005, AC-010, AC-011 | FN-013, FN-014 | unit |
| TS-005 | AC-003, AC-004, AC-012 | FN-012, FN-016, FN-017 | end to end |
| TS-006 | AC-002, AC-007, AC-008, AC-009, AC-010 | FN-005, FN-006, FN-014 | integration |
| TS-007 | AC-012 | FN-008, FN-019 | integration |
| TS-008 | AC-006, AC-008, AC-011 | FN-015 | unit and integration |
| TS-009 | AC-013 | FN-016 | integration |
| TS-010 | AC-008, AC-009, AC-010 | FN-005, FN-006, FN-007, FN-008, FN-020 | unit |
| TS-011 | AC-001 | FN-005, FN-009 | regression gate |
| TS-012 | AC-001, AC-003 | FN-018 | end to end (CI) |
| TS-013 | AC-003, AC-014 | FN-016, FN-021 | manual |
| TS-014 | AC-004, AC-005 | FN-010, FN-013 | end to end |
| TS-015 | AC-001, AC-013, AC-014 | FN-018, FN-021 | integration (text checks) |
| TS-016 | AC-013 | FN-016 | integration |
| TS-017 | AC-001 | FN-009 | regression gate |
| TS-018 | AC-003, AC-008 | FN-013 | unit |
| TS-019 | AC-001 | none (existing code, test-only fix) | unit |

Threat coverage: TH-01/02 -> TS-006, TS-010; TH-06/07 -> TS-004, TS-014; TH-08/09 -> TS-003, TS-014; TH-11/12 -> TS-008; TH-13 -> TS-006, TS-010; TH-14 -> TS-005; TH-15 -> TS-007; TH-18 -> TS-016; TH-19/21/22 -> TS-001..TS-003.

## Work unit to test mapping
| Work unit | Tests |
| WU-001 | TS-016, TS-017 |
| WU-010 | TS-019 |
| WU-002 | TS-007, TS-010, TS-011 |
| WU-003 | TS-010, TS-011 |
| WU-004 | TS-001, TS-002, TS-003 |
| WU-005 | TS-004, TS-018 |
| WU-006 | TS-008 |
| WU-007 | TS-005, TS-006, TS-007, TS-008, TS-009, TS-014 |
| WU-008 | TS-012, TS-015 |
| WU-009 | TS-013, TS-015 |

Test names start with their id (for example `ts010_skip_runs_no_command`) so the CLI can attribute failures to a unit.

## Test-first versus alongside
| Unit | Approach | Reason |
| WU-001 | alongside | TS-016 needs the bin to exist; TS-017 baseline is captured before any edit |
| WU-010 | alongside | existing tests corrected; confirm each fails before and passes after on macOS |
| WU-002 | test-first for TS-010 skip and STATE_CHANGED equality | security guard, three sites, golden risk |
| WU-003 | test-first for the Q18 approver guard and option reset; rest alongside | TH-02, public contract; plumbing follows existing goldens |
| WU-004 | test-first | the transcripts are the public wire contract |
| WU-005 | test-first for validation-before-access and error classification | TH-06, TH-07 |
| WU-006 | test-first | pure function with explicit cases, Windows forms cannot be run for real on macOS |
| WU-007 | alongside | tests are the unit's deliverable |
| WU-008 | alongside | only verifiable by a real run (TS-012) |
| WU-009 | alongside | text with repo_content checks |

## Transcript tests
- Frames are exact `serde_json::Value` equality, each reviewed once against the saved 2025-11-25 and 2026-07-28 schemas. Fixtures are JSONL under `tests/fixtures/mcp/` (new files only, autocrlf-safe, no existing golden touched). The server writes `\n` only and accepts `\n` or `\r\n`.
- In-process (WU-004) via `serve` with a fake host; child-process (WU-007) via `CARGO_BIN_EXE_dtech-aidlc-mcp` on every OS with no skips on Windows.

## CI on windows-latest and macos-latest
- `.github/workflows/ci.yml` (CFG-003, IF-010): build, full test with `--no-fail-fast`, then a named `MCP transcript tests` step. macOS sets `TMPDIR=/private/tmp/aidlc-test` and `target/debug` on PATH; Windows adds `target\debug` to PATH. Linux is an optional `continue-on-error` leg.
- macOS must be genuinely green: no allowlist, skip list or `continue-on-error` on the macOS leg. This depends on WU-010 (TS-019).
- A step fails if rust_cargo_install output contains `skipped: pwsh unavailable` or `skipped: POSIX shell unavailable`, so skips are visible.
- Pushing to run it needs separate owner authorization; TS-012 is open until then. The first run is the Windows baseline measurement.

## Known baseline failures handling
- macOS baseline before WU-010: 454 passed, 2 failed (`onboarding::tests::rejects_traversal_and_absolute_paths`, `onboarding::projection::tests::refuses_symlink_targets`), both Unix-only test bugs, not caused by this intent.
- Owner decision (2026-11-27): WU-010 fixes both tests (tests only). This supersedes N5 for these two tests only; goldens are still not re-blessed. After WU-010 the baseline is 0 lib failures. No allowlist file, no skip by name, no blanket `continue-on-error`.
- TS-019: after WU-010, `cargo test --lib onboarding` passes both tests on Unix; the `C:/escape` `is_err()` assertion remains under `#[cfg(windows)]` (Windows is verified only by CI). Before WU-010 the two names are the only permitted failures in the TS-017 comparison; after it any failure is a regression.
- clippy `-D warnings` already fails at `tests/orchestration_golden.rs:1108` (needless_borrow); CI has no clippy step (IF-010).

## regression.json conventions
- Record the TS-017 baseline as a list of test names with pass or fail, taken before WU-001. After each unit, compare the failing set to it. A failure owned by an unfinished unit (via `tests` in work-units.json) is deferred to that unit; any failure of a pre-existing test outside the two baseline names (until WU-010 is done, after which none are permitted) is a regression and blocks the unit. Do not edit `tests/golden/` to make a diff go away.

## Commands
- Build: `cargo build --locked`
- Format: `cargo fmt --check`
- Test: `cargo test --locked --no-fail-fast`
- MCP transcript tests: `cargo test --locked --test mcp_stdio --test mcp_parity --test mcp_redaction`
- Baseline (whole existing suite): `cargo build && mkdir -p /private/tmp/aidlc-test && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast`

## Environment prerequisites
- rust 1.100 or newer — `rustc --version` — `rustup toolchain install 1.100`
- `dtech-aidlc` on PATH from `target/debug` — `command -v dtech-aidlc` — add `$PWD/target/debug` to PATH after building (needed by doctor and routing goldens)
- Non-symlinked TMPDIR on macOS — `echo $TMPDIR` — `mkdir -p /private/tmp/aidlc-test` and export it
- pwsh and a POSIX shell for the install tests — `pwsh -v`, `sh -c true` — present on the CI images (Assumed); local absence is a silent skip

## Environments and data
- Fixtures are temp projects created per test under a canonical, symlink-free base (CFG-004) with the Windows `\\?\` prefix stripped. The one exception is the redaction case that deliberately uses the default macOS `/var/folders` base for status and queue only.
- Prerequisite marker checks use a platform-neutral check command (`cmd /c` on Windows, `sh` elsewhere).

## Unverified
- Windows behaviour (verbatim prefix, case-insensitivity, USERPROFILE, CRLF, rename over an open file) has no local run; covered by OS-independent pure tests and, for real, only by the first CI run and TS-013.
- pwsh and Git Bash availability on runner images is from general knowledge.
- Host launch of `dtech-aidlc-mcp.exe` by Claude Code and DeepSeek Harness on Windows is Assumed; DeepSeek Harness launch-and-list has not been run.
- The 1 MiB line cap and the field caps are Estimates to confirm in WU-004 and WU-005.
