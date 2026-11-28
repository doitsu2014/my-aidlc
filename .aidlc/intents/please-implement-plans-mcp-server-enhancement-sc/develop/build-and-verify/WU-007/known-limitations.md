# Known limitations: WU-007

- Windows lock behaviour (LockFileEx, deleting an open file, delete-pending `PermissionDenied`) is unverified locally; the WU-008 CI run is the first evidence.
- A waiter queued behind 4+ successive holders that each retire their lock file gets the non-retryable stale error (ponytail-marked ceiling).
- `open_lock_file` checks `symlink_metadata` then opens without O_NOFOLLOW (small TOCTOU window); `assert_safe_state_dir` already refuses a symlinked `.aidlc`.
- A crash between removing the lock file and process exit cannot leave a marked file (the marker is written only after a successful remove); an older build or a manual edit could, and then the stale-delete hint applies.
- Known gap (pre-existing, outside scope, not reachable over MCP): `deps add` saves intent state without `ensure_unchanged` or the lock.
- ts007 depends on timing to observe a conflict (4 rounds, 8 MiB padding); if CI ever sees zero conflicts, raise rounds or padding rather than dropping the assertion.
- Pre-existing timing-sensitive tests may flake on loaded CI runners (see regression report).
- Rollback: revert this unit's commit (removes the lock and the integration tests).
