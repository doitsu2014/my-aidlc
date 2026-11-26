//! Parity tests for the native `config ...` commands against the Node CLI.
//!
//! Each scenario builds one fixture, copies it to two temp projects, runs the Node CLI on
//! one and `dtech_aidlc::config_commands::run` on the other, and compares the JSON output
//! (paths and timestamps normalised), error texts, exit codes, every project file byte for
//! byte (except `audit.jsonl`), and the new audit rows without their `at` field.
//! Node comparisons are skipped when `node` is not on PATH.

use dtech_aidlc::args::{command_key, parse_args};
use dtech_aidlc::config_commands;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aidlc-config-parity-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn graph() -> Value {
    serde_json::from_slice(&fs::read(repo().join("core/data/phase-graph.json")).unwrap()).unwrap()
}

fn node_raw(project: &Path, words: &[&str]) -> std::process::Output {
    Command::new("node")
        .arg(repo().join("core/tools/dtech-aidlc.mjs"))
        .args(words)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap()
}

/// A Node-onboarded project (claude, solo) used as the starting fixture.
fn onboarded(harness: &str) -> Dir {
    let dir = Dir::new("template");
    let out = node_raw(
        &dir.0,
        &["onboard", "--harness", harness, "--scope", "solo"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Ok(Value, i32),
    Err(String),
}

fn normalize_text(text: &str, roots: &[&Path]) -> String {
    let mut text = text.to_owned();
    for root in roots {
        text = text.replace(&root.display().to_string(), "<ROOT>");
    }
    text
}

fn normalize(value: &Value, roots: &[&Path]) -> Value {
    match value {
        Value::String(text) => Value::String(normalize_text(text, roots)),
        Value::Array(items) => Value::Array(items.iter().map(|v| normalize(v, roots)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| {
                    let v = if key == "startedAt" {
                        json!("<TIME>")
                    } else {
                        normalize(v, roots)
                    };
                    (key.clone(), v)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn run_node(project: &Path, words: &[&str]) -> Outcome {
    let out = node_raw(project, words);
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.trim().is_empty() {
        let value: Value =
            serde_json::from_str(&stdout).unwrap_or_else(|_| Value::String(stdout.to_string()));
        return Outcome::Ok(normalize(&value, &[project]), code);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(code, 1, "unexpected Node exit with no output: {stderr}");
    let message = stderr
        .trim_end()
        .strip_prefix("dtech-aidlc: ")
        .unwrap_or(stderr.trim_end());
    Outcome::Err(normalize_text(message, &[project]))
}

fn run_rust(project: &Path, words: &[&str]) -> Outcome {
    let argv: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let parsed = parse_args(&argv).unwrap();
    let key = command_key(&parsed.positional).unwrap();
    let count = key.split(' ').count();
    let args = parsed.positional[count.min(parsed.positional.len())..].to_vec();
    match config_commands::run(&key, project, &args, &parsed.flags) {
        Ok((value, code)) => Outcome::Ok(normalize(&value, &[project]), code),
        Err(message) => Outcome::Err(normalize_text(&message, &[project])),
    }
}

/// Every file under the project except the audit log, as relative path -> bytes.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if entry.file_type().unwrap().is_dir() {
                out.insert(format!("{rel}/"), Vec::new());
                walk(root, &path, out);
            } else if rel != ".aidlc/audit.jsonl" {
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn audit_rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row.as_object_mut().unwrap().remove("at");
            row
        })
        .collect()
}

/// Two identical projects; `node` and `rust` receive the same commands in order.
struct Pair {
    node: Dir,
    rust: Dir,
}

impl Pair {
    fn from(template: &Path) -> Self {
        let pair = Pair {
            node: Dir::new("node"),
            rust: Dir::new("rust"),
        };
        copy_tree(template, &pair.node.0);
        copy_tree(template, &pair.rust.0);
        pair
    }
    fn empty() -> Self {
        Pair {
            node: Dir::new("node"),
            rust: Dir::new("rust"),
        }
    }
    fn write(&self, rel: &str, text: &str) {
        for root in [&self.node.0, &self.rust.0] {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }
    /// Run one command on both projects and compare output, files and audit rows.
    fn step(&self, words: &[&str]) -> Outcome {
        let expected = run_node(&self.node.0, words);
        let actual = run_rust(&self.rust.0, words);
        assert_eq!(actual, expected, "output differs for {words:?}");
        assert_eq!(
            snapshot(&self.rust.0),
            snapshot(&self.node.0),
            "project files differ after {words:?}"
        );
        assert_eq!(
            audit_rows(&self.rust.0),
            audit_rows(&self.node.0),
            "audit rows differ after {words:?}"
        );
        actual
    }
}

fn expect_err(outcome: &Outcome) {
    assert!(
        matches!(outcome, Outcome::Err(_)),
        "expected an error, got {outcome:?}"
    );
}

fn expect_ok(outcome: &Outcome) {
    assert!(
        matches!(outcome, Outcome::Ok(..)),
        "expected success, got {outcome:?}"
    );
}

/// Link `link` to directory `target`: a symlink, or on Windows without symlink rights a
/// junction (both Node and Rust treat a junction as a symbolic link).
fn link_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
            || Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()
                .is_ok_and(|out| out.status.success())
    }
}

macro_rules! require_node {
    () => {
        if !node_available() {
            eprintln!("node is not on PATH; skipping Node parity comparison");
            return;
        }
    };
}

#[test]
fn config_scope_parity() {
    require_node!();
    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    expect_ok(&pair.step(&["config", "scope"]));
    expect_err(&pair.step(&["config", "scope", "galaxy"]));
    expect_ok(&pair.step(&["config", "scope", "solo"]));
    expect_ok(&pair.step(&["config", "scope", "team"]));
    expect_ok(&pair.step(&["config", "scope"]));
    expect_ok(&pair.step(&["config", "scope", "enterprise"]));

    let fresh = Pair::empty();
    expect_ok(&fresh.step(&["config", "scope"]));
    expect_err(&fresh.step(&["config", "scope", "team"]));
    assert!(!fresh.rust.0.join(".aidlc").exists());

    let broken = Pair::empty();
    broken.write(".aidlc/config.json", r#"{"scope":"solo"}"#);
    expect_ok(&broken.step(&["config", "scope"]));
    expect_err(&broken.step(&["config", "scope", "team"]));
    broken.write(".aidlc/config.json", "{not json");
    expect_ok(&broken.step(&["config", "scope"]));
    expect_err(&broken.step(&["config", "scope", "team"]));
}

#[test]
fn config_mode_parity() {
    require_node!();
    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    expect_ok(&pair.step(&["config", "mode"]));
    expect_err(&pair.step(&["config", "mode", "robot"]));
    expect_ok(&pair.step(&["config", "mode", "human"]));
    expect_ok(&pair.step(&["config", "mode", "ai"]));
    expect_ok(&pair.step(&["config", "mode", "ai"]));
    expect_ok(&pair.step(&["config", "mode"]));
    expect_ok(&pair.step(&["config", "mode", "human"]));

    let fresh = Pair::empty();
    expect_ok(&fresh.step(&["config", "mode"]));
    expect_err(&fresh.step(&["config", "mode", "ai"]));

    let odd = Pair::empty();
    odd.write(
        ".aidlc/config.json",
        r#"{"harness":"claude","scope":"team","mode":"robot","extra":[1,2]}"#,
    );
    expect_ok(&odd.step(&["config", "mode"]));
    expect_ok(&odd.step(&["config", "mode", "ai"]));
}

#[test]
fn config_gate_mode_parity() {
    require_node!();
    let gates: Vec<String> = graph()["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["gate"].as_str().unwrap().to_owned())
        .collect();
    let (first, second) = (gates[0].as_str(), gates[2].as_str());
    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    expect_ok(&pair.step(&["config", "gate-mode"]));
    expect_ok(&pair.step(&["config", "gate-mode", first]));
    expect_err(&pair.step(&["config", "gate-mode", "no-such-gate"]));
    expect_err(&pair.step(&["config", "gate-mode", first, "ai"]));
    expect_ok(&pair.step(&["config", "gate-mode", second, "human"]));
    expect_ok(&pair.step(&["config", "gate-mode", first, "human"]));
    expect_ok(&pair.step(&["config", "gate-mode", first, "human"]));
    expect_ok(&pair.step(&["config", "gate-mode"]));
    expect_ok(&pair.step(&["config", "gate-mode", second, "default"]));
    expect_ok(&pair.step(&["config", "gate-mode", second, "default"]));
    expect_ok(&pair.step(&["config", "gate-mode", first, "default"]));
    expect_ok(&pair.step(&["config", "gate-mode"]));

    let fresh = Pair::empty();
    expect_ok(&fresh.step(&["config", "gate-mode"]));
    expect_err(&fresh.step(&["config", "gate-mode", first, "human"]));
    let broken = Pair::empty();
    broken.write(".aidlc/config.json", r#"{"harness":"nope","scope":"solo"}"#);
    expect_err(&broken.step(&["config", "gate-mode", first, "human"]));
}

#[test]
fn config_prerequisite_parity() {
    require_node!();
    let graph = graph();
    let stage = graph["stages"][0]["id"].as_str().unwrap().to_owned();
    let phase = graph["phases"][1]["id"].as_str().unwrap().to_owned();
    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    expect_ok(&pair.step(&["config", "prerequisite"]));
    expect_err(&pair.step(&["config", "prerequisite", "set"]));
    expect_err(&pair.step(&["config", "prerequisite", "set", "docker"]));
    expect_err(&pair.step(&[
        "config",
        "prerequisite",
        "set",
        "Bad_Name",
        "--check",
        "exit 0",
    ]));
    expect_err(&pair.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
        "--applies-to",
        "nowhere",
    ]));
    let set = pair.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
        "--hint",
        "Start Docker",
        "--applies-to",
        &stage,
        "--applies-to",
        &phase,
        "--applies-to",
        &stage,
    ]);
    assert_eq!(
        set,
        Outcome::Ok(
            json!({"name":"docker","check":"exit 0","hint":"Start Docker","stages":[stage, phase]}),
            0
        )
    );
    expect_ok(&pair.step(&["config", "prerequisite", "set", "db", "--check", "exit 3"]));
    expect_ok(&pair.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
    ]));
    expect_ok(&pair.step(&["config", "prerequisite"]));
    let ok = pair.step(&["config", "prerequisite", "check", "docker"]);
    assert!(matches!(ok, Outcome::Ok(_, 0)), "{ok:?}");
    let failing = pair.step(&["config", "prerequisite", "check"]);
    assert!(matches!(failing, Outcome::Ok(_, 1)), "{failing:?}");
    expect_err(&pair.step(&["config", "prerequisite", "check", "ghost"]));
    expect_err(&pair.step(&["config", "prerequisite", "remove"]));
    expect_err(&pair.step(&["config", "prerequisite", "remove", "ghost"]));
    expect_err(&pair.step(&["config", "prerequisite", "frobnicate"]));
    expect_ok(&pair.step(&["config", "prerequisite", "remove", "db"]));
    expect_ok(&pair.step(&["config", "prerequisite", "remove", "docker"]));
    expect_ok(&pair.step(&["config", "prerequisite"]));

    let fresh = Pair::empty();
    expect_ok(&fresh.step(&["config", "prerequisite"]));
    expect_err(&fresh.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
    ]));
    expect_err(&fresh.step(&["config", "prerequisite", "check", "docker"]));
    expect_ok(&fresh.step(&["config", "prerequisite", "check"]));
}

