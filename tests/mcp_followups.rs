//! Characterisation tests for intent-1 MCP behaviour that had no test (TS-033, FN-044):
//! the AI-mode gate with the policy edited between calls, an unsafe state layout at
//! launch and mid-session, CLI and MCP interleaving, and restart or shutdown mid-session.
//! Every spawn passes `--telemetry off` and `--project <Tmp>`.

#[path = "fixtures/mcp/common.rs"]
mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Barrier};

const INTENT: &str = "Add login page";
const ID: &str = "add-login-page";
const AI: &str = "AI Orchestrator";

/// A long-lived `dtech-aidlc-mcp --project <dir> --telemetry off` driven line by line.
struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn start(project: &Path) -> Self {
        let mut child = Command::new(MCP_BIN)
            .arg("--project")
            .arg(project)
            .args(["--telemetry", "off"])
            .env_remove("DTECH_AIDLC_HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Session {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    /// One modern `tools/call`; returns (structuredContent, isError).
    fn call(&mut self, name: &str, args: Value) -> (Value, bool) {
        self.next_id += 1;
        let request = modern(
            self.next_id,
            "tools/call",
            json!({"name": name, "arguments": args}),
        );
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        assert!(
            self.stdout.read_line(&mut line).unwrap() > 0,
            "server closed"
        );
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], json!(self.next_id), "{reply}");
        let result = &reply["result"];
        (
            result["structuredContent"].clone(),
            result["isError"].as_bool().unwrap(),
        )
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (value, is_error) = self.call(name, args);
        assert!(!is_error, "{name} {value}");
        value
    }

    fn err(&mut self, name: &str, args: Value) -> Value {
        let (value, is_error) = self.call(name, args);
        assert!(is_error, "{name} {value}");
        value["error"].clone()
    }

    /// Close stdin (EOF) and wait: the exit code and any remaining stdout.
    fn shutdown(mut self) -> (Option<i32>, String) {
        drop(self.stdin.take());
        let mut rest = String::new();
        let _ = self.stdout.read_to_string(&mut rest);
        (self.child.wait().unwrap().code(), rest)
    }

    /// Kill the process without a clean EOF.
    fn kill(mut self) {
        self.child.kill().unwrap();
        let _ = self.child.wait();
    }
}

