# Known limitations: WU-009

- DeepSeek Harness was never run against `dtech-aidlc-mcp`; the README says so. That `DTECH_AIDLC_HOME` survives its scrubbed environment is assumed.
- Windows host launch of `dtech-aidlc-mcp.exe` is unverified (CI covers build and tests, not a host).
- The section is labelled proposed; the platform sentence near the install section stays until CI is green on Windows and macOS.
- Rollback: revert this unit's commit.
