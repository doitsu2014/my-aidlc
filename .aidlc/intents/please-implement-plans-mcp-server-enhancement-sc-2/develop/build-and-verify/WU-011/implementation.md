# Implementation: WU-011 (Flaky-test fixes)

- Status: Complete (build-and-verify, 2026-11-28). Cites FN-038, TS-032, AC-027, AC-029.
- Changed files (test modules only, no production line changed):
  - `rust-cli/artifacts.rs`: `temp()` adds a module `AtomicUsize` sequence to the directory name.
  - `rust-cli/onboarding/projection.rs`: same for `temp(tag)`.
  - `rust-cli/prerequisites.rs`: `check_timeout_is_reported` sleeps 5 s (Windows `ping -n 6`) against the 1 s timeout.
- Not changed: `rust-cli/scope.rs` (0 failures in Loops A and B, so the optional counter was not kept).
- Worker: isolated worktree at 2ef7fa6; independent review: ok, no blocking issue; patch taken from the worktree (the pasted patch had a transport error on the Windows line).
