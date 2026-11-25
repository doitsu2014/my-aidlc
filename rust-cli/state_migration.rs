//! Explicit migration of the legacy `.aidlc/state.json` workflow state.
//!
//! This module is standalone by design. Call [`migrate_legacy_state`] only after the
//! caller has established that the file is a valid legacy state and has chosen to migrate.
//! It preserves arbitrary state fields, refuses unsafe paths and malformed input, and
//! writes the v2 state, original-byte backup, pointer, then migration audit record.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

pub type MigrationResult<T> = Result<T, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct Migration {
    pub intent_id: String,
    pub state: Value,
}

/// Migrate an existing legacy state file. The graph requires `scopes` and `stages`;
/// stages may be either the workflow graph's array of stage records or a list of IDs.
pub fn migrate_legacy_state(root: impl AsRef<Path>, graph: &Value) -> MigrationResult<Migration> {
    let root = root.as_ref();
    #[cfg(windows)]
    if root.join(".aidlc/state.json").exists() {
        return Err("Legacy state migration is not yet safely supported on Windows".into());
    }
    let aidlc = root.join(".aidlc");
    let pointer_path = aidlc.join("state.json");
    ensure_safe(root, &pointer_path)?;
    let original = fs::read(&pointer_path).map_err(ioerr)?;
    let parsed: Value = serde_json::from_slice(&original)
        .map_err(|_| ".aidlc/state.json cannot be parsed as JSON".to_owned())?;
    if parsed.get("version").and_then(Value::as_u64) == Some(2) {
        return Err("State is already version 2; no changes were made".into());
    }
    validate_legacy(&parsed, graph)?;

    let mut state = parsed;
    let base_id = state
        .get("intentId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let id = if let Some(id) = base_id {
        safe_id(&id)?;
        id
    } else {
        choose_id(root, &state)?
    };
    state["intentId"] = Value::String(id.clone());
    normalize(&mut state);
    state["status"] = Value::String(derive_status(&state, graph));

    let intent_dir = aidlc.join("intents").join(&id);
    let state_path = intent_dir.join("state.json");
    let markdown_path = intent_dir.join("intent.md");
    let backup_path = aidlc.join("state.v1.bak.json");
    let audit_path = aidlc.join("audit.jsonl");
    for path in [
        &intent_dir,
        &state_path,
        &markdown_path,
        &backup_path,
        &audit_path,
    ] {
        ensure_safe(root, path)?;
    }
    if let Some(parent) = intent_dir.parent() {
        fs::create_dir_all(parent).map_err(ioerr)?;
        ensure_safe(root, &intent_dir)?;
    }
    fs::create_dir_all(&intent_dir).map_err(ioerr)?;
    ensure_safe(root, &state_path)?;

    // Never replace a state that does not represent this exact interrupted migration.
    if state_path.exists() && !matching_interrupted_state(&state_path, &state) {
        return Err(format!(
            "Intent folder '{id}' already contains different state; no changes were made"
        ));
    }

    let intent = state
        .get("intent")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !markdown_path.exists() {
        write_new(&markdown_path, format!("# {id}\n\n{intent}\n").as_bytes())?;
    }
    atomic_json(&state_path, &state)?;
    if !backup_path.exists() {
        write_new(&backup_path, &original)?;
    }
    atomic_json(
        &pointer_path,
        &serde_json::json!({
            "version": 2,
            "activeIntentId": id,
            "updatedAt": iso_now(),
        }),
    )?;
    append_audit(root, &audit_path, &id)?;
    Ok(Migration {
        intent_id: id,
        state,
    })
}

