//! Node-vs-Rust parity for `dtech-aidlc knowledge ...` (and the `knowledge_preflight`
//! fix that lets `orchestrate next` report Node's messages for a missing project).
//!
//! Each scenario builds the same fixture twice, runs every step with the Node reference
//! CLI on one copy and with `dtech_aidlc::knowledge_commands::run` on the other, then
//! compares exit codes, JSON output, error text, every project file and the audit rows
//! written by the steps. Only the two project roots and ISO timestamps are normalised.
//! The comparison is skipped (with a note) when `node` is not on PATH.

use dtech_aidlc::args::{command_key, parse_args};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

macro_rules! require_node {
    () => {
        if !node_available() {
            eprintln!("node is not on PATH; skipping Node parity comparison");
            return;
        }
    };
}

/// A unique temporary directory outside the repository, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aidlc-kc-{label}-{}-{}-{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, PartialEq)]
struct Outcome {
    code: i32,
    stdout: Value,
    stderr: String,
}

fn node_cli(root: &Path, argv: &[&str]) -> Outcome {
    let out = Command::new("node")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core/tools/dtech-aidlc.mjs"))
        .args(argv)
        .arg("--project")
        .arg(root)
        .current_dir(std::env::temp_dir())
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run node");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let stdout = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };
    Outcome {
        code: out.status.code().unwrap_or(-1),
        stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Parse `argv` exactly like the binary does and call the native handler directly.
fn rust_cli(root: &Path, argv: &[&str]) -> Outcome {
    let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_owned()).collect();
    let parsed = parse_args(&argv).expect("valid arguments");
    let key = command_key(&parsed.positional).expect("a command");
    let words = key.split(' ').count().min(parsed.positional.len());
    let args = parsed.positional[words..].to_vec();
    let result = match parsed.positional[0].as_str() {
        "knowledge" => dtech_aidlc::knowledge_commands::run(&key, root, &args, &parsed.flags),
        other => panic!("unexpected command {other}"),
    };
    match result {
        Ok((stdout, code)) => Outcome {
            code,
            stdout,
            stderr: String::new(),
        },
        Err(message) => Outcome {
            code: 1,
            stdout: Value::Null,
            stderr: format!("dtech-aidlc: {message}\n"),
        },
    }
}

fn root_variants(root: &Path) -> Vec<String> {
    let mut variants = vec![root.display().to_string()];
    if let Ok(canonical) = root.canonicalize() {
        let text = canonical.display().to_string();
        variants.push(text.trim_start_matches(r"\\?\").to_owned());
    }
    let more: Vec<String> = variants.iter().map(|v| v.replace('\\', "/")).collect();
    variants.extend(more);
    variants.sort_by_key(|v| std::cmp::Reverse(v.len()));
    variants.dedup();
    variants
}

/// Replace ISO-8601 timestamps such as `2026-01-02T03:04:05.678Z` with `<TS>`.
fn mask_timestamps(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let digit = |b: Option<&u8>| b.is_some_and(u8::is_ascii_digit);
    while i < bytes.len() {
        let shape = b"dddd-dd-ddTdd:dd:dd";
        let matches_shape = i + shape.len() <= bytes.len()
            && shape.iter().enumerate().all(|(k, s)| match s {
                b'd' => digit(bytes.get(i + k)),
                other => bytes[i + k] == *other,
            });
        if matches_shape {
            let mut j = i + shape.len();
            if bytes.get(j) == Some(&b'.') {
                j += 1;
                while digit(bytes.get(j)) {
                    j += 1;
                }
            }
            if bytes.get(j) == Some(&b'Z') {
                j += 1;
            }
            out.push_str("<TS>");
            i = j;
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn normalize_text(text: &str, root: &Path) -> String {
    let mut text = text.to_owned();
    for variant in root_variants(root) {
        text = text.replace(&variant, "<ROOT>");
    }
    mask_timestamps(&text)
}

fn normalize_value(value: &Value, root: &Path) -> Value {
    match value {
        Value::String(s) => Value::String(normalize_text(s, root)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| normalize_value(v, root)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), normalize_value(v, root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn mask_mtimes(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "mtimeMs" {
                    *item = json!("<MTIME>");
                } else {
                    mask_mtimes(item);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask_mtimes),
        _ => {}
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                if entry.file_name() == ".git" {
                    continue;
                }
                out.insert(format!("{rel}/"), String::new());
                walk(base, &path, out);
            } else if rel != ".aidlc/audit.jsonl" {
                let bytes = fs::read(&path).unwrap_or_default();
                let mut text = normalize_text(&String::from_utf8_lossy(&bytes), base);
                // Setup-time file modification times are not produced by the commands.
                if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
                    mask_mtimes(&mut value);
                    text = serde_json::to_string_pretty(&value).unwrap();
                }
                out.insert(rel, text);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn assert_same_files(rust: &Path, node: &Path, argv: &[&str]) {
    let (rust, node) = (snapshot(rust), snapshot(node));
    let differing: Vec<(&String, Option<&String>, Option<&String>)> = rust
        .keys()
        .chain(node.keys())
        .filter(|key| rust.get(*key) != node.get(*key))
        .map(|key| (key, rust.get(key), node.get(key)))
        .collect();
    assert!(
        differing.is_empty(),
        "files differ after {argv:?} (path, rust, node): {differing:#?}"
    );
}

fn audit_rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row.as_object_mut().unwrap().remove("at");
            normalize_value(&row, root)
        })
        .collect()
}

/// One project per CLI, prepared identically.
struct Pair {
    node: TempDir,
    rust: TempDir,
    node_audit: usize,
    rust_audit: usize,
}

impl Pair {
    fn new(label: &str, setup: impl Fn(&Path)) -> Self {
        let node = TempDir::new(&format!("{label}-node"));
        let rust = TempDir::new(&format!("{label}-rust"));
        setup(&node.0);
        setup(&rust.0);
        let node_audit = audit_rows(&node.0).len();
        let rust_audit = audit_rows(&rust.0).len();
        Pair {
            node,
            rust,
            node_audit,
            rust_audit,
        }
    }

    /// Run one step on both projects and require identical observable results.
    fn step(&mut self, argv: &[&str]) -> Outcome {
        let node = node_cli(&self.node.0, argv);
        let rust = rust_cli(&self.rust.0, argv);
        let normalize = |o: &Outcome, root: &Path| Outcome {
            code: o.code,
            stdout: normalize_value(&o.stdout, root),
            stderr: normalize_text(&o.stderr, root),
        };
        let (node, rust) = (
            normalize(&node, &self.node.0),
            normalize(&rust, &self.rust.0),
        );
        assert_eq!(rust, node, "output differs for {argv:?}");
        assert_same_files(&self.rust.0, &self.node.0, argv);
        let node_rows = audit_rows(&self.node.0)[self.node_audit..].to_vec();
        let rust_rows = audit_rows(&self.rust.0)[self.rust_audit..].to_vec();
        assert_eq!(rust_rows, node_rows, "audit rows differ after {argv:?}");
        rust
    }
}

fn onboard(root: &Path) {
    let out = Command::new("node")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core/tools/dtech-aidlc.mjs"))
        .args([
            "onboard",
            "--harness",
            "claude",
            "--scope",
            "solo",
            "--project",
        ])
        .arg(root)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

const COMPLETE: &str = "---\nid: alpha\ntitle: Alpha record\norigin: archived\nclosed: 2026-01-01\nscope: solo\ntags: [one, two ,, three]\nsupersedes: old-1\n---\n# Alpha\n\n## Outcome\nShipped.\n## Decisions\nD\n## Limitations\nL\n## Follow-ups\nF\n## Links\nNone\n";

fn write_records(root: &Path) {
    let dir = root.join(".aidlc/knowledge");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("alpha.md"), COMPLETE).unwrap();
    // Missing headings, quoted values, CRLF line endings, H1 title fallback.
    fs::write(
        dir.join("beta.md"),
        "---\r\nid: \"beta\"\r\ntitle: ''\r\n  indented: ignored\r\nweird key: ignored\r\nphases-run: analyze, develop\r\n---\r\n# Beta heading  \r\n## Outcome\r\nPartial\r\n",
    )
    .unwrap();
    // No frontmatter: a legacy record.
    fs::write(dir.join("gamma.md"), "# Gamma\n\nPlain notes.\n").unwrap();
    // Frontmatter closed at end of file, empty values fall back.
    fs::write(dir.join("delta.md"), "---\nid:\norigin:\n---").unwrap();
    // Empty frontmatter block requires a newline before the closing fence.
    fs::write(dir.join("epsilon.md"), "---\n---\n# E\n").unwrap();
    // Intent-origin record (not checked for headings) with invalid UTF-8 bytes.
    fs::write(
        dir.join("zeta.md"),
        b"---\nid: zeta\norigin: intent\ntitle: Z \xff\n---\n# Zeta\n".as_slice(),
    )
    .unwrap();
    // Skipped and invalid entries.
    fs::write(dir.join("_index.md"), "old index").unwrap();
    fs::write(dir.join(".hidden.md"), "hidden").unwrap();
    fs::write(dir.join("notes.txt"), "not markdown").unwrap();
    fs::write(dir.join("-bad.md"), "---\nid: bad\n---\n").unwrap();
    fs::create_dir_all(dir.join("folder.md")).unwrap();
}

#[test]
fn onboarded_project_commands_match_node() {
    require_node!();
    let mut pair = Pair::new("onboarded", onboard);
    for argv in [
        vec!["knowledge", "sources"],
        vec!["knowledge", "sources", "list"],
        vec!["knowledge", "list"],
        vec!["knowledge", "check"],
        vec!["knowledge", "show", "missing"],
        vec!["knowledge", "show"],
        vec!["knowledge", "init"],
        vec!["knowledge", "index"],
        vec!["knowledge", "list"],
        vec!["knowledge", "bogus"],
        vec!["knowledge", "sources", "bogus"],
    ] {
        pair.step(&argv);
    }
}

#[test]
fn records_list_show_check_and_index_match_node() {
    require_node!();
    let mut pair = Pair::new("records", |root| {
        onboard(root);
        write_records(root);
    });
    let listed = pair.step(&["knowledge", "list"]);
    assert_eq!(listed.stdout["invalid"].as_array().unwrap().len(), 2);
    for argv in [
        vec!["knowledge", "show", "alpha"],
        vec!["knowledge", "show", "beta"],
        vec!["knowledge", "show", "gamma"],
        vec!["knowledge", "show", "delta"],
        vec!["knowledge", "show", "epsilon"],
        vec!["knowledge", "show", "zeta"],
        vec!["knowledge", "show", "bad"],
        vec!["knowledge", "check"],
        vec!["knowledge", "index"],
    ] {
        pair.step(&argv);
    }
    let checked = pair.step(&["knowledge", "check"]);
    assert_eq!(checked.code, 0);
    assert_eq!(checked.stdout["ok"], json!(false));
}

#[test]
fn source_management_and_errors_match_node() {
    require_node!();
    let mut pair = Pair::new("sources", |root| {
        onboard(root);
        fs::create_dir_all(root.join("docs/adr")).unwrap();
        fs::write(root.join("docs/README.md"), "# Docs\n").unwrap();
    });
    for argv in [
        vec!["knowledge", "sources", "remove", "project"],
        vec![
            "knowledge",
            "sources",
            "add",
            "readme",
            "docs/README.md",
            "--kind",
            "readme",
            "--description",
            "Project readme",
        ],
        vec![
            "knowledge",
            "sources",
            "add",
            "adr",
            "docs/adr",
            "--kind",
            "adr",
        ],
        vec!["knowledge", "sources", "add", "later", "docs/missing.md"],
        vec!["knowledge", "sources"],
        vec!["knowledge", "sources", "add", "readme", "docs/README.md"],
        vec!["knowledge", "sources", "add", "up", "../outside"],
        vec!["knowledge", "sources", "add", "state", ".aidlc/state.json"],
        vec!["knowledge", "sources", "add", "git", ".git/config"],
        vec!["knowledge", "sources", "add", "drive", "C:/x"],
        vec!["knowledge", "sources", "add", "back", "docs\\README.md"],
        vec![
            "knowledge",
            "sources",
            "add",
            "kind",
            "docs",
            "--kind",
            "nope",
        ],
        vec!["knowledge", "sources", "add", "-bad", "docs"],
        vec![
            "knowledge",
            "sources",
            "add",
            "p2",
            "docs",
            "--kind",
            "project",
        ],
        vec!["knowledge", "sources", "add", "only-id"],
        vec!["knowledge", "sources", "remove"],
        vec!["knowledge", "sources", "remove", "unknown"],
        vec!["knowledge", "sources", "remove", "readme"],
        vec!["knowledge", "sources", "list"],
        vec!["knowledge", "init"],
    ] {
        pair.step(&argv);
    }
}

#[test]
fn uninitialized_and_missing_projects_match_node() {
    require_node!();
    // A bare directory: init creates PROJECT.md from the scaffold and .aidlc/knowledge.
    let mut bare = Pair::new("bare", |_| {});
    for argv in [
        vec!["knowledge", "sources", "remove", "project"],
        vec!["knowledge", "sources"],
        vec!["knowledge", "list"],
        vec!["knowledge", "init"],
        vec!["knowledge", "init"],
        vec!["knowledge", "sources"],
    ] {
        bare.step(&argv);
    }
    // A lowercase project.md is found and kept.
    let mut lower = Pair::new("lower", |root| {
        fs::write(root.join("project.md"), "# lower\n").unwrap();
    });
    lower.step(&["knowledge", "sources"]);
    lower.step(&["knowledge", "init"]);
    // A manifest naming a project file in a missing folder.
    let mut nested = Pair::new("nested", |root| {
        fs::write(
            root.join("aidlc.knowledge.json"),
            r#"{"schemaVersion":1,"sources":[{"id":"project","path":"docs/PROJECT.md","kind":"project"}]}"#,
        )
        .unwrap();
    });
    nested.step(&["knowledge", "init"]);
    // No project source declared.
    let mut none = Pair::new("noproject", |root| {
        fs::write(
            root.join("aidlc.knowledge.json"),
            r#"{"schemaVersion":1,"sources":[]}"#,
        )
        .unwrap();
    });
    none.step(&["knowledge", "init"]);
    none.step(&["knowledge", "sources"]);
    // A project root that does not exist: read-only commands behave like Node.
    let node_parent = TempDir::new("missing-node");
    let rust_parent = TempDir::new("missing-rust");
    let (node_root, rust_root) = (node_parent.0.join("absent"), rust_parent.0.join("absent"));
    for argv in [
        vec!["knowledge", "sources"],
        vec!["knowledge", "list"],
        vec!["knowledge", "check"],
        vec!["knowledge", "show", "x"],
    ] {
        let node = node_cli(&node_root, &argv);
        let rust = rust_cli(&rust_root, &argv);
        assert_eq!(
            Outcome {
                code: rust.code,
                stdout: normalize_value(&rust.stdout, &rust_root),
                stderr: normalize_text(&rust.stderr, &rust_root)
            },
            Outcome {
                code: node.code,
                stdout: normalize_value(&node.stdout, &node_root),
                stderr: normalize_text(&node.stderr, &node_root)
            },
            "{argv:?}"
        );
        assert!(!rust_root.exists(), "{argv:?} created the project");
    }
}

#[test]
fn invalid_manifests_and_unsafe_state_match_node() {
    require_node!();
    for (label, manifest) in [
        ("malformed", "{not json"),
        ("version", r#"{"schemaVersion":2,"sources":[]}"#),
        ("extra", r#"{"schemaVersion":1,"sources":[],"x":1}"#),
        (
            "dup",
            r#"{"schemaVersion":1,"sources":[{"id":"a","path":"a","kind":"docs"},{"id":"a","path":"b","kind":"docs"}]}"#,
        ),
        (
            "numeric-id",
            r#"{"schemaVersion":1,"sources":[{"id":5,"path":"a","kind":"docs"}]}"#,
        ),
        (
            "desc",
            r#"{"schemaVersion":1,"sources":[{"id":"a","path":"a","kind":"docs","description":"x\ny"}]}"#,
        ),
    ] {
        let mut pair = Pair::new(label, |root| {
            fs::write(root.join("aidlc.knowledge.json"), manifest).unwrap();
        });
        pair.step(&["knowledge", "sources"]);
        pair.step(&["knowledge", "init"]);
        pair.step(&["knowledge", "sources", "add", "x", "docs"]);
        pair.step(&["knowledge", "sources", "remove", "a"]);
    }
    // `.aidlc` that is a file is refused before anything is read or written.
    let mut file_state = Pair::new("aidlc-file", |root| {
        fs::write(root.join(".aidlc"), "not a directory").unwrap();
    });
    for argv in [
        vec!["knowledge", "init"],
        vec!["knowledge", "list"],
        vec!["knowledge", "index"],
        vec!["knowledge", "sources", "add", "x", "docs"],
    ] {
        let out = file_state.step(&argv);
        assert_eq!(out.code, 1);
    }
}

#[cfg(unix)]
#[test]
fn symlinked_state_is_refused_like_node() {
    require_node!();
    let outside = TempDir::new("outside");
    let target = outside.0.clone();
    let mut pair = Pair::new("symlink", move |root| {
        std::os::unix::fs::symlink(&target, root.join(".aidlc")).unwrap();
    });
    pair.step(&["knowledge", "init"]);
    pair.step(&["knowledge", "index"]);
    assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
}

/// The reviewed defect: `knowledge_preflight` on a project that does not exist used to
/// fail with the raw OS error. It now behaves like Node (no error, default manifest).
#[test]
fn knowledge_preflight_accepts_a_missing_project() {
    let parent = TempDir::new("preflight");
    let missing = parent.0.join("absent");
    let (exists, manifest) = dtech_aidlc::knowledge::knowledge_preflight(&missing).unwrap();
    assert!(!exists);
    assert_eq!(
        manifest,
        json!({"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]})
    );
    assert_eq!(
        dtech_aidlc::knowledge::resolve_project_root(&missing).unwrap(),
        dtech_aidlc::knowledge::ProjectRoot::Missing(missing.clone())
    );
    assert!(!missing.exists());
    let file = parent.0.join("file");
    fs::write(&file, "x").unwrap();
    assert_eq!(
        dtech_aidlc::knowledge::knowledge_preflight(&file).unwrap_err(),
        "Project root must be a directory"
    );
}

/// With the preflight fixed, the binary's `orchestrate next` reports Node's messages
/// for a project directory that does not exist.
#[test]
fn orchestrate_next_on_missing_project_matches_node() {
    require_node!();
    let node_parent = TempDir::new("orch-missing-node");
    let rust_parent = TempDir::new("orch-missing-rust");
    for extra in [vec!["--intent", "x"], vec![]] {
        let (node_root, rust_root) = (node_parent.0.join("absent"), rust_parent.0.join("absent"));
        let mut argv = vec!["orchestrate", "next"];
        argv.extend(extra.iter().copied());
        let node = node_cli(&node_root, &argv);
        let rust = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(&argv)
            .arg("--project")
            .arg(&rust_root)
            .current_dir(std::env::temp_dir())
            .env_remove("DTECH_AIDLC_HOME")
            .output()
            .unwrap();
        assert_eq!(rust.status.code(), Some(node.code), "{argv:?}");
        assert_eq!(
            normalize_text(&String::from_utf8_lossy(&rust.stderr), &rust_root),
            normalize_text(&node.stderr, &node_root),
            "{argv:?}"
        );
        assert!(!rust.stderr.is_empty());
    }
}
