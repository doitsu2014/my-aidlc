use dtech_aidlc::{scope::load_graph, status};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn status_matches_node_for_fresh_and_active_projects() {
    let project = std::env::temp_dir().join(format!(
        "aidlc-status-parity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&project).unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli = repo.join("core/tools/dtech-aidlc.mjs");
    let run = |args: &[&str]| {
        let output = Command::new("node")
            .arg(&cli)
            .args(args)
            .arg("--project")
            .arg(&project)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let (graph, _) = load_graph(repo.join("core"), &project).unwrap();
    assert_eq!(
        status::status(&project, &graph, None).unwrap(),
        run(&["status"])
    );
    run(&["init", "--harness", "claude", "--scope", "solo"]);
    assert_eq!(
        status::status(&project, &graph, None).unwrap(),
        run(&["status"])
    );
    run(&["orchestrate", "next", "--intent", "parity status"]);
    for args in [&["status"][..], &["intent", "list"][..]] {
        let expected = run(args);
        let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(args)
            .arg("--project")
            .arg(&project)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            expected
        );
    }
    assert_eq!(
        status::status(&project, &graph, None).unwrap(),
        run(&["status"])
    );
    assert_eq!(
        status::list_intents(&project, &graph).unwrap(),
        run(&["intent", "list"])
    );
    fs::write(project.join(".aidlc/intents/not-a-directory"), "user file").unwrap();
    assert_eq!(
        status::list_intents(&project, &graph).unwrap(),
        run(&["intent", "list"])
    );
    let state_path = project.join(".aidlc/intents/parity-status/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    state["currentStage"] = serde_json::json!("not-a-stage");
    fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(status::status(&project, &graph, None).is_err());
    let node_error = Command::new("node")
        .arg(&cli)
        .arg("status")
        .arg("--project")
        .arg(&project)
        .output()
        .unwrap();
    assert!(!node_error.status.success());
    fs::remove_dir_all(project).unwrap();
}
