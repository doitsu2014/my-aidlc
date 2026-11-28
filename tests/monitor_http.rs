//! TS-029: the read-only loopback monitor over raw `TcpStream` (FN-033, FN-034; IF-018, IF-019,
//! CFG-010; ADR-032 rows 0..13). Row 14 (500 `internal_error`) is the panic-guard unit test in
//! `rust-cli/monitor/http.rs`. Every spawn passes `--project <Tmp>`.

use dtech_aidlc::telemetry::{file_name, Event, EventKind, Outcome};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        let p = PathBuf::from(
            std::env::var_os("TMPDIR").unwrap_or_else(|| std::env::temp_dir().into()),
        )
        .join(format!("monitor-http-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Monitor {
    child: Child,
    port: u16,
    token: String,
    first_line: String,
    stderr: BufReader<std::process::ChildStderr>,
}
impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn(project: &Path) -> Monitor {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .arg("--project")
        .arg(project)
        .arg("monitor")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    stderr.read_line(&mut line).unwrap();
    let line = line.trim_end().to_owned();
    let rest = match line.strip_prefix("monitor: http://127.0.0.1:") {
        Some(r) => r.to_owned(),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("no startup line, got {line:?}");
        }
    };
    let (port, rest) = rest.split_once("/#token=").expect("fragment token");
    let token = rest.split("  ").next().unwrap().to_owned();
    Monitor {
        child,
        port: port.parse().unwrap(),
        token,
        first_line: line,
        stderr,
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}
impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_reply(s: &mut TcpStream) -> Reply {
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response head, got {text:?}"));
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .map(|l| {
            let (k, v) = l.split_once(": ").unwrap();
            (k.to_owned(), v.to_owned())
        })
        .collect();
    Reply {
        status,
        headers,
        body: body.to_owned(),
    }
}

fn raw(m: &Monitor, bytes: &[u8]) -> Reply {
    let mut s = TcpStream::connect(("127.0.0.1", m.port)).unwrap();
    s.write_all(bytes).unwrap();
    read_reply(&mut s)
}

fn get_with(m: &Monitor, target: &str, extra: &str) -> Reply {
    let req = format!(
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{extra}\r\n",
        m.port
    );
    raw(m, req.as_bytes())
}

fn api(m: &Monitor, target: &str) -> Reply {
    get_with(m, target, &format!("Authorization: Bearer {}\r\n", m.token))
}

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";

fn assert_fixed_headers(r: &Reply, ctx: &str) {
    for (k, v) in [
        ("Connection", "close"),
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("Content-Security-Policy", CSP),
    ] {
        assert_eq!(r.header(k), Some(v), "{ctx}: {k}");
    }
    assert_eq!(
        r.header("Content-Length")
            .map(|v| v.parse::<usize>().unwrap()),
        Some(r.body.len()),
        "{ctx}: Content-Length"
    );
    assert!(r.header("Content-Type").is_some(), "{ctx}: Content-Type");
    assert!(
        r.headers
            .iter()
            .all(|(k, _)| !k.to_ascii_lowercase().starts_with("access-control-")),
        "{ctx}: Access-Control-*"
    );
}

fn assert_err(r: &Reply, status: u16, code: &str, ctx: &str) {
    assert_eq!(r.status, status, "{ctx}");
    assert_eq!(r.body, format!("{{\"error\":\"{code}\"}}"), "{ctx}");
    assert!(r
        .header("Content-Type")
        .unwrap()
        .starts_with("application/json"));
    assert_fixed_headers(r, ctx);
}

#[test]
fn ts029_startup_line_is_one_stderr_line_and_stdout_stays_empty() {
    let project = Tmp::new("startup");
    let mut m = spawn(&project.0);
    assert_eq!(m.token.len(), 32, "{}", m.first_line);
    assert!(m
        .token
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    assert!(
        m.first_line.ends_with("  (Ctrl+C to stop)"),
        "{}",
        m.first_line
    );
    assert!(
        !project.0.join(".aidlc").exists(),
        "monitor must create nothing"
    );
    // Serve a request, then stop: stdout is empty and the token is nowhere else.
    assert_eq!(api(&m, "/api/events").status, 200);
    let _ = m.child.kill();
    let mut out = String::new();
    m.child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    assert!(out.is_empty(), "stdout: {out:?}");
    let mut err = String::new();
    m.stderr.read_to_string(&mut err).unwrap();
    assert!(
        !err.contains(&m.token),
        "token leaked after the startup line"
    );
    assert_eq!(err.trim(), "", "nothing else on stderr");
    // The monitor never exits 0 on its own: a kill is the only stop.
    assert!(!m.child.wait().unwrap().success());
}

#[test]
fn ts029_two_monitors_get_distinct_ports_and_tokens() {
    let project = Tmp::new("two");
    let a = spawn(&project.0);
    let b = spawn(&project.0);
    assert_ne!(a.port, b.port);
    assert_ne!(a.token, b.token);
    assert_eq!(api(&a, "/api/events").status, 200);
    let r = get_with(
        &a,
        "/api/events",
        &format!("Authorization: Bearer {}\r\n", b.token),
    );
    assert_eq!(r.status, 401);
}

#[test]
fn ts029_static_routes_need_no_token_and_carry_the_fixed_headers() {
    let project = Tmp::new("static");
    let m = spawn(&project.0);
    for (path, ctype) in [
        ("/", "text/html; charset=utf-8"),
        ("/app.js", "text/javascript; charset=utf-8"),
        ("/app.css", "text/css; charset=utf-8"),
    ] {
        let r = get_with(&m, path, "");
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("Content-Type"), Some(ctype), "{path}");
        assert!(!r.body.is_empty(), "{path}");
        assert_fixed_headers(&r, path);
    }
    // HTTP/1.0 is accepted too.
    let r = raw(
        &m,
        format!("GET / HTTP/1.0\r\nHost: 127.0.0.1:{}\r\n\r\n", m.port).as_bytes(),
    );
    assert_eq!(r.status, 200);
    // The page is CSP-compatible: no inline style, no remote URL, scripts only by src.
    let page = get_with(&m, "/", "").body;
    assert!(
        !page.contains("<style") && !page.contains("style="),
        "{page}"
    );
    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "{page}"
    );
    assert!(page
        .lines()
        .all(|l| !l.contains("<script") || l.contains("src=")));
}

