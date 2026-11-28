# Implementation: WU-008 (CI workflow)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-008, FN-018, IF-010, CFG-003, TS-012, TS-015; post-gate amendment (Windows and macOS, owner 2026-11-27).
- Kind: code manifest.

| File | Change |
|---|---|
| `.github/workflows/ci.yml` (new) | Triggers `push`, `pull_request`; `permissions: contents: read`. Job `build-test`, `fail-fast: false`, matrix `windows-latest`, `macos-latest` (no `continue-on-error`) and best-effort `ubuntu-latest` (`continue-on-error` via `experimental: true`). Steps: keep LF line endings (`git config --global core.autocrlf false`, before checkout); `actions/checkout@v4`; install Rust 1.100 with rustup; macOS: create `/private/tmp/aidlc-test`, `TMPDIR` via `GITHUB_ENV`, `target/debug` on `GITHUB_PATH`; Linux: `target/debug` on PATH; Windows (pwsh): `target\debug` on PATH; `cargo build --locked`; `cargo test --locked --no-fail-fast`; "Installer tests must not skip" (re-runs `rust_cargo_install` with `--nocapture` and fails if it printed `skipped: pwsh unavailable` or `skipped: POSIX shell unavailable`); "MCP transcript tests" (`cargo test --locked --test mcp_stdio --test mcp_parity --test mcp_redaction`). No allowlist, no skip list, no installers, no secrets, no deployment. |
| `tests/repo_content.rs` | `ts015_ci_workflow_and_readme_mcp_section` (TS-015): the workflow has both legs, `fail-fast: false`, the locked build/test commands in order, the named MCP step, Rust 1.100, a pinned checkout, `pull_request`, the macOS TMPDIR setup, GITHUB_PATH, the LF step and the skip-visibility step; only the Ubuntu leg is experimental; no installers and no secrets; the README MCP section says proposed, names the four tools, says the approver is self-asserted and `state_conflict` retryable. |

Integration additions on top of the worker patch (from the independent review): the skip-visibility step required by test-strategy.md, the LF line-ending step for Windows goldens, and stronger TS-015 assertions (macOS env, GITHUB_PATH, triggers, only-Ubuntu-experimental, step order). No push was made. Codebase map updated (CI entry point).
