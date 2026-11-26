//! Regression tests for review findings in the native CLI: commands that wrote state and
//! then failed (non-UTF-8 `.aidlc/.gitignore`, an audit log that cannot be appended to),
//! `status` / `intent list` for unusual state pointers, malformed prerequisites,
//! onboarding parity (every configured harness re-projected, projection stamp, output
//! and generated-file shapes, audit fields) and the malformed-graph message.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "aidlc-review-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = make_writable(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_writable(dir: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            make_writable(&entry.path())?;
        } else if meta.permissions().readonly() {
            let mut permissions = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(entry.path(), permissions)?;
        }
    }
    Ok(())
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust(project: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(args)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap()
}

fn node(project: &Path, args: &[&str]) -> Output {
    Command::new("node")
        .arg(repo().join("core/tools/dtech-aidlc.mjs"))
        .args(args)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("Node CLI must be available for parity tests")
}

fn ok(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&read(path)).unwrap()
}

fn audit_rows(project: &Path) -> Vec<Value> {
    fs::read_to_string(project.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Replace `from` (both separator spellings and JSON-escaped) with `to` in a JSON value.
fn rebase(value: &Value, from: &Path, to: &str) -> Value {
    let text = serde_json::to_string(value).unwrap();
    let native = from.to_string_lossy().into_owned();
    let escaped = serde_json::to_string(&native).unwrap();
    let escaped = &escaped[1..escaped.len() - 1];
    serde_json::from_str(&text.replace(escaped, to)).unwrap()
}

fn onboarded(project: &Path) {
    ok(&rust(
        project,
        &["onboard", "--harness", "claude", "--scope", "solo"],
    ));
}

// ---------------------------------------------------------------------------
// knowledge.rs: non-UTF-8 .aidlc/.gitignore
// ---------------------------------------------------------------------------

/// Regression: `orchestrate next` recorded the selection, then failed reading a
/// `.aidlc/.gitignore` that is not UTF-8 and printed no directive. Node reads it lossily.
#[test]
fn orchestrate_next_succeeds_with_a_non_utf8_gitignore() {
    let temp = TempDir::new("gitignore");
    let project = &temp.0;
    onboarded(project);
    ok(&rust(
        project,
        &["orchestrate", "next", "--intent", "Add login"],
    ));
    ok(&rust(
        project,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "waiting",
        ],
    ));
    let ignore = project.join(".aidlc/.gitignore");
    let mut bytes = read(&ignore);
    bytes.extend_from_slice(b"# caf\xe9 notes\nlocal.txt\n");
    fs::write(&ignore, &bytes).unwrap();
    let directive = ok(&rust(project, &["orchestrate", "next"]));
    assert!(directive["stage"].is_object(), "{directive}");
    assert_eq!(
        audit_rows(project).last().unwrap()["event"],
        "stage.selected"
    );
    assert_eq!(read(&ignore), bytes, "an up-to-date block is left as is");
}

// ---------------------------------------------------------------------------
// Audit preflight: no write when the audit log cannot be appended to
// ---------------------------------------------------------------------------

/// Make `.aidlc/audit.jsonl` read-only; `false` when this platform/user can still
/// append to it (for example when running as root), in which case the test is moot.
fn lock_audit(project: &Path) -> bool {
    let path = project.join(".aidlc/audit.jsonl");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    fs::OpenOptions::new().append(true).open(&path).is_err()
}

/// Regression: intent, config, deps and harness commands wrote state, then failed on
/// the audit append, leaving the change on disk without its audit row.
#[test]
fn commands_leave_state_unchanged_when_the_audit_log_is_not_writable() {
    let temp = TempDir::new("audit-lock");
    let project = &temp.0;
    onboarded(project);
    ok(&rust(
        project,
        &["orchestrate", "next", "--intent", "Add login"],
    ));
    if !lock_audit(project) {
        eprintln!("skipped: the audit log stays appendable for this user");
        return;
    }
    let audit_before = read(&project.join(".aidlc/audit.jsonl"));
    let watched = [
        ".aidlc/state.json",
        ".aidlc/intents/add-login/state.json",
        ".aidlc/config.json",
    ];
    let before: Vec<Vec<u8>> = watched.iter().map(|p| read(&project.join(p))).collect();
    let attempts: &[&[&str]] = &[
        &[
            "intent",
            "depth",
            "fast",
            "--approved-by",
            "bob",
            "--detail",
            "d",
        ],
        &["intent", "pause"],
        &["intent", "abandon", "add-login", "--detail", "stop"],
        &["config", "scope", "team"],
        &["config", "mode", "ai"],
        &[
            "config",
            "prerequisite",
            "set",
            "docker",
            "--check",
            "docker info",
        ],
        &["config", "agents", "set", "requirements-analyst", "opus"],
        &["deps", "add", "a", "b"],
        &["harness", "add", "deepseek"],
    ];
    for args in attempts {
        let output = rust(project, args);
        assert!(!output.status.success(), "{args:?} should fail");
        for (path, bytes) in watched.iter().zip(&before) {
            assert_eq!(&read(&project.join(path)), bytes, "{args:?} changed {path}");
        }
    }
    assert!(
        !project.join(".dsh").exists(),
        "harness add projected files"
    );
    assert_eq!(read(&project.join(".aidlc/audit.jsonl")), audit_before);
}

// ---------------------------------------------------------------------------
// status / intent list
// ---------------------------------------------------------------------------

/// Exit code, stdout and stderr with the project path (plain and JSON-escaped)
/// replaced by `<ROOT>`.
fn normalized(output: &Output, dir: &Path) -> (Option<i32>, String, String) {
    let plain = dir.to_string_lossy().into_owned();
    let escaped = serde_json::to_string(&plain).unwrap();
    let escaped = escaped[1..escaped.len() - 1].to_owned();
    let swap = |text: String| text.replace(&escaped, "<ROOT>").replace(&plain, "<ROOT>");
    (
        output.status.code(),
        swap(String::from_utf8_lossy(&output.stdout).into_owned()),
        swap(stderr(output)),
    )
}

/// Regression: `status` and `intent list` reported no active intent for an unmigrated
/// v1 state, and `status` shortened Node's errors for corrupt, unrecognised, dangling
/// and invalid pointers.
#[test]
fn status_and_intent_list_match_node_for_unusual_pointers() {
    let temp = TempDir::new("status");
    let (node_dir, rust_dir) = (temp.path("node"), temp.path("rust"));
    for dir in [&node_dir, &rust_dir] {
        ok(&node(dir, &["onboard", "--harness", "claude"]));
    }
    let cases: &[(&str, &str, bool)] = &[
        (
            "legacy without id",
            r#"{"version":1,"intent":"Legacy work","scope":"solo","currentStage":"problem-framing","stages":{"problem-framing":{"status":"in_progress"}}}"#,
            false,
        ),
        (
            "legacy without id, intents folder",
            r#"{"version":1,"intent":"Legacy work","scope":"solo","currentStage":"problem-framing","stages":{"problem-framing":{"status":"in_progress"}}}"#,
            true,
        ),
        (
            "legacy with id",
            r#"{"intent":"Legacy","intentId":"leg-1","scope":"solo","stages":{"requirements":{"status":"complete"}}}"#,
            true,
        ),
        ("garbage", "garbage", true),
        ("unknown version", r#"{"version":3}"#, true),
        (
            "dangling",
            r#"{"version":2,"activeIntentId":"ghost"}"#,
            true,
        ),
        (
            "invalid legacy",
            r#"{"intent":"","scope":"solo","stages":{}}"#,
            true,
        ),
    ];
    for (label, pointer, intents_dir) in cases {
        for dir in [&node_dir, &rust_dir] {
            fs::write(dir.join(".aidlc/state.json"), pointer).unwrap();
            if *intents_dir {
                fs::create_dir_all(dir.join(".aidlc/intents")).unwrap();
            }
        }
        for args in [&["status"][..], &["intent", "list"][..]] {
            assert_eq!(
                normalized(&node(&node_dir, args), &node_dir),
                normalized(&rust(&rust_dir, args), &rust_dir),
                "{label}: {args:?}"
            );
        }
    }
    // A corrupt intent state is listed with Node's error text.
    for dir in [&node_dir, &rust_dir] {
        fs::write(
            dir.join(".aidlc/state.json"),
            r#"{"version":2,"activeIntentId":null}"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join(".aidlc/intents/zzz")).unwrap();
        fs::write(dir.join(".aidlc/intents/zzz/state.json"), "{bad").unwrap();
    }
    let expected = rebase(
        &ok(&node(&node_dir, &["intent", "list"])),
        &node_dir,
        "<ROOT>",
    );
    let actual = rebase(
        &ok(&rust(&rust_dir, &["intent", "list"])),
        &rust_dir,
        "<ROOT>",
    );
    assert_eq!(expected, actual);
    let paused = ok(&rust(&rust_dir, &["status"]));
    assert_eq!(paused["activeIntent"], Value::Null);
}

// ---------------------------------------------------------------------------
// prerequisites
// ---------------------------------------------------------------------------

/// Regression: a prerequisite whose `check` is not a string ran an empty shell command
/// and was reported as passing.
#[test]
fn prerequisite_check_refuses_a_declaration_without_a_command() {
    let temp = TempDir::new("prereq");
    let project = &temp.0;
    onboarded(project);
    let config_path = project.join(".aidlc/config.json");
    let mut config = read_json(&config_path);
    config["prerequisites"] = serde_json::json!({
        "docker": {"check": 5, "hint": "start docker", "stages": ["problem-framing"]}
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    for args in [
        &["config", "prerequisite", "check"][..],
        &["config", "prerequisite", "check", "docker"][..],
    ] {
        let output = rust(project, args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert_eq!(
            stderr(&output).trim(),
            "dtech-aidlc: Prerequisite docker needs a one-line --check command"
        );
    }
}

// ---------------------------------------------------------------------------
// onboarding
// ---------------------------------------------------------------------------

fn brownfield(dir: &Path) {
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("docs/a.md"), "x\n").unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.join("README.md"), "hi\n").unwrap();
}

fn without_times(value: &Value) -> Value {
    let mut value = value.clone();
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for key in ["scannedAt", "initializedAt", "at", "mtimeMs"] {
                    if map.contains_key(key) {
                        map.insert(key.into(), Value::Null);
                    }
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(rows) => rows.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut value);
    value
}

/// Regression: native onboarding output, generated files, projection stamp and audit
/// rows differed from Node (verbatim `\\?\` paths, trimmed projections, build-time
/// methodology path, missing skipped entries and counts, no `.projection.json`, no
/// `graphPhases`/`projection` audit fields). With `DTECH_AIDLC_HOME` pointing at this
/// repository both CLIs use the same engine, so the outputs must be identical.
#[test]
fn onboard_and_rescan_match_node() {
    let temp = TempDir::new("onboard");
    let (node_dir, rust_dir) = (temp.path("node"), temp.path("rust"));
    brownfield(&node_dir);
    brownfield(&rust_dir);
    let run_both = |args: &[&str]| -> (Value, Value) {
        let n = Command::new("node")
            .arg(repo().join("core/tools/dtech-aidlc.mjs"))
            .args(args)
            .arg("--project")
            .arg(&node_dir)
            .env("DTECH_AIDLC_HOME", repo())
            .output()
            .unwrap();
        let r = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(args)
            .arg("--project")
            .arg(&rust_dir)
            .env("DTECH_AIDLC_HOME", repo())
            .output()
            .unwrap();
        (
            without_times(&rebase(&ok(&n), &node_dir, "<ROOT>")),
            without_times(&rebase(&ok(&r), &rust_dir, "<ROOT>")),
        )
    };
    let (expected, actual) = run_both(&["onboard", "--harness", "claude"]);
    assert_eq!(
        serde_json::to_string_pretty(&expected).unwrap(),
        serde_json::to_string_pretty(&actual).unwrap(),
        "onboard output (including key order)"
    );
    assert!(!actual["stateDir"].as_str().unwrap().contains(r"\\?\"));
    for file in [
        ".claude/skills/dtech-aidlc/.projection.json",
        ".aidlc/shared/project-profile.md",
        "aidlc.knowledge.json",
        ".aidlc/config.json",
    ] {
        let n = String::from_utf8(read(&node_dir.join(file))).unwrap();
        let r = String::from_utf8(read(&rust_dir.join(file))).unwrap();
        if file.ends_with("config.json") {
            assert_eq!(
                without_times(&serde_json::from_str(&n).unwrap()),
                without_times(&serde_json::from_str(&r).unwrap()),
                "{file}"
            );
        } else {
            assert_eq!(n, r, "{file}");
        }
    }
    for file in [
        ".aidlc/shared/project-profile.json",
        ".aidlc/shared/sources-snapshot.json",
    ] {
        assert_eq!(
            without_times(&read_json(&node_dir.join(file))),
            without_times(&read_json(&rust_dir.join(file))),
            "{file}"
        );
    }
    let audit = |dir: &Path| -> Vec<Value> { audit_rows(dir).iter().map(without_times).collect() };
    assert_eq!(audit(&node_dir), audit(&rust_dir));
    assert_eq!(
        audit(&rust_dir)[0]["projection"],
        ".claude/skills/dtech-aidlc"
    );

    for dir in [&node_dir, &rust_dir] {
        fs::write(dir.join("docs/b.md"), "more\n").unwrap();
        fs::create_dir_all(dir.join("specs")).unwrap();
        fs::write(dir.join("specs/openapi.yaml"), "y\n").unwrap();
    }
    let (expected, actual) = run_both(&["onboard", "--rescan"]);
    assert_eq!(expected, actual, "rescan output");
    assert_eq!(actual["documentation"]["manifest"], "aidlc.knowledge.json");
    let (expected, actual) = run_both(&["onboard", "--rescan", "--docs", "specs/openapi.yaml"]);
    assert_eq!(expected, actual, "rescan --docs output");
    assert_eq!(
        read(&node_dir.join("aidlc.knowledge.json")),
        read(&rust_dir.join("aidlc.knowledge.json"))
    );
    assert_eq!(audit(&node_dir), audit(&rust_dir));
}

/// Regression: re-running `onboard` re-projected only the first configured harness.
#[test]
fn onboard_rerun_restores_every_configured_harness() {
    let temp = TempDir::new("multi");
    let project = &temp.0;
    onboarded(project);
    ok(&rust(project, &["harness", "add", "deepseek"]));
    let skill = project.join(".dsh/skills/aidlc/SKILL.md");
    fs::remove_file(&skill).unwrap();
    let output = ok(&rust(project, &["onboard"]));
    assert!(skill.is_file());
    assert_eq!(output["filesCreated"], 1);
    assert_eq!(
        output["harnesses"],
        serde_json::json!(["claude", "deepseek"])
    );
    let projections: Vec<&str> = output["projections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["harness"].as_str().unwrap())
        .collect();
    assert_eq!(projections, ["claude", "deepseek"]);
    assert_eq!(
        read_json(&project.join(".dsh/skills/aidlc/.projection.json"))["engineVersion"],
        env!("CARGO_PKG_VERSION")
    );
}

/// Regression: a Rust-onboarded project had no projection stamp, so `harness status`
/// and `harness refresh` reported `null` engine versions.
#[test]
fn onboard_writes_the_projection_stamp() {
    let temp = TempDir::new("stamp");
    let project = &temp.0;
    onboarded(project);
    let status = ok(&rust(project, &["harness", "status"]));
    assert_eq!(
        status["harnesses"][0]["stampedEngineVersion"],
        env!("CARGO_PKG_VERSION")
    );
    let skill = project.join(".claude/skills/dtech-aidlc/SKILL.md");
    let mut text = read(&skill);
    text.extend_from_slice(b"\nlocal edit\n");
    fs::write(&skill, text).unwrap();
    let refresh = ok(&rust(project, &["harness", "refresh", "--force"]));
    assert_eq!(refresh["harnesses"][0]["from"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        audit_rows(project).last().unwrap()["from"],
        env!("CARGO_PKG_VERSION")
    );
}

// ---------------------------------------------------------------------------
// scope.rs: malformed graph message
// ---------------------------------------------------------------------------

/// Regression: a malformed `phase-graph.json` was reported with the serde error in
/// parentheses; Node names the graph file.
#[test]
fn malformed_graph_message_names_the_file() {
    let temp = TempDir::new("graph");
    let home = temp.path("home");
    let project = temp.path("project");
    fs::create_dir_all(home.join("core/data")).unwrap();
    fs::write(home.join("core/data/phase-graph.json"), "{bad").unwrap();
    let posix_home = home.to_string_lossy().replace('\\', "/");
    for home_arg in [home.to_string_lossy().into_owned(), posix_home] {
        let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(["config", "agents", "--project"])
            .arg(&project)
            .env("DTECH_AIDLC_HOME", &home_arg)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let graph = home.join("core").join("data").join("phase-graph.json");
        assert_eq!(
            stderr(&output).trim(),
            format!(
                "dtech-aidlc: Invalid phase-graph.json: malformed JSON ({})",
                graph.display()
            )
        );
        let expected = Command::new("node")
            .arg(repo().join("core/tools/dtech-aidlc.mjs"))
            .args(["config", "agents", "--project"])
            .arg(&project)
            .env("DTECH_AIDLC_HOME", &home_arg)
            .output()
            .unwrap();
        assert_eq!(stderr(&expected), stderr(&output));
    }
}
