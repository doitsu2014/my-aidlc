//! Regression tests for review findings in the native CLI: commands that wrote state and
//! then failed (non-UTF-8 `.aidlc/.gitignore`, an audit log that cannot be appended to),
//! `status` / `intent list` for unusual state pointers, malformed prerequisites,
//! onboarding parity (every configured harness re-projected, projection stamp, output
//! and generated-file shapes, audit fields) and the malformed-graph message.

#[path = "common/golden.rs"]
mod golden;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
/// `.aidlc/.gitignore` that is not UTF-8 and printed no directive; it must be read
/// lossily.
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
    // Separators after the root become `/` (JSON-escaped in stdout), so golden files
    // match on every platform.
    let swap = |text: String, separator: &str| {
        let text = text.replace(&escaped, "<ROOT>").replace(&plain, "<ROOT>");
        if text.contains("<ROOT>") {
            text.replace(separator, "/")
        } else {
            text
        }
    };
    (
        output.status.code(),
        swap(String::from_utf8_lossy(&output.stdout).into_owned(), r"\\"),
        swap(stderr(output), r"\"),
    )
}

/// Regression: `status` and `intent list` reported no active intent for an unmigrated
/// v1 state, and `status` shortened the reference errors for corrupt, unrecognised,
/// dangling and invalid pointers. The expected outputs are golden files recorded from the
/// reference implementation.
#[test]
fn status_and_intent_list_match_golden_for_unusual_pointers() {
    let temp = TempDir::new("status");
    let dir = temp.path("rust");
    ok(&rust(&dir, &["onboard", "--harness", "claude"]));
    assert_golden(
        "status-unusual-pointers",
        &status_scenario(&dir, &|dir: &Path, args: &[&str]| rust(dir, args)),
    );
    let paused = ok(&rust(&dir, &["status"]));
    assert_eq!(paused["activeIntent"], Value::Null);
}

