//! Native workflow state and sequencing primitives.
//!
//! This module intentionally accepts the methodology graph as `serde_json::Value` so
//! it can be tested independently and can consume the installed graph without codegen.
//! It does not claim parity for context construction or the Node artifact/prerequisite
//! validators; callers must run those checks before using `report` to complete work.

use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub type WorkflowResult<T> = Result<T, String>;

#[derive(Clone, Debug)]
pub struct Workflow {
    root: PathBuf,
    graph: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadedState {
    Absent,
    Active { id: String, state: Value },
    Legacy(Value),
    Paused,
}

impl Workflow {
    pub fn new(root: impl Into<PathBuf>, graph: Value) -> Self {
        Self {
            root: root.into(),
            graph,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn aidlc(&self) -> PathBuf {
        self.root.join(".aidlc")
    }
    fn pointer(&self) -> PathBuf {
        self.aidlc().join("state.json")
    }
    fn intent_dir(&self, id: &str) -> PathBuf {
        self.aidlc().join("intents").join(id)
    }
    fn intent_state(&self, id: &str) -> PathBuf {
        self.intent_dir(id).join("state.json")
    }

    fn safe_intent_id(id: &str) -> WorkflowResult<()> {
        if id.is_empty()
            || id.len() > 64
            || !id.as_bytes()[0].is_ascii_lowercase() && !id.as_bytes()[0].is_ascii_digit()
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("intentId is not a safe identifier".into());
        }
        Ok(())
    }

    fn check_no_symlink(&self, path: &Path) -> WorkflowResult<()> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| "workflow path escapes project root")?;
        let mut current = self.root.clone();
        for component in relative.components() {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(format!(
                        "workflow path contains a symlink: {}",
                        current.display()
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ioerr(error)),
            }
        }
        Ok(())
    }

    /// Loads the pointer/state without changing files. Legacy v1 is returned to the
    /// caller for an explicit migration decision rather than being silently rewritten.
    pub fn load(&self) -> WorkflowResult<LoadedState> {
        self.check_no_symlink(&self.pointer())?;
        if !self.pointer().exists() {
            return Ok(LoadedState::Absent);
        }
        let pointer = read_json(&self.pointer())?;
        if pointer.get("version").and_then(Value::as_u64) == Some(2) {
            let id = match pointer.get("activeIntentId") {
                Some(Value::Null) => return Ok(LoadedState::Paused),
                Some(Value::String(id)) if !id.is_empty() => id.as_str(),
                _ => return Err(
                    ".aidlc/state.json is not a recognised workflow state; no changes were made"
                        .into(),
                ),
            };
            Self::safe_intent_id(id)?;
            self.check_no_symlink(&self.intent_state(id))?;
            let state = read_json(&self.intent_state(id))
                .map_err(|e| format!("active intent '{id}': {e}"))?;
            validate_state(&state, &self.graph, Some(id))?;
            Ok(LoadedState::Active {
                id: id.to_owned(),
                state,
            })
        } else if pointer.get("stages").and_then(Value::as_object).is_some() {
            validate_state(&pointer, &self.graph, None)?;
            Ok(LoadedState::Legacy(pointer))
        } else {
            Err(".aidlc/state.json is not a recognised workflow state; no changes were made".into())
        }
    }

