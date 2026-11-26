//! `scope` output of the library and the binary against goldens recorded from the former
//! JavaScript reference CLI (`tests/golden/scope_golden/`). Set
//! `DTECH_AIDLC_BLESS=1` to re-record them.

#[path = "common/golden.rs"]
mod golden;

use dtech_aidlc::scope::load_graph;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

const SUITE: &str = "scope_golden";

/// The CLI under test.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
}

fn normalise(value: &Value, project: &Path) -> Value {
    let repo = golden::repo();
    golden::normalise_json(value, &[(repo.as_path(), "<REPO>"), (project, "<ROOT>")])
}

/// Runs `scope [SCOPE] [--depth DEPTH] --project <project>`, which must succeed.
fn scope_json(project: &Path, scope: Option<&str>, depth: Option<&str>) -> Value {
    let mut command = cli();
    command.arg("scope");
    if let Some(scope) = scope {
        command.arg(scope);
    }
    if let Some(depth) = depth {
        command.args(["--depth", depth]);
    }
    let output = command
        .args(["--project", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    normalise(&serde_json::from_slice(&output.stdout).unwrap(), project)
}

#[test]
fn graph_scope_output_matches_the_golden_for_unconfigured_project() {
    let temp = std::env::temp_dir().join(format!("aidlc-scope-parity-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let repo = golden::repo();
    let mut cases = golden::Cases::open(SUITE, "scope-output");
    let (graph, config) = load_graph(repo.join("core"), &temp).unwrap();
    for scope in [None, Some("solo"), Some("team"), Some("enterprise")] {
        for depth in [None, Some("standard"), Some("fast")] {
            let native = scope_json(&temp, scope, depth);
            let key = format!(
                "unconfigured scope={} depth={}",
                scope.unwrap_or("(default)"),
                depth.unwrap_or("(default)")
            );
            let expected = cases.expect(&key, native.clone());
            assert_eq!(
                normalise(&graph.scope_output(scope, depth, &config).unwrap(), &temp),
                expected,
                "scope={scope:?}, depth={depth:?}"
            );
            assert_eq!(native, expected, "scope={scope:?}, depth={depth:?}");
        }
    }
    std::fs::create_dir_all(temp.join(".aidlc")).unwrap();
    for (name, config) in [
        (
            "deepseek",
            serde_json::json!({"harness":"deepseek","scope":"solo"}),
        ),
        (
            "claude-agent-models",
            serde_json::json!({"harness":"claude","scope":"solo","agentModels":{"brief-analyst":"opus"}}),
        ),
    ] {
        std::fs::write(
            temp.join(".aidlc/config.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let (graph, loaded) = load_graph(repo.join("core"), &temp).unwrap();
        let native = scope_json(&temp, Some("solo"), None);
        let expected = cases.expect(&format!("configured {name} scope=solo"), native.clone());
        assert_eq!(
            normalise(
                &graph.scope_output(Some("solo"), None, &loaded).unwrap(),
                &temp
            ),
            expected,
            "{name}"
        );
        assert_eq!(native, expected, "{name}");
    }
    cases.finish();
    std::fs::remove_dir_all(temp).unwrap();
}
