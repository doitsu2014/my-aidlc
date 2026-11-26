//! Node-vs-Rust parity for `dtech-aidlc ids check`, `repos ...` and `deps ...`.
//!
//! Each scenario builds the same fixture twice (onboarded with the Node CLI, plus an
//! intent started with Node where needed), runs every step with the Node reference CLI
//! on one copy and with the native handler (`dtech_aidlc::ids::run`,
//! `dtech_aidlc::dependencies::run_repos` / `run_deps`) on the other, then compares
//! exit codes, JSON output, error text, every project file (Git internals excluded) and
//! the audit rows written by the steps. Only the two project roots, ISO timestamps and
//! setup-time file mtimes are normalised. Skipped when `node` is not on PATH;
//! `repos pull` additionally needs `git`.

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
            "aidlc-rdi-{label}-{}-{}-{nanos}",
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
        // Same working directory as this test process, so relative --file paths agree.
        .current_dir(std::env::current_dir().unwrap())
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
        "ids" => dtech_aidlc::ids::run(&key, root, &args, &parsed.flags),
        "repos" => dtech_aidlc::dependencies::run_repos(&key, root, &args, &parsed.flags),
        "deps" => dtech_aidlc::dependencies::run_deps(&key, root, &args, &parsed.flags),
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
    /// Node's raw standard error from the last step (notes included).
    last_node_stderr: String,
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
            last_node_stderr: String::new(),
        }
    }

    /// Run one step on both projects and require identical observable results.
    fn step(&mut self, argv: &[&str]) -> Outcome {
        let mut node = node_cli(&self.node.0, argv);
        let rust = rust_cli(&self.rust.0, argv);
        // The native handler prints Node's `note:` lines itself (eprintln!), which this
        // in-process harness cannot capture; they are checked on the Node side only.
        self.last_node_stderr = node.stderr.clone();
        node.stderr = node
            .stderr
            .lines()
            .filter(|line| !line.starts_with("dtech-aidlc: note:"))
            .map(|line| {
                format!(
                    "{line}
"
                )
            })
            .collect();
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

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn test_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A bare repository with one commit on `main`, shared by both projects so that the
/// URL and commit ids are identical.
fn bare_remote(base: &Path) -> PathBuf {
    let remote = base.join("remote.git");
    let seed = base.join("seed");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&seed).unwrap();
    test_git(&remote, &["init", "--bare"]);
    test_git(&seed, &["init"]);
    test_git(&seed, &["config", "user.email", "test@example.invalid"]);
    test_git(&seed, &["config", "user.name", "Test"]);
    fs::write(seed.join("tracked.txt"), "base\n").unwrap();
    test_git(&seed, &["add", "."]);
    test_git(&seed, &["commit", "-m", "base"]);
    test_git(&seed, &["branch", "-M", "main"]);
    test_git(
        &seed,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    test_git(&seed, &["push", "-u", "origin", "main"]);
    test_git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    remote
}

fn start_intent(root: &Path) {
    onboard(root);
    let out = Command::new("node")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core/tools/dtech-aidlc.mjs"))
        .args([
            "orchestrate",
            "next",
            "--intent",
            "Build a sample feature",
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

const INTENT_DIR: &str = ".aidlc/intents/build-a-sample-feature";

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_citations(root: &Path, registry: &str) {
    write(
        root,
        &format!("{INTENT_DIR}/develop/implementation-design/ids.json"),
        registry,
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/implementation-design/work-units.json"),
        r#"{"units":[{"id":"WU-02"},{"id":"U-03"},{"name":"no id"}]}"#,
    );
    write(
        root,
        &format!("{INTENT_DIR}/analyze/problem-framing/acceptance-criteria.md"),
        "AC-1 and AC-2\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/analyze/problem-framing/user-stories.md"),
        "US-1\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/analyze/problem-framing/problem-statement.md"),
        "Cites FN-404 and AC-77.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/implementation-design/functional-design.md"),
        "FN-01 FN-03 AC-1 AC-9 US-2 WU-02 U-03 U-01 IF-1 CFG-1 TS-4 FN-03 xFN-5 FN-123456\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/red-tests/sub/notes.md"),
        "Nested FN-77.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/red-tests/a-first.md"),
        "FN-01 only.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/build-and-verify/green-verification-report.md"),
        "Covers FN-01.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/build-and-verify/other.txt"),
        "FN-88\n",
    );
}