    /// Starts and persists a new intent. Existing active pointer is never overwritten.
    pub fn create(
        &self,
        intent: &str,
        scope: &str,
        mode: &str,
        depth: &str,
    ) -> WorkflowResult<Value> {
        if intent.trim().is_empty() {
            return Err("Intent text is empty".into());
        }
        if !self
            .graph
            .get("scopes")
            .and_then(|v| v.get(scope))
            .and_then(Value::as_array)
            .is_some()
        {
            return Err("scope must be a key of graph.scopes".into());
        }
        if !["full", "standard", "fast"].contains(&depth)
            || (depth != "full"
                && self
                    .graph
                    .pointer(&format!("/depths/{}/stages", escape(depth)))
                    .and_then(Value::as_array)
                    .is_none())
        {
            return Err(format!("unknown depth '{depth}'"));
        }
        if !["human", "ai"].contains(&mode) {
            return Err("mode must be human or ai".into());
        }
        match self.load()? {
            LoadedState::Active { .. } => return Err("An active intent already exists".into()),
            LoadedState::Legacy(_) => {
                return Err(
                    "Legacy state must be migrated explicitly before creating an intent".into(),
                )
            }
            _ => (),
        }
        let base = slug(intent);
        let mut id = base.clone();
        let mut n = 2;
        while self.intent_dir(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let state = json!({"version":2,"intentId":id,"intent":intent.trim(),"scope":scope,"mode":mode,"depth":depth,"status":"active","currentStage":null,"stages":{},"dependencies":{},"createdAt":now()});
        validate_state(&state, &self.graph, Some(&id))?;
        Self::safe_intent_id(&id)?;
        self.check_no_symlink(&self.intent_state(&id))?;
        fs::create_dir_all(self.intent_dir(&id)).map_err(ioerr)?;
        atomic_json(&self.intent_state(&id), &state)?;
        atomic_json(
            &self.pointer(),
            &json!({"version":2,"activeIntentId":id,"updatedAt":now()}),
        )?;
        Ok(state)
    }

    /// Selects the next stage, preserving graph order, scope/depth filtering, and
    /// work-unit progression. Returns the updated state and the stage record.
    pub fn next(&self, state: &mut Value) -> WorkflowResult<Option<Value>> {
        validate_state(
            state,
            &self.graph,
            state.get("intentId").and_then(Value::as_str),
        )?;
        let stages = self.ordered_stages(state)?;
        let units = work_units(state);
        let repeated: Vec<&Value> = stages
            .iter()
            .copied()
            .filter(|s| s.get("repeat").and_then(Value::as_str) == Some("work-unit"))
            .collect();
        let first = repeated.first().copied();
        let last = repeated.last().copied();
        let end_before = first
            .map(|x| stages.iter().position(|s| std::ptr::eq(*s, x)).unwrap())
            .unwrap_or(stages.len());
        let after = last
            .map(|x| stages.iter().position(|s| std::ptr::eq(*s, x)).unwrap() + 1)
            .unwrap_or(stages.len());
        if let Some(stage) = stages[..end_before]
            .iter()
            .find(|s| !finished(stage_status(state, s)))
        {
            set_null(state, "currentUnit");
            return self.select(state, stage);
        }
        if !repeated.is_empty() {
            if let Some(unit) = units
                .iter()
                .find(|u| !finished(u.get("status").and_then(Value::as_str)))
            {
                let uid = unit.get("id").and_then(Value::as_str).unwrap_or("");
                set_opt_str(state, "currentUnit", uid);
                let stage = repeated.iter().find(|s| {
                    let sid = s.get("id").and_then(Value::as_str).unwrap_or("");
                    let key = if uid.is_empty() { "_" } else { uid };
                    let status = state
                        .pointer(&format!("/stages/{sid}/units/{}/status", escape(key)))
                        .and_then(Value::as_str)
                        .or_else(|| {
                            if uid.is_empty() {
                                stage_status(state, s)
                            } else {
                                None
                            }
                        });
                    !status.map(|value| finished(Some(value))).unwrap_or(false)
                });
                if let Some(stage) = stage {
                    return self.select(state, stage);
                }
            }
        }
        set_null(state, "currentUnit");
        if let Some(stage) = stages[after..]
            .iter()
            .find(|s| !finished(stage_status(state, s)))
        {
            return self.select(state, stage);
        }
        set_null(state, "currentStage");
        Ok(None)
    }

    fn select(&self, state: &mut Value, stage: &Value) -> WorkflowResult<Option<Value>> {
        let id = stage
            .get("id")
            .and_then(Value::as_str)
            .ok_or("stage has no id")?;
        let stages = state
            .get_mut("stages")
            .and_then(Value::as_object_mut)
            .ok_or("stages is not an object")?;
        if !stages.contains_key(id) {
            stages.insert(
                id.to_owned(),
                json!({"status":"in_progress","startedAt":now()}),
            );
        }
        state["currentStage"] = json!(id);
        Ok(Some(stage.clone()))
    }

    /// Records a report only for the active stage. Gate stages require explicit
    /// approval/rejection input; successful completion must be prevalidated by caller.
    pub fn report(
        &self,
        state: &mut Value,
        stage_id: &str,
        result: &str,
        detail: Option<&str>,
        approval: Option<&str>,
        approved_by: Option<&str>,
    ) -> WorkflowResult<()> {
        if !["complete", "blocked", "skipped"].contains(&result) {
            return Err("result must be complete, blocked, or skipped".into());
        }
        if result != "complete" && detail.unwrap_or("").trim().is_empty() {
            return Err(format!("result {result} requires detail"));
        }
        if state.get("currentStage").and_then(Value::as_str) != Some(stage_id) {
            return Err("Only the current stage can be reported".into());
        }
        let stage = self
            .graph
            .get("stages")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|s| s.get("id").and_then(Value::as_str) == Some(stage_id))
            })
            .ok_or_else(|| format!("Unknown stage '{stage_id}'"))?;
        let phase = stage.get("phase").and_then(Value::as_str).unwrap_or("");
        let ordered = self.ordered_stages(state)?;
        let final_stage = ordered
            .iter()
            .rfind(|s| s.get("phase").and_then(Value::as_str) == Some(phase))
            .and_then(|s| s.get("id").and_then(Value::as_str));
        let phase_gate = final_stage == Some(stage_id)
            && (stage.get("repeat").and_then(Value::as_str) != Some("work-unit")
                || is_final_unit(state));
        let gate = self
            .graph
            .get("phases")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(phase))
            })
            .and_then(|p| p.get("gate"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if approval.is_some_and(|a| !["approved", "rejected"].contains(&a)) {
            return Err("approval must be approved or rejected".into());
        }
        if approval == Some("approved") && result == "blocked" {
            return Err("approval approved cannot be combined with result blocked".into());
        }
        if approval == Some("rejected") && result != "blocked" {
            return Err("approval rejected must be reported with result blocked and detail".into());
        }
        if approval.is_some() && !phase_gate {
            return Err(
                "approval is only accepted at a phase gate and on the last work unit".into(),
            );
        }
        if matches!(result, "complete" | "skipped") && phase_gate && approval != Some("approved") {
            return Err(format!("Phase gate {gate} requires explicit approval"));
        }
        if phase_gate && approval.is_some() && approved_by.unwrap_or("").trim().is_empty() {
            return Err("approval requires approved_by".into());
        }
        let stamp = now();
        let mode = state
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("human")
            .to_owned();
        let unit_key = state
            .get("currentUnit")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("_")
            .to_owned();
        let unit_done_status = if stage.get("repeat").and_then(Value::as_str) == Some("work-unit") {
            Some(work_units(state).iter().all(|unit| {
                let key = unit
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .unwrap_or("_");
                let status = if key == unit_key {
                    Some(result)
                } else {
                    state
                        .pointer(&format!(
                            "/stages/{}/units/{}/status",
                            escape(stage_id),
                            escape(key)
                        ))
                        .and_then(Value::as_str)
                };
                finished(status)
            }))
        } else {
            None
        };
        let row = state
            .pointer_mut(&format!("/stages/{}", escape(stage_id)))
            .and_then(Value::as_object_mut)
            .ok_or("stage state is missing")?;
        let mut reports = row
            .get("reports")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut report = json!({"at":stamp,"result":result,"detail":detail});
        if let Some(a) = approval {
            report["approval"] = json!({"decision":a,"by":approved_by,"mode":mode});
        }
        reports.push(report);
        row.insert("reports".into(), Value::Array(reports));
        row.insert("status".into(), json!(result));
        row.insert("reportedAt".into(), json!(stamp));
        row.insert("detail".into(), json!(detail));
        if stage.get("repeat").and_then(Value::as_str) == Some("work-unit") {
            row.entry("units").or_insert_with(|| json!({}))[unit_key.clone()] =
                json!({"status":result,"reportedAt":stamp});
            row.insert(
                "status".into(),
                json!(if unit_done_status == Some(true) {
                    result
                } else {
                    "in_progress"
                }),
            );
            let is_last_repeat = ordered
                .iter()
                .filter(|candidate| {
                    candidate.get("phase").and_then(Value::as_str) == Some(phase)
                        && candidate.get("repeat").and_then(Value::as_str) == Some("work-unit")
                })
                .filter_map(|candidate| candidate.get("id").and_then(Value::as_str))
                .next_back()
                == Some(stage_id);
            if is_last_repeat && finished(Some(result)) {
                if let Some(units) = state.get_mut("units").and_then(Value::as_array_mut) {
                    if let Some(unit) = units.iter_mut().find(|unit| {
                        unit.get("id").and_then(Value::as_str).unwrap_or("_") == unit_key
                    }) {
                        unit["status"] = json!(result);
                    }
                }
            }
        }
        if phase_gate && approval.is_some() {
            state["gates"][gate] = json!({"phase":phase,"stage":stage_id,"decision":approval,"result":result,"by":approved_by,"at":stamp,"artifacts":[]});
        }
        set_null(state, "currentStage");
        Ok(())
    }

    pub fn persist(&self, state: &Value) -> WorkflowResult<()> {
        validate_state(
            state,
            &self.graph,
            state.get("intentId").and_then(Value::as_str),
        )?;
        let id = state
            .get("intentId")
            .and_then(Value::as_str)
            .ok_or("intentId is missing")?;
        Self::safe_intent_id(id)?;
        self.check_no_symlink(&self.intent_state(id))?;
        self.check_no_symlink(&self.pointer())?;
        atomic_json(&self.intent_state(id), state)?;
        atomic_json(
            &self.pointer(),
            &json!({"version":2,"activeIntentId":id,"updatedAt":now()}),
        )
    }

    fn ordered_stages<'a>(&'a self, state: &Value) -> WorkflowResult<Vec<&'a Value>> {
        let scope = state
            .get("scope")
            .and_then(Value::as_str)
            .ok_or("scope is missing")?;
        let phases = self
            .graph
            .get("scopes")
            .and_then(|v| v.get(scope))
            .and_then(Value::as_array)
            .ok_or("scope is not in graph")?;
        if phases.iter().any(|phase| {
            !phase.as_str().is_some_and(|id| {
                self.graph
                    .get("phases")
                    .and_then(Value::as_array)
                    .is_some_and(|rows| {
                        rows.iter()
                            .any(|row| row.get("id").and_then(Value::as_str) == Some(id))
                    })
            })
        }) {
            return Err("scope references an unknown phase".into());
        }
        let depth = state.get("depth").and_then(Value::as_str).unwrap_or("full");
        let depth_stages = if depth == "full" {
            None
        } else {
            Some(
                self.graph
                    .pointer(&format!("/depths/{}/stages", escape(depth)))
                    .and_then(Value::as_array)
                    .ok_or_else(|| format!("unknown depth '{depth}'"))?,
            )
        };
        let stages = self
            .graph
            .get("stages")
            .and_then(Value::as_array)
            .ok_or("graph stages missing")?;
        let mut out: Vec<&Value> = stages
            .iter()
            .filter(|s| {
                phases
                    .iter()
                    .any(|p| p.as_str() == s.get("phase").and_then(Value::as_str))
                    && depth_stages
                        .map(|d| {
                            d.iter()
                                .any(|id| id.as_str() == s.get("id").and_then(Value::as_str))
                        })
                        .unwrap_or(true)
            })
            .collect();
        out.sort_by_key(|s| {
            (
                phases
                    .iter()
                    .position(|p| p.as_str() == s.get("phase").and_then(Value::as_str))
                    .unwrap_or(usize::MAX),
                s.get("order").and_then(Value::as_u64).unwrap_or(0),
            )
        });
        Ok(out)
    }
}

