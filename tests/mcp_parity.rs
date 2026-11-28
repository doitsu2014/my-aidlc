//! CLI versus MCP parity on identical projects (TS-006) and concurrent writers (TS-007).
//! Both binaries are driven as child processes. The deliberate differences are only: no
//! prerequisite command runs over MCP, a gate decision needs an explicit label, paths are
//! redacted, and no intent can be created over MCP.

#[path = "fixtures/mcp/common.rs"]
mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};

const INTENT: &str = "Add login page";

fn fixture(name: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/unit_scheduling")
            .join(name),
    )
    .unwrap()
}

/// The sanitized 1.0.0 state at the repeat span (from the unit-scheduling fixtures), so a
/// work-unit queue exists. Mirrors `seed_legacy` in `rust_unit_scheduling.rs`.
fn seed_legacy(project: &Path) {
    let dir = project.join(".aidlc/intents/unit-replay");
    let design = dir.join("develop/implementation-design");
    fs::create_dir_all(&design).unwrap();
    fs::write(dir.join("state.json"), fixture("state-1.0.0.json")).unwrap();
    fs::write(dir.join("intent.md"), "# unit-replay\n\nReplay\n").unwrap();
    fs::write(
        design.join("work-units.json"),
        fixture("work-units.real.json"),
    )
    .unwrap();
    fs::write(design.join("ids.json"), fixture("ids.json")).unwrap();
    fs::write(design.join("work-units.md"), fixture("work-units.md")).unwrap();
    fs::write(
        project.join(".aidlc/state.json"),
        "{\n  \"version\": 2,\n  \"activeIntentId\": \"unit-replay\",\n  \"updatedAt\": \"2026-11-26T02:17:34.449Z\"\n}\n",
    )
    .unwrap();
    fs::write(
        project.join(".aidlc/audit.jsonl"),
        "{\"at\":\"2026-11-21T12:42:56.822Z\",\"event\":\"intent.created\",\"intentId\":\"unit-replay\"}\n",
    )
    .unwrap();
}

fn drop_key(mut v: Value, key: &str) -> Value {
    v.as_object_mut().unwrap().remove(key);
    v
}

/// Rows of the audit log with timestamps normalised.
fn audit_rows(project: &Path) -> Vec<Value> {
    fs::read_to_string(project.join(".aidlc/audit.jsonl"))
        .unwrap()
        .lines()
        .map(|l| normalize(&serde_json::from_str(l).unwrap(), project))
        .collect()
}

fn state_files(project: &Path) -> Vec<(String, Value)> {
    snapshot(project)
        .into_iter()
        .filter(|(k, _)| k.ends_with("state.json"))
        .map(|(k, bytes)| {
            (
                k,
                normalize(&serde_json::from_slice(&bytes).unwrap(), project),
            )
        })
        .collect()
}

fn cli_text(project: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = cli(project, args);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).trim().to_owned(),
    )
}

#[test]
fn ts006_status_matches_the_cli() {
    let (a, b) = (Tmp::project("par-status-a"), Tmp::project("par-status-b"));
    for p in [&a, &b] {
        start_intent(&p.0, INTENT);
    }
    let before = snapshot(&b.0);
    let from_cli = normalize(&cli_ok(&a.0, &["status"]), &a.0);
    let from_mcp = normalize(&tool_ok(&b.0, "status", json!({})), &b.0);
    assert_eq!(from_cli, from_mcp);
    // The intent selector works the same, and an unknown one fails with the CLI's text.
    let by_id = tool_ok(&b.0, "status", json!({"intent_id": "add-login-page"}));
    assert_eq!(normalize(&by_id, &b.0), from_mcp);
    let (_, stderr) = cli_text(&a.0, &["status", "--intent", "nope"]);
    let error = tool_err(&b.0, "status", json!({"intent_id": "nope"}));
    assert_eq!(error["code"], "operation_failed");
    assert_eq!(
        format!("dtech-aidlc: {}", error["message"].as_str().unwrap()),
        stderr
    );
    assert_eq!(snapshot(&b.0), before, "status is read-only");
}

