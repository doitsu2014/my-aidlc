//! Query engine for the monitor (FN-032, IF-017, CFG-011). Read-only: it never writes, creates
//! or deletes anything under the project root.

use super::{hex, is_store_file, parse_line, store_state, Event, EventKind, Outcome, StoreState};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Read caps (CFG-011). Injected so unit tests can reach them with small data.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub max_files: usize,
    pub max_bytes: u64,
    pub max_events: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps {
            max_files: 512,
            max_bytes: 64 * 1024 * 1024,
            max_events: 250_000,
        }
    }
}

/// Parsed content of one store file plus its tolerance counts.
pub struct FileData {
    pub events: Vec<Event>,
    pub skipped: u64,
    pub partial_tail: bool,
}

type CacheMap = HashMap<PathBuf, (u64, SystemTime, Arc<FileData>)>;

/// Parse cache keyed by (path, length, mtime). In memory only, never persisted.
#[derive(Default)]
pub struct Cache(Mutex<CacheMap>);

impl Cache {
    pub fn new() -> Cache {
        Cache::default()
    }
    fn map(&self) -> std::sync::MutexGuard<'_, CacheMap> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn len(&self) -> usize {
        self.map().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Estimated heap bytes held by the cached events (struct plus string capacity).
    pub fn approx_bytes(&self) -> usize {
        self.map()
            .values()
            .flat_map(|(_, _, d)| d.events.iter())
            .map(|e| {
                let opt = |s: &Option<String>| s.as_ref().map_or(0, String::capacity);
                std::mem::size_of::<Event>()
                    + e.event_id.capacity()
                    + e.timestamp.capacity()
                    + e.session_id.capacity()
                    + e.trace_id.capacity()
                    + e.span_id.capacity()
                    + e.name.capacity()
                    + opt(&e.parent_span_id)
                    + opt(&e.intent_id)
                    + opt(&e.stage_id)
                    + opt(&e.unit_id)
                    + opt(&e.error_code)
            })
            .sum()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct QueryError {
    pub status: u16,
    pub code: &'static str,
}

const INVALID: QueryError = QueryError {
    status: 400,
    code: "invalid_filter",
};
const STALE: QueryError = QueryError {
    status: 409,
    code: "cursor_stale",
};
const UNAVAILABLE: QueryError = QueryError {
    status: 503,
    code: "store_unavailable",
};
const EXPIRY_MS: i64 = 7 * 86_400_000;
const MTIME_SLACK_MS: i64 = 2_000;
const KINDS: [EventKind; 8] = [
    EventKind::RequestStarted,
    EventKind::ToolCompleted,
    EventKind::ProtocolError,
    EventKind::OperationStarted,
    EventKind::OperationCompleted,
    EventKind::StageSelected,
    EventKind::StageReported,
    EventKind::GateDecided,
];

struct Params {
    range: i64,
    limit: usize,
    component: Option<String>,
    name: Option<String>,
    outcome: Option<String>,
    stage: Option<String>,
    session: Option<String>,
    trace: Option<String>,
    cursor: Option<String>,
    /// Normalised filters plus range, keys sorted, without cursor and limit.
    norm: String,
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn lower_hex(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn parse_params(raw: &str) -> Result<Params, QueryError> {
    if raw.contains('%') {
        return Err(INVALID);
    }
    let mut m: std::collections::BTreeMap<&str, &str> = Default::default();
    if !raw.is_empty() {
        for seg in raw.split('&') {
            let (k, v) = seg.split_once('=').ok_or(INVALID)?;
            if v.is_empty() || m.insert(k, v).is_some() {
                return Err(INVALID);
            }
        }
    }
    const KEYS: [&str; 9] = [
        "range",
        "component",
        "name",
        "outcome",
        "stage",
        "session",
        "trace",
        "limit",
        "cursor",
    ];
    if m.keys().any(|k| !KEYS.contains(k)) {
        return Err(INVALID);
    }
    let range = match m.get("range") {
        None => 3600,
        Some(v) if digits(v) => match v.parse::<u64>() {
            Ok(0) => return Err(INVALID),
            Ok(n) if n <= 604_800 => n as i64,
            _ => {
                return Err(QueryError {
                    status: 400,
                    code: "range_too_large",
                })
            }
        },
        Some(_) => return Err(INVALID),
    };
    let limit = match m.get("limit") {
        None => 50,
        Some(v) if digits(v) && v.len() <= 3 => match v.parse::<usize>() {
            Ok(n @ 1..=200) => n,
            _ => return Err(INVALID),
        },
        Some(_) => return Err(INVALID),
    };
    let pick = |k: &str, ok: &dyn Fn(&str) -> bool| -> Result<Option<String>, QueryError> {
        match m.get(k) {
            None => Ok(None),
            Some(v) if ok(v) => Ok(Some((*v).to_owned())),
            Some(_) => Err(INVALID),
        }
    };
    let component = pick("component", &|v| ["mcp", "application"].contains(&v))?;
    let name = pick("name", &|v| {
        ["status", "queue", "next", "report", "unknown", "protocol"].contains(&v)
    })?;
    let outcome = pick("outcome", &|v| ["ok", "blocked", "error"].contains(&v))?;
    // Per-field id rules (design pack s6).
    let stage = pick("stage", &|v| {
        v.len() <= 64
            && v.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })?;
    let session = pick("session", &|v| v.len() == 32 && lower_hex(v))?;
    let trace = pick("trace", &|v| {
        v.len() <= 64
            && v.split_once('-')
                .is_some_and(|(s, n)| s.len() == 32 && lower_hex(s) && digits(n))
    })?;
    // Length is checked in decode_cursor, so an overlong cursor is 409 cursor_stale (IF-017).
    let cursor = pick("cursor", &|_| true)?;
    let mut norm = format!("range={range}");
    for (k, v) in [
        ("component", &component),
        ("name", &name),
        ("outcome", &outcome),
        ("session", &session),
        ("stage", &stage),
        ("trace", &trace),
    ] {
        if let Some(v) = v {
            norm.push_str(&format!("&{k}={v}"));
        }
    }
    Ok(Params {
        range,
        limit,
        component,
        name,
        outcome,
        stage,
        session,
        trace,
        cursor,
        norm,
    })
}

fn qhash(norm: &str) -> String {
    use sha2::{Digest, Sha256};
    hex(&Sha256::digest(norm.as_bytes()))[..16].to_owned()
}

struct Cursor {
    to: i64,
    ts: i64,
    id: String,
}

fn decode_cursor(c: &str, hash: &str, now_ms: i64) -> Option<Cursor> {
    if c.len() > 1024 || !c.len().is_multiple_of(2) || !lower_hex(c) {
        return None;
    }
    let bytes: Vec<u8> = (0..c.len() / 2)
        .map(|i| u8::from_str_radix(&c[2 * i..2 * i + 2], 16).ok())
        .collect::<Option<_>>()?;
    let s = String::from_utf8(bytes).ok()?;
    let p: Vec<&str> = s.split('|').collect();
    let [v, to, h, ts, id] = p[..] else {
        return None;
    };
    let (to, ts) = (to.parse::<i64>().ok()?, ts.parse::<i64>().ok()?);
    // A cursor from the future or past its own window is not one this reader issued.
    (v == "v1" && h == hash && !id.is_empty() && to <= now_ms && ts <= to).then(|| Cursor {
        to,
        ts,
        id: id.to_owned(),
    })
}

/// Epoch ms of a timestamp that already passed `parse_line` validation.
fn ts_ms(s: &str) -> i64 {
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().unwrap_or(0);
    let (y, m, d) = (n(0..4), n(5..7), n(8..10));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days * 24 + n(11..13)) * 60 + n(14..16)) * 60_000 + n(17..19) * 1000 + n(20..23)
}

fn mtime_ms(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn parse_file(data: &[u8]) -> FileData {
    let complete = match data.iter().rposition(|b| *b == b'\n') {
        Some(i) => &data[..=i],
        None => &data[..0],
    };
    let mut events = Vec::new();
    let mut skipped = 0;
    for line in complete.split_inclusive(|b| *b == b'\n') {
        let line = &line[..line.len() - 1];
        match std::str::from_utf8(line).ok().and_then(parse_line) {
            Some(e) => events.push(e),
            None => skipped += 1,
        }
    }
    FileData {
        events,
        skipped,
        partial_tail: complete.len() != data.len(),
    }
}

pub fn query(
    root: &Path,
    raw_query: &str,
    now_ms: i64,
    cache: &Cache,
) -> Result<Value, QueryError> {
    query_with(root, raw_query, now_ms, cache, Caps::default())
}

pub fn query_with(
    root: &Path,
    raw_query: &str,
    now_ms: i64,
    cache: &Cache,
    caps: Caps,
) -> Result<Value, QueryError> {
    let p = parse_params(raw_query)?;
    let hash = qhash(&p.norm);
    let cursor = match &p.cursor {
        Some(c) => Some(decode_cursor(c, &hash, now_ms).ok_or(STALE)?),
        None => None,
    };
    let to_ms = cursor.as_ref().map_or(now_ms, |c| c.to);
    let from_ms = to_ms - p.range * 1000;

    // List store files (lstat, strict pattern) without touching anything.
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let (mut unreadable, mut expired) = (0u64, 0u64);
    match store_state(root) {
        StoreState::Unsafe => return Err(UNAVAILABLE),
        StoreState::Absent => {}
        StoreState::Dir => {
            let dir = root.join(".aidlc").join("telemetry");
            for entry in std::fs::read_dir(&dir).map_err(|_| UNAVAILABLE)? {
                let Ok(entry) = entry else {
                    unreadable += 1;
                    continue;
                };
                if !entry.file_name().to_str().is_some_and(is_store_file) {
                    continue;
                }
                match std::fs::symlink_metadata(entry.path()) {
                    Ok(m) if m.file_type().is_file() => {
                        let mtime = m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        if mtime_ms(mtime) < now_ms - EXPIRY_MS {
                            expired += 1;
                        } else if mtime_ms(mtime) >= from_ms - MTIME_SLACK_MS {
                            // A file last written before the window holds nothing in it. The
                            // slack covers coarse mtimes (FAT 2 s, HFS+ 1 s) and small clock steps.
                            files.push((entry.path(), m.len(), mtime));
                        }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => unreadable += 1,
                }
            }
        }
    }
    files.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));
    cache.map().retain(|k, _| files.iter().any(|f| &f.0 == k));

    // Read newest first within the caps.
    let (mut limited, mut files_read, mut bytes_read, mut total) = (false, 0usize, 0u64, 0usize);
    let (mut skipped, mut partial) = (0u64, 0u64);
    let mut loaded: Vec<Arc<FileData>> = Vec::new();
    for (path, len, mtime) in &files {
        if files_read >= caps.max_files || bytes_read + len > caps.max_bytes {
            limited = true;
            break;
        }
        let hit = cache
            .map()
            .get(path)
            .filter(|(l, t, _)| l == len && t == mtime)
            .map(|(_, _, d)| Arc::clone(d));
        let data = match hit {
            Some(d) => d,
            None => match std::fs::read(path) {
                Ok(bytes) => {
                    let d = Arc::new(parse_file(&bytes));
                    cache
                        .map()
                        .insert(path.clone(), (*len, *mtime, Arc::clone(&d)));
                    d
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    unreadable += 1;
                    continue;
                }
            },
        };
        files_read += 1;
        bytes_read += len;
        total += data.events.len();
        skipped += data.skipped;
        partial += u64::from(data.partial_tail);
        loaded.push(data);
        if total > caps.max_events {
            limited = true;
            break;
        }
    }

    // Dedup by eventId, then window, filters and order.
    let mut seen = std::collections::HashSet::new();
    let all: Vec<&Event> = loaded
        .iter()
        .flat_map(|d| d.events.iter())
        .filter(|e| seen.insert(e.event_id.as_str()))
        .collect();
    let completed_spans: std::collections::HashSet<&str> = all
        .iter()
        .filter(|e| e.kind == EventKind::ToolCompleted)
        .map(|e| e.span_id.as_str())
        .collect();
    let mut hits: Vec<(&Event, i64)> = all
        .iter()
        .map(|e| (*e, ts_ms(&e.timestamp)))
        .filter(|(e, t)| {
            *t >= from_ms
                && *t <= to_ms
                && p.component
                    .as_deref()
                    .is_none_or(|c| e.kind.component() == c)
                && p.name.as_deref().is_none_or(|n| e.name == n)
                && p.outcome
                    .as_deref()
                    .is_none_or(|o| e.outcome.is_some_and(|x| x.as_str() == o))
                && p.stage
                    .as_deref()
                    .is_none_or(|s| e.stage_id.as_deref() == Some(s))
                && p.session.as_deref().is_none_or(|s| e.session_id == s)
                && p.trace.as_deref().is_none_or(|t| e.trace_id == t)
        })
        .collect();
    hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.event_id.cmp(&b.0.event_id)));

