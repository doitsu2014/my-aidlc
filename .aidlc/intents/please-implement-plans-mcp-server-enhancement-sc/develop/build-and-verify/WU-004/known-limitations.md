# Known limitations: WU-004

- `dtech-aidlc-mcp` serves an empty tool table (`tools/list` returns `[]`) until WU-005 wires the tool adapter.
- The DTECH_AIDLC_HOME unusable-engine stderr warning and the startup Redactor are not here; re-assigned to WU-005 (conductor ruling, recorded in implementation.md).
- A final line of exactly 1 MiB followed by a lone CR at EOF is accepted (lenient, harmless; frozen by `ts003_cap_boundaries`).
- interfaces-and-contracts.md lists "wrong or missing jsonrpc" among the -32600 "(id null)" cases; the code follows FN-011 and echoes a valid id there (JSON-RPC 2.0 allows it). Design wording to be aligned at the release-candidate gate.
- No real-host launch yet (TS-013 manual checks in Claude Code and DeepSeek Harness) and no Windows run (WU-008 CI).
- Rollback: revert this unit's commit.