#[test]
fn ts006_queue_is_a_read_only_preview_and_matches_the_cli() {
    let (a, b) = (Tmp::project("par-queue-a"), Tmp::project("par-queue-b"));
    seed_legacy(&a.0);
    seed_legacy(&b.0);
    let before_a = snapshot(&a.0);
    let before_b = snapshot(&b.0);
    let from_cli = cli_ok(&a.0, &["orchestrate", "queue"]);
    let from_mcp = tool_ok(&b.0, "queue", json!({}));
    assert_eq!(normalize(&from_cli, &a.0), normalize(&from_mcp, &b.0));
    // The legacy ledger is only previewed (AC-007): state.json, the pointer and
    // audit.jsonl bytes, and every other file, are unchanged after both calls.
    assert_eq!(from_mcp["migration"], "pending");
    assert_eq!(from_mcp["intentId"], "unit-replay");
    assert_eq!(snapshot(&a.0), before_a);
    assert_eq!(snapshot(&b.0), before_b);
    for name in [".aidlc/state.json", ".aidlc/audit.jsonl"] {
        assert_eq!(
            fs::read(b.0.join(name)).unwrap(),
            before_b[name],
            "{name} bytes"
        );
    }
    // Repeating it is idempotent.
    assert_eq!(tool_ok(&b.0, "queue", json!({})), from_mcp);
    assert_eq!(snapshot(&b.0), before_b);
}

#[test]
fn ts006_queue_without_work_units_fails_like_the_cli_and_changes_nothing() {
    let (a, b) = (Tmp::project("par-noq-a"), Tmp::project("par-noq-b"));
    for p in [&a, &b] {
        start_intent(&p.0, INTENT);
    }
    let before = snapshot(&b.0);
    let (code, stderr) = cli_text(&a.0, &["orchestrate", "queue"]);
    assert_eq!(code, Some(1));
    let error = tool_err(&b.0, "queue", json!({}));
    assert_eq!(error["code"], "operation_failed");
    assert_eq!(error["retryable"], false);
    assert_eq!(
        format!("dtech-aidlc: {}", error["message"].as_str().unwrap()),
        stderr
    );
    assert_eq!(snapshot(&b.0), before);
}

/// The same scripted lifecycle through the CLI in one project and through MCP in another
/// (the intent itself is created by the CLI in both): every result is equal after path
/// normalisation, and so are the final state files and audit rows.
#[test]
fn ts006_lifecycle_through_mcp_matches_the_cli_state_and_audit() {
    let (a, b) = (Tmp::project("par-life-a"), Tmp::project("par-life-b"));
    for p in [&a, &b] {
        start_intent(&p.0, INTENT);
    }
    let cli_next = || normalize(&cli_ok(&a.0, &["orchestrate", "next"]), &a.0);
    let mcp_next = || {
        let v = tool_ok(&b.0, "next", json!({}));
        assert_eq!(v["outcome"], "directive");
        normalize(&v["result"], &b.0)
    };
    let cli_report = |args: &[&str]| {
        let mut all = vec!["orchestrate", "report"];
        all.extend(args);
        normalize(&cli_ok(&a.0, &all), &a.0)
    };
    let mcp_report = |args: Value| {
        let v = tool_ok(&b.0, "report", args);
        assert_eq!(v["pendingPrerequisites"], json!([]), "{v}");
        normalize(&drop_key(v, "pendingPrerequisites"), &b.0)
    };

    assert_eq!(cli_next(), mcp_next());

    // Completing without artifacts is refused with the same text and changes nothing.
    let (_, stderr) = cli_text(&a.0, &["orchestrate", "report", "--result", "complete"]);
    let before = snapshot(&b.0);
    let error = tool_err(&b.0, "report", json!({"result": "complete"}));
    assert_eq!(error["code"], "operation_failed");
    assert_eq!(
        format!("dtech-aidlc: {}", error["message"].as_str().unwrap()),
        stderr
    );
    assert_eq!(snapshot(&b.0), before);

    // Blocked, then the next selection is a rework of the same stage.
    assert_eq!(
        cli_report(&["--result", "blocked", "--detail", "need input"]),
        mcp_report(json!({"result": "blocked", "detail": "need input"}))
    );
    let rework = cli_next();
    assert_eq!(rework, mcp_next());
    assert_eq!(rework["stage"]["rework"]["detail"], "need input");
    assert_eq!(rework["stage"]["rework"]["attempt"], 2);

    // Complete the first stage, then reach the Analyze gate.
    for p in [&a, &b] {
        write_artifacts(&p.0, &rework);
    }
    assert_eq!(
        cli_report(&["--result", "complete"]),
        mcp_report(json!({"result": "complete"}))
    );
    let gate = cli_next();
    assert_eq!(gate, mcp_next());
    assert_eq!(gate["stage"]["id"], "context-and-risk");
    assert_eq!(gate["stage"]["phaseGate"], true);
    for p in [&a, &b] {
        write_artifacts(&p.0, &gate);
    }

    // A gate decision without a decision is refused by both.
    let (_, stderr) = cli_text(&a.0, &["orchestrate", "report", "--result", "complete"]);
    let error = tool_err(&b.0, "report", json!({"result": "complete"}));
    assert_eq!(
        format!("dtech-aidlc: {}", error["message"].as_str().unwrap()),
        stderr
    );
    assert!(stderr.contains("requires explicit --approval"), "{stderr}");

    // Over MCP the decision also needs a label; the refusal changes nothing (AC-010).
    let before = snapshot(&b.0);
    for approved_by in [None, Some(""), Some("   ")] {
        let mut args = json!({"result": "complete", "approval": "approved"});
        if let Some(label) = approved_by {
            args["approved_by"] = json!(label);
        }
        let code = if approved_by == Some("") {
            // An empty label never reaches the facade: the schema requires 1 to 200.
            "invalid_arguments"
        } else {
            "approver_required"
        };
        let error = tool_err(&b.0, "report", args);
        assert_eq!(error["code"], code, "{approved_by:?}");
        assert_eq!(error["retryable"], false);
    }
    assert_eq!(snapshot(&b.0), before);
    assert!(!audit_events(&b.0).contains(&"gate.decided".to_owned()));

    // With a label the same gate policy runs and both record the decision identically.
    let cli_gate = cli_report(&[
        "--result",
        "complete",
        "--approval",
        "approved",
        "--approved-by",
        "Pat Owner",
    ]);
    let mcp_gate = mcp_report(
        json!({"result": "complete", "approval": "approved", "approved_by": "Pat Owner"}),
    );
    assert_eq!(cli_gate, mcp_gate);
    assert_eq!(mcp_gate["gate"], "analysis-baseline");
    assert_eq!(mcp_gate["approvedBy"], "Pat Owner");
    assert_eq!(cli_next(), mcp_next());

    assert_eq!(audit_events(&a.0), audit_events(&b.0));
    assert_eq!(audit_rows(&a.0), audit_rows(&b.0));
    assert_eq!(state_files(&a.0), state_files(&b.0));
    let state = fs::read(b.0.join(".aidlc/intents/add-login-page/state.json")).unwrap();
    let state: Value = serde_json::from_slice(&state).unwrap();
    assert_eq!(state["gates"]["analysis-baseline"]["decision"], "approved");
    assert_eq!(state["gates"]["analysis-baseline"]["bySource"], "flag");
}

