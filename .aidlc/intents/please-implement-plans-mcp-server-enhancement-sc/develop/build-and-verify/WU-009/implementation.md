# Implementation: WU-009 (README docs)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-009, FN-021, TS-013, TS-015, AC-014, US-009; ADR-017.
- Kind: code manifest. One file: `README.md` (+66 lines, a new section after "Harnesses"; nothing else changed).

New `## MCP server (proposed)` section, labelled as proposed:
- What it is: a local stdio MCP server, no HTTP transport and no network listener; four tools `status`, `queue`, `next`, `report` over the same library behaviour as the CLI commands, which are unchanged.
- Install: `cargo install --path . --locked` installs both binaries; the prebuilt-binary installers in `scripts/rust-install/` stay CLI-only (owner decision N2).
- Claude Code: `claude mcp add --transport stdio aidlc -- dtech-aidlc-mcp --project <dir>`, or a project `.mcp.json`; advice not to auto-allow `next`/`report`.
- DeepSeek Harness: a `dsh-mcp-client` plugin `insert` entry for a user patch layer (`cordis.patch.yml`), `transport: stdio`, `serverName: aidlc`, `cwd: !!js process.cwd()`; states plainly what was verified (entry shape from the dsh-mcp-client documentation, its pinned client against a prototype server) and what was not (DeepSeek Harness never run against `dtech-aidlc-mcp`).
- Troubleshooting: served protocol revisions (2026-07-28; legacy 2025-11-25, 2025-06-18), stderr-only diagnostics, prerequisites never run over MCP, `approved_by` required and self-asserted (not authentication), `state_conflict` retryable (re-run `status`, then retry), path redaction forms.
- Windows and macOS: binary locations, PATH, unverified Windows host launch, and the non-retryable `operation_failed` when another process holds `.aidlc` state open on Windows.
- The existing platform sentence near the install section is unchanged (owner: updated only after CI is green on both OSes).

Integration correction (conductor): the worker's DeepSeek block used an invented `mcpServers:` shape and claimed it was verified; replaced with the documented `dsh-mcp-client` `insert` entry from the DeepSeek Harness docs (spike notes `mcp-memory.md`, `README.md` field table) and the claim reworded. Added the `$DTECH_AIDLC_HOME` redaction form.