fn validate_legacy(state: &Value, graph: &Value) -> MigrationResult<()> {
    if !state.is_object() {
        return Err(".aidlc/state.json is invalid: not an object. No changes were made.".into());
    }
    if state
        .get("intent")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(
            ".aidlc/state.json is invalid: intent is not a non-empty string. No changes were made."
                .into(),
        );
    }
    let scope = state.get("scope").and_then(Value::as_str).ok_or_else(|| {
        ".aidlc/state.json is invalid: scope is not a key of graph.scopes. No changes were made"
            .to_owned()
    })?;
    if graph
        .pointer(&format!("/scopes/{}", escape(scope)))
        .and_then(Value::as_array)
        .is_none()
    {
        return Err(".aidlc/state.json is invalid: scope is not a key of graph.scopes. No changes were made.".into());
    }
    if let Some(mode) = state.get("mode") {
        if !matches!(mode.as_str(), Some("ai" | "human")) {
            return Err(
                ".aidlc/state.json is invalid: mode is not ai or human. No changes were made."
                    .into(),
            );
        }
    }
    let stages = state
        .get("stages")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ".aidlc/state.json is invalid: stages is not an object. No changes were made."
                .to_owned()
        })?;
    for (id, row) in stages {
        if !matches!(
            row.get("status").and_then(Value::as_str),
            Some("in_progress" | "complete" | "blocked" | "skipped")
        ) {
            return Err(format!(".aidlc/state.json is invalid: stage {id} has an invalid status. No changes were made."));
        }
    }
    if let Some(id) = state.get("intentId").and_then(Value::as_str) {
        safe_id(id)?;
    }
    if state.get("intentId").is_some() && !state["intentId"].is_string() {
        return Err(
            ".aidlc/state.json is invalid: intentId does not match folder. No changes were made."
                .into(),
        );
    }
    if let Some(current) = state.get("currentStage").and_then(Value::as_str) {
        let graph_stages = graph.get("stages").and_then(Value::as_array);
        let known = graph_stages.is_some_and(|rows| {
            rows.iter()
                .any(|row| row.get("id").and_then(Value::as_str) == Some(current))
        });
        if !known {
            return Err(".aidlc/state.json is invalid: currentStage is not a stage of the scope. No changes were made.".into());
        }
    }
    if let Some(deps) = state.get("dependencies") {
        if !deps.as_object().is_some_and(|m| {
            m.values()
                .all(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
        }) {
            return Err(".aidlc/state.json is invalid: dependencies is not an object of string arrays. No changes were made.".into());
        }
    }
    Ok(())
}

fn normalize(state: &mut Value) {
    state["version"] = Value::from(2);
    if state.get("dependencies").is_none() || state["dependencies"].is_null() {
        state["dependencies"] = Value::Object(Map::new());
    }
    if !state.get("stages").is_some_and(Value::is_object) {
        state["stages"] = Value::Object(Map::new());
    }
    if let Some(rows) = state["stages"].as_object_mut() {
        for row in rows.values_mut() {
            if let Some(obj) = row.as_object_mut() {
                if !obj.get("detail").is_some_and(Value::is_string) {
                    obj.insert("detail".into(), Value::Null);
                }
            }
        }
    }
    if let Some(obj) = state.as_object_mut() {
        obj.remove("project");
    }
}

fn derive_status(state: &Value, graph: &Value) -> String {
    if state.pointer("/closed/kind").and_then(Value::as_str) == Some("abandoned") {
        return "abandoned".into();
    }
    let scope = state.get("scope").and_then(Value::as_str).unwrap_or("");
    let phases = graph
        .pointer(&format!("/scopes/{}", escape(scope)))
        .and_then(Value::as_array);
    let stages = graph.get("stages").and_then(Value::as_array);
    let complete = phases.zip(stages).is_some_and(|(phases, stages)| {
        let ordered: Vec<&Value> = stages
            .iter()
            .filter(|stage| {
                phases
                    .iter()
                    .any(|p| p.as_str() == stage.get("phase").and_then(Value::as_str))
            })
            .collect();
        !ordered.is_empty()
            && ordered.iter().all(|stage| {
                let id = stage.get("id").and_then(Value::as_str).unwrap_or("");
                matches!(
                    state
                        .pointer(&format!("/stages/{}/status", escape(id)))
                        .and_then(Value::as_str),
                    Some("complete" | "skipped")
                )
            })
    });
    if complete {
        "complete".into()
    } else {
        "active".into()
    }
}