#[test]
fn ts006_no_prerequisite_command_runs_over_mcp_but_the_cli_runs_it() {
    let (a, b) = (Tmp::project("par-pre-a"), Tmp::project("par-pre-b"));
    for p in [&a, &b] {
        start_intent(&p.0, INTENT);
    }
    let marker_a = a.0.join("cli-marker");
    let marker_b = b.0.join("mcp-marker");
    for (p, marker) in [(&a, &marker_a), (&b, &marker_b)] {
        let check = marker_check(marker);
        write_config(&p.0, |c| {
            c["prerequisites"] = json!({"toolchain": {
                "check": check, "hint": "Install it", "stages": ["problem-framing"]}});
        });
    }

    // Over MCP: resuming the stage is held, nothing is spawned, no check row is audited.
    let held = tool_ok(&b.0, "next", json!({}));
    assert_eq!(held["outcome"], "blocked");
    let needs = &held["result"]["needs"];
    assert_eq!(
        needs,
        &json!([{"name": "toolchain", "ok": false, "hint": "Install it",
                 "exitCode": null, "executed": false}])
    );
    let text = held.to_string();
    assert!(text.contains("not executed"), "{text}");
    assert!(
        !text.contains(&marker_b.to_string_lossy().to_string()) && !text.contains("touch"),
        "the check command line must never be returned: {text}"
    );
    assert!(!marker_b.exists());
    assert!(!audit_events(&b.0).contains(&"prerequisite.checked".to_owned()));

    // pendingPrerequisites after a report (AC-009): a preview of the same rows.
    let report = tool_ok(
        &b.0,
        "report",
        json!({"result": "blocked", "detail": "toolchain missing", "needs": ["toolchain"]}),
    );
    assert_eq!(report["recorded"], true);
    assert_eq!(
        report["pendingPrerequisites"],
        json!([{"name": "toolchain", "ok": false, "hint": "Install it",
                "exitCode": null, "executed": false}])
    );
    let again = tool_ok(&b.0, "next", json!({}));
    assert_eq!(again["outcome"], "blocked");
    assert!(!marker_b.exists());
    assert!(!audit_events(&b.0).contains(&"prerequisite.checked".to_owned()));

    // The CLI on the same configuration does run the command (and exits 3 when it fails).
    let out = cli(&a.0, &["orchestrate", "next"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(marker_a.exists(), "the CLI runs prerequisite checks");
}

#[test]
fn ts006_an_intent_cannot_be_created_over_mcp() {
    let project = Tmp::project("par-nointent");
    let before = snapshot(&project.0);
    let error = tool_err(&project.0, "next", json!({"intent": INTENT}));
    assert_eq!(error["code"], "invalid_arguments");
    // With no intent and valid arguments, `next` fails and creates nothing.
    let error = tool_err(&project.0, "next", json!({}));
    assert_eq!(error["code"], "operation_failed");
    assert!(error["message"]
        .as_str()
        .unwrap()
        .contains("No active intent"));
    assert_eq!(snapshot(&project.0), before);
    assert!(
        !project.0.join(".aidlc/intents").exists()
            || before.keys().all(|k| !k.contains("intents/"))
    );
    assert!(!project.0.join(".aidlc/state.json").exists());
}

#[test]
fn ts006_next_options_cannot_reach_the_workflow() {
    let project = Tmp::project("par-opts");
    start_intent(&project.0, INTENT);
    let before = snapshot(&project.0);
    for args in [
        json!({"scope": "team"}),
        json!({"depth": "lite"}),
        json!({"mode": "ai"}),
        json!({"approved_by": "x"}),
        json!({"force_resume": true, "detail": "x"}),
    ] {
        assert_eq!(
            tool_err(&project.0, "next", args)["code"],
            "invalid_arguments"
        );
    }
    assert_eq!(snapshot(&project.0), before);
}

// ---------------------------------------------------------------------------
// TS-007: two writers on one project (AC-012)
// ---------------------------------------------------------------------------

/// One `report blocked` through its own server process, released by `barrier` so the two
/// writers start together.
fn racer(
    project: PathBuf,
    barrier: Arc<Barrier>,
    detail: String,
) -> std::thread::JoinHandle<Value> {
    let input = lines(&[modern(
        1,
        "tools/call",
        json!({"name": "report", "arguments": {"result": "blocked", "detail": detail}}),
    )]);
    std::thread::spawn(move || {
        let mut child = Command::new(MCP_BIN)
            .arg("--project")
            .arg(&project)
            .env_remove("DTECH_AIDLC_HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        barrier.wait();
        stdin.write_all(&input).unwrap();
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        frames_of(&out).remove(0)["result"].clone()
    })
}

/// Two servers report on the same stage at once. Whatever the interleaving, a report is
/// either recorded or refused with a retryable `state_conflict` that wrote nothing: the
/// recorded reports, the audit rows and the successful replies always agree. A large
/// state file widens the load-to-write window so the writers genuinely overlap.
#[test]
fn ts007_concurrent_reports_never_lose_an_update() {
    let mut conflicts = 0;
    for round in 0..4 {
        let project = Tmp::project("par-race");
        start_intent(&project.0, INTENT);
        let state_path = project.0.join(".aidlc/intents/add-login-page/state.json");
        let mut state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        state["padding"] = Value::String("x".repeat(8 << 20));
        fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let racers: Vec<_> = ["a", "b"]
            .iter()
            .map(|who| racer(project.0.clone(), barrier.clone(), format!("{who}{round}")))
            .collect();
        let results: Vec<Value> = racers.into_iter().map(|r| r.join().unwrap()).collect();

        let mut recorded = 0;
        for result in &results {
            let structured = &result["structuredContent"];
            if result["isError"] == false {
                assert_eq!(structured["recorded"], true, "{result}");
                recorded += 1;
            } else {
                let error = &structured["error"];
                // The loser is a conflict, or (when it ran strictly after the winner) a
                // report with no stage left to report on. Never a corrupted state.
                match error["code"].as_str().unwrap() {
                    "state_conflict" => {
                        assert_eq!(error["retryable"], true, "{result}");
                        conflicts += 1;
                    }
                    "operation_failed" => assert!(
                        error["message"]
                            .as_str()
                            .unwrap()
                            .contains("No current stage"),
                        "{result}"
                    ),
                    other => panic!("unexpected error code {other}: {result}"),
                }
            }
        }
        // The state file is still valid JSON and holds exactly one report per recorded
        // reply; every recorded reply has exactly one audit row (no lost update).
        let state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        let reports = state["stages"]["problem-framing"]["reports"]
            .as_array()
            .map_or(0, Vec::len);
        let rows = audit_events(&project.0)
            .iter()
            .filter(|e| *e == "stage.reported")
            .count();
        assert!(recorded >= 1, "round {round}: {results:?}");
        assert_eq!(
            reports, recorded,
            "round {round}: {recorded} replies recorded a report but state holds {reports} ({results:?})"
        );
        assert_eq!(rows, recorded, "round {round}: audit rows vs replies");
    }
    eprintln!("ts007: state_conflict observed {conflicts} time(s) in 4 rounds");
    assert!(conflicts >= 1, "no round produced a state_conflict");
}
