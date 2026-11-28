//! TS-027: the MCP server writes local telemetry (FN-029, FN-031) without changing a byte of
//! what it says on stdout. Every stored line must satisfy the WU-013 `parse_line` contract.

#[path = "fixtures/mcp/common.rs"]
mod common;

use common::*;
use dtech_aidlc::telemetry::{parse_line, Event, EventKind, Outcome};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn store_files(project: &Path) -> Vec<std::path::PathBuf> {
    let dir = project.join(".aidlc/telemetry");
    let mut files: Vec<_> = match fs::read_dir(&dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

/// Every stored line, each required to parse under the v1 contract.
fn events(project: &Path) -> Vec<Event> {
    let mut out = Vec::new();
    for file in store_files(project) {
        for line in fs::read_to_string(file).unwrap().lines() {
            out.push(parse_line(line).unwrap_or_else(|| panic!("line violates IF-012: {line}")));
        }
    }
    out
}

/// One traced (default, telemetry on) session.
fn traced(project: &Path, frames: &[Value]) -> Vec<Value> {
    frames_of(&run_mcp_args(Some(project), lines(frames), &[], &[]))
}

fn call(id: u64, name: &str, args: Value) -> Value {
    modern(id, "tools/call", json!({"name": name, "arguments": args}))
}

fn of_kind(events: &[Event], kind: EventKind) -> Vec<&Event> {
    events.iter().filter(|e| e.kind == kind).collect()
}

#[test]
fn ts027_telemetry_off_writes_nothing() {
    let project = Tmp::project("tel-off");
    let before = snapshot(&project.0);
    let out = run_mcp_args(
        Some(&project.0),
        lines(&[call(1, "status", json!({})), call(2, "nope", json!({}))]),
        &[],
        &["--telemetry", "off"],
    );
    frames_of(&out);
    assert!(!project.0.join(".aidlc/telemetry").exists());
    assert_eq!(snapshot(&project.0), before);
    // The default helper passes --telemetry off too.
    tool_ok(&project.0, "status", json!({}));
    assert!(!project.0.join(".aidlc/telemetry").exists());
}

#[test]
fn ts027_default_is_on_and_flags_are_order_independent() {
    let project = Tmp::project("tel-default");
    traced(&project.0, &[call(1, "status", json!({}))]);
    assert!(!events(&project.0).is_empty(), "default must be on");

    let second = Tmp::project("tel-order");
    let out = std::process::Command::new(MCP_BIN)
        .args(["--telemetry", "on", "--project"])
        .arg(&second.0)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .take()
                .unwrap()
                .write_all(&lines(&[call(1, "status", json!({}))]))?;
            c.wait_with_output()
        })
        .unwrap();
    frames_of(&out);
    assert!(!events(&second.0).is_empty());
}

#[test]
fn ts027_stdout_is_byte_identical_with_telemetry_on_and_off() {
    let on = Tmp::project("tel-same-on");
    let off = Tmp::project("tel-same-off");
    let big = format!("{}\n", "x".repeat(1 << 20 | 7));
    let mut input = lines(&[
        initialize(1, "2025-11-25"),
        call(2, "status", json!({})),
        call(3, "queue", json!({})),
        call(4, "status", json!({"intent_id": "Bad_Id"})),
        call(5, "nope", json!({})),
        call(6, "report", json!({"approval": "approved"})),
        call(7, "report", json!({"result": "complete"})),
        modern(8, "bogus/method", json!({})),
    ]);
    input.extend_from_slice(b"{not json\n");
    input.extend_from_slice(big.as_bytes());
    input.extend_from_slice(&lines(&[call(9, "next", json!({}))]));
    let a = run_mcp_args(Some(&on.0), input.clone(), &[], &["--telemetry", "on"]);
    let b = run_mcp_args(Some(&off.0), input, &[], &["--telemetry", "off"]);
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(a.stdout, b.stdout);
    assert!(!a.stdout.is_empty());
    assert!(
        a.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&a.stderr)
    );
    assert!(!events(&on.0).is_empty());
    assert!(!off.0.join(".aidlc/telemetry").exists());
}

