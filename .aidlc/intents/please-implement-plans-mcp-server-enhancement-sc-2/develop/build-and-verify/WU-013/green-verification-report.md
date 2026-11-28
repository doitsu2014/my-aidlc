# Green verification: WU-013

- Status: Complete (2026-11-28). Cites TS-021..TS-025.
- Observed (fix worktree): `cargo test --locked --lib telemetry::` 21 passed; `--test telemetry_guards` 3 passed; routing_golden bless run 11 passed with only the two `/telemetry/` lines changed; `cargo fmt --check` clean; Cargo.lock unchanged.
- Observed (independent re-review on a clean export of 695d62f + patch): 570 passed, 0 failed; each fix confirmed against its own mutant.
- Observed (conductor, main tree at 695d62f + patch): `cargo fmt --check` clean, full suite 570 passed / 0 failed; `git diff --stat -- tests/golden` = 2 files, 1 line each.
- Measurements (Observed, release build, temporary harness, not committed): queue headroom - no drop at 1k to 200k events/s sustained for 2 s (ceiling not reached; far above the ADR estimate). Rotation - 200 MiB written with 5 MiB parts: peak on disk 54.96 MiB, final 50.01 MiB, i.e. the 50 MiB cap plus up to one open part, matching the ADR-020 known ceiling; no reopen trigger fires.
- Trace: US-001 AC-002/AC-003/AC-005 FN-023 TS-021 PASS; US-003 AC-010 FN-024/FN-026 TS-022/TS-023 PASS; US-003 AC-012 FN-025/FN-027 TS-024 PASS; US-003 AC-028 TS-023 PASS; US-004 AC-006 (Q1) TS-023 PASS; US-004 AC-008/AC-009 TS-024 PASS; AC-027 TS-025 PASS.
