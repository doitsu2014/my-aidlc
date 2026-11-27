# Known limitations: WU-002

- Site 1 (the `--claim` path) has no dedicated skip test. Under skip it only calls `check_prerequisites` (covered by `prerequisites::tests::ts010_skip_runs_no_command`) and writes no audit row itself; risk judged low by the re-reviewer.
- Under skip, the site-2 hold warning keeps the CLI wording ("the check is failing"), although nothing ran. The Blocked hint (site 3) uses the "not run over MCP (not executed)" wording. Cosmetic; CLI text untouched.
- `pending_prerequisites` on a ledger intent uses `work_units::peek` and ignores claim/parallel selection; it is a preview, not a guarantee of the next selection.
- The persist-time audit row can still be written under skip when every needed name is undeclared (`ok: true`); that row is identical to the CLI's and does not claim a check ran.
- Test check commands are Unix `sh` forms; Windows is covered only by CI (WU-008).
- Rollback: revert this unit's commit.
