//! Read-only Node-compatible `status` and `intent list` projection.
//!
//! This module intentionally does not migrate legacy state or quarantine corrupt
//! pointers. State is read through `crate::workflow` (Node `loadWorkflow`,
//! `loadIntent`, `readPointer`), so errors carry Node's text. Symbolic links in state
//! paths fail closed for status; listing reports individual unreadable intents
//! without following symlinks.

use crate::scope::Graph;
use crate::workflow::{
    depth_of, derive_status, intent_ids_on_disk, load_intent, load_workflow, normalise_state,
    ordered_stages, read_pointer, stage_finished, Pointer,
};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};

const ID_MAX: usize = 64;

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_MAX
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && id.as_bytes()[0].is_ascii_alphanumeric()
}

fn safe_file(root: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("Refusing unsafe {label} path"));
    }
    let mut path = root.to_path_buf();
    for part in rel.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing symlink in {label} path: {}",
                    path.display()
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "Cannot inspect {label} path {}: {e}",
                    path.display()
                ))
            }
        }
    }
    Ok(path)
}

fn state_path(root: &Path) -> Result<PathBuf, String> {
    safe_file(root, ".aidlc/state.json", ".aidlc state")
}
fn intent_state_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err(format!("Invalid intent ID '{id}'"));
    }
    safe_file(
        root,
        &format!(".aidlc/intents/{id}/state.json"),
        "intent state",
    )
}

/// Node `titleOf(intent)`: the first line, trimmed, at most 80 characters.
fn title(state: &Value) -> String {
    let text = match state.get("intent") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    };
    let first = text.split('\n').next().unwrap_or("");
    let first = first.strip_suffix('\r').unwrap_or(first);
    crate::identity::js_trim(first).chars().take(80).collect()
}

/// `state[key] ?? null`.
fn or_null(state: &Value, key: &str) -> Value {
    state
        .get(key)
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or(Value::Null)
}

/// `state.mode ?? 'human'` (the stored value is echoed as is).
fn mode_or_human(state: &Value) -> Value {
    match state.get("mode") {
        None | Some(Value::Null) => json!("human"),
        Some(mode) => mode.clone(),
    }
}

/// Node `progressOf(state, graph)`.
fn progress(state: &Value, graph: &Value) -> Value {
    let stages = ordered_stages(
        graph,
        state.get("scope").and_then(Value::as_str).unwrap_or(""),
        depth_of(state),
    );
    let done = stages
        .iter()
        .filter(|stage| {
            let id = stage.get("id").and_then(Value::as_str).unwrap_or("");
            stage_finished(
                state
                    .get("stages")
                    .and_then(|rows| rows.get(id))
                    .and_then(|row| row.get("status"))
                    .and_then(Value::as_str),
            )
        })
        .count();
    json!({"done": done, "total": stages.len()})
}

/// Node `listRow(state, graph, current)`.
fn list_row(state: &Value, graph: &Value, current: bool) -> Value {
    let derived = derive_status(state, graph);
    let display_status = match derived {
        "complete" | "abandoned" => derived,
        _ if current => "active",
        _ => "paused",
    };
    let mut row = Map::new();
    row.insert("intentId".into(), or_null(state, "intentId"));
    row.insert("title".into(), json!(title(state)));
    row.insert("scope".into(), or_null(state, "scope"));
    row.insert("mode".into(), mode_or_human(state));
    row.insert("status".into(), json!(display_status));
    row.insert("current".into(), json!(current && derived == "active"));
    row.insert("currentStage".into(), or_null(state, "currentStage"));
    row.insert("assignee".into(), or_null(state, "assignee"));
    row.insert("progress".into(), progress(state, graph));
    if let Some(units) = state.get("units").filter(|units| units.is_array()) {
        row.insert("units".into(), units.clone());
    }
    row.insert("createdAt".into(), or_null(state, "createdAt"));
    row.insert("updatedAt".into(), or_null(state, "updatedAt"));
    Value::Object(row)
}

/// The `status` payload for a loaded intent state (Node's `out({...})` in the `status`
/// branch). `currentStage` is left out when the state has no such key, as
/// `JSON.stringify` drops `undefined`.
fn status_payload(root: &Path, state: &Value, status: Value, legacy: bool) -> Value {
    let id = state.get("intentId").filter(|id| !id.is_null()).cloned();
    let dir = match &id {
        Some(Value::String(text)) if !text.is_empty() => json!(root
            .join(".aidlc")
            .join("intents")
            .join(text)
            .to_string_lossy()),
        _ => Value::Null,
    };
    let mut out = Map::new();
    out.insert(
        "intent".into(),
        state.get("intent").cloned().unwrap_or(Value::Null),
    );
    out.insert("intentId".into(), id.unwrap_or(Value::Null));
    out.insert("intentDir".into(), dir);
    out.insert(
        "scope".into(),
        state.get("scope").cloned().unwrap_or(Value::Null),
    );
    out.insert("mode".into(), mode_or_human(state));
    out.insert("status".into(), status);
    if let Some(stage) = state.get("currentStage") {
        out.insert("currentStage".into(), stage.clone());
    }
    out.insert(
        "stages".into(),
        state.get("stages").cloned().unwrap_or(Value::Null),
    );
    if legacy {
        out.insert("migration".into(), json!("pending"));
    }
    Value::Object(out)
}

