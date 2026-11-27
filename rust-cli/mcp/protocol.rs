//! JSON-RPC 2.0 over stdio for the MCP server: framing (FN-010), per-request era
//! dispatch (FN-011) and the serve loop (FN-012). Hand-rolled on purpose (ADR-001); the
//! `(method, params) -> Result<Value, Value>` boundary keeps an SDK swap contained.

use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

/// Per-line cap (CFG-006).
pub const MAX_LINE_BYTES: usize = 1 << 20;

/// What the protocol layer needs from the tool adapter. Generic over the adapter's
/// context so this module does not depend on `tools.rs` (IF-005, ToolContext).
pub struct ToolHost<C> {
    pub list: fn() -> Value,
    /// `None` means unknown tool. `Ok`/`Err` carry structuredContent objects.
    pub call: fn(&C, &str, &Value) -> Option<Result<Value, Value>>,
}

const LEGACY: &[&str] = &["2025-11-25", "2025-06-18"]; // newest first
const MODERN: &[&str] = &["2026-07-28"];
const PV: &str = "io.modelcontextprotocol/protocolVersion";
const CAPS: &str = "io.modelcontextprotocol/clientCapabilities";
const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";

enum Frame {
    Eof,
    Line(String),
    Oversized,
    BadUtf8,
}

/// FN-010: one bounded line. Blank lines are skipped; a trailing `\r` is stripped; an
/// oversized line is drained to its newline so the session continues. I/O errors end it.
fn read_frame<R: BufRead>(r: &mut R) -> Frame {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // room for MAX_LINE_BYTES of content plus an optional CR and the LF
        let cap = MAX_LINE_BYTES as u64 + 2;
        let n = match (&mut *r).take(cap).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return Frame::Eof,
            Ok(n) => n as u64,
        };
        let newline = buf.last() == Some(&b'\n');
        if newline {
            buf.pop();
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        if buf.len() > MAX_LINE_BYTES || (!newline && n == cap) {
            // drain the remainder of the line without keeping it
            let mut more = newline;
            while !more {
                buf.clear();
                match (&mut *r).take(cap).read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => more = buf.last() == Some(&b'\n'),
                }
            }
            return Frame::Oversized;
        }
        if buf.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        return match String::from_utf8(std::mem::take(&mut buf)) {
            Ok(line) => Frame::Line(line),
            Err(_) => Frame::BadUtf8,
        };
    }
}

fn error(code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({"code": code, "message": message});
    if let Some(d) = data {
        e["data"] = d;
    }
    e
}

fn frame_error(id: Value, e: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": e})
}

fn server_info() -> Value {
    json!({"name": "dtech-aidlc-mcp", "version": env!("CARGO_PKG_VERSION")})
}

fn tool_result(r: Result<Value, Value>) -> Value {
    let (v, is_error) = match r {
        Ok(v) => (v, false),
        Err(v) => (v, true),
    };
    json!({"content": [{"type": "text", "text": v.to_string()}], "structuredContent": v, "isError": is_error})
}

/// The era-independent handlers: `(method, params) -> Result<Value, Value>`.
fn call_method<C>(
    host: &ToolHost<C>,
    ctx: &C,
    method: &str,
    params: &Value,
) -> Result<Value, Value> {
    match method {
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": (host.list)()})),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Err(error(-32602, "tools/call requires a string name", None));
            };
            let empty = json!({});
            let args = params.get("arguments").unwrap_or(&empty);
            if !args.is_object() {
                return Err(error(-32602, "arguments must be an object", None));
            }
            match (host.call)(ctx, name, args) {
                None => Err(error(-32602, &format!("Unknown tool: {name}"), None)),
                Some(r) => Ok(tool_result(r)),
            }
        }
        _ => Err(error(-32601, &format!("Method not found: {method}"), None)),
    }
}

