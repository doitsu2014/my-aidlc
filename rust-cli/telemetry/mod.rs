//! Telemetry core: the v1 event line (IF-012), the OS random source (IF-013) and the store
//! location rules. Written by the MCP adapter only (owner Q1: the CLI writes no telemetry).

pub mod sink;

pub use sink::Sink;

use serde_json::{json, Map, Value};
use std::io;
use std::path::Path;

/// Longest accepted line, in bytes.
pub const MAX_LINE: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    RequestStarted,
    ToolCompleted,
    ProtocolError,
    OperationStarted,
    OperationCompleted,
    StageSelected,
    StageReported,
    GateDecided,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Blocked,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub event_id: String,
    pub timestamp: String,
    pub process_id: u32,
    pub session_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub intent_id: Option<String>,
    pub stage_id: Option<String>,
    pub unit_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub outcome: Option<Outcome>,
    pub error_code: Option<String>,
}

const TOOL_NAMES: [&str; 5] = ["status", "queue", "next", "report", "unknown"];
const TOOL_CODES: [&str; 6] = [
    "invalid_arguments",
    "approver_required",
    "state_conflict",
    "operation_failed",
    "unknown_tool",
    "internal_error",
];
const PROTOCOL_CODES: [&str; 8] = [
    "line_too_long",
    "parse_error",
    "invalid_request",
    "method_not_found",
    "invalid_params",
    "unsupported_protocol_version",
    "internal_error",
    "protocol_error",
];

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::RequestStarted => "mcp.request.started",
            EventKind::ToolCompleted => "mcp.tool.completed",
            EventKind::ProtocolError => "mcp.protocol.error",
            EventKind::OperationStarted => "operation.started",
            EventKind::OperationCompleted => "operation.completed",
            EventKind::StageSelected => "workflow.stage_selected",
            EventKind::StageReported => "workflow.stage_reported",
            EventKind::GateDecided => "workflow.gate_decided",
        }
    }

    fn parse(s: &str) -> Option<EventKind> {
        use EventKind::*;
        [
            RequestStarted,
            ToolCompleted,
            ProtocolError,
            OperationStarted,
            OperationCompleted,
            StageSelected,
            StageReported,
            GateDecided,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }

    pub fn component(self) -> &'static str {
        match self {
            EventKind::RequestStarted | EventKind::ToolCompleted | EventKind::ProtocolError => {
                "mcp"
            }
            _ => "application",
        }
    }

    fn started(self) -> bool {
        matches!(
            self,
            EventKind::RequestStarted | EventKind::OperationStarted
        )
    }

    fn timed(self) -> bool {
        matches!(
            self,
            EventKind::ToolCompleted | EventKind::OperationCompleted
        )
    }

    fn name_ok(self, name: &str) -> bool {
        match self {
            EventKind::RequestStarted | EventKind::ToolCompleted => TOOL_NAMES.contains(&name),
            EventKind::ProtocolError => name == "protocol",
            EventKind::OperationStarted | EventKind::OperationCompleted => valid_id(name),
            EventKind::StageSelected => name == "next",
            EventKind::StageReported | EventKind::GateDecided => name == "report",
        }
    }
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Blocked => "blocked",
            Outcome::Error => "error",
        }
    }

    fn parse(s: &str) -> Option<Outcome> {
        [Outcome::Ok, Outcome::Blocked, Outcome::Error]
            .into_iter()
            .find(|o| o.as_str() == s)
    }
}

