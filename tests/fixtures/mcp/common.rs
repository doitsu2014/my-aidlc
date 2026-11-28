//! Shared helpers for the MCP integration tests (`mcp_stdio`, `mcp_parity`,
//! `mcp_redaction`), included with `#[path]` so no test-support crate is needed. Every
//! project lives under a canonical, symlink-free base (CFG-004) with the Windows
//! `\\?\` prefix stripped.
#![allow(dead_code)]

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MCP_BIN: &str = env!("CARGO_BIN_EXE_dtech-aidlc-mcp");
pub const CLI_BIN: &str = env!("CARGO_BIN_EXE_dtech-aidlc");
pub const PV: &str = "io.modelcontextprotocol/protocolVersion";
pub const CAPS: &str = "io.modelcontextprotocol/clientCapabilities";

/// Strip the Windows verbatim prefix that `canonicalize` adds.
pub fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// Canonical temp base without the Windows verbatim prefix.
pub fn base() -> PathBuf {
    plain(std::env::temp_dir().canonicalize().unwrap())
}

pub struct Tmp(pub PathBuf);

impl Tmp {
    pub fn new(label: &str) -> Self {
        Self::under(&base(), label)
    }

    /// Like `new` but under an explicit base (the redaction test uses the raw temp dir).
    pub fn under(base: &Path, label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = base.join(format!(
            "aidlc-mcp-{label}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Tmp(path)
    }

    /// A temp project onboarded through the CLI (solo scope).
    pub fn project(label: &str) -> Self {
        let tmp = Tmp::new(label);
        cli_ok(
            &tmp.0,
            &["onboard", "--harness", "claude", "--scope", "solo"],
        );
        tmp
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn cli(project: &Path, args: &[&str]) -> Output {
    Command::new(CLI_BIN)
        .args(args)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap()
}

/// CLI call that must exit 0; stdout parsed as JSON (`null` when empty).
pub fn cli_ok(project: &Path, args: &[&str]) -> Value {
    let out = cli(project, args);
    assert!(
        out.status.success(),
        "{args:?} exit {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}

pub fn start_intent(project: &Path, text: &str) -> Value {
    cli_ok(project, &["orchestrate", "next", "--intent", text])
}

pub fn write_config(project: &Path, edit: impl FnOnce(&mut Value)) {
    let path = project.join(".aidlc/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut config);
    fs::write(path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

/// Write placeholder content for every artifact a directive names.
pub fn write_artifacts(project: &Path, directive: &Value) {
    for artifact in directive["artifacts"].as_array().unwrap() {
        let path = project.join(artifact["path"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# Artifact\n\nContent.\n").unwrap();
    }
}

// ---------------------------------------------------------------------------
// MCP child process
// ---------------------------------------------------------------------------

pub fn frame(id: Value, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

pub fn modern_meta() -> Value {
    json!({PV: "2026-07-28", CAPS: {}})
}

/// A modern-era request: the `_meta` protocol-version key rides on every request.
pub fn modern(id: u64, method: &str, mut params: Value) -> Value {
    params["_meta"] = modern_meta();
    frame(json!(id), method, params)
}

pub fn initialize(id: u64, version: &str) -> Value {
    frame(
        json!(id),
        "initialize",
        json!({"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
    )
}

pub fn lines(frames: &[Value]) -> Vec<u8> {
    frames
        .iter()
        .flat_map(|f| format!("{f}\n").into_bytes())
        .collect()
}

/// Spawn `dtech-aidlc-mcp [--project <dir>]`, feed `input` from a thread (so large
/// inputs cannot deadlock against the replies), close stdin, wait for exit.
pub fn run_mcp(project: Option<&Path>, input: Vec<u8>) -> Output {
    run_mcp_env(project, input, &[])
}

/// `run_mcp` with extra environment variables (`DTECH_AIDLC_HOME` is cleared unless given).
pub fn run_mcp_env(project: Option<&Path>, input: Vec<u8>, envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(MCP_BIN);
    if let Some(project) = project {
        command.arg("--project").arg(project);
    }
    if !envs.iter().any(|(k, _)| *k == "DTECH_AIDLC_HOME") {
        command.env_remove("DTECH_AIDLC_HOME");
    }
    command.envs(envs.iter().copied());
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    out
}

/// Stdout purity (upstream threat label TH-14, not an ids.json id): exit 0 on EOF, LF-only, every line one JSON-RPC object.
pub fn frames_of(out: &Output) -> Vec<Value> {
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.stdout.contains(&b'\r'), "stdout must be LF only");
    let text = String::from_utf8(out.stdout.clone()).expect("stdout is UTF-8");
    assert!(text.is_empty() || text.ends_with('\n'));
    text.lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"));
            assert_eq!(v["jsonrpc"], "2.0", "{line}");
            assert!(
                v.get("result").is_some() ^ v.get("error").is_some(),
                "{line}"
            );
            v
        })
        .collect()
}

/// One modern-era `tools/call` in a fresh session; returns (structuredContent, isError).
pub fn tool(project: &Path, name: &str, args: Value) -> (Value, bool) {
    let out = run_mcp(
        Some(project),
        lines(&[modern(
            1,
            "tools/call",
            json!({"name": name, "arguments": args}),
        )]),
    );
    let replies = frames_of(&out);
    assert_eq!(replies.len(), 1, "{replies:?}");
    let result = &replies[0]["result"];
    let structured = result["structuredContent"].clone();
    assert!(structured.is_object(), "{replies:?}");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, structured, "text block mirrors structuredContent");
    (structured, result["isError"].as_bool().unwrap())
}

pub fn tool_ok(project: &Path, name: &str, args: Value) -> Value {
    let (value, is_error) = tool(project, name, args);
    assert!(!is_error, "{name} {value}");
    value
}

pub fn tool_err(project: &Path, name: &str, args: Value) -> Value {
    let (value, is_error) = tool(project, name, args);
    assert!(is_error, "{name} {value}");
    value["error"].clone()
}

// ---------------------------------------------------------------------------
// Snapshots and normalisation
// ---------------------------------------------------------------------------

/// Bytes of every file under `.aidlc` except the embedded engine copy.
pub fn snapshot(project: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if rel == ".aidlc/engine" {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&project.join(".aidlc"), project, &mut out);
    out
}

pub fn audit_events(project: &Path) -> Vec<String> {
    fs::read_to_string(project.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["event"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn is_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b.last() == Some(&b'Z')
        && b[..4].iter().all(u8::is_ascii_digit)
}

/// Make a CLI or MCP value comparable across two projects: the project root becomes
/// project-relative (as the redactor does), timestamps become `<ts>`.
pub fn normalize(v: &Value, root: &Path) -> Value {
    let root_text = root.to_string_lossy().into_owned();
    let fwd = root_text.replace('\\', "/");
    let text = |s: &str| -> String {
        if is_timestamp(s) {
            return "<ts>".into();
        }
        let mut s = s.to_owned();
        for r in [&root_text, &fwd] {
            for sep in ["/", "\\"] {
                // Strip the root wherever it occurs; the path remainder (up to the next
                // delimiter) uses `/` like the redactor's output, also on Windows.
                let prefix = format!("{r}{sep}");
                let mut parts = s.split(prefix.as_str());
                let mut out = parts.next().unwrap_or("").to_owned();
                for part in parts {
                    let end = part
                        .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ')'))
                        .unwrap_or(part.len());
                    out.push_str(&part[..end].replace('\\', "/"));
                    out.push_str(&part[end..]);
                }
                s = out;
            }
            if s == *r {
                s = ".".into();
            }
        }
        s
    };
    match v {
        Value::String(s) => Value::String(text(s)),
        Value::Array(a) => Value::Array(a.iter().map(|x| normalize(x, root)).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| (text(k), normalize(x, root)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

/// A prerequisite check command that creates `marker` on every platform, then fails.
pub fn marker_check(marker: &Path) -> String {
    if cfg!(windows) {
        format!("type nul > \"{}\" & exit 1", marker.display())
    } else {
        format!("touch '{}'; exit 1", marker.display())
    }
}