/// FN-011: per-request era detection (ADR-005 rule 4) and modern result stamping.
fn dispatch<C>(
    initialized: &mut bool,
    host: &ToolHost<C>,
    ctx: &C,
    method: &str,
    params: &Value,
) -> Result<Value, Value> {
    if method == "initialize" {
        let Some(asked) = params.get("protocolVersion").and_then(Value::as_str) else {
            return Err(error(
                -32602,
                "initialize requires a string protocolVersion",
                None,
            ));
        };
        let v = LEGACY
            .iter()
            .find(|x| **x == asked)
            .copied()
            .unwrap_or(LEGACY[0]);
        *initialized = true;
        return Ok(
            json!({"protocolVersion": v, "capabilities": {"tools": {}}, "serverInfo": server_info()}),
        );
    }
    let meta = match params.get("_meta") {
        None => None,
        Some(m) if m.is_object() => Some(m),
        Some(_) => return Err(error(-32602, "_meta must be an object", None)),
    };
    let Some(version) = meta.and_then(|m| m.get(PV)) else {
        // legacy era (key absent)
        return match method {
            "ping" => Ok(json!({})),
            _ if !*initialized => Err(error(
                -32602,
                "Send initialize (supported: 2025-11-25, 2025-06-18) or include _meta io.modelcontextprotocol/protocolVersion and io.modelcontextprotocol/clientCapabilities (supported: 2026-07-28)",
                None,
            )),
            "tools/list" | "tools/call" => call_method(host, ctx, method, params),
            _ => Err(error(-32601, &format!("Method not found: {method}"), None)),
        };
    };
    // modern era (key present): validate _meta before touching the method
    let version = match (version.as_str(), meta.and_then(|m| m.get(CAPS))) {
        (Some(v), Some(c)) if c.is_object() => v,
        _ => {
            return Err(error(
                -32602,
                "request _meta must carry io.modelcontextprotocol/protocolVersion (string) and io.modelcontextprotocol/clientCapabilities (object)",
                Some(json!({"supported": MODERN})),
            ))
        }
    };
    if !MODERN.contains(&version) {
        return Err(error(
            -32022,
            "Unsupported protocol version",
            Some(json!({"supported": MODERN, "requested": version})),
        ));
    }
    let mut out = match method {
        "server/discover" => json!({"supportedVersions": MODERN, "capabilities": {"tools": {}}}),
        "tools/list" | "tools/call" => call_method(host, ctx, method, params)?,
        _ => return Err(error(-32601, &format!("Method not found: {method}"), None)),
    };
    if method != "tools/call" {
        out["ttlMs"] = json!(0);
        out["cacheScope"] = json!("private");
    }
    out["resultType"] = json!("complete");
    out["_meta"] = json!({ SERVER_INFO_KEY: server_info() });
    Ok(out)
}

/// Validate the envelope and answer one parsed message; `None` for notifications.
fn handle<C>(initialized: &mut bool, host: &ToolHost<C>, ctx: &C, msg: &Value) -> Option<Value> {
    let invalid = |id: Value| Some(frame_error(id, error(-32600, "Invalid request", None)));
    let Some(obj) = msg.as_object() else {
        return invalid(Value::Null);
    };
    // FN-011: echo a valid id verbatim, even when the envelope is otherwise invalid.
    let id = match obj.get("id") {
        None => None,
        Some(id) if id.is_string() || id.is_number() => Some(id.clone()),
        Some(_) => return invalid(Value::Null),
    };
    if obj.get("jsonrpc") != Some(&json!("2.0")) {
        return invalid(id.unwrap_or(Value::Null));
    }
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return invalid(id.unwrap_or(Value::Null));
    };
    // Notifications are never answered and never validated (ADR-005 rule 4e).
    let id = id?;
    let params = match obj.get("params") {
        None => json!({}),
        Some(p) if p.is_object() => p.clone(),
        Some(_) => {
            return Some(frame_error(
                id,
                error(-32602, "params must be an object", None),
            ))
        }
    };
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch(initialized, host, ctx, method, &params)
    }))
    .unwrap_or_else(|_| Err(error(-32603, "Internal error", None)));
    Some(match r {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(e) => frame_error(id, e),
    })
}

