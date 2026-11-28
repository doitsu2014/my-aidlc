# Implementation: WU-007 (Integration, parity, install tests and state write lock)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-007, FN-022, IF-011, TS-005..TS-009, TS-014, TS-020; ADR-015 (as superseded by the owner on 2026-11-28).
- Kind: code manifest.

## State write lock (FN-022, IF-011)
| File | Change |
|---|---|
| `rust-cli/orchestrate.rs` | `with_state_lock(root, f)`: after `assert_safe_state_dir`, runs `f` (the compare-then-write) while holding an exclusive `File::try_lock` on an ephemeral `.aidlc/state.lock` (poll 10 ms, cap 5 s -> `STATE_CHANGED`, i.e. the retryable `state_conflict` over MCP). On release the holder removes the file and, only if removal succeeded, writes a one-byte retired marker through its handle; a waiter that locks a marked file reopens (up to 3 times, then a non-retryable "state.lock is stale; delete .aidlc/state.lock ..." error). `PermissionDenied` on open is retried within the budget (Windows delete-pending). No `.aidlc` -> runs unlocked and creates nothing. Wraps `persist_active` and `persist_new` (covers next, report, park, revisit, gates and unit commands). 8 `ts020_` tests. |
| `rust-cli/intent_commands.rs` | `persist_workflow`, `pause`, `resume`, `abandon` run their check-and-write under `with_state_lock`; `preflight_audit` keeps its place inside the closure so error precedence is byte-identical. Audit rows are appended after the lock, only for writes that landed. |

## Integration tests (tests only)
| File | Tests |
|---|---|
| `tests/mcp_stdio.rs` | TS-005 child-process sessions per era (legacy, modern, mixed), stdout purity, CRLF, startup failure, root fixed at launch; TS-014 hostile protocol input table, hostile tool arguments rejected before the project opens, boundary values (exact 1 MiB line, approved_by 200, approval_note 4096) |
| `tests/mcp_parity.rs` | TS-006 CLI vs MCP parity (lifecycle state and audit, status, queue read-only with state.json and audit.jsonl bytes unchanged, no prerequisite command over MCP, pendingPrerequisites, no intent creation, next options cannot reach the workflow); TS-007 `ts007_concurrent_reports_never_lose_an_update` (no lost update, loser gets retryable state_conflict, `conflicts >= 1` asserted) |
| `tests/mcp_redaction.rs` | TS-008 end-to-end redaction (default temp base incl. macOS /private/var twin, next/report results project-relative, error messages carry no path, unusable DTECH_AIDLC_HOME warns on stderr without the path) |
| `tests/rust_cargo_install.rs` | TS-009 offline `--locked` install of both binaries, installed CLI `--version`/`--help`, installed MCP server answers over stdio, one `cargo uninstall dtech-aidlc` removes both |
| `tests/fixtures/mcp/*` | shared helpers (`common.rs`, Windows-safe path normalisation) and request-only session fixtures |

Integration (conductor): a `ponytail:` comment records the reopen-counter ceiling (a waiter behind 4+ successive retiring holders gets the stale error). Codebase map updated (lock, new test targets).
