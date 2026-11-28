//! The monitor HTTP layer (FN-033, FN-034; IF-018, ADR-032): raw `std::net`, one request per
//! connection, read-only. Nothing here writes to stdout, and no request is logged.

use crate::telemetry::query::{self, Cache};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// CFG-010.
const MAX_CONNECTIONS: usize = 16;
const HEAD_DEADLINE: Duration = Duration::from_secs(5);
const MAX_HEAD: usize = 8 * 1024;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const DRAIN_BYTES: usize = 1024 * 1024;
const DRAIN_TIME: Duration = Duration::from_secs(1);

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";

const INDEX: &str = include_str!("assets/index.html");
const APP_JS: &str = include_str!("assets/app.js");
const APP_CSS: &str = include_str!("assets/app.css");

pub struct Response {
    status: u16,
    content_type: &'static str,
    body: String,
    allow: bool,
    www_authenticate: bool,
    /// Read and discard the unread request bytes (bounded) so the reply is delivered.
    /// Runs for every error reply except 408/503 (wider than IF-018/CFG-010's refused-method
    /// case); measured bound is about 1 MiB or 1 s, whichever comes first (DRAIN_BYTES/DRAIN_TIME).
    drain: bool,
}

impl Response {
    fn ok(content_type: &'static str, body: String) -> Response {
        Response {
            status: 200,
            content_type,
            body,
            allow: false,
            www_authenticate: false,
            drain: false,
        }
    }

    fn err(status: u16, code: &str) -> Response {
        Response {
            status,
            content_type: "application/json",
            body: format!("{{\"error\":\"{code}\"}}"),
            allow: status == 405,
            www_authenticate: status == 401,
            drain: !matches!(status, 408 | 503),
        }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

fn write_response(s: &mut TcpStream, r: &Response) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nCross-Origin-Resource-Policy: same-origin\r\nContent-Security-Policy: {CSP}\r\n",
        r.status,
        reason(r.status),
        r.content_type,
        r.body.len()
    );
    if r.allow {
        head.push_str("Allow: GET\r\n");
    }
    if r.www_authenticate {
        head.push_str("WWW-Authenticate: Bearer\r\n");
    }
    head.push_str("\r\n");
    head.push_str(&r.body);
    let _ = s.write_all(head.as_bytes());
    let _ = s.flush();
}

/// Constant-time token comparison: lengths first (public), then an XOR fold of every byte.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut fold = 0u8;
    for (x, y) in a.iter().zip(b) {
        fold |= std::hint::black_box(x ^ y);
    }
    std::hint::black_box(fold) == 0
}

/// Panic guard (ADR-032 row 14): a panicking handler answers 500 `internal_error`.
pub fn guarded<F: FnOnce() -> Response>(f: F) -> Response {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Response::err(500, "internal_error"))
}

/// Releases the connection slot when dropped, also on a panic.
struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Ctx {
    root: PathBuf,
    token: String,
    port: u16,
    cache: Cache,
}

/// Accept loop. Never returns; the process ends on a signal.
pub fn serve(listener: TcpListener, root: PathBuf, token: String) -> ! {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let ctx = Arc::new(Ctx {
        root,
        token,
        port,
        cache: Cache::new(),
    });
    let slots = Arc::new(AtomicUsize::new(0));
    loop {
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
        };
        if slots.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            slots.fetch_sub(1, Ordering::SeqCst);
            let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
            write_response(&mut stream, &Response::err(503, "busy"));
            continue;
        }
        let slot = Slot(slots.clone());
        let ctx = ctx.clone();
        // A failed spawn drops the closure, and with it the slot.
        let _ = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            handle(stream, &ctx);
        });
    }
}

fn handle(mut s: TcpStream, ctx: &Ctx) {
    let _ = s.set_write_timeout(Some(WRITE_TIMEOUT));
    let resp = match guarded(|| process(&mut s, ctx)) {
        r if r.status == 0 => return,
        r => r,
    };
    write_response(&mut s, &resp);
    if resp.drain {
        let _ = s.shutdown(Shutdown::Write);
        drain(&mut s);
    }
}

fn drain(s: &mut TcpStream) {
    let start = Instant::now();
    let mut left = DRAIN_BYTES;
    let mut buf = [0u8; 16 * 1024];
    while left > 0 {
        let Some(rest) = DRAIN_TIME.checked_sub(start.elapsed()) else {
            break;
        };
        if s.set_read_timeout(Some(rest.max(Duration::from_millis(1))))
            .is_err()
        {
            break;
        }
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => left = left.saturating_sub(n),
        }
    }
}

fn is_timeout(e: &std::io::Error) -> bool {
    // Windows reports a read timeout as TimedOut, Unix as WouldBlock.
    matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
}

enum Head {
    Complete(Vec<u8>),
    Timeout,
    TooLarge,
    Closed,
}

fn read_head(s: &mut TcpStream) -> Head {
    let start = Instant::now();
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return if i + 4 > MAX_HEAD {
                Head::TooLarge
            } else {
                buf.truncate(i);
                Head::Complete(buf)
            };
        }
        if buf.len() >= MAX_HEAD + 3 {
            return Head::TooLarge;
        }
        let Some(rest) = HEAD_DEADLINE.checked_sub(start.elapsed()) else {
            return Head::Timeout;
        };
        if s.set_read_timeout(Some(rest.max(Duration::from_millis(1))))
            .is_err()
        {
            return Head::Closed;
        }
        match s.read(&mut chunk) {
            Ok(0) => return Head::Closed,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if is_timeout(&e) => return Head::Timeout,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return Head::Closed,
        }
    }
}

