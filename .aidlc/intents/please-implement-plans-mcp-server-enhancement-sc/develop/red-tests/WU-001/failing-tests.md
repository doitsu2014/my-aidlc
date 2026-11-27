# Failing tests: WU-001 (Scaffold and packaging)

- Purpose: the tests written before WU-001's implementation.
- Status: Complete (red-tests step, 2026-11-27). Cites TS-016, TS-017, AC-013, US-009, FN-016.
- Kind: code manifest.

| Test | File | Test id | Red before implementation (Observed) |
|---|---|---|---|
| `ts016_rust_version_pinned` | `tests/mcp_packaging.rs` (new) | TS-016 | FAILED: `assertion failed: manifest().contains("rust-version = \"1.100\"")` |
| `ts016_both_bins_declared` | `tests/mcp_packaging.rs` | TS-016 | FAILED: `assertion failed: m.contains("name = \"dtech-aidlc-mcp\"")` |
| `ts016_dependency_set_is_unchanged` | `tests/mcp_packaging.rs` | TS-016 | passed (guard: must stay green; dependency set is serde_json + sha2) |
| `ts016_cargo_lock_bytes_unchanged` | `tests/mcp_packaging.rs` | TS-016 | passed (guard: Cargo.lock sha256 pinned at the base value) |
| TS-017 baseline gate | full existing suite | TS-017 | baseline captured before the first edit (`develop/baseline.md`) |

Added during integration (after review, see build-and-verify): `ts016_cli_stays_the_default_run_target`, and stronger assertions in `ts016_both_bins_declared`.
