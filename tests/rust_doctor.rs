//! Parity and safety tests for the native `doctor` command (`dtech_aidlc::doctor::run`).
//!
//! Every case builds a fixture once, copies it, runs Node `doctor` on one copy and the
//! Rust handler on the other, and compares the JSON report, the exit code or the error
//! text (project paths normalised). The Rust copy is snapshotted before and after to
//! prove `doctor` changes nothing. Node comparisons are skipped only when `node` is not
//! on PATH; the Rust-side expectations still run.

use dtech_aidlc::args::FlagValue;
use dtech_aidlc::doctor;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "dtech-doctor-{label}-{}-{}-{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn node_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

fn node(project: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new("node")
        .arg(repo().join("core/tools/dtech-aidlc.mjs"))
        .args(args)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run node");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn onboard(project: &Path, harness: &str) {
    if node_available() {
        let (code, _, stderr) = node(
            project,
            &["onboard", "--harness", harness, "--scope", "solo"],
        );
        assert_eq!(code, 0, "node onboard failed: {stderr}");
    } else {
        dtech_aidlc::onboarding::onboard_project(
            project,
            &dtech_aidlc::onboarding::OnboardOptions {
                harness: harness.into(),
                scope: "solo".into(),
                explicit_docs: Vec::new(),
                detect_docs: true,
            },
        )
        .unwrap();
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

type Snapshot = BTreeMap<String, (bool, Vec<u8>, Option<SystemTime>)>;

fn snapshot(root: &Path) -> Snapshot {
    fn walk(root: &Path, dir: &Path, out: &mut Snapshot) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            if meta.is_dir() {
                out.insert(rel, (true, Vec::new(), meta.modified().ok()));
                walk(root, &path, out);
            } else {
                let bytes = fs::read(&path).unwrap_or_default();
                out.insert(rel, (false, bytes, meta.modified().ok()));
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, root, &mut out);
    out
}

fn rust_doctor(project: &Path) -> Result<(Value, i32), String> {
    doctor::run("doctor", project, &[], &BTreeMap::new())
}

fn path_forms(root: &Path) -> Vec<String> {
    let mut forms = vec![root.display().to_string()];
    if let Ok(canonical) = fs::canonicalize(root) {
        let text = canonical.display().to_string();
        forms.push(text.trim_start_matches(r"\\?\").to_owned());
        forms.push(text);
    }
    let slashed: Vec<String> = forms.iter().map(|form| form.replace('\\', "/")).collect();
    forms.extend(slashed);
    forms.sort_by_key(|form| std::cmp::Reverse(form.len()));
    forms.dedup();
    forms
}

fn normalise_text(text: &str, roots: &[&Path]) -> String {
    let mut text = text.to_owned();
    for root in roots {
        for form in path_forms(root) {
            text = text.replace(&form, "<ROOT>");
        }
    }
    text
}

fn normalise(value: &Value, roots: &[&Path]) -> Value {
    match value {
        Value::String(text) => Value::String(normalise_text(text, roots)),
        Value::Array(rows) => Value::Array(rows.iter().map(|row| normalise(row, roots)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, row)| (key.clone(), normalise(row, roots)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Run Node on `node_root` and Rust on `rust_root` (already holding identical
/// fixtures), assert the Rust copy is untouched, and compare the outcomes.
fn compare_roots(node_root: &Path, rust_root: &Path) -> Result<(Value, i32), String> {
    let before = snapshot(rust_root);
    let rust = rust_doctor(rust_root);
    assert_eq!(before, snapshot(rust_root), "doctor changed project files");
    if node_available() {
        let (code, stdout, stderr) = node(node_root, &["doctor"]);
        match &rust {
            Ok((value, rust_code)) => {
                let expected: Value = serde_json::from_str(&stdout)
                    .unwrap_or_else(|_| panic!("node output: {stdout}\n{stderr}"));
                assert_eq!(
                    normalise(value, &[rust_root]),
                    normalise(&expected, &[node_root]),
                    "doctor report differs from Node"
                );
                assert_eq!(*rust_code, code, "exit code differs from Node");
            }
            Err(message) => {
                assert_eq!(code, 1, "node succeeded: {stdout}");
                assert_eq!(
                    normalise_text(&format!("dtech-aidlc: {message}"), &[rust_root]),
                    normalise_text(stderr.trim_end(), &[node_root]),
                );
            }
        }
    }
    rust
}

/// Copy `template` (if any) twice, apply `setup` to both copies, then compare.
fn compare(
    label: &str,
    template: Option<&Path>,
    setup: impl Fn(&Path),
) -> Result<(Value, i32), String> {
    let a = TempDir::new(label);
    let b = TempDir::new(label);
    for dir in [a.path(), b.path()] {
        if let Some(template) = template {
            copy_tree(template, dir);
        }
        setup(dir);
    }
    compare_roots(a.path(), b.path())
}

fn messages(report: &Value) -> Vec<(String, String)> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["status"].as_str().unwrap().to_owned(),
                row["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn has(report: &Value, status: &str, message: &str) -> bool {
    messages(report)
        .iter()
        .any(|(s, m)| s == status && m == message)
}

fn has_prefix(report: &Value, status: &str, prefix: &str) -> bool {
    messages(report)
        .iter()
        .any(|(s, m)| s == status && m.starts_with(prefix))
}

fn graph() -> Value {
    let empty = TempDir::new("graph");
    let (graph, _) = dtech_aidlc::scope::load_embedded_graph(empty.path()).unwrap();
    graph.value().clone()
}

fn solo_stage_ids() -> Vec<String> {
    let graph = graph();
    dtech_aidlc::workflow::ordered_stages(&graph, "solo", "full")
        .iter()
        .map(|stage| stage["id"].as_str().unwrap().to_owned())
        .collect()
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_json(root: &Path, rel: &str, value: &Value) {
    write(root, rel, &serde_json::to_string_pretty(value).unwrap());
}

fn read_json(root: &Path, rel: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(rel)).unwrap()).unwrap()
}

fn sha256_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(text.replace("\r\n", "\n").as_bytes())
    )
}

fn sep_join(parts: &[&str]) -> String {
    parts.join(if cfg!(windows) { "\\" } else { "/" })
}

// ---------------------------------------------------------------------------

#[test]
fn healthy_project_matches_node_and_is_read_only() {
    let (report, code) = compare("healthy", None, |dir| onboard(dir, "claude")).unwrap();
    assert_eq!(code, 0);
    assert_eq!(report["ok"], true);
    let graph = graph();
    assert!(has(
        &report,
        "pass",
        &format!(
            "Graph: {} phases, {} stages, {} agents",
            graph["phases"].as_array().unwrap().len(),
            graph["stages"].as_array().unwrap().len(),
            graph["agents"].as_array().unwrap().len()
        )
    ));
    assert!(has(
        &report,
        "pass",
        "Project configuration and agent models"
    ));
    assert!(has(
        &report,
        "pass",
        &format!(
            "Projection claude: current (.claude/skills/dtech-aidlc, engine {})",
            env!("CARGO_PKG_VERSION")
        )
    ));
    assert!(has(
        &report,
        "pass",
        "Dependency repositories: 0 registered"
    ));
    assert!(has(&report, "pass", "Audit log contains valid JSON lines"));
    assert!(has_prefix(
        &report,
        "warn",
        "No allowed approver is configured for gate "
    ));
    assert_eq!(
        report["warnings"].as_u64().unwrap() as usize,
        messages(&report)
            .iter()
            .filter(|(s, _)| s == "warn")
            .count()
    );

    // A deepseek project projects a different skill directory.
    let (report, _) = compare("healthy-deepseek", None, |dir| onboard(dir, "deepseek")).unwrap();
    assert!(has_prefix(
        &report,
        "pass",
        "Projection deepseek: current (.dsh/skills/aidlc"
    ));
}

#[test]
fn uninitialized_and_missing_projects_match_node() {
    let (report, code) = compare("empty", None, |_| {}).unwrap();
    assert_eq!(code, 0);
    assert!(has(
        &report,
        "pass",
        "Knowledge sources: no aidlc.knowledge.json; run dtech-aidlc onboard --harness <claude|deepseek>"
    ));
    assert!(!has_prefix(&report, "pass", "Projection"));
    assert!(!has_prefix(&report, "warn", "No allowed approver"));

    let a = TempDir::new("missing");
    let b = TempDir::new("missing");
    let (report, code) =
        compare_roots(&a.path().join("not-there"), &b.path().join("not-there")).unwrap();
    assert_eq!(code, 0);
    assert!(
        !b.path().join("not-there").exists(),
        "doctor created the project"
    );
    assert!(has_prefix(
        &report,
        "pass",
        "Knowledge sources: no aidlc.knowledge.json; run"
    ));
}

#[test]
fn invalid_configuration_matches_node() {
    let template = TempDir::new("config-template");
    onboard(template.path(), "claude");
    let base = read_json(template.path(), ".aidlc/config.json");
    let graph = graph();
    let agent = graph["agents"][0]["id"].as_str().unwrap().to_owned();
    let gate = graph["phases"][0]["gate"].as_str().unwrap().to_owned();
    let with = |key: &str, value: Value| {
        let mut config = base.clone();
        config[key] = value;
        serde_json::to_string_pretty(&config).unwrap()
    };
    let label = "Project configuration and agent models";
    let cases: Vec<(String, String)> = vec![
        ("{not json".into(), format!("{label}: Invalid project configuration JSON")),
        ("[]".into(), format!("{label}: Invalid project configuration: expected an object")),
        (with("mode", json!("auto")), format!("{label}: mode must be human or ai")),
        (with("mode", Value::Null), format!("{label}: mode must be human or ai")),
        (
            with("agentModels", json!({"nobody": "opus"})),
            format!("{label}: Unknown agent in agentModels: nobody"),
        ),
        (
            with("agentModels", json!({ agent.clone(): 5 })),
            format!(
                "{label}: Invalid model alias for {agent}: 5; choose opus, sonnet, haiku, or fable"
            ),
        ),
        (
            with("agentModels", json!(["opus"])),
            format!("{label}: Invalid agentModels: expected an object mapping agent IDs to model aliases"),
        ),
        (
            with("gateModes", json!("human")),
            format!("{label}: Invalid gateModes: expected an object mapping gate ids to \"human\""),
        ),
        (
            with("gateModes", json!({"no-such-gate": "human"})),
            format!("{label}: Unknown gate in gateModes: no-such-gate"),
        ),
        (
            with("gateModes", json!({ gate.clone(): "ai" })),
            format!("{label}: Invalid gateModes.{gate}: a gate can only be pinned to human"),
        ),
        (
            with("prerequisites", json!({"Bad Name": {"check": "exit 0"}})),
            format!("{label}: Prerequisite name 'Bad Name' must be 1-32 lowercase letters, digits or hyphens"),
        ),
        (
            with("prerequisites", json!({"tool": {"check": "exit 0", "stages": ["nowhere"]}})),
            format!("{label}: Prerequisite tool names unknown stage or phase: nowhere"),
        ),
        (
            with("unitSplitThreshold", json!(1.5)),
            format!("{label}: Invalid unitSplitThreshold: expected a positive integer"),
        ),
        (with("roles", json!([])), format!("{label}: Invalid roles: expected an object")),
        (
            with("roles", json!({"owner": "me"})),
            format!("{label}: Invalid roles.owner: expected an array of names"),
        ),
        (
            with("roles", json!({"approvers": []})),
            format!("{label}: Invalid roles.approvers: expected an object mapping gate ids to names"),
        ),
        (
            with("roles", json!({"approvers": {"g": "x"}})),
            format!("{label}: Invalid roles.approvers.g"),
        ),
        (with("harness", json!("vim")), label.to_owned()),
        (with("scope", json!("galaxy")), label.to_owned()),
    ];
    for (index, (text, expected)) in cases.iter().enumerate() {
        let (report, code) = compare(&format!("config-{index}"), Some(template.path()), |dir| {
            write(dir, ".aidlc/config.json", text)
        })
        .unwrap();
        assert_eq!(code, 1, "case {index}: {report:#}");
        assert_eq!(report["ok"], false);
        assert!(has(&report, "fail", expected), "case {index}: {report:#}");
    }

    // An invalid configuration contributes no projection or approver rows.
    let (report, _) = compare("config-invalid-rows", Some(template.path()), |dir| {
        write(dir, ".aidlc/config.json", "{not json")
    })
    .unwrap();
    assert!(!has_prefix(&report, "pass", "Projection"));
    assert!(!has_prefix(&report, "warn", "No allowed approver"));

    // Configured roles silence the approver warning; harnesses lists every projection.
    let (report, code) = compare("config-roles", Some(template.path()), |dir| {
        let mut config = base.clone();
        config["roles"] = json!({"owner": ["Ana"], "approvers": { gate.clone(): ["Ana"] }});
        config["agentModels"] = json!({ agent.clone(): "haiku" });
        config["gateModes"] = json!({ gate.clone(): "human" });
        write_json(dir, ".aidlc/config.json", &config);
    })
    .unwrap();
    assert_eq!(code, 0);
    assert!(!has_prefix(&report, "warn", "No allowed approver"));

    let (report, code) = compare("config-harnesses", Some(template.path()), |dir| {
        let mut config = base.clone();
        config["harnesses"] = json!(["claude", "deepseek"]);
        write_json(dir, ".aidlc/config.json", &config);
    })
    .unwrap();
    assert_eq!(code, 1);
    assert!(has(
        &report,
        "fail",
        &format!(
            "Projection deepseek: {} is missing; run: dtech-aidlc harness refresh",
            sep_join(&[".dsh", "skills", "aidlc", "SKILL.md"])
        )
    ));
}

#[test]
fn prerequisites_run_and_report_like_node() {
    let template = TempDir::new("prereq-template");
    onboard(template.path(), "claude");
    let mut config = read_json(template.path(), ".aidlc/config.json");
    config["prerequisites"] = json!({
        "ready": {"check": "exit 0"},
        "broken": {"check": "exit 3", "hint": "Install the tool"},
        "silent": {"check": "exit 4", "hint": ""}
    });
    let (report, code) = compare("prereq", Some(template.path()), |dir| {
        write_json(dir, ".aidlc/config.json", &config)
    })
    .unwrap();
    assert_eq!(code, 0);
    assert!(has(&report, "pass", "Prerequisite ready: check passed"));
    assert!(has(
        &report,
        "warn",
        "Prerequisite broken: check failed (exit 3); Install the tool"
    ));
    assert!(has(
        &report,
        "warn",
        "Prerequisite silent: check failed (exit 4)"
    ));
}

#[test]
fn knowledge_manifest_and_sources_match_node() {
    let template = TempDir::new("knowledge-template");
    onboard(template.path(), "claude");

    let (report, code) = compare("knowledge-json", Some(template.path()), |dir| {
        write(dir, "aidlc.knowledge.json", "{oops")
    })
    .unwrap();
    assert_eq!(code, 1);
    assert!(has(
        &report,
        "fail",
        "Knowledge: Invalid knowledge manifest: cannot be parsed as JSON"
    ));
    assert!(!has_prefix(&report, "pass", "Knowledge sources"));

    let (report, _) = compare("knowledge-schema", Some(template.path()), |dir| {
        write_json(
            dir,
            "aidlc.knowledge.json",
            &json!({"schemaVersion": 2, "sources": []}),
        )
    })
    .unwrap();
    assert!(has(
        &report,
        "fail",
        "Knowledge: Invalid knowledge manifest: expected schemaVersion 1 and a sources array"
    ));

    let (report, _) = compare("knowledge-kind", Some(template.path()), |dir| {
        write_json(
            dir,
            "aidlc.knowledge.json",
            &json!({"schemaVersion": 1, "sources": [{"id": "a", "path": "a.md", "kind": "novel"}]}),
        )
    })
    .unwrap();
    assert!(has_prefix(
        &report,
        "fail",
        "Knowledge: Invalid knowledge manifest: source a: kind must be one of"
    ));

    let manifest = json!({"schemaVersion": 1, "sources": [
        {"id": "project", "path": "PROJECT.md", "kind": "project"},
        {"id": "docs", "path": "docs", "kind": "docs"}
    ]});
    let (report, code) = compare("knowledge-missing", Some(template.path()), |dir| {
        write_json(dir, "aidlc.knowledge.json", &manifest)
    })
    .unwrap();
    assert_eq!(code, 0);
    assert!(has_prefix(
        &report,
        "warn",
        "Knowledge sources: 2 registered, 1 missing, "
    ));

    let (report, _) = compare("knowledge-unscanned", Some(template.path()), |dir| {
        write_json(dir, "aidlc.knowledge.json", &manifest);
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::remove_file(dir.join(".aidlc/shared/sources-snapshot.json")).unwrap();
    })
    .unwrap();
    assert!(has(
        &report,
        "pass",
        "Knowledge sources: 2 registered, 0 missing; not scanned on this machine"
    ));

    // Documentation registered at onboarding, then edited, added and removed.
    let docs_template = TempDir::new("knowledge-docs-template");
    write(
        docs_template.path(),
        "docs/guide.md",
        "# Guide\n\nFirst version.\n",
    );
    write(docs_template.path(), "docs/old.md", "# Old\n");
    onboard(docs_template.path(), "claude");
    let (report, _) = compare("knowledge-docs-clean", Some(docs_template.path()), |_| {}).unwrap();
    assert!(has_prefix(&report, "pass", "Knowledge sources: "));
    let (report, _) = compare(
        "knowledge-docs-changed",
        Some(docs_template.path()),
        |dir| {
            write(dir, "docs/guide.md", "# Guide\n\nSecond, longer version.\n");
            write(dir, "docs/new.md", "# New\n");
            fs::remove_file(dir.join("docs/old.md")).unwrap();
        },
    )
    .unwrap();
    assert!(
        messages(&report)
            .iter()
            .any(|(_, m)| m.starts_with("Knowledge sources: ")
                && m.ends_with(" 3 changed since last scan")),
        "{report:#}"
    );
}

#[test]
fn harness_drift_matches_node() {
    let template = TempDir::new("drift-template");
    onboard(template.path(), "claude");
    let skill = ".claude/skills/dtech-aidlc";
    let stamp_rel = format!("{skill}/.projection.json");
    let stamp = read_json(template.path(), &stamp_rel);
    let version = env!("CARGO_PKG_VERSION");

    let (report, code) = compare("drift-modified", Some(template.path()), |dir| {
        write(dir, &format!("{skill}/SKILL.md"), "locally edited skill\n")
    })
    .unwrap();
    assert_eq!(code, 0);
    assert!(has(
        &report,
        "warn",
        &format!(
            "Projection claude: {} edited locally or of unknown origin; keep them, or for skill files run: dtech-aidlc harness refresh --force; obsolete files must be deleted by hand",
            sep_join(&[".claude", "skills", "dtech-aidlc", "SKILL.md"])
        )
    ));

    let (report, code) = compare("drift-missing", Some(template.path()), |dir| {
        fs::remove_file(dir.join(skill).join("onboarding.md")).unwrap()
    })
    .unwrap();
    assert_eq!(code, 1);
    assert!(has(
        &report,
        "fail",
        &format!(
            "Projection claude: {} is missing; run: dtech-aidlc harness refresh",
            sep_join(&[".claude", "skills", "dtech-aidlc", "onboarding.md"])
        )
    ));

    let old_text = "skill text projected by an older engine\n";
    let behind_stamp = |engine: Value| {
        let mut next = stamp.clone();
        next["engineVersion"] = engine;
        next["files"]["SKILL.md"] = json!({"sha256": sha256_text(old_text)});
        next
    };
    let (report, _) = compare("drift-behind", Some(template.path()), |dir| {
        write(dir, &format!("{skill}/SKILL.md"), old_text);
        write_json(dir, &stamp_rel, &behind_stamp(json!("0.1.0")));
    })
    .unwrap();
    assert!(has(
        &report,
        "warn",
        &format!("Projection claude: behind engine {version} (projected by 0.1.0); run: dtech-aidlc harness refresh")
    ));

    let (report, _) = compare("drift-behind-unstamped", Some(template.path()), |dir| {
        write(dir, &format!("{skill}/SKILL.md"), old_text);
        write_json(dir, &stamp_rel, &behind_stamp(Value::Null));
    })
    .unwrap();
    assert!(has_prefix(
        &report,
        "warn",
        &format!("Projection claude: behind engine {version} (projected by an earlier version without a stamp)")
    ));

    let (report, _) = compare("drift-ahead", Some(template.path()), |dir| {
        write(dir, &format!("{skill}/SKILL.md"), "from the future\n");
        let mut next = stamp.clone();
        next["engineVersion"] = json!("99.0.0");
        write_json(dir, &stamp_rel, &next);
    })
    .unwrap();
    assert!(has(
        &report,
        "warn",
        &format!("Projection claude: projected by engine 99.0.0, newer than the installed {version}; upgrade dtech-aidlc")
    ));

    let (report, _) = compare("drift-obsolete", Some(template.path()), |dir| {
        write(
            dir,
            &format!("{skill}/manifest.mjs"),
            "export default {};\n",
        );
        write(dir, ".claude/skills/aidlc/SKILL.md", "old skill copy\n");
    })
    .unwrap();
    assert!(has_prefix(&report, "warn", "Projection claude: .claude/skills/dtech-aidlc/manifest.mjs, .claude/skills/aidlc/SKILL.md edited locally"));

    let pristine = "export const old = true;\n";
    let (report, _) = compare("drift-obsolete-pristine", Some(template.path()), |dir| {
        write(dir, &format!("{skill}/manifest.mjs"), pristine);
        let mut next = stamp.clone();
        next["files"]["manifest.mjs"] = json!({"sha256": sha256_text(pristine)});
        write_json(dir, &stamp_rel, &next);
    })
    .unwrap();
    assert!(has_prefix(
        &report,
        "warn",
        "Projection claude: behind engine"
    ));

    let (report, _) = compare("drift-dir", Some(template.path()), |dir| {
        fs::remove_file(dir.join(skill).join("SKILL.md")).unwrap();
        fs::create_dir_all(dir.join(skill).join("SKILL.md")).unwrap();
    })
    .unwrap();
    assert!(has_prefix(&report, "warn", "Projection claude: "));

    let (report, _) = compare("drift-legacy-methodology", Some(template.path()), |dir| {
        write(dir, ".aidlc/methodology/notes.md", "my notes\n");
        write(dir, ".aidlc/methodology/sub/other.md", "more notes\n");
    })
    .unwrap();
    assert!(has(
        &report,
        "warn",
        "Unused methodology copy at .aidlc/methodology from an earlier version; 0 pristine file(s) can be removed by harness refresh, 2 modified or unknown file(s) will be kept"
    ));
}

#[test]
fn audit_log_and_dependency_registry_match_node() {
    let template = TempDir::new("audit-template");
    onboard(template.path(), "claude");
    let (report, code) = compare("audit-bad", Some(template.path()), |dir| {
        let mut text = fs::read_to_string(dir.join(".aidlc/audit.jsonl")).unwrap();
        text.push_str("not json\r\n\r\n");
        fs::write(dir.join(".aidlc/audit.jsonl"), text).unwrap();
    })
    .unwrap();
    assert_eq!(code, 1);
    assert!(has(&report, "fail", "Audit log contains valid JSON lines"));

    let (report, _) = compare("audit-crlf", Some(template.path()), |dir| {
        fs::write(
            dir.join(".aidlc/audit.jsonl"),
            "{\"event\":\"a\"}\r\n\r\n{\"event\":\"b\"}\r\n",
        )
        .unwrap();
    })
    .unwrap();
    assert!(has(&report, "pass", "Audit log contains valid JSON lines"));

    let (report, code) = compare("deps-bad", Some(template.path()), |dir| {
        write(dir, ".aidlc/deps.json", "{\"schemaVersion\": 7}")
    })
    .unwrap();
    assert_eq!(code, 1);
    assert!(has_prefix(&report, "fail", "Dependency repositories: "));
}

fn intent_state(id: &str, stages: Value, current: Option<&str>) -> Value {
    json!({
        "version": 2,
        "intent": "Doctor parity intent",
        "intentId": id,
        "scope": "solo",
        "mode": "human",
        "createdAt": "2026-01-01T00:00:00.000Z",
        "currentStage": current,
        "stages": stages,
        "dependencies": {}
    })
}

#[test]
fn workflow_state_problems_match_node() {
    let template = TempDir::new("state-template");
    onboard(template.path(), "claude");
    let ids = solo_stage_ids();
    let graph = graph();
    let pointer = |dir: &Path, id: &str| {
        write_json(
            dir,
            ".aidlc/state.json",
            &json!({"version": 2, "activeIntentId": id}),
        )
    };

    let (report, code) = compare("state-corrupt", Some(template.path()), |dir| {
        write(dir, ".aidlc/state.json", "{broken")
    })
    .unwrap();
    assert_eq!(code, 1);
    assert!(has(
        &report,
        "fail",
        "State pointer: .aidlc/state.json cannot be parsed"
    ));

    let (report, _) = compare("state-unknown", Some(template.path()), |dir| {
        write(dir, ".aidlc/state.json", "[1, 2]")
    })
    .unwrap();
    assert!(has(
        &report,
        "fail",
        "State pointer: .aidlc/state.json is not a recognised state file"
    ));

    let (report, _) = compare("state-ghost", Some(template.path()), |dir| {
        pointer(dir, "ghost")
    })
    .unwrap();
    assert!(has(
        &report,
        "fail",
        "State pointer: active intent 'ghost' has no state file"
    ));

    let (report, code) = compare("state-legacy", Some(template.path()), |dir| {
        write_json(
            dir,
            ".aidlc/state.json",
            &json!({"intent": "Old", "scope": "solo", "currentStage": ids[0], "stages": {}}),
        )
    })
    .unwrap();
    assert_eq!(code, 0);
    assert!(has(
        &report,
        "warn",
        "State: version 1 format; it is migrated automatically by the next orchestrate or intent command"
    ));

    let (report, _) = compare("state-paused", Some(template.path()), |dir| {
        write_json(
            dir,
            ".aidlc/state.json",
            &json!({"version": 2, "activeIntentId": null}),
        )
    })
    .unwrap();
    assert!(has(&report, "pass", "Workflow state"));

    let (report, _) = compare("state-no-config", Some(template.path()), |dir| {
        pointer(dir, "demo");
        write_json(
            dir,
            ".aidlc/intents/demo/state.json",
            &intent_state("demo", json!({}), Some(&ids[0])),
        );
        fs::remove_file(dir.join(".aidlc/config.json")).unwrap();
    })
    .unwrap();
    assert!(has_prefix(
        &report,
        "warn",
        "Project has workflow state but no .aidlc/config.json"
    ));

    let (report, _) = compare("state-unparseable", Some(template.path()), |dir| {
        write(dir, ".aidlc/intents/demo/state.json", "{")
    })
    .unwrap();
    assert!(has(&report, "fail", "Intent demo: cannot be parsed"));

    let (report, _) = compare("state-null", Some(template.path()), |dir| {
        write(dir, ".aidlc/intents/demo/state.json", "null")
    })
    .unwrap();
    assert!(has_prefix(
        &report,
        "fail",
        "Cannot read properties of null"
    ));

    let (report, _) = compare("state-invalid", Some(template.path()), |dir| {
        write_json(
            dir,
            ".aidlc/intents/demo/state.json",
            &json!({"intent": " ", "intentId": "other", "scope": "galaxy", "mode": "robot",
                    "stages": {"x": {"status": "weird"}}, "currentStage": "nowhere",
                    "dependencies": {"a": "b"}}),
        );
        write(dir, ".aidlc/intents/Not-An-Id/state.json", "{");
    })
    .unwrap();
    for problem in [
        "intent is not a non-empty string",
        "scope is not a key of graph.scopes",
        "mode is not ai or human",
        "stage x has an invalid status",
        "intentId does not match folder",
        "currentStage is not a stage of the scope",
        "dependencies is not an object of string arrays",
    ] {
        assert!(
            has(&report, "fail", &format!("Intent demo: {problem}")),
            "{report:#}"
        );
    }

    let (report, _) = compare("state-order", Some(template.path()), |dir| {
        pointer(dir, "demo");
        let stages = json!({
            ids[1].clone(): {"status": "complete"},
            "retired-stage": {"status": "complete"}
        });
        write_json(
            dir,
            ".aidlc/intents/demo/state.json",
            &intent_state("demo", stages, Some(&ids[2])),
        );
    })
    .unwrap();
    assert!(has(
        &report,
        "fail",
        &format!(
            "Intent demo: stage {} is recorded complete although the earlier stage is not finished",
            ids[1]
        )
    ));
    assert!(has(
        &report,
        "fail",
        &format!(
            "Intent demo: currentStage {} is not the first unfinished stage",
            ids[2]
        )
    ));
    assert!(has(
        &report,
        "warn",
        "Intent demo: stage retired-stage is not in the current graph"
    ));

    // Completing a whole phase with reports but no gate decision.
    let first_phase = graph["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stage| stage["id"] == ids[0].as_str())
        .unwrap()["phase"]
        .clone();
    let phase_stages: Vec<String> = dtech_aidlc::workflow::ordered_stages(&graph, "solo", "full")
        .iter()
        .filter(|stage| stage["phase"] == first_phase)
        .map(|stage| stage["id"].as_str().unwrap().to_owned())
        .collect();
    let gate = graph["phases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|phase| phase["id"] == first_phase)
        .unwrap()["gate"]
        .as_str()
        .unwrap()
        .to_owned();
    let completed = |decision: Option<&str>| {
        let mut stages = serde_json::Map::new();
        for id in &phase_stages {
            stages.insert(id.clone(), json!({"status": "complete", "reports": []}));
        }
        let next = ids.iter().find(|id| !phase_stages.contains(id)).cloned();
        let mut state = intent_state("demo", Value::Object(stages), next.as_deref());
        if let Some(decision) = decision {
            state["gates"] = json!({ gate.clone(): {"decision": decision} });
        }
        state
    };
    let (report, _) = compare("state-gate", Some(template.path()), |dir| {
        pointer(dir, "demo");
        write_json(dir, ".aidlc/intents/demo/state.json", &completed(None));
    })
    .unwrap();
    assert!(has(
        &report,
        "fail",
        &format!("Intent demo: gate {gate} was passed without a recorded approval")
    ));
    let (report, code) = compare("state-gate-approved", Some(template.path()), |dir| {
        pointer(dir, "demo");
        write_json(
            dir,
            ".aidlc/intents/demo/state.json",
            &completed(Some("approved")),
        );
    })
    .unwrap();
    assert_eq!(code, 0);
    assert!(has(&report, "pass", "Workflow state"));
}

#[test]
fn intent_started_by_node_is_healthy() {
    if !node_available() {
        return;
    }
    let template = TempDir::new("intent-template");
    onboard(template.path(), "claude");
    let (code, _, stderr) = node(
        template.path(),
        &["orchestrate", "next", "--intent", "Doctor parity check"],
    );
    assert_eq!(code, 0, "{stderr}");
    let (report, code) = compare("intent-active", Some(template.path()), |_| {}).unwrap();
    assert_eq!(code, 0);
    assert!(has(&report, "pass", "Workflow state"));
}

#[test]
fn unsafe_state_paths_and_arguments_are_refused_like_node() {
    let error = compare("unsafe-file", None, |dir| {
        write(dir, ".aidlc", "not a directory")
    })
    .unwrap_err();
    assert_eq!(error, "Refusing unsafe .aidlc path");

    let error = compare("unsafe-intents", None, |dir| {
        write(dir, ".aidlc/intents", "file")
    })
    .unwrap_err();
    assert_eq!(error, "Refusing unsafe .aidlc/intents path");

    // A symlinked .aidlc (skipped where the platform refuses to create symlinks).
    let a = TempDir::new("unsafe-link");
    let b = TempDir::new("unsafe-link");
    let mut linked = true;
    for dir in [a.path(), b.path()] {
        let real = dir.join("real-state");
        fs::create_dir_all(&real).unwrap();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, dir.join(".aidlc"));
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&real, dir.join(".aidlc"));
        linked &= made.is_ok();
    }
    if linked {
        let error = compare_roots(a.path(), b.path()).unwrap_err();
        assert_eq!(error, "Refusing to access symlink .aidlc directory");
    }

    let project = TempDir::new("args");
    let error = doctor::run(
        "doctor",
        project.path(),
        &["extra".into()],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert_eq!(error, "Unexpected argument 'extra' for 'doctor'.");
    let mut flags = BTreeMap::new();
    flags.insert("force".to_owned(), FlagValue::Boolean(true));
    let flag_error = doctor::run("doctor", project.path(), &[], &flags).unwrap_err();
    assert_eq!(
        flag_error,
        "Unknown option --force for 'doctor'. Allowed options: --project."
    );
    if node_available() {
        let (code, _, stderr) = node(project.path(), &["doctor", "extra"]);
        assert_eq!(code, 1);
        assert_eq!(stderr.trim_end(), format!("dtech-aidlc: {error}"));
        let (code, _, stderr) = node(project.path(), &["doctor", "--force"]);
        assert_eq!(code, 1);
        assert_eq!(stderr.trim_end(), format!("dtech-aidlc: {flag_error}"));
    }
    assert!(doctor::run("status", project.path(), &[], &BTreeMap::new()).is_err());
    assert!(
        fs::read_dir(project.path()).unwrap().next().is_none(),
        "doctor wrote files"
    );
}
