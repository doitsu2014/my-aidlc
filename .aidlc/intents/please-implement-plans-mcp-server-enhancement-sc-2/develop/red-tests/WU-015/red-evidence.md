# Red evidence: WU-015

- Status: Complete (red-tests step, 2026-11-28). Cites TS-033, FN-044.
- Test-first: not used. Reason: these tests characterise intent-1 behaviour that already exists (work-units.md: "expected green on arrival"); a red result would be a real bug.
- Meaningfulness (Observed, isolated worktree at 164f09c): one expectation per test was mutated and each test failed, then the file was restored (diff against the saved original empty): ai-mode ('pinned to human mode' -> 'robot'), unsafe-at-launch (exit 1 -> 0), symlinked (needle changed), unsafe-mid-session (report count +2), interleave (expected +1), race (audit rows +1), restart (detail 'nope'), shutdown (rest non-empty).
- First run: 4 failures that were wrong test assumptions, not product bugs (an unsafe layout makes the server exit 1 at launch rather than fail per call; a report needs a prior `next`). Fixed in the test file. No product defect found, so no WU-014 park was needed.
