use std::process::Command;

#[test]
fn reports_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .arg("--version")
        .output()
        .expect("run Rust CLI");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("dtech-aidlc {}\n", env!("CARGO_PKG_VERSION"))
    );
}

fn audit_events(project: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(project.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            row["event"].as_str().unwrap().to_owned()
        })
        .collect()
}

#[test]
fn onboard_initializes_metadata_and_orchestration_starts_an_intent() {
    let project = std::env::temp_dir().join(format!("aidlc-rust-onboard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/main.rs"), "fn main() {}\n").unwrap();
    let onboard = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .current_dir(&project)
        .args(["onboard", "--harness", "claude", "--scope", "solo"])
        .output()
        .expect("run Rust CLI onboard");
    assert!(
        onboard.status.success(),
        "{}",
        String::from_utf8_lossy(&onboard.stderr)
    );
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".aidlc/config.json")).unwrap())
            .unwrap();
    assert_eq!(config["harness"], "claude");
    let next = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .current_dir(&project)
        .args([
            "orchestrate",
            "next",
            "--intent",
            "Implement a sample feature",
        ])
        .output()
        .expect("run Rust CLI orchestrate next");
    assert!(
        next.status.success(),
        "{}",
        String::from_utf8_lossy(&next.stderr)
    );
    let directive: serde_json::Value = serde_json::from_slice(&next.stdout).unwrap();
    assert_eq!(directive["intentId"], "implement-a-sample-feature");
    assert_eq!(directive["stage"]["id"], "problem-framing");
    let pointer: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".aidlc/state.json")).unwrap()).unwrap();
    assert_eq!(pointer["version"], 2);
    assert_eq!(pointer["activeIntentId"], "implement-a-sample-feature");
    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(project.join(".aidlc/intents/implement-a-sample-feature/state.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state["currentStage"], "problem-framing");
    let events = audit_events(&project);
    assert!(
        events.ends_with(&["intent.created".to_owned(), "stage.selected".to_owned()]),
        "{events:?}"
    );
    let config_before = std::fs::read(project.join(".aidlc/config.json")).unwrap();
    for flag in [
        &["--harness", "claude"][..],
        &["--scope", "solo"][..],
        &["--no-detect"][..],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .current_dir(&project)
            .args(["onboard", "--rescan"])
            .args(flag)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{flag:?}");
        assert_eq!(
            std::fs::read(project.join(".aidlc/config.json")).unwrap(),
            config_before
        );
    }
    let existing_state = b"{\"schemaVersion\":2,\"activeIntentId\":\"existing\"}\n";
    std::fs::write(project.join(".aidlc/state.json"), existing_state).unwrap();
    for args in [
        &["orchestrate", "next"][..],
        &["orchestrate", "report", "--result", "completed"][..],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .current_dir(&project)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(
            std::fs::read(project.join(".aidlc/state.json")).unwrap(),
            existing_state
        );
    }
    std::fs::remove_dir_all(project).unwrap();
}

fn run_in(project: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(args)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run Rust CLI")
}

fn stdout_json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {} / stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn orchestration_lifecycle_reports_blocks_reworks_and_passes_a_gate() {
    let project = std::env::temp_dir().join(format!("aidlc-rust-lifecycle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).unwrap();
    assert!(run_in(
        &project,
        &["onboard", "--harness", "claude", "--scope", "solo"]
    )
    .status
    .success());

    // A failing preflight prerequisite holds a new intent back with exit code 3 and
    // writes nothing.
    let config_path = project.join(".aidlc/config.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    config["prerequisites"] =
        serde_json::json!({"toolchain": {"check": "exit 7", "stages": ["problem-framing"]}});
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let blocked = run_in(
        &project,
        &["orchestrate", "next", "--intent", "Lifecycle work"],
    );
    assert_eq!(blocked.status.code(), Some(3));
    assert_eq!(stdout_json(&blocked)["blocked"], true);
    assert!(!project.join(".aidlc/state.json").exists());
    assert!(!project.join(".aidlc/intents/lifecycle-work").exists());
    config.as_object_mut().unwrap().remove("prerequisites");
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    // The intent text can come from standard input.
    let mut child = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(["orchestrate", "next", "--intent-file", "-", "--project"])
        .arg(&project)
        .env_remove("DTECH_AIDLC_HOME")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"\xEF\xBB\xBFLifecycle work\r\n")
            .unwrap();
    }
    let started = child.wait_with_output().unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let directive = stdout_json(&started);
    assert_eq!(directive["intentId"], "lifecycle-work");
    assert_eq!(directive["intent"], "Lifecycle work");
    let state_path = project.join(".aidlc/intents/lifecycle-work/state.json");
    let pointer_path = project.join(".aidlc/state.json");
    let pointer = std::fs::read(&pointer_path).unwrap();

    // Completing without the artifacts is refused and changes nothing.
    let state_before = std::fs::read(&state_path).unwrap();
    let early = run_in(&project, &["orchestrate", "report", "--result", "complete"]);
    assert_eq!(early.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&early.stderr).contains("cannot be completed"));
    assert_eq!(std::fs::read(&state_path).unwrap(), state_before);

    // Blocked, then the next selection is a rework of the same stage.
    let report = run_in(
        &project,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "need input",
        ],
    );
    assert!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    assert_eq!(stdout_json(&report)["result"], "blocked");
    let rework = stdout_json(&run_in(&project, &["orchestrate", "next"]));
    assert_eq!(rework["stage"]["id"], "problem-framing");
    assert_eq!(rework["stage"]["rework"]["detail"], "need input");
    assert_eq!(rework["stage"]["rework"]["attempt"], 2);

    // Complete the first stage, then pass the Analyze gate with an explicit approval.
    let write_artifacts = |directive: &serde_json::Value| {
        for artifact in directive["artifacts"].as_array().unwrap() {
            let path = project.join(artifact["path"].as_str().unwrap());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "# Artifact\n\nContent.\n").unwrap();
        }
    };
    write_artifacts(&rework);
    let done = run_in(&project, &["orchestrate", "report", "--result", "complete"]);
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let gate_stage = stdout_json(&run_in(&project, &["orchestrate", "next"]));
    assert_eq!(gate_stage["stage"]["id"], "context-and-risk");
    assert_eq!(gate_stage["stage"]["phaseGate"], true);
    write_artifacts(&gate_stage);
    let unapproved = run_in(&project, &["orchestrate", "report", "--result", "complete"]);
    assert_eq!(unapproved.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unapproved.stderr).contains("requires explicit --approval"));
    let approved = run_in(
        &project,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--approval",
            "approved",
            "--approved-by",
            "Pat Owner",
        ],
    );
    assert!(
        approved.status.success(),
        "{}",
        String::from_utf8_lossy(&approved.stderr)
    );
    let approved = stdout_json(&approved);
    assert_eq!(approved["gate"], "analysis-baseline");
    assert_eq!(approved["approvedBy"], "Pat Owner");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    assert_eq!(state["gates"]["analysis-baseline"]["decision"], "approved");
    assert_eq!(state["gates"]["analysis-baseline"]["bySource"], "flag");
    let next = stdout_json(&run_in(&project, &["orchestrate", "next"]));
    assert_eq!(next["stage"]["id"], "option-generation");
    // Updates of the active intent never rewrite the pointer.
    assert_eq!(std::fs::read(&pointer_path).unwrap(), pointer);
    let expected = [
        "intent.created",
        "stage.selected",
        "stage.reported",
        "stage.selected",
        "stage.reported",
        "stage.selected",
        "stage.reported",
        "gate.decided",
        "stage.selected",
    ]
    .map(str::to_owned);
    let events = audit_events(&project);
    assert!(events.ends_with(&expected), "{events:?}");
    std::fs::remove_dir_all(project).unwrap();
}

