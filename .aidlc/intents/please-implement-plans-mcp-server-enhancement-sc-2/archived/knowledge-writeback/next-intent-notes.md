# Next-intent notes (after intent 2 of 2)

- Purpose: what the next intent should read before Analyze. Provenance: Observed unless labelled.
- The plans in `plans/mcp-server-enhancement/` are now fully implemented (intent 1: facade and MCP server; intent 2: telemetry, monitor, release prep). No planned phase is left.

## Start here
- Knowledge records: `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc.md` and `.aidlc/knowledge/please-implement-plans-mcp-server-enhancement-sc-2.md`.
- PROJECT.md is stale (still says 1.2.2, no CI, old baseline). Fix it first or read the knowledge records instead (Q16/Q25 follow-up).
- Baseline to expect: 657 passed, 0 failed, 1 ignored on macOS with `TMPDIR` under `/private/tmp` and `target/debug` on `PATH`; CI on windows-latest/macos-latest/ubuntu-latest via `.github/workflows/ci.yml`. The local count includes PowerShell tests that return early when pwsh is absent.

## Rules that held (keep unless the owner changes them)
- The CLI writes no telemetry; `tests/telemetry_guards.rs` enforces it. MCP replies and stdout must stay byte-identical with telemetry on or off.
- Zero new crates (serde_json, sha2 only); Cargo.lock changes only on a version bump; `tests/mcp_packaging.rs` pins the lock hash (CR-stripped).
- Goldens change only for owner-approved reasons and only by `DTECH_AIDLC_BLESS=1` in the baseline environment; check with a filtered diff.
- Monitor: loopback only, token in the fragment, Host exactly `127.0.0.1:<port>`, no `Access-Control-*`, no inline script or style, values rendered with `textContent` only.

## Workflow lessons (conductor)
- Claude Code Workflow worktrees start from `main`, not the branch HEAD: workers must `git checkout --detach <base>` first. Take patches from the worktree with `git diff --binary <base>`, never from text an agent pasted.
- `regression.json` must be written after the last unit became satisfied; re-run the suite before reporting the next unit.
- A design-unit or test-free unit reports red-tests `skipped` with the reason.
- Independent reviewers caught real defects in most units (wrong event ids, an installer rollback regression, a forced-colors contrast issue); keep the review and re-review rounds.

## Candidate next work (Assumption: depends on the owner)
- Launch 1.3.0: tag, GitHub release and merge need owner approval.
- The low threat findings and contract text updates listed in the intent-2 knowledge record.
- DeepSeek Harness verification when the owner wants it.
