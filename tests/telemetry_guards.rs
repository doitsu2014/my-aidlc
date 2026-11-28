//! TS-023 owner-Q1 guards: the CLI writes no telemetry, and library code never writes stdout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rs_files(&root.join("rust-cli"), &mut files);
    files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (rel, fs::read_to_string(&p).unwrap())
        })
        .collect()
}

#[test]
fn ts023_sink_is_built_only_under_mcp() {
    let needles = [
        "Sink::start",
        "Sink::spawn",
        "telemetry::Sink",
        "sink::Sink",
    ];
    let mut seen_in_telemetry = false;
    for (rel, text) in sources() {
        let allowed = rel.starts_with("rust-cli/telemetry/") || rel.starts_with("rust-cli/mcp/");
        let hit = needles.iter().any(|n| text.contains(n));
        seen_in_telemetry |= hit && rel.starts_with("rust-cli/telemetry/");
        assert!(
            !hit || allowed,
            "{rel} constructs or names the telemetry Sink"
        );
    }
    assert!(
        seen_in_telemetry,
        "scan found no Sink at all; the guard is blind"
    );
}

#[test]
fn ts023_no_stdout_writes_outside_main_and_mcp_handoff() {
    let mut main_hits = 0;
    let mut mcp_hits = 0;
    for (rel, text) in sources() {
        let text = text.replace("eprintln!", "").replace("eprint!(", "");
        let hits = text.matches("println!").count()
            + text.matches("print!(").count()
            + text.matches("io::stdout").count();
        match rel.as_str() {
            "rust-cli/main.rs" => main_hits = hits,
            "rust-cli/mcp/mod.rs" => mcp_hits = hits,
            _ => assert_eq!(hits, 0, "{rel} writes to stdout"),
        }
    }
    assert_eq!((main_hits, mcp_hits), (1, 1));
}

#[test]
fn ts023_cli_run_creates_no_telemetry_dir() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = std::env::temp_dir().join(format!("ts023-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("README.md"), "# Proj\n").unwrap();
    let steps: [&[&str]; 5] = [
        &["onboard", "--harness", "claude", "--scope", "solo"],
        &["doctor"],
        &["orchestrate", "next", "--intent", "Guard work"],
        &["status"],
        &["intent", "list"],
    ];
    for argv in steps {
        let out = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(argv)
            .arg("--project")
            .arg(&root)
            .current_dir(&root)
            .env("DTECH_AIDLC_HOME", repo)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(out.status.code().is_some(), "{argv:?}");
    }
    assert!(
        root.join(".aidlc").is_dir(),
        "the CLI run must have initialised .aidlc"
    );
    assert!(!root.join(".aidlc/telemetry").exists());
    let _ = fs::remove_dir_all(&root);
}
