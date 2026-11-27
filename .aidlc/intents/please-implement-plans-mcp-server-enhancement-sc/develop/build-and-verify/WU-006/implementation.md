# Implementation: WU-006 (Path redaction)

- Status: Complete (build-and-verify, 2026-11-28). Cites WU-006, FN-015, IF-007, TS-008; ADR-010; TH-11, TH-12.
- Kind: code manifest.

| File | Change |
|---|---|
| `rust-cli/mcp/redact.rs` (new) | `Redactor::new(root)` / `Redactor::with_env(root, engine_home, home)`: registers the project root (as `.`), `DTECH_AIDLC_HOME` (resolved against the root when relative; only absolute prefixes are registered) and `HOME` (`~`), including macOS `/var` vs `/private/var` twins and Windows forms (`\\?\C:\`, case-insensitive on Windows, separators interchangeable). Prefixes are filtered and sorted once. `redact_value`: every string, and every object key, has registered prefixes replaced by the project-relative or symbolic form; any other absolute-looking token becomes `<redacted-path>`; colliding keys get stable `#2`, `#3` suffixes (no value lost). `redact_message(msg, fallback_code_text)`: if any unregistered absolute path remains after substitution, the whole message becomes the code's generic text (ADR-010 step 4). Token starts include whitespace and `"'\`:(=[{<,;`; `scheme://` URLs pass through except `file:///` (any case); a second absolute path joined by `:` or `=` after a match is redacted on its own; the scan always makes progress. |
| `rust-cli/mcp/mod.rs` | one line `pub mod redact;` (accepted ownership deviation: WU-006 owns only `redact.rs`, but the module must be declared to compile; merged by the conductor after WU-004). |

History: worker patch -> review (hang on relative `DTECH_AIDLC_HOME`, leaks after backtick/colon, message wording, key collisions) -> fix round 1 -> re-review (message redaction not per ADR-010) -> fix round 2 (re-review ok) -> conductor fix at integration for the `:`/`=` joined second path (red-first). Not wired into the server yet: WU-005 builds the Redactor at startup and applies it to every tool result and error.
