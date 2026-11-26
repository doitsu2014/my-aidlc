//! Native workflow state and sequencing primitives.
//!
//! This module intentionally accepts the methodology graph as `serde_json::Value` so
//! it can be tested independently and can consume the installed graph without codegen.
//!
//! Two layers live here:
//! - The free functions under "State primitives" (pointer reading, intent
//!   loading/normalisation, status derivation, intent ids, Node's on-disk JSON format).
//!   `crate::orchestrate` builds the native `orchestrate next/report` on them.
//! - The older [`Workflow`] type (`create`/`next`/`report`/`persist`). It does not follow
//!   Node's selection, report or validation rules exactly and is kept for existing
//!   library callers; new code should use the free functions.

use crate::artifacts::{expected_artifacts, verify_artifacts};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod unicode_tables;

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

    pub fn graph(&self) -> &Value {
        &self.graph
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
        let state = self.draft(intent, scope, mode, depth)?;
        self.persist_new(&state)?;
        Ok(state)
    }

    /// Prepare a new intent without writing any files. Callers can perform
    /// prerequisite and directive validation before persisting the draft.
    pub fn draft(
        &self,
        intent: &str,
        scope: &str,
        mode: &str,
        depth: &str,
    ) -> WorkflowResult<Value> {
        let intent = intent.trim_start_matches('\u{feff}').replace("\r\n", "\n");
        let intent = intent.trim();
        if intent.is_empty() {
            return Err("Intent text is empty".into());
        }
        if intent.contains('\0') {
            return Err("Intent text contains a NUL character".into());
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
        self.check_no_symlink(&self.aidlc().join("intents"))?;
        self.check_no_symlink(&self.aidlc().join("knowledge"))?;
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
        loop {
            let intent_path = self.intent_dir(&id);
            let knowledge_path = self.aidlc().join("knowledge").join(format!("{id}.md"));
            if !path_taken(&intent_path)? && !path_taken(&knowledge_path)? {
                break;
            }
            id = format!("{base}-{n}");
            n += 1;
        }
        let state = json!({"version":2,"intentId":id,"intent":intent.trim(),"scope":scope,"mode":mode,"depth":depth,"status":"active","currentStage":null,"stages":{},"dependencies":{},"createdAt":now()});
        validate_state(&state, &self.graph, Some(&id))?;
        Self::safe_intent_id(&id)?;
        self.check_no_symlink(&self.intent_state(&id))?;
        self.check_no_symlink(&self.pointer())?;
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

    fn validate_completion(&self, state: &Value, stage: &Value) -> WorkflowResult<()> {
        let expected = expected_artifacts(&self.root, &self.graph, state, stage)?;
        if expected.is_empty() {
            return Ok(());
        }
        let verified = verify_artifacts(&self.root, &expected, Some(&self.graph), Some(state));
        if !verified.ok {
            let first = verified
                .problems
                .first()
                .map(|problem| {
                    format!(
                        "{} is {} ({})",
                        problem.output, problem.problem, problem.path
                    )
                })
                .unwrap_or_else(|| "artifact validation failed".into());
            return Err(format!(
                "Stage {} cannot be completed: {first}",
                stage["id"].as_str().unwrap_or("")
            ));
        }
        if matches!(stage["phase"].as_str(), Some("develop" | "launch")) {
            let files = expected
                .iter()
                .filter_map(|item| {
                    fs::read_to_string(&item.path)
                        .ok()
                        .map(|text| (item.rel_path.clone(), text))
                })
                .collect::<Vec<_>>();
            let units = state
                .get("units")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|unit| unit.get("id").and_then(Value::as_str))
                .map(crate::ids::normalize_id)
                .collect::<HashSet<_>>();
            let registry_path = expected.iter().find(|item| item.output == "id-registry");
            let registry_text = registry_path.and_then(|item| fs::read_to_string(&item.path).ok());
            let unit_ids = (!units.is_empty()).then_some(&units);
            let analysis = HashMap::new();
            let citation = crate::ids::citation_report(
                registry_path.map(|item| item.rel_path.as_str()),
                registry_text.as_deref(),
                unit_ids,
                &analysis,
                &files,
            );
            if !citation.unregistered.is_empty() {
                let ids = citation
                    .unregistered
                    .iter()
                    .map(|item| format!("{} in {}", item.id, item.file))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "Stage {} cites unregistered identifiers: {ids}",
                    stage["id"].as_str().unwrap_or("")
                ));
            }
        }
        Ok(())
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
        validate_state(
            state,
            &self.graph,
            state.get("intentId").and_then(Value::as_str),
        )?;
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
        if matches!(result, "complete" | "skipped") && phase_gate && approval != Some("approved") {
            let gate_name = self
                .graph
                .get("phases")
                .and_then(Value::as_array)
                .and_then(|phases| {
                    phases
                        .iter()
                        .find(|row| row.get("id").and_then(Value::as_str) == Some(phase))
                })
                .and_then(|row| row.get("gate"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return Err(format!("Phase gate {gate_name} requires explicit approval"));
        }
        if result == "complete" {
            self.validate_completion(state, stage)?;
        }
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

    /// Persists an update to an already-active intent. The pointer is validated
    /// before writing, and is deliberately not rewritten: its identity cannot
    /// change during an ordinary state update.
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
        match self.load()? {
            LoadedState::Active { id: active, .. } if active == id => {}
            LoadedState::Active { id: active, .. } => {
                return Err(format!(
                    "Active intent is '{active}', not '{id}'; no changes were made"
                ));
            }
            LoadedState::Absent | LoadedState::Paused => {
                return Err("No active intent pointer exists; no changes were made".into());
            }
            LoadedState::Legacy(_) => {
                return Err(
                    "Legacy state must be migrated explicitly before updating an intent".into(),
                );
            }
        }
        let mut saved = state.clone();
        saved["updatedAt"] = json!(now());
        atomic_json(&self.intent_state(id), &saved)
    }

    /// Persists a newly drafted intent. Creation is separate from updates so it
    /// may install the pointer once; if that write fails, the intent file is
    /// left as an unreferenced record rather than deleting/reverting data.
    fn persist_new(&self, state: &Value) -> WorkflowResult<()> {
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
        match self.load()? {
            LoadedState::Absent | LoadedState::Paused => {}
            LoadedState::Active { .. } => return Err("An active intent already exists".into()),
            LoadedState::Legacy(_) => {
                return Err(
                    "Legacy state must be migrated explicitly before creating an intent".into(),
                );
            }
        }
        if path_taken(&self.intent_dir(id))? {
            return Err(format!(
                "Intent '{id}' already exists; no changes were made"
            ));
        }
        let mut saved = state.clone();
        saved["updatedAt"] = json!(now());
        atomic_json(&self.intent_state(id), &saved)?;
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

fn path_taken(path: &Path) -> WorkflowResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ioerr(error)),
    }
}

