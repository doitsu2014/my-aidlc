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

#[test]
fn rust_onboard_output_and_side_effects_compare_with_node_for_greenfield() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let node = repo.join("core/tools/dtech-aidlc.mjs");
    let rust = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let roots = [temp("node"), temp("rust")];
    for root in &roots {
        fs::create_dir_all(root).unwrap();
    }
    let node_out = parse_output(
        &Command::new("node")
            .arg(&node)
            .args([
                "onboard",
                "--harness",
                "deepseek",
                "--scope",
                "team",
                "--project",
                roots[0].to_str().unwrap(),
            ])
            .output()
            .unwrap(),
    );
    let rust_out = parse_output(&run(
        &rust,
        &["onboard", "--harness", "deepseek", "--scope", "team"],
        &roots[1],
    ));
    for key in [
        "initialized",
        "harness",
        "harnesses",
        "scope",
        "kind",
        "languages",
        "architecture",
    ] {
        assert_eq!(rust_out[key], node_out[key], "field {key}");
    }
    for root in &roots {
        assert!(root.join(".dsh/skills/aidlc/SKILL.md").is_file());
        assert!(root.join(".aidlc/shared/project-profile.json").is_file());
        assert!(root.join(".aidlc/shared/sources-snapshot.json").is_file());
    }
    for root in roots {
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn onboard_reruns_match_node_idempotent_registration_behavior() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli = repo.join("core/tools/dtech-aidlc.mjs");
    let rust = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let roots = [temp("node-rerun"), temp("rust-rerun")];
    for root in &roots {
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/guide.md"), "guide\n").unwrap();
    }
    for root in &roots {
        parse_output(
            &Command::new("node")
                .arg(&cli)
                .args([
                    "onboard",
                    "--harness",
                    "claude",
                    "--scope",
                    "solo",
                    "--project",
                    root.to_str().unwrap(),
                ])
                .output()
                .unwrap(),
        );
    }
    let node_rerun = parse_output(
        &Command::new("node")
            .arg(&cli)
            .args([
                "onboard",
                "--harness",
                "claude",
                "--scope",
                "solo",
                "--project",
                roots[0].to_str().unwrap(),
            ])
            .output()
            .unwrap(),
    );
    let rust_rerun = parse_output(&run(
        &rust,
        &["onboard", "--harness", "claude", "--scope", "solo"],
        &roots[1],
    ));
    for key in ["initialized", "harness", "scope", "kind"] {
        assert_eq!(rust_rerun[key], node_rerun[key], "field {key}");
    }
    assert_eq!(rust_rerun["knowledge"]["manifestStatus"], "unchanged");
    assert_eq!(node_rerun["knowledge"]["manifestStatus"], "unchanged");
    assert_eq!(rust_rerun["knowledge"]["added"], serde_json::json!([]));
    assert_eq!(node_rerun["knowledge"]["added"], serde_json::json!([]));
    for root in &roots {
        let audit = fs::read_to_string(root.join(".aidlc/audit.jsonl")).unwrap();
        let rows = audit
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == "project.initialized")
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == "knowledge.sources.registered")
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == "project.onboarded")
                .count(),
            2
        );
    }
    for root in roots {
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn rescan_preserves_configuration_and_matches_node_document_registration_rules() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli = repo.join("core/tools/dtech-aidlc.mjs");
    let rust = PathBuf::from(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let roots = [temp("node-rescan"), temp("rust-rescan")];
    for root in &roots {
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("docs/guide.md"), "guide\n").unwrap();
    }
    for root in &roots {
        parse_output(
            &Command::new("node")
                .arg(&cli)
                .args([
                    "onboard",
                    "--harness",
                    "claude",
                    "--scope",
                    "solo",
                    "--project",
                    root.to_str().unwrap(),
                ])
                .output()
                .unwrap(),
        );
    }
    let config_before = fs::read(roots[1].join(".aidlc/config.json")).unwrap();
    fs::write(roots[0].join("docs/new.md"), "new\n").unwrap();
    fs::write(roots[1].join("docs/new.md"), "new\n").unwrap();
    let node = parse_output(
        &Command::new("node")
            .arg(&cli)
            .args([
                "onboard",
                "--rescan",
                "--project",
                roots[0].to_str().unwrap(),
            ])
            .output()
            .unwrap(),
    );
    let native = parse_output(&run(&rust, &["onboard", "--rescan"], &roots[1]));
    assert_eq!(native["rescanned"], node["rescanned"]);
    assert_eq!(
        native["knowledge"]["manifestStatus"],
        node["knowledge"]["manifestStatus"]
    );
    assert_eq!(
        native["knowledge"]["wouldAdd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["path"].clone())
            .collect::<Vec<_>>(),
        node["knowledge"]["wouldAdd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["path"].clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read(roots[1].join(".aidlc/config.json")).unwrap(),
        config_before
    );
    for root in roots {
        fs::remove_dir_all(root).unwrap();
    }
}