/// Id charset and cap shared by the optional ids and operation names.
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn is_session(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `<sessionId>-<n>`.
fn is_scoped_id(s: &str, session: &str) -> bool {
    s.strip_prefix(session)
        .and_then(|r| r.strip_prefix('-'))
        .is_some_and(|n| !n.is_empty() && n.len() <= 20 && n.bytes().all(|b| b.is_ascii_digit()))
}

fn is_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'.',
            23 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

impl Event {
    /// The IF-012 line without the trailing newline. Optional ids that fail validation are
    /// omitted, never stored.
    pub fn to_line(&self) -> String {
        let mut m = Map::new();
        m.insert("schemaVersion".into(), json!(1));
        m.insert("eventId".into(), json!(self.event_id));
        m.insert("timestamp".into(), json!(self.timestamp));
        m.insert("component".into(), json!(self.kind.component()));
        m.insert("caller".into(), json!("mcp"));
        m.insert("processId".into(), json!(self.process_id));
        m.insert("sessionId".into(), json!(self.session_id));
        m.insert("traceId".into(), json!(self.trace_id));
        m.insert("spanId".into(), json!(self.span_id));
        m.insert("parentSpanId".into(), json!(self.parent_span_id));
        m.insert("event".into(), json!(self.kind.as_str()));
        m.insert("name".into(), json!(self.name));
        for (key, v) in [
            ("intentId", &self.intent_id),
            ("stageId", &self.stage_id),
            ("unitId", &self.unit_id),
        ] {
            if let Some(v) = v.as_deref().filter(|v| valid_id(v)) {
                m.insert(key.into(), json!(v));
            }
        }
        m.insert("durationMs".into(), json!(self.duration_ms));
        m.insert("outcome".into(), json!(self.outcome.map(Outcome::as_str)));
        m.insert("errorCode".into(), json!(self.error_code));
        Value::Object(m).to_string()
    }
}

/// Re-validates every field; `None` for anything outside the v1 contract. Never panics.
pub fn parse_line(line: &str) -> Option<Event> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    if line.len() > MAX_LINE {
        return None;
    }
    let Value::Object(m) = serde_json::from_str::<Value>(line).ok()? else {
        return None;
    };
    const REQUIRED: [&str; 15] = [
        "schemaVersion",
        "eventId",
        "timestamp",
        "component",
        "caller",
        "processId",
        "sessionId",
        "traceId",
        "spanId",
        "parentSpanId",
        "event",
        "name",
        "durationMs",
        "outcome",
        "errorCode",
    ];
    const OPTIONAL: [&str; 3] = ["intentId", "stageId", "unitId"];
    if !REQUIRED.iter().all(|k| m.contains_key(*k))
        || !m
            .keys()
            .all(|k| REQUIRED.contains(&k.as_str()) || OPTIONAL.contains(&k.as_str()))
    {
        return None;
    }
    let s = |k: &str| m.get(k).and_then(Value::as_str);
    let opt_s = |k: &str| match m.get(k) {
        Some(Value::Null) => Some(None),
        Some(Value::String(v)) => Some(Some(v.as_str())),
        _ => None,
    };
    if m["schemaVersion"] != json!(1) || s("caller")? != "mcp" {
        return None;
    }
    let kind = EventKind::parse(s("event")?)?;
    if s("component")? != kind.component() {
        return None;
    }
    let session = s("sessionId")?;
    if !is_session(session) || !is_timestamp(s("timestamp")?) {
        return None;
    }
    let (event_id, trace_id, span_id) = (s("eventId")?, s("traceId")?, s("spanId")?);
    if ![event_id, trace_id, span_id]
        .iter()
        .all(|i| is_scoped_id(i, session))
    {
        return None;
    }
    let parent = opt_s("parentSpanId")?;
    if parent.is_some_and(|p| !is_scoped_id(p, session)) {
        return None;
    }
    let name = s("name")?;
    if !kind.name_ok(name) {
        return None;
    }
    let mut ids = [None, None, None];
    for (slot, key) in ids.iter_mut().zip(OPTIONAL) {
        if let Some(v) = m.get(key) {
            let v = v.as_str().filter(|v| valid_id(v))?;
            *slot = Some(v.to_owned());
        }
    }
    let duration_ms = match m.get("durationMs")? {
        Value::Null => None,
        v => Some(v.as_u64()?),
    };
    if duration_ms.is_some() && !kind.timed() {
        return None;
    }
    let outcome = match opt_s("outcome")? {
        None => None,
        Some(o) => Some(Outcome::parse(o)?),
    };
    if outcome.is_none() != kind.started() {
        return None;
    }
    let error_code = opt_s("errorCode")?;
    if let Some(code) = error_code {
        let set: &[&str] = if kind == EventKind::ProtocolError {
            &PROTOCOL_CODES
        } else {
            &TOOL_CODES
        };
        if outcome != Some(Outcome::Error) || !set.contains(&code) {
            return None;
        }
    }
    let [intent_id, stage_id, unit_id] = ids;
    Some(Event {
        kind,
        event_id: event_id.to_owned(),
        timestamp: s("timestamp")?.to_owned(),
        process_id: u32::try_from(m.get("processId")?.as_u64()?).ok()?,
        session_id: session.to_owned(),
        trace_id: trace_id.to_owned(),
        span_id: span_id.to_owned(),
        parent_span_id: parent.map(str::to_owned),
        name: name.to_owned(),
        intent_id,
        stage_id,
        unit_id,
        duration_ms,
        outcome,
        error_code: error_code.map(str::to_owned),
    })
}

/// Fills `buf` from the OS random source. No fallback generator (fail closed).
#[cfg(unix)]
pub fn os_random(buf: &mut [u8]) -> io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

