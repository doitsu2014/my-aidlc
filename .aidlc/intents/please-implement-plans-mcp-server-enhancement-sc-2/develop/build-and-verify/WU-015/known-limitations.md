# Known limitations: WU-015

- Windows not run locally; only the symlink case is cfg(unix).
- The race test asserts consistency (state report count equals audit rows equals successful replies), not that a conflict happens in every round.
- `Session::call` blocks on `read_line` without a watchdog, like the existing fixtures; a hung server stalls the test binary instead of failing it.
