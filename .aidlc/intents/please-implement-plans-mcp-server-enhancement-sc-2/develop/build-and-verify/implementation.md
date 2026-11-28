# Implementation: release candidate 1.3.0 (intent 2, final sweep)

- Status: Complete for review (build-and-verify, no unit, 2026-11-28). Release-candidate gate in AI mode: this is not an owner approval.
- Scope: telemetry for the MCP server, the read-only `dtech-aidlc monitor` dashboard, installers for both binaries, version 1.3.0 prepared only. Nothing is tagged, published or merged.
- Integrated tree (Observed): HEAD 483e65f on `feature/please-implement-plans-mcp-server-enhancement-sc`; 16 commits since intent-1 close 327b02a; Cargo.lock differs only on the `dtech-aidlc` version line.
- Cites: FN-023..FN-044, IF-012..IF-020, TS-021..TS-037, CFG-007..CFG-013, ADR-019..ADR-039, AC-001..AC-030.

## What was built, per unit

| Unit | Commit | What | Main files |
|---|---|---|---|
| WU-011 | c23b4a8 | Flaky-test fixes (FN-038, TS-032): unique temp dir names, longer sleep against the 1 s timeout. Test modules only. | `rust-cli/artifacts.rs`, `rust-cli/onboarding/projection.rs`, `rust-cli/prerequisites.rs` (test code) |
| WU-012 | 695d62f | `deps add` under the state lock (FN-037, TS-031). | `rust-cli/dependencies.rs`, `tests/deps_lock.rs` |
| WU-013 | 2f9a33e | Telemetry core (FN-023..FN-028, IF-012..IF-014): schema, `os_random` (BCryptGenRandom on Windows), `Sink` and `FileStore`, retention, managed `/telemetry/` ignore line. | `rust-cli/telemetry/mod.rs`, `rust-cli/telemetry/sink.rs`, `rust-cli/knowledge.rs`, `rust-cli/lib.rs`, `tests/telemetry_guards.rs`, 2 lifecycle goldens |
| WU-020 | cba3669 | Installers for both binaries (FN-039, IF-020, TS-034); marker `dtech-aidlc rust installer v2`. | `scripts/rust-install/install.sh`, `install.ps1`, `tests/rust_cargo_install.rs`, README sentence |
| WU-016 | db30d93 | Dashboard design pack, AI design review (accepted), Studio mock renamed "dtech-aidlc MCP Studio", no remote fonts (FN-042). | `develop/build-and-verify/WU-016/`, `plans/mcp-server-enhancement/mcp-studio-server.html` |
| WU-014 | 164f09c | MCP telemetry wiring (FN-029..FN-031, IF-015, IF-016): `--telemetry on\|off`, `on_error`, `traced_call`. | `rust-cli/mcp/{mod,protocol,tools}.rs`, `tests/mcp_telemetry.rs`, MCP test fixtures |
| WU-017 | d3f0312 | Query engine (FN-032, IF-017, CFG-011): strict params, cursor, caps, summary, parse cache. | `rust-cli/telemetry/query.rs` |
| WU-015 | ddf97ca | Characterisation tests for intent-1 MCP gaps (FN-044, TS-033). No product code. | `tests/mcp_followups.rs` |
| WU-018 | ed1b131 | Monitor server and CLI wiring (FN-033..FN-035, IF-018, IF-019, CFG-010). | `rust-cli/monitor/{mod,http}.rs`, stub assets, `args.rs`, `main.rs`, `help.txt`, `completion.zsh`, `tests/monitor_http.rs`, 3 goldens |
| WU-019 | 63852db | Dashboard build (FN-036, CFG-012). | `rust-cli/monitor/assets/{index.html,app.js,app.css}`, `tests/monitor_assets.rs` |
| WU-021 | 6aec542 | Docs, AI threat review, 1.3.0 prep (FN-040, FN-041, CFG-013): README telemetry and monitor sections, CHANGELOG 1.3.0, version, lock, badge, LOCK_SHA256, 7 version goldens; threat-review fixes (T2-13, T2-10, T2-22a, T2-22c, and the Trusted Types CSP finding of `WU-021/threat-model-review.md`). | `README.md`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `sink.rs`, `monitor/http.rs`, `index.html`, `install.sh`, tests |
| WU-022 | 483e65f | Dropped the `(proposed)` label (FN-043) on the CI and live-check evidence. README, CHANGELOG, mock and `tests/repo_content.rs` needles only. | `README.md`, `CHANGELOG.md`, `tests/repo_content.rs`, mock |

Per-unit records: `develop/build-and-verify/<WU-xxx>/`.

## Install and upgrade notes

- Install (Observed in TS-034): `cargo install --path . --locked` installs both binaries; the POSIX and PowerShell installers install, upgrade and uninstall `dtech-aidlc` and `dtech-aidlc-mcp`. An install from the v1 (CLI-only) installer is upgraded and unrelated files are kept. A missing MCP binary, or a foreign `bin/dtech-aidlc-mcp` launcher, is refused.
- Telemetry is on by default and MCP-only (owner decision Q1). Start the server with `--telemetry off` to record nothing. Existing MCP host configurations keep working; hosts that should not record must add the flag.
- Events go to `.aidlc/telemetry/` (git-ignored through the managed block; an existing `.aidlc/.gitignore` gains `/telemetry/` on the next managed write). Retention: 5 MiB per file, 7 days, 50 MiB total, with a soft overshoot of up to one open part.
- Monitor: `dtech-aidlc monitor` prints one stderr line with a loopback URL and a per-run token in the fragment; Ctrl+C stops it. It needs an existing, safe `.aidlc/telemetry`.
- Existing CLI behaviour: unchanged except the scoped golden changes listed in `regression-report.md`.
- Toolchain: unchanged; no new crate (Cargo.lock differs only in the package version line).

## Rollback note

- Revert only the product and test paths of the unit commits (c23b4a8 through 483e65f) on the feature branch; those commits also carry the intent records (`.aidlc/.../state.json` and unit artifacts), so a plain revert would roll the workflow state back too (revert them as well only if that is wanted). Or reinstall 1.2.2 (the v1 installer or `cargo install` from the 1.2.2 source). Nothing was tagged, published or merged, so there is nothing to un-publish.
- Telemetry files are plain data under `.aidlc/telemetry/` and can be deleted at any time; deleting them changes no workflow state, audit record or artifact. Removing the `/telemetry/` line from `.aidlc/.gitignore` is optional.
- Installed 1.3.0 prefixes: run the 1.3.0 uninstall, or reinstall 1.2.2 with its installer. The v2 marker is not recognised by the 1.2.2 installer (Assumption, not tested).

## Conductor decisions recorded in unit artifacts (AI mode, not owner approvals)

- `routing_golden/unsafe.json` re-blessed as an eighth golden for one monitor completion line (WU-018).
- IF-018 deviations: OWS stripped around Authorization; wider bounded drain (WU-018). See `known-limitations.md`.
- WU-021 touched files of earlier units for the AC-030 fixes; WU-022 updated the CHANGELOG (a WU-021 file).
- ADR-025: handshake-error telemetry write, stated as an owner notice at the Ideate gate; no objection received.