#[cfg(windows)]
pub fn os_random(buf: &mut [u8]) -> io::Result<()> {
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut std::ffi::c_void,
            buffer: *mut u8,
            len: u32,
            flags: u32,
        ) -> i32;
    }
    const USE_SYSTEM_PREFERRED_RNG: u32 = 0x2;
    let len = u32::try_from(buf.len()).map_err(|_| io::Error::other("buffer too large"))?;
    // SAFETY: `buf` is valid for `len` writable bytes; a null algorithm handle is allowed with
    // BCRYPT_USE_SYSTEM_PREFERRED_RNG.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            len,
            USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(io::Error::other("BCryptGenRandom failed"))
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
pub fn os_random(_buf: &mut [u8]) -> io::Result<()> {
    Err(io::Error::other("no OS random source"))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreState {
    Absent,
    Dir,
    Unsafe,
}

/// Classifies `.aidlc/telemetry` without writing anything.
pub fn store_state(root: &Path) -> StoreState {
    if crate::audit::assert_safe_state_dir(root).is_err() {
        return StoreState::Unsafe;
    }
    let lstat = |p: &Path| match std::fs::symlink_metadata(p) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    };
    let aidlc = root.join(".aidlc");
    match lstat(&aidlc) {
        Ok(None) => return StoreState::Absent,
        Ok(Some(m)) if m.is_dir() => {}
        _ => return StoreState::Unsafe,
    }
    match lstat(&aidlc.join("telemetry")) {
        Ok(None) => StoreState::Absent,
        Ok(Some(m)) if m.is_dir() => StoreState::Dir,
        _ => StoreState::Unsafe,
    }
}

/// `mcp-<yyyymmddThhmmssZ>-<pid>-<8hex>-<NNN>.jsonl`; `start_ms` is the process start time.
pub fn file_name(start_ms: i64, pid: u32, session: &str, part: u32) -> String {
    let iso = crate::audit::format_iso_millis(start_ms);
    let stamp: String = iso[..19]
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'T')
        .collect();
    let short = &session[..session.len().min(8)];
    format!("mcp-{stamp}Z-{pid}-{short}-{part:03}.jsonl")
}

