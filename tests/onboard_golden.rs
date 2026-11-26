use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp(label: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aidlc-rust-onboard-{label}-{}-{n}",
        std::process::id()
    ))
}
fn run(binary: &Path, args: &[&str], root: &Path) -> std::process::Output {
    Command::new(binary)
        .args(args)
        .args(["--project", root.to_str().unwrap()])
        .output()
        .unwrap()
}
fn parse_output(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn rust_onboard_projects_assets_and_initializes_knowledge_and_audit() {
    let root = temp("full");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("docs/requirements.md"), "requirements\n").unwrap();
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let output = parse_output(&run(
        &exe,
        &["onboard", "--harness", "claude", "--scope", "solo"],
        &root,
    ));
    assert_eq!(output["initialized"], true);
    assert_eq!(output["kind"], "brownfield");
    assert_eq!(output["knowledge"]["manifestStatus"], "created");
    assert!(!output["knowledge"]["added"].as_array().unwrap().is_empty());
    assert!(root.join(".claude/skills/dtech-aidlc/SKILL.md").is_file());
    assert!(root
        .join(".claude/skills/dtech-aidlc/onboarding.md")
        .is_file());
    let profile: Value =
        serde_json::from_slice(&fs::read(root.join(".aidlc/shared/project-profile.json")).unwrap())
            .unwrap();
    assert!(profile["scannedAt"].as_str().unwrap().ends_with('Z'));
    assert!(profile["documentation"]["detected"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["path"] == "docs"));
    assert!(root.join("aidlc.knowledge.json").is_file());
    assert!(root.join(".aidlc/shared/sources-snapshot.json").is_file());
    let audit = fs::read_to_string(root.join(".aidlc/audit.jsonl")).unwrap();
    let rows = audit
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.iter()
            .map(|r| r["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "project.initialized",
            "knowledge.sources.registered",
            "project.onboarded"
        ]
    );
    let rerun = parse_output(&run(
        &exe,
        &["onboard", "--harness", "claude", "--scope", "solo"],
        &root,
    ));
    assert_eq!(rerun["knowledge"]["manifestStatus"], "unchanged");
    assert_eq!(rerun["knowledge"]["added"], serde_json::json!([]));
    assert!(root.join(".claude/skills/dtech-aidlc/SKILL.md").is_file());
    fs::remove_dir_all(root).unwrap();
}

/// Fields of `onboard` output that the reference implementation recorded in the golden file.
fn subset(output: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|key| ((*key).to_owned(), output[*key].clone()))
            .collect(),
    )
}

#[test]
fn rust_onboard_output_and_side_effects_match_golden_for_greenfield() {
    let rust = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let root = temp("rust");
    fs::create_dir_all(&root).unwrap();
    let rust_out = parse_output(&run(
        &rust,
        &["onboard", "--harness", "deepseek", "--scope", "team"],
        &root,
    ));
    let keys = [
        "initialized",
        "harness",
        "harnesses",
        "scope",
        "kind",
        "languages",
        "architecture",
    ];
    assert_golden("greenfield-deepseek-team", &subset(&rust_out, &keys));
    assert!(root.join(".dsh/skills/aidlc/SKILL.md").is_file());
    assert!(root.join(".aidlc/shared/project-profile.json").is_file());
    assert!(root.join(".aidlc/shared/sources-snapshot.json").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn onboard_reruns_match_golden_idempotent_registration_behavior() {
    let rust = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let root = temp("rust-rerun");
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "guide\n").unwrap();
    let args = ["onboard", "--harness", "claude", "--scope", "solo"];
    parse_output(&run(&rust, &args, &root));
    let rust_rerun = parse_output(&run(&rust, &args, &root));
    let mut recorded = subset(&rust_rerun, &["initialized", "harness", "scope", "kind"]);
    recorded["knowledge"] = subset(&rust_rerun["knowledge"], &["manifestStatus", "added"]);
    assert_golden("rerun-claude-solo", &recorded);
    assert_eq!(rust_rerun["knowledge"]["manifestStatus"], "unchanged");
    assert_eq!(rust_rerun["knowledge"]["added"], serde_json::json!([]));
    let audit = fs::read_to_string(root.join(".aidlc/audit.jsonl")).unwrap();
    let rows = audit
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let count = |event: &str| rows.iter().filter(|r| r["event"] == event).count();
    assert_eq!(count("project.initialized"), 1);
    assert_eq!(count("knowledge.sources.registered"), 1);
    assert_eq!(count("project.onboarded"), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rescan_preserves_configuration_and_matches_golden_document_registration_rules() {
    let rust = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let root = temp("rust-rescan");
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("docs/guide.md"), "guide\n").unwrap();
    parse_output(&run(
        &rust,
        &["onboard", "--harness", "claude", "--scope", "solo"],
        &root,
    ));
    let config_before = fs::read(root.join(".aidlc/config.json")).unwrap();
    fs::write(root.join("docs/new.md"), "new\n").unwrap();
    let native = parse_output(&run(&rust, &["onboard", "--rescan"], &root));
    assert_golden("rescan-new-document", &rescan_summary(&native));
    assert_eq!(
        fs::read(root.join(".aidlc/config.json")).unwrap(),
        config_before
    );
    fs::remove_dir_all(root).unwrap();
}

/// The parts of `onboard --rescan` output the golden file records.
fn rescan_summary(output: &Value) -> Value {
    serde_json::json!({
        "rescanned": output["rescanned"],
        "manifestStatus": output["knowledge"]["manifestStatus"],
        "wouldAdd": output["knowledge"]["wouldAdd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["path"].clone())
            .collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Golden files: tests/golden/rust_onboard_parity/<case>.json. The expectations were
// recorded from the reference implementation; set DTECH_AIDLC_BLESS=1 to rewrite them
// from the native CLI after an intentional change.
// ---------------------------------------------------------------------------

fn golden_path(case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/rust_onboard_parity")
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

fn assert_golden(case: &str, actual: &Value) {
    if blessing() {
        write_golden(case, actual);
        return;
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
}
