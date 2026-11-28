# Known limitations: WU-022

- DeepSeek Harness was not run against `dtech-aidlc-mcp` (owner deferral); host launch of the `.exe` on Windows by an AI host was not run (CI ran the binaries directly).
- The live check used the owner's normal Claude Code login with `--strict-mcp-config`; Claude Code stores that session transcript in its usual place.
