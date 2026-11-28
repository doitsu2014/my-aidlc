# Green verification report: WU-009

- Status: Complete (build-and-verify, 2026-11-28). Integrated main tree, base `0487f49`.

## Results per test level (Observed)
- `cargo test --test repo_content` -> 35 passed, 0 failed (required README sections intact; every `dtech-aidlc <cmd>` in README sh blocks is a real CLI command).
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 30 targets, 506 passed, 0 failed (unchanged; documentation only).
- Content checked against the code: protocol revisions (`rust-cli/mcp/protocol.rs`), tool names and `approved_by` (`rust-cli/mcp/tools.rs`), redaction forms (`rust-cli/mcp/redact.rs`), non-retryable `operation_failed`.
- Independent review: 7 items in round 1, 2 important left in round 2 (DeepSeek shape and a false "verified" claim), fixed by the conductor from the source notes.
- Not run: TS-013 (manual launch-and-list in Claude Code and DeepSeek Harness), deferred to a manual release check; TS-015 (README text checks in CI) belongs to WU-008.

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-009 | AC-014 | FN-021 | `cargo test --test repo_content` (README guards) | passed |
| US-009 | AC-014 | FN-021 | TS-015 README text checks (WU-008) | not run here |
| US-003 | AC-003 | FN-016 | TS-013 manual host launch-and-list | not run (deferred, manual) |