#[test]
fn ts029_api_needs_the_exact_bearer_token() {
    let project = Tmp::new("auth");
    let m = spawn(&project.0);
    let t = m.token.clone();
    let upper = t.to_ascii_uppercase();
    let flipped = format!("{}{}", &t[..31], if t.ends_with('0') { '1' } else { '0' });
    let cases: Vec<(&str, String, String)> = vec![
        ("missing", "/api/events".into(), String::new()),
        (
            "wrong",
            "/api/events".into(),
            format!("Authorization: Bearer {}\r\n", "0".repeat(32)),
        ),
        (
            "short",
            "/api/events".into(),
            format!("Authorization: Bearer {}\r\n", &t[..31]),
        ),
        (
            "long",
            "/api/events".into(),
            format!("Authorization: Bearer {t}0\r\n"),
        ),
        (
            "one-bit",
            "/api/events".into(),
            format!("Authorization: Bearer {flipped}\r\n"),
        ),
        (
            "case-folded token",
            "/api/events".into(),
            format!("Authorization: Bearer {upper}\r\n"),
        ),
        (
            "lowercase scheme",
            "/api/events".into(),
            format!("Authorization: bearer {t}\r\n"),
        ),
        (
            "no scheme",
            "/api/events".into(),
            format!("Authorization: {t}\r\n"),
        ),
        (
            "two spaces",
            "/api/events".into(),
            format!("Authorization: Bearer  {t}\r\n"),
        ),
        (
            "basic",
            "/api/events".into(),
            format!("Authorization: Basic {t}\r\n"),
        ),
        (
            "duplicate",
            "/api/events".into(),
            format!("Authorization: Bearer {t}\r\nAuthorization: Bearer {t}\r\n"),
        ),
        (
            "token in query only",
            format!("/api/events?token={t}"),
            String::new(),
        ),
    ];
    for (name, target, extra) in cases {
        let r = get_with(&m, &target, &extra);
        assert_err(&r, 401, "unauthorized", name);
        assert_eq!(r.header("WWW-Authenticate"), Some("Bearer"), "{name}");
    }
    // RFC 9110 OWS around a field value is stripped; the token itself is compared exactly.
    for ws in [" ", "\t", " \t "] {
        let r = get_with(
            &m,
            "/api/events",
            &format!("Authorization: Bearer {t}{ws}\r\n"),
        );
        assert_eq!(r.status, 200, "trailing OWS {ws:?}");
    }
    // Header name is case-insensitive.
    let ok = get_with(&m, "/api/events", &format!("authorization: Bearer {t}\r\n"));
    assert_eq!(ok.status, 200);
    assert_fixed_headers(&ok, "200 api");
    assert!(ok
        .header("Content-Type")
        .unwrap()
        .starts_with("application/json"));
    let v: serde_json::Value = serde_json::from_str(&ok.body).unwrap();
    assert_eq!(v["schemaVersion"], 1);
    assert!(v["events"].as_array().unwrap().is_empty());
}