#[test]
fn ts027_event_set_trace_shape_and_session_ids() {
    let project = Tmp::project("tel-shape");
    traced(
        &project.0,
        &[
            call(1, "status", json!({})),
            call(2, "queue", json!({})),
            call(3, "status", json!({"intent_id": "Bad_Id"})),
            call(4, "nope", json!({})),
        ],
    );
    let evs = events(&project.0);
    let session = evs[0].session_id.clone();
    assert_eq!(session.len(), 32);
    for e in &evs {
        assert_eq!(e.session_id, session);
        assert_eq!(e.process_id, evs[0].process_id);
        for id in [&e.event_id, &e.trace_id, &e.span_id] {
            assert!(id.starts_with(&format!("{session}-")), "{id}");
        }
        assert_eq!(
            e.outcome.is_none(),
            matches!(
                e.kind,
                EventKind::RequestStarted | EventKind::OperationStarted
            )
        );
    }
    let mut ids: Vec<_> = evs.iter().map(|e| e.event_id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), evs.len(), "eventIds are unique");
    let file = store_files(&project.0)[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(file.contains(&session[..8]), "{file}");

    // status ok: tool pair (root) and an operation pair under the tool span.
    let started: Vec<_> = of_kind(&evs, EventKind::RequestStarted);
    assert_eq!(
        started.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["status", "queue", "status", "unknown"]
    );
    let first = started[0];
    assert!(first.parent_span_id.is_none());
    let done: Vec<_> = of_kind(&evs, EventKind::ToolCompleted);
    assert_eq!(done.len(), 4);
    assert_eq!(done[0].span_id, first.span_id);
    assert_eq!(done[0].trace_id, first.trace_id);
    assert!(done[0].duration_ms.is_some());
    assert_eq!(done[0].outcome, Some(Outcome::Ok));
    let ops = of_kind(&evs, EventKind::OperationStarted);
    let op_done = of_kind(&evs, EventKind::OperationCompleted);
    assert_eq!(ops.len(), 2, "invalid and unknown calls run no operation");
    assert_eq!(op_done.len(), 2);
    assert_eq!(ops[0].span_id, op_done[0].span_id);
    assert_eq!(
        ops[0].parent_span_id.as_deref(),
        Some(first.span_id.as_str())
    );
    assert!(ops[0].duration_ms.is_none() && op_done[0].duration_ms.is_some());
    // invalid_arguments and unknown_tool with the closed codes; unknown name is not stored.
    assert_eq!(done[2].error_code.as_deref(), Some("invalid_arguments"));
    assert_eq!(done[3].name, "unknown");
    assert_eq!(done[3].error_code.as_deref(), Some("unknown_tool"));
    // The unknown tool reply is an invalid_params protocol error: one more one-event trace.
    let proto = of_kind(&evs, EventKind::ProtocolError);
    assert_eq!(proto.len(), 1);
    assert_eq!(proto[0].name, "protocol");
    assert_eq!(proto[0].error_code.as_deref(), Some("invalid_params"));
    assert!(proto[0].parent_span_id.is_none());
    assert!(proto[0].duration_ms.is_none());
}

