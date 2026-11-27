# Implementation: WU-001 (Scaffold and packaging)

- Purpose: the code change that completes WU-001.
- Status: Complete (build-and-verify, 2026-11-27). Cites WU-001, FN-016, TS-016, TS-017, AC-013, US-009, CFG-001, CFG-002.
- Kind: code manifest.

| File | Change |
|---|---|
| `Cargo.toml` | `rust-version = "1.100"` (owner decision); `default-run = "dtech-aidlc"`; second `[[bin]] dtech-aidlc-mcp` at `rust-cli/mcp_main.rs`. No dependency or feature change. |
| `rust-cli/lib.rs` | `pub mod application;` and `pub mod mcp;` |
| `rust-cli/application.rs` (new) | Doc comment only: the shared CLI/MCP facade, filled in by WU-003. |
| `rust-cli/mcp/mod.rs` (new) | Stub `pub fn run(_args: Vec<OsString>) -> u8` returning 0; replaced by WU-004. |
| `rust-cli/mcp_main.rs` (new) | `main` collects `std::env::args_os()` (no panic on non-UTF-8 argv) and exits with `mcp::run`'s code. |
| `tests/mcp_packaging.rs` (new) | TS-016: dependency set is exactly serde_json + sha2 with no `[features]`; rust-version pinned; both bins declared, two `[[bin]]` sections, both executables built; `default-run` is the CLI; Cargo.lock sha256 (CR stripped, so a Windows CRLF checkout matches) equals the base value. |

Integration changes on top of the worker patch (from the independent review): CRLF-tolerant Cargo.lock/Cargo.toml reads (Windows CI), `default-run` (the README/PROJECT.md `cargo run -- --help` would otherwise fail with "could not determine which binary to run"), `application` module registered here because WU-003 does not own `lib.rs`, `args_os`/`OsString` entry point (FN-016), stronger bin assertions plus a default-run test.

Cargo.lock: byte-identical. Codebase map updated (sections 3 and 8): new entry point, module and test command.
