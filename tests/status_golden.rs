//! `status` and `intent list` of the library and the binary, for a fresh, an initialized
//! and an active project, against goldens recorded from the former JavaScript reference
//! CLI (`tests/golden/rust_status_parity/`). Set `DTECH_AIDLC_BLESS=1` to re-record them.

#[path = "common/golden.rs"]
mod golden;

use dtech_aidlc::{scope::load_graph, status};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

const SUITE: &str = "rust_status_parity";

/// The CLI under test.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
}

fn normalise_text(text: &str, project: &Path) -> String {
    let repo = golden::repo();
    golden::normalise(text, &[(repo.as_path(), "<REPO>"), (project, "<ROOT>")])
}

fn normalise(value: &Value, project: &Path) -> Value {
    serde_json::from_str(&normalise_text(
        &serde_json::to_string(value).unwrap(),
        project,
    ))
    .unwrap()
}

fn run_cli(project: &Path, args: &[&str]) -> Output {
    cli()
        .args(args)
        .arg("--project")
        .arg(project)
        .output()
        .unwrap()
}

#[test]
fn status_matches_the_golden_for_fresh_and_active_projects() {
    let project = std::env::temp_dir().join(format!(
        "aidlc-status-parity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&project).unwrap();
    let repo = golden::repo();
    let mut cases = golden::Cases::open(SUITE, "status");
    // Runs a command that must succeed and returns its normalised JSON output.
    let run = |args: &[&str]| {
        let output = run_cli(&project, args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        normalise(&serde_json::from_slice(&output.stdout).unwrap(), &project)
    };
    let (graph, _) = load_graph(repo.join("core"), &project).unwrap();
    let library_status = || normalise(&status::status(&project, &graph, None).unwrap(), &project);
    let library_intents = || normalise(&status::list_intents(&project, &graph).unwrap(), &project);

    let expected = cases.expect("fresh status", run(&["status"]));
    assert_eq!(library_status(), expected);

    run(&["init", "--harness", "claude", "--scope", "solo"]);
    let expected = cases.expect("initialized status", run(&["status"]));
    assert_eq!(library_status(), expected);

    run(&["orchestrate", "next", "--intent", "parity status"]);
    for (key, args) in [
        ("active status", &["status"][..]),
        ("active intent list", &["intent", "list"][..]),
    ] {
        let native = run(args);
        let expected = cases.expect(key, native.clone());
        assert_eq!(native, expected, "{args:?}");
    }
    assert_eq!(
        library_status(),
        cases.expect("active status", run(&["status"]))
    );
    assert_eq!(
        library_intents(),
        cases.expect("active intent list", run(&["intent", "list"]))
    );

    fs::write(project.join(".aidlc/intents/not-a-directory"), "user file").unwrap();
    let native = run(&["intent", "list"]);
    let expected = cases.expect("intent list with a stray file", native.clone());
    assert_eq!(native, expected);
    assert_eq!(library_intents(), expected);

    let state_path = project.join(".aidlc/intents/parity-status/state.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    state["currentStage"] = json!("not-a-stage");
    fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(status::status(&project, &graph, None).is_err());
    let error = run_cli(&project, &["status"]);
    assert!(!error.status.success());
    let actual = json!({
        "code": error.status.code(),
        "stderr": normalise_text(&String::from_utf8_lossy(&error.stderr), &project),
    });
    let expected = cases.expect("status with an unknown current stage", actual.clone());
    assert_eq!(actual, expected);
    cases.finish();
    fs::remove_dir_all(project).unwrap();
}