pub fn validate_state(state: &Value, graph: &Value, folder: Option<&str>) -> WorkflowResult<()> {
    if !state.is_object() {
        return Err("state is not an object".into());
    }
    if state
        .get("intent")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err("intent is not a non-empty string".into());
    }
    let scope = state
        .get("scope")
        .and_then(Value::as_str)
        .ok_or("scope is not a key of graph.scopes")?;
    if graph
        .get("scopes")
        .and_then(|v| v.get(scope))
        .and_then(Value::as_array)
        .is_none()
    {
        return Err("scope is not a key of graph.scopes".into());
    }
    if let Some(id) = folder {
        Workflow::safe_intent_id(id)?;
    }
    if let Some(id) = state.get("intentId").and_then(Value::as_str) {
        Workflow::safe_intent_id(id)?;
    }
    if let Some(depth) = state.get("depth").and_then(Value::as_str) {
        if depth != "full"
            && graph
                .pointer(&format!("/depths/{}/stages", escape(depth)))
                .and_then(Value::as_array)
                .is_none()
        {
            return Err(format!("unknown depth '{depth}'"));
        }
    }
    if !state.get("stages").and_then(Value::as_object).is_some() {
        return Err("stages is not an object".into());
    }
    if let Some(id) = folder {
        if state.get("intentId").and_then(Value::as_str) != Some(id) {
            return Err("intentId does not match folder".into());
        }
    }
    if let Some(mode) = state.get("mode").and_then(Value::as_str) {
        if !["human", "ai"].contains(&mode) {
            return Err("mode is not ai or human".into());
        }
    }
    if let Some(rows) = state.get("stages").and_then(Value::as_object) {
        for (id, row) in rows {
            if !["in_progress", "complete", "blocked", "skipped"]
                .contains(&row.get("status").and_then(Value::as_str).unwrap_or(""))
            {
                return Err(format!("stage {id} has an invalid status"));
            }
        }
    }
    Ok(())
}

