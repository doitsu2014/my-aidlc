# Known limitations: WU-014

- Windows untested locally (windows-latest CI).
- The panic path in `traced_call` (record internal_error, resume the panic) is covered only by the protocol-layer host-panic case, not by an integration test inside the real tools.
- Owner notice (ADR-025, gate follow-up): with telemetry on and `.aidlc` present, an error reply in initialize, server/discover or a list handler leads to one `mcp.protocol.error` write. Reply bytes and stdout are unchanged.
- Nits accepted: the concurrent test waits on a barrier after spawning (a spawn panic could hang it, same as the mcp_parity racer); the failing-store test relies on the writer printing its line within the 500 ms drain.