#[test]
fn ts029_api_returns_stored_events() {
    let project = Tmp::new("events");
    let dir = project.0.join(".aidlc/telemetry");
    fs::create_dir_all(&dir).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let sid = "0123456789abcdef0123456789abcdef";
    let ev = Event {
        kind: EventKind::ToolCompleted,
        event_id: format!("{sid}-1"),
        timestamp: dtech_aidlc::audit::format_iso_millis(now - 1000),
        process_id: 7,
        session_id: sid.into(),
        trace_id: format!("{sid}-0"),
        span_id: format!("{sid}-0"),
        parent_span_id: None,
        name: "status".into(),
        intent_id: None,
        stage_id: Some("develop".into()),
        unit_id: None,
        duration_ms: Some(5),
        outcome: Some(Outcome::Ok),
        error_code: None,
    };
    fs::write(
        dir.join(file_name(now - 5000, 7, sid, 1)),
        format!("{}\n", ev.to_line()),
    )
    .unwrap();
    let m = spawn(&project.0);
    let v: serde_json::Value = serde_json::from_str(&api(&m, "/api/events").body).unwrap();
    assert_eq!(v["events"].as_array().unwrap().len(), 1, "{v}");
    let v: serde_json::Value =
        serde_json::from_str(&api(&m, "/api/events?name=queue&limit=5&range=900").body).unwrap();
    assert!(v["events"].as_array().unwrap().is_empty());
    // The monitor only reads: the store is unchanged.
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn ts029_filter_errors_map_to_their_http_status() {
    let project = Tmp::new("filter");
    let m = spawn(&project.0);
    for q in [
        "bogus=1",
        "range=abc",
        "range=0",
        "limit=0",
        "limit=201",
        "path=x",
        "file=a",
        "a=%41",
        "range=60&range=60",
        "outcome=nope",
        "name",
        "component=",
    ] {
        assert_err(
            &api(&m, &format!("/api/events?{q}")),
            400,
            "invalid_filter",
            q,
        );
    }
    assert_err(
        &api(&m, "/api/events?range=604801"),
        400,
        "range_too_large",
        "range",
    );
    assert_err(
        &api(&m, "/api/events?cursor=zz"),
        409,
        "cursor_stale",
        "cursor",
    );
    assert_eq!(api(&m, "/api/events?range=604800&limit=200").status, 200);
    assert_eq!(api(&m, "/api/events?").status, 200);
}

#[cfg(unix)]
#[test]
fn ts029_store_unavailable_when_the_telemetry_dir_turns_unsafe() {
    let project = Tmp::new("unavail");
    fs::create_dir_all(project.0.join(".aidlc")).unwrap();
    let m = spawn(&project.0);
    assert_eq!(api(&m, "/api/events").status, 200);
    let other = project.0.join("elsewhere");
    fs::create_dir_all(&other).unwrap();
    std::os::unix::fs::symlink(&other, project.0.join(".aidlc/telemetry")).unwrap();
    let r = api(&m, "/api/events");
    assert_err(&r, 503, "store_unavailable", "symlinked store");
    assert!(!r.body.contains("elsewhere"));
}

#[test]
fn ts029_start_refuses_an_unsafe_telemetry_path_without_naming_it() {
    let project = Tmp::new("unsafe");
    fs::create_dir_all(project.0.join(".aidlc")).unwrap();
    fs::write(project.0.join(".aidlc/telemetry"), "x").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .arg("--project")
        .arg(&project.0)
        .arg("monitor")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Refusing unsafe .aidlc/telemetry path"),
        "{err}"
    );
    assert!(!err.contains(project.0.to_str().unwrap()), "{err}");
}

