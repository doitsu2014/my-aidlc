# Known limitations: WU-005

- Over MCP, `next` with no active intent returns `operation_failed` whose (redacted) text suggests the CLI flag `--intent`, which MCP does not offer. Candidate wording fix in WU-007 or later.
- The DTECH_AIDLC_HOME startup warning has no unit test (it lives in `run`, which reads stdin); child-process coverage belongs to WU-007. A manual check showed it goes to stderr with the path redacted.
- Length limits count characters; `needs` items are checked by byte length (same for the ASCII-only charset).
- Stage ids are validated by pattern, not against the live phase graph at call time; the CLI path still validates them.
- Windows not run (WU-008 CI); real-host launch checks in Claude Code and DeepSeek Harness are TS-013 (manual).
- Rollback: revert this unit's commit.
