# Red evidence: WU-012

- Purpose: the observed failing run before the lock.
- Status: Complete (red-tests step, 2026-11-28). Cites TS-031, FN-037, AC-027.
- Test-first: yes (isolated worktree at 2ef7fa6, before any change to `rust-cli/dependencies.rs`).

## Observed red run
- Command: `TMPDIR=/private/tmp/aidlc-test-WU-012 cargo test --test deps_lock`
- Result: `test result: FAILED. 0 passed; 1 failed`, panicked at `tests/deps_lock.rs:57`: `edge e2 lost: [{"id":"x","dependency":"e1"}]`.
- Behavioural: three concurrent `deps add x e1|e2|e3` processes all exit 0, but the last writer overwrites the others (read-modify-write without a lock). Not a compile or setup error. Observed once; the design's 6-of-6 spike (Observed, `spike2/release-hardening/deps-lock.diff`) is the earlier repetition.

## Planned fix (handed to build-and-verify)
- Wrap only the `add` action of `run_deps` in the existing `crate::orchestrate::with_state_lock` (intent-1 lock, `.aidlc/state.lock`); `list` and `pull` unchanged; output and exit codes unchanged.
