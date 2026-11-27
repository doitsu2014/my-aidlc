# Green verification report: WU-004

- Status: Complete (build-and-verify, 2026-11-28). Integrated main tree, base `4bc3ec2`.

## Results per test level (Observed)
- Focused: `cargo test --lib mcp` -> 16 passed, 0 failed.
- Process-level smoke (binary, real stdio): `printf <initialize 2025-11-25> <tools/list> <modern server/discover with _meta> 'not json' | target/debug/dtech-aidlc-mcp --project $PWD` -> legacy initialize result with serverInfo; `{"tools":[]}`; DiscoverResult `{supportedVersions:["2026-07-28"], capabilities:{tools:{}}, ttlMs:0, cacheScope:"private", resultType:"complete", _meta:{serverInfo}}`; -32700 Parse error; exit 0. Every stdout line is one JSON-RPC frame.
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 30 targets, 483 passed, 0 failed (baseline 467/0; +16).
- `cargo fmt --check` clean; clippy: only the known pre-existing warning. `grep print!/println!` in `rust-cli/mcp`: none; one `eprintln!` (stderr) for startup errors.
- Independent review: 1 medium (ownership of the startup warning, re-assigned), 2 low, 1 nit; fixed with red-first tests; re-review ok.
- Not run: real host launch (TS-013, manual) and child-process transcript tests (WU-007); Windows (WU-008 CI).

## Quality requirements verified
- Stdout purity (AC-004): only protocol frames on stdout (unit tests and smoke run).
- Input bounds (AC-005, TH-07): 1 MiB line cap including CRLF and EOF boundaries; oversized input drained without ending the session.
- Robustness: panic recovery without detail; invalid UTF-8 survives.

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-003 | AC-003 | FN-011 | `ts001_legacy_session_exact_frames`, `ts001_initialize_version_negotiation`, `ts001_before_initialize_without_key` (TS-001) | passed |
| US-003 | AC-003 | FN-011 | `ts002_modern_exact_frames`, `ts002_modern_meta_validation`, `ts002_era_mixing_in_one_process` (TS-002) | passed |
| US-003 | AC-003 | FN-016 | `mcp::tests::parse_args_accepts_only_project`, `launch_root_rejects_missing_and_file_roots` | passed |
| US-004 | AC-004 | FN-010, FN-011, FN-012 | `ts003_envelope_errors_and_notifications`, `ts003_parse_errors_crlf_blank_lines_and_eof`, `ts003_invalid_utf8_keeps_session_alive`, `ts003_panic_is_recovered_without_detail`, `ts003_empty_stdin_exits_zero_with_no_output`, `ts003_notifications_are_never_answered` (TS-003) | passed |
| US-004 | AC-005 | FN-010 | `ts003_oversized_line_is_drained_and_session_continues`, `ts003_cap_boundaries` (TS-003) | passed |
