# Implementation: WU-002 (Prerequisite guard, error constant and pending preview)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-002, FN-007, FN-008, FN-019, FN-020, IF-002, IF-003, IF-004, TS-007, TS-010, TS-011.
- Kind: code manifest. 4 files, +414 / -73.

| File | Change |
|---|---|
| `rust-cli/orchestrate.rs` | `pub const STATE_CHANGED` (IF-002, top-level consts); `NextOptions.skip_prerequisite_checks` (default false; only the MCP facade will set it, Q5); the three `run_prerequisites` sites in `orchestrate_next` route through `check_prerequisites`, which spawns nothing under skip and returns fail-closed rows `{name, ok:false, hint, exitCode:null, executed:false}`; under skip no `prerequisite.checked` audit row is written at site 2 (held checks not pushed) or site 3 (Blocked); `needed_prerequisites` extracted from `orchestrate_next` (mechanical) and shared with the read-only `pub fn pending_prerequisites` (FN-020, AC-009 preview); tests. |
| `rust-cli/prerequisites.rs` | `check_prerequisites(root, config, names, skip)` wrapper over `run_prerequisites`; test. |
| `rust-cli/intent_commands.rs` | uses the shared `STATE_CHANGED` (identical bytes to the removed local `CHANGED`). |
| `rust-cli/main.rs` | one line: `skip_prerequisite_checks: false` (CLI behaviour unchanged). |

Integration notes: the worker's patch was fixed after independent review (site-2/site-3 audit rows under skip, two new tests, constant placement, tightened assertion); the fix was re-reviewed (ok). The CLI path is byte-identical: the non-skip branch keeps every message and audit row, and all goldens pass unchanged. No codebase-map change (no new component, entry point or test command).