/// Runs `status` / `intent list` for each unusual pointer in an onboarded project and
/// returns the normalized transcript.
fn status_scenario(dir: &Path, cli: &dyn Fn(&Path, &[&str]) -> Output) -> Value {
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
    let mut rows = Vec::new();
    for (label, pointer, intents_dir) in cases {
        fs::write(dir.join(".aidlc/state.json"), pointer).unwrap();
        if *intents_dir {
            fs::create_dir_all(dir.join(".aidlc/intents")).unwrap();
        }
        for args in [&["status"][..], &["intent", "list"][..]] {
            let (exit, stdout, stderr) = normalized(&cli(dir, args), dir);
            rows.push(serde_json::json!({
                "case": label,
                "args": args,
                "exit": exit,
                "stdout": stdout,
                "stderr": stderr,
            }));
        }
    }
    // A corrupt intent state is listed with the reference error text.
    fs::write(
        dir.join(".aidlc/state.json"),
        r#"{"version":2,"activeIntentId":null}"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join(".aidlc/intents/zzz")).unwrap();
    fs::write(dir.join(".aidlc/intents/zzz/state.json"), "{bad").unwrap();
    let corrupt = portable(&rebase(&ok(&cli(dir, &["intent", "list"])), dir, "<ROOT>"));
    serde_json::json!({"pointers": rows, "corruptIntentList": corrupt})
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
/// rows differed from the reference implementation (verbatim `\\?\` paths, trimmed
/// projections, build-time methodology path, missing skipped entries and counts, no
/// `.projection.json`, no `graphPhases`/`projection` audit fields). With
/// `DTECH_AIDLC_HOME` pointing at this repository the outputs must equal the golden
/// transcript recorded from the reference implementation with the same engine.
#[test]
fn onboard_and_rescan_match_golden() {
    let temp = TempDir::new("onboard");
    let dir = temp.path("rust");
    let cli = |dir: &Path, args: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(args)
            .arg("--project")
            .arg(dir)
            .env("DTECH_AIDLC_HOME", repo())
            .output()
            .unwrap()
    };
    let transcript = onboard_scenario(&dir, &cli);
    assert!(!transcript["onboard"]["stateDir"]
        .as_str()
        .unwrap()
        .contains(r"\\?\"));
    assert_eq!(
        transcript["audit"][0]["projection"],
        ".claude/skills/dtech-aidlc"
    );
    assert_eq!(
        transcript["rescan"]["documentation"]["manifest"],
        "aidlc.knowledge.json"
    );
    let golden = assert_golden("onboard-and-rescan", &transcript);
    // Key order of the onboard output is part of the contract.
    assert_eq!(
        serde_json::to_string_pretty(&golden["onboard"]).unwrap(),
        serde_json::to_string_pretty(&golden::checkout_value(&transcript["onboard"])).unwrap(),
        "onboard output (including key order)"
    );
}

/// Onboards a small brownfield project, rescans it twice and returns the normalized
/// outputs, generated files and audit rows.
fn onboard_scenario(dir: &Path, cli: &dyn Fn(&Path, &[&str]) -> Output) -> Value {
    brownfield(dir);
    let clean = |value: &Value| -> Value {
        versionless(&portable(&without_times(&rebase(
            &rebase(value, dir, "<ROOT>"),
            &repo(),
            "<REPO>",
        ))))
    };
    let run = |args: &[&str]| -> Value { clean(&ok(&cli(dir, args))) };
    let text =
        |file: &str| -> Value { Value::String(String::from_utf8(read(&dir.join(file))).unwrap()) };
    let audit = || -> Value { Value::Array(audit_rows(dir).iter().map(clean).collect()) };
    let onboard = run(&["onboard", "--harness", "claude"]);
    let mut files = serde_json::Map::new();
    // The projection stamp hashes each projected file (text with CRLF read as LF): check
    // each hash, then record it as `<SHA256>` so harness edits need no re-blessing.
    let skill_dir = dir.join(".claude/skills/dtech-aidlc");
    let mut stamp_text = String::from_utf8(read(&skill_dir.join(".projection.json"))).unwrap();
    let stamp: Value = serde_json::from_str(&stamp_text).unwrap();
    for (name, entry) in stamp["files"].as_object().unwrap() {
        let text = String::from_utf8_lossy(&read(&skill_dir.join(name))).replace("\r\n", "\n");
        let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
        assert_eq!(
            entry["sha256"],
            digest.as_str(),
            "projection stamp of {name}"
        );
        stamp_text = stamp_text.replace(&digest, "<SHA256>");
    }
    let version = format!("\"{}\"", env!("CARGO_PKG_VERSION"));
    files.insert(
        ".claude/skills/dtech-aidlc/.projection.json".to_owned(),
        Value::String(stamp_text.replace(&version, "\"<VERSION>\"")),
    );
    for file in [".aidlc/shared/project-profile.md", "aidlc.knowledge.json"] {
        files.insert(file.to_owned(), text(file));
    }
    for file in [
        ".aidlc/config.json",
        ".aidlc/shared/project-profile.json",
        ".aidlc/shared/sources-snapshot.json",
    ] {
        files.insert(file.to_owned(), clean(&read_json(&dir.join(file))));
    }
    let onboard_audit = audit();

    fs::write(dir.join("docs/b.md"), "more\n").unwrap();
    fs::create_dir_all(dir.join("specs")).unwrap();
    fs::write(dir.join("specs/openapi.yaml"), "y\n").unwrap();
    let rescan = run(&["onboard", "--rescan"]);
    let rescan_docs = run(&["onboard", "--rescan", "--docs", "specs/openapi.yaml"]);
    serde_json::json!({
        "onboard": onboard,
        "files": files,
        "audit": onboard_audit,
        "rescan": rescan,
        "rescanDocs": rescan_docs,
        "knowledgeAfterRescan": text("aidlc.knowledge.json"),
        "auditAfterRescan": audit(),
    })
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
/// parentheses; the reference implementation names the graph file (golden file).
#[test]
fn malformed_graph_message_names_the_file() {
    let temp = TempDir::new("graph");
    let home = temp.path("home");
    let project = temp.path("project");
    fs::create_dir_all(home.join("core/data")).unwrap();
    fs::write(home.join("core/data/phase-graph.json"), "{bad").unwrap();
    let messages = graph_scenario(&home, &|home_arg: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(["config", "agents", "--project"])
            .arg(&project)
            .env("DTECH_AIDLC_HOME", home_arg)
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
        output
    });
    assert_golden("malformed-graph", &messages);
}

/// The stderr of `config agents` for the native and the forward-slash spelling of a
/// `DTECH_AIDLC_HOME` whose graph is malformed, with the home folder as `<HOME>`.
fn graph_scenario(home: &Path, cli: &dyn Fn(&str) -> Output) -> Value {
    let native = home.to_string_lossy().into_owned();
    let posix_home = native.replace('\\', "/");
    let mut rows = Vec::new();
    for home_arg in [native.clone(), posix_home.clone()] {
        let message = stderr(&cli(&home_arg))
            .replace(&native, "<HOME>")
            .replace(&posix_home, "<HOME>")
            .replace('\\', "/");
        rows.push(Value::String(message));
    }
    Value::Array(rows)
}

/// Replaces this package's version with `<VERSION>`, so a release bump does not
/// invalidate the golden files.
fn versionless(value: &Value) -> Value {
    match value {
        Value::String(text) if text == env!("CARGO_PKG_VERSION") => json!("<VERSION>"),
        Value::Array(items) => Value::Array(items.iter().map(versionless).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), versionless(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Rewrites Windows separators to `/` in every string that holds a placeholder path
/// (`<ROOT>`, `<REPO>`), so golden files match on every platform.
fn portable(value: &Value) -> Value {
    match value {
        Value::String(text) if text.contains("<ROOT>") || text.contains("<REPO>") => {
            Value::String(text.replace('\\', "/"))
        }
        Value::Array(items) => Value::Array(items.iter().map(portable).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), portable(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Golden files: tests/golden/rust_review_regressions/<case>.json. The expectations were
// recorded from the reference implementation; set DTECH_AIDLC_BLESS=1 to rewrite them
// from the native CLI after an intentional change.
// ---------------------------------------------------------------------------

fn golden_path(case: &str) -> PathBuf {
    repo()
        .join("tests/golden/rust_review_regressions")
        .join(format!("{case}.json"))
}

fn blessing() -> bool {
    std::env::var_os("DTECH_AIDLC_BLESS").is_some_and(|value| value == "1")
}

fn write_golden(case: &str, value: &Value) {
    let path = golden_path(case);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    fs::write(path, text).unwrap();
}

/// Compares `actual` with the golden file (or records it when blessing) and returns
/// the expected value.
fn assert_golden(case: &str, actual: &Value) -> Value {
    // Repository files copied into projects (the scaffold) have the checkout's line
    // endings; the golden files hold the LF form.
    let actual = &golden::checkout_value(actual);
    if blessing() {
        write_golden(case, actual);
        return actual.clone();
    }
    let path = golden_path(case);
    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read golden {} ({error}); record it with DTECH_AIDLC_BLESS=1",
            path.display()
        )
    });
    let expected: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(actual, &expected, "golden {}", path.display());
    expected
}