#[test]
fn ts029_unknown_targets_are_404_and_traversal_never_reads_files() {
    let project = Tmp::new("paths");
    let m = spawn(&project.0);
    for target in [
        "/nope",
        "/index.html",
        "/app.js?x=1",
        "/?x=1",
        "/app.css?",
        "/api/events/",
        "/api/other",
        "/api",
        "/../etc/passwd",
        "/..%2f..%2fetc/passwd",
        "/%2e%2e/etc/passwd",
        "/app.js/..",
        "//",
        "/./app.js",
        "/app.js%00",
        "/favicon.ico",
        "/Cargo.toml",
        "/APP.JS",
        "/api/events%3Fx",
    ] {
        // No token on purpose: unknown targets are 404 before authentication.
        assert_err(&get_with(&m, target, ""), 404, "not_found", target);
    }
}

#[test]
fn ts029_malformed_requests_are_400_bad_request() {
    let project = Tmp::new("bad");
    let m = spawn(&project.0);
    let host = format!("Host: 127.0.0.1:{}\r\n", m.port);
    let bad: Vec<(&str, Vec<u8>)> = vec![
        ("no version", format!("GET /\r\n{host}\r\n").into_bytes()),
        (
            "two spaces",
            format!("GET  / HTTP/1.1\r\n{host}\r\n").into_bytes(),
        ),
        (
            "extra token",
            format!("GET / HTTP/1.1 x\r\n{host}\r\n").into_bytes(),
        ),
        (
            "asterisk target",
            format!("GET * HTTP/1.1\r\n{host}\r\n").into_bytes(),
        ),
        (
            "absolute target",
            format!("GET http://127.0.0.1/ HTTP/1.1\r\n{host}\r\n").into_bytes(),
        ),
        (
            "http/2",
            format!("GET / HTTP/2.0\r\n{host}\r\n").into_bytes(),
        ),
        (
            "http/0.9",
            format!("GET / HTTP/0.9\r\n{host}\r\n").into_bytes(),
        ),
        (
            "lowercase http",
            format!("GET / http/1.1\r\n{host}\r\n").into_bytes(),
        ),
        (
            "control byte",
            format!("GET /\x01 HTTP/1.1\r\n{host}\r\n").into_bytes(),
        ),
        (
            "nul in header",
            format!("GET / HTTP/1.1\r\n{host}X: a\0b\r\n\r\n").into_bytes(),
        ),
        (
            "non-ascii",
            format!("GET /\u{e9} HTTP/1.1\r\n{host}\r\n").into_bytes(),
        ),
        (
            "header without colon",
            format!("GET / HTTP/1.1\r\n{host}Oops\r\n\r\n").into_bytes(),
        ),
        (
            "duplicate host",
            format!("GET / HTTP/1.1\r\n{host}{host}\r\n").into_bytes(),
        ),
        (
            "content-length",
            format!("GET / HTTP/1.1\r\n{host}Content-Length: 5\r\n\r\nhello").into_bytes(),
        ),
        (
            "transfer-encoding",
            format!("GET / HTTP/1.1\r\n{host}Transfer-Encoding: chunked\r\n\r\n0\r\n\r\n")
                .into_bytes(),
        ),
        (
            "transfer-encoding identity",
            format!("GET / HTTP/1.1\r\n{host}Transfer-Encoding: identity\r\n\r\n").into_bytes(),
        ),
    ];
    for (name, bytes) in bad {
        assert_err(&raw(&m, &bytes), 400, "bad_request", name);
    }
    // Content-Length: 0 is fine.
    assert_eq!(get_with(&m, "/", "Content-Length: 0\r\n").status, 200);
}

