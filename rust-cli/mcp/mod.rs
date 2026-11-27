//! MCP server (stub). The protocol layer lands in later work units.

use std::ffi::OsString;

/// Entry point of `dtech-aidlc-mcp`; returns the process exit code.
///
/// Arguments are taken as `OsString` so a non-UTF-8 `--project` path never
/// panics before the server can report a proper error.
pub fn run(_args: Vec<OsString>) -> u8 {
    0
}