/// Regression: the audit log was checked only when appending, after the intent state and
/// pointer were written, so a directory at `.aidlc/audit.jsonl` left state recorded with
/// no audit row. `orchestrate next` and `report` now check it before their first write.
#[test]
fn orchestrate_refuses_a_broken_audit_log_before_writing_state() {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let project = std::env::temp_dir().join(format!(
        "aidlc-rust-audit-preflight-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).unwrap();
    assert!(run_in(
        &project,
        &["onboard", "--harness", "claude", "--scope", "solo"]
    )
    .status
    .success());
    let audit = project.join(".aidlc/audit.jsonl");
    let break_log = || {
        let _ = std::fs::remove_file(&audit);
        std::fs::create_dir_all(&audit).unwrap();
    };
    let restore_log = || std::fs::remove_dir_all(&audit).unwrap();
    let refused = |output: std::process::Output| {
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).trim(),
            "dtech-aidlc: Refusing unsafe .aidlc/audit.jsonl path"
        );
    };

    break_log();
    refused(run_in(
        &project,
        &["orchestrate", "next", "--intent", "Audit preflight work"],
    ));
    assert!(!project.join(".aidlc/state.json").exists());
    assert!(!project.join(".aidlc/intents").exists());

    restore_log();
    let started = run_in(
        &project,
        &["orchestrate", "next", "--intent", "Audit preflight work"],
    );
    let intent_id = stdout_json(&started)["intentId"]
        .as_str()
        .unwrap()
        .to_owned();
    let state_path = project.join(format!(".aidlc/intents/{intent_id}/state.json"));
    let pointer = std::fs::read(project.join(".aidlc/state.json")).unwrap();
    let state = std::fs::read(&state_path).unwrap();

    break_log();
    refused(run_in(&project, &["orchestrate", "next"]));
    refused(run_in(
        &project,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "wait",
        ],
    ));
    assert_eq!(
        std::fs::read(project.join(".aidlc/state.json")).unwrap(),
        pointer
    );
    assert_eq!(std::fs::read(&state_path).unwrap(), state);
    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn init_alias_initializes_a_project() {
    let project = std::env::temp_dir().join(format!("aidlc-rust-init-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .current_dir(&project)
        .args(["init", "--harness", "deepseek"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".aidlc/config.json")).unwrap())
            .unwrap();
    assert_eq!(config["harness"], "deepseek");
    assert_eq!(config["scope"], "solo");
    let rerun = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .current_dir(&project)
        .arg("onboard")
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(project.join(".aidlc/config.json")).unwrap()
        )
        .unwrap(),
        config
    );
    std::fs::remove_dir_all(project).unwrap();
}