const REGISTRY: &str = r#"{"schemaVersion":1,"ids":{"FN":[{"id":"FN-01"},{"id":"FN-02"},{"id":"FN-01"}],"IF":[{"id":"IF-1"}],"WU":[{"id":"U-01"}],"7":[{"id":"CFG-1"}],"TS":"not an array","CFG":[{"title":"missing id"}]}}"#;

#[test]
fn ids_check_matches_node() {
    require_node!();
    let mut idle = Pair::new("ids-idle", onboard);
    idle.step(&["ids", "check"]);
    idle.step(&["ids", "bogus"]);

    let mut empty = Pair::new("ids-empty", start_intent);
    let out = empty.step(&["ids", "check"]);
    assert_eq!((out.code, out.stdout["ok"].clone()), (0, json!(true)));
    empty.step(&["ids", "check", "--stage", "problem-framing"]);
    empty.step(&["ids", "check", "--stage", "release-readiness"]);
    empty.step(&["ids", "check", "--stage", "no-such-stage"]);
    empty.step(&["ids", "bogus"]);

    let mut cited = Pair::new("ids-cited", |root| {
        start_intent(root);
        write_citations(root, REGISTRY);
    });
    let out = cited.step(&["ids", "check"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout["uncoveredFunctions"], json!(["FN-02"]));
    cited.step(&["ids", "check", "--stage", "build-and-verify"]);
    cited.step(&["ids", "check", "--stage", "red-tests"]);
    cited.step(&["ids", "check", "--stage", "problem-framing"]);

    for (label, registry) in [
        ("ids-malformed", "{not json"),
        (
            "ids-na",
            r#"{"status":"not-applicable","reason":"no code"}"#,
        ),
        (
            "ids-array",
            r#"{"schemaVersion":1,"ids":[[{"id":"FN-01"}]]}"#,
        ),
    ] {
        let mut pair = Pair::new(label, |root| {
            start_intent(root);
            write_citations(root, registry);
        });
        pair.step(&["ids", "check"]);
        pair.step(&["ids", "check", "--stage", "implementation-design"]);
    }
}

#[test]
fn deps_commands_match_node() {
    require_node!();
    let mut idle = Pair::new("deps-idle", onboard);
    for argv in [
        vec!["deps", "list"],
        vec!["deps", "add", "a", "b"],
        vec!["deps", "pull", "a"],
        vec!["deps", "bogus"],
    ] {
        let out = idle.step(&argv);
        assert_eq!(out.code, 1);
    }
    let mut active = Pair::new("deps-active", start_intent);
    for argv in [
        vec!["deps", "list"],
        vec!["deps", "add", "x", "problem-framing"],
        vec!["deps", "add", "x", "missing-stage"],
        vec!["deps", "add", "x", "problem-framing"],
        vec!["deps", "add", "y", "x"],
        vec!["deps", "add", "x", "x"],
        vec!["deps", "add", "bad id", "x"],
        vec!["deps", "add", "x", "-bad"],
        vec!["deps", "add", "x"],
        vec!["deps", "add"],
        vec!["deps", "list"],
        vec!["deps", "pull", "x"],
        vec!["deps", "pull", "y"],
        vec!["deps", "pull", "nothing"],
        vec!["deps", "pull"],
        vec!["deps", "pull", "bad id"],
        vec!["deps", "bogus"],
    ] {
        active.step(&argv);
    }
}

#[test]
fn repos_registration_matches_node() {
    require_node!();
    let mut pair = Pair::new("repos-reg", onboard);
    for argv in [
        vec!["repos", "list"],
        vec![
            "repos",
            "add",
            "lib",
            "https://example.invalid/org/lib.git",
            "--ref",
            "main",
        ],
        vec![
            "repos",
            "add",
            "lib",
            "https://example.invalid/org/lib.git",
            "--ref",
            "main",
        ],
        vec![
            "repos",
            "add",
            "lib",
            "https://example.invalid/org/other.git",
        ],
        vec!["repos", "add", "tool", "git@example.invalid:org/tool.git"],
        vec![
            "repos",
            "add",
            "ssh",
            "ssh://git@example.invalid:2222/org/ssh.git",
        ],
        vec![
            "repos",
            "add",
            "creds",
            "https://user:secret@example.invalid/x.git",
        ],
        vec!["repos", "add", "user", "https://user@example.invalid/x.git"],
        vec![
            "repos",
            "add",
            "sshpass",
            "ssh://git:pw@example.invalid/x.git",
        ],
        vec!["repos", "add", "rel", "relative/path"],
        vec!["repos", "add", "ext", "ext::sh -c x"],
        vec!["repos", "add", "dash", "-upload-pack"],
        vec!["repos", "add", "dots", "git@example.invalid:org/../x.git"],
        vec!["repos", "add", "query", "git@example.invalid:org/x.git?y"],
        vec![
            "repos",
            "add",
            "port",
            "https://example.invalid:99999/x.git",
        ],
        vec!["repos", "add", "-bad", "https://example.invalid/x.git"],
        vec![
            "repos",
            "add",
            "badref",
            "https://example.invalid/x.git",
            "--ref",
            "a..b",
        ],
        vec![
            "repos",
            "add",
            "badref2",
            "https://example.invalid/x.git",
            "--ref",
            "-x",
        ],
        vec!["repos", "add", "only-name"],
        vec!["repos", "add"],
        vec!["repos", "list"],
        vec!["repos", "remove", "unknown"],
        vec!["repos", "remove", "tool"],
        vec!["repos", "remove"],
        vec!["repos", "list"],
        vec!["repos", "bogus"],
        vec!["repos"],
    ] {
        pair.step(&argv);
    }
    // A declared aidlc.deps.json is edited alongside the runtime copy.
    let mut declared = Pair::new("repos-declared", |root| {
        onboard(root);
        fs::write(
            root.join("aidlc.deps.json"),
            "{\n  \"schemaVersion\": 1,\n  \"repos\": {\n    \"kept\": {\n      \"url\": \"https://example.invalid/kept.git\"\n    }\n  }\n}\n",
        )
        .unwrap();
    });
    for argv in [
        vec!["repos", "add", "lib", "https://example.invalid/lib.git"],
        vec!["repos", "add", "kept", "https://example.invalid/kept.git"],
        vec![
            "repos",
            "add",
            "kept",
            "https://example.invalid/changed.git",
        ],
        vec!["repos", "list"],
        vec!["repos", "remove", "kept"],
        vec!["repos", "remove", "kept"],
        vec!["repos", "list"],
    ] {
        declared.step(&argv);
    }
    // Invalid manifests are reported, never rewritten.
    for (label, file, text) in [
        ("repos-badjson", ".aidlc/deps.json", "{oops"),
        (
            "repos-badshape",
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":[]}"#,
        ),
        (
            "repos-badentry",
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":{"a":{"url":"https://example.invalid/a.git","ref":5}}}"#,
        ),
        (
            "repos-baddeclared",
            "aidlc.deps.json",
            r#"{"schemaVersion":2,"repos":{}}"#,
        ),
    ] {
        let mut pair = Pair::new(label, |root| {
            onboard(root);
            write(root, file, text);
        });
        pair.step(&["repos", "list"]);
        pair.step(&["repos", "add", "z", "https://example.invalid/z.git"]);
        pair.step(&["repos", "remove", "a"]);
    }
}

#[test]
fn repos_locking_matches_node() {
    require_node!();
    let live = std::process::id();
    let mut finished = Command::new("git").arg("--version").spawn().unwrap();
    let dead = finished.id();
    finished.wait().unwrap();
    for (label, lock, expect_ok) in [
        (
            "lock-live",
            format!("{{\"pid\":{live},\"at\":\"2026-01-01T00:00:00.000Z\"}}\n"),
            false,
        ),
        ("lock-noat", format!("{{\"pid\":{live}}}\n"), false),
        ("lock-garbage", "not json".to_owned(), false),
        (
            "lock-dead",
            format!("{{\"pid\":{dead},\"at\":\"2026-01-01T00:00:00.000Z\"}}\n"),
            true,
        ),
    ] {
        let mut pair = Pair::new(label, |root| {
            onboard(root);
            write(root, ".aidlc/deps.lock", &lock);
        });
        let out = pair.step(&["repos", "add", "lib", "https://example.invalid/lib.git"]);
        assert_eq!(out.code == 0, expect_ok, "{label}: {out:?}");
        pair.step(&["repos", "list"]);
    }
}

#[test]
fn repos_pull_matches_node() {
    require_node!();
    if !git_available() {
        eprintln!("git is not on PATH; skipping repos pull parity");
        return;
    }
    let shared = TempDir::new("remote");
    let remote = bare_remote(&shared.0);
    let url = remote.to_str().unwrap().to_owned();
    let declared = format!(
        "{}\n",
        serde_json::to_string_pretty(&json!({"schemaVersion":1,"repos":{"lib":{"url":url}}}))
            .unwrap()
    );
    let mut pair = Pair::new("pull", |root| {
        onboard(root);
        fs::write(root.join("aidlc.deps.json"), &declared).unwrap();
        // A runtime-only entry is dropped (with a note) when the declared file is imported.
        write(
            root,
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":{"stale":{"url":"https://example.invalid/stale.git"}}}"#,
        );
    });
    let first = pair.step(&["repos", "pull"]);
    assert_eq!(first.code, 0, "{first:?}");
    assert_eq!(first.stdout[0]["ok"], json!(true));
    assert!(
        pair.last_node_stderr.contains("dtech-aidlc: note: removed from the runtime copy because they are not declared in aidlc.deps.json: stale."),
        "{}",
        pair.last_node_stderr
    );
    pair.step(&["repos", "list"]);
    pair.step(&["repos", "pull", "lib"]);
    pair.step(&["repos", "pull", "unknown"]);
    pair.step(&["repos", "pull", "-bad"]);
    // A dirty checkout is preserved and reported as a failed row with exit code 1.
    for root in [&pair.node.0, &pair.rust.0] {
        fs::write(root.join(".aidlc/repos/lib/tracked.txt"), "local change\n").unwrap();
    }
    let dirty = pair.step(&["repos", "pull"]);
    assert_eq!(dirty.code, 1);
    assert_eq!(dirty.stdout[0]["ok"], json!(false));
    pair.step(&["repos", "remove", "lib"]);
    pair.step(&["repos", "list"]);

    // `--file` imports another manifest; an unmanaged directory blocks the pull.
    let other = TempDir::new("declared");
    let file = other.0.join("deps.json");
    fs::write(&file, &declared).unwrap();
    let file = file.to_str().unwrap().to_owned();
    let mut with_file = Pair::new("pull-file", |root| {
        onboard(root);
        write(root, ".aidlc/repos/lib/README.md", "not a checkout\n");
    });
    with_file.step(&["repos", "pull", "--file", &file]);
    with_file.step(&["repos", "pull", "--file", "missing-deps.json"]);
    with_file.step(&["repos", "list"]);
}
