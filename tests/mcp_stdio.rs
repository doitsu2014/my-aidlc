//! Child-process sessions of `dtech-aidlc-mcp` (TS-005) and the hostile-input table
//! (TS-014). Both protocol eras are driven through the real binary; stdout must carry
//! protocol frames only.

#[path = "fixtures/mcp/common.rs"]
mod common;

use common::*;
use serde_json::{json, Value};

const LEGACY_SESSION: &[u8] = include_bytes!("fixtures/mcp/legacy_session.jsonl");
const MODERN_SESSION: &[u8] = include_bytes!("fixtures/mcp/modern_session.jsonl");

fn started_project(label: &str) -> Tmp {
    let tmp = Tmp::project(label);
    start_intent(&tmp.0, "Add login page");
    tmp
}

fn server_info() -> Value {
    json!({"name": "dtech-aidlc-mcp", "version": env!("CARGO_PKG_VERSION")})
}

fn tool_names(list: &Value) -> Vec<&str> {
    list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect()
}

#[test]
fn ts005_legacy_session_is_stdout_pure_and_answers_every_request() {
    let project = started_project("stdio-legacy");
    let out = run_mcp(Some(&project.0), LEGACY_SESSION.to_vec());
    let replies = frames_of(&out);
    // Six lines in, one notification among them: five replies, in order, ids echoed.
    assert_eq!(replies.len(), 5, "{replies:?}");
    assert_eq!(
        replies[0],
        json!({"jsonrpc": "2.0", "id": 1, "result": {
            "protocolVersion": "2025-11-25",
            "capabilities": {"tools": {}},
            "serverInfo": server_info()}})
    );
    assert_eq!(replies[1], json!({"jsonrpc": "2.0", "id": 2, "result": {}}));
    assert_eq!(replies[2]["id"], 3);
    assert_eq!(
        tool_names(&replies[2]["result"]),
        ["status", "queue", "next", "report"]
    );
    // Legacy results carry nothing modern.
    for key in ["resultType", "_meta", "ttlMs", "cacheScope"] {
        assert!(replies[2]["result"].get(key).is_none(), "{key}");
    }
    assert_eq!(replies[3]["id"], "four");
    let result = &replies[3]["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["intentId"], "add-login-page");
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        result["structuredContent"]
    );
    // AC-012: a root in a call is an unknown field, not a protocol error.
    assert_eq!(replies[4]["id"], 5);
    assert_eq!(replies[4]["result"]["isError"], true);
    assert_eq!(
        replies[4]["result"]["structuredContent"]["error"]["code"],
        "invalid_arguments"
    );
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn ts005_legacy_server_echoes_the_older_supported_version_and_defaults_unknown_ones() {
    let project = started_project("stdio-versions");
    let out = run_mcp(
        Some(&project.0),
        lines(&[
            initialize(1, "2025-06-18"),
            initialize(2, "1999-01-01"),
            initialize(3, "2025-11-25"),
        ]),
    );
    let versions: Vec<_> = frames_of(&out)
        .iter()
        .map(|f| f["result"]["protocolVersion"].clone())
        .collect();
    assert_eq!(versions, ["2025-06-18", "2025-11-25", "2025-11-25"]);
}

#[test]
fn ts005_modern_session_needs_no_handshake_and_stamps_results_not_errors() {
    let project = started_project("stdio-modern");
    let out = run_mcp(Some(&project.0), MODERN_SESSION.to_vec());
    let replies = frames_of(&out);
    assert_eq!(replies.len(), 5, "{replies:?}");
    let stamp = json!({"io.modelcontextprotocol/serverInfo": server_info()});
    let discover = &replies[0]["result"];
    assert_eq!(discover["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(discover["resultType"], "complete");
    assert_eq!(discover["_meta"], stamp);
    assert_eq!(discover["ttlMs"], 0);
    assert_eq!(discover["cacheScope"], "private");
    let list = &replies[1]["result"];
    assert_eq!(tool_names(list), ["status", "queue", "next", "report"]);
    assert_eq!(list["resultType"], "complete");
    assert_eq!(list["ttlMs"], 0);
    assert_eq!(list["cacheScope"], "private");
    assert!(list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["inputSchema"]["additionalProperties"] == false
            && t["annotations"]["openWorldHint"] == false
            && t.get("outputSchema").is_none()));
    let call = &replies[2]["result"];
    assert_eq!(call["resultType"], "complete");
    assert_eq!(call["_meta"], stamp);
    assert!(call.get("ttlMs").is_none());
    assert_eq!(call["structuredContent"]["intentId"], "add-login-page");
    // `ping` does not exist in the modern era; an unsupported version names what is.
    assert_eq!(replies[3]["error"]["code"], -32601);
    assert_eq!(replies[4]["error"]["code"], -32022);
    assert_eq!(
        replies[4]["error"]["data"],
        json!({"supported": ["2026-07-28"], "requested": "2099-01-01"})
    );
    assert!(replies[3].get("result").is_none() && replies[4].get("result").is_none());
    assert!(out.stderr.is_empty());
}

#[test]
fn ts005_eras_mix_per_request_on_one_connection() {
    let project = started_project("stdio-mixed");
    let out = run_mcp(
        Some(&project.0),
        lines(&[
            initialize(1, "2025-11-25"),
            modern(2, "tools/list", json!({})),
            frame(json!(3), "tools/list", json!({})),
        ]),
    );
    let replies = frames_of(&out);
    assert_eq!(replies[1]["result"]["resultType"], "complete");
    assert!(replies[2]["result"].get("resultType").is_none());
    assert_eq!(replies[2]["id"], 3);
}

#[test]
fn ts005_empty_stdin_exits_zero_and_writes_nothing() {
    let project = Tmp::new("stdio-eof");
    let out = run_mcp(Some(&project.0), Vec::new());
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
    // Opening the server touches nothing in the project.
    assert!(snapshot(&project.0).is_empty());
}

#[test]
fn ts005_crlf_framing_is_accepted_and_replies_are_lf_only() {
    let project = started_project("stdio-crlf");
    let mut input = Vec::new();
    for f in [
        initialize(1, "2025-11-25"),
        frame(json!(2), "ping", json!({})),
    ] {
        input.extend(format!("{f}\r\n").into_bytes());
    }
    let replies = frames_of(&run_mcp(Some(&project.0), input));
    assert_eq!(replies.len(), 2);
}

#[test]
fn ts005_startup_failure_exits_one_before_reading_stdin() {
    let tmp = Tmp::new("stdio-startup");
    let missing = tmp.0.join("missing");
    let out = run_mcp(Some(&missing), lines(&[initialize(1, "2025-11-25")]));
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no frame on a startup failure");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("dtech-aidlc-mcp: "), "{stderr}");
    assert!(!tmp.0.join("missing").exists());
    // A project path that is a file is refused the same way.
    let file = tmp.0.join("file");
    std::fs::write(&file, "x").unwrap();
    let out = run_mcp(Some(&file), Vec::new());
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    // Unknown arguments are refused; there is no --version or --help.
    for args in [
        &["--version"][..],
        &["--help"],
        &["--project"],
        &["x"],
        &["--telemetry"],
        &["--telemetry", "ON"],
        &["--telemetry", "yes"],
        &["--telemetry", "on", "--telemetry", "off"],
        &["--project", "d", "--project", "e"],
        &["--telemetry", "on", "x"],
    ] {
        let out = std::process::Command::new(MCP_BIN)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).starts_with("dtech-aidlc-mcp: "));
    }
}

