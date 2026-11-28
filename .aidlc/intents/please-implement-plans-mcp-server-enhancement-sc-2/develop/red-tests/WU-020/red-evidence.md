# Red evidence: WU-020

- Status: Complete (red-tests step, 2026-11-28). Cites TS-034, FN-039, AC-026.
- Test-first: yes (isolated worktree at 695d62f, tests written before install.sh / install.ps1 changed).

## Observed
- `cargo test --locked --test rust_cargo_install` before the installer change: `8 passed, 3 failed`; the three POSIX ts034 tests failed on behaviour (missing installed MCP binary; installer accepted a missing MCP binary / foreign MCP launcher).
- Review-fix reds (Observed, mutants of install.sh): with the uninstall recognition check removed, `ts034_posix_uninstall_refuses_foreign_mcp_launcher` failed (:748); with `--upgrade` appending to `$HOME/.claude/settings.json`, the round trip failed at the settings byte comparison (re-reviewer); with the MCP print line removed, the round trip failed (:568).
- PowerShell: no red locally (pwsh absent). Behaviour is an Assumption until macOS and Windows CI, where pwsh is present and the CI step forbids skipping.