#[test]
fn ts027_workflow_events_follow_a_durable_success() {
    let project = Tmp::project("tel-flow");
    start_intent(&project.0, "Add login page");
    // stage_selected: id keys live at result.intentId and result.stage.id.
    let replies = traced(&project.0, &[call(1, "next", json!({}))]);
    let directive = replies[0]["result"]["structuredContent"]["result"].clone();
    let evs = events(&project.0);
    let selected = of_kind(&evs, EventKind::StageSelected);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "next");
    assert_eq!(selected[0].outcome, Some(Outcome::Ok));
    assert_eq!(
        selected[0].intent_id.as_deref(),
        directive["intentId"].as_str()
    );
    assert_eq!(
        selected[0].stage_id.as_deref(),
        directive["stage"]["id"].as_str()
    );
    assert!(selected[0].intent_id.is_some() && selected[0].stage_id.is_some());
    let op = of_kind(&evs, EventKind::OperationStarted)[0];
    assert_eq!(
        selected[0].parent_span_id.as_deref(),
        Some(op.span_id.as_str())
    );
    assert_ne!(selected[0].span_id, op.span_id);

    // Move to the gate over untraced sessions, then decide it in a traced one.
    write_artifacts(&project.0, &directive);
    tool_ok(&project.0, "report", json!({"result": "complete"}));
    let gate = tool_ok(&project.0, "next", json!({}));
    assert_eq!(gate["result"]["stage"]["phaseGate"], true);
    write_artifacts(&project.0, &gate["result"]);
    let seen: Vec<String> = events(&project.0)
        .into_iter()
        .map(|e| e.session_id)
        .collect();
    traced(
        &project.0,
        &[call(
            1,
            "report",
            json!({"result": "complete", "approval": "approved", "approved_by": "owner"}),
        )],
    );
    let evs = events(&project.0);
    let fresh: Vec<Event> = evs
        .into_iter()
        .filter(|e| !seen.contains(&e.session_id))
        .collect();
    let fresh = &fresh[..];
    let reported = of_kind(fresh, EventKind::StageReported);
    let decided = of_kind(fresh, EventKind::GateDecided);
    assert_eq!((reported.len(), decided.len()), (1, 1));
    assert_eq!(decided[0].name, "report");
    // The report's stage argument is omitted: ids come from the result.
    for e in [reported[0], decided[0]] {
        assert_eq!(
            e.stage_id.as_deref(),
            gate["result"]["stage"]["id"].as_str()
        );
        assert!(e.intent_id.is_some());
    }
    assert_eq!(decided[0].outcome, Some(Outcome::Ok));
    assert!(decided[0].error_code.is_none());
    let mut line = String::new();
    for f in store_files(&project.0) {
        line.push_str(&fs::read_to_string(f).unwrap());
    }
    assert!(!line.contains("approved"), "no decision value is stored");
    assert!(!line.contains("owner"));
}

#[test]
fn ts027_a_failed_operation_emits_no_workflow_event() {
    let project = Tmp::project("tel-fail");
    // No intent: report fails inside the operation.
    let replies = traced(
        &project.0,
        &[call(1, "report", json!({"result": "complete"}))],
    );
    assert_eq!(replies[0]["result"]["isError"], true);
    let evs = events(&project.0);
    for kind in [
        EventKind::StageSelected,
        EventKind::StageReported,
        EventKind::GateDecided,
    ] {
        assert!(of_kind(&evs, kind).is_empty(), "{kind:?}");
    }
    let op = of_kind(&evs, EventKind::OperationCompleted);
    assert_eq!(op.len(), 1);
    assert_eq!(op[0].outcome, Some(Outcome::Error));
    let tool = of_kind(&evs, EventKind::ToolCompleted);
    assert_eq!(tool[0].error_code, op[0].error_code);
    assert!(op[0].error_code.is_some());
}

#[test]
fn ts027_sentinel_secrets_never_reach_the_store() {
    let project = Tmp::project("tel-secrets");
    start_intent(&project.0, "Add login page");
    let secret = "SENTINEL-s3cr3t-9f2a";
    let root = project.0.to_string_lossy().into_owned();
    traced(
        &project.0,
        &[
            call(1, secret, json!({"detail": secret})),
            call(2, "status", json!({"intent_id": secret})),
            call(3, "status", json!({secret: secret})),
            call(
                4,
                "report",
                json!({"result": "blocked", "detail": format!("{secret} {root}"),
                       "approved_by": secret, "approval_note": secret, "approval": "approved"}),
            ),
            call(
                5,
                "report",
                json!({"result": "blocked", "detail": format!("{secret} {root}"),
                       "approved_by": secret, "approval_note": secret}),
            ),
            modern(6, secret, json!({"detail": secret})),
        ],
    );
    let mut raw = String::new();
    for file in store_files(&project.0) {
        raw.push_str(&fs::read_to_string(file).unwrap());
    }
    assert!(!raw.is_empty());
    assert!(!raw.contains(secret), "{raw}");
    assert!(!raw.contains(&root), "{raw}");
    assert!(!raw.contains("detail"));
    let unique = Path::new(&root).file_name().unwrap().to_string_lossy();
    assert!(!raw.contains(unique.as_ref()), "{raw}");
    assert!(!events(&project.0).is_empty());
}

