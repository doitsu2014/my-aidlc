# Failing tests: WU-009 (README docs)

- Status: Complete (red-tests, 2026-11-28). Cites TS-013, TS-015, FN-021, AC-014, US-009.
- Kind: code manifest. No new test code: this unit only changes `README.md`.

| Check | Test id | Owner / state |
|---|---|---|
| `cargo test --test repo_content` (required README sections; every `dtech-aidlc <cmd>` in README sh blocks is a real command) | existing guard | runs in build-and-verify |
| README text checks for the MCP section | TS-015 | owned by WU-008 (CI) per the design; not part of this unit |
| Manual launch-and-list in Claude Code and DeepSeek Harness | TS-013 | manual release check, deferred (not run) |
