//! TS-030: static checks over the three embedded dashboard assets (FN-036; AC-011, AC-019,
//! AC-020, AC-021) plus an HTTP check through the real monitor. The browser pass (320 px, 360 px
//! with an error state, 1280 px, keyboard flow, every state) is a manual checklist: no browser
//! runs here.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn asset(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("rust-cli/monitor/assets")
        .join(name);
    fs::read_to_string(p).unwrap()
}

fn all() -> [(&'static str, String); 3] {
    [
        ("index.html", asset("index.html")),
        ("app.js", asset("app.js")),
        ("app.css", asset("app.css")),
    ]
}

#[test]
fn ts030_forbidden_apis_are_absent() {
    let forbidden = [
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "eval(",
        "new Function",
        "localStorage",
        "sessionStorage",
        "document.cookie",
        "indexedDB",
        "setAttribute('style'",
        "setAttribute(\"style\"",
        ".style.",
        "cssText",
        "XMLHttpRequest",
        "WebSocket",
        "sendBeacon",
        "pushState",
        "serviceWorker",
    ];
    for (name, text) in all() {
        for f in forbidden {
            assert!(!text.contains(f), "{name} contains forbidden {f:?}");
        }
    }
}

#[test]
fn ts030_no_remote_urls_no_inline_script_or_style() {
    for (name, text) in all() {
        assert!(!text.contains("http://"), "{name}: http://");
        assert!(!text.contains("https://"), "{name}: https://");
        assert!(!text.contains("@import"), "{name}: @import");
    }
    let html = asset("index.html");
    assert!(!html.contains(" style="), "inline style attribute");
    assert!(!html.contains("<style"), "inline style element");
    for attr in [
        "onclick=",
        "onload=",
        "onerror=",
        "onsubmit=",
        "javascript:",
    ] {
        assert!(!html.contains(attr), "inline handler {attr}");
    }
    let mut scripts = 0;
    for part in html.split("<script").skip(1) {
        scripts += 1;
        let (open, rest) = part.split_once('>').unwrap();
        assert!(
            open.contains("src=\"/app.js\""),
            "script without src: {open}"
        );
        assert!(rest.starts_with("</script>"), "inline script body");
    }
    assert_eq!(scripts, 1, "exactly one script element");
    assert!(html.contains("href=\"/app.css\""));
    assert!(html.contains("name=\"viewport\""));
}

#[test]
fn ts030_token_handling_and_request_shape() {
    let js = asset("app.js");
    assert!(js.contains("history.replaceState(null, '', '/')"));
    assert!(js.contains("location.hash"));
    assert!(js.contains("Authorization"));
    assert!(js.contains("'Bearer '"));
    assert!(js.contains("method: 'GET'"));
    assert!(js.contains("AbortController"));
    assert!(js.contains("/api/events"));
    // The token lives in a closure, not a top-level binding of a classic script.
    assert!(js.trim_start().starts_with("(function"), "IIFE wrapper");
    assert!(js.contains("textContent"));
    assert!(
        js.contains("'submit'"),
        "Apply goes through a submit listener"
    );
    assert!(js.contains("preventDefault"));
    assert!(
        js.find("history.replaceState").unwrap() < js.find("fetch(").unwrap(),
        "token is scrubbed from the URL before any fetch"
    );
    assert!(!js.contains("console."), "no console output");
}

#[test]
fn ts030_review_fixes_retry_hint_banner_and_columns() {
    let js = asset("app.js");
    // Retries must fire even after Load older paused auto-refresh.
    let at = js.find("st.retryAt = Date.now() + st.backoff").unwrap();
    let pre = &js[at - 120..at];
    assert!(pre.contains("st.paused = false"), "retry branch unpauses");
    // aria-describedby only points at hints that exist.
    assert!(
        js.contains("$('f-' + k + '-hint')"),
        "setErr checks the hint element exists"
    );
    // Container divs must not carry class=banner (children already do).
    let html = asset("index.html");
    assert!(!html.contains("id=\"banner\" class=\"banner\""));
    assert!(!html.contains("id=\"empty\" class=\"banner\""));
    // The event column flexes; the outcome column is a fixed track.
    let css = asset("app.css");
    assert!(css.contains("5.5rem 3rem minmax(0, 1fr) 5.5rem 4.5rem 6.5rem"));
}

#[test]
fn ts030_privacy_text_drops_line_and_generic_label() {
    let html = asset("index.html");
    for needle in [
        "Dropped events: not recorded",
        "Setting: unknown",
        ".aidlc/telemetry/",
        "inside the local project",
        "The command-line tool (CLI) writes no telemetry",
        "this monitor never deletes anything",
        "Never kept:",
        "The intent id is made from the first words of the intent text you typed; do not put names or secrets in intent text.",
        "Fields kept per event:",
        "telemetry is not audit",
        "stores nothing in your browser",
        "local project",
        "MCP process: unknown",
        "read-only",
    ] {
        assert!(html.contains(needle), "index.html lacks {needle:?}");
    }
    let js = asset("app.js");
    assert!(js.contains("Dropped events: not recorded"), "coverage line");
    assert!(js.contains("Token missing"));
    assert!(js.contains("Token rejected"));
    assert!(html.contains("Gate-decided events"));
    assert!(js.contains("no approve/reject value"));
    // The stage filter matches only workflow events (WU-017 contract).
    assert!(html.contains("workflow events only"));
    // No absolute path or username is ever rendered.
    for (name, text) in all() {
        assert!(!text.contains("/Users/"), "{name}");
        assert!(!text.contains("/home/"), "{name}");
    }
}

#[test]
fn ts030_narrow_width_and_accessibility_css() {
    let css = asset("app.css");
    for needle in [
        "@media (min-width: 60rem)",
        "@media (prefers-color-scheme: dark)",
        "@media (prefers-reduced-motion: reduce)",
        "@media (forced-colors: active)",
        ":focus-visible",
        "overflow-wrap: anywhere",
        "min-width: 0",
        "font-variant-numeric: tabular-nums",
        "--accent: #006b66",
        "--accent: #53d3c2",
        "min-height: 44px",
        "[hidden]",
    ] {
        assert!(css.contains(needle), "app.css lacks {needle:?}");
    }
    assert!(!css.contains("outline: none"), "focus ring must stay");
    assert!(!css.contains("min-width: 320px"));
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

fn get(port: u16, path: &str) -> Reply {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").expect("response head");
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

#[test]
fn ts030_monitor_serves_the_assets_with_the_csp() {
    let dir =
        PathBuf::from(std::env::var_os("TMPDIR").unwrap_or_else(|| std::env::temp_dir().into()))
            .join(format!("monitor-assets-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .arg("--project")
        .arg(&dir)
        .arg("monitor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut err = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    err.read_line(&mut line).unwrap();
    let port: Option<u16> = line
        .strip_prefix("monitor: http://127.0.0.1:")
        .and_then(|r| r.split_once("/#token="))
        .map(|(p, _)| p.parse().unwrap());
    let result = std::panic::catch_unwind(|| {
        let port = port.unwrap_or_else(|| panic!("startup line {line:?}"));
        for (path, file, ct) in [
            ("/", "index.html", "text/html; charset=utf-8"),
            ("/app.js", "app.js", "text/javascript; charset=utf-8"),
            ("/app.css", "app.css", "text/css; charset=utf-8"),
        ] {
            let r = get(port, path);
            assert_eq!(r.status, 200, "{path}");
            assert_eq!(r.body, asset(file), "{path} body");
            let h = |k: &str| {
                r.headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(k))
                    .map(|(_, v)| v.as_str())
            };
            assert_eq!(h("Content-Type"), Some(ct), "{path}");
            let csp = h("Content-Security-Policy").expect("CSP");
            assert!(csp.contains("script-src 'self'") && csp.contains("style-src 'self'"));
            assert!(csp.contains("default-src 'none'") && csp.contains("connect-src 'self'"));
            assert!(csp.contains("require-trusted-types-for 'script'; trusted-types 'none'"));
            assert!(r
                .headers
                .iter()
                .all(|(k, _)| !k.starts_with("Access-Control-")));
        }
        // The page itself carries no token and no data.
        assert!(!get(port, "/").body.contains("token="));
    });
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(&dir);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
