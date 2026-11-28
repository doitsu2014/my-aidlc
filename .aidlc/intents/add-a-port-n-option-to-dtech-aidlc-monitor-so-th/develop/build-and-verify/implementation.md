# Implementation: monitor --port (intent 3)

- Status: Complete (build-and-verify, 2026-11-28). Commit e319bac on feature/please-implement-plans-mcp-server-enhancement-sc. Cites AC-001..AC-010.
- `rust-cli/monitor/mod.rs`: `parse_port` (ASCII digits, 1..=65535, leading zeros allowed); `run(root, port)` validates the port first, then the telemetry check, token and bind; binds `127.0.0.1:<n>` (or a free port without the flag); a failed fixed bind returns `Cannot bind 127.0.0.1:<n> (port in use or not permitted)` with the validated number and never falls back. Startup line and Host check unchanged in form (Host uses the bound port).
- `rust-cli/args.rs`, `rust-cli/main.rs`: `--port` is a String option of `monitor` in the merged flag table (D5); `main.rs` passes the value to `monitor::run`. Monitor's allowed-options text becomes `--port, --project`.
- `rust-cli/help.txt`, `rust-cli/completion.zsh`: `monitor [--port N]` and a monitor completion arm.
- Goldens re-blessed with `DTECH_AIDLC_BLESS=1` (owner-scoped): `routing_golden/validation.json`, `routing_golden/unsafe.json`, `utility_golden/help-and-completion.json`; word-diff contains only `[--port N]`, the help sentence, `--port,` and the monitor completion arm.
- README "## Monitor" documents `--port`, the per-run token (a bookmark without the token shows Token missing), and the OS limits; CHANGELOG 1.3.0 gains one line. Version stays 1.3.0; Cargo.toml and Cargo.lock unchanged.
- Tests: 2 unit, 10 `port_` integration tests, `status_port_effects_are_the_disclosed_ones` (D5), table/help/README needles. Conductor fixes after review: `run_refused` has a 10 s deadline so a regression that starts serving fails instead of hanging; the in-use test also checks the leading-zero spelling; doc comment moved back to `run`.
- Rollback: `git revert e319bac`.
