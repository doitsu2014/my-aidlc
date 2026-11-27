# Known limitations: WU-001

- `dtech-aidlc-mcp` is a stub that ignores its arguments and exits 0; the protocol layer and `--project` handling arrive in WU-004.
- TS-016 checks the manifest text and a pinned Cargo.lock hash; it does not install the binaries. The two-binary offline install check is TS-009 in WU-007. Any later unit that legitimately changes Cargo.lock must update the pinned hash (none is planned: zero new crates).
- Windows behaviour (CRLF-stripped hash, args_os) is verified only on the WU-008 CI leg.
- Rollback: revert this unit's commit.