/// Status 0 means "close without a reply" (the peer went away before a full head).
fn silent() -> Response {
    let mut r = Response::err(500, "internal_error");
    r.status = 0;
    r
}

fn bad() -> Response {
    Response::err(400, "bad_request")
}

fn process(s: &mut TcpStream, ctx: &Ctx) -> Response {
    let head = match read_head(s) {
        Head::Complete(h) => h,
        Head::Timeout => return Response::err(408, "request_timeout"),
        Head::TooLarge => return Response::err(431, "header_too_large"),
        Head::Closed => return silent(),
    };
    route(&head, ctx)
}

fn route(head: &[u8], ctx: &Ctx) -> Response {
    // Row 3: ASCII only, no control bytes (CR and LF only as the CRLF line ends).
    if head
        .iter()
        .any(|&b| b >= 0x80 || b == 0x7f || (b < 0x20 && !matches!(b, b'\r' | b'\n' | b'\t')))
    {
        return bad();
    }
    let Ok(text) = std::str::from_utf8(head) else {
        return bad();
    };
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = request_line.split(' ').collect();
    let [method, target, version] = parts[..] else {
        return bad();
    };
    if method.is_empty()
        || !method
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        || !target.starts_with('/')
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || request_line.contains(['\r', '\n'])
    {
        return bad();
    }
    let mut headers: Vec<(String, &str)> = Vec::new();
    for line in lines {
        if line.contains(['\r', '\n']) {
            return bad();
        }
        let Some((name, value)) = line.split_once(':') else {
            return bad();
        };
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_graphic() && b != b':') {
            return bad();
        }
        // RFC 9110 OWS (space/tab) around a field value is stripped, so a trailing space
        // after the bearer token is accepted; the token itself is compared exactly (IF-018).
        headers.push((
            name.to_ascii_lowercase(),
            value.trim_matches(|c| c == ' ' || c == '\t'),
        ));
    }
    // Row 4: only GET; the body is never read (the drain delivers the reply).
    if method != "GET" {
        return Response::err(405, "method_not_allowed");
    }
    let all = |name: &str| -> Vec<&str> {
        headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| *v)
            .collect()
    };
    // Row 5.
    let hosts = all("host");
    if hosts.len() > 1
        || all("content-length").iter().any(|v| *v != "0")
        || !all("transfer-encoding").is_empty()
    {
        return bad();
    }
    // Row 6.
    if hosts.first().copied() != Some(format!("127.0.0.1:{}", ctx.port).as_str()) {
        return Response::err(421, "host_rejected");
    }
    // Row 7.
    let origin = format!("http://127.0.0.1:{}", ctx.port);
    if all("origin").iter().any(|v| *v != origin)
        || all("sec-fetch-site")
            .iter()
            .any(|v| !matches!(*v, "same-origin" | "none"))
    {
        return Response::err(403, "origin_rejected");
    }
    // Row 8: exact raw target, no percent-decoding, query only on /api/events.
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (target, None),
    };
    match (path, query) {
        ("/", None) => return Response::ok("text/html; charset=utf-8", INDEX.to_owned()),
        ("/app.js", None) => {
            return Response::ok("text/javascript; charset=utf-8", APP_JS.to_owned())
        }
        ("/app.css", None) => return Response::ok("text/css; charset=utf-8", APP_CSS.to_owned()),
        ("/api/events", _) => {}
        _ => return Response::err(404, "not_found"),
    }
    // Row 9: exactly one `Authorization: Bearer <token>`.
    let auth = all("authorization");
    let presented = match auth[..] {
        [one] => one.strip_prefix("Bearer "),
        _ => None,
    };
    if !presented.is_some_and(|p| ct_eq(p.as_bytes(), ctx.token.as_bytes())) {
        return Response::err(401, "unauthorized");
    }
    // Rows 10 to 13 come from the query engine.
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    match query::query(&ctx.root, query.unwrap_or(""), now_ms, &ctx.cache) {
        Ok(v) => Response::ok("application/json", v.to_string()),
        Err(e) => Response::err(e.status, e.code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts029_ct_eq_equal_short_and_one_bit() {
        let t = b"0123456789abcdef0123456789abcdef";
        assert!(ct_eq(t, t));
        assert!(ct_eq(b"", b""));
        assert!(!ct_eq(t, &t[..31]));
        assert!(!ct_eq(&t[..31], t));
        assert!(!ct_eq(t, b""));
        for i in 0..t.len() {
            for bit in 0..8 {
                let mut u = *t;
                u[i] ^= 1 << bit;
                assert!(!ct_eq(t, &u), "byte {i} bit {bit}");
            }
        }
    }

    #[test]
    fn ts029_a_handler_panic_answers_500_internal_error() {
        let r = guarded(|| panic!("boom"));
        assert_eq!(r.status, 500);
        assert_eq!(r.body, "{\"error\":\"internal_error\"}");
        assert_eq!(r.content_type, "application/json");
        // A normal handler passes through untouched.
        assert_eq!(guarded(|| Response::err(404, "not_found")).status, 404);
    }

    #[test]
    fn ts029_the_slot_is_released_when_a_handler_panics() {
        let n = Arc::new(AtomicUsize::new(1));
        let slot = Slot(n.clone());
        let r = std::thread::spawn(move || {
            let _slot = slot;
            panic!("boom");
        })
        .join();
        assert!(r.is_err());
        assert_eq!(n.load(Ordering::SeqCst), 0);
    }
}
