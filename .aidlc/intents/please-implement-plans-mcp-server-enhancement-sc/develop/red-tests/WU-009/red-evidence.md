# Red evidence: WU-009

- Status: Complete (red-tests, 2026-11-28).
- Test-first: not used, by design. WU-009 is documentation only (FN-021, AC-014). There is no behaviour to fail first; the existing `tests/repo_content.rs` already guards README structure and sh-block commands, and the TS-015 text checks belong to WU-008. A separate red run would add nothing.
- Planned focused check: `cargo test --test repo_content` (35 tests).