/// The one strict store-file pattern shared by the writer, prune and the reader.
pub fn is_store_file(name: &str) -> bool {
    let Some(rest) = name
        .strip_prefix("mcp-")
        .and_then(|r| r.strip_suffix(".jsonl"))
    else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    let [stamp, pid, short, part] = parts[..] else {
        return false;
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let sb = stamp.as_bytes();
    sb.len() == 16
        && sb[8] == b'T'
        && sb[15] == b'Z'
        && digits(&stamp[..8])
        && digits(&stamp[9..15])
        && !pid.is_empty()
        && pid.len() <= 10
        && digits(pid)
        && short.len() == 8
        && short
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && part.len() == 3
        && digits(part)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "0123456789abcdef0123456789abcdef";

    fn sample() -> Event {
        Event {
            kind: EventKind::ToolCompleted,
            event_id: format!("{SID}-2"),
            timestamp: "2026-01-02T03:04:05.678Z".into(),
            process_id: 42,
            session_id: SID.into(),
            trace_id: format!("{SID}-1"),
            span_id: format!("{SID}-1"),
            parent_span_id: None,
            name: "next".into(),
            intent_id: Some("intent-1".into()),
            stage_id: None,
            unit_id: None,
            duration_ms: Some(7),
            outcome: Some(Outcome::Ok),
            error_code: None,
        }
    }

    #[test]
    fn ts021_line_round_trips_with_keys_in_contract_order() {
        let e = sample();
        let line = e.to_line();
        assert!(!line.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "schemaVersion",
                "eventId",
                "timestamp",
                "component",
                "caller",
                "processId",
                "sessionId",
                "traceId",
                "spanId",
                "parentSpanId",
                "event",
                "name",
                "intentId",
                "durationMs",
                "outcome",
                "errorCode"
            ]
        );
        assert_eq!(v["component"], "mcp");
        assert_eq!(v["caller"], "mcp");
        assert_eq!(parse_line(&line), Some(e));
    }

    #[test]
    fn ts021_parse_rejects_unknown_extra_missing_and_bad_values() {
        use serde_json::Value;
        let good = sample().to_line();
        let mutate = |f: &dyn Fn(&mut serde_json::Map<String, Value>)| {
            let mut v: Value = serde_json::from_str(&good).unwrap();
            f(v.as_object_mut().unwrap());
            v.to_string()
        };
        assert!(parse_line(&good).is_some());
        let set =
            |k: &'static str, val: Value| mutate(&move |m| drop(m.insert(k.into(), val.clone())));
        assert!(parse_line(&set("event", "bogus".into())).is_none());
        assert!(parse_line(&set("detail", "x".into())).is_none());
        assert!(parse_line(&mutate(&|m| drop(m.remove("outcome")))).is_none());
        assert!(parse_line(&set("caller", "cli".into())).is_none());
        assert!(parse_line(&set("outcome", "approved".into())).is_none());
        assert!(parse_line(&set("intentId", "/etc/passwd".into())).is_none());
        assert!(parse_line(&set("schemaVersion", 2.into())).is_none());
        assert!(parse_line(&set("traceId", "nothex-1".into())).is_none());
        assert!(parse_line(&set("durationMs", "7".into())).is_none());
        assert!(parse_line("not json").is_none());
        assert!(parse_line("[]").is_none());
        assert!(parse_line(&"x".repeat(MAX_LINE + 1)).is_none());
    }

    #[test]
    fn ts021_invalid_optional_ids_are_omitted_not_stored() {
        let mut e = sample();
        e.intent_id = Some("C:\\Users\\secret".into());
        e.stage_id = Some("a".repeat(65));
        e.unit_id = Some("WU-001".into());
        let line = e.to_line();
        assert!(
            !line.contains("secret") && !line.contains("intentId") && !line.contains("stageId")
        );
        assert!(line.contains("\"unitId\":\"WU-001\""));
        assert!(parse_line(&line).is_some());
    }

    #[test]
    fn ts021_started_events_have_null_outcome_and_duration() {
        let mut e = sample();
        e.kind = EventKind::RequestStarted;
        e.duration_ms = None;
        e.outcome = None;
        assert_eq!(parse_line(&e.to_line()), Some(e.clone()));
        e.outcome = Some(Outcome::Ok);
        assert!(parse_line(&e.to_line()).is_none());
    }

    #[test]
    fn ts021_completed_accepts_null_duration_and_started_rejects_a_duration() {
        for kind in [EventKind::ToolCompleted, EventKind::OperationCompleted] {
            let mut e = sample();
            e.kind = kind;
            e.name = if kind == EventKind::ToolCompleted {
                "status"
            } else {
                "build"
            }
            .into();
            e.duration_ms = None;
            assert_eq!(parse_line(&e.to_line()), Some(e.clone()), "{kind:?}");
            e.duration_ms = Some(3);
            assert_eq!(parse_line(&e.to_line()), Some(e), "{kind:?}");
        }
        let mut s = sample();
        s.kind = EventKind::RequestStarted;
        s.outcome = None;
        s.duration_ms = Some(3);
        assert!(parse_line(&s.to_line()).is_none());
    }

    #[test]
    fn ts022_hex_is_lowercase_and_os_random_fills_distinct_buffers() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
        let (mut a, mut b) = ([0u8; 16], [0u8; 16]);
        os_random(&mut a).unwrap();
        os_random(&mut b).unwrap();
        assert_ne!(a, [0u8; 16]);
        assert_ne!(a, b);
        assert_eq!(hex(&a).len(), 32);
    }

    #[test]
    fn ts024_file_name_pattern_is_strict() {
        let n = file_name(1_767_323_045_678, 42, SID, 3);
        assert_eq!(n, "mcp-20260102T030405Z-42-01234567-003.jsonl");
        assert!(is_store_file(&n));
        for bad in [
            ".gitignore",
            "mcp-20260102T030405Z-42-01234567-3.jsonl",
            "mcp-20260102T030405Z-42-0123456G-003.jsonl",
            "mcp-20260102T030405Z-42-01234567-003.jsonl.tmp",
            "../mcp-20260102T030405Z-42-01234567-003.jsonl",
            "other-20260102T030405Z-42-01234567-003.jsonl",
        ] {
            assert!(!is_store_file(bad), "{bad}");
        }
    }

    #[test]
    fn ts024_store_state_distinguishes_absent_dir_and_unsafe() {
        let root = std::env::temp_dir().join(format!("ts024-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(store_state(&root), StoreState::Absent);
        std::fs::create_dir_all(root.join(".aidlc")).unwrap();
        assert_eq!(store_state(&root), StoreState::Absent);
        std::fs::create_dir_all(root.join(".aidlc/telemetry")).unwrap();
        assert_eq!(store_state(&root), StoreState::Dir);
        std::fs::remove_dir(root.join(".aidlc/telemetry")).unwrap();
        std::fs::write(root.join(".aidlc/telemetry"), b"file").unwrap();
        assert_eq!(store_state(&root), StoreState::Unsafe);
        let _ = std::fs::remove_dir_all(&root);
    }
}