    // Summary over the whole filtered range. Latency population: every distinct
    // `mcp.tool.completed` that carries a durationMs (every outcome included), so n equals
    // toolCalls unless a completion has a null duration.
    let count = |f: &dyn Fn(&Event) -> bool| hits.iter().filter(|(e, _)| f(e)).count();
    let tool = |e: &Event| e.kind == EventKind::ToolCompleted;
    let tool_calls = count(&tool);
    let op_done = |e: &Event| e.kind == EventKind::OperationCompleted;
    let mut durations: Vec<u64> = hits
        .iter()
        .filter(|(e, _)| tool(e))
        .filter_map(|(e, _)| e.duration_ms)
        .collect();
    durations.sort_unstable();
    let latency = if durations.len() >= 20 {
        let rank = |pct: usize| durations[(durations.len() * pct).div_ceil(100) - 1];
        json!({"n": durations.len(), "p50": rank(50), "p95": rank(95)})
    } else {
        json!({"n": durations.len(), "insufficient": true})
    };
    let mut kinds = serde_json::Map::new();
    for k in KINDS {
        kinds.insert(k.as_str().into(), json!(count(&|e| e.kind == k)));
    }
    let summary = json!({
        "toolCalls": tool_calls,
        "blocked": {"count": count(&|e| tool(e) && e.outcome == Some(Outcome::Blocked)), "of": tool_calls},
        "errors": {"count": count(&|e| tool(e) && e.outcome == Some(Outcome::Error)), "of": tool_calls},
        "incomplete": count(&|e| e.kind == EventKind::RequestStarted && !completed_spans.contains(e.span_id.as_str())),
        "operations": {"count": count(&op_done), "errors": count(&|e| op_done(e) && e.outcome == Some(Outcome::Error))},
        "latency": latency,
        "lastEventAt": hits.first().map(|(e, _)| e.timestamp.as_str()),
        "eventKinds": kinds,
    });

