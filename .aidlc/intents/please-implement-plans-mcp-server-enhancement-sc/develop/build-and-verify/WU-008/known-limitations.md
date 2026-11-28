# Known limitations: WU-008

- The workflow has never run on GitHub (TS-012 open). The first run is the Windows baseline, including Windows goldens, the MCP tests and the state lock (LockFileEx, delete-pending).
- "Installer tests must not skip" relies on the runners having both `sh` and `pwsh` (GitHub's Windows, macOS and Ubuntu images do). It would fail on a developer machine without pwsh; it is a CI-only step.
- `rust_cargo_install` runs `cargo install --offline --locked`; the workflow relies on the preceding `cargo build --locked` to warm the registry (no explicit `cargo fetch`). Not yet proven on a clean runner.
- Pre-existing timing-sensitive tests could fail a CI leg under load: `prerequisites::tests::check_timeout_is_reported` (`sleep 1` vs a 1 s timeout), `artifacts::tests::verifies_empty_missing_and_not_applicable_reason`, `onboarding::projection::tests::keeps_user_edits_unless_force_then_backups`. Not changed (not this unit's files); a follow-up should give them margin.
- No actionlint run (not installed); the YAML was parsed with Ruby and read by the reviewer.
- Rollback: delete `.github/workflows/ci.yml` and the ts015 test.