fn read_json(path: &Path) -> WorkflowResult<Value> {
    let bytes = fs::read(path).map_err(ioerr)?;
    serde_json::from_slice(&bytes).map_err(|_| format!("{} cannot be parsed", path.display()))
}
fn atomic_json(path: &Path, value: &Value) -> WorkflowResult<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(ioerr)?;
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    for _ in 0..100 {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let tmp = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ioerr(error)),
        };
        let result = (|| {
            file.write_all(&bytes).map_err(ioerr)?;
            file.sync_all().map_err(ioerr)?;
            drop(file);
            fs::rename(&tmp, path).map_err(ioerr)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        return result;
    }
    Err("Could not allocate a unique workflow temporary file".into())
}
fn ioerr(e: std::io::Error) -> String {
    e.to_string()
}
fn now() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = time.as_secs() as i64;
    let z = seconds.div_euclid(86400) + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let day_seconds = seconds.rem_euclid(86400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        day_seconds / 3600,
        day_seconds % 3600 / 60,
        day_seconds % 60,
        time.subsec_millis()
    )
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
        format!(
            "intent-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )
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

// ---------------------------------------------------------------------------
// State primitives (ported from the former Node CLI's `workflow.mjs` / `lib.mjs`).
//
// These free functions reproduce Node's pointer reading, intent loading and
// normalisation, status derivation, intent id derivation and on-disk JSON format
// (`JSON.stringify(value, null, 2) + "\n"`, written to a temp file and renamed).
// `crate::orchestrate` builds the native `orchestrate next/report` lifecycle on them.
// ---------------------------------------------------------------------------

/// The state pointer as Node's `readPointer` classifies it.
#[derive(Clone, Debug, PartialEq)]
pub enum Pointer {
    Absent,
    /// A version 2 pointer; `None` means no active intent (paused).
    Active(Option<String>),
    Legacy(Value),
    Corrupt(&'static str),
}

/// What `load_workflow` found, plus the raw bytes it read so callers can refuse to
/// persist when the files changed underneath them.
#[derive(Clone, Debug, Default)]
pub struct LoadedWorkflow {
    pub active_intent_id: Option<String>,
    pub state: Option<Value>,
    pub pointer_bytes: Option<Vec<u8>>,
    pub state_bytes: Option<Vec<u8>>,
}

/// Node `ID`: `^[a-z0-9][a-z0-9-]{0,63}$`.
pub fn valid_intent_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

pub fn state_path(root: &Path) -> PathBuf {
    root.join(".aidlc").join("state.json")
}

pub fn intent_dir_path(root: &Path, id: &str) -> PathBuf {
    root.join(".aidlc").join("intents").join(id)
}

pub fn intent_state_path(root: &Path, id: &str) -> PathBuf {
    intent_dir_path(root, id).join("state.json")
}

fn read_optional(path: &Path) -> WorkflowResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Node `readJsonStrict` on bytes already read (`undefined` is `None`).
fn parse_strict(path: &Path, bytes: Option<&[u8]>) -> WorkflowResult<Option<Value>> {
    let Some(bytes) = bytes else { return Ok(None) };
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text)
        .map(Some)
        .map_err(|_| format!("{} cannot be parsed as JSON", path.display()))
}

/// Node `readPointer`, returning the raw bytes alongside the classification.
pub fn read_pointer(root: &Path) -> (Pointer, Option<Vec<u8>>) {
    let path = state_path(root);
    let bytes = match read_optional(&path) {
        Ok(bytes) => bytes,
        Err(_) => return (Pointer::Corrupt("cannot be parsed"), None),
    };
    let raw = match parse_strict(&path, bytes.as_deref()) {
        Ok(None) => return (Pointer::Absent, None),
        Ok(Some(raw)) => raw,
        Err(_) => return (Pointer::Corrupt("cannot be parsed"), bytes),
    };
    let pointer = if raw.is_object() && raw.get("version").and_then(Value::as_f64) == Some(2.0) {
        match raw.get("activeIntentId") {
            Some(Value::Null) => Some(Pointer::Active(None)),
            Some(Value::String(id)) if valid_intent_id(id) => {
                Some(Pointer::Active(Some(id.clone())))
            }
            _ => None,
        }
    } else {
        None
    };
    let pointer = pointer.unwrap_or_else(|| {
        if raw.get("stages").is_some_and(Value::is_object) {
            Pointer::Legacy(raw)
        } else {
            Pointer::Corrupt("is not a recognised state file")
        }
    });
    (pointer, bytes)
}

/// Node `intentIdsOnDisk`: every directory under `.aidlc/intents`, sorted.
pub fn intent_ids_on_disk(root: &Path) -> Vec<String> {
    let mut ids = fs::read_dir(root.join(".aidlc").join("intents"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

fn corrupt_message(reason: &str, root: &Path) -> String {
    let ids = intent_ids_on_disk(root);
    format!(
        ".aidlc/state.json {reason}. No changes were made. Intents on disk: {}. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
        if ids.is_empty() { "none".to_owned() } else { ids.join(", ") }
    )
}

/// Node `orderedStages(graph, scope, depth)`.
pub fn ordered_stages<'a>(graph: &'a Value, scope: &str, depth: &str) -> Vec<&'a Value> {
    let Some(included) = graph
        .get("scopes")
        .and_then(|scopes| scopes.get(scope))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let phases = graph
        .get("phases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let phase_order = |id: Option<&str>| -> f64 {
        phases
            .iter()
            .enumerate()
            .find(|(_, phase)| phase.get("id").and_then(Value::as_str) == id)
            .map(|(index, phase)| {
                phase
                    .get("order")
                    .and_then(Value::as_f64)
                    .filter(|n| n.is_finite())
                    .unwrap_or(index as f64)
            })
            .unwrap_or(f64::NAN)
    };
    let mut rows: Vec<(usize, &Value)> = graph
        .get("stages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, stage)| {
            included.iter().any(|phase| {
                phase.as_str().is_some()
                    && phase.as_str() == stage.get("phase").and_then(Value::as_str)
            })
        })
        .collect();
    rows.sort_by(|(ai, a), (bi, b)| {
        let pa = phase_order(a.get("phase").and_then(Value::as_str));
        let pb = phase_order(b.get("phase").and_then(Value::as_str));
        let oa = a
            .get("order")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .unwrap_or(*ai as f64);
        let ob = b
            .get("order")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .unwrap_or(*bi as f64);
        let primary = pa - pb;
        let primary = if primary.is_nan() || primary == 0.0 {
            0.0
        } else {
            primary
        };
        if primary != 0.0 {
            return primary.total_cmp(&0.0);
        }
        (oa - ob).total_cmp(&0.0)
    });
    let mut stages: Vec<&Value> = rows.into_iter().map(|(_, stage)| stage).collect();
    if !depth.is_empty() && depth != "full" {
        let allowed: Vec<&str> = graph
            .pointer(&format!("/depths/{}/stages", escape(depth)))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        stages.retain(|stage| {
            stage
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| allowed.contains(&id))
        });
    }
    stages
}