/// FN-012: serve until EOF (returns 0) or a stdout write error (returns 1). The only
/// stdout writer: one compact line plus flush per response.
pub fn serve<R: BufRead, W: Write, C>(
    mut input: R,
    mut output: W,
    host: &ToolHost<C>,
    ctx: &C,
) -> i32 {
    let mut initialized = false;
    loop {
        let reply = match read_frame(&mut input) {
            Frame::Eof => return 0,
            Frame::Oversized => Some(frame_error(
                Value::Null,
                error(
                    -32600,
                    &format!("Invalid request: line exceeds {MAX_LINE_BYTES} bytes"),
                    None,
                ),
            )),
            Frame::BadUtf8 => Some(frame_error(Value::Null, error(-32700, "Parse error", None))),
            Frame::Line(l) => match serde_json::from_str::<Value>(&l) {
                Ok(msg) => handle(&mut initialized, host, ctx, &msg),
                Err(_) => Some(frame_error(Value::Null, error(-32700, "Parse error", None))),
            },
        };
        if let Some(v) = reply {
            if writeln!(output, "{v}")
                .and_then(|_| output.flush())
                .is_err()
            {
                return 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list() -> Value {
        json!([{"name": "echo", "inputSchema": {"type": "object"}}])
    }
    fn call(_: &(), name: &str, args: &Value) -> Option<Result<Value, Value>> {
        match name {
            "echo" => Some(Ok(json!({"echo": args}))),
            "fail" => Some(Err(json!({"error": {"code": "operation_failed"}}))),
            "boom" => panic!("secret detail"),
            _ => None,
        }
    }
    const HOST: ToolHost<()> = ToolHost { list, call };

    fn raw(input: &[u8]) -> (i32, Vec<u8>) {
        let mut out = Vec::new();
        let code = serve(input, &mut out, &HOST, &());
        (code, out)
    }
    fn run(input: &[u8]) -> Vec<Value> {
        let (code, out) = raw(input);
        assert_eq!(code, 0);
        assert!(!out.contains(&b'\r'));
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn lines(frames: &[Value]) -> Vec<u8> {
        frames
            .iter()
            .flat_map(|f| format!("{f}\n").into_bytes())
            .collect()
    }
    fn ok(id: Value, result: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }
    fn err(id: Value, code: i64, message: &str) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
    }
    fn err_data(id: Value, code: i64, message: &str, data: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message, "data": data}})
    }
    fn req(id: Value, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }
    fn info() -> Value {
        json!({"name": "dtech-aidlc-mcp", "version": env!("CARGO_PKG_VERSION")})
    }
    fn meta(version: Value) -> Value {
        json!({"io.modelcontextprotocol/protocolVersion": version,
               "io.modelcontextprotocol/clientCapabilities": {}})
    }
    fn stamp() -> Value {
        json!({"io.modelcontextprotocol/serverInfo": info()})
    }
    fn text_of(v: &Value) -> Value {
        json!([{"type": "text", "text": v.to_string()}])
    }

    #[test]
    fn ts001_legacy_session_exact_frames() {
        let input = lines(&[
            req(
                json!(0),
                "initialize",
                json!({"protocolVersion": "2025-11-25", "capabilities": {}}),
            ),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            req(json!("p"), "ping", json!({})),
            req(json!(1), "tools/list", json!({})),
            req(
                json!(2),
                "tools/call",
                json!({"name": "echo", "arguments": {"a": 1}}),
            ),
            req(json!(3), "tools/call", json!({"name": "fail"})),
            req(json!(4), "tools/call", json!({"name": "nope"})),
            req(json!(5), "resources/list", json!({})),
            req(json!(6), "server/discover", json!({})),
            req(
                json!(7),
                "tools/call",
                json!({"name": "echo", "arguments": []}),
            ),
            req(json!(8), "tools/call", json!({"arguments": {}})),
        ]);
        let echo = json!({"echo": {"a": 1}});
        let fail = json!({"error": {"code": "operation_failed"}});
        assert_eq!(
            run(&input),
            vec![
                ok(
                    json!(0),
                    json!({"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": info()})
                ),
                ok(json!("p"), json!({})),
                ok(json!(1), json!({"tools": list()})),
                ok(
                    json!(2),
                    json!({"content": text_of(&echo), "structuredContent": echo, "isError": false})
                ),
                ok(
                    json!(3),
                    json!({"content": text_of(&fail), "structuredContent": fail, "isError": true})
                ),
                err(json!(4), -32602, "Unknown tool: nope"),
                err(json!(5), -32601, "Method not found: resources/list"),
                err(json!(6), -32601, "Method not found: server/discover"),
                err(json!(7), -32602, "arguments must be an object"),
                err(json!(8), -32602, "tools/call requires a string name"),
            ]
        );
    }

    #[test]
    fn ts001_initialize_version_negotiation() {
        let out = run(&lines(&[
            req(
                json!(1),
                "initialize",
                json!({"protocolVersion": "2025-06-18"}),
            ),
            req(
                json!(2),
                "initialize",
                json!({"protocolVersion": "1999-01-01"}),
            ),
            req(json!(3), "initialize", json!({})),
            req(json!(4), "initialize", json!({"protocolVersion": 5})),
            req(
                json!(5),
                "initialize",
                json!({"protocolVersion": "2025-06-18", "_meta": meta(json!("2026-07-28"))}),
            ),
        ]));
        assert_eq!(out[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(out[1]["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(out[2]["error"]["code"], -32602);
        assert_eq!(out[3]["error"]["code"], -32602);
        // initialize is always legacy: no modern stamps whatever its _meta.
        assert_eq!(
            out[4]["result"],
            json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": info()})
        );
    }

    #[test]
    fn ts001_before_initialize_without_key() {
        let out = run(&lines(&[
            req(json!(1), "ping", json!({})),
            req(json!(2), "tools/list", json!({})),
            req(json!(3), "server/discover", json!({})),
            req(json!(4), "bogus", json!({})),
        ]));
        assert_eq!(out[0], ok(json!(1), json!({})));
        let msg = "Send initialize (supported: 2025-11-25, 2025-06-18) or include _meta io.modelcontextprotocol/protocolVersion and io.modelcontextprotocol/clientCapabilities (supported: 2026-07-28)";
        for (i, id) in [(1, 2), (2, 3), (3, 4)] {
            assert_eq!(out[i], err(json!(id), -32602, msg));
        }
    }

    #[test]
    fn ts002_modern_exact_frames() {
        let echo = json!({"echo": {"x": true}});
        let fail = json!({"error": {"code": "operation_failed"}});
        let m = || json!({"_meta": meta(json!("2026-07-28"))});
        let with = |extra: Value| {
            let mut p = m();
            for (k, v) in extra.as_object().unwrap() {
                p[k] = v.clone();
            }
            p
        };
        let out = run(&lines(&[
            req(json!("server-discover-probe-1"), "server/discover", m()),
            req(json!(0), "tools/list", m()),
            req(
                json!(1),
                "tools/call",
                with(json!({"name": "echo", "arguments": {"x": true}})),
            ),
            req(json!(2), "tools/call", with(json!({"name": "fail"}))),
            req(json!(3), "tools/call", with(json!({"name": "nope"}))),
            req(json!(4), "ping", m()),
            req(json!(5), "initialize", m()),
            req(json!(6), "resources/list", m()),
        ]));
        assert_eq!(
            out,
            vec![
                ok(
                    json!("server-discover-probe-1"),
                    json!({"supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}}, "ttlMs": 0, "cacheScope": "private", "resultType": "complete", "_meta": stamp()})
                ),
                ok(
                    json!(0),
                    json!({"tools": list(), "ttlMs": 0, "cacheScope": "private", "resultType": "complete", "_meta": stamp()})
                ),
                ok(
                    json!(1),
                    json!({"content": text_of(&echo), "structuredContent": echo, "isError": false, "resultType": "complete", "_meta": stamp()})
                ),
                ok(
                    json!(2),
                    json!({"content": text_of(&fail), "structuredContent": fail, "isError": true, "resultType": "complete", "_meta": stamp()})
                ),
                err(json!(3), -32602, "Unknown tool: nope"),
                err(json!(4), -32601, "Method not found: ping"),
                // initialize is always legacy, so the modern _meta is ignored (and params lack protocolVersion).
                err(
                    json!(5),
                    -32602,
                    "initialize requires a string protocolVersion"
                ),
                err(json!(6), -32601, "Method not found: resources/list"),
            ]
        );
    }

    #[test]
    fn ts002_modern_meta_validation() {
        let msg = "request _meta must carry io.modelcontextprotocol/protocolVersion (string) and io.modelcontextprotocol/clientCapabilities (object)";
        let caps = "io.modelcontextprotocol/clientCapabilities";
        let pv = "io.modelcontextprotocol/protocolVersion";
        let bad = |m: Value| req(json!(1), "tools/list", json!({"_meta": m}));
        let out = run(&lines(&[
            bad(meta(json!("2025-11-25"))),
            bad(meta(json!("9999-01-01"))),
            bad(meta(json!(20260728))),
            bad(meta(json!({"v": 1}))),
            bad(json!({pv: "2026-07-28"})),
            bad(json!({pv: "2026-07-28", caps: null})),
            bad(json!({pv: "2026-07-28", caps: []})),
            req(json!(2), "tools/list", json!({"_meta": 7})),
            req(json!(3), "tools/list", json!([])),
        ]));
        let unsupported = |v: &str| {
            err_data(
                json!(1),
                -32022,
                "Unsupported protocol version",
                json!({"supported": ["2026-07-28"], "requested": v}),
            )
        };
        assert_eq!(out[0], unsupported("2025-11-25"));
        assert_eq!(out[1], unsupported("9999-01-01"));
        for frame in &out[2..7] {
            assert_eq!(
                frame,
                &err_data(json!(1), -32602, msg, json!({"supported": ["2026-07-28"]}))
            );
        }
        assert_eq!(out[7], err(json!(2), -32602, "_meta must be an object"));
        assert_eq!(out[8], err(json!(3), -32602, "params must be an object"));
    }

    #[test]
    fn ts002_era_mixing_in_one_process() {
        let m = json!({"_meta": meta(json!("2026-07-28"))});
        let out = run(&lines(&[
            req(
                json!(1),
                "initialize",
                json!({"protocolVersion": "2025-11-25"}),
            ),
            req(json!(2), "tools/list", m.clone()),
            req(json!(3), "tools/list", json!({})),
            req(json!(4), "ping", json!({})),
            req(json!(5), "ping", m),
        ]));
        assert_eq!(
            out[1],
            ok(
                json!(2),
                json!({"tools": list(), "ttlMs": 0, "cacheScope": "private", "resultType": "complete", "_meta": stamp()})
            )
        );
        assert_eq!(out[2], ok(json!(3), json!({"tools": list()})));
        assert_eq!(out[3], ok(json!(4), json!({})));
        assert_eq!(out[4], err(json!(5), -32601, "Method not found: ping"));
        // Shape sweep: errors never carry modern fields.
        for f in out.iter().filter(|f| f.get("error").is_some()) {
            assert!(f["error"].get("resultType").is_none() && f.get("resultType").is_none());
        }
    }

    #[test]
    fn ts003_envelope_errors_and_notifications() {
        let input = b"[1,2]\n7\n\"s\"\n{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}\n\
{\"id\":1,\"method\":\"ping\"}\n{\"jsonrpc\":\"1.0\",\"id\":2,\"method\":\"ping\"}\n\
{\"jsonrpc\":\"2.0\",\"id\":[1],\"method\":\"ping\"}\n{\"jsonrpc\":\"2.0\",\"id\":3}\n\
{\"jsonrpc\":\"2.0\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"params\":{\"name\":\"boom\"}}\n\
{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"ping\"}\n";
        let inv = |id: Value| err(id, -32600, "Invalid request");
        assert_eq!(
            run(input),
            vec![
                inv(json!(null)),
                inv(json!(null)),
                inv(json!(null)),
                inv(json!(null)),
                inv(json!(1)),
                inv(json!(2)),
                inv(json!(null)),
                inv(json!(3)),
                inv(json!(null)), // {"jsonrpc":"2.0"}: no id, no method
                ok(json!(4), json!({})),
            ]
        );
    }

    #[test]
    fn ts003_parse_errors_crlf_blank_lines_and_eof() {
        let input = b"\r\n\n  \nnot json\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}";
        assert_eq!(
            run(input),
            vec![
                err(json!(null), -32700, "Parse error"),
                ok(json!(1), json!({})),
                ok(json!(2), json!({}))
            ]
        );
    }

    #[test]
    fn ts003_invalid_utf8_keeps_session_alive() {
        let mut input =
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{\"x\":\"".to_vec();
        input.extend([0xFF, 0xFE]);
        input.extend(b"\"}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n");
        assert_eq!(
            run(&input),
            vec![
                err(json!(null), -32700, "Parse error"),
                ok(json!(2), json!({}))
            ]
        );
    }

    #[test]
    fn ts003_oversized_line_is_drained_and_session_continues() {
        let mut input = vec![b'a'; MAX_LINE_BYTES + 5000];
        input.push(b'\n');
        // A line of exactly the cap is not oversized (it is just invalid JSON here).
        input.extend(vec![b'b'; MAX_LINE_BYTES]);
        input.push(b'\n');
        input.extend(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n");
        assert_eq!(
            run(&input),
            vec![
                err(
                    json!(null),
                    -32600,
                    "Invalid request: line exceeds 1048576 bytes"
                ),
                err(json!(null), -32700, "Parse error"),
                ok(json!(9), json!({})),
            ]
        );
        // An oversized final line without a newline also ends cleanly.
        let tail = vec![b'a'; MAX_LINE_BYTES * 2 + 1];
        assert_eq!(run(&tail).len(), 1);
    }

    #[test]
    fn ts003_cap_boundaries() {
        let ping = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n";
        let line = |n: usize, eol: &[u8], tail: &[u8]| {
            let mut v = vec![b'b'; n];
            v.extend(eol);
            v.extend(tail);
            run(&v)
        };
        let parse = err(json!(null), -32700, "Parse error");
        let big = err(
            json!(null),
            -32600,
            "Invalid request: line exceeds 1048576 bytes",
        );
        let nine = ok(json!(9), json!({}));
        for eol in [&b"\n"[..], &b"\r\n"[..]] {
            assert_eq!(
                line(MAX_LINE_BYTES, eol, ping),
                vec![parse.clone(), nine.clone()]
            );
            assert_eq!(
                line(MAX_LINE_BYTES + 1, eol, ping),
                vec![big.clone(), nine.clone()]
            );
        }
        assert_eq!(line(MAX_LINE_BYTES, b"", b""), vec![parse.clone()]);
        assert_eq!(line(MAX_LINE_BYTES, b"\r", b""), vec![parse.clone()]);
        assert_eq!(line(MAX_LINE_BYTES + 1, b"", b""), vec![big.clone()]);
    }

    #[test]
    fn ts003_panic_is_recovered_without_detail() {
        let input = lines(&[
            req(
                json!(1),
                "initialize",
                json!({"protocolVersion": "2025-11-25"}),
            ),
            req(json!(2), "tools/call", json!({"name": "boom"})),
            req(json!(3), "ping", json!({})),
        ]);
        let out = run(&input);
        assert_eq!(out[1], err(json!(2), -32603, "Internal error"));
        assert_eq!(out[2], ok(json!(3), json!({})));
        assert!(!out[1].to_string().contains("secret"));
    }

    #[test]
    fn ts003_empty_stdin_exits_zero_with_no_output() {
        assert_eq!(raw(b""), (0, Vec::new()));
        assert_eq!(raw(b"\n\r\n"), (0, Vec::new()));
    }

    #[test]
    fn ts003_notifications_are_never_answered() {
        let input = lines(&[
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}}),
            json!({"jsonrpc": "2.0", "method": "tools/list", "params": {"_meta": meta(json!("0000"))}}),
        ]);
        assert_eq!(raw(&input), (0, Vec::new()));
    }
}
