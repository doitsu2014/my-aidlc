# Known limitations: WU-018

- Windows untested locally (TimedOut/WouldBlock, 408 timing, BCryptGenRandom, loopback firewall prompt) - windows-latest CI and a manual desktop check.
- No test drives SIGINT/SIGTERM; stop is checked through `Child::kill`.
- ADR-032 row 14 (500 internal_error) is covered by the panic-guard unit test, not over a raw TcpStream (documented deviation).
- Three IF-018 / CFG-010 deviations recorded in implementation.md (eighth golden, OWS around Authorization, wider drain) are for the release-gate record.