fn state(project: &Path) -> Value {
    let path = project.join(format!(".aidlc/intents/{ID}/state.json"));
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn reports(project: &Path) -> usize {
    state(project)["stages"]["problem-framing"]["reports"]
        .as_array()
        .map_or(0, Vec::len)
}

fn count(project: &Path, event: &str) -> usize {
    audit_events(project).iter().filter(|e| *e == event).count()
}

fn message(error: &Value) -> &str {
    assert_eq!(error["code"], "operation_failed", "{error}");
    error["message"].as_str().unwrap()
}

fn started(label: &str) -> Tmp {
    let project = Tmp::project(label);
    start_intent(&project.0, INTENT);
    project
}

fn call_frame(id: u64, name: &str, args: Value) -> Value {
    modern(id, "tools/call", json!({"name": name, "arguments": args}))
}

#[test]
fn ts033_ai_mode_gate_follows_the_policy_edited_between_calls() {
    let project = Tmp::project("fu-ai-gate");
    write_config(&project.0, |c| c["mode"] = json!("ai"));
    start_intent(&project.0, INTENT);
    let first = cli_ok(&project.0, &["orchestrate", "next"]);
    write_artifacts(&project.0, &first);
    cli_ok(
        &project.0,
        &["orchestrate", "report", "--result", "complete"],
    );
    let gate = cli_ok(&project.0, &["orchestrate", "next"]);
    assert_eq!(gate["stage"]["phaseGate"], true, "{gate}");
    write_artifacts(&project.0, &gate);
    let recommendation = project.0.join(format!(
        ".aidlc/intents/{ID}/{}/{}/gate-recommendation.md",
        gate["stage"]["phase"].as_str().unwrap(),
        gate["stage"]["id"].as_str().unwrap()
    ));
    let approve =
        |who: &str| json!({"result": "complete", "approval": "approved", "approved_by": who});
    let mut session = Session::start(&project.0);
    let before = snapshot(&project.0);

    // No recommendation yet: the AI label is refused with the reason.
    let e = session.err("report", approve(AI));
    assert!(
        message(&e).contains("requires a gate-recommendation artifact"),
        "{e}"
    );
    // A human label cannot approve an AI-mode intent's gate.
    let e = session.err("report", approve("Pat Owner"));
    assert!(message(&e).contains("runs in AI orchestrator mode"), "{e}");
    assert_eq!(snapshot(&project.0), before, "refusals wrote nothing");

    // A recommendation with an open written-approval item blocks AI approval.
    fs::write(
        &recommendation,
        "## Open items\n- [written-approval] budget\n",
    )
    .unwrap();
    let e = session.err("report", approve(AI));
    assert!(
        message(&e).contains("cannot be approved by the AI Orchestrator"),
        "{e}"
    );

    // The owner pins the gate to human between two calls of the same session.
    fs::write(&recommendation, "## Open items\n- [follow-up] telemetry\n").unwrap();
    cli_ok(
        &project.0,
        &["config", "gate-mode", "analysis-baseline", "human"],
    );
    let e = session.err("report", approve(AI));
    assert!(message(&e).contains("pinned to human mode"), "{e}");
    assert_eq!(count(&project.0, "gate.decided"), 0);

    // Un-pinning lets the same call through: the policy is read on every call.
    cli_ok(
        &project.0,
        &["config", "gate-mode", "analysis-baseline", "default"],
    );
    let done = session.ok("report", approve(AI));
    assert_eq!(done["recorded"], true, "{done}");
    assert_eq!(done["approvedBy"], AI);
    assert_eq!(count(&project.0, "gate.decided"), 1);
    let recorded = &state(&project.0)["gates"]["analysis-baseline"];
    assert_eq!(recorded["mode"], "ai");
    assert_eq!(
        recorded["openItems"],
        json!({"writtenApproval": 0, "followUp": 1})
    );
    assert_eq!(session.shutdown().0, Some(0));
}

/// Launch on an unsafe layout: exit 1 before reading stdin, no frame, one stderr line.
fn assert_refused_at_launch(project: &Path, needle: &str) {
    let out = run_mcp(
        Some(project),
        lines(&[call_frame(
            1,
            "report",
            json!({"result": "blocked", "detail": "x"}),
        )]),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no frame on a startup failure");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("dtech-aidlc-mcp: "), "{stderr}");
    assert!(stderr.contains(needle), "{stderr}");
}

#[test]
fn ts033_unsafe_layout_at_launch_is_refused_and_writes_nothing() {
    // A regular file where the intents directory belongs is unsafe on every platform.
    let project = started("fu-unsafe-launch");
    let intents = project.0.join(".aidlc/intents");
    fs::rename(&intents, project.0.join("intents-aside")).unwrap();
    fs::write(&intents, "not a directory").unwrap();
    let before = snapshot(&project.0);
    assert_refused_at_launch(&project.0, "Refusing unsafe .aidlc/intents path");
    assert_eq!(snapshot(&project.0), before);
}

#[cfg(unix)]
#[test]
fn ts033_symlinked_state_directory_at_launch_is_refused() {
    let project = started("fu-unsafe-symlink");
    let aside = project.0.join("aidlc-real");
    fs::rename(project.0.join(".aidlc"), &aside).unwrap();
    std::os::unix::fs::symlink(&aside, project.0.join(".aidlc")).unwrap();
    let before = snapshot(&aside);
    assert_refused_at_launch(&project.0, "symlink .aidlc directory");
    assert_eq!(snapshot(&aside), before);
}

#[test]
fn ts033_unsafe_layout_mid_session_is_refused_and_the_session_recovers() {
    let project = started("fu-unsafe-mid");
    let mut session = Session::start(&project.0);
    assert_eq!(session.ok("status", json!({}))["intentId"], ID);
    let reports_before = reports(&project.0);

    let intents = project.0.join(".aidlc/intents");
    let aside = project.0.join("intents-aside");
    fs::rename(&intents, &aside).unwrap();
    fs::write(&intents, "not a directory").unwrap();
    for (name, args) in [
        ("status", json!({})),
        ("report", json!({"result": "blocked", "detail": "x"})),
    ] {
        let e = session.err(name, args);
        assert!(
            message(&e).contains("Refusing unsafe .aidlc/intents path"),
            "{name} {e}"
        );
    }
    assert_eq!(fs::read(&intents).unwrap(), b"not a directory");

    // Restoring the layout is enough: the same process answers again, nothing was lost.
    fs::remove_file(&intents).unwrap();
    fs::rename(&aside, &intents).unwrap();
    assert_eq!(session.ok("status", json!({}))["intentId"], ID);
    assert_eq!(reports(&project.0), reports_before);
    session.ok("report", json!({"result": "blocked", "detail": "after"}));
    assert_eq!(reports(&project.0), reports_before + 1);
    assert_eq!(session.shutdown().0, Some(0));
}

#[test]
fn ts033_cli_and_mcp_interleave_sequentially_on_one_state() {
    let project = started("fu-interleave");
    let mut session = Session::start(&project.0);
    let mut expected = 0;
    for round in 0..3 {
        cli_ok(
            &project.0,
            &[
                "orchestrate",
                "report",
                "--result",
                "blocked",
                "--detail",
                &format!("cli{round}"),
            ],
        );
        expected += 1;
        assert_eq!(reports(&project.0), expected);
        // The live server selects the stage the CLI left and writes on top of the CLI's.
        let n = session.ok("next", json!({}));
        assert_eq!(
            n["result"]["stage"]["rework"]["detail"],
            format!("cli{round}")
        );
        session.ok(
            "report",
            json!({"result": "blocked", "detail": format!("mcp{round}")}),
        );
        expected += 1;
        // The CLI sees the MCP write as the rework detail of the next selection.
        let next = cli_ok(&project.0, &["orchestrate", "next"]);
        assert_eq!(next["stage"]["rework"]["detail"], format!("mcp{round}"));
    }
    assert_eq!(reports(&project.0), expected);
    assert_eq!(count(&project.0, "stage.reported"), expected);
    assert_eq!(session.shutdown().0, Some(0));
}

#[test]
fn ts033_cli_and_mcp_race_never_loses_an_update() {
    for round in 0..3 {
        let project = started("fu-race");
        let path = project.0.join(format!(".aidlc/intents/{ID}/state.json"));
        let mut padded = state(&project.0);
        padded["padding"] = Value::String("x".repeat(8 << 20));
        fs::write(&path, serde_json::to_vec(&padded).unwrap()).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let cli_thread = {
            let (barrier, root) = (barrier.clone(), project.0.clone());
            std::thread::spawn(move || {
                barrier.wait();
                cli(
                    &root,
                    &[
                        "orchestrate",
                        "report",
                        "--result",
                        "blocked",
                        "--detail",
                        "cli",
                    ],
                )
            })
        };
        let mcp_thread = {
            let (barrier, root) = (barrier.clone(), project.0.clone());
            std::thread::spawn(move || {
                barrier.wait();
                tool(
                    &root,
                    "report",
                    json!({"result": "blocked", "detail": "mcp"}),
                )
            })
        };
        let cli_out = cli_thread.join().unwrap();
        let (mcp_value, mcp_error) = mcp_thread.join().unwrap();

        let recorded = usize::from(cli_out.status.success()) + usize::from(!mcp_error);
        assert!(recorded >= 1, "round {round}: someone must win");
        // A losing CLI run is a refused write (exit 1), never a crash.
        assert!(
            cli_out.status.success() || cli_out.status.code() == Some(1),
            "round {round}: {}",
            String::from_utf8_lossy(&cli_out.stderr)
        );
        if mcp_error {
            let code = mcp_value["error"]["code"].as_str().unwrap();
            assert!(
                matches!(code, "state_conflict" | "operation_failed"),
                "{mcp_value}"
            );
        }
        assert_eq!(
            reports(&project.0),
            recorded,
            "round {round}: state vs successful replies"
        );
        assert_eq!(
            count(&project.0, "stage.reported"),
            recorded,
            "round {round}: audit rows"
        );
    }
}

#[test]
fn ts033_restart_after_a_killed_session_resumes_from_the_recorded_state() {
    let project = started("fu-restart");
    let mut first = Session::start(&project.0);
    first.ok(
        "report",
        json!({"result": "blocked", "detail": "before kill"}),
    );
    let snap = snapshot(&project.0);
    first.kill();
    assert_eq!(
        snapshot(&project.0),
        snap,
        "a killed server leaves no partial write"
    );

    let mut second = Session::start(&project.0);
    assert_eq!(second.ok("status", json!({}))["intentId"], ID);
    let next = second.ok("next", json!({}));
    assert_eq!(next["result"]["stage"]["rework"]["detail"], "before kill");
    second.ok(
        "report",
        json!({"result": "blocked", "detail": "after restart"}),
    );
    assert_eq!(reports(&project.0), 2);
    assert_eq!(count(&project.0, "stage.reported"), 2);
    assert_eq!(second.shutdown().0, Some(0));
}

#[test]
fn ts033_shutdown_mid_session_is_clean_and_answers_what_was_sent() {
    let project = started("fu-shutdown");
    // EOF straight after the last reply: exit 0, nothing more on stdout.
    let mut session = Session::start(&project.0);
    session.ok("report", json!({"result": "blocked", "detail": "one"}));
    let (code, rest) = session.shutdown();
    assert_eq!(code, Some(0));
    assert!(rest.is_empty(), "nothing after the last reply: {rest}");

    // EOF in the middle of a request line: the complete request is answered, the
    // unterminated fragment gets one Parse error reply with a null id, it writes no
    // state, and the exit is 0.
    let mut input = lines(&[call_frame(1, "status", json!({}))]);
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","par"#);
    let before = snapshot(&project.0);
    let out = run_mcp(Some(&project.0), input);
    let replies = frames_of(&out);
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[0]["result"]["isError"], false, "{replies:?}");
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(replies[1]["id"], Value::Null);
    assert_eq!(replies[1]["error"]["code"], -32700);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(snapshot(&project.0), before);

    // State is intact and a later session reports normally.
    assert_eq!(reports(&project.0), 1);
    tool_ok(&project.0, "next", json!({}));
    tool_ok(
        &project.0,
        "report",
        json!({"result": "blocked", "detail": "two"}),
    );
    assert_eq!(reports(&project.0), 2);
}