#[test]
fn ts029_only_get_is_allowed_and_the_method_is_checked_before_the_host() {
    let project = Tmp::new("methods");
    let m = spawn(&project.0);
    for method in [
        "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "CONNECT", "get", "M-SEARCH",
    ] {
        let req = format!(
            "{method} /api/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
            m.port
        );
        let r = raw(&m, req.as_bytes());
        assert_err(&r, 405, "method_not_allowed", method);
        assert_eq!(r.header("Allow"), Some("GET"), "{method}");
    }
    // A CORS preflight is refused with no CORS headers, even from a foreign origin and host.
    let pre = "OPTIONS /api/events HTTP/1.1\r\nHost: evil.example\r\nOrigin: http://evil.example\r\nAccess-Control-Request-Method: GET\r\nAccess-Control-Request-Headers: authorization\r\n\r\n";
    assert_err(
        &raw(&m, pre.as_bytes()),
        405,
        "method_not_allowed",
        "preflight",
    );
    // Method is checked before body headers: a POST with a body still gets its 405.
    let req = format!(
        "POST / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 5\r\n\r\nhello",
        m.port
    );
    assert_err(
        &raw(&m, req.as_bytes()),
        405,
        "method_not_allowed",
        "POST body",
    );
}

#[test]
fn ts029_a_large_post_body_still_receives_its_405_and_the_drain_is_bounded() {
    let project = Tmp::new("drain");
    let m = spawn(&project.0);
    for (label, size) in [("2 MiB", 2usize << 20), ("64 MiB", 64usize << 20)] {
        let mut s = TcpStream::connect(("127.0.0.1", m.port)).unwrap();
        s.write_all(
            format!(
                "POST /api/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: {size}\r\n\r\n",
                m.port
            )
            .as_bytes(),
        )
        .unwrap();
        let mut w = s.try_clone().unwrap();
        let start = Instant::now();
        let writer = std::thread::spawn(move || {
            let chunk = vec![b'x'; 64 * 1024];
            let mut sent = 0usize;
            while sent < size {
                if w.write_all(&chunk).is_err() {
                    break;
                }
                sent += chunk.len();
            }
            sent
        });
        let r = read_reply(&mut s);
        let got_reply = start.elapsed();
        let sent = writer.join().unwrap();
        assert_err(&r, 405, "method_not_allowed", label);
        eprintln!(
            "drain measure: body {label}: 405 read after {got_reply:?}, client sent {sent} bytes, total {:?}",
            start.elapsed()
        );
        // The drain is bounded: the connection is over well within 1 s plus slack.
        assert!(start.elapsed() < Duration::from_secs(4), "{label}");
    }
}

