use dtech_aidlc::scope::load_graph;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn graph_scope_output_matches_node_for_unconfigured_project() {
    let temp = std::env::temp_dir().join(format!("aidlc-scope-parity-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let (graph, config) = load_graph(repo.join("core"), &temp).unwrap();
    for scope in [None, Some("solo"), Some("team"), Some("enterprise")] {
        for depth in [None, Some("standard"), Some("fast")] {
            let mut command = Command::new("node");
            command
                .arg(repo.join("core/tools/dtech-aidlc.mjs"))
                .arg("scope");
            if let Some(scope) = scope {
                command.arg(scope);
            }
            if let Some(depth) = depth {
                command.args(["--depth", depth]);
            }
            command.args(["--project", temp.to_str().unwrap()]);
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                graph.scope_output(scope, depth, &config).unwrap(),
                expected,
                "scope={scope:?}, depth={depth:?}"
            );
            let mut native = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc-rs"));
            native.arg("scope");
            if let Some(scope) = scope {
                native.arg(scope);
            }
            if let Some(depth) = depth {
                native.args(["--depth", depth]);
            }
            let native = native
                .args(["--project", temp.to_str().unwrap()])
                .output()
                .unwrap();
            assert!(
                native.status.success(),
                "{}",
                String::from_utf8_lossy(&native.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&native.stdout).unwrap(),
                expected
            );
        }
    }
    std::fs::create_dir_all(temp.join(".aidlc")).unwrap();
    for config in [
        serde_json::json!({"harness":"deepseek","scope":"solo"}),
        serde_json::json!({"harness":"claude","scope":"solo","agentModels":{"brief-analyst":"opus"}}),
    ] {
        std::fs::write(
            temp.join(".aidlc/config.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let (graph, loaded) = load_graph(repo.join("core"), &temp).unwrap();
        let output = Command::new("node")
            .arg(repo.join("core/tools/dtech-aidlc.mjs"))
            .args(["scope", "solo", "--project", temp.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            graph.scope_output(Some("solo"), None, &loaded).unwrap(),
            expected
        );
        let native = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc-rs"))
            .args(["scope", "solo", "--project", temp.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&native.stdout).unwrap(),
            expected
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
