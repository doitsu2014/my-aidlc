# Implementation: WU-012 (deps add lock)

- Status: Complete (build-and-verify, 2026-11-28). Cites FN-037, TS-031, AC-027.
- Changed files:
  - `rust-cli/dependencies.rs`: the `add` action of `run_deps` runs inside `crate::orchestrate::with_state_lock` after `assert_safe_state_dir`; `list` and `pull` unchanged. Output and exit codes unchanged; a busy lock returns the existing lock error after the existing wait.
  - `tests/deps_lock.rs` (new): `ts031_concurrent_deps_add_keeps_three_edges` (three concurrent processes behind a barrier, then `deps list` must hold e1, e2, e3).
- Review nits applied by the conductor: the temp project is removed by a drop guard even on panic; `deps list` exit status asserted before JSON parsing.