#[test]
fn config_agents_parity() {
    require_node!();
    let agent = graph()["agents"][0]["id"].as_str().unwrap().to_owned();
    let other = graph()["agents"][2]["id"].as_str().unwrap().to_owned();
    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    expect_ok(&pair.step(&["config", "agents"]));
    expect_ok(&pair.step(&["config", "agents", "set", &agent, "opus"]));
    expect_ok(&pair.step(&["config", "agents", "set", &other, "haiku"]));
    expect_err(&pair.step(&["config", "agents", "set", &agent, "gpt"]));
    expect_err(&pair.step(&["config", "agents", "set", &agent]));
    expect_err(&pair.step(&["config", "agents", "set", "nobody", "opus"]));
    expect_err(&pair.step(&["config", "agents", "set"]));
    expect_err(&pair.step(&["config", "agents", "frobnicate"]));
    expect_ok(&pair.step(&["config", "agents"]));
    expect_ok(&pair.step(&["config", "agents", "reset", &agent]));
    expect_ok(&pair.step(&["config", "agents", "reset", &other]));
    expect_ok(&pair.step(&["config", "agents", "reset", &other]));
    expect_ok(&pair.step(&["config", "agents"]));

    let deepseek = Pair::from(&onboarded("deepseek").0);
    expect_ok(&deepseek.step(&["config", "agents"]));
    expect_err(&deepseek.step(&["config", "agents", "set", &agent, "opus"]));
    expect_ok(&deepseek.step(&["config", "agents", "reset", &agent]));

    let fresh = Pair::empty();
    expect_ok(&fresh.step(&["config", "agents"]));
    expect_err(&fresh.step(&["config", "agents", "set", &agent, "opus"]));

    let broken = Pair::empty();
    for text in [
        "{not json",
        "[]",
        r#"{"harness":"claude","scope":"solo","mode":"robot"}"#,
        r#"{"harness":"claude","scope":"solo","agentModels":{"nobody":"opus"}}"#,
        &format!(r#"{{"harness":"claude","scope":"solo","agentModels":{{"{agent}":7}}}}"#),
        r#"{"harness":"claude","scope":"solo","gateModes":{"nope":"human"}}"#,
        r#"{"harness":"claude","scope":"solo","unitSplitThreshold":0}"#,
        r#"{"harness":"claude","scope":"solo","roles":{"owner":"me"}}"#,
    ] {
        broken.write(".aidlc/config.json", text);
        expect_err(&broken.step(&["config", "agents"]));
    }
    // Valid but not initialized (no initializedAt): listing works, changes are refused.
    broken.write(
        ".aidlc/config.json",
        r#"{"harness":"claude","scope":"solo"}"#,
    );
    expect_ok(&broken.step(&["config", "agents"]));
    expect_err(&broken.step(&["config", "agents", "set", &agent, "opus"]));
}

#[test]
fn config_unknown_section_parity() {
    require_node!();
    let pair = Pair::empty();
    expect_err(&pair.step(&["config"]));
    expect_err(&pair.step(&["config", "colour"]));
}

#[test]
fn config_refuses_symlinked_state_dir() {
    let real = Dir::new("real");
    let project = Dir::new("link");
    fs::create_dir_all(real.0.join(".aidlc")).unwrap();
    fs::write(
        real.0.join(".aidlc/config.json"),
        r#"{"harness":"claude","scope":"solo","initializedAt":"x"}"#,
    )
    .unwrap();
    if !link_dir(&real.0.join(".aidlc"), &project.0.join(".aidlc")) {
        eprintln!("symlinks unavailable; skipping");
        return;
    }
    let before = fs::read(real.0.join(".aidlc/config.json")).unwrap();
    let outcome = run_rust(&project.0, &["config", "scope", "team"]);
    assert_eq!(
        outcome,
        Outcome::Err("Refusing to access symlink .aidlc directory".into())
    );
    if node_available() {
        assert_eq!(run_node(&project.0, &["config", "scope", "team"]), outcome);
    }
    assert_eq!(fs::read(real.0.join(".aidlc/config.json")).unwrap(), before);
    assert!(!real.0.join(".aidlc/audit.jsonl").exists());
}