#[test]
fn ts027_protocol_errors_after_handshake_and_list_are_recorded() {
    let project = Tmp::project("tel-proto");
    let mut input = lines(&[initialize(1, "2025-11-25")]);
    input.extend_from_slice(b"{not json\n");
    input.extend_from_slice(&lines(&[
        frame(
            json!(2),
            "tools/list",
            json!({"_meta": {PV: "1999-01-01", CAPS: {}}}),
        ),
        frame(json!(3), "no/such", json!({})),
    ]));
    let out = run_mcp_args(Some(&project.0), input, &[], &[]);
    frames_of(&out);
    let evs = events(&project.0);
    assert!(evs.iter().all(|e| e.kind == EventKind::ProtocolError));
    let codes: Vec<_> = evs.iter().map(|e| e.error_code.clone().unwrap()).collect();
    assert_eq!(
        codes,
        [
            "parse_error",
            "unsupported_protocol_version",
            "method_not_found"
        ]
    );
    let mut traces: Vec<_> = evs.iter().map(|e| e.trace_id.clone()).collect();
    traces.sort();
    traces.dedup();
    assert_eq!(traces.len(), 3, "each protocol error is its own trace");
}

#[test]
fn ts027_each_server_process_gets_its_own_session() {
    let project = Tmp::project("tel-sessions");
    traced(&project.0, &[call(1, "status", json!({}))]);
    traced(&project.0, &[call(1, "status", json!({}))]);
    let evs = events(&project.0);
    let mut sessions: Vec<_> = evs.iter().map(|e| e.session_id.clone()).collect();
    sessions.sort();
    sessions.dedup();
    assert_eq!(sessions.len(), 2);
    assert_eq!(store_files(&project.0).len(), 2);
}

#[test]
fn ts027_a_failing_store_changes_nothing_on_stdout() {
    let input = {
        let mut i = lines(&[
            initialize(1, "2025-11-25"),
            call(2, "status", json!({})),
            call(3, "nope", json!({})),
            call(4, "report", json!({"result": "complete"})),
        ]);
        i.extend_from_slice(b"{not json\n");
        i
    };
    let on = Tmp::project("tel-bad-on");
    let off = Tmp::project("tel-bad-off");
    for p in [&on.0, &off.0] {
        fs::create_dir_all(p.join(".aidlc")).unwrap();
        fs::write(p.join(".aidlc/telemetry"), "planted").unwrap();
    }
    let a = run_mcp_args(Some(&on.0), input.clone(), &[], &["--telemetry", "on"]);
    let b = run_mcp_args(Some(&off.0), input, &[], &["--telemetry", "off"]);
    assert_eq!(a.status.code(), Some(0));
    assert_eq!(b.status.code(), Some(0));
    assert_eq!(a.stdout, b.stdout);
    assert!(!a.stdout.is_empty());
    let err = String::from_utf8_lossy(&a.stderr);
    // A regular file at .aidlc/telemetry is an unsafe location on every platform.
    assert_eq!(
        err,
        "dtech-aidlc-mcp: telemetry disabled (unsafe telemetry location)\n"
    );
    assert!(b.stderr.is_empty());
    for p in [&on.0, &off.0] {
        assert_eq!(fs::read(p.join(".aidlc/telemetry")).unwrap(), b"planted");
    }
}

#[test]
fn ts027_concurrent_servers_write_separate_valid_stores() {
    let project = Tmp::project("tel-race");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let (root, barrier) = (project.0.clone(), barrier.clone());
            let input = lines(&[call(1, "status", json!({})), call(2, "queue", json!({}))]);
            std::thread::spawn(move || {
                use std::io::Write;
                let mut child = std::process::Command::new(MCP_BIN)
                    .arg("--project")
                    .arg(&root)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut stdin = child.stdin.take().unwrap();
                barrier.wait();
                stdin.write_all(&input).unwrap();
                drop(stdin);
                child.wait_with_output().unwrap()
            })
        })
        .collect();
    for h in handles {
        let out = h.join().unwrap();
        assert!(out.stderr.is_empty());
    }
    let files = store_files(&project.0);
    assert_eq!(files.len(), 2);
    let mut seen = std::collections::HashSet::new();
    for f in files {
        for line in fs::read_to_string(f).unwrap().lines() {
            let e = parse_line(line).unwrap_or_else(|| panic!("bad line: {line}"));
            assert!(seen.insert(e.event_id), "eventId appears twice");
        }
    }
    assert!(!seen.is_empty());
}