/// `state.depth || 'full'`.
pub fn depth_of(state: &Value) -> &str {
    state
        .get("depth")
        .and_then(Value::as_str)
        .filter(|depth| !depth.is_empty())
        .unwrap_or("full")
}

/// `state.mode ?? 'human'`.
pub fn mode_of(state: &Value) -> &str {
    match state.get("mode") {
        None | Some(Value::Null) => "human",
        Some(value) => value.as_str().unwrap_or("human"),
    }
}

pub fn stage_finished(status: Option<&str>) -> bool {
    matches!(status, Some("complete" | "skipped"))
}

/// Node `deriveStatus`.
pub fn derive_status(state: &Value, graph: &Value) -> &'static str {
    if state.pointer("/closed/kind").and_then(Value::as_str) == Some("abandoned") {
        return "abandoned";
    }
    let stages = ordered_stages(
        graph,
        state.get("scope").and_then(Value::as_str).unwrap_or(""),
        depth_of(state),
    );
    if !stages.is_empty()
        && stages.iter().all(|stage| {
            let id = stage.get("id").and_then(Value::as_str).unwrap_or("");
            stage_finished(
                state
                    .get("stages")
                    .and_then(|rows| rows.get(id))
                    .and_then(|row| row.get("status"))
                    .and_then(Value::as_str),
            )
        })
    {
        return "complete";
    }
    "active"
}

