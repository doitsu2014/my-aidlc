//! Parity tests for the native `harness add|status|refresh` commands against the Node CLI.
//!
//! Each scenario builds one fixture (a Node-onboarded project, optionally edited), copies it
//! to two temp projects, runs the Node CLI on one and `dtech_aidlc::harness_commands::run` on
//! the other, and compares the JSON output (project paths normalised), error texts, exit
//! codes, every project file byte for byte (skill files, `.projection.json`, config, legacy
//! copies; `audit.jsonl` excepted), and the new audit rows without their `at` field.
//! Node comparisons are skipped when `node` is not on PATH.

use dtech_aidlc::args::{command_key, parse_args};
use dtech_aidlc::harness_commands;
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
            "aidlc-harness-parity-{tag}-{}-{}",
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
    match harness_commands::run(&key, project, &args, &parsed.flags) {
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

impl Pair {
    fn each(&self, f: impl Fn(&Path)) {
        f(&self.node.0);
        f(&self.rust.0);
    }
    fn read(&self, rel: &str) -> Vec<u8> {
        fs::read(self.node.0.join(rel)).unwrap()
    }
    /// Replace fields of a projection stamp in both projects.
    fn edit_stamp(&self, rel: &str, edit: impl Fn(&mut Value)) {
        let mut stamp: Value = serde_json::from_slice(&self.read(rel)).unwrap();
        edit(&mut stamp);
        self.write(
            rel,
            &format!("{}\n", serde_json::to_string_pretty(&stamp).unwrap()),
        );
    }
}

const CLAUDE_DIR: &str = ".claude/skills/dtech-aidlc";
const CLAUDE_STAMP: &str = ".claude/skills/dtech-aidlc/.projection.json";

fn result_of(outcome: &Outcome) -> (&Value, i32) {
    match outcome {
        Outcome::Ok(value, code) => (value, *code),
        Outcome::Err(message) => panic!("expected success, got error: {message}"),
    }
}

fn actions(value: &Value, harness: usize) -> Vec<String> {
    value["harnesses"][harness]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["action"].as_str().unwrap().to_owned())
        .collect()
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn harness_add_parity() {
    require_node!();
    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    expect_err(&pair.step(&["harness", "add", "bogus"]));
    expect_err(&pair.step(&["harness", "add"]));
    let same = pair.step(&["harness", "add", "claude"]);
    assert_eq!(result_of(&same).0["filesCreated"], 0);
    let added = pair.step(&["harness", "add", "deepseek"]);
    assert_eq!(
        result_of(&added).0["harnesses"],
        json!(["claude", "deepseek"])
    );
    assert_eq!(result_of(&added).0["filesCreated"], 2);
    expect_ok(&pair.step(&["harness", "add", "deepseek"]));
    pair.each(|root| fs::remove_file(root.join(".dsh/skills/aidlc/onboarding.md")).unwrap());
    let again = pair.step(&["harness", "add", "deepseek"]);
    assert_eq!(result_of(&again).0["filesCreated"], 1);
    expect_ok(&pair.step(&["harness", "status"]));

    let fresh = Pair::empty();
    expect_err(&fresh.step(&["harness", "add", "claude"]));
    expect_err(&fresh.step(&["harness", "add", "bogus"]));
    assert!(!fresh.rust.0.join(".claude").exists());

    let broken = Pair::empty();
    broken.write(".aidlc/config.json", r#"{"scope":"solo"}"#);
    expect_err(&broken.step(&["harness", "add", "claude"]));
    broken.write(".aidlc/config.json", "{not json");
    expect_err(&broken.step(&["harness", "add", "claude"]));
    assert!(!broken.rust.0.join(".claude").exists());

    // A single-harness config without `harnesses`: the list is appended after other keys.
    let legacy = Pair::empty();
    legacy.write(
        ".aidlc/config.json",
        "{\n  \"harness\": \"claude\",\n  \"scope\": \"team\",\n  \"custom\": {\"keep\": 1}\n}\n",
    );
    expect_ok(&legacy.step(&["harness", "add", "deepseek"]));
    expect_ok(&legacy.step(&["harness", "add", "claude"]));
    expect_ok(&legacy.step(&["harness", "status"]));
}

#[test]
fn harness_status_parity() {
    require_node!();
    let fresh = Pair::empty();
    let empty = fresh.step(&["harness", "status"]);
    assert_eq!(result_of(&empty).0["harnesses"], json!([]));

    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    let current = pair.step(&["harness", "status"]);
    assert_eq!(result_of(&current).0["harnesses"][0]["state"], "current");

    let skill = pair.read(&format!("{CLAUDE_DIR}/SKILL.md"));
    let onboarding = pair.read(&format!("{CLAUDE_DIR}/onboarding.md"));
    pair.write(&format!("{CLAUDE_DIR}/manifest.mjs"), "// user file\n");
    pair.write(".claude/skills/aidlc/SKILL.md", text(&skill));
    pair.write(".claude/skills/aidlc/manifest.mjs", "// kept\n");
    let archived = fs::read(repo().join("core/methodology/phases/archived.md")).unwrap();
    pair.write(".aidlc/methodology/phases/archived.md", text(&archived));
    pair.write(".aidlc/methodology/notes.md", "my notes\n");
    let obsolete = pair.step(&["harness", "status"]);
    let (value, _) = result_of(&obsolete);
    assert_eq!(value["harnesses"][0]["state"], "modified");
    assert_eq!(
        value["legacyMethodology"],
        json!({"pristine": ["phases/archived.md"], "modified": ["notes.md"]})
    );

    pair.write(&format!("{CLAUDE_DIR}/SKILL.md"), "edited locally\n");
    expect_ok(&pair.step(&["harness", "status"]));
    pair.each(|root| fs::remove_file(root.join(CLAUDE_DIR).join("onboarding.md")).unwrap());
    let missing = pair.step(&["harness", "status"]);
    assert_eq!(result_of(&missing).0["harnesses"][0]["state"], "missing");
    pair.write(&format!("{CLAUDE_DIR}/onboarding.md"), text(&onboarding));
    pair.edit_stamp(CLAUDE_STAMP, |stamp| {
        stamp["engineVersion"] = json!("99.0.0")
    });
    let ahead = pair.step(&["harness", "status"]);
    assert_eq!(result_of(&ahead).0["harnesses"][0]["state"], "ahead");
}

#[test]
fn harness_refresh_parity() {
    require_node!();
    let fresh = Pair::empty();
    expect_err(&fresh.step(&["harness", "refresh"]));
    expect_err(&fresh.step(&["harness"]));
    expect_err(&fresh.step(&["harness", "bogus"]));

    let template = onboarded("claude");
    let pair = Pair::from(&template.0);
    let noop = pair.step(&["harness", "refresh"]);
    assert_eq!(result_of(&noop).1, 0);
    assert_eq!(result_of(&noop).0["needsAttention"], false);
    expect_ok(&pair.step(&["harness", "refresh", "claude"]));
    expect_err(&pair.step(&["harness", "refresh", "deepseek"]));

    // behind SKILL.md, locally edited onboarding.md, a pristine obsolete file, pristine
    // legacy copies and legacy methodology files.
    let onboarding = pair.read(&format!("{CLAUDE_DIR}/onboarding.md"));
    pair.write(&format!("{CLAUDE_DIR}/SKILL.md"), "old skill\n");
    pair.write(
        &format!("{CLAUDE_DIR}/onboarding.md"),
        "my onboarding notes\n",
    );
    pair.write(
        &format!("{CLAUDE_DIR}/manifest.mjs"),
        "export const old = 1;\n",
    );
    pair.edit_stamp(CLAUDE_STAMP, |stamp| {
        stamp["engineVersion"] = json!("0.6.0");
        stamp["files"]["SKILL.md"]["sha256"] = json!(harness_commands::hash_text(b"old skill\n"));
        stamp["files"]["manifest.mjs"] =
            json!({"sha256": harness_commands::hash_text(b"export const old = 1;\n")});
    });
    // Legacy copies are judged against the stamp entry of the same name ("old skill").
    pair.write(".claude/skills/aidlc/SKILL.md", "old skill\n");
    pair.write(".claude/skills/aidlc/onboarding.md", text(&onboarding));
    let archived = fs::read(repo().join("core/methodology/phases/archived.md")).unwrap();
    pair.write(".aidlc/methodology/phases/archived.md", text(&archived));
    pair.write(".aidlc/methodology/notes.md", "my notes\n");

    let dry = pair.step(&["harness", "refresh", "--dry-run"]);
    assert_eq!(result_of(&dry).0["dryRun"], true);
    let real = pair.step(&["harness", "refresh"]);
    let (value, code) = result_of(&real);
    assert_eq!(code, 0);
    assert_eq!(
        actions(value, 0),
        [
            "updated",
            "kept-modified",
            "removed",
            "removed",
            "removed",
            "removed-directory"
        ]
    );
    assert_eq!(
        value["legacyMethodology"]["removed"],
        json!(["phases/archived.md"])
    );
    assert_eq!(value["needsAttention"], true);

    let forced = pair.step(&["harness", "refresh", "--force"]);
    assert_eq!(actions(result_of(&forced).0, 0), ["replaced-with-backup"]);
    pair.write(&format!("{CLAUDE_DIR}/onboarding.md"), "second edit\n");
    let second = pair.step(&["harness", "refresh", "--force", "--dry-run"]);
    assert_eq!(actions(result_of(&second).0, 0), ["replaced-with-backup"]);
    expect_ok(&pair.step(&["harness", "refresh", "claude", "--force"]));
    expect_ok(&pair.step(&["harness", "status"]));

    // A stamp from a newer engine with a non-current file is refused unless forced.
    pair.write(&format!("{CLAUDE_DIR}/SKILL.md"), "newer engine skill\n");
    pair.edit_stamp(CLAUDE_STAMP, |stamp| {
        stamp["engineVersion"] = json!("99.0.0")
    });
    let refused = pair.step(&["harness", "refresh"]);
    let (value, code) = result_of(&refused);
    assert_eq!(code, 1);
    assert_eq!(actions(value, 0), ["refused-ahead"]);
    let overridden = pair.step(&["harness", "refresh", "--force"]);
    assert_eq!(result_of(&overridden).1, 0);

    // A directory where a skill file belongs is never replaced.
    pair.each(|root| {
        let path = root.join(CLAUDE_DIR).join("SKILL.md");
        fs::remove_file(&path).unwrap();
        fs::create_dir_all(path.join("inner")).unwrap();
    });
    let blocked = pair.step(&["harness", "refresh", "--force"]);
    let (value, code) = result_of(&blocked);
    assert_eq!(code, 1);
    assert_eq!(actions(value, 0), ["refused-not-a-regular-file"]);
}

#[test]
fn harness_refresh_multiple_harnesses_parity() {
    require_node!();
    let template = onboarded("deepseek");
    let pair = Pair::from(&template.0);
    expect_ok(&pair.step(&["harness", "add", "claude"]));
    pair.write(
        ".dsh/skills/aidlc/skills/aidlc/SKILL.md",
        "stale nested copy\n",
    );
    pair.each(|root| fs::remove_file(root.join(CLAUDE_DIR).join("SKILL.md")).unwrap());
    expect_ok(&pair.step(&["harness", "status"]));
    let refreshed = pair.step(&["harness", "refresh"]);
    let (value, _) = result_of(&refreshed);
    assert_eq!(actions(value, 0), ["kept-modified-obsolete"]);
    assert_eq!(actions(value, 1), ["created"]);
    expect_ok(&pair.step(&["harness", "refresh", "deepseek", "--force"]));
}

#[test]
fn harness_refuses_symlinked_skill_directory() {
    let template = Dir::new("template");
    fs::create_dir_all(template.0.join(".aidlc")).unwrap();
    fs::write(
        template.0.join(".aidlc/config.json"),
        r#"{"harness":"claude","scope":"solo"}"#,
    )
    .unwrap();
    let outside = Dir::new("outside");
    let pair = Pair::from(&template.0);
    pair.each(|root| {
        fs::create_dir_all(root.join(".claude/skills")).unwrap();
        let _ = link_dir(&outside.0, &root.join(CLAUDE_DIR));
    });
    let linked = [&pair.node.0, &pair.rust.0].iter().all(|root| {
        fs::symlink_metadata(root.join(CLAUDE_DIR)).is_ok_and(|meta| meta.file_type().is_symlink())
    });
    if !linked {
        eprintln!("symlinks unavailable; skipping");
        return;
    }
    let refused = run_rust(&pair.rust.0, &["harness", "refresh"]);
    assert_eq!(
        refused,
        Outcome::Err("Refusing to write through symlink: .claude/skills/dtech-aidlc".into())
    );
    let add = run_rust(&pair.rust.0, &["harness", "add", "claude"]);
    assert_eq!(add, refused);
    if node_available() {
        assert_eq!(run_node(&pair.node.0, &["harness", "refresh"]), refused);
        assert_eq!(run_node(&pair.node.0, &["harness", "add", "claude"]), add);
    }
    assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
    assert_eq!(snapshot(&pair.rust.0), snapshot(&pair.node.0));
}