fn read_json(path: &Path) -> WorkflowResult<Value> {
    let bytes = fs::read(path).map_err(ioerr)?;
    serde_json::from_slice(&bytes).map_err(|_| format!("{} cannot be parsed", path.display()))
}
fn atomic_json(path: &Path, value: &Value) -> WorkflowResult<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(ioerr)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(ioerr)?;
    f.write_all(
        serde_json::to_string_pretty(value)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(ioerr)?;
    f.sync_all().map_err(ioerr)?;
    fs::rename(&tmp, path).map_err(ioerr)
}
fn ioerr(e: std::io::Error) -> String {
    e.to_string()
}
fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            if out.len() < 48 {
                out.push(c);
                dash = false
            }
        } else if !out.is_empty() && !dash && out.len() < 48 {
            out.push('-');
            dash = true
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() < 3 {
        format!("intent-{}", now())
    } else {
        out
    }
}
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn finished(s: Option<&str>) -> bool {
    matches!(s, Some("complete" | "skipped"))
}
fn stage_status<'a>(state: &'a Value, stage: &Value) -> Option<&'a str> {
    let id = stage.get("id")?.as_str()?;
    state
        .pointer(&format!("/stages/{}/status", escape(id)))?
        .as_str()
}
fn set_null(state: &mut Value, key: &str) {
    state[key] = Value::Null;
}
fn set_opt_str(state: &mut Value, key: &str, s: &str) {
    state[key] = if s.is_empty() { Value::Null } else { json!(s) };
}
fn work_units(state: &Value) -> Vec<Value> {
    state
        .get("units")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![json!({"status":"in_progress"})])
}
fn is_final_unit(state: &Value) -> bool {
    let current = state.get("currentUnit").and_then(Value::as_str);
    work_units(state).iter().all(|u| {
        u.get("id").and_then(Value::as_str) == current
            || finished(u.get("status").and_then(Value::as_str))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph() -> Value {
        json!({"phases":[{"id":"a","gate":"g"},{"id":"b","gate":"h"}],"scopes":{"solo":["a","b"]},"depths":{},"stages":[{"id":"a1","phase":"a","order":1},{"id":"a2","phase":"a","order":2,"repeat":"work-unit"},{"id":"b1","phase":"b","order":1}]})
    }
    fn state() -> Value {
        json!({"intentId":"x","intent":"test","scope":"solo","mode":"human","depth":"full","stages":{},"units":[{"id":"u1","status":"in_progress"},{"id":"u2","status":"in_progress"}]})
    }
    #[test]
    fn malformed_pointer_is_not_treated_as_paused() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-malformed-pointer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let path = root.join(".aidlc/state.json");
        let workflow = Workflow::new(&root, graph());
        for malformed in [
            json!({"version":2}),
            json!({"version":2,"activeIntentId":7}),
            json!({"version":2,"activeIntentId":""}),
        ] {
            let bytes = serde_json::to_vec(&malformed).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(workflow.load().is_err());
            assert!(workflow.create("test", "solo", "human", "full").is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, br#"{"version":2,"activeIntentId":null}"#).unwrap();
        assert_eq!(workflow.load().unwrap(), LoadedState::Paused);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stages_follow_graph_then_units() {
        let w = Workflow::new(".", graph());
        let mut s = state();
        assert_eq!(w.next(&mut s).unwrap().unwrap()["id"], "a1");
        s["stages"]["a1"]["status"] = json!("complete");
        assert_eq!(w.next(&mut s).unwrap().unwrap()["id"], "a2");
        assert_eq!(s["currentUnit"], "u1");
    }
    #[test]
    fn gate_requires_explicit_approval() {
        let w = Workflow::new(".", graph());
        let mut s = state();
        s["stages"]["b1"] = json!({"status":"in_progress"});
        s["currentStage"] = json!("b1");
        assert!(w
            .report(&mut s, "b1", "complete", None, None, None)
            .unwrap_err()
            .contains("explicit approval"));
    }
    #[test]
    fn repeated_stage_tracks_each_unit_without_finishing_early() {
        let mut g = graph();
        g["stages"].as_array_mut().unwrap().insert(
            2,
            json!({"id":"a3","phase":"a","order":3,"repeat":"work-unit"}),
        );
        let w = Workflow::new(".", g);
        let mut s = state();
        s["currentStage"] = json!("a2");
        s["currentUnit"] = json!("u1");
        s["stages"]["a1"] = json!({"status":"complete"});
        s["stages"]["a2"] = json!({"status":"in_progress"});
        w.report(&mut s, "a2", "complete", None, None, None)
            .unwrap();
        assert_eq!(s["stages"]["a2"]["units"]["u1"]["status"], "complete");
        assert_eq!(s["stages"]["a2"]["status"], "in_progress");
        assert_eq!(s["units"][0]["status"], "in_progress");

        assert_eq!(w.next(&mut s).unwrap().unwrap()["id"], "a3");
        assert_eq!(s["currentUnit"], "u1");
        w.report(&mut s, "a3", "complete", None, None, None)
            .unwrap();
        assert_eq!(s["units"][0]["status"], "complete");
        assert_eq!(w.next(&mut s).unwrap().unwrap()["id"], "a2");
        assert_eq!(s["currentUnit"], "u2");
    }

    #[test]
    fn rejects_scope_and_depth_outside_graph() {
        let w = Workflow::new(".", graph());
        assert!(w
            .create("test", "missing", "human", "full")
            .unwrap_err()
            .contains("graph.scopes"));
        assert!(w
            .create("test", "solo", "human", "unknown")
            .unwrap_err()
            .contains("depth"));
        let mut invalid = state();
        invalid["scope"] = json!("missing");
        assert!(validate_state(&invalid, &graph(), None).is_err());
    }

    #[test]
    fn gate_rejects_invalid_or_contradictory_approval() {
        let w = Workflow::new(".", graph());
        let mut s = state();
        s["stages"]["b1"] = json!({"status":"in_progress"});
        s["currentStage"] = json!("b1");
        assert!(w
            .report(&mut s, "b1", "complete", None, Some("maybe"), Some("Owner"))
            .is_err());
        assert!(w
            .report(
                &mut s,
                "b1",
                "blocked",
                Some("blocked"),
                Some("approved"),
                Some("Owner")
            )
            .is_err());
        assert!(w
            .report(&mut s, "b1", "complete", None, Some("approved"), Some(" "))
            .is_err());
        w.report(
            &mut s,
            "b1",
            "complete",
            None,
            Some("approved"),
            Some("Owner"),
        )
        .unwrap();
        assert_eq!(s["gates"]["h"]["decision"], "approved");
    }

    #[test]
    fn validates_safe_intent_ids_and_scope_membership() {
        assert!(Workflow::safe_intent_id("../outside").is_err());
        assert!(Workflow::safe_intent_id("valid-intent-2").is_ok());
        let mut invalid = state();
        invalid["intentId"] = json!("../outside");
        assert!(validate_state(&invalid, &graph(), None).is_err());
        let mut malformed_graph = graph();
        malformed_graph["scopes"]["solo"] = json!(["unknown-phase"]);
        assert!(Workflow::new(".", malformed_graph)
            .ordered_stages(&state())
            .is_err());
    }

    #[test]
    fn invalid_state_is_rejected() {
        assert!(validate_state(&json!({}), &graph(), None).is_err());
    }
}