/// Node `normalise`: version 2, default dependencies, no `project`, string-or-null
/// `detail` on every stage row. Object-spread key positions are preserved.
pub fn normalise_state(state: Value) -> Value {
    let mut copy = match state {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    copy.insert("version".into(), json!(2));
    let dependencies = match copy.get("dependencies") {
        None | Some(Value::Null) => json!({}),
        Some(value) => value.clone(),
    };
    copy.insert("dependencies".into(), dependencies);
    let stages = match copy.get("stages") {
        Some(Value::Object(rows)) => rows.clone(),
        _ => serde_json::Map::new(),
    };
    let mut normalised = serde_json::Map::new();
    for (id, row) in stages {
        let mut row = match row {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        let detail = match row.get("detail") {
            Some(Value::String(text)) => json!(text),
            _ => Value::Null,
        };
        row.insert("detail".into(), detail);
        normalised.insert(id, Value::Object(row));
    }
    copy.insert("stages".into(), Value::Object(normalised));
    copy.shift_remove("project");
    Value::Object(copy)
}

fn js_blank(text: &str) -> bool {
    crate::identity::js_trim(text).is_empty()
}

/// Node `validateIntentState`; returns every problem in Node's order.
pub fn validate_intent_state(
    state: &Value,
    graph: &Value,
    folder: Option<&str>,
    check_id: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(map) = state.as_object() else {
        return vec!["not an object".into()];
    };
    if !map
        .get("intent")
        .and_then(Value::as_str)
        .is_some_and(|text| !js_blank(text))
    {
        errors.push("intent is not a non-empty string".into());
    }
    let scope = map.get("scope").and_then(Value::as_str);
    if !scope.is_some_and(|scope| graph.get("scopes").and_then(|s| s.get(scope)).is_some()) {
        errors.push("scope is not a key of graph.scopes".into());
    }
    if let Some(mode) = map.get("mode") {
        if !matches!(mode.as_str(), Some("human" | "ai")) {
            errors.push("mode is not ai or human".into());
        }
    }
    match map.get("stages") {
        Some(Value::Object(rows)) => {
            for (id, row) in rows {
                if !matches!(
                    row.get("status").and_then(Value::as_str),
                    Some("in_progress" | "complete" | "blocked" | "skipped")
                ) {
                    errors.push(format!("stage {id} has an invalid status"));
                }
            }
        }
        _ => errors.push("stages is not an object".into()),
    }
    if check_id {
        let id = map.get("intentId").and_then(Value::as_str).unwrap_or("");
        if !valid_intent_id(id) || folder.is_some_and(|folder| !folder.is_empty() && folder != id) {
            errors.push("intentId does not match folder".into());
        }
    }
    if let Some(current) = map.get("currentStage").filter(|value| !value.is_null()) {
        let in_scope = ordered_stages(graph, scope.unwrap_or(""), "full")
            .iter()
            .any(|stage| stage.get("id") == Some(current));
        if !in_scope {
            errors.push("currentStage is not a stage of the scope".into());
        }
    }
    if let Some(deps) = map.get("dependencies") {
        let valid = deps.as_object().is_some_and(|deps| {
            deps.values().all(|value| {
                value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
            })
        });
        if !valid {
            errors.push("dependencies is not an object of string arrays".into());
        }
    }
    errors
}

/// Node `loadIntent`, returning the normalised state (with derived `status`) and the
/// bytes that were read.
pub fn load_intent(root: &Path, graph: &Value, id: &str) -> WorkflowResult<(Value, Vec<u8>)> {
    let path = intent_state_path(root, id);
    let bytes = read_optional(&path)?;
    let Some(raw) = parse_strict(&path, bytes.as_deref())? else {
        return Err(format!(
            "Active intent '{id}' has no state file at .aidlc/intents/{id}/state.json. No changes were made."
        ));
    };
    let mut state = normalise_state(raw);
    let problems = validate_intent_state(&state, graph, Some(id), true);
    if let Some(problem) = problems.first() {
        return Err(format!(
            ".aidlc/intents/{id}/state.json is invalid: {problem}. No changes were made."
        ));
    }
    state["status"] = json!(derive_status(&state, graph));
    Ok((state, bytes.unwrap_or_default()))
}

/// Node `loadWorkflow(root, graph, { migrate })`. Legacy v1 state is migrated through
/// [`crate::state_migration::migrate_legacy_state`] when `migrate` is set.
pub fn load_workflow(root: &Path, graph: &Value, migrate: bool) -> WorkflowResult<LoadedWorkflow> {
    let (pointer, pointer_bytes) = read_pointer(root);
    match pointer {
        Pointer::Absent => Ok(LoadedWorkflow::default()),
        Pointer::Corrupt(reason) => Err(corrupt_message(reason, root)),
        Pointer::Active(None) => Ok(LoadedWorkflow {
            pointer_bytes,
            ..LoadedWorkflow::default()
        }),
        Pointer::Active(Some(id)) => match load_intent(root, graph, &id) {
            Ok((state, bytes)) => {
                if derive_status(&state, graph) == "abandoned" {
                    return Ok(LoadedWorkflow {
                        pointer_bytes,
                        ..LoadedWorkflow::default()
                    });
                }
                Ok(LoadedWorkflow {
                    active_intent_id: Some(id),
                    state: Some(state),
                    pointer_bytes,
                    state_bytes: Some(bytes),
                })
            }
            Err(error) if error.contains("has no state file") => Err(format!(
                "{error} Run 'dtech-aidlc intent list', then 'dtech-aidlc intent resume <id>' or 'dtech-aidlc intent pause'."
            )),
            Err(error) => Err(error),
        },
        Pointer::Legacy(raw) => {
            let legacy = normalise_state(raw);
            let check_id = legacy
                .get("intentId")
                .is_some_and(|id| !matches!(id, Value::Null | Value::Bool(false)) && id != "");
            let folder = legacy.get("intentId").and_then(Value::as_str);
            let problems = validate_intent_state(&legacy, graph, folder, check_id);
            if let Some(problem) = problems.first() {
                return Err(format!(
                    ".aidlc/state.json is invalid: {problem}. No changes were made."
                ));
            }
            if !migrate {
                return Ok(LoadedWorkflow {
                    active_intent_id: folder.map(str::to_owned),
                    state: Some(legacy),
                    pointer_bytes,
                    state_bytes: None,
                });
            }
            let migration = crate::state_migration::migrate_legacy_state(root, graph)?;
            let (pointer_bytes, state_bytes) = (
                read_optional(&state_path(root))?,
                read_optional(&intent_state_path(root, &migration.intent_id))?,
            );
            Ok(LoadedWorkflow {
                active_intent_id: Some(migration.intent_id),
                state: Some(normalise_state(migration.state)),
                pointer_bytes,
                state_bytes,
            })
        }
    }
}

// Hangul syllable arithmetic (Unicode 3.12), used by NFKD folding and NFC.
const HANGUL_S_BASE: u32 = 0xAC00;
const HANGUL_L_BASE: u32 = 0x1100;
const HANGUL_V_BASE: u32 = 0x1161;
const HANGUL_T_BASE: u32 = 0x11A7;
const HANGUL_L_COUNT: u32 = 19;
const HANGUL_V_COUNT: u32 = 21;
const HANGUL_T_COUNT: u32 = 28;
const HANGUL_N_COUNT: u32 = HANGUL_V_COUNT * HANGUL_T_COUNT;
const HANGUL_S_COUNT: u32 = HANGUL_L_COUNT * HANGUL_N_COUNT;

fn is_hangul_syllable(cp: u32) -> bool {
    (HANGUL_S_BASE..HANGUL_S_BASE + HANGUL_S_COUNT).contains(&cp)
}

fn range_row<T>(rows: &[T], cp: u32, bounds: impl Fn(&T) -> (u32, u32)) -> Option<&T> {
    rows.binary_search_by(|row| {
        let (first, last) = bounds(row);
        if last < cp {
            std::cmp::Ordering::Less
        } else if first > cp {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .ok()
    .map(|index| &rows[index])
}

/// `/[\p{L}\p{N}]/u` as evaluated by the Node the tables were generated from.
fn is_letter_or_number(cp: u32) -> bool {
    range_row(unicode_tables::LETTER_OR_NUMBER, cp, |row| *row).is_some()
}

/// Fold one character as Node `intentSlug` does (`NFKD`, strip U+0300..U+036F,
/// `đ/Đ -> d`, `toLowerCase`), appending its projection (`[a-z0-9]`, any other run as
/// `-`) to `out`. Returns how many `\p{L}\p{N}` characters the folded form holds.
fn slug_fold_into(c: char, out: &mut String) -> usize {
    let cp = c as u32;
    if c.is_ascii() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            return 1;
        }
        out.push('-');
        return 0;
    }
    if is_hangul_syllable(cp) {
        // NFKD gives two or three conjoining jamo, all letters.
        out.push('-');
        return if (cp - HANGUL_S_BASE).is_multiple_of(HANGUL_T_COUNT) {
            2
        } else {
            3
        };
    }
    if let Some(&(first, _, projection, letters, step)) =
        range_row(unicode_tables::SLUG_FOLD, cp, |row| (row.0, row.1))
    {
        if step {
            out.push(char::from(projection.as_bytes()[0] + (cp - first) as u8));
        } else {
            out.push_str(projection);
        }
        return usize::from(letters);
    }
    // No decomposition: the character folds to itself.
    if !(0x0300..=0x036F).contains(&cp) {
        out.push('-');
    }
    usize::from(is_letter_or_number(cp))
}

/// The values Node `intentSlug` decides on: `(base, kept, letters)`, where `base` is the
/// folded text as `[a-z0-9]` words joined by `-` (at most 48 bytes), `kept` counts
/// `[a-z0-9]` and `letters` counts `\p{L}\p{N}` in the folded text. Exposed for the golden
/// tests.
#[doc(hidden)]
pub fn intent_slug_parts(text: &str) -> (String, usize, usize) {
    let mut folded = String::with_capacity(text.len());
    let mut letters = 0usize;
    for c in text.chars() {
        letters += slug_fold_into(c, &mut folded);
    }
    let kept = folded.bytes().filter(u8::is_ascii_alphanumeric).count();
    // folded.replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48).replace(/-+$/, '')
    let mut base = folded
        .split('-')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    base.truncate(48);
    while base.ends_with('-') {
        base.pop();
    }
    (base, kept, letters)
}

/// Node `intentSlug`: `NFKD` folding to `[a-z0-9-]`, or `intent-<yyyymmdd>-<hex>` (SHA-256
/// of the NFC form) when fewer than three or less than half of the letters survive.
/// Unicode data comes from the Node release recorded in `unicode_tables.rs`.
pub fn intent_slug(text: &str) -> String {
    let (base, kept, letters) = intent_slug_parts(text);
    if kept >= 3 && kept * 2 >= letters && !base.is_empty() {
        return base;
    }
    use sha2::{Digest, Sha256};
    let day = crate::audit::now_iso()[0..10].replace('-', "");
    let digest = Sha256::digest(nfc(text).as_bytes());
    format!(
        "intent-{day}-{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2]
    )
}

fn combining_rank(c: char) -> u16 {
    range_row(unicode_tables::COMBINING_RANK, c as u32, |row| {
        (row.0, row.1)
    })
    .map_or(0, |row| row.2)
}

fn decompose_canonical(c: char, out: &mut Vec<char>) {
    let cp = c as u32;
    if is_hangul_syllable(cp) {
        let index = cp - HANGUL_S_BASE;
        let jamo = [
            HANGUL_L_BASE + index / HANGUL_N_COUNT,
            HANGUL_V_BASE + (index % HANGUL_N_COUNT) / HANGUL_T_COUNT,
            HANGUL_T_BASE + index % HANGUL_T_COUNT,
        ];
        let count = if jamo[2] == HANGUL_T_BASE { 2 } else { 3 };
        out.extend(jamo[..count].iter().filter_map(|&cp| char::from_u32(cp)));
        return;
    }
    match unicode_tables::CANONICAL_DECOMPOSITION.binary_search_by_key(&cp, |row| row.0) {
        Ok(index) => out.extend(unicode_tables::CANONICAL_DECOMPOSITION[index].1.chars()),
        Err(_) => out.push(c),
    }
}

fn compose_pair(first: char, second: char) -> Option<char> {
    let (a, b) = (first as u32, second as u32);
    if (HANGUL_L_BASE..HANGUL_L_BASE + HANGUL_L_COUNT).contains(&a)
        && (HANGUL_V_BASE..HANGUL_V_BASE + HANGUL_V_COUNT).contains(&b)
    {
        let index = (a - HANGUL_L_BASE) * HANGUL_N_COUNT + (b - HANGUL_V_BASE) * HANGUL_T_COUNT;
        return char::from_u32(HANGUL_S_BASE + index);
    }
    if is_hangul_syllable(a)
        && (a - HANGUL_S_BASE).is_multiple_of(HANGUL_T_COUNT)
        && (HANGUL_T_BASE + 1..HANGUL_T_BASE + HANGUL_T_COUNT).contains(&b)
    {
        return char::from_u32(a + (b - HANGUL_T_BASE));
    }
    unicode_tables::COMPOSITION
        .binary_search_by(|row| (row.0, row.1).cmp(&(a, b)))
        .ok()
        .and_then(|index| char::from_u32(unicode_tables::COMPOSITION[index].2))
}

/// `String.prototype.normalize('NFC')` (canonical decomposition, canonical ordering,
/// canonical composition) with the Unicode data in `unicode_tables.rs`. Exposed for the
/// golden tests of [`intent_slug`]'s fallback hash.
#[doc(hidden)]
pub fn nfc(text: &str) -> String {
    let mut chars = Vec::with_capacity(text.len());
    for c in text.chars() {
        decompose_canonical(c, &mut chars);
    }
    // Canonical ordering: stable sort of every run of non-starters by class.
    let mut index = 0;
    while index < chars.len() {
        if combining_rank(chars[index]) == 0 {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && combining_rank(chars[index]) != 0 {
            index += 1;
        }
        chars[start..index].sort_by_key(|&c| combining_rank(c));
    }
    // Canonical composition.
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut starter: Option<usize> = None;
    for c in chars {
        let rank = combining_rank(c);
        if let Some(position) = starter {
            let adjacent = position + 1 == out.len();
            let blocked = !adjacent
                && out.last().is_some_and(|&last| {
                    let last_rank = combining_rank(last);
                    last_rank == 0 || last_rank >= rank
                });
            if !blocked {
                if let Some(composite) = compose_pair(out[position], c) {
                    out[position] = composite;
                    continue;
                }
            }
        }
        if rank == 0 {
            starter = Some(out.len());
        }
        out.push(c);
    }
    out.into_iter().collect()
}

/// Node `freeIntentId`: the base id, or the first free `base-N` (N >= 2), skipping both
/// intent folders and knowledge records.
pub fn free_intent_id(root: &Path, base: &str) -> WorkflowResult<String> {
    let taken = |id: &str| -> WorkflowResult<bool> {
        Ok(path_taken(&intent_dir_path(root, id))?
            || path_taken(
                &root
                    .join(".aidlc")
                    .join("knowledge")
                    .join(format!("{id}.md")),
            )?)
    };
    if !taken(base)? {
        return Ok(base.to_owned());
    }
    let mut n = 2;
    while taken(&format!("{base}-{n}"))? {
        n += 1;
    }
    Ok(format!("{base}-{n}"))
}

/// Node `assertNoSymlinkComponents(root, relative)`.
pub fn assert_no_symlink_components(root: &Path, relative: &str) -> WorkflowResult<()> {
    let mut current = root.to_path_buf();
    let mut walked = Vec::new();
    for part in relative.split(['/', '\\']).filter(|part| !part.is_empty()) {
        current.push(part);
        walked.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing to write through symlink: {}",
                    walked.join("/")
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ioerr(error)),
        }
    }
    Ok(())
}

fn transient_rename_error(error: &std::io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// Node `writeFileAtomic`: refuse a symlinked target, write a unique temp file beside
/// it (`<file>.<pid>.<unique>.tmp`), then rename over the target. The temp file is
/// removed on failure; the target is never deleted first.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> WorkflowResult<()> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(format!(
            "Refusing to write through symlink: {}",
            path.display()
        ));
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or("atomic write target has no file name")?
        .to_string_lossy()
        .into_owned();
    for _ in 0..100 {
        let unique = format!(
            "{:x}{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let tmp = path.with_file_name(format!("{name}.{}.{unique}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&tmp) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("{}: {error}", tmp.display())),
        };
        let result = (|| {
            file.write_all(bytes).map_err(ioerr)?;
            file.sync_all().map_err(ioerr)?;
            drop(file);
            let mut attempt = 0;
            loop {
                match fs::rename(&tmp, path) {
                    Ok(()) => return Ok(()),
                    Err(error) if transient_rename_error(&error) && attempt < 20 => {
                        attempt += 1;
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(error) => return Err(format!("{}: {error}", path.display())),
                }
            }
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        return result;
    }
    Err("Could not allocate a unique temporary file".into())
}

/// Node `writeJsonAtomic`: `JSON.stringify(value, null, 2) + "\n"`.
pub fn write_json_atomic(path: &Path, value: &Value) -> WorkflowResult<()> {
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    write_file_atomic(path, text.as_bytes())
}

/// Node `saveIntent(root, state, graph)`: derive `status`, stamp `updatedAt`, create the
/// intent folder and write its `state.json`.
pub fn save_intent(root: &Path, state: &mut Value, graph: Option<&Value>) -> WorkflowResult<()> {
    let id = state
        .get("intentId")
        .and_then(Value::as_str)
        .filter(|id| valid_intent_id(id))
        .ok_or("intentId is not a safe identifier")?
        .to_owned();
    if let Some(graph) = graph {
        let status = derive_status(state, graph);
        state["status"] = json!(status);
    }
    state["updatedAt"] = json!(crate::audit::now_iso());
    assert_no_symlink_components(root, &format!(".aidlc/intents/{id}/state.json"))?;
    fs::create_dir_all(intent_dir_path(root, &id)).map_err(ioerr)?;
    assert_no_symlink_components(root, &format!(".aidlc/intents/{id}/state.json"))?;
    write_json_atomic(&intent_state_path(root, &id), state)
}

/// Node `savePointer(root, activeIntentId)`.
pub fn save_pointer(root: &Path, active_intent_id: Option<&str>) -> WorkflowResult<()> {
    assert_no_symlink_components(root, ".aidlc/state.json")?;
    fs::create_dir_all(root.join(".aidlc")).map_err(ioerr)?;
    write_json_atomic(
        &state_path(root),
        &json!({"version": 2, "activeIntentId": active_intent_id, "updatedAt": crate::audit::now_iso()}),
    )
}

/// Node `writeIntentMarkdown`: `# <id>\n\n<intent>\n`, created once (never replaced).
pub fn write_intent_markdown(root: &Path, id: &str, intent: &str) -> WorkflowResult<()> {
    assert_no_symlink_components(root, &format!(".aidlc/intents/{id}/intent.md"))?;
    let path = intent_dir_path(root, id).join("intent.md");
    if path_taken(&path)? {
        return Ok(());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(format!("# {id}\n\n{intent}\n").as_bytes())
        .map_err(ioerr)
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
    fn drafting_an_intent_does_not_write_project_state() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-draft-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let workflow = Workflow::new(&root, graph());
        let draft = workflow
            .draft("new intent", "solo", "human", "full")
            .unwrap();
        assert_eq!(draft["intentId"], "new-intent");
        let timestamp = draft["createdAt"].as_str().unwrap();
        assert_eq!(timestamp.len(), 24);
        assert!(timestamp.ends_with('Z'));
        assert!(!root.join(".aidlc").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn draft_normalizes_intent_and_avoids_knowledge_id_collision() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-draft-collision-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
        fs::write(root.join(".aidlc/knowledge/new-intent.md"), "existing").unwrap();
        let workflow = Workflow::new(&root, graph());
        let draft = workflow
            .draft("\u{feff}new intent\r\nline", "solo", "human", "full")
            .unwrap();
        assert_eq!(draft["intent"], "new intent\nline");
        let collision = workflow
            .draft("new intent", "solo", "human", "full")
            .unwrap();
        assert_eq!(collision["intentId"], "new-intent-2");
        assert!(workflow
            .draft("bad\0intent", "solo", "human", "full")
            .is_err());
        assert!(!root.join(".aidlc/state.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persist_updates_existing_active_intent_without_replacing_pointer() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-persist-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let workflow = Workflow::new(&root, graph());
        let mut state = workflow
            .create("persist intent", "solo", "human", "full")
            .unwrap();
        state["note"] = json!("updated");
        workflow.persist(&state).unwrap();
        let loaded = workflow.load().unwrap();
        assert!(
            matches!(loaded, LoadedState::Active { state, .. } if state["note"] == "updated" && state["updatedAt"].as_str().is_some_and(|value| value.ends_with('Z')))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_intent_update_leaves_pointer_bytes_unchanged() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-persist-pointer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let workflow = Workflow::new(&root, graph());
        let mut state = workflow
            .create("persist intent", "solo", "human", "full")
            .unwrap();
        let pointer_path = root.join(".aidlc/state.json");
        let before = fs::read(&pointer_path).unwrap();
        state["note"] = json!("updated");

        workflow.persist(&state).unwrap();

        assert_eq!(fs::read(&pointer_path).unwrap(), before);
        assert!(matches!(
            workflow.load().unwrap(),
            LoadedState::Active { id, state: loaded }
                if id == "persist-intent"
                    && loaded["note"] == "updated"
                    && loaded["updatedAt"].as_str().is_some_and(|value| value.ends_with('Z'))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persist_fails_closed_when_pointer_does_not_match_or_is_unsupported() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-persist-fail-closed-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let workflow = Workflow::new(&root, graph());
        let active = workflow
            .create("active one", "solo", "human", "full")
            .unwrap();
        let active_path = workflow.intent_state("active-one");
        let original_state = fs::read(&active_path).unwrap();
        let pointer_path = workflow.pointer();
        let original_pointer = fs::read(&pointer_path).unwrap();

        let mut other = active.clone();
        other["intentId"] = json!("other");
        assert!(workflow
            .persist(&other)
            .unwrap_err()
            .contains("not 'other'"));
        assert_eq!(fs::read(&active_path).unwrap(), original_state);
        assert_eq!(fs::read(&pointer_path).unwrap(), original_pointer);

        fs::write(&pointer_path, br#"{"version":2,"activeIntentId":null}"#).unwrap();
        let paused_pointer = fs::read(&pointer_path).unwrap();
        assert!(workflow
            .persist(&active)
            .unwrap_err()
            .contains("No active intent pointer"));
        assert_eq!(fs::read(&pointer_path).unwrap(), paused_pointer);
        assert_eq!(fs::read(&active_path).unwrap(), original_state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_pointer_blocks_creation_before_writing_intent() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-create-pointer-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let workflow = Workflow::new(&root, graph());
        let draft = workflow
            .draft("new intent", "solo", "human", "full")
            .unwrap();
        fs::create_dir(root.join(".aidlc/state.json")).unwrap();

        assert!(workflow.persist_new(&draft).is_err());
        assert!(!workflow.intent_state("new-intent").exists());
        assert!(root.join(".aidlc/state.json").is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_atomic_write_cleans_its_temporary_file() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-atomic-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("state.json")).unwrap();
        assert!(atomic_json(&root.join("state.json"), &json!({"value": 1})).is_err());
        let entries: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|row| row.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "state.json");
        fs::remove_dir_all(root).unwrap();
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
    fn report_requires_detail_and_current_stage() {
        let w = Workflow::new(".", graph());
        let mut s = state();
        assert!(w
            .report(&mut s, "a1", "blocked", None, None, None)
            .unwrap_err()
            .contains("requires detail"));
        assert!(w
            .report(&mut s, "a1", "skipped", Some("not needed"), None, None)
            .unwrap_err()
            .contains("Only the current stage"));
    }

    #[test]
    fn complete_report_validates_expected_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "aidlc-workflow-report-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut g = graph();
        g["outputs"] = json!({"design":{"kind":"document"}});
        g["stages"][0]["produces"] = json!(["design"]);
        let w = Workflow::new(&root, g.clone());
        let mut s = state();
        s["intentId"] = json!("intent-1");
        s["currentStage"] = json!("a1");
        s["stages"]["a1"] = json!({"status":"in_progress"});
        assert!(w
            .report(&mut s, "a1", "complete", None, None, None)
            .unwrap_err()
            .contains("cannot be completed"));
        let artifact = root.join(".aidlc/intents/intent-1/a/a1/design.md");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, "Design accepted").unwrap();
        w.report(&mut s, "a1", "complete", None, None, None)
            .unwrap();
        assert_eq!(s["stages"]["a1"]["status"], "complete");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_state_is_rejected() {
        assert!(validate_state(&json!({}), &graph(), None).is_err());
    }

    fn node_temp(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aidlc-node-{label}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        root
    }

    #[test]
    fn intent_slug_matches_node_for_ascii_latin_and_vietnamese() {
        assert_eq!(
            intent_slug("Build a sample feature"),
            "build-a-sample-feature"
        );
        assert_eq!(intent_slug("  Café -- Déjà vu!  "), "cafe-deja-vu");
        assert_eq!(
            intent_slug("Xây dựng tính năng đăng nhập"),
            "xay-dung-tinh-nang-dang-nhap"
        );
        let long = intent_slug(&"word ".repeat(30));
        assert!(long.len() <= 48 && !long.ends_with('-'));
        let short = intent_slug("ab");
        assert!(short.starts_with("intent-") && short.len() == "intent-20260101-abcdef".len());
        // Mostly non-Latin letters fall back to the dated hash form, as in Node.
        assert!(intent_slug("构建一个示例功能 ab").starts_with("intent-"));
    }

    /// Regression: Latin Extended-A, full-width Latin, ligatures, number forms and
    /// decomposed marks must fold (NFKD) and hash (NFC) exactly as Node `intentSlug`.
    /// Expected values were produced by `core/tools/workflow.mjs` on Node 24.
    #[test]
    fn intent_slug_matches_node_nfkd_folding_and_nfc_hash() {
        for (text, expected) in [
            (
                "Opravit chybu v přihlášení uživatele",
                "opravit-chybu-v-prihlaseni-uzivatele",
            ),
            (
                "Naprawić błąd logowania w źródle",
                "naprawic-b-ad-logowania-w-zrodle",
            ),
            ("ＡＰＩ ｒｅｆａｃｔｏｒ", "api-refactor"),
            ("ﬁle ﬂow Ⅻ ①②③ test", "file-flow-xii-123-test"),
            ("İstanbul ǅemal", "istanbul-dzemal"),
        ] {
            assert_eq!(intent_slug(text), expected, "{text}");
        }
        let day = crate::audit::now_iso()[0..10].replace('-', "");
        // Decomposed and precomposed input hash the same NFC text.
        assert_eq!(
            intent_slug("构建 e\u{0301}"),
            format!("intent-{day}-cb4957")
        );
        assert_eq!(intent_slug("构建 é"), format!("intent-{day}-cb4957"));
        // Hangul syllables count as two or three letters after NFKD.
        assert_eq!(intent_slug("API 설계"), format!("intent-{day}-3981ad"));
        assert_eq!(intent_slug("Ελληνικά abc"), format!("intent-{day}-cfa29f"));
        assert_eq!(nfc("e\u{0301}\u{0323}"), "\u{1EB9}\u{0301}");
        assert_eq!(nfc("\u{1100}\u{1161}\u{11A8}"), "\u{AC01}");
        assert_eq!(nfc("\u{212B}"), "\u{00C5}");
    }

    #[test]
    fn normalise_and_validate_follow_node() {
        let state = normalise_state(json!({
            "intentId":"x","intent":"t","scope":"solo","project":"p",
            "stages":{"a1":{"status":"complete","detail":5},"a2":{"status":"in_progress","detail":"why"}}
        }));
        assert!(state.get("project").is_none());
        assert_eq!(state["version"], 2);
        assert_eq!(state["dependencies"], json!({}));
        assert_eq!(state["stages"]["a1"]["detail"], Value::Null);
        assert_eq!(state["stages"]["a2"]["detail"], "why");
        assert!(validate_intent_state(&state, &graph(), Some("x"), true).is_empty());
        let keys: Vec<&String> = state.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "intentId",
                "intent",
                "scope",
                "stages",
                "version",
                "dependencies"
            ]
        );

        let mut bad = state.clone();
        bad["mode"] = Value::Null;
        bad["currentStage"] = json!("nope");
        bad["stages"]["a1"]["status"] = json!("done");
        assert_eq!(
            validate_intent_state(&bad, &graph(), Some("y"), true),
            [
                "mode is not ai or human",
                "stage a1 has an invalid status",
                "intentId does not match folder",
                "currentStage is not a stage of the scope"
            ]
        );
        assert_eq!(
            validate_intent_state(&json!([]), &graph(), None, true),
            ["not an object"]
        );
    }

    #[test]
    fn derive_status_and_ordered_stages_follow_node() {
        let g = graph();
        let ids = |depth: &str| {
            ordered_stages(&g, "solo", depth)
                .iter()
                .map(|s| s["id"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("full"), ["a1", "a2", "b1"]);
        assert!(ids("missing-depth").is_empty());
        let mut s = state();
        assert_eq!(derive_status(&s, &g), "active");
        for id in ["a1", "a2", "b1"] {
            s["stages"][id] = json!({"status":"skipped"});
        }
        assert_eq!(derive_status(&s, &g), "complete");
        s["closed"] = json!({"kind":"abandoned"});
        assert_eq!(derive_status(&s, &g), "abandoned");
    }

    #[test]
    fn read_pointer_classifies_like_node() {
        let root = node_temp("pointer");
        assert_eq!(read_pointer(&root).0, Pointer::Absent);
        let path = state_path(&root);
        for (bytes, expected) in [
            (
                &br#"{"version":2,"activeIntentId":null}"#[..],
                Pointer::Active(None),
            ),
            (
                br#"{"version":2,"activeIntentId":"abc"}"#,
                Pointer::Active(Some("abc".into())),
            ),
            (
                br#"{"version":2,"activeIntentId":"../x"}"#,
                Pointer::Corrupt("is not a recognised state file"),
            ),
            (
                br#"{"stages":[]}"#,
                Pointer::Corrupt("is not a recognised state file"),
            ),
            (b"{oops", Pointer::Corrupt("cannot be parsed")),
        ] {
            fs::write(&path, bytes).unwrap();
            assert_eq!(read_pointer(&root).0, expected);
        }
        fs::write(&path, br#"{"stages":{}}"#).unwrap();
        assert!(matches!(read_pointer(&root).0, Pointer::Legacy(_)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn node_writers_use_node_format_and_leave_no_temp_files() {
        let root = node_temp("writers");
        let mut state =
            json!({"version":2,"intentId":"my-id","intent":"t","scope":"solo","stages":{}});
        save_intent(&root, &mut state, Some(&graph())).unwrap();
        save_pointer(&root, Some("my-id")).unwrap();
        write_intent_markdown(&root, "my-id", "t").unwrap();
        write_intent_markdown(&root, "my-id", "changed").unwrap();
        let text = fs::read_to_string(intent_state_path(&root, "my-id")).unwrap();
        assert!(text.ends_with("}\n") && text.contains("\n  \"status\": \"active\""));
        assert_eq!(
            fs::read_to_string(intent_dir_path(&root, "my-id").join("intent.md")).unwrap(),
            "# my-id\n\nt\n"
        );
        let loaded = load_workflow(&root, &graph(), false).unwrap();
        assert_eq!(loaded.active_intent_id.as_deref(), Some("my-id"));
        assert_eq!(loaded.state.unwrap()["status"], "active");
        let leftovers = fs::read_dir(intent_dir_path(&root, "my-id"))
            .unwrap()
            .chain(fs::read_dir(root.join(".aidlc")).unwrap())
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        assert_eq!(free_intent_id(&root, "my-id").unwrap(), "my-id-2");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writers_refuse_symlinked_targets() {
        let root = node_temp("symlink");
        let outside = node_temp("symlink-outside");
        fs::create_dir_all(root.join(".aidlc/intents")).unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, root.join(".aidlc/intents/linked"));
        #[cfg(windows)]
        let linked =
            std::os::windows::fs::symlink_dir(&outside, root.join(".aidlc/intents/linked"))
                .or_else(|_| {
                    // Without symlink rights a directory junction is the reparse point to test.
                    let status = std::process::Command::new("cmd")
                        .args(["/C", "mklink", "/J"])
                        .arg(root.join(".aidlc").join("intents").join("linked"))
                        .arg(&outside)
                        .output()?
                        .status;
                    if status.success() {
                        Ok(())
                    } else {
                        Err(std::io::Error::other("mklink /J failed"))
                    }
                });
        if linked.is_err() {
            eprintln!("skipping: this account cannot create symlinks");
        } else {
            let mut state = json!({"intentId":"linked","intent":"t","scope":"solo","stages":{}});
            let error = save_intent(&root, &mut state, None).unwrap_err();
            assert!(
                error.contains("Refusing to write through symlink"),
                "{error}"
            );
            assert_eq!(fs::read_dir(&outside).unwrap().count(), 1); // only its own .aidlc
        }
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
