# Known limitations: WU-021

- Windows: the extended in-use installer test, the PowerShell tests, BCryptGenRandom and the monitor on Windows are unverified until windows-latest CI (release-gate condition). The extended Windows test asserts before killing the started process, so a failure there can leak a process on the runner.
- Manual checks not done: Windows desktop firewall prompt and `.cmd` Ctrl+C prompt; dashboard keyboard, screen reader, dark mode and forced-colors pass.
- The README "(proposed)" label stays until WU-022 (green CI on Windows and macOS plus a live Claude Code tools/call by the owner).
- T2-13 remains check-then-open per write (narrowed, same-user only).
- Low threat findings carried as follow-ups are listed in threat-model-review.md.