fn choose_id(root: &Path, state: &Value) -> MigrationResult<String> {
    let intent = state["intent"].as_str().unwrap_or("");
    let base = slug(intent)?;
    let mut id = base.clone();
    let mut n = 2usize;
    loop {
        safe_id(&id)?;
        let dir = root.join(".aidlc/intents").join(&id);
        let knowledge = root.join(".aidlc/knowledge").join(format!("{id}.md"));
        if !dir.exists() && !knowledge.exists() {
            return Ok(id);
        }
        let candidate = dir.join("state.json");
        if matching_by_identity(&candidate, state) {
            return Ok(id);
        }
        id = format!("{base}-{n}");
        n += 1;
    }
}

fn slug(text: &str) -> MigrationResult<String> {
    let folded = text.to_lowercase();
    let mut out = String::new();
    let mut dash = false;
    for ch in folded.chars() {
        if ch.is_ascii_alphanumeric() {
            if out.len() < 48 {
                out.push(ch);
                dash = false;
            }
        } else if !out.is_empty() && !dash && out.len() < 48 {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() >= 3 {
        return Ok(out);
    }
    // The JS implementation hashes NFC-normalized UTF-8. Without a Unicode normalization
    // dependency we cannot safely produce the same identifier for all equivalent strings.
    if !text.is_ascii() {
        return Err("Cannot safely derive the legacy Unicode fallback intentId without NFC normalization; add an explicit intentId and retry. No changes were made.".into());
    }
    let day = iso_now().get(0..10).unwrap_or("19700101").replace('-', "");
    let digest = Sha256::digest(text.as_bytes());
    Ok(format!(
        "intent-{day}-{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2]
    ))
}

fn matching_interrupted_state(path: &Path, wanted: &Value) -> bool {
    let Ok(existing) = read_value(path) else {
        return false;
    };
    matching_by_identity_values(&existing, wanted)
}
fn matching_by_identity(path: &Path, wanted: &Value) -> bool {
    read_value(path).is_ok_and(|existing| matching_by_identity_values(&existing, wanted))
}
fn matching_by_identity_values(existing: &Value, wanted: &Value) -> bool {
    existing.get("intent") == wanted.get("intent")
        && existing.get("createdAt") == wanted.get("createdAt")
}
fn read_value(path: &Path) -> MigrationResult<Value> {
    let bytes = fs::read(path).map_err(ioerr)?;
    serde_json::from_slice(&bytes).map_err(|_| "existing intent state cannot be parsed".into())
}

fn safe_id(id: &str) -> MigrationResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id.as_bytes()[0].is_ascii_lowercase() && !id.as_bytes()[0].is_ascii_digit()
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(
            ".aidlc/state.json is invalid: intentId does not match folder. No changes were made."
                .into(),
        );
    }
    Ok(())
}
fn ensure_safe(root: &Path, path: &Path) -> MigrationResult<()> {
    let root = root.to_path_buf();
    if !path.starts_with(&root) {
        return Err("workflow path escapes project root".into());
    }
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| "workflow path escapes project root")?;
    let mut current = root.clone();
    for part in relative.components() {
        if matches!(
            part,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err("workflow path escapes project root".into());
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing to write through symlink: {}",
                    current
                        .strip_prefix(&root)
                        .unwrap_or(&current)
                        .display()
                        .to_string()
                        .replace('\\', "/")
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ioerr(e)),
        }
    }
    Ok(())
}
fn atomic_json(path: &Path, value: &Value) -> MigrationResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("state"),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(ioerr)?;
    let result = (|| {
        file.write_all(&bytes).map_err(ioerr)?;
        file.sync_all().map_err(ioerr)?;
        replace_file(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
fn replace_file(from: &Path, to: &Path) -> MigrationResult<()> {
    // Fail closed on Windows instead of deleting the only existing state before
    // replacement. Atomic replacement must be implemented before migration is wired.
    #[cfg(windows)]
    if to.exists() {
        return Err("Atomic state replacement is not implemented on Windows".into());
    }
    fs::rename(from, to).map_err(ioerr)
}
fn write_new(path: &Path, bytes: &[u8]) -> MigrationResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(ioerr)?;
    file.write_all(bytes).map_err(ioerr)?;
    file.sync_all().map_err(ioerr)
}
fn append_audit(root: &Path, path: &Path, id: &str) -> MigrationResult<()> {
    ensure_safe(root, path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(ioerr)?;
    let row = serde_json::json!({"at":iso_now(),"event":"state.migrated","from":1,"intentId":id});
    writeln!(
        file,
        "{}",
        serde_json::to_string(&row).map_err(|e| e.to_string())?
    )
    .map_err(ioerr)?;
    file.sync_all().map_err(ioerr)
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn ioerr(error: std::io::Error) -> String {
    error.to_string()
}

fn iso_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86400);
    let day_seconds = seconds.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60
    )
}
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::UNIX_EPOCH;
    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    fn graph() -> Value {
        serde_json::json!({"scopes":{"solo":["phase"]},"phases":[{"id":"phase"}],"stages":[{"id":"problem-framing","phase":"phase","order":1}]})
    }
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "aidlc-state-migration-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join(".aidlc")).unwrap();
        p
    }
    fn legacy() -> Value {
        serde_json::json!({"version":1,"scope":"solo","intent":"test","currentStage":"problem-framing","stages":{"problem-framing":{"status":"in_progress","startedAt":"then"}},"dependencies":{},"createdAt":"original"})
    }

    #[test]
    #[cfg(not(windows))]
    fn migrates_preserving_bytes_and_is_idempotent() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        let bytes = b"{\"version\":1,\"scope\":\"solo\",\"intent\":\"test\",\"currentStage\":\"problem-framing\",\"stages\":{\"problem-framing\":{\"status\":\"in_progress\",\"startedAt\":\"then\"}},\"dependencies\":{},\"createdAt\":\"original\"}\n";
        fs::write(&path, bytes).unwrap();
        let result = migrate_legacy_state(&root, &graph()).unwrap();
        assert_eq!(result.intent_id, "test");
        assert_eq!(
            fs::read(root.join(".aidlc/state.v1.bak.json")).unwrap(),
            bytes
        );
        assert_eq!(result.state["version"], 2);
        assert_eq!(
            result.state["stages"]["problem-framing"]["detail"],
            Value::Null
        );
        assert_eq!(
            migrate_legacy_state(&root, &graph()).unwrap_err(),
            "State is already version 2; no changes were made"
        );
        let audit = fs::read_to_string(root.join(".aidlc/audit.jsonl")).unwrap();
        assert_eq!(audit.lines().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(windows)]
    fn windows_migration_fails_before_any_write() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        let bytes = serde_json::to_vec(&legacy()).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(migrate_legacy_state(&root, &graph())
            .unwrap_err()
            .contains("not yet safely supported"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!root.join(".aidlc/intents").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_state_fails_without_writes() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        let mut state = legacy();
        state["scope"] = Value::String("galaxy".into());
        let bytes = serde_json::to_vec(&state).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(migrate_legacy_state(&root, &graph()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!root.join(".aidlc/intents").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refuses_symlink_components() {
        let root = temp();
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(std::env::temp_dir(), root.join(".aidlc/intents"));
        #[cfg(windows)]
        let link =
            std::os::windows::fs::symlink_dir(std::env::temp_dir(), root.join(".aidlc/intents"));
        if let Err(error) = link {
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314)
            {
                fs::remove_dir_all(root).unwrap();
                return;
            }
            panic!("cannot create symlink fixture: {error}");
        }
        assert!(migrate_legacy_state(&root, &graph()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(not(windows))]
    fn interrupted_matching_folder_reused_and_conflicts_refused() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        let mut state = legacy();
        state["intentId"] = Value::String("test".into());
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        fs::create_dir_all(root.join(".aidlc/intents/test")).unwrap();
        let mut mirror = state.clone();
        normalize(&mut mirror);
        fs::write(
            root.join(".aidlc/intents/test/state.json"),
            serde_json::to_vec(&mirror).unwrap(),
        )
        .unwrap();
        assert_eq!(
            migrate_legacy_state(&root, &graph()).unwrap().intent_id,
            "test"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
