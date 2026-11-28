# Green verification: WU-011

- Status: Complete (2026-11-28). Cites TS-032, FN-038, AC-027, AC-029.
- After the fix (Observed, worktree at 2ef7fa6 + patch): Loop A 40 runs, 0 failures (`20 passed; 0 failed` each); Loop C 40 runs under full CPU load, 0 failures. `cargo build` ok, `cargo fmt --check` clean.
- Full suite (Observed): worker worktree 545 passed / 0 failed (two runs); conductor main tree with WU-011 + WU-012 applied, 546 passed / 0 failed (third run; +1 is the WU-012 test).
- Trace: US-013 | AC-027 | FN-038 | TS-032 evidence record | PASS. US-010 | AC-029 | FN-038 | TS-032 | PASS (macOS; Windows via CI).
