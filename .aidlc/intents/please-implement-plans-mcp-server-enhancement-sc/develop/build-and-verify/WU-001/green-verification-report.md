# Green verification report: WU-001

- Purpose: evidence that WU-001 is complete.
- Status: Complete (build-and-verify, 2026-11-27). Integrated main tree, base `919e0b9` (WU-010 already integrated).

## Results per test level (all Observed)
- Focused: `cargo test --test mcp_packaging` -> 5 passed, 0 failed.
- Behaviour checks: `cargo build` builds `target/debug/dtech-aidlc` and `target/debug/dtech-aidlc-mcp`; `cargo run -q --locked -- --version` prints `dtech-aidlc 1.2.2` (CLI stays the default run target); `dtech-aidlc-mcp </dev/null` exits 0 (stub).
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 30 targets, 461 passed, 0 failed (baseline after WU-010: 456 / 0; +5 new TS-016 tests).
- Format: `cargo fmt --check` clean. Clippy (`cargo clippy --all-targets`): only the known pre-existing warning in `tests/orchestration_golden.rs:1108`; nothing new.
- Cargo.lock unchanged (`git diff --stat Cargo.lock` empty; sha256 test passes).
- Independent review of the worker patch (opus): 6 issues, all addressed during integration (see implementation.md).
- Not run: Windows (WU-008 CI). An offline install of both binaries is TS-009 (WU-007).

## Quality requirements verified
- Dependency footprint (Operability): CLI and MCP binaries share exactly serde_json + sha2; no new crates.
- CLI compatibility: default run target and all existing goldens unchanged.

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-009 | AC-013 | FN-016 | `ts016_dependency_set_is_unchanged` (TS-016) | passed |
| US-009 | AC-013 | FN-016 | `ts016_rust_version_pinned` (TS-016) | passed (red before) |
| US-009 | AC-013 | FN-016 | `ts016_both_bins_declared` (TS-016) | passed (red before) |
| US-009 | AC-013 | FN-016 | `ts016_cli_stays_the_default_run_target` (TS-016) | passed |
| US-009 | AC-013 | FN-016 | `ts016_cargo_lock_bytes_unchanged` (TS-016) | passed |
| US-009 | AC-013 | FN-016 | full suite vs baseline (TS-017) | passed (461/0, no existing test changed state) |
