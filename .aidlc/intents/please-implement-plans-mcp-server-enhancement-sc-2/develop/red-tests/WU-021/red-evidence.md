# Red evidence: WU-021

- Status: Complete (red-tests step, 2026-11-28). Cites TS-034, TS-035, TS-036.
- Test-first: partly. The README sections were written before TS-036 (the unit order is docs first); TS-036 was then confirmed red against the README at 63852db (Observed: panic at tests/repo_content.rs:1418). The installed-monitor smoke test characterises existing behaviour.
- Threat-review fixes were test-first or shown red by mutation (Observed): T2-13 unit test failed with the guards removed (`unwrap_err() on an Ok value`); the TERM test failed with the old install.sh (`left: Some(0) right: Some(143)`); the fresh-install test failed before the `had_root` fix (lib/dtech-aidlc left behind, tests/rust_cargo_install.rs:768).
