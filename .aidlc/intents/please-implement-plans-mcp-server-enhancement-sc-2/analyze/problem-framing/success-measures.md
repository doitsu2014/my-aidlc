# Success measures: intent 2

Purpose: state how success will be recognized, as measurable conditions.
Status: Draft - pending analysis-baseline gate
Sources: `problem-statement.md`; plan s5-s6 and phase exits; design s9, s11; knowledge record (baseline); `.github/workflows/ci.yml`.
Provenance labels: Observed, User-provided, Assumption, Estimate, Open question.
Row numbers are local to this table and are not citable identifiers (Analyze mints only AC- and US- ids).

| Row | Measure | Target | Provenance / validation |
|---|---|---|---|
| 1 | Telemetry never changes operation results | Outcome, exit code and stdout identical with telemetry on, off, corrupt, full or unwritable | Observed (plan Phase 3 exit); validated by tests (AC-010) |
| 2 | Privacy | Zero payload, free text, credential, env value or unredacted absolute path in any stored event, in tests that feed such inputs | Observed (plan s3.4); AC-002 |
| 3 | Coverage honesty | Every v1 event kind has a producer and a test; dashboard labels uncovered paths as coverage gaps | Observed (plan s5); AC-004..AC-007, AC-017 |
| 4 | Bounded storage | Store stays within the confirmed per-file, age and total limits (proposed 5 MiB, 7 days, 50 MiB) | Open question Q6; AC-009 |
| 5 | Monitor is read-only and local | 100% of non-GET methods and non-loopback/mismatched Host or Origin requests refused; only telemetry data returned | Observed (plan s3.6); AC-014..AC-016 |
| 6 | Dashboard answers the three questions | A reviewer can answer freshness, slow/blocked/failing, and event sequence from sample data without leaving the page | Observed (plan s1); AC-017, AC-018 |
| 7 | Accessibility basics | Keyboard-only completion of range, filter, row, trace, refresh and close flows; no color-only status | Observed (design s7.6); AC-021 |
| 8 | Release consistency | Version, badge, CHANGELOG heading and lock pin agree; README section no longer proposed (per owner evidence decision) | Observed (`tests/repo_content.rs`); AC-023..AC-025 |
| 9 | Install | Both binaries install, update and uninstall from outside the checkout on Windows and POSIX; monitor self-launch works from the installed CLI | Observed (plan AC 12); AC-026 |
| 10 | No regression | Test suite at or above the intent-1 baseline (545 passed, 0 failed); CI green on Windows and macOS; zero new crates unless the owner decides | Observed (knowledge record); AC-027..AC-029; Q3 |
| 11 | Release notes | Notes say MCP, telemetry and dashboard are local optional capabilities and distinguish self-asserted approver names from authentication | Observed (plan Phase 5 exit); AC-030 |

Not measured (no basis in the docs): adoption counts, dashboard load time, telemetry latency overhead (Open question, owner: no bound is specified in the plans).