#[test]
fn ts029_host_must_be_exactly_loopback_and_port() {
    let project = Tmp::new("host");
    let m = spawn(&project.0);
    let p = m.port;
    let other = if p == 1 { 2 } else { p - 1 };
    let hosts = [
        format!("Host: localhost:{p}\r\n"),
        "Host: 127.0.0.1\r\n".to_owned(),
        format!("Host: 127.0.0.1:{other}\r\n"),
        format!("Host: evil.example:{p}\r\n"),
        format!("Host: 127.0.0.1:{p}.evil.example\r\n"),
        format!("Host: 127.0.0.1:{p}x\r\n"),
        format!("Host: [::1]:{p}\r\n"),
        format!("Host: 0.0.0.0:{p}\r\n"),
        "Host: \r\n".to_owned(),
        String::new(),
    ];
    for h in hosts {
        let req = format!("GET / HTTP/1.1\r\n{h}\r\n");
        assert_err(&raw(&m, req.as_bytes()), 421, "host_rejected", &h);
    }
    // HTTP/1.0 without Host is rejected too.
    assert_err(
        &raw(&m, b"GET / HTTP/1.0\r\n\r\n"),
        421,
        "host_rejected",
        "1.0 no host",
    );
}

#[test]
fn ts029_origin_and_sec_fetch_site_are_exact() {
    let project = Tmp::new("origin");
    let m = spawn(&project.0);
    let p = m.port;
    for extra in [
        "Origin: http://evil.example\r\n".to_owned(),
        "Origin: null\r\n".to_owned(),
        format!("Origin: http://localhost:{p}\r\n"),
        format!("Origin: http://127.0.0.1:{p}/\r\n"),
        format!("Origin: https://127.0.0.1:{p}\r\n"),
        "Origin: \r\n".to_owned(),
        "Sec-Fetch-Site: cross-site\r\n".to_owned(),
        "Sec-Fetch-Site: same-site\r\n".to_owned(),
        "Sec-Fetch-Site: SAME-ORIGIN\r\n".to_owned(),
        format!("Origin: http://127.0.0.1:{p}\r\nSec-Fetch-Site: cross-site\r\n"),
    ] {
        assert_err(&get_with(&m, "/", &extra), 403, "origin_rejected", &extra);
        let with_token = format!("{extra}Authorization: Bearer {}\r\n", m.token);
        assert_err(
            &get_with(&m, "/api/events", &with_token),
            403,
            "origin_rejected",
            &extra,
        );
    }
    for extra in [
        format!("Origin: http://127.0.0.1:{p}\r\n"),
        "Sec-Fetch-Site: same-origin\r\n".to_owned(),
        "Sec-Fetch-Site: none\r\n".to_owned(),
        format!("Origin: http://127.0.0.1:{p}\r\nSec-Fetch-Site: same-origin\r\n"),
    ] {
        assert_eq!(get_with(&m, "/", &extra).status, 200, "{extra}");
    }
    // Origin is checked before the token: a foreign origin without a token is 403, not 401.
    assert_err(
        &get_with(&m, "/api/events", "Origin: http://evil.example\r\n"),
        403,
        "origin_rejected",
        "origin before token",
    );
    // Host is checked before Origin.
    let req = "GET / HTTP/1.1\r\nHost: evil.example\r\nOrigin: http://evil.example\r\n\r\n";
    assert_err(
        &raw(&m, req.as_bytes()),
        421,
        "host_rejected",
        "host before origin",
    );
}

#[test]
fn ts029_an_oversized_head_is_431() {
    let project = Tmp::new("big");
    let m = spawn(&project.0);
    let req = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Pad: {}\r\n\r\n",
        m.port,
        "a".repeat(9000)
    );
    assert_err(
        &raw(&m, req.as_bytes()),
        431,
        "header_too_large",
        "9000 pad",
    );
    // Never-terminated junk past the cap is 431 as well, without waiting for the deadline.
    let start = Instant::now();
    let r = raw(&m, "G".repeat(9000).as_bytes());
    assert_err(&r, 431, "header_too_large", "junk");
    assert!(start.elapsed() < Duration::from_secs(3));
    // A head just under the cap is served.
    let base = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Pad: \r\n\r\n",
        m.port
    )
    .len();
    let req = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Pad: {}\r\n\r\n",
        m.port,
        "a".repeat(8192 - base)
    );
    assert_eq!(req.len(), 8192);
    assert_eq!(raw(&m, req.as_bytes()).status, 200);
}