/// The commands that used to be refused ("not yet implemented") are routed to their
/// native handlers: read-only and refused commands leave a new project untouched, and
/// `knowledge init` writes only what Node writes.
#[test]
fn formerly_unrouted_commands_run_natively_without_unexpected_writes() {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let project = std::env::temp_dir().join(format!(
        "aidlc-rust-routed-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .current_dir(&project)
            .args(args)
            .env_remove("DTECH_AIDLC_HOME")
            .output()
            .expect("run Rust CLI")
    };

    let doctor = run(&["doctor"]);
    assert_eq!(doctor.status.code(), Some(0), "{doctor:?}");
    let report = stdout_json(&doctor);
    assert_eq!(report["ok"], true);
    assert!(report["checks"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty()));
    assert!(!project.join(".aidlc").exists());

    for (args, message) in [
        (
            &["config", "scope", "team"][..],
            "Initialize the project before changing the default scope",
        ),
        (&["harness", "refresh"][..], "Project is not initialized; run: dtech-aidlc onboard --harness claude|deepseek --scope <solo|team|enterprise>"),
        (&["intent", "pause"][..], "No unfinished active intent to pause"),
        (&["deps", "list"][..], "Initialize a workflow before managing dependencies"),
        (&["frobnicate"][..], "Unknown command frobnicate; use dtech-aidlc help"),
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("dtech-aidlc: {message}\n"),
            "{args:?}"
        );
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!project.join(".aidlc").exists(), "{args:?}");
    }
    for args in [
        &["config", "scope"][..],
        &["harness", "status"],
        &["repos", "list"],
    ] {
        let output = run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        stdout_json(&output);
        assert!(!project.join(".aidlc").exists(), "{args:?}");
    }

    let init = run(&["knowledge", "init"]);
    assert!(init.status.success(), "{init:?}");
    assert_eq!(stdout_json(&init)["project"]["created"], true);
    assert!(project.join("PROJECT.md").is_file());
    assert!(project.join(".aidlc/knowledge").is_dir());
    assert!(!project.join(".aidlc/config.json").exists());
    std::fs::remove_dir_all(&project).unwrap();
}