    // Page after the cursor.
    let rest: Vec<&(&Event, i64)> = hits
        .iter()
        .filter(|(e, t)| {
            cursor
                .as_ref()
                .is_none_or(|c| *t < c.ts || (*t == c.ts && e.event_id > c.id))
        })
        .collect();
    let page = &rest[..rest.len().min(p.limit)];
    let next = (rest.len() > p.limit)
        .then(|| page.last())
        .flatten()
        .map(|(e, t)| hex(format!("v1|{to_ms}|{hash}|{t}|{}", e.event_id).as_bytes()));
    let events: Vec<Value> = page
        .iter()
        .filter_map(|(e, _)| serde_json::from_str(&e.to_line()).ok())
        .collect();
    let mut out = json!({
        "schemaVersion": 1,
        "range": p.range,
        "to": crate::audit::format_iso_millis(to_ms),
        "events": events,
        "summary": summary,
        "coverage": {
            "filesRead": files_read,
            "skippedLines": skipped,
            "partialTails": partial,
            "unreadableFiles": unreadable,
            "expiredFiles": expired,
        },
        "nextCursor": next,
    });
    if limited {
        out["limitReached"] = json!("query_limit_reached");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{hex, EventKind, Outcome};
    use serde_json::json;
    use std::fs;

    const SID: &str = "0123456789abcdef0123456789abcdef";
    const SID2: &str = "fedcba9876543210fedcba9876543210";
    /// 2026-01-02T03:04:05.000Z in epoch ms.
    const T0: i64 = 1_767_323_045_000;

    fn tmp(label: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("q-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join(".aidlc").join("telemetry")).unwrap();
        p
    }

    fn tdir(root: &Path) -> PathBuf {
        root.join(".aidlc").join("telemetry")
    }

    fn iso(ms: i64) -> String {
        crate::audit::format_iso_millis(ms)
    }

    fn ev(kind: EventKind, sid: &str, n: u64, ts: i64, name: &str, out: Option<Outcome>) -> Event {
        let timed = matches!(
            kind,
            EventKind::ToolCompleted | EventKind::OperationCompleted
        );
        // Tool spans share the span id between start and completion (ADR-026).
        let span = if matches!(kind, EventKind::RequestStarted | EventKind::ToolCompleted) {
            format!("{sid}-{}", n / 10)
        } else {
            format!("{sid}-{n}")
        };
        Event {
            kind,
            event_id: format!("{sid}-{n}"),
            timestamp: iso(ts),
            process_id: 7,
            session_id: sid.into(),
            trace_id: format!("{sid}-{}", n / 10),
            span_id: span,
            parent_span_id: None,
            name: name.into(),
            intent_id: None,
            stage_id: Some("develop".into()),
            unit_id: None,
            duration_ms: timed.then_some(10),
            outcome: out,
            error_code: (out == Some(Outcome::Error)).then(|| "operation_failed".to_string()),
        }
    }

    fn done(sid: &str, n: u64, ts: i64, out: Outcome, dur: u64) -> Event {
        let mut e = ev(EventKind::ToolCompleted, sid, n, ts, "next", Some(out));
        e.duration_ms = Some(dur);
        e
    }

    fn file(root: &Path, part: u32, lines: &[String]) -> PathBuf {
        let name = crate::telemetry::file_name(T0 - 86_400_000, 7, SID, part);
        let p = tdir(root).join(name);
        let mut body = lines.join("\n");
        body.push('\n');
        fs::write(&p, body).unwrap();
        p
    }

    fn lines(evs: &[Event]) -> Vec<String> {
        evs.iter().map(Event::to_line).collect()
    }

    fn run(root: &Path, q: &str) -> Result<Value, QueryError> {
        query(root, q, T0 + 1000, &Cache::new())
    }

    fn err(root: &Path, q: &str) -> QueryError {
        run(root, q).unwrap_err()
    }

    fn ids(v: &Value) -> Vec<String> {
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["eventId"].as_str().unwrap().to_string())
            .collect()
    }

    fn seeded() -> PathBuf {
        let root = tmp("seed");
        let evs: Vec<Event> = (0..5)
            .map(|i| done(SID, 10 * (i + 1), T0 - 1000 * i as i64, Outcome::Ok, 5))
            .collect();
        file(&root, 0, &lines(&evs));
        root
    }

    fn set_age(p: &Path, age_ms: i64) {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis((T0 - age_ms) as u64);
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    // ---- parameter rejection (test-first) ----

    #[test]
    fn ts028_rejects_bad_parameters_with_invalid_filter() {
        let root = tmp("rej");
        let bad = [
            "bogus=1",
            "range=900&range=900",
            "component=mcp&component=mcp",
            "component=%6dcp",
            "%72ange=900",
            "../etc=1",
            "path=/etc/passwd",
            "file=x",
            "a/b=1",
            "stage=..%2f",
            "stage=../x",
            "stage=a/b",
            "stage=a.b",
            "stage=A",
            "stage=a_b",
            "stage=",
            "stage",
            "range",
            "range=",
            "range=abc",
            "range=-5",
            "range=+5",
            "range=0",
            "range=1.5",
            "limit=0",
            "limit=201",
            "limit=abc",
            "limit=-1",
            "component=cli",
            "component=MCP",
            "name=nope",
            "outcome=fail",
            "session=abc",
            "session=0123456789ABCDEF0123456789ABCDEF",
            "session=0123456789abcdef0123456789abcde",
            "trace=abc",
            "trace=0123456789abcdef0123456789abcdef",
            "trace=0123456789abcdef0123456789abcdef-x",
            "trace=0123456789abcdef0123456789abcdef-",
            "&",
            "range=900&",
            "&range=900",
            "range=900&&limit=5",
        ];
        for q in bad {
            assert_eq!(
                err(&root, q),
                QueryError {
                    status: 400,
                    code: "invalid_filter"
                },
                "query {q:?}"
            );
        }
        let long = "a".repeat(65);
        assert_eq!(err(&root, &format!("stage={long}")).code, "invalid_filter");
    }

    #[test]
    fn ts028_range_above_604800_is_range_too_large() {
        let root = tmp("rtl");
        for q in ["range=604801", "range=99999999999999999999999"] {
            assert_eq!(
                err(&root, q),
                QueryError {
                    status: 400,
                    code: "range_too_large"
                },
                "{q}"
            );
        }
    }

    #[test]
    fn ts028_accepts_valid_parameters_and_bounds() {
        let root = tmp("ok");
        let q = format!(
            "range=604800&limit=200&component=mcp&name=next&outcome=ok&stage=develop&session={SID}&trace={SID}-1"
        );
        let v = run(&root, &q).unwrap();
        assert_eq!(v["schemaVersion"], 1);
        assert_eq!(v["range"], 604800);
        assert_eq!(run(&root, "range=1&limit=1").unwrap()["range"], 1);
        assert_eq!(run(&root, "").unwrap()["range"], 3600);
    }

    // ---- cursor (test-first) ----

    #[test]
    fn ts028_cursor_pages_without_gap_or_repeat_and_freezes_to() {
        let root = seeded();
        let cache = Cache::new();
        let now = T0 + 1000;
        let p1 = query(&root, "limit=2", now, &cache).unwrap();
        assert_eq!(ids(&p1).len(), 2);
        let c1 = p1["nextCursor"].as_str().unwrap().to_string();
        let p2 = query(&root, &format!("limit=2&cursor={c1}"), now + 5000, &cache).unwrap();
        let c2 = p2["nextCursor"].as_str().unwrap().to_string();
        let p3 = query(&root, &format!("limit=2&cursor={c2}"), now + 9000, &cache).unwrap();
        assert!(p3["nextCursor"].is_null());
        let mut all = ids(&p1);
        all.extend(ids(&p2));
        all.extend(ids(&p3));
        let expect: Vec<String> = (1..=5).map(|i| format!("{SID}-{}", 10 * i)).collect();
        assert_eq!(all, expect);
        assert_eq!(p1["to"], p2["to"]);
        assert_eq!(p1["to"], p3["to"]);
        assert_eq!(p1["summary"], p3["summary"]);
        // The cursor is lowercase hex of v1|to|hash|ts|id.
        let raw: Vec<u8> = (0..c1.len() / 2)
            .map(|i| u8::from_str_radix(&c1[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        let s = String::from_utf8(raw).unwrap();
        let parts: Vec<&str> = s.split('|').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], "v1");
        assert_eq!(parts[2].len(), 16);
        assert_eq!(parts[4], ids(&p1)[1]);
    }

    #[test]
    fn ts028_cursor_decode_failures_are_409_cursor_stale() {
        let root = seeded();
        let stale = QueryError {
            status: 409,
            code: "cursor_stale",
        };
        for c in [
            "zz",
            "abc",
            "00",
            &hex(b"v1|1|2"),
            &hex(b"v2|1|0000000000000000|1|x"),
            &"ab".repeat(513),
        ] {
            assert_eq!(err(&root, &format!("cursor={c}")), stale, "cursor {c}");
        }
    }

    #[test]
    fn ts028_cursor_for_another_query_or_future_to_is_409() {
        let root = seeded();
        let cache = Cache::new();
        let now = T0 + 1000;
        let p1 = query(&root, "limit=2", now, &cache).unwrap();
        let c = p1["nextCursor"].as_str().unwrap();
        let stale = QueryError {
            status: 409,
            code: "cursor_stale",
        };
        for q in [
            format!("limit=2&outcome=ok&cursor={c}"),
            format!("limit=2&range=900&cursor={c}"),
            format!("limit=2&stage=x&cursor={c}"),
        ] {
            assert_eq!(query(&root, &q, now, &cache).unwrap_err(), stale, "{q}");
        }
        // Same cursor, but "now" earlier than the frozen `to`: a to mismatch.
        let q = format!("limit=2&cursor={c}");
        assert_eq!(query(&root, &q, T0 - 3_600_000, &cache).unwrap_err(), stale);
        // limit may change between pages (it is not part of the query hash).
        assert!(query(&root, &format!("limit=3&cursor={c}"), now, &cache).is_ok());
    }

    // ---- result shape and metrics ----

    #[test]
    fn ts028_shape_and_event_kinds_always_has_all_eight_keys() {
        let root = tmp("shape");
        let v = run(&root, "").unwrap();
        let kinds = v["summary"]["eventKinds"].as_object().unwrap();
        let mut keys: Vec<&str> = kinds.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = vec![
            "mcp.request.started",
            "mcp.tool.completed",
            "mcp.protocol.error",
            "operation.started",
            "operation.completed",
            "workflow.stage_selected",
            "workflow.stage_reported",
            "workflow.gate_decided",
        ];
        want.sort_unstable();
        assert_eq!(keys, want);
        assert!(kinds.values().all(|n| *n == 0));
        assert_eq!(v["summary"]["toolCalls"], 0);
        assert_eq!(v["summary"]["lastEventAt"], Value::Null);
        assert_eq!(
            v["summary"]["latency"],
            json!({"n": 0, "insufficient": true})
        );
        assert_eq!(v["nextCursor"], Value::Null);
        assert!(v.get("limitReached").is_none());
        for k in [
            "filesRead",
            "skippedLines",
            "partialTails",
            "unreadableFiles",
            "expiredFiles",
        ] {
            assert_eq!(v["coverage"][k], 0, "{k}");
        }
        assert_eq!(v["to"], iso(T0 + 1000));
    }

    #[test]
    fn ts028_summary_counts_dedup_and_separate_populations() {
        let root = tmp("sum");
        let mut evs = vec![
            ev(EventKind::RequestStarted, SID, 10, T0 - 9000, "next", None),
            done(SID, 11, T0 - 8000, Outcome::Ok, 10),
            ev(
                EventKind::RequestStarted,
                SID,
                20,
                T0 - 7000,
                "report",
                None,
            ),
            done(SID, 21, T0 - 6000, Outcome::Blocked, 10),
            ev(
                EventKind::RequestStarted,
                SID,
                30,
                T0 - 5000,
                "report",
                None,
            ),
            done(SID, 31, T0 - 4000, Outcome::Error, 10),
            // started, never completed: incomplete, not a success
            ev(
                EventKind::RequestStarted,
                SID,
                40,
                T0 - 3000,
                "status",
                None,
            ),
            ev(
                EventKind::OperationCompleted,
                SID,
                50,
                T0 - 2000,
                "report",
                Some(Outcome::Ok),
            ),
            ev(
                EventKind::OperationCompleted,
                SID,
                51,
                T0 - 1900,
                "report",
                Some(Outcome::Error),
            ),
            ev(
                EventKind::GateDecided,
                SID,
                60,
                T0 - 1800,
                "report",
                Some(Outcome::Ok),
            ),
        ];
        // the same eventId again (a retried duplicate record) and in a second file
        evs.push(done(SID, 11, T0 - 8000, Outcome::Ok, 10));
        file(&root, 0, &lines(&evs));
        file(
            &root,
            1,
            &lines(&[done(SID, 11, T0 - 8000, Outcome::Ok, 10)]),
        );
        let v = run(&root, "").unwrap();
        let s = &v["summary"];
        assert_eq!(s["toolCalls"], 3);
        assert_eq!(s["blocked"], json!({"count": 1, "of": 3}));
        assert_eq!(s["errors"], json!({"count": 1, "of": 3}));
        assert_eq!(s["incomplete"], 1);
        assert_eq!(s["operations"], json!({"count": 2, "errors": 1}));
        assert_eq!(s["latency"], json!({"n": 3, "insufficient": true}));
        assert_eq!(s["lastEventAt"], iso(T0 - 1800));
        assert_eq!(s["eventKinds"]["mcp.tool.completed"], 3);
        assert_eq!(s["eventKinds"]["mcp.request.started"], 4);
        assert_eq!(s["eventKinds"]["workflow.gate_decided"], 1);
        assert_eq!(s["eventKinds"]["operation.completed"], 2);
        assert_eq!(s["eventKinds"]["mcp.protocol.error"], 0);
        assert_eq!(ids(&v).len(), 10);
        assert_eq!(v["coverage"]["filesRead"], 2);
    }

    #[test]
    fn ts028_summary_covers_whole_range_and_filters_apply() {
        let root = seeded();
        let v = run(&root, "limit=1").unwrap();
        assert_eq!(ids(&v).len(), 1);
        assert_eq!(v["summary"]["toolCalls"], 5);
        let v = run(&root, "outcome=blocked").unwrap();
        assert!(ids(&v).is_empty());
        assert_eq!(v["summary"]["toolCalls"], 0);
        let v = run(&root, &format!("session={SID2}")).unwrap();
        assert!(ids(&v).is_empty());
        let v = run(&root, "stage=develop&component=mcp&name=next").unwrap();
        assert_eq!(ids(&v).len(), 5);
        let v = run(&root, "component=application").unwrap();
        assert!(ids(&v).is_empty());
        let v = run(&root, &format!("trace={SID}-1")).unwrap();
        assert_eq!(ids(&v), vec![format!("{SID}-10")]);
    }

    #[test]
    fn ts028_range_window_excludes_older_events() {
        let root = tmp("win");
        file(
            &root,
            0,
            &lines(&[
                done(SID, 10, T0 - 10_000, Outcome::Ok, 5),
                done(SID, 20, T0 - 2_000_000, Outcome::Ok, 5),
            ]),
        );
        assert_eq!(ids(&run(&root, "range=900").unwrap()).len(), 1);
        assert_eq!(ids(&run(&root, "range=3600").unwrap()).len(), 2);
    }

    #[test]
    fn ts028_latency_is_nearest_rank_at_20_samples() {
        let root = tmp("lat");
        let evs: Vec<Event> = (1..=20)
            .map(|i| done(SID, 10 * i, T0 - 1000 * i as i64, Outcome::Ok, i))
            .collect();
        file(&root, 0, &lines(&evs));
        let v = run(&root, "").unwrap();
        assert_eq!(
            v["summary"]["latency"],
            json!({"n": 20, "p50": 10, "p95": 19})
        );
        assert_eq!(v["summary"]["toolCalls"], 20);
        // 19 samples is still insufficient
        let root = tmp("lat19");
        file(&root, 0, &lines(&evs[..19]));
        let v = run(&root, "").unwrap();
        assert_eq!(
            v["summary"]["latency"],
            json!({"n": 19, "insufficient": true})
        );
    }

    #[test]
    fn ts028_events_are_newest_first_with_event_id_tie_break() {
        let root = tmp("ord");
        let evs = vec![
            done(SID, 10, T0 - 1000, Outcome::Ok, 1),
            done(SID, 30, T0 - 500, Outcome::Ok, 1),
            done(SID, 20, T0 - 500, Outcome::Ok, 1),
        ];
        file(&root, 0, &lines(&evs));
        let v = run(&root, "").unwrap();
        assert_eq!(
            ids(&v),
            vec![
                format!("{SID}-20"),
                format!("{SID}-30"),
                format!("{SID}-10")
            ]
        );
        // optional ids are omitted when absent and the line allowlist is kept
        let first = v["events"][2].as_object().unwrap();
        assert!(!first.contains_key("intentId") && first.contains_key("stageId"));
    }

    // ---- tolerance ----

    #[test]
    fn ts028_tolerance_counters() {
        let root = tmp("tol");
        let good = done(SID, 10, T0 - 1000, Outcome::Ok, 5).to_line();
        let mut unknown_kind = good.replace("mcp.tool.completed", "mcp.nope");
        let extra = good.replace("\"errorCode\":null", "\"errorCode\":null,\"x\":1");
        unknown_kind.push(' ');
        let big = format!("{{\"pad\":\"{}\"}}", "a".repeat(16 * 1024));
        let body = format!("{good}\nnot json\n{unknown_kind}\n{extra}\n{big}\n\n{good}\n{{\"half");
        let p = tdir(&root).join(crate::telemetry::file_name(T0 - 86_400_000, 7, SID, 0));
        fs::write(&p, body).unwrap();
        let v = run(&root, "").unwrap();
        assert_eq!(ids(&v).len(), 1, "duplicate good line counted once");
        assert_eq!(v["coverage"]["skippedLines"], 5);
        assert_eq!(v["coverage"]["partialTails"], 1);
        assert_eq!(v["coverage"]["filesRead"], 1);
        // a clean file counts neither
        let root = seeded();
        let v = run(&root, "").unwrap();
        assert_eq!(v["coverage"]["skippedLines"], 0);
        assert_eq!(v["coverage"]["partialTails"], 0);
    }

    #[test]
    fn ts028_ignores_non_store_entries_and_symlinks() {
        let root = seeded();
        let d = tdir(&root);
        fs::write(d.join(".gitignore"), "*\n").unwrap();
        fs::write(d.join("notes.jsonl"), "garbage\n").unwrap();
        fs::create_dir_all(d.join("mcp-20260101T000000Z-1-abcdef01-000.jsonl")).unwrap();
        #[cfg(unix)]
        {
            let target = d.join(crate::telemetry::file_name(T0 - 86_400_000, 7, SID, 0));
            let link = d.join("mcp-20260101T000000Z-2-abcdef02-000.jsonl");
            std::os::unix::fs::symlink(target, link).unwrap();
        }
        let v = run(&root, "").unwrap();
        assert_eq!(v["coverage"]["filesRead"], 1);
        assert_eq!(v["coverage"]["skippedLines"], 0);
        assert_eq!(ids(&v).len(), 5);
    }

    #[test]
    fn ts028_expired_files_are_counted_and_not_shown() {
        let root = tmp("exp");
        let p = file(
            &root,
            0,
            &lines(&[done(SID, 10, T0 - 1000, Outcome::Ok, 5)]),
        );
        set_age(&p, 8 * 86_400_000);
        let v = run(&root, "range=604800").unwrap();
        assert_eq!(v["coverage"]["expiredFiles"], 1);
        assert_eq!(v["coverage"]["filesRead"], 0);
        assert!(ids(&v).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn ts028_unreadable_file_is_counted_and_others_still_read() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp("unr");
        let p = file(
            &root,
            0,
            &lines(&[done(SID, 10, T0 - 1000, Outcome::Ok, 5)]),
        );
        file(&root, 1, &lines(&[done(SID, 20, T0 - 900, Outcome::Ok, 5)]));
        fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&p).is_ok() {
            return; // running as a user that ignores modes
        }
        let v = run(&root, "").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(v["coverage"]["unreadableFiles"], 1);
        assert_eq!(ids(&v).len(), 1);
    }

    // ---- store state ----

    #[test]
    fn ts028_absent_store_is_an_empty_200_and_is_not_created() {
        let root = std::env::temp_dir().join(format!("q-absent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let v = run(&root, "").unwrap();
        assert!(ids(&v).is_empty());
        assert!(!root.join(".aidlc").exists());
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        run(&root, "").unwrap();
        assert!(!root.join(".aidlc").join("telemetry").exists());
    }

    #[cfg(unix)]
    #[test]
    fn ts028_unsafe_store_is_503_store_unavailable() {
        let root = tmp("unsafe");
        let real = root.join("elsewhere");
        fs::create_dir_all(&real).unwrap();
        fs::remove_dir_all(tdir(&root)).unwrap();
        std::os::unix::fs::symlink(&real, tdir(&root)).unwrap();
        assert_eq!(
            err(&root, ""),
            QueryError {
                status: 503,
                code: "store_unavailable"
            }
        );
        let root = tmp("unsafe2");
        fs::remove_dir_all(root.join(".aidlc")).unwrap();
        fs::write(root.join(".aidlc"), "x").unwrap();
        assert_eq!(err(&root, "").status, 503);
    }

    #[test]
    fn ts028_reader_never_writes_creates_or_deletes() {
        let root = seeded();
        fs::write(tdir(&root).join(".gitignore"), "*\n").unwrap();
        let snap = |root: &Path| {
            let mut v: Vec<(String, Vec<u8>, SystemTime)> = fs::read_dir(tdir(root))
                .unwrap()
                .map(|e| {
                    let e = e.unwrap();
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        fs::read(e.path()).unwrap(),
                        e.metadata().unwrap().modified().unwrap(),
                    )
                })
                .collect();
            v.sort();
            v
        };
        let before = snap(&root);
        let cache = Cache::new();
        for q in ["", "limit=1", "outcome=error", "range=900"] {
            let _ = query(&root, q, T0 + 1000, &cache);
        }
        assert_eq!(snap(&root), before);
        let mut top: Vec<_> = fs::read_dir(root.join(".aidlc"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        top.sort();
        assert_eq!(top, vec![std::ffi::OsString::from("telemetry")]);
    }

    // ---- caps ----

    #[test]
    fn ts028_file_cap_returns_partial_200_newest_first() {
        let root = tmp("fcap");
        for i in 0..4u32 {
            let p = file(
                &root,
                i,
                &lines(&[done(
                    SID,
                    10 * (i as u64 + 1),
                    T0 - 1000 * (4 - i as i64),
                    Outcome::Ok,
                    5,
                )]),
            );
            set_age(&p, 1000 * (4 - i as i64));
        }
        let caps = Caps {
            max_files: 2,
            ..Caps::default()
        };
        let v = query_with(&root, "", T0 + 1000, &Cache::new(), caps).unwrap();
        assert_eq!(v["limitReached"], "query_limit_reached");
        assert_eq!(v["coverage"]["filesRead"], 2);
        assert_eq!(ids(&v), vec![format!("{SID}-40"), format!("{SID}-30")]);
    }

    #[test]
    fn ts028_byte_cap_returns_partial_200() {
        let root = tmp("bcap");
        let mut size = 0;
        for i in 0..3u32 {
            let p = file(
                &root,
                i,
                &lines(&[done(SID, 10 * (i as u64 + 1), T0 - 1000, Outcome::Ok, 5)]),
            );
            size = fs::metadata(&p).unwrap().len();
            set_age(&p, 1000 * (3 - i as i64));
        }
        let caps = Caps {
            max_bytes: size * 2,
            ..Caps::default()
        };
        let v = query_with(&root, "", T0 + 1000, &Cache::new(), caps).unwrap();
        assert_eq!(v["limitReached"], "query_limit_reached");
        assert_eq!(v["coverage"]["filesRead"], 2);
        // exactly at the cap is not a limit
        let caps = Caps {
            max_bytes: size * 3,
            ..Caps::default()
        };
        let v = query_with(&root, "", T0 + 1000, &Cache::new(), caps).unwrap();
        assert!(v.get("limitReached").is_none());
        assert_eq!(v["coverage"]["filesRead"], 3);
    }

    #[test]
    fn ts028_event_cap_returns_partial_200() {
        let root = tmp("ecap");
        for i in 0..3u32 {
            let evs: Vec<Event> = (0..4)
                .map(|j| {
                    done(
                        SID,
                        100 * (i as u64 + 1) + 10 * j,
                        T0 - 100 * (i as i64 * 4 + j as i64),
                        Outcome::Ok,
                        5,
                    )
                })
                .collect();
            let p = file(&root, i, &lines(&evs));
            set_age(&p, 1000 * (3 - i as i64));
        }
        let caps = Caps {
            max_events: 6,
            ..Caps::default()
        };
        let v = query_with(&root, "", T0 + 1000, &Cache::new(), caps).unwrap();
        assert_eq!(v["limitReached"], "query_limit_reached");
        assert_eq!(v["coverage"]["filesRead"], 2);
        let caps = Caps {
            max_events: 12,
            ..Caps::default()
        };
        let v = query_with(&root, "", T0 + 1000, &Cache::new(), caps).unwrap();
        assert!(v.get("limitReached").is_none());
        assert_eq!(v["summary"]["toolCalls"], 12);
    }

    // ---- cache ----

    #[test]
    fn ts028_cache_reuses_unchanged_files_and_reparses_changed_ones() {
        let root = seeded();
        let cache = Cache::new();
        let a = query(&root, "", T0 + 1000, &cache).unwrap();
        assert_eq!(cache.len(), 1);
        assert!(cache.approx_bytes() > 0);
        let b = query(&root, "", T0 + 1000, &cache).unwrap();
        assert_eq!(a, b);
        // append a line: length changes, so the file is parsed again
        let p = fs::read_dir(tdir(&root))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut body = fs::read_to_string(&p).unwrap();
        body.push_str(&done(SID, 99, T0 - 10, Outcome::Ok, 5).to_line());
        body.push('\n');
        fs::write(&p, body).unwrap();
        let c = query(&root, "", T0 + 1000, &cache).unwrap();
        assert_eq!(ids(&c).len(), 6);
        // a removed file leaves the cache
        fs::remove_file(&p).unwrap();
        query(&root, "", T0 + 1000, &cache).unwrap();
        assert_eq!(cache.len(), 0);
    }

    /// Measurement, not a gate: `cargo test --lib ts028_measure -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[cfg(unix)]
    fn ts028_measure_parse_cache_memory_at_caps() {
        let root = tmp("measure");
        let rss = || -> u64 {
            let out = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(0)
        };
        let (mut bytes, mut n, mut part) = (0u64, 0u64, 0u32);
        while bytes < 64 * 1024 * 1024 - 5 * 1024 * 1024 {
            let mut body = String::new();
            while body.len() < 5 * 1024 * 1024 - 1024 {
                n += 1;
                let mut e = done(SID, n, T0 - (n as i64 % 600_000), Outcome::Ok, 5);
                e.intent_id = Some("intent-long-identifier-0001".into());
                body.push_str(&e.to_line());
                body.push('\n');
            }
            bytes += body.len() as u64;
            let name = crate::telemetry::file_name(T0 - 86_400_000, 7, SID, part);
            fs::write(tdir(&root).join(name), body).unwrap();
            part += 1;
        }
        let before = rss();
        let cache = Cache::new();
        let t = std::time::Instant::now();
        let v = query(&root, "", T0 + 1000, &cache).unwrap();
        let cold = t.elapsed();
        let after = rss();
        let t = std::time::Instant::now();
        query(&root, "", T0 + 1000, &cache).unwrap();
        eprintln!(
            "MEASURE files={part} bytes={bytes} events={n} toolCalls={} limit={:?} rss_before_kib={before} rss_after_kib={after} cache_approx_bytes={} cold_ms={} warm_ms={}",
            v["summary"]["toolCalls"],
            v.get("limitReached"),
            cache.approx_bytes(),
            cold.as_millis(),
            t.elapsed().as_millis()
        );
        let _ = fs::remove_dir_all(&root);
    }
}
