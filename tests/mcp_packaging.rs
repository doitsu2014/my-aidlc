//! TS-016 (AC-013, FN-016): the MCP binary is additive. No new dependency, a pinned
//! rust-version, a byte-identical Cargo.lock, and both binaries declared.

use sha2::{Digest, Sha256};

const LOCK_SHA256: &str = "e0c3502fdb3ae86f0371e47879595413e6d45eaf9791199a47e23868e44c4258";

fn manifest() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn ts016_dependency_set_is_unchanged() {
    let m = manifest();
    let deps = m
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap();
    let names: Vec<&str> = deps
        .lines()
        .filter_map(|l| l.split('=').next().map(str::trim))
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(names, ["serde_json", "sha2"]);
    assert!(!m.contains("[features]"));
}

#[test]
fn ts016_rust_version_pinned() {
    assert!(manifest().contains("rust-version = \"1.100\""));
}

#[test]
fn ts016_both_bins_declared() {
    let m = manifest();
    assert!(m.contains("name = \"dtech-aidlc\""));
    assert!(m.contains("name = \"dtech-aidlc-mcp\""));
    assert!(m.contains("path = \"rust-cli/mcp_main.rs\""));
    assert!(m.contains("path = \"rust-cli/main.rs\""));
    assert_eq!(m.matches("[[bin]]").count(), 2);
    assert!(std::path::Path::new(env!("CARGO_BIN_EXE_dtech-aidlc")).exists());
    assert!(std::path::Path::new(env!("CARGO_BIN_EXE_dtech-aidlc-mcp")).exists());
}

#[test]
fn ts016_cli_stays_the_default_run_target() {
    // `cargo run -- --help` (README, PROJECT.md) must keep running the CLI.
    assert!(manifest().contains("default-run = \"dtech-aidlc\""));
}

#[test]
fn ts016_cargo_lock_bytes_unchanged() {
    // A Windows checkout with core.autocrlf=true has CRLF line endings; hash the
    // LF form so the pinned value holds on every platform.
    let bytes: Vec<u8> = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock"))
        .unwrap()
        .into_iter()
        .filter(|b| *b != b'\r')
        .collect();
    let hex: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hex, LOCK_SHA256);
}
