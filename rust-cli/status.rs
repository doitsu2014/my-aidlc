//! Read-only Node-compatible `status` and `intent list` projection.
//!
//! This module intentionally does not migrate legacy state or quarantine corrupt
//! pointers. Unsafe paths and malformed state fail closed for status; listing
//! reports individual unreadable intents without following symlinks.

use crate::scope::Graph;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
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

fn read_json(path: &Path) -> Result<Option<Value>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("{} cannot be parsed: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
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

fn ordered(graph: &Graph, scope: &str, depth: &str) -> Result<Vec<Value>, String> {
    let stages = graph.ordered_stages(scope, depth);
    if stages.is_empty() || graph.value()["scopes"].get(scope).is_none() {
        return Err(format!("Invalid intent scope '{scope}'"));
    }
    Ok(stages)
}

fn normalized_state(raw: Value, graph: &Graph, folder_id: Option<&str>) -> Result<Value, String> {
    let mut state = raw.as_object().cloned().ok_or("state is not an object")?;
    let id = state.get("intentId").and_then(Value::as_str);
    if folder_id.is_some_and(|folder| id != Some(folder))
        || id.is_some_and(|value| !valid_id(value))
    {
        return Err("intentId does not match folder".into());
    }
    if !state
        .get("intent")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Err("intent is not a non-empty string".into());
    }
    let scope = state
        .get("scope")
        .and_then(Value::as_str)
        .ok_or("scope is invalid")?;
    let _ = ordered(
        graph,
        scope,
        state.get("depth").and_then(Value::as_str).unwrap_or("full"),
    )?;
    if let Some(current) = state.get("currentStage").filter(|value| !value.is_null()) {
        if !current.as_str().is_some_and(|id| {
            graph
                .ordered_stages(scope, "full")
                .iter()
                .any(|stage| stage["id"].as_str() == Some(id))
        }) {
            return Err("currentStage is not a stage of the scope".into());
        }
    }
    if let Some(deps) = state.get("dependencies") {
        if !deps.as_object().is_some_and(|map| {
            map.values().all(|value| {
                value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
            })
        }) {
            return Err("dependencies is not an object of string arrays".into());
        }
    }
    if state
        .get("mode")
        .is_some_and(|m| !matches!(m.as_str(), Some("human" | "ai")))
    {
        return Err("mode is not ai or human".into());
    }
    let stages = state
        .get("stages")
        .and_then(Value::as_object)
        .ok_or("stages is not an object")?;
    for (name, row) in stages {
        if !matches!(
            row.get("status").and_then(Value::as_str),
            Some("in_progress" | "complete" | "blocked" | "skipped")
        ) {
            return Err(format!("stage {name} has an invalid status"));
        }
    }
    if let Some(stages) = state.get_mut("stages").and_then(Value::as_object_mut) {
        for row in stages.values_mut().filter_map(Value::as_object_mut) {
            if !row.get("detail").is_some_and(Value::is_string) {
                row.insert("detail".into(), Value::Null);
            }
        }
    }
    Ok(Value::Object(state))
}

fn derived_status(state: &Value, graph: &Graph) -> Result<&'static str, String> {
    if state["closed"]["kind"] == "abandoned" {
        return Ok("abandoned");
    }
    let stages = ordered(
        graph,
        state["scope"].as_str().unwrap_or(""),
        state["depth"].as_str().unwrap_or("full"),
    )?;
    if stages.iter().all(|stage| {
        matches!(
            state["stages"][stage["id"].as_str().unwrap_or("")]["status"].as_str(),
            Some("complete" | "skipped")
        )
    }) {
        Ok("complete")
    } else {
        Ok("active")
    }
}

fn current_pointer(root: &Path) -> Result<(Option<String>, bool, Option<Value>), String> {
    let path = state_path(root)?;
    let Some(pointer) = read_json(&path)? else {
        return Ok((None, false, None));
    };
    if pointer.is_object() && pointer["version"] == 2 {
        let active = pointer
            .get("activeIntentId")
            .ok_or(".aidlc/state.json is not a recognised state file")?;
        if active.is_null() {
            return Ok((None, false, None));
        }
        if let Some(id) = active.as_str().filter(|id| valid_id(id)) {
            return Ok((Some(id.to_owned()), false, None));
        }
        return Err(".aidlc/state.json is not a recognised state file".into());
    }
    if pointer.is_object() && pointer["stages"].is_object() {
        let id = pointer["intentId"].as_str().map(str::to_owned);
        return Ok((id, true, Some(pointer)));
    }
    Err(".aidlc/state.json is not a recognised state file".into())
}

