# Known limitations: WU-020

- install.ps1 and all PowerShell tests are unverified until CI (pwsh absent locally). On macOS CI they run under pwsh; on Windows CI they run for real, including the in-use preflight and the `cmd /C dtech-aidlc.cmd --version` launcher check.
- `Mcp-Is-Ours` recognises `bin\dtech-aidlc-mcp.exe` by hash against `lib\dtech-aidlc\dtech-aidlc-mcp.exe`; after a v1 upgrade a pre-existing foreign MCP exe is refused (no installed copy to compare).
- Upgrade replaces `lib/dtech-aidlc` wholesale; unrelated files elsewhere in the prefix are kept.
- The installed-monitor smoke test belongs to WU-021 (TS-034 second owner).
