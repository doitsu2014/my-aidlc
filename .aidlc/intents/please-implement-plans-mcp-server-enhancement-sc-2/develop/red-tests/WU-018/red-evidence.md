# Red evidence: WU-018

- Status: Complete (red-tests step, 2026-11-28). Cites TS-029, TS-037, FN-033..FN-035.
- Test-first: yes for the 19 raw-TcpStream tests (isolated worktree at ddf97ca, before any monitor code).
- Observed: `TMPDIR=/private/tmp/aidlc-test-WU-018 cargo test --locked --test monitor_http` -> `test result: FAILED. 0 passed; 19 failed`; each test ran the real binary and failed on behaviour (`dtech-aidlc: Unknown command monitor` instead of the startup line, tests/monitor_http.rs:62), not on compilation.
- The `ct_eq` and panic-guard unit tests were written with the implementation (not red first).
- Review-round reds (Observed): `ts029_only_get_is_allowed_and_the_method_is_checked_before_the_host` failed (`get`: left 400, right 405) before lowercase method tokens reached the 405 row; TS-037 (`an_unsafe_state_directory_is_refused_by_every_command_but_completion`) failed with `stdout differs for ["completion", "zsh"]` before the one-line re-bless of `routing_golden/unsafe.json`.