fn progress(state: &Value, graph: &Graph) -> Result<Value, String> {
    let stages = ordered(
        graph,
        state["scope"].as_str().unwrap_or(""),
        state["depth"].as_str().unwrap_or("full"),
    )?;
    let done = stages
        .iter()
        .filter(|s| {
            matches!(
                state["stages"][s["id"].as_str().unwrap_or("")]["status"].as_str(),
                Some("complete" | "skipped")
            )
        })
        .count();
    Ok(json!({"done":done,"total":stages.len()}))
}
fn title(state: &Value) -> String {
    state["intent"]
        .as_str()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(80)
        .collect()
}
fn list_row(state: &Value, graph: &Graph, current: bool) -> Result<Value, String> {
    let status = derived_status(state, graph)?;
    let display_status = if status == "complete" || status == "abandoned" {
        status
    } else if current {
        "active"
    } else {
        "paused"
    };
    let mut row = Map::new();
    row.insert("intentId".into(), state["intentId"].clone());
    row.insert("title".into(), json!(title(state)));
    row.insert("scope".into(), state["scope"].clone());
    row.insert(
        "mode".into(),
        json!(state["mode"].as_str().unwrap_or("human")),
    );
    row.insert("status".into(), json!(display_status));
    row.insert("current".into(), json!(current && status == "active"));
    row.insert(
        "currentStage".into(),
        state
            .get("currentStage")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null),
    );
    row.insert(
        "assignee".into(),
        state
            .get("assignee")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null),
    );
    row.insert("progress".into(), progress(state, graph)?);
    if state["units"].is_array() {
        row.insert("units".into(), state["units"].clone());
    }
    row.insert(
        "createdAt".into(),
        state
            .get("createdAt")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null),
    );
    row.insert(
        "updatedAt".into(),
        state
            .get("updatedAt")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null),
    );
    Ok(Value::Object(row))
}

/// Node-compatible `status [--intent ID]` output. Never migrates legacy state.
pub fn status(
    root: impl AsRef<Path>,
    graph: &Graph,
    intent_id: Option<&str>,
) -> Result<Value, String> {
    let root = root.as_ref();
    if let Some(id) = intent_id {
        let path = intent_state_path(root, id)?;
        let raw = read_json(&path)?
            .ok_or_else(|| format!("Unknown intent '{id}'. Run 'dtech-aidlc intent list'."))?;
        let state = normalized_state(raw, graph, Some(id))
            .map_err(|e| format!(".aidlc/intents/{id}/state.json is invalid: {e}"))?;
        return status_row(&state, graph, root, false);
    }
    let (id, legacy, legacy_state) = current_pointer(root)?;
    let Some(id) = id else {
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
    let raw = if legacy {
        legacy_state.unwrap()
    } else {
        read_json(&intent_state_path(root, &id)?)?
            .ok_or_else(|| format!("Active intent '{id}' has no state file"))?
    };
    let state = normalized_state(raw, graph, if legacy { None } else { Some(&id) })?;
    status_row(&state, graph, root, legacy)
}

fn status_row(state: &Value, graph: &Graph, root: &Path, legacy: bool) -> Result<Value, String> {
    let mut out = json!({"intent":state["intent"],"intentId":state.get("intentId").cloned().unwrap_or(Value::Null),"intentDir":state.get("intentId").and_then(Value::as_str).map(|id| root.join(".aidlc").join("intents").join(id).to_string_lossy().into_owned()),"scope":state["scope"],"mode":state["mode"].as_str().unwrap_or("human"),"status":derived_status(state, graph)?,"currentStage":state.get("currentStage").filter(|v| !v.is_null()).cloned().unwrap_or(Value::Null),"stages":state["stages"]});
    if legacy {
        out["migration"] = json!("pending");
    }
    Ok(out)
}

/// Node-compatible `intent list` output, sorted by creation time then ID.
pub fn list_intents(root: impl AsRef<Path>, graph: &Graph) -> Result<Value, String> {
    let root = root.as_ref();
    let (current_id, corrupt, legacy) = match current_pointer(root) {
        Ok((id, is_legacy, state)) => (id, false, if is_legacy { state } else { None }),
        Err(_) => (None, true, None),
    };
    let dir = safe_file(root, ".aidlc/intents", "intents directory")?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!([])),
        Err(e) => return Err(format!("Cannot list intents: {e}")),
    };
    let mut rows: BTreeMap<String, Value> = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Cannot inspect intent entry: {e}"))?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&id) {
            continue;
        }
        let path = intent_state_path(root, &id)?;
        let row = match read_json(&path)
            .and_then(|raw| raw.ok_or_else(|| "state file is missing".to_owned()))
            .and_then(|raw| normalized_state(raw, graph, Some(&id)))
        {
            Ok(state) => list_row(
                &state,
                graph,
                !corrupt && current_id.as_deref() == Some(&id),
            )
            .unwrap_or_else(|e| json!({"intentId":id,"status":"unreadable","error":e})),
            Err(error) => {
                json!({"intentId":id,"status":"unreadable","error":format!(".aidlc/intents/{id}/state.json: {error}")})
            }
        };
        rows.insert(id, row);
    }
    if let Some(raw) = legacy {
        if let Ok(state) = normalized_state(raw, graph, None) {
            let id = state["intentId"].as_str().unwrap_or("legacy").to_owned();
            let mut row = list_row(&state, graph, true)?;
            row["intentId"] = json!(id);
            row["current"] = json!(true);
            rows.insert(id, row);
        }
    }
    let mut result: Vec<Value> = rows.into_values().collect();
    result.sort_by(|a, b| {
        a["createdAt"]
            .as_str()
            .unwrap_or("")
            .cmp(b["createdAt"].as_str().unwrap_or(""))
            .then_with(|| {
                a["intentId"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["intentId"].as_str().unwrap_or(""))
            })
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
        assert!(current_pointer(&root).is_err());
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