#[test]
fn ts005_root_is_fixed_at_launch_and_never_taken_from_a_call() {
    let bound = started_project("stdio-root-a");
    let other = Tmp::project("stdio-root-b");
    start_intent(&other.0, "Other project work");
    let other_before = snapshot(&other.0);
    // Every spelling of a root is an unknown field and reaches nothing.
    for field in [
        "root",
        "project",
        "project_dir",
        "cwd",
        "path",
        "projectRoot",
    ] {
        let err = tool_err(
            &bound.0,
            "status",
            json!({ field: other.0.to_string_lossy() }),
        );
        assert_eq!(err["code"], "invalid_arguments", "{field}");
        assert_eq!(err["retryable"], false);
    }
    // The server answers for the launch root only.
    let status = tool_ok(&bound.0, "status", json!({}));
    assert_eq!(status["intentId"], "add-login-page");
    assert_eq!(snapshot(&other.0), other_before);
}

#[test]
fn ts005_launch_directory_is_the_root_without_project_flag() {
    let project = started_project("stdio-cwd");
    let mut command = std::process::Command::new(MCP_BIN);
    command
        .current_dir(&project.0)
        .args(["--telemetry", "off"])
        .env_remove("DTECH_AIDLC_HOME")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    {
        use std::io::Write;
        let call = modern(1, "tools/call", json!({"name": "status", "arguments": {}}));
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&lines(&[call]))
            .unwrap();
    }
    let replies = frames_of(&child.wait_with_output().unwrap());
    assert_eq!(
        replies[0]["result"]["structuredContent"]["intentId"],
        "add-login-page"
    );
}