/// Node-compatible `status [--intent ID]` output. Never migrates legacy state.
///
/// The state is read with [`crate::workflow::load_workflow`] (no migration), so every
/// error carries Node's text. Unlike Node, a symbolic link in the pointer or intent
/// state path is refused rather than followed.
pub fn status(
    root: impl AsRef<Path>,
    graph: &Graph,
    intent_id: Option<&str>,
) -> Result<Value, String> {
    let root = root.as_ref();
    let graph_value = graph.value();
    if let Some(id) = intent_id {
        let unknown = || format!("Unknown intent '{id}'. Run 'dtech-aidlc intent list'.");
        if !valid_id(id) {
            return Err(unknown());
        }
        intent_state_path(root, id)?;
        let (state, _) = load_intent(root, graph_value, id).map_err(|_| unknown())?;
        let status = state.get("status").cloned().unwrap_or(Value::Null);
        return Ok(status_payload(root, &state, status, false));
    }
    state_path(root)?;
    if let (Pointer::Active(Some(id)), _) = read_pointer(root) {
        intent_state_path(root, &id)?;
    }
    let loaded = load_workflow(root, graph_value, false)?;
    let Some(state) = loaded.state else {
        let initialized = safe_file(root, ".aidlc/config.json", "config")?.exists();
        let rows = list_intents(root, graph)?;
        let paused: Vec<Value> = rows
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| matches!(r["status"].as_str(), Some("active" | "paused")))
            .map(|r| r["intentId"].clone())
            .collect();
        let mut out = json!({"initialized":initialized,"activeIntent":null});
        if !paused.is_empty() {
            out["pausedIntents"] = json!(paused);
        }
        return Ok(out);
    };
    // A loaded v2 intent always carries the bytes it was read from; an unmigrated v1
    // state (read straight from the pointer file) does not.
    let legacy = loaded.state_bytes.is_none();
    // `state.status ?? 'active'`
    let status = state
        .get("status")
        .filter(|status| !status.is_null())
        .cloned()
        .unwrap_or_else(|| json!("active"));
    Ok(status_payload(root, &state, status, legacy))
}

/// Node-compatible `intent list` output (Node `listIntents`), sorted by creation time
/// then ID. An unmigrated v1 `.aidlc/state.json` is listed as the current intent (its
/// `intentId`, or `legacy`). Unlike Node, an intent whose state path holds a symbolic
/// link is reported as unreadable instead of being followed, and a symlinked pointer
/// is treated as corrupt.
pub fn list_intents(root: impl AsRef<Path>, graph: &Graph) -> Result<Value, String> {
    let root = root.as_ref();
    let graph_value = graph.value();
    let pointer = if state_path(root).is_ok() {
        read_pointer(root).0
    } else {
        Pointer::Corrupt("is not a recognised state file")
    };
    let corrupt = matches!(pointer, Pointer::Corrupt(_));
    let current_id: Option<String> = match &pointer {
        Pointer::Active(id) => id.clone(),
        Pointer::Legacy(state) => state
            .get("intentId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    };
    safe_file(root, ".aidlc/intents", "intents directory")?;
    let mut rows: Map<String, Value> = Map::new();
    for id in intent_ids_on_disk(root)
        .into_iter()
        .filter(|id| valid_id(id))
    {
        let loaded = intent_state_path(root, &id).and_then(|_| load_intent(root, graph_value, &id));
        let row = match loaded {
            Ok((state, _)) => {
                let current = !corrupt
                    && current_id.as_deref() == Some(id.as_str())
                    && derive_status(&state, graph_value) != "abandoned";
                list_row(&state, graph_value, current)
            }
            Err(error) => json!({"intentId": id, "status": "unreadable", "error": error}),
        };
        rows.insert(id, row);
    }
    if let Pointer::Legacy(raw) = pointer {
        let mut state = normalise_state(raw);
        let id = match state.get("intentId") {
            None | Some(Value::Null) => json!("legacy"),
            Some(id) => id.clone(),
        };
        state["intentId"] = id.clone();
        let mut row = list_row(&state, graph_value, true);
        row["current"] = json!(true);
        let key = match &id {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        rows.insert(key, row);
    }
    let mut result: Vec<Value> = rows.into_iter().map(|(_, row)| row).collect();
    let text = |value: &Value| match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    result.sort_by(|a, b| {
        text(&a["createdAt"])
            .cmp(&text(&b["createdAt"]))
            .then_with(|| text(&a["intentId"]).cmp(&text(&b["intentId"])))
    });
    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_validation_is_bounded_and_ascii() {
        assert!(valid_id("abc-1"));
        assert!(!valid_id("../abc"));
        assert!(!valid_id("Upper"));
        assert!(!valid_id(&"a".repeat(65)));
    }
    #[test]
    fn missing_active_intent_id_is_corrupt_not_paused() {
        let root =
            std::env::temp_dir().join(format!("aidlc-status-pointer-{}", std::process::id()));
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        fs::write(root.join(".aidlc/state.json"), r#"{"version":2}"#).unwrap();
        assert!(matches!(read_pointer(&root).0, Pointer::Corrupt(_)));
        let graph = crate::scope::read_graph(None).unwrap();
        assert!(status(&root, &graph, None).unwrap_err().starts_with(
            ".aidlc/state.json is not a recognised state file. No changes were made."
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn path_checker_rejects_symlink_components() {
        let root =
            std::env::temp_dir().join(format!("aidlc-status-symlink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let target = root.join("outside");
        fs::write(&target, "{}").unwrap();
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(&target, root.join(".aidlc/state.json"));
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_file(&target, root.join(".aidlc/state.json"));
        if link.is_ok() {
            assert!(state_path(&root).unwrap_err().contains("symlink"));
        }
        let _ = fs::remove_dir_all(root);
    }
}
