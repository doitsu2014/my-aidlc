# Failing tests: WU-008 (CI workflow)

- Status: Complete (red-tests, 2026-11-28). Cites TS-012, TS-015, FN-018, FN-021, IF-010, CFG-003, AC-001, AC-003, AC-013, AC-014.
- Kind: code manifest.

| Test | File | Test id | Red observed |
|---|---|---|---|
| `ts015_ci_workflow_and_readme_mcp_section` | `tests/repo_content.rs` | TS-015 | yes: "read .github/workflows/ci.yml: No such file or directory" before the workflow existed |
| The CI run itself on windows-latest and macos-latest | `.github/workflows/ci.yml` | TS-012 | not runnable locally; needs an owner-authorised push (deferred open item) |
