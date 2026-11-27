# Red evidence: WU-001

- Purpose: the observed behavioural failure before implementation.
- Status: Complete (red-tests step, 2026-11-27). Cites TS-016, AC-013, FN-016.
- Test-first: yes (packaging is a public contract: the second binary, the pinned MSRV and the unchanged dependency set, test-strategy.md).

## Observed red run (isolated worktree at d765edd, test written first, manifest unmodified)
- Command: `cargo test --test mcp_packaging`
- Result: `test result: FAILED. 2 passed; 2 failed`
  - `ts016_rust_version_pinned`: assertion failed, `rust-version = "1.100"` absent from Cargo.toml.
  - `ts016_both_bins_declared`: assertion failed, no `dtech-aidlc-mcp` bin declared.
  - `ts016_dependency_set_is_unchanged` and `ts016_cargo_lock_bytes_unchanged` passed, as intended (they guard properties that must not change).
- These are assertion failures in executing tests, not compile or setup errors.

## Baseline (TS-017)
- Captured before the first edit: 454 passed / 2 failed, both failures owned by WU-010 (`develop/baseline.md`). WU-010 has since fixed them (commit 919e0b9), so the expected baseline for WU-001's full run is 0 failures.