/// Frames for the hostile table: raw bytes of one input line and the expected error.
struct Hostile {
    name: &'static str,
    line: Vec<u8>,
    code: i64,
    id: Value,
}

fn hostile(name: &'static str, line: &str, code: i64, id: Value) -> Hostile {
    Hostile {
        name,
        line: line.as_bytes().to_vec(),
        code,
        id,
    }
}

fn meta_text() -> String {
    modern_meta().to_string()
}

#[test]
fn ts014_hostile_protocol_input_table_keeps_the_session_alive() {
    let project = started_project("stdio-hostile");
    let meta = meta_text();
    let mut table = vec![
        hostile("invalid json", "{not json", -32700, Value::Null),
        hostile(
            "truncated json",
            r#"{"jsonrpc":"2.0","id":1,"meth"#,
            -32700,
            Value::Null,
        ),
        Hostile {
            name: "invalid utf-8",
            line: vec![b'{', 0xff, 0xfe, b'}'],
            code: -32700,
            id: Value::Null,
        },
        hostile(
            "batch array",
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
            -32600,
            Value::Null,
        ),
        hostile("empty batch", "[]", -32600, Value::Null),
        hostile("number", "42", -32600, Value::Null),
        hostile("string", "\"ping\"", -32600, Value::Null),
        hostile("null", "null", -32600, Value::Null),
        hostile(
            "null id",
            r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            -32600,
            Value::Null,
        ),
        hostile(
            "object id",
            r#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#,
            -32600,
            Value::Null,
        ),
        hostile(
            "wrong jsonrpc",
            r#"{"jsonrpc":"1.0","id":7,"method":"ping"}"#,
            -32600,
            json!(7),
        ),
        hostile(
            "missing jsonrpc",
            r#"{"id":8,"method":"ping"}"#,
            -32600,
            json!(8),
        ),
        hostile(
            "missing method",
            r#"{"jsonrpc":"2.0","id":9}"#,
            -32600,
            json!(9),
        ),
        hostile(
            "numeric method",
            r#"{"jsonrpc":"2.0","id":10,"method":5}"#,
            -32600,
            json!(10),
        ),
        hostile(
            "params array",
            r#"{"jsonrpc":"2.0","id":11,"method":"tools/list","params":[]}"#,
            -32602,
            json!(11),
        ),
        hostile(
            "modern unknown method",
            &format!(
                r#"{{"jsonrpc":"2.0","id":12,"method":"resources/list","params":{{"_meta":{meta}}}}}"#
            ),
            -32601,
            json!(12),
        ),
        hostile(
            "modern ping",
            &format!(r#"{{"jsonrpc":"2.0","id":13,"method":"ping","params":{{"_meta":{meta}}}}}"#),
            -32601,
            json!(13),
        ),
        hostile(
            "meta not object",
            r#"{"jsonrpc":"2.0","id":14,"method":"tools/list","params":{"_meta":"x"}}"#,
            -32602,
            json!(14),
        ),
        hostile(
            "meta without capabilities",
            r#"{"jsonrpc":"2.0","id":15,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
            -32602,
            json!(15),
        ),
        hostile(
            "meta version not a string",
            r#"{"jsonrpc":"2.0","id":16,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":5,"io.modelcontextprotocol/clientCapabilities":{}}}}"#,
            -32602,
            json!(16),
        ),
        hostile(
            "unsupported modern version",
            r#"{"jsonrpc":"2.0","id":17,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2030-01-01","io.modelcontextprotocol/clientCapabilities":{}}}}"#,
            -32022,
            json!(17),
        ),
        hostile(
            "unknown tool",
            &format!(
                r#"{{"jsonrpc":"2.0","id":18,"method":"tools/call","params":{{"name":"doctor","arguments":{{}},"_meta":{meta}}}}}"#
            ),
            -32602,
            json!(18),
        ),
        hostile(
            "tool name not a string",
            &format!(
                r#"{{"jsonrpc":"2.0","id":19,"method":"tools/call","params":{{"name":3,"_meta":{meta}}}}}"#
            ),
            -32602,
            json!(19),
        ),
        hostile(
            "arguments not an object",
            &format!(
                r#"{{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{{"name":"status","arguments":[],"_meta":{meta}}}}}"#
            ),
            -32602,
            json!(20),
        ),
        hostile(
            "legacy request before initialize",
            r#"{"jsonrpc":"2.0","id":21,"method":"tools/list"}"#,
            -32602,
            json!(21),
        ),
        hostile(
            "initialize without version",
            r#"{"jsonrpc":"2.0","id":22,"method":"initialize","params":{}}"#,
            -32602,
            json!(22),
        ),
        hostile(
            "initialize with numeric version",
            r#"{"jsonrpc":"2.0","id":23,"method":"initialize","params":{"protocolVersion":20251125}}"#,
            -32602,
            json!(23),
        ),
    ];
    // Oversized line: three bytes past the 1 MiB cap (the error carries no id).
    let mut oversized = br#"{"jsonrpc":"2.0","id":24,"method":"ping","params":{"pad":""#.to_vec();
    oversized.resize(1 << 20, b'a');
    oversized.extend(br#""}}"#);
    table.push(Hostile {
        name: "oversized line",
        line: oversized,
        code: -32600,
        id: Value::Null,
    });

    let mut input = Vec::new();
    for case in &table {
        input.extend(&case.line);
        input.push(b'\n');
    }
    // Notifications are never answered, even when malformed.
    input.extend(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    input.push(b'\n');
    input.extend(br#"{"jsonrpc":"2.0","method":"nonsense","params":[1]}"#);
    input.push(b'\n');
    input.extend(b"   \n\n");
    // The session must still serve a valid request after every hostile line.
    input.extend(lines(&[modern(99, "tools/list", json!({}))]));

    let before = snapshot(&project.0);
    let out = run_mcp(Some(&project.0), input);
    let replies = frames_of(&out);
    assert_eq!(replies.len(), table.len() + 1, "{replies:#?}");
    for (case, reply) in table.iter().zip(&replies) {
        assert_eq!(reply["error"]["code"], case.code, "{}: {reply}", case.name);
        assert_eq!(reply["id"], case.id, "{}: {reply}", case.name);
    }
    let unsupported = table
        .iter()
        .position(|c| c.name == "unsupported modern version")
        .unwrap();
    assert_eq!(
        replies[unsupported]["error"]["data"],
        json!({"supported": ["2026-07-28"], "requested": "2030-01-01"})
    );
    let last = replies.last().unwrap();
    assert_eq!(last["id"], 99);
    assert_eq!(
        tool_names(&last["result"]),
        ["status", "queue", "next", "report"]
    );
    assert_eq!(snapshot(&project.0), before, "hostile input changed state");
}

#[test]
fn ts014_hostile_tool_arguments_are_rejected_before_the_project_is_opened() {
    // A directory that is not a project: validation must not create or read anything.
    let empty = Tmp::new("stdio-args");
    let long = |n: usize| "x".repeat(n);
    let needs17: Vec<String> = (0..17).map(|i| format!("n{i}")).collect();
    let cases: Vec<(&str, Value)> = vec![
        ("status", json!({"intent_id": "Bad_Id"})),
        ("status", json!({"intent_id": long(65)})),
        ("status", json!({"intent_id": 5})),
        ("status", json!({"intent_id": ""})),
        ("status", json!({"intent_id": "../etc"})),
        ("status", json!({"root": "/etc"})),
        ("queue", json!({"x": 1})),
        ("next", json!({"claim": "yes"})),
        ("next", json!({"unit": "WU 1"})),
        ("next", json!({"unit": long(65)})),
        ("next", json!({"claim": true, "unit": "WU-001"})),
        ("next", json!({"intent": "start something new"})),
        ("next", json!({"approved_by": "me"})),
        (
            "next",
            json!({"scope": "team", "depth": "full", "mode": "ai"}),
        ),
        ("next", json!({"force_resume": true})),
        ("report", json!({"result": "done"})),
        ("report", json!({"approval": "maybe"})),
        ("report", json!({"stage": "Bad Stage"})),
        ("report", json!({"stage": long(65)})),
        ("report", json!({"unit": "a/b"})),
        ("report", json!({"detail": long(16385)})),
        ("report", json!({"approval_note": long(4097)})),
        ("report", json!({"approved_by": ""})),
        ("report", json!({"approved_by": long(201)})),
        ("report", json!({"needs": needs17})),
        ("report", json!({"needs": ["Bad"]})),
        ("report", json!({"needs": [long(33)]})),
        ("report", json!({"needs": "toolchain"})),
        ("report", json!({"artifacts": ["x"]})),
        ("report", json!({"evidence": "x"})),
        ("report", json!({"force_resume": true})),
    ];
    let mut input = Vec::new();
    for (i, (name, args)) in cases.iter().enumerate() {
        input.extend(lines(&[modern(
            i as u64,
            "tools/call",
            json!({"name": name, "arguments": args}),
        )]));
    }
    let out = run_mcp(Some(&empty.0), input);
    let replies = frames_of(&out);
    assert_eq!(replies.len(), cases.len());
    for ((name, args), reply) in cases.iter().zip(&replies) {
        let result = &reply["result"];
        assert_eq!(result["isError"], true, "{name} {args}");
        let error = &result["structuredContent"]["error"];
        assert_eq!(error["code"], "invalid_arguments", "{name} {args}: {reply}");
        assert_eq!(error["retryable"], false);
        assert!(error["message"].as_str().unwrap().chars().count() <= 1000);
    }
    assert!(snapshot(&empty.0).is_empty());
    assert!(!empty.0.join(".aidlc").exists());
}

#[test]
fn ts014_boundary_values_pass_validation() {
    // At the caps the arguments are valid; the call then fails (or not) for workflow
    // reasons, never as invalid_arguments.
    let project = started_project("stdio-bounds");
    let ok_args = [
        ("status", json!({"intent_id": "a".repeat(64)})),
        ("report", json!({"approval_note": "n".repeat(4096)})),
        ("report", json!({"approved_by": "a".repeat(200)})),
        (
            "report",
            json!({"stage": "nope", "result": "blocked", "detail": "x".repeat(16384),
            "needs": vec!["n".repeat(32); 16]}),
        ),
    ];
    for (name, args) in ok_args {
        let (value, is_error) = tool(&project.0, name, args);
        if is_error {
            assert_ne!(value["error"]["code"], "invalid_arguments", "{value}");
        }
    }
}

#[test]
fn ts014_line_of_exactly_the_cap_is_accepted() {
    let mut line = br#"{"jsonrpc":"2.0","id":24,"method":"ping","params":{"pad":""#.to_vec();
    let tail = br#""}}"#;
    line.resize((1 << 20) - tail.len(), b'a');
    line.extend(tail);
    assert_eq!(line.len(), 1 << 20);
    line.push(b'\n');
    let replies = frames_of(&run_mcp(None, line));
    assert_eq!(replies.len(), 1, "{replies:#?}");
    assert!(replies[0].get("error").is_none(), "{}", replies[0]);
    assert_eq!(replies[0]["id"], 24);
}
