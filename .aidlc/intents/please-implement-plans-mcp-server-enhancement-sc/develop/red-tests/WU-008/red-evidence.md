# Red evidence: WU-008

- Status: Complete (red-tests, 2026-11-28). Test-first: yes for the TS-015 text check (CI and README contract).
- Command (worker worktree, before `ci.yml` existed): `TMPDIR=/private/tmp/aidlc-test-WU-008 cargo test --test repo_content ts015` -> `test result: FAILED. 0 passed; 1 failed`, panicked "read .github/workflows/ci.yml: No such file or directory (os error 2)". The test compiled and ran; the failure is the assertion on the missing workflow, not a compile error.
- The README half passed immediately (WU-009 had already added the section); only the CI half was red.
- TS-012 (a green CI run on both OSes) cannot be red/green locally; it stays open until the owner authorises a push.