#[test]
fn ts029_a_slow_head_gets_408_after_about_five_seconds() {
    let project = Tmp::new("slow");
    let m = spawn(&project.0);
    let mut s = TcpStream::connect(("127.0.0.1", m.port)).unwrap();
    s.write_all(b"GET / HTT").unwrap();
    let start = Instant::now();
    // Drip a byte every second: activity does not extend the total deadline.
    let mut w = s.try_clone().unwrap();
    let dripper = std::thread::spawn(move || {
        for _ in 0..8 {
            std::thread::sleep(Duration::from_secs(1));
            if w.write_all(b"P").is_err() {
                break;
            }
        }
    });
    let r = read_reply(&mut s);
    let took = start.elapsed();
    let _ = dripper.join();
    assert_err(&r, 408, "request_timeout", "slowloris");
    eprintln!("head deadline measure: 408 after {took:?}");
    assert!(
        took >= Duration::from_millis(4500) && took < Duration::from_secs(7),
        "{took:?}"
    );
}

#[test]
fn ts029_the_seventeenth_connection_gets_503_busy_and_slots_are_released() {
    let project = Tmp::new("busy");
    let m = spawn(&project.0);
    let mut held: Vec<TcpStream> = (0..16)
        .map(|_| TcpStream::connect(("127.0.0.1", m.port)).unwrap())
        .collect();
    // Retry until the accept loop has admitted all sixteen and the 17th is refused (bounded, 5 s).
    let start = Instant::now();
    let (r, busy_after) = loop {
        let mut s = TcpStream::connect(("127.0.0.1", m.port)).unwrap();
        let t = Instant::now();
        // An admitted (not refused) probe stays silent; give up on it quickly and retry.
        s.set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        if s.peek(&mut [0u8; 1]).is_ok_and(|n| n > 0) {
            break (read_reply(&mut s), t.elapsed());
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "no 503 within 5 s"
        );
    };
    assert_err(&r, 503, "busy", "17th");
    eprintln!(
        "slot measure: 17th connection answered 503 busy after {busy_after:?} (nothing sent)"
    );
    assert!(busy_after < Duration::from_secs(2));
    // The idle ones time out (408) and free their slots; then service resumes.
    let start = Instant::now();
    let r = read_reply(&mut held[0]);
    assert_err(&r, 408, "request_timeout", "held idle");
    drop(held);
    let mut ok = false;
    for _ in 0..50 {
        if get_with(&m, "/", "").status == 200 {
            ok = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "slots must be released");
    eprintln!(
        "slot measure: idle slots freed by 408 after {:?}",
        start.elapsed()
    );
}

#[test]
fn ts029_a_closed_connection_frees_its_slot() {
    let project = Tmp::new("slots");
    let m = spawn(&project.0);
    let start = Instant::now();
    for round in 0..40 {
        // 40 rounds of 8 abandoned connections would exhaust 16 slots if any leaked.
        let held: Vec<TcpStream> = (0..8)
            .map(|_| TcpStream::connect(("127.0.0.1", m.port)).unwrap())
            .collect();
        drop(held);
        assert_eq!(get_with(&m, "/", "").status, 200, "round {round}");
    }
    eprintln!(
        "slot measure: 40 rounds of 8 abandoned connections ok in {:?}",
        start.elapsed()
    );
}

#[test]
fn ts029_no_response_ever_carries_access_control_headers() {
    let project = Tmp::new("cors");
    let m = spawn(&project.0);
    let p = m.port;
    let replies = [
        get_with(&m, "/", "Origin: http://evil.example\r\n"),
        get_with(&m, "/app.js", &format!("Origin: http://127.0.0.1:{p}\r\n")),
        api(&m, "/api/events"),
        get_with(&m, "/api/events", ""),
        raw(&m, b"OPTIONS / HTTP/1.1\r\nHost: x\r\n\r\n"),
        raw(&m, b"garbage\r\n\r\n"),
    ];
    for r in replies {
        assert_fixed_headers(&r, "cors sweep");
    }
}
