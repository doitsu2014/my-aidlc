# Green verification report: WU-008

- Status: Complete for the local part (build-and-verify, 2026-11-28). Integrated main tree, base `bab236e`. TS-012 is a deferred open item.

## Results per test level (Observed)
- `cargo test --test repo_content ts015` -> 1 passed (after the strengthened assertions).
- YAML parse (Ruby `YAML.load_file`): 10 steps in order (Keep LF line endings, checkout, Install Rust 1.100, macOS/Linux/Windows environment, Build, Test, Installer tests must not skip, MCP transcript tests); `fail-fast: false`; matrix windows/macos (experimental false), ubuntu (experimental true). actionlint and Python yaml not installed.
- Full existing suite: `cargo build && TMPDIR=/private/tmp/aidlc-test PATH="$PWD/target/debug:$PATH" cargo test --no-fail-fast` -> 33 targets, 545 passed, 0 failed (baseline 544/0; +1).
- Not run: the workflow on GitHub (TS-012). It needs an owner-authorised push; until then Windows behaviour (including the state lock on Windows and Windows goldens) is unmeasured, and the release-candidate gate must list TS-012 and TS-013 as open.

## Trace table
| Story | Criterion | Function | Test | Result |
|---|---|---|---|---|
| US-001 | AC-001 | FN-018 | `ts015_ci_workflow_and_readme_mcp_section` (workflow contract) | passed |
| US-001 | AC-001 | FN-018 | TS-012 CI run on windows-latest and macos-latest | not run (deferred; needs push) |
| US-003 | AC-003 | FN-018 | TS-012 "MCP transcript tests" step on both OSes | not run (deferred) |
| US-009 | AC-013 | FN-018 | TS-012 install tests on both OSes with skip visibility | not run (deferred) |
| US-009 | AC-014 | FN-021 | `ts015_ci_workflow_and_readme_mcp_section` (README section) | passed |
