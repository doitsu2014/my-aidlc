//! Native `orchestrate next` / `orchestrate report`, ported from Node's
//! `orchestrateNext` / `orchestrateReport` (`core/tools/workflow.mjs`) and the CLI
//! wrapper in `core/tools/dtech-aidlc.mjs`.
//!
//! Validation order, error texts, directive/report shapes, persisted state and audit
//! rows follow Node. Deliberate safety differences:
//! - Nothing is written until every check has passed. Node resolves the `depth.chosen`
//!   approver after writing a new intent; here an invalid `--approved-by` fails first.
//! - Before writing, the pointer (and, for an active intent, its `state.json`) must still
//!   hold the bytes read at the start of the command; otherwise nothing is written.
//! - An update of the active intent rewrites only its `state.json`. Node also rewrites
//!   `.aidlc/state.json` with the same `activeIntentId` and a fresh `updatedAt`; here the
//!   pointer is left untouched. A new intent is written as Node does it: the intent
//!   folder, `intent.md` (once), its `state.json`, then the pointer, then audit rows.
//! - Methodology file paths point at the engine in `DTECH_AIDLC_HOME` when set, otherwise
//!   at the embedded methodology materialized under `<project>/.aidlc/engine`.
//!
//! Legacy (v1) state is migrated on load, as Node's `loadWorkflow({ migrate: true })`,
//! through [`crate::state_migration::migrate_legacy_state`].

use crate::artifacts::{expected_artifacts, verify_artifacts, ExpectedArtifact, VerifiedArtifact};
use crate::workflow::{
    depth_of, derive_status, free_intent_id, intent_dir_path, intent_ids_on_disk, intent_slug,
    load_intent, load_workflow, mode_of, ordered_stages, read_pointer, save_intent, save_pointer,
    stage_finished, state_path, valid_intent_id, write_intent_markdown, LoadedWorkflow, Pointer,
};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub type OrchestrateResult<T> = Result<T, String>;

pub const AI_ORCHESTRATOR: &str = "AI Orchestrator";
pub const DEFAULT_UNIT_SPLIT_THRESHOLD: i64 = 6;
/// Process exit code for a `next` that is held back by a failing prerequisite.
pub const BLOCKED_EXIT_CODE: u8 = 3;

const PROJECTION_HISTORY: &str = include_str!("../core/data/projection-history.json");

/// Where directive methodology paths (`contractsFile`, `phaseFile`, templates) point.
#[derive(Clone, Debug)]
pub struct Engine {
    root: PathBuf,
    materialize: bool,
}

impl Engine {
    /// The embedded methodology, materialized under `<project>/.aidlc/engine`.
    pub fn embedded(project_root: &Path) -> Self {
        Engine {
            root: project_root.join(".aidlc").join("engine"),
            materialize: true,
        }
    }

    /// An installed engine root (the directory holding `methodology/`).
    pub fn external(engine_root: impl Into<PathBuf>) -> Self {
        Engine {
            root: engine_root.into(),
            materialize: false,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Node `join(engineRoot(), relative)`.
    pub fn path(&self, relative: &str) -> String {
        let mut path = self.root.clone();
        for part in relative.split(['/', '\\']).filter(|part| !part.is_empty()) {
            path.push(part);
        }
        path.to_string_lossy().into_owned()
    }

    fn prepare(&self) -> OrchestrateResult<()> {
        if self.materialize {
            let project_root = self
                .root
                .parent()
                .and_then(Path::parent)
                .ok_or("Invalid embedded engine path")?;
            materialize_embedded_engine(project_root)?;
        }
        Ok(())
    }
}

/// Files the CLI keeps in `<project>/.aidlc/engine` besides the methodology.
const ENGINE_GITIGNORE: &str = ".gitignore";
const ENGINE_VERSION: &str = ".engine-version";
const ENGINE_GITIGNORE_BODY: &str =
    "# Written by dtech-aidlc from its embedded methodology; regenerated on demand.\n*\n";

/// Write the CLI's embedded methodology to `<project>/.aidlc/engine` and return the
/// canonical engine root.
///
/// The folder belongs to the CLI: methodology files whose bytes differ from the embedded
/// ones (an older or newer CLI wrote them, or a checkout changed line endings) are
/// replaced, and `.engine-version` records the CLI version and a digest of what was
/// written. A `.gitignore` holding `*` keeps the folder out of version control. Like
/// Node's `assertSafeStateDir`, only `.aidlc` and the folders below it must be real
/// directories (not symbolic links or junctions); folders above the project are not
/// checked, so a project reached through a linked path works. A methodology target that
/// is a symbolic link or not a regular file is refused, never followed or replaced.
pub fn materialize_embedded_engine(project_root: &Path) -> OrchestrateResult<PathBuf> {
    use sha2::{Digest, Sha256};
    let state_dir = project_root.join(".aidlc");
    let engine_root = state_dir.join("engine");
    ensure_real_directory(&state_dir)?;
    ensure_real_directory(&engine_root)?;
    write_engine_file(
        &engine_root.join(ENGINE_GITIGNORE),
        ENGINE_GITIGNORE_BODY.as_bytes(),
    )?;
    let mut digest = Sha256::new();
    for (name, bytes) in crate::scope::embedded_methodology_assets() {
        let parts = name.split('/').collect::<Vec<_>>();
        let unsafe_part = |part: &&str| {
            part.is_empty() || *part == "." || *part == ".." || part.contains(['\\', ':'])
        };
        if parts.len() < 2 || parts[0] != "methodology" || parts.iter().any(unsafe_part) {
            return Err(format!("Invalid embedded methodology asset name: {name}"));
        }
        let mut target = engine_root.clone();
        for directory in &parts[..parts.len() - 1] {
            target.push(directory);
            ensure_real_directory(&target)?;
        }
        target.push(parts[parts.len() - 1]);
        write_engine_file(&target, bytes)?;
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    let version = format!(
        "dtech-aidlc {}\nmethodology-sha256 {}\n",
        env!("CARGO_PKG_VERSION"),
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    write_engine_file(&engine_root.join(ENGINE_VERSION), version.as_bytes())?;
    fs::canonicalize(&engine_root).map_err(|error| format!("{}: {error}", engine_root.display()))
}

fn engine_refusal(path: &Path, what: &str) -> String {
    format!(
        "Refusing {what}: {}. .aidlc/engine is written by dtech-aidlc; remove it and run the command again to regenerate it.",
        path.display()
    )
}

/// Create `path` if missing; refuse a symbolic link, junction or non-directory.
fn ensure_real_directory(path: &Path) -> OrchestrateResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(engine_refusal(path, "symlink or non-directory path"));
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(engine_refusal(path, "symlink or non-directory path"));
    }
    Ok(())
}

/// Leave a regular file holding `bytes` alone; otherwise write `bytes` to a temporary
/// file in the same folder and rename it over the target (which replaces the target
/// itself, never what a link would point at).
fn write_engine_file(target: &Path, bytes: &[u8]) -> OrchestrateResult<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(engine_refusal(
                target,
                "non-regular methodology asset target",
            ));
        }
        Ok(_) => {
            if fs::read(target).map_err(|error| format!("{}: {error}", target.display()))? == bytes
            {
                return Ok(());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{}: {error}", target.display())),
    }
    let parent = target.parent().ok_or("Invalid methodology asset target")?;
    let temp = parent.join(format!(
        ".aidlc-engine-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, target)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(format!("{}: {error}", target.display()));
    }
    Ok(())
}

/// Options of `orchestrate next`, already validated by the CLI layer.
#[derive(Clone, Debug, Default)]
pub struct NextOptions {
    /// Normalized intent text (see [`normalize_intent_text`]).
    pub intent: Option<String>,
    pub scope: Option<String>,
    pub depth: Option<String>,
    pub mode: Option<String>,
    pub approved_by: Option<String>,
    pub detail: Option<String>,
    pub force_resume: bool,
}

/// Options of `orchestrate report`.
#[derive(Clone, Debug, Default)]
pub struct ReportOptions {
    pub stage: Option<String>,
    pub result: Option<String>,
    pub approval: Option<String>,
    pub detail: Option<String>,
    pub approved_by: Option<String>,
    pub approval_note: Option<String>,
    pub needs: Vec<String>,
}

/// What `orchestrate next` produced.
#[derive(Clone, Debug, PartialEq)]
pub enum NextOutcome {
    /// A stage was selected and persisted.
    Directive(Value),
    /// Every stage is finished (Node `completePayload`).
    Complete(Value),
    /// A prerequisite check is failing; exit with [`BLOCKED_EXIT_CODE`].
    Blocked(Value),
}

impl NextOutcome {
    pub fn value(&self) -> &Value {
        match self {
            NextOutcome::Directive(value)
            | NextOutcome::Complete(value)
            | NextOutcome::Blocked(value) => value,
        }
    }
    pub fn into_value(self) -> Value {
        match self {
            NextOutcome::Directive(value)
            | NextOutcome::Complete(value)
            | NextOutcome::Blocked(value) => value,
        }
    }
}

// ---------------------------------------------------------------------------
// Small JavaScript-semantics helpers
// ---------------------------------------------------------------------------

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(_) => true,
    }
}

/// `a ?? b ?? ...`: the first value that is neither missing nor null.
fn coalesce<'a>(values: &[Option<&'a Value>]) -> Option<&'a Value> {
    values
        .iter()
        .flatten()
        .copied()
        .find(|value| !value.is_null())
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".into(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// Node `normalizeIntentText`: drop a leading BOM, CRLF to LF, trim, refuse empty or NUL.
pub fn normalize_intent_text(value: &str) -> OrchestrateResult<String> {
    let text = value.strip_prefix('\u{feff}').unwrap_or(value);
    let text = text.replace("\r\n", "\n");
    let text = crate::identity::js_trim(&text);
    if text.is_empty() {
        return Err("Intent text is empty".into());
    }
    if text.contains('\0') {
        return Err("Intent text contains a NUL character".into());
    }
    Ok(text.to_owned())
}

/// Node's work-unit split threshold: `config.unitSplitThreshold` when it is a positive
/// integer in the `Number.isInteger` sense (so `1.0` counts as `1`), else the default.
fn unit_split_threshold(config: &Value) -> f64 {
    config
        .get("unitSplitThreshold")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && n.fract() == 0.0 && *n > 0.0)
        .unwrap_or(DEFAULT_UNIT_SPLIT_THRESHOLD as f64)
}

/// Node `path.resolve(path)`: absolute against the working directory, `.` and `..`
/// segments folded lexically, trailing separators dropped, and (on Windows) separators
/// written as `\`. Symbolic links are not resolved.
pub fn resolve_like_node(path: &Path) -> OrchestrateResult<PathBuf> {
    use std::path::Component;
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                resolved.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    resolved.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    resolved.pop();
                }
            }
        }
    }
    Ok(resolved)
}

/// Node `sameIntentText`: Unicode NFC, trimmed, whitespace runs collapsed.
pub fn same_intent_text(a: &str, b: &str) -> bool {
    let norm = |value: &str| {
        crate::identity::js_trim(&crate::workflow::nfc(value))
            .split(is_js_space)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    norm(a) == norm(b)
}

// ---------------------------------------------------------------------------
// Graph and state helpers (Node workflow.mjs)
// ---------------------------------------------------------------------------

fn stage_by_id<'a>(graph: &'a Value, id: &str) -> Option<&'a Value> {
    graph
        .get("stages")
        .and_then(Value::as_array)?
        .iter()
        .find(|stage| str_of(stage, "id") == Some(id))
}

fn phase_by_id<'a>(graph: &'a Value, id: Option<&str>) -> Option<&'a Value> {
    graph
        .get("phases")
        .and_then(Value::as_array)?
        .iter()
        .find(|phase| id.is_some() && str_of(phase, "id") == id)
}

fn stage_status<'a>(state: &'a Value, stage: &Value) -> Option<&'a str> {
    state
        .get("stages")
        .and_then(|rows| rows.get(str_of(stage, "id").unwrap_or("")))
        .and_then(|row| row.get("status"))
        .and_then(Value::as_str)
}

fn scope_of(state: &Value) -> &str {
    str_of(state, "scope").unwrap_or("")
}

struct GateInfo {
    id: Value,
    phase: Value,
    final_stage: Option<String>,
}

impl GateInfo {
    fn to_value(&self) -> Value {
        let mut gate = Map::new();
        gate.insert("id".into(), self.id.clone());
        gate.insert("phase".into(), self.phase.clone());
        if let Some(stage) = &self.final_stage {
            gate.insert("finalStage".into(), json!(stage));
        }
        Value::Object(gate)
    }
    fn id_str(&self) -> &str {
        self.id.as_str().unwrap_or("")
    }
}

/// Node `gateInfo(graph, scope, stage, depth)`.
fn gate_info(graph: &Value, scope: &str, stage: &Value, depth: &str) -> GateInfo {
    let phase_id = str_of(stage, "phase");
    let phase = phase_by_id(graph, phase_id);
    let final_stage = ordered_stages(graph, scope, depth)
        .into_iter()
        .rfind(|item| str_of(item, "phase") == phase_id)
        .and_then(|item| str_of(item, "id"))
        .map(str::to_owned);
    GateInfo {
        id: phase
            .and_then(|phase| phase.get("gate"))
            .filter(|gate| !gate.is_null())
            .cloned()
            .unwrap_or(Value::Null),
        phase: stage.get("phase").cloned().unwrap_or(Value::Null),
        final_stage,
    }
}

/// Node `workUnitsOf`.
fn work_units_of(state: &Value) -> Vec<Value> {
    match state.get("units").and_then(Value::as_array) {
        Some(units) if !units.is_empty() => units.clone(),
        _ => vec![json!({"id": null, "status": "in_progress"})],
    }
}

fn unit_finished(unit: &Value) -> bool {
    stage_finished(str_of(unit, "status"))
}

/// `unit.id || null` as a comparable key.
fn unit_id(unit: &Value) -> Option<&Value> {
    unit.get("id").filter(|id| truthy(Some(id)))
}

/// `unit.id || '_'` as an object key.
fn unit_key(id: Option<&Value>) -> String {
    id.filter(|id| truthy(Some(id)))
        .map(js_string)
        .unwrap_or_else(|| "_".into())
}

fn current_unit(state: &Value) -> Option<&Value> {
    state.get("currentUnit").filter(|unit| truthy(Some(unit)))
}

/// Node `isFinalUnit`.
fn is_final_unit(state: &Value, stage: &Value) -> bool {
    if str_of(stage, "repeat") != Some("work-unit") {
        return true;
    }
    let current = current_unit(state);
    work_units_of(state)
        .iter()
        .all(|unit| unit_id(unit) == current || unit_finished(unit))
}

/// Node `isPhaseGate`.
fn is_phase_gate(graph: &Value, state: &Value, stage: &Value) -> bool {
    let info = gate_info(graph, scope_of(state), stage, depth_of(state));
    info.final_stage.is_some()
        && info.final_stage.as_deref() == str_of(stage, "id")
        && is_final_unit(state, stage)
}

/// Node `lastRepeatOfPhase`.
fn last_repeat_of_phase(graph: &Value, state: &Value, stage: &Value) -> bool {
    ordered_stages(graph, scope_of(state), depth_of(state))
        .into_iter()
        .rfind(|item| {
            str_of(item, "phase") == str_of(stage, "phase")
                && str_of(item, "repeat") == Some("work-unit")
        })
        .is_some_and(|item| str_of(item, "id") == str_of(stage, "id"))
}

/// Node `selectNextStage`; sets or clears `state.currentUnit` as Node does.
fn select_next_stage(state: &mut Value, stages: &[Value]) -> Option<Value> {
    let units = work_units_of(state);
    let repeating: Vec<&Value> = stages
        .iter()
        .filter(|stage| str_of(stage, "repeat") == Some("work-unit"))
        .collect();
    let first = stages
        .iter()
        .position(|stage| str_of(stage, "repeat") == Some("work-unit"));
    let last = stages
        .iter()
        .rposition(|stage| str_of(stage, "repeat") == Some("work-unit"));
    let before = &stages[..first.unwrap_or(stages.len())];
    let after = last.map(|index| &stages[index + 1..]).unwrap_or(&[]);
    let unfinished = |state: &Value, stage: &Value| !stage_finished(stage_status(state, stage));
    if let Some(stage) = before.iter().find(|stage| unfinished(state, stage)) {
        remove_key(state, "currentUnit");
        return Some(stage.clone());
    }
    if first.is_some() {
        if let Some(pending) = units.iter().find(|unit| !unit_finished(unit)) {
            match unit_id(pending) {
                Some(id) => state["currentUnit"] = id.clone(),
                None => remove_key(state, "currentUnit"),
            }
            let key = unit_key(unit_id(pending));
            let found = repeating.iter().find(|stage| {
                let row = state
                    .get("stages")
                    .and_then(|rows| rows.get(str_of(stage, "id").unwrap_or("")));
                let done = row
                    .and_then(|row| row.get("units"))
                    .and_then(|units| units.get(&key))
                    .and_then(|unit| unit.get("status"))
                    .filter(|status| truthy(Some(status)));
                let status = match done {
                    Some(done) => done.as_str(),
                    None if unit_id(pending).is_none() => row
                        .and_then(|row| row.get("status"))
                        .and_then(Value::as_str),
                    None => None,
                };
                !stage_finished(status)
            });
            if let Some(stage) = found {
                return Some((*stage).clone());
            }
        }
    }
    remove_key(state, "currentUnit");
    after.iter().find(|stage| unfinished(state, stage)).cloned()
}

fn remove_key(value: &mut Value, key: &str) {
    if let Some(map) = value.as_object_mut() {
        map.shift_remove(key);
    }
}

/// Node `unitsFromData`.
fn units_from_data(data: &Value) -> Vec<Value> {
    if data.is_object() && str_of(data, "status") == Some("not-applicable") {
        return Vec::new();
    }
    let units = match data.get("units") {
        Some(Value::Array(units)) => Some(units),
        _ => data.as_array(),
    };
    units.cloned().unwrap_or_default()
}

/// Node `applyWorkUnits`: read `work-units.json` beside `work-units.md` once.
fn apply_work_units(root: &Path, graph: &Value, state: &mut Value) {
    if truthy(state.get("designReworkPending")) {
        return;
    }
    if state
        .get("units")
        .and_then(Value::as_array)
        .is_some_and(|units| !units.is_empty())
    {
        return;
    }
    let mut lookup = state.clone();
    remove_key(&mut lookup, "currentUnit");
    lookup["depth"] = json!(depth_of(state));
    let design_stage = stage_by_id(graph, "implementation-design")
        .cloned()
        .unwrap_or_else(|| json!({"produces": []}));
    let Ok(design) = expected_artifacts(root, graph, &lookup, &design_stage) else {
        return;
    };
    let Some(path) = design
        .iter()
        .find(|item| item.output == "work-units")
        .map(|item| {
            let text = item.path.to_string_lossy();
            PathBuf::from(
                text.strip_suffix(".md")
                    .map_or_else(|| text.to_string(), |stem| format!("{stem}.json")),
            )
        })
    else {
        return;
    };
    if !path.exists() {
        return;
    }
    let Ok(bytes) = fs::read(&path) else { return };
    let Ok(data) = serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes)) else {
        return;
    };
    let rows = units_from_data(&data);
    if rows.is_empty() || rows.iter().any(Value::is_null) {
        return;
    }
    let units = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let id = row
                .get("id")
                .filter(|id| truthy(Some(id)))
                .map(js_string)
                .unwrap_or_else(|| format!("U-{:02}", index + 1));
            let depends_on = row
                .get("dependsOn")
                .filter(|value| truthy(Some(value)))
                .cloned()
                .unwrap_or_else(|| json!([]));
            json!({"id": id, "status": "in_progress", "dependsOn": depends_on})
        })
        .collect::<Vec<_>>();
    state["units"] = Value::Array(units);
}

/// Node `progressOf`.
fn progress_of(state: &Value, graph: &Value) -> (usize, usize) {
    let stages = ordered_stages(graph, scope_of(state), depth_of(state));
    let done = stages
        .iter()
        .filter(|stage| stage_finished(stage_status(state, stage)))
        .count();
    (done, stages.len())
}

/// Node `completePayload`.
fn complete_payload(state: &Value, graph: &Value, hint: Option<&str>) -> Value {
    let skipped = ordered_stages(graph, scope_of(state), "full")
        .into_iter()
        .filter(|stage| stage_status(state, stage) == Some("skipped"))
        .filter_map(|stage| stage.get("id").cloned())
        .collect::<Vec<_>>();
    let approvals = state
        .get("gates")
        .and_then(Value::as_object)
        .map(|gates| {
            gates
                .iter()
                .map(|(gate, row)| {
                    let mut item = Map::new();
                    item.insert("gate".into(), json!(gate));
                    if let Some(by) = row.get("by") {
                        item.insert("by".into(), by.clone());
                    }
                    if let Some(at) = row.get("at") {
                        item.insert("at".into(), at.clone());
                    }
                    Value::Object(item)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut payload = Map::new();
    payload.insert("complete".into(), json!(true));
    if let Some(intent) = state.get("intent") {
        payload.insert("intent".into(), intent.clone());
    }
    if let Some(id) = state.get("intentId") {
        payload.insert("intentId".into(), id.clone());
    }
    payload.insert("skippedStages".into(), Value::Array(skipped));
    payload.insert("approvals".into(), Value::Array(approvals));
    if let Some(hint) = hint {
        payload.insert("hint".into(), json!(hint));
    }
    Value::Object(payload)
}

/// Node `unfinishedIds` (`listIntents` rows that are active or paused).
fn unfinished_ids(root: &Path, graph: &Value) -> Vec<String> {
    let (pointer, _) = read_pointer(root);
    let current = match &pointer {
        Pointer::Active(id) => id.clone(),
        Pointer::Legacy(state) => str_of(state, "intentId").map(str::to_owned),
        _ => None,
    };
    let corrupt = matches!(pointer, Pointer::Corrupt(_));
    let mut rows: Vec<(String, String, &'static str)> = Vec::new();
    for id in intent_ids_on_disk(root)
        .into_iter()
        .filter(|id| valid_intent_id(id))
    {
        let Ok((state, _)) = load_intent(root, graph, &id) else {
            continue;
        };
        let derived = derive_status(&state, graph);
        let status = match derived {
            "complete" | "abandoned" => derived,
            _ if !corrupt && current.as_deref() == Some(id.as_str()) => "active",
            _ => "paused",
        };
        let created = state
            .get("createdAt")
            .filter(|value| !value.is_null())
            .map(js_string)
            .unwrap_or_default();
        rows.push((created, id, status));
    }
    if let Pointer::Legacy(raw) = &pointer {
        let state = crate::workflow::normalise_state(raw.clone());
        let id = str_of(&state, "intentId").unwrap_or("legacy").to_owned();
        let derived = derive_status(&state, graph);
        let status = if matches!(derived, "complete" | "abandoned") {
            derived
        } else {
            "active"
        };
        let created = state
            .get("createdAt")
            .filter(|value| !value.is_null())
            .map(js_string)
            .unwrap_or_default();
        rows.retain(|row| row.1 != id);
        rows.push((created, id, status));
    }
    rows.sort();
    rows.into_iter()
        .filter(|row| matches!(row.2, "active" | "paused"))
        .map(|row| row.1)
        .collect()
}

fn config_path(root: &Path) -> PathBuf {
    root.join(".aidlc").join("config.json")
}

fn dirname(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

/// Node `nearestConfigParent`.
fn nearest_config_parent(root: &Path) -> Option<PathBuf> {
    let mut current = dirname(root);
    let stop = dirname(&current);
    loop {
        if config_path(&current).exists() {
            return Some(current);
        }
        let parent = dirname(&current);
        if parent == current {
            return None;
        }
        current = parent;
        if current == stop && stop == dirname(&stop) {
            return None;
        }
    }
}

/// Node `requireInitialized`.
fn require_initialized(root: &Path) -> OrchestrateResult<()> {
    if config_path(root).exists() {
        return Ok(());
    }
    let parent = nearest_config_parent(root)
        .map(|parent| {
            let parent = parent.display();
            format!(" An initialized project exists at {parent}; run the command there or pass --project {parent}.")
        })
        .unwrap_or_default();
    Err(format!(
        "Project is not initialized at {}. Run 'dtech-aidlc onboard --harness <claude|deepseek> --scope <solo|team|enterprise>' there first.{parent}",
        root.display()
    ))
}

/// Node `resolveDepth`.
fn resolve_depth(graph: &Value, depth: Option<&str>) -> OrchestrateResult<String> {
    let name = depth.filter(|depth| !depth.is_empty()).unwrap_or("full");
    if name == "full" {
        return Ok("full".into());
    }
    if graph
        .get("depths")
        .and_then(|depths| depths.get(name))
        .is_none()
    {
        let mut known = vec!["full".to_owned()];
        if let Some(depths) = graph.get("depths").and_then(Value::as_object) {
            known.extend(depths.keys().cloned());
        }
        return Err(format!(
            "Unknown depth '{name}'. Known depths: {}",
            known.join(", ")
        ));
    }
    Ok(name.to_owned())
}

/// Node `resolveMode(config, flag)`.
fn resolve_mode(config: &Value, flag: Option<&str>) -> OrchestrateResult<String> {
    let mode = match flag {
        Some(flag) => flag.to_owned(),
        None => match config.get("mode") {
            None | Some(Value::Null) => "human".into(),
            Some(value) => js_string(value),
        },
    };
    if !matches!(mode.as_str(), "human" | "ai") {
        return Err(format!("Unknown mode '{mode}'. Known modes: human, ai"));
    }
    Ok(mode)
}

/// Node `effectiveGateMode`.
fn effective_gate_mode(config: &Value, state: &Value, gate_id: &Value) -> String {
    let key = js_string(gate_id);
    if config
        .get("gateModes")
        .and_then(|modes| modes.get(&key))
        .and_then(Value::as_str)
        == Some("human")
    {
        return "human".into();
    }
    mode_of(state).to_owned()
}

/// Node `configuredHarnesses` against the bundled harness manifests.
fn configured_harnesses(config: &Value) -> Vec<String> {
    let known = |name: &str| harness_manifest(name).is_some();
    if let Some(list) = config.get("harnesses").and_then(Value::as_array) {
        if !list.is_empty() && list.iter().all(|name| name.as_str().is_some_and(known)) {
            return list
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
        }
    }
    match config.get("harness").and_then(Value::as_str) {
        Some(name) if known(name) => vec![name.to_owned()],
        _ => Vec::new(),
    }
}

fn harness_manifest(name: &str) -> Option<Value> {
    if name.is_empty() || name.contains(['/', '\\', '.']) {
        return None;
    }
    let bytes = crate::scope::embedded_asset(&format!("harness/{name}/manifest.json"))?;
    serde_json::from_slice(bytes).ok()
}

/// Node `aliasesSupported`.
fn aliases_supported(config: &Value) -> bool {
    let harnesses = configured_harnesses(config);
    harnesses.is_empty()
        || harnesses.iter().any(|name| {
            harness_manifest(name)
                .and_then(|manifest| manifest.get("modelAliases").cloned())
                .and_then(|aliases| aliases.as_array().map(|aliases| !aliases.is_empty()))
                .unwrap_or(false)
        })
}

/// Node `allowedApprovers`.
fn allowed_approvers(config: &Value, gate_id: &str) -> Option<Vec<Value>> {
    let roles = config
        .get("roles")
        .filter(|roles| roles.is_object() || roles.is_array())?;
    if let Some(named) = roles
        .get("approvers")
        .and_then(|approvers| approvers.get(gate_id))
        .and_then(Value::as_array)
        .filter(|named| !named.is_empty())
    {
        return Some(named.clone());
    }
    roles
        .get("owner")
        .and_then(Value::as_array)
        .filter(|owner| !owner.is_empty())
        .cloned()
}

/// Node `buildDirective`.
#[allow(clippy::too_many_arguments)]
fn build_directive(
    root: &Path,
    graph: &Value,
    config: &Value,
    engine: &Engine,
    state: &Value,
    stage: &Value,
    warnings: Vec<String>,
    context: Option<Value>,
    rework: Option<Value>,
) -> OrchestrateResult<Value> {
    let intent_id = str_of(state, "intentId").unwrap_or("");
    let phase = phase_by_id(graph, str_of(stage, "phase"));
    let gate = gate_info(graph, scope_of(state), stage, depth_of(state));
    let phase_gate = is_phase_gate(graph, state, stage);
    let expected = expected_artifacts(root, graph, state, stage)?;
    let mut directive = Map::new();
    for key in ["intent", "intentId"] {
        if let Some(value) = state.get(key) {
            directive.insert(key.into(), value.clone());
        }
    }
    directive.insert(
        "intentDir".into(),
        json!(intent_dir_path(root, intent_id).to_string_lossy()),
    );
    if let Some(scope) = state.get("scope") {
        directive.insert("scope".into(), scope.clone());
    }
    directive.insert("depth".into(), json!(depth_of(state)));
    directive.insert("mode".into(), json!(mode_of(state)));
    directive.insert("orchestrationMode".into(), json!(mode_of(state)));
    directive.insert(
        "scopeSource".into(),
        coalesce(&[state.get("scopeSource")])
            .cloned()
            .unwrap_or_else(|| json!("config")),
    );
    if let Some(unit) = current_unit(state) {
        directive.insert("currentUnit".into(), unit.clone());
    }
    if let Some(assignee) = state.get("assignee").filter(|value| truthy(Some(value))) {
        directive.insert("assignee".into(), assignee.clone());
    }
    directive.insert("warnings".into(), json!(warnings));
    directive.insert("gate".into(), gate.to_value());
    directive.insert(
        "gateMode".into(),
        json!(effective_gate_mode(config, state, &gate.id)),
    );
    if let Some(final_stage) = &gate.final_stage {
        directive.insert("approvalRequiredAfter".into(), json!(final_stage));
    }
    directive.insert(
        "contractsFile".into(),
        json!(engine.path("methodology/contracts.md")),
    );
    directive.insert(
        "suggestedBranch".into(),
        json!(format!("feature/{intent_id}")),
    );
    let artifacts = expected
        .iter()
        .map(|item| {
            let mut row = Map::new();
            row.insert("output".into(), json!(item.output));
            row.insert("kind".into(), json!(item.kind));
            row.insert("path".into(), json!(item.rel_path));
            if let Some(template) = graph
                .get("outputs")
                .and_then(|outputs| outputs.get(&item.output))
                .and_then(|output| output.get("template"))
                .filter(|template| truthy(Some(template)))
            {
                row.insert("template".into(), json!(engine.path(&js_string(template))));
            }
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    directive.insert("artifacts".into(), Value::Array(artifacts));

    let mut stage_row = stage.as_object().cloned().unwrap_or_default();
    stage_row.insert("phaseGate".into(), json!(phase_gate));
    stage_row.insert(
        "phaseFile".into(),
        phase
            .map(|phase| json!(engine.path(&js_string(phase.get("file").unwrap_or(&Value::Null)))))
            .unwrap_or(Value::Null),
    );
    if truthy(stage.get("agent")) {
        let agent_id = js_string(&stage["agent"]);
        let agent = graph
            .get("agents")
            .and_then(Value::as_array)
            .and_then(|agents| {
                agents
                    .iter()
                    .find(|agent| str_of(agent, "id") == Some(agent_id.as_str()))
            })
            .ok_or_else(|| {
                format!(
                    "Unknown agent '{agent_id}' for stage {}",
                    js_string(&stage["id"])
                )
            })?;
        stage_row.insert(
            "agentFile".into(),
            json!(engine.path(&js_string(agent.get("file").unwrap_or(&Value::Null)))),
        );
        if aliases_supported(config) {
            let override_model = config
                .get("agentModels")
                .and_then(|models| models.get(&agent_id));
            let default_model = coalesce(&[
                agent.get("model"),
                agent.pointer("/modelRecommendation/model"),
            ]);
            let model = coalesce(&[override_model, default_model]).cloned();
            stage_row.insert("model".into(), model.unwrap_or(Value::Null));
            stage_row.insert(
                "modelSource".into(),
                json!(if override_model.is_none() {
                    "default"
                } else {
                    "project"
                }),
            );
        }
    }
    if let Some(rework) = rework {
        stage_row.insert("rework".into(), rework);
    }
    directive.insert("stage".into(), Value::Object(stage_row));
    if let Some(context) = context.filter(|context| !context.is_null()) {
        directive.insert("context".into(), context);
    }
    Ok(Value::Object(directive))
}

fn audit(root: &Path, event: &str, details: Value) -> OrchestrateResult<()> {
    crate::audit::audit(root, event, details).map(|_| ())
}

fn prerequisite_results(results: &[Value]) -> Value {
    Value::Array(
        results
            .iter()
            .map(|item| {
                let mut row = Map::new();
                for key in ["name", "ok", "exitCode"] {
                    if let Some(value) = item.get(key) {
                        row.insert(key.into(), value.clone());
                    }
                }
                Value::Object(row)
            })
            .collect(),
    )
}

/// Refuse to write when the pointer (and the loaded intent state) changed since load.
fn ensure_unchanged(
    root: &Path,
    loaded: &LoadedWorkflow,
    intent_id: Option<&str>,
) -> OrchestrateResult<()> {
    let read = |path: &Path| match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    };
    let changed = "Workflow state changed while this command was running; no changes were made. Run the command again.";
    if read(&state_path(root))? != loaded.pointer_bytes {
        return Err(changed.into());
    }
    if let Some(id) = intent_id {
        if read(&crate::workflow::intent_state_path(root, id))? != loaded.state_bytes {
            return Err(changed.into());
        }
    }
    Ok(())
}

/// Write the active intent's state (Node `persistWorkflow`, without rewriting the pointer).
fn persist_active(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    state: &mut Value,
) -> OrchestrateResult<()> {
    let id = str_of(state, "intentId").unwrap_or("").to_owned();
    if loaded.active_intent_id.as_deref() != Some(id.as_str()) {
        return Err(format!(
            "Intent '{id}' is not the active intent; no changes were made"
        ));
    }
    ensure_unchanged(root, loaded, Some(&id))?;
    save_intent(root, state, Some(graph))
}

/// Write a new intent: folder, `intent.md`, `state.json`, then the pointer.
fn persist_new(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    state: &mut Value,
) -> OrchestrateResult<()> {
    let id = str_of(state, "intentId").unwrap_or("").to_owned();
    if !valid_intent_id(&id) {
        return Err("intentId is not a safe identifier".into());
    }
    ensure_unchanged(root, loaded, None)?;
    let dir = intent_dir_path(root, &id);
    if fs::symlink_metadata(&dir).is_ok() {
        return Err(format!(
            "Intent folder .aidlc/intents/{id} appeared while this command was running; no changes were made. Run the command again."
        ));
    }
    crate::workflow::assert_no_symlink_components(root, &format!(".aidlc/intents/{id}"))?;
    fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    write_intent_markdown(root, &id, str_of(state, "intent").unwrap_or(""))?;
    save_intent(root, state, Some(graph))?;
    save_pointer(root, Some(&id))
}

// ---------------------------------------------------------------------------
// orchestrate next
// ---------------------------------------------------------------------------

/// Node `orchestrateNext`. `root` must be absolute; `config` is the validated project
/// configuration (Node `validateModelConfig`).
pub fn orchestrate_next(
    root: &Path,
    graph: &Value,
    config: &Value,
    engine: &Engine,
    options: &NextOptions,
) -> OrchestrateResult<NextOutcome> {
    let loaded = load_workflow(root, graph, true)?;
    let requested = options.intent.as_deref().filter(|text| !text.is_empty());
    let mut state = loaded.state.clone();
    let finished = state
        .as_ref()
        .is_some_and(|state| matches!(derive_status(state, graph), "complete" | "abandoned"));
    let is_complete = |state: &Option<Value>| {
        state
            .as_ref()
            .is_some_and(|state| derive_status(state, graph) == "complete")
    };
    let mut is_new = false;
    let mut warnings: Vec<String> = Vec::new();
    let mut scope_source = state
        .as_ref()
        .and_then(|state| coalesce(&[state.get("scopeSource")]).cloned())
        .unwrap_or_else(|| json!("config"));
    let project_scope = config
        .get("scope")
        .and_then(Value::as_str)
        .filter(|scope| matches!(*scope, "solo" | "team" | "enterprise"))
        .map(str::to_owned);

    match requested {
        None => {
            if state.is_none() || finished {
                if is_complete(&state) {
                    return Ok(NextOutcome::Complete(complete_payload(
                        state.as_ref().unwrap(),
                        graph,
                        None,
                    )));
                }
                let paused = unfinished_ids(root, graph);
                let paused = if paused.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Paused intents: {}. Continue one with: dtech-aidlc intent resume <id>.",
                        paused.join(", ")
                    )
                };
                return Err(format!(
                    "No active intent. Start one with: dtech-aidlc orchestrate next --intent \"...\"{paused}"
                ));
            }
        }
        Some(requested) if state.is_none() || finished => {
            if is_complete(&state)
                && same_intent_text(
                    requested,
                    str_of(state.as_ref().unwrap(), "intent").unwrap_or(""),
                )
            {
                return Ok(NextOutcome::Complete(complete_payload(
                    state.as_ref().unwrap(),
                    graph,
                    Some("This intent is already complete. Use a different --intent text to start new work."),
                )));
            }
            require_initialized(root)?;
            let scope = match options.scope.as_deref().filter(|scope| !scope.is_empty()) {
                Some(scope) => scope.to_owned(),
                None => match config.get("scope") {
                    Some(value) if truthy(Some(value)) => js_string(value),
                    _ => "solo".into(),
                },
            };
            if !matches!(scope.as_str(), "solo" | "team" | "enterprise") {
                return Err("scope must be solo, team, or enterprise".into());
            }
            scope_source = json!(if options.scope.is_some() {
                "flag"
            } else {
                "config"
            });
            let depth = resolve_depth(graph, options.depth.as_deref())?;
            if options
                .depth
                .as_deref()
                .is_some_and(|depth| !depth.is_empty() && depth != "full")
                && options.approved_by.as_deref().unwrap_or("").is_empty()
            {
                return Err("Creating an intent with --depth requires --approved-by NAME".into());
            }
            let mode = resolve_mode(config, options.mode.as_deref())?;
            let mode_source = if options.mode.is_some() {
                "flag"
            } else {
                "config"
            };
            let intent_id = free_intent_id(root, &intent_slug(requested))?;
            let mut draft = json!({
                "version": 2,
                "intentId": intent_id,
                "intent": requested,
                "scope": scope,
                "scopeSource": scope_source,
                "projectScope": project_scope,
                "depth": depth,
                "mode": mode,
                "status": "active",
                "currentStage": null,
                "stages": {},
                "dependencies": {},
                "createdAt": crate::audit::now_iso(),
            });
            draft["modeSource"] = json!(mode_source);
            state = Some(draft);
            is_new = true;
            if let (Some(flag), Some(project)) =
                (options.scope.as_deref(), project_scope.as_deref())
            {
                if flag != project {
                    warnings.push(format!(
                        "Intent scope '{scope}' overrides the project scope '{project}' from .aidlc/config.json (given by --scope)."
                    ));
                }
            }
            if let Some(flag) = options.mode.as_deref() {
                let project_mode = match config.get("mode") {
                    None | Some(Value::Null) => "human".to_owned(),
                    Some(value) => js_string(value),
                };
                if project_mode != flag {
                    warnings.push(format!(
                        "Intent mode '{mode}' overrides the project mode '{project_mode}' from .aidlc/config.json (given by --mode)."
                    ));
                }
            }
        }
        Some(requested) => {
            let current = state.as_ref().unwrap();
            let intent_id = str_of(current, "intentId").unwrap_or("");
            if let Some(depth) = options.depth.as_deref().filter(|depth| !depth.is_empty()) {
                if resolve_depth(graph, Some(depth))? != depth_of(current) {
                    return Err(format!(
                        "Intent '{intent_id}' uses depth {}; the depth cannot change after an intent starts. Use 'dtech-aidlc intent depth'.",
                        depth_of(current)
                    ));
                }
            }
            if !same_intent_text(requested, str_of(current, "intent").unwrap_or("")) {
                let stages = ordered_stages(graph, scope_of(current), depth_of(current));
                let next_unfinished = stages
                    .iter()
                    .find(|stage| !stage_finished(stage_status(current, stage)));
                let (done, total) = progress_of(current, graph);
                let stage = current
                    .get("currentStage")
                    .filter(|stage| truthy(Some(stage)))
                    .map(js_string)
                    .or_else(|| {
                        next_unfinished
                            .and_then(|stage| str_of(stage, "id"))
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "undefined".into());
                return Err(format!(
                    "Intent '{intent_id}' is still in progress (stage {stage}, {done} of {total} stages finished). Finish it, set it aside with 'dtech-aidlc intent pause', or close it with 'dtech-aidlc intent abandon --detail \"<reason>\"', then start the new intent."
                ));
            }
            if let Some(scope) = options.scope.as_deref().filter(|scope| !scope.is_empty()) {
                if Some(scope) != str_of(current, "scope") {
                    return Err(format!(
                        "Intent '{intent_id}' uses scope {}; the scope cannot change after an intent starts. Omit --scope to continue it.",
                        scope_of(current)
                    ));
                }
            }
            if let Some(mode) = options.mode.as_deref().filter(|mode| !mode.is_empty()) {
                if mode != mode_of(current) {
                    return Err(format!(
                        "Intent '{intent_id}' uses mode {}; the mode cannot change after an intent starts. Omit --mode to continue it.",
                        mode_of(current)
                    ));
                }
            }
        }
    }

    let mut state = state.expect("state is set on every path that continues");
    let intent_id = str_of(&state, "intentId").unwrap_or("").to_owned();
    if !is_new && !config_path(root).exists() {
        warnings = vec![format!(
            "Project has an active intent but no .aidlc/config.json. Run dtech-aidlc onboard --harness <claude|deepseek> --scope {} to complete setup.",
            scope_of(&state)
        )];
    }

    let stages: Vec<Value> = ordered_stages(graph, scope_of(&state), depth_of(&state))
        .into_iter()
        .cloned()
        .collect();
    apply_work_units(root, graph, &mut state);
    let Some(next) = select_next_stage(&mut state, &stages) else {
        if !is_new {
            let mut finished_state = state.clone();
            finished_state["currentStage"] = Value::Null;
            remove_key(&mut finished_state, "currentUnit");
            persist_active(root, graph, &loaded, &mut finished_state)?;
        }
        return Ok(NextOutcome::Complete(complete_payload(&state, graph, None)));
    };
    let next_id = str_of(&next, "id").unwrap_or("").to_owned();

    let row = state
        .get("stages")
        .and_then(|rows| rows.get(&next_id))
        .filter(|row| !row.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let reports = row
        .get("reports")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let last_report = reports.last().cloned();
    let was_blocked = str_of(&row, "status") == Some("blocked")
        || last_report
            .as_ref()
            .is_some_and(|report| str_of(report, "result") == Some("blocked"));

    // Named prerequisites: those the last blocked report named, plus preflight checks the
    // owner attached to this stage or phase. Nothing is persisted while one still fails.
    let last_needs = last_report
        .as_ref()
        .and_then(|report| report.get("needs"))
        .and_then(Value::as_array)
        .cloned();
    let mut needed: Vec<String> = if was_blocked {
        last_needs
            .clone()
            .unwrap_or_default()
            .iter()
            .map(js_string)
            .collect()
    } else {
        Vec::new()
    };
    needed.extend(crate::prerequisites::preflight_names(config, &next));
    let mut prerequisites: Option<Vec<Value>> = None;
    if !needed.is_empty() {
        let results =
            crate::prerequisites::run_prerequisites(&root.to_string_lossy(), config, &needed);
        let failing: Vec<&Value> = results
            .iter()
            .filter(|item| item.get("ok") != Some(&Value::Bool(true)))
            .collect();
        if !failing.is_empty() && !options.force_resume {
            if !is_new {
                audit(
                    root,
                    "prerequisite.checked",
                    json!({"intentId": intent_id, "stage": next_id, "results": prerequisite_results(&results)}),
                )?;
            }
            let hints = failing
                .iter()
                .map(|item| {
                    let name = item.get("name").map(js_string).unwrap_or_default();
                    match item.get("hint").filter(|hint| truthy(Some(hint))) {
                        Some(hint) => format!("{name} ({})", js_string(hint)),
                        None => name,
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut payload = Map::new();
            payload.insert("complete".into(), json!(false));
            payload.insert("blocked".into(), json!(true));
            payload.insert(
                "intentId".into(),
                if is_new {
                    Value::Null
                } else {
                    json!(intent_id)
                },
            );
            payload.insert("stage".into(), json!(next_id));
            if let Some(unit) = current_unit(&state) {
                payload.insert("currentUnit".into(), unit.clone());
            }
            payload.insert("needs".into(), Value::Array(results.clone()));
            payload.insert(
                "hint".into(),
                json!(format!(
                    "Stage {next_id} needs {hints}, and its check is failing. Fix the environment and run 'dtech-aidlc orchestrate next' again{}. Only the owner may override with --force-resume --detail \"<reason>\".",
                    if is_new { " with the same request (the intent was not created yet)" } else { "" }
                )),
            );
            return Ok(NextOutcome::Blocked(Value::Object(payload)));
        }
        if !failing.is_empty() {
            if options.detail.as_deref().unwrap_or("").is_empty() {
                return Err("--force-resume requires --detail \"<reason>\"".into());
            }
            warnings.push(format!(
                "Prerequisite check failing and overridden with --force-resume: {}.",
                failing
                    .iter()
                    .map(|item| item.get("name").map(js_string).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        prerequisites = Some(results);
    }

    if str_of(&next, "repeat") == Some("work-unit") {
        let units = state.get("units").and_then(Value::as_array);
        if !units.is_some_and(|units| !units.is_empty()) && depth_of(&state) != "fast" {
            warnings.push("No work-units.json was read for this intent, so Develop runs as one implicit work unit. Write it beside work-units.md in implementation-design.".into());
        }
        let threshold = unit_split_threshold(config);
        let first_repeat = stages
            .iter()
            .find(|stage| str_of(stage, "repeat") == Some("work-unit"))
            .and_then(|stage| str_of(stage, "id"));
        if let Some(units) = units {
            if units.len() as f64 > threshold
                && first_repeat == Some(next_id.as_str())
                && state.get("currentUnit") == units[0].get("id")
            {
                warnings.push(format!(
                    "This intent has {} work units, more than the split threshold of {threshold}. Consider asking the owner to split it into separate intents (for example backend, then API/UI) before building.",
                    units.len()
                ));
            }
        }
    }

    let now = crate::audit::now_iso();
    let mut rework: Option<Value> = None;
    if was_blocked {
        let mut resumed = row.as_object().cloned().unwrap_or_default();
        resumed.insert("status".into(), json!("in_progress"));
        if coalesce(&[row.get("resumedAt")]).is_none() {
            resumed.insert("resumedAt".into(), json!(now));
        }
        state["stages"][&next_id] = Value::Object(resumed);
        let mut info = Map::new();
        info.insert(
            "blockedAt".into(),
            coalesce(&[
                last_report.as_ref().and_then(|report| report.get("at")),
                row.get("reportedAt"),
            ])
            .cloned()
            .unwrap_or(Value::Null),
        );
        info.insert(
            "detail".into(),
            coalesce(&[
                last_report.as_ref().and_then(|report| report.get("detail")),
                row.get("detail"),
            ])
            .cloned()
            .unwrap_or(Value::Null),
        );
        info.insert("attempt".into(), json!(reports.len().max(1) + 1));
        if let Some(needs) = last_needs.filter(|needs| !needs.is_empty()) {
            info.insert("needs".into(), Value::Array(needs));
        }
        rework = Some(Value::Object(info));
    } else {
        if state["stages"].get(&next_id).is_none_or(Value::is_null) {
            state["stages"][&next_id] = json!({"status": "in_progress", "startedAt": now});
        }
        if next_id == "implementation-design" && truthy(state.get("designRework")) {
            let request = state["designRework"].clone();
            let mut info = Map::new();
            for key in ["requestedAt", "fromStage", "detail"] {
                if let Some(value) = request.get(key) {
                    info.insert(key.into(), value.clone());
                }
            }
            rework = Some(Value::Object(info));
            remove_key(&mut state, "designRework");
            remove_key(&mut state, "designReworkPending");
        }
    }
    state["currentStage"] = json!(next_id);

    let mut stage_for_context = next.clone();
    stage_for_context["phaseGate"] = json!(is_phase_gate(graph, &state, &next));
    // Node returns null (and omits `context`) when the context cannot be built.
    let context = crate::orchestration_context::build_with_engine(
        crate::orchestration_context::ContextInput {
            root,
            state: &state,
            stage: &stage_for_context,
            graph,
        },
        engine.root(),
    )
    .ok();
    let mut directive = build_directive(
        root,
        graph,
        config,
        engine,
        &state,
        &next,
        warnings,
        context,
        rework.clone(),
    )?;
    if let Some(results) = &prerequisites {
        directive["prerequisites"] = Value::Array(results.clone());
    }

    // Everything below writes. Resolve the depth approver first so an invalid
    // --approved-by cannot leave a half-recorded intent behind.
    let depth_approver = if is_new && depth_of(&state) != "full" {
        Some(crate::identity::resolve_approver(
            root,
            options.approved_by.as_deref(),
        )?)
    } else {
        None
    };
    // Node checks the audit log only when it appends, after the state is written;
    // check it first so a broken log cannot leave state recorded without its audit row.
    crate::audit::preflight_audit(root)?;
    engine.prepare()?;

    if is_new {
        persist_new(root, graph, &loaded, &mut state)?;
        audit(
            root,
            "intent.created",
            json!({
                "intent": state["intent"],
                "intentId": intent_id,
                "scope": state["scope"],
                "scopeSource": scope_source,
                "projectScope": project_scope,
                "mode": mode_of(&state),
                "modeSource": coalesce(&[state.get("modeSource")]).cloned().unwrap_or_else(|| json!("config")),
            }),
        )?;
        if let Some(who) = depth_approver {
            audit(
                root,
                "depth.chosen",
                json!({
                    "intentId": intent_id,
                    "depth": state["depth"],
                    "by": who.by,
                    "bySource": who.by_source,
                    "detail": options.detail,
                }),
            )?;
        }
    } else {
        persist_active(root, graph, &loaded, &mut state)?;
    }
    if let Some(results) = &prerequisites {
        audit(
            root,
            "prerequisite.checked",
            json!({"intentId": intent_id, "stage": next_id, "results": prerequisite_results(results)}),
        )?;
        let overridden = results
            .iter()
            .filter(|item| item.get("ok") != Some(&Value::Bool(true)))
            .filter_map(|item| item.get("name").cloned())
            .collect::<Vec<_>>();
        if !overridden.is_empty() {
            let mut details = Map::new();
            details.insert("intentId".into(), json!(intent_id));
            details.insert("stage".into(), json!(next_id));
            details.insert("names".into(), Value::Array(overridden));
            if let Some(detail) = &options.detail {
                details.insert("detail".into(), json!(detail));
            }
            audit(root, "prerequisite.overridden", Value::Object(details))?;
        }
    }
    audit(
        root,
        "stage.selected",
        json!({"intent": state["intent"], "intentId": intent_id, "stage": next_id, "rework": rework.is_some()}),
    )?;
    Ok(NextOutcome::Directive(directive))
}

/// Library entry point kept for existing callers: run `orchestrate next` (no new
/// intent) for the active intent of `workflow`, with methodology paths under the
/// canonical, materialized `<project>/.aidlc/engine`. On success `state` is replaced by
/// the persisted intent state. A prerequisite hold is returned as an error.
pub fn next(workflow: &crate::workflow::Workflow, state: &mut Value) -> OrchestrateResult<Value> {
    let root = resolve_like_node(workflow.root())?;
    let graph = workflow.graph();
    let config = crate::scope::load_config(&root)?;
    let engine_root = materialize_embedded_engine(&root)?;
    let outcome = orchestrate_next(
        &root,
        graph,
        &config,
        &Engine::external(engine_root),
        &NextOptions::default(),
    )?;
    if let NextOutcome::Blocked(payload) = &outcome {
        return Err(payload
            .get("hint")
            .map(js_string)
            .unwrap_or_else(|| "A prerequisite check failed; no stage was selected".into()));
    }
    if let Some(id) = str_of(state, "intentId").map(str::to_owned) {
        if let Ok((persisted, _)) = load_intent(&root, graph, &id) {
            *state = persisted;
        }
    }
    Ok(outcome.into_value())
}

/// The CLI's work after a directive was produced (Node `dtech-aidlc.mjs`): make sure the
/// knowledge folder and `.aidlc/.gitignore` exist and append projection notices.
pub fn finish_directive(root: &Path, directive: &mut Value) -> OrchestrateResult<()> {
    if directive.get("stage").is_none() {
        return Ok(());
    }
    crate::knowledge::ensure_knowledge_dir(root)?;
    crate::knowledge::ensure_ignore_file(root)?;
    let notices = projection_notices(root, env!("CARGO_PKG_VERSION"));
    if let Some(warnings) = directive.get_mut("warnings").and_then(Value::as_array_mut) {
        warnings.extend(notices.into_iter().map(Value::String));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Projection notices (Node projection.mjs `projectionNotices`)
// ---------------------------------------------------------------------------

fn hash_text(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn compare_versions(a: Option<&str>, b: &str) -> std::cmp::Ordering {
    let parse = |value: Option<&str>| -> [u64; 3] {
        let value = value.unwrap_or("");
        let mut parts = [0u64; 3];
        let mut rest = value;
        for (index, part) in parts.iter_mut().enumerate() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return [0, 0, 0];
            }
            *part = rest[..digits].parse().unwrap_or(0);
            rest = &rest[digits..];
            if index < 2 {
                match rest.strip_prefix('.') {
                    Some(next) => rest = next,
                    None => return [0, 0, 0],
                }
            }
        }
        parts
    };
    parse(a).cmp(&parse(Some(b)))
}

fn projection_state(
    root: &Path,
    name: &str,
    manifest: &Value,
    version: &str,
) -> OrchestrateResult<(String, Option<String>)> {
    let skill_dir = str_of(manifest, "skillDir").ok_or("invalid manifest")?;
    let stamp = {
        let path = Path::new(root).join(skill_dir).join(".projection.json");
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.file_type().is_symlink() => fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .filter(|stamp| stamp.get("schemaVersion").and_then(Value::as_f64) == Some(1.0)),
            _ => None,
        }
    };
    let history: Value = serde_json::from_str(PROJECTION_HISTORY)
        .ok()
        .and_then(|history: Value| history.pointer(&format!("/harness/{name}")).cloned())
        .unwrap_or_else(|| json!({}));
    let base = |file: &str| file.rsplit(['/', '\\']).next().unwrap_or(file).to_owned();
    let stamp_hash = |file: &str| -> Option<String> {
        stamp
            .as_ref()
            .and_then(|stamp| stamp.get("files"))
            .and_then(|files| files.get(file))
            .and_then(|row| row.get("sha256"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let in_history = |file: &str, hash: &str| {
        history
            .get(base(file))
            .and_then(Value::as_array)
            .is_some_and(|hashes| hashes.iter().any(|known| known.as_str() == Some(hash)))
    };
    let mut files = Vec::new();
    for file in manifest
        .get("files")
        .and_then(Value::as_array)
        .ok_or("invalid manifest")?
    {
        let target = str_of(file, "target").ok_or("invalid manifest")?;
        let source = str_of(file, "source").ok_or("invalid manifest")?;
        let parent = Path::new(target)
            .parent()
            .map(|parent| parent.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        crate::workflow::assert_no_symlink_components(root, &format!("{skill_dir}/{parent}"))?;
        let path = root.join(skill_dir).join(target);
        let state = if !path.exists() {
            "missing"
        } else {
            match fs::symlink_metadata(&path) {
                Ok(meta) if !meta.is_file() => "modified",
                Err(_) => "missing",
                Ok(_) => match fs::read(&path) {
                    Err(_) => "missing",
                    Ok(bytes) => {
                        let hash = hash_text(&bytes);
                        let source_bytes =
                            crate::scope::embedded_asset(&format!("harness/{name}/{source}"))
                                .ok_or("missing harness source")?;
                        if hash == hash_text(source_bytes) {
                            "current"
                        } else if stamp_hash(target).as_deref() == Some(hash.as_str())
                            || in_history(target, &hash)
                        {
                            "behind"
                        } else {
                            "modified"
                        }
                    }
                },
            }
        };
        files.push(state);
    }
    let mut obsolete = Vec::new();
    let mut candidates: Vec<(String, String)> = manifest
        .get("obsoleteFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|file| (skill_dir.to_owned(), file.to_owned()))
        .collect();
    for entry in manifest
        .get("legacy")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let dir = str_of(entry, "dir").unwrap_or("").to_owned();
        for file in entry
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            candidates.push((dir.clone(), file.to_owned()));
        }
    }
    for (dir, file) in candidates {
        crate::workflow::assert_no_symlink_components(root, &dir)?;
        let path = root.join(&dir).join(&file);
        if !path.exists() {
            continue;
        }
        let mut state = "modified";
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()) {
            if let Ok(bytes) = fs::read(&path) {
                let hash = hash_text(&bytes);
                let prior =
                    stamp_hash(&file).or_else(|| in_history(&file, &hash).then(|| hash.clone()));
                if prior.as_deref() == Some(hash.as_str()) {
                    state = "pristine";
                }
            }
        }
        obsolete.push(state);
    }
    let stamped = stamp
        .as_ref()
        .and_then(|stamp| stamp.get("engineVersion"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let ahead = compare_versions(stamped.as_deref(), version) == std::cmp::Ordering::Greater
        && files.iter().any(|state| *state != "current");
    let state = if ahead {
        "ahead"
    } else if files.contains(&"missing") {
        "missing"
    } else if files
        .iter()
        .chain(obsolete.iter())
        .any(|state| *state == "modified")
    {
        "modified"
    } else if files.contains(&"behind") || obsolete.contains(&"pristine") {
        "behind"
    } else {
        "current"
    };
    Ok((state.to_owned(), stamped))
}

/// Node `projectionNotices(root, engineVersion)`, against the bundled harness files.
pub fn projection_notices(root: &Path, version: &str) -> Vec<String> {
    let run = || -> OrchestrateResult<Vec<String>> {
        let bytes = fs::read(config_path(root)).map_err(|e| e.to_string())?;
        let config: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if !config.is_object() {
            return Ok(Vec::new());
        }
        let mut notices = Vec::new();
        for name in configured_harnesses(&config) {
            let manifest = harness_manifest(&name).ok_or("missing manifest")?;
            let invoke = str_of(&manifest, "invoke").unwrap_or("").to_owned();
            let (state, stamped) = projection_state(root, &name, &manifest, version)?;
            match state.as_str() {
                "current" => {}
                "ahead" => notices.push(format!(
                    "The projected {invoke} skill for {name} was written by engine {}, newer than the installed {version}. Ask the user to upgrade dtech-aidlc.",
                    stamped.unwrap_or_else(|| "null".into())
                )),
                other => notices.push(format!(
                    "The projected {invoke} skill for {name} is {other} relative to engine {version}. Ask the user to run: dtech-aidlc harness refresh"
                )),
            }
        }
        Ok(notices)
    };
    run().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// orchestrate report
// ---------------------------------------------------------------------------

fn artifact_location(
    root: &Path,
    graph: &Value,
    intent_id: &str,
    output: &str,
) -> Option<ExpectedArtifact> {
    let stage = graph
        .get("stages")
        .and_then(Value::as_array)?
        .iter()
        .find(|stage| {
            stage
                .get("produces")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(output)))
        })?;
    expected_artifacts(root, graph, &json!({"intentId": intent_id}), stage)
        .ok()?
        .into_iter()
        .find(|item| item.output == output)
}

fn read_text(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Node `unitIdsFromFile`.
fn unit_ids_from_file(path: Option<&Path>) -> Option<HashSet<String>> {
    let text = read_text(path?)?;
    let data: Value = serde_json::from_str(&text).ok()?;
    let units = units_from_data(&data);
    if units.iter().any(Value::is_null) {
        return None;
    }
    Some(
        units
            .iter()
            .filter_map(|row| row.get("id").and_then(Value::as_str))
            .map(crate::ids::normalize_id)
            .collect(),
    )
}

struct Registry {
    path: Option<String>,
    text: Option<String>,
    usable: bool,
    functions: Vec<String>,
}

/// Node `loadRegistry` (only the parts the report path needs).
fn load_registry(root: &Path, graph: &Value, intent_id: &str) -> Registry {
    let location = artifact_location(root, graph, intent_id, "id-registry");
    let text = location.as_ref().and_then(|item| read_text(&item.path));
    let mut registry = Registry {
        path: location.map(|item| item.rel_path),
        text: text.clone(),
        usable: false,
        functions: Vec::new(),
    };
    let Some(text) = text else { return registry };
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return registry;
    };
    if data.is_object() && str_of(&data, "status") == Some("not-applicable") {
        return registry;
    }
    registry.usable = true;
    registry.functions = data
        .pointer("/ids/FN")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(crate::ids::normalize_id)
        .collect();
    registry
}

/// Node `citationReport(root, graph, intentId, files)` on the stage's expected artifacts.
fn citation_report(
    root: &Path,
    graph: &Value,
    intent_id: &str,
    files: &[ExpectedArtifact],
) -> crate::ids::CitationReport {
    let registry = load_registry(root, graph, intent_id);
    let units = unit_ids_from_file(
        artifact_location(root, graph, intent_id, "work-units-json")
            .as_ref()
            .map(|item| item.path.as_path()),
    );
    let mut analyze = HashMap::new();
    for (output, prefix) in [("acceptance-criteria", "AC"), ("user-stories", "US")] {
        let defined = artifact_location(root, graph, intent_id, output)
            .and_then(|item| read_text(&item.path))
            .map(|text| {
                crate::ids::cited_ids(&text)
                    .into_iter()
                    .filter(|id| crate::ids::prefix_of(id) == Some(prefix))
                    .collect::<HashSet<_>>()
            });
        analyze.insert(prefix.to_owned(), defined);
    }
    let contents = files
        .iter()
        .filter(|item| item.rel_path.ends_with(".md"))
        .filter_map(|item| read_text(&item.path).map(|text| (item.rel_path.clone(), text)))
        .collect::<Vec<_>>();
    crate::ids::citation_report(
        registry.path.as_deref(),
        registry.text.as_deref(),
        units.as_ref(),
        &analyze,
        &contents,
    )
}

fn format_unregistered(rows: &[crate::ids::UnregisteredCitation]) -> String {
    let mut lines = rows
        .iter()
        .take(10)
        .map(|row| format!("  {} in {} (not in {})", row.id, row.file, row.source))
        .collect::<Vec<_>>();
    if rows.len() > 10 {
        lines.push(format!("  ... and {} more", rows.len() - 10));
    }
    lines.join("\n")
}

fn markdown_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            markdown_files_under(&path, out);
        } else if kind.is_file() && entry.file_name().to_string_lossy().ends_with(".md") {
            out.push(path);
        }
    }
}

/// Node `uncoveredFunctions`.
fn uncovered_functions(root: &Path, graph: &Value, intent_id: &str) -> Vec<String> {
    let registry = load_registry(root, graph, intent_id);
    if !registry.usable || registry.functions.is_empty() {
        return Vec::new();
    }
    let mut reports = Vec::new();
    markdown_files_under(
        &intent_dir_path(root, intent_id)
            .join("develop")
            .join("build-and-verify"),
        &mut reports,
    );
    let cited: HashSet<String> = reports
        .iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name == "green-verification-report.md")
        })
        .filter_map(|path| read_text(path))
        .flat_map(|text| crate::ids::cited_ids(&text))
        .collect();
    registry
        .functions
        .into_iter()
        .filter(|id| !cited.contains(id))
        .collect()
}

struct OpenItems {
    present: bool,
    none: bool,
    written_approval: usize,
    follow_up: usize,
    untagged: usize,
}

fn heading_level(line: &str) -> Option<(usize, &str)> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    let rest = &line[hashes..];
    if hashes == 0 || !rest.starts_with(|c: char| is_js_space(c)) {
        return None;
    }
    Some((hashes, rest))
}

fn list_item(line: &str) -> Option<&str> {
    let marker = if line.starts_with(['-', '*']) {
        1
    } else {
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || line.as_bytes().get(digits) != Some(&b'.') {
            return None;
        }
        digits + 1
    };
    let rest = &line[marker..];
    rest.starts_with(|c: char| is_js_space(c))
        .then(|| crate::identity::js_trim(rest))
}

fn starts_with_tag(item: &str, tag: &str) -> bool {
    item.get(..tag.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(tag))
}

/// Node `gateOpenItems`.
fn gate_open_items(text: &str) -> OpenItems {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let trim = crate::identity::js_trim;
    let start = lines.iter().position(|line| {
        heading_level(trim(line)).is_some_and(|(level, title)| {
            (2..=3).contains(&level) && trim(title).eq_ignore_ascii_case("open items")
        })
    });
    let Some(start) = start else {
        return OpenItems {
            present: false,
            none: false,
            written_approval: 0,
            follow_up: 0,
            untagged: 0,
        };
    };
    let body: Vec<&str> = lines[start + 1..]
        .iter()
        .take_while(|line| !heading_level(trim(line)).is_some_and(|(level, _)| level <= 3))
        .map(|line| trim(line))
        .filter(|line| !line.is_empty())
        .collect();
    let items: Vec<&str> = body.iter().filter_map(|line| list_item(line)).collect();
    let is_none =
        |line: &str| line.eq_ignore_ascii_case("none") || line.eq_ignore_ascii_case("none.");
    let open: Vec<&str> = items
        .iter()
        .copied()
        .filter(|item| !is_none(item))
        .collect();
    let written_approval = open
        .iter()
        .filter(|item| starts_with_tag(item, "[written-approval]"))
        .count();
    let follow_up = open
        .iter()
        .filter(|item| starts_with_tag(item, "[follow-up]"))
        .count();
    OpenItems {
        present: true,
        none: open.is_empty()
            && (items.iter().any(|item| is_none(item)) || body.iter().any(|line| is_none(line))),
        written_approval,
        follow_up,
        untagged: open.len() - written_approval - follow_up,
    }
}

/// Node `aiApprovalProblem`.
fn ai_approval_problem(text: Option<&str>, gate_id: &str) -> Option<String> {
    let Some(text) = text else {
        return Some(format!("AI gate approval for {gate_id} requires a gate-recommendation artifact; the agent must research options and write its recommendation first."));
    };
    let open = gate_open_items(text);
    if !open.present {
        return Some(format!("gate-recommendation.md for {gate_id} needs an '## Open items' section: list each deferred item tagged [written-approval] or [follow-up], or write '- None'."));
    }
    if !open.none && open.written_approval + open.follow_up + open.untagged == 0 {
        return Some(format!("gate-recommendation.md for {gate_id} has an empty '## Open items' section; write '- None' when nothing is open."));
    }
    if open.untagged > 0 {
        return Some(format!("gate-recommendation.md for {gate_id} has {} open item(s) without a [written-approval] or [follow-up] tag; classify each one.", open.untagged));
    }
    if open.written_approval > 0 {
        return Some(format!("Gate {gate_id} cannot be approved by the AI Orchestrator: gate-recommendation.md leaves {} item(s) that need written approval open. Report the stage blocked with those items in --detail, or ask the owner to pin the gate to human approval with 'dtech-aidlc config gate-mode {gate_id} human'.", open.written_approval));
    }
    None
}

/// Node `readGateRecommendation`: the text when it is readable and not blank.
fn read_gate_recommendation(root: &Path, intent_id: &str, stage: &Value) -> Option<String> {
    let path = intent_dir_path(root, intent_id)
        .join(str_of(stage, "phase").unwrap_or(""))
        .join(str_of(stage, "id").unwrap_or(""))
        .join("gate-recommendation.md");
    let text = read_text(&path)?;
    (!crate::identity::js_trim(&text).is_empty()).then_some(text)
}

fn verified_json(item: &VerifiedArtifact) -> Value {
    let mut row = Map::new();
    row.insert("output".into(), json!(item.output));
    row.insert("path".into(), json!(item.path));
    row.insert("bytes".into(), json!(item.bytes));
    row.insert("sha256".into(), json!(item.sha256));
    if item.not_applicable {
        row.insert("notApplicable".into(), json!(true));
        row.insert("reason".into(), json!(item.reason));
    }
    Value::Object(row)
}

/// Node `orchestrateReport`. `root` must be absolute. Methodology templates named in
/// messages point under the embedded engine; see [`orchestrate_report_with_engine`].
pub fn orchestrate_report(
    root: &Path,
    graph: &Value,
    config: &Value,
    options: &ReportOptions,
) -> OrchestrateResult<Value> {
    orchestrate_report_with_engine(root, graph, config, &Engine::embedded(root), options)
}

/// [`orchestrate_report`] with methodology templates under `engine`.
pub fn orchestrate_report_with_engine(
    root: &Path,
    graph: &Value,
    config: &Value,
    engine: &Engine,
    options: &ReportOptions,
) -> OrchestrateResult<Value> {
    let loaded = load_workflow(root, graph, true)?;
    let mut state = loaded.state.clone().ok_or("No active workflow state")?;
    let current = state
        .get("currentStage")
        .filter(|stage| truthy(Some(stage)))
        .map(js_string)
        .ok_or("No current stage; run orchestrate next first")?;
    let requested_flag = options.stage.as_deref().filter(|stage| !stage.is_empty());
    if let Some(flag) = requested_flag {
        if stage_by_id(graph, flag).is_none() {
            return Err(format!("Unknown stage '{flag}'"));
        }
    }
    let stage = stage_by_id(graph, requested_flag.unwrap_or(&current))
        .cloned()
        .ok_or("No current stage; run orchestrate next first")?;
    let stage_id = str_of(&stage, "id").unwrap_or("").to_owned();
    if stage_id != current {
        return Err(format!(
            "Only the current stage ({current}) can be reported"
        ));
    }
    let result = options.result.as_deref().unwrap_or("");
    if !matches!(result, "complete" | "blocked" | "skipped") {
        return Err("--result must be complete, blocked, or skipped".into());
    }
    let detail = options
        .detail
        .as_deref()
        .filter(|detail| !detail.is_empty());
    if matches!(result, "blocked" | "skipped") && detail.is_none() {
        return Err(format!("--result {result} requires --detail \"<reason>\""));
    }
    let approval = options
        .approval
        .as_deref()
        .filter(|value| !value.is_empty());
    if approval.is_some_and(|value| !matches!(value, "approved" | "rejected")) {
        return Err("--approval must be approved or rejected".into());
    }
    if approval == Some("approved") && result == "blocked" {
        return Err("--approval approved cannot be combined with --result blocked; a blocked report never passes a gate".into());
    }
    let mut needs: Vec<String> = Vec::new();
    for need in &options.needs {
        if !needs.contains(need) {
            needs.push(need.clone());
        }
    }
    if !needs.is_empty() {
        if result != "blocked" {
            return Err("--needs is only accepted with --result blocked".into());
        }
        crate::prerequisites::require_known_prerequisites(config, &needs)?;
    }
    let scope = scope_of(&state).to_owned();
    let depth = depth_of(&state).to_owned();
    let stages: Vec<Value> = ordered_stages(graph, &scope, &depth)
        .into_iter()
        .cloned()
        .collect();
    if !stages
        .iter()
        .any(|item| str_of(item, "id") == Some(stage_id.as_str()))
    {
        return Err(format!(
            "Stage {stage_id} is not included in the active {scope} scope"
        ));
    }
    let gate = gate_info(graph, &scope, &stage, &depth);
    let phase_gate = is_phase_gate(graph, &state, &stage);
    let approved_by = options
        .approved_by
        .as_deref()
        .filter(|value| !value.is_empty());
    let approval_note = options
        .approval_note
        .as_deref()
        .filter(|value| !value.is_empty());
    if !phase_gate && (approval.is_some() || approved_by.is_some() || approval_note.is_some()) {
        let final_stage = gate
            .final_stage
            .clone()
            .unwrap_or_else(|| "undefined".into());
        let place = if Some(stage_id.as_str()) == gate.final_stage.as_deref() {
            format!(
                "on the last work unit of {stage_id}; work unit {} is not the last",
                state
                    .get("currentUnit")
                    .map(js_string)
                    .unwrap_or_else(|| "undefined".into())
            )
        } else {
            format!(
                "on {final_stage}, the last stage of the {} phase",
                js_string(&gate.phase)
            )
        };
        return Err(format!("Stage {stage_id} is not a phase gate here; --approval, --approved-by and --approval-note are only accepted {place}"));
    }
    if (approved_by.is_some() || approval_note.is_some()) && approval.is_none() {
        return Err("--approved-by and --approval-note require --approval".into());
    }
    if approval == Some("rejected") && result != "blocked" {
        return Err("--approval rejected must be reported with --result blocked --detail \"<the owner's feedback>\"".into());
    }
    if phase_gate && matches!(result, "complete" | "skipped") && approval != Some("approved") {
        return Err(format!("Phase gate {} after {stage_id} requires explicit --approval approved from the project owner (name them with --approved-by NAME); this also applies when the stage is skipped.", js_string(&gate.id)));
    }
    let intent_id = str_of(&state, "intentId").unwrap_or("").to_owned();
    let mut verified: Vec<VerifiedArtifact> = Vec::new();
    if result == "complete" {
        let expected = expected_artifacts(root, graph, &state, &stage)?;
        let check = verify_artifacts(root, &expected, Some(graph), Some(&state));
        if !check.ok {
            let first = check
                .problems
                .first()
                .map(|problem| {
                    format!(
                        "{} is {} ({})",
                        problem.output, problem.problem, problem.path
                    )
                })
                .unwrap_or_else(|| "artifact validation failed".into());
            return Err(format!("Stage {stage_id} cannot be completed: {first}"));
        }
        verified = check.artifacts;
        let required = crate::orchestration_context::extra_required_artifacts_with_engine(
            root,
            &stage,
            engine.root(),
        );
        let required_items: Vec<ExpectedArtifact> = required
            .iter()
            .map(|item| {
                let rel = item.get("path").map(js_string).unwrap_or_default();
                let mut path = root.to_path_buf();
                for part in rel.split('/').filter(|part| !part.is_empty()) {
                    path.push(part);
                }
                ExpectedArtifact {
                    output: rel.clone(),
                    kind: "document".into(),
                    path,
                    rel_path: rel,
                }
            })
            .collect();
        let required_check = verify_artifacts(root, &required_items, Some(graph), Some(&state));
        if !required_check.ok {
            let missing: HashSet<&str> = required_check
                .problems
                .iter()
                .map(|problem| problem.path.as_str())
                .collect();
            let detail_lines = required
                .iter()
                .filter(|item| {
                    missing.contains(item.get("path").and_then(Value::as_str).unwrap_or(""))
                })
                .map(|item| {
                    format!(
                        "  missing: {} ({})",
                        item.get("path").map(js_string).unwrap_or_default(),
                        item.get("reason").map(js_string).unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!("Stage {stage_id} cannot be reported complete: {} of {} expected artifacts are missing or invalid.\n{detail_lines}", missing.len(), required.len()));
        }
        if stage_id == "knowledge-writeback" {
            let check = crate::knowledge::knowledge_gate_check(root, graph, &state);
            if check.get("ok") != Some(&Value::Bool(true)) {
                return Err(format!(
                    "Knowledge record is not ready: {} ({})",
                    check
                        .pointer("/errors/0")
                        .map(js_string)
                        .unwrap_or_default(),
                    check.get("path").map(js_string).unwrap_or_default()
                ));
            }
        }
        if crate::ids::CITATION_PHASES.contains(&str_of(&stage, "phase").unwrap_or("")) {
            let citations = citation_report(root, graph, &intent_id, &expected);
            if !citations.unregistered.is_empty() {
                return Err(format!(
                    "Stage {stage_id} cannot be reported complete: {} cited identifier(s) are not registered.\n{}\nCite only identifiers in {} and the Analyze artifacts. A new identifier is minted by implementation-design: report this stage blocked and rework the design. Run 'dtech-aidlc ids check' to list them.",
                    citations.unregistered.len(),
                    format_unregistered(&citations.unregistered),
                    citations.registry.as_deref().unwrap_or("null")
                ));
            }
        }
    }

    let now = crate::audit::now_iso();
    let previous = state
        .get("stages")
        .and_then(|rows| rows.get(&stage_id))
        .filter(|row| truthy(Some(row)))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mode = if phase_gate {
        effective_gate_mode(config, &state, &gate.id)
    } else {
        mode_of(&state).to_owned()
    };
    let gate_pinned = mode != mode_of(&state);
    let mut open_items: Option<Value> = None;
    let mut approval_record: Option<Value> = None;
    let mut recorded_approval: Option<&str> = None;
    let mut approver_by = Value::Null;
    let mut approver_source = Value::Null;
    if phase_gate {
        if let Some(decision) = approval {
            let who = crate::identity::resolve_approver(root, approved_by)?;
            if mode == "ai" && who.by != AI_ORCHESTRATOR {
                return Err(format!("Intent '{intent_id}' runs in AI orchestrator mode; gate approvals must use --approved-by \"{AI_ORCHESTRATOR}\". Human approvals apply to human-mode intents."));
            }
            if mode == "human" && who.by == AI_ORCHESTRATOR {
                return Err(if gate_pinned {
                    format!("Gate {} is pinned to human mode by 'config gate-mode'; \"{AI_ORCHESTRATOR}\" cannot approve it. Present it to the owner.", gate.id_str())
                } else {
                    format!("Intent '{intent_id}' runs in human mode; \"{AI_ORCHESTRATOR}\" cannot approve a human-mode gate.")
                });
            }
            if mode == "ai" && decision == "approved" {
                let text = read_gate_recommendation(root, &intent_id, &stage);
                if let Some(problem) = ai_approval_problem(text.as_deref(), &js_string(&gate.id)) {
                    return Err(problem);
                }
                let open = gate_open_items(text.as_deref().unwrap_or(""));
                open_items = Some(
                    json!({"writtenApproval": open.written_approval, "followUp": open.follow_up}),
                );
            }
            if mode == "human" {
                if let Some(allowed) = allowed_approvers(config, &js_string(&gate.id)) {
                    if !allowed
                        .iter()
                        .any(|name| name.as_str() == Some(who.by.as_str()))
                    {
                        return Err(format!(
                            "Approver '{}' is not allowed for gate {}. Allowed: {}",
                            who.by,
                            js_string(&gate.id),
                            allowed.iter().map(js_string).collect::<Vec<_>>().join(", ")
                        ));
                    }
                }
            }
            recorded_approval = Some(decision);
            approver_by = json!(who.by);
            approver_source = json!(who.by_source);
            let mut record = Map::new();
            record.insert("decision".into(), json!(decision));
            record.insert("by".into(), approver_by.clone());
            record.insert("bySource".into(), approver_source.clone());
            record.insert("mode".into(), json!(mode));
            record.insert("note".into(), json!(approval_note));
            if gate_pinned {
                record.insert("pinned".into(), json!(true));
            }
            if let Some(open) = &open_items {
                record.insert("openItems".into(), open.clone());
            }
            approval_record = Some(Value::Object(record));
        }
    }

    // All validation has passed; build the updated state.
    let mut reports = previous
        .get("reports")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut report = Map::new();
    report.insert("at".into(), json!(now));
    report.insert("result".into(), json!(result));
    report.insert("detail".into(), json!(detail));
    if !needs.is_empty() {
        report.insert("needs".into(), json!(needs));
    }
    if let Some(record) = &approval_record {
        report.insert("approval".into(), record.clone());
    }
    reports.push(Value::Object(report));
    let repeat = str_of(&stage, "repeat") == Some("work-unit");
    let unit_key = unit_key(current_unit(&state));
    let mut unit_map = previous
        .get("units")
        .filter(|units| truthy(Some(units)))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if repeat {
        unit_map.insert(
            unit_key.clone(),
            json!({"status": result, "reportedAt": now}),
        );
    }
    let repeating_done = repeat
        && work_units_of(&state).iter().all(|unit| {
            let key = unit_key_of(unit);
            let status = if key == unit_key {
                Some(result)
            } else {
                previous
                    .pointer(&format!(
                        "/units/{}",
                        key.replace('~', "~0").replace('/', "~1")
                    ))
                    .and_then(|row| row.get("status"))
                    .and_then(Value::as_str)
            };
            stage_finished(status)
        });
    let status_value = if repeat && !repeating_done {
        "in_progress"
    } else {
        result
    };
    if repeat
        && stage_finished(Some(result))
        && truthy(state.get("units"))
        && last_repeat_of_phase(graph, &state, &stage)
    {
        let current_id = current_unit(&state).cloned();
        if let Some(units) = state.get_mut("units").and_then(Value::as_array_mut) {
            if let Some(unit) = units
                .iter_mut()
                .find(|unit| unit_id(unit).cloned() == current_id)
            {
                unit["status"] = json!(result);
            }
        }
    }
    let mut row = previous.as_object().cloned().unwrap_or_default();
    row.insert("status".into(), json!(status_value));
    row.insert("reportedAt".into(), json!(now));
    row.insert("detail".into(), json!(detail));
    row.insert("reports".into(), Value::Array(reports));
    row.insert("units".into(), Value::Object(unit_map));
    if result == "complete" {
        let mut artifacts = previous
            .get("artifacts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        artifacts.extend(verified.iter().map(verified_json));
        row.insert("artifacts".into(), Value::Array(artifacts));
    }
    state["stages"][&stage_id] = Value::Object(row);
    let mut skipped_stages = Value::Array(Vec::new());
    if approval_record.is_some() {
        skipped_stages = Value::Array(
            stages
                .iter()
                .filter(|item| str_of(item, "phase") == str_of(&stage, "phase"))
                .filter(|item| {
                    let id = str_of(item, "id").unwrap_or("");
                    let status = if id == stage_id {
                        Some(result)
                    } else {
                        stage_status(&state, item)
                    };
                    status == Some("skipped")
                })
                .filter_map(|item| item.get("id").cloned())
                .collect(),
        );
        if !state.get("gates").is_some_and(|gates| !gates.is_null()) {
            state["gates"] = json!({});
        }
        let mut gate_row = Map::new();
        gate_row.insert("phase".into(), gate.phase.clone());
        gate_row.insert("stage".into(), json!(stage_id));
        gate_row.insert("decision".into(), json!(approval));
        gate_row.insert("result".into(), json!(result));
        gate_row.insert("by".into(), approver_by.clone());
        gate_row.insert("bySource".into(), approver_source.clone());
        gate_row.insert("mode".into(), json!(mode));
        gate_row.insert("at".into(), json!(now));
        gate_row.insert("note".into(), json!(approval_note));
        gate_row.insert("detail".into(), json!(detail));
        gate_row.insert("skippedStages".into(), skipped_stages.clone());
        gate_row.insert("artifacts".into(), json!([]));
        if gate_pinned {
            gate_row.insert("pinned".into(), json!(true));
        }
        if let Some(open) = &open_items {
            gate_row.insert("openItems".into(), open.clone());
        }
        state["gates"][js_string(&gate.id)] = Value::Object(gate_row);
    }
    state["currentStage"] = Value::Null;
    crate::audit::preflight_audit(root)?;
    persist_active(root, graph, &loaded, &mut state)?;

    let mut reported = Map::new();
    reported.insert("intent".into(), state["intent"].clone());
    reported.insert("intentId".into(), json!(intent_id));
    reported.insert("stage".into(), json!(stage_id));
    reported.insert(
        "phase".into(),
        stage.get("phase").cloned().unwrap_or(Value::Null),
    );
    reported.insert("result".into(), json!(result));
    reported.insert("detail".into(), json!(detail));
    reported.insert("phaseGate".into(), json!(phase_gate));
    reported.insert(
        "gate".into(),
        if phase_gate {
            gate.id.clone()
        } else {
            Value::Null
        },
    );
    reported.insert("gateApproval".into(), json!(recorded_approval));
    reported.insert("approvedBy".into(), approver_by.clone());
    if !needs.is_empty() {
        reported.insert("needs".into(), json!(needs));
    }
    audit(root, "stage.reported", Value::Object(reported))?;
    if approval_record.is_some() {
        let mut decided = Map::new();
        decided.insert("intentId".into(), json!(intent_id));
        decided.insert("gate".into(), gate.id.clone());
        decided.insert("phase".into(), gate.phase.clone());
        decided.insert("stage".into(), json!(stage_id));
        decided.insert("decision".into(), json!(approval));
        decided.insert("result".into(), json!(result));
        decided.insert("by".into(), approver_by.clone());
        decided.insert("bySource".into(), approver_source.clone());
        decided.insert("mode".into(), json!(mode));
        decided.insert("note".into(), json!(approval_note));
        decided.insert("detail".into(), json!(detail));
        decided.insert("skippedStages".into(), skipped_stages);
        decided.insert("artifacts".into(), json!([]));
        if gate_pinned {
            decided.insert("pinned".into(), json!(true));
        }
        if let Some(open) = &open_items {
            decided.insert("openItems".into(), open.clone());
        }
        audit(root, "gate.decided", Value::Object(decided))?;
    }
    let mut warnings = Vec::new();
    if result == "complete" && phase_gate && stage_id == "build-and-verify" {
        let uncovered = uncovered_functions(root, graph, &intent_id);
        if !uncovered.is_empty() {
            warnings.push(format!(
                "Registered functions with no row in any green verification report trace table: {}.",
                uncovered.join(", ")
            ));
        }
    }
    let mut output = Map::new();
    output.insert("recorded".into(), json!(true));
    output.insert("intentId".into(), json!(intent_id));
    output.insert("stage".into(), json!(stage_id));
    output.insert("result".into(), json!(result));
    output.insert("phaseGate".into(), json!(phase_gate));
    output.insert("gate".into(), gate.id.clone());
    output.insert("approval".into(), json!(recorded_approval));
    output.insert("approvedBy".into(), approver_by);
    output.insert(
        "intentComplete".into(),
        json!(derive_status(&state, graph) == "complete"),
    );
    if !needs.is_empty() {
        output.insert("needs".into(), json!(needs));
    }
    if !warnings.is_empty() {
        output.insert("warnings".into(), json!(warnings));
    }
    if result == "complete" {
        output.insert(
            "artifacts".into(),
            Value::Array(verified.iter().map(verified_json).collect()),
        );
    }
    Ok(Value::Object(output))
}

fn unit_key_of(unit: &Value) -> String {
    unit_key(unit_id(unit))
}

/// The CLI's work after a report (Node `dtech-aidlc.mjs`): a completed knowledge
/// writeback rewrites the knowledge index and names the record.
pub fn finish_report(root: &Path, report: &mut Value) -> OrchestrateResult<()> {
    if str_of(report, "stage") == Some("knowledge-writeback")
        && str_of(report, "result") == Some("complete")
    {
        crate::knowledge::write_index(root)?;
        report["knowledgeRecord"] = json!(format!(
            ".aidlc/knowledge/{}.md",
            str_of(report, "intentId").unwrap_or("")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "aidlc-orchestrate-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".aidlc")).unwrap();
        path
    }

    fn embedded_graph() -> Value {
        serde_json::from_slice(crate::scope::embedded_asset("data/phase-graph.json").unwrap())
            .unwrap()
    }

    fn graph() -> Value {
        json!({
            "scopes":{"solo":["phase"]},
            "phases":[{"id":"phase","gate":"phase-gate","file":"phases/phase.md"}],
            "stages":[{"id":"work","phase":"phase","order":1,"produces":["result"]}],
            "outputs":{"result":{"kind":"document"}}
        })
    }

    fn two_stage_graph() -> Value {
        json!({
            "scopes":{"solo":["phase"]},
            "phases":[{"id":"phase","gate":"phase-gate"}],
            "stages":[
                {"id":"work","phase":"phase","order":1,"produces":["result"]},
                {"id":"last","phase":"phase","order":2,"produces":["last-result"]}
            ],
            "outputs":{"result":{"kind":"document"},"last-result":{"kind":"document"}}
        })
    }

    fn write_config(root: &Path, config: &str) {
        fs::write(root.join(".aidlc/config.json"), config).unwrap();
    }

    fn config(root: &Path) -> Value {
        crate::scope::load_config(root).unwrap()
    }

    fn engine(root: &Path) -> Engine {
        Engine::external(root.join("engine"))
    }

    fn next(root: &Path, graph: &Value, options: NextOptions) -> OrchestrateResult<NextOutcome> {
        orchestrate_next(root, graph, &config(root), &engine(root), &options)
    }

    fn start(root: &Path, graph: &Value, intent: &str) -> Value {
        let options = NextOptions {
            intent: Some(intent.into()),
            ..NextOptions::default()
        };
        match next(root, graph, options).unwrap() {
            NextOutcome::Directive(directive) => directive,
            other => panic!("expected a directive, got {other:?}"),
        }
    }

    fn report(root: &Path, graph: &Value, options: ReportOptions) -> OrchestrateResult<Value> {
        orchestrate_report(root, graph, &config(root), &options)
    }

    fn result(value: &str) -> ReportOptions {
        ReportOptions {
            result: Some(value.into()),
            ..ReportOptions::default()
        }
    }

    fn approved(by: &str) -> ReportOptions {
        ReportOptions {
            approval: Some("approved".into()),
            approved_by: Some(by.into()),
            ..result("complete")
        }
    }

    fn intent_state(root: &Path, id: &str) -> Value {
        let path = root.join(format!(".aidlc/intents/{id}/state.json"));
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    fn audit_events(root: &Path) -> Vec<String> {
        fs::read_to_string(root.join(".aidlc/audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                row["event"].as_str().unwrap().to_owned()
            })
            .collect()
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push((path.clone(), fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&root.join(".aidlc"), &mut out);
        out.sort();
        out
    }

    fn write_artifact(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn next_emits_compatible_directive_envelope_and_default_context_grants() {
        let root = temp();
        write_config(&root, "{}");
        let directive = start(&root, &graph(), "test intent");

        // Node's directive has no `complete` key; completion payloads do.
        assert!(directive.get("complete").is_none());
        let dir = root.join(".aidlc").join("intents").join("test-intent");
        assert_eq!(directive["intentDir"], json!(dir.to_string_lossy()));
        assert_eq!(directive["scopeSource"], "config");
        assert_eq!(directive["depth"], "full");
        assert_eq!(directive["stage"]["id"], "work");
        assert_eq!(directive["context"]["schemaVersion"], 1);
        assert_eq!(
            directive["context"]["projectRoot"],
            json!(root.to_string_lossy())
        );
        for grant in [
            "request",
            "project",
            "sources",
            "knowledge",
            "profile",
            "repos",
        ] {
            assert!(directive["context"]["granted"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item == grant));
        }
        assert_eq!(
            directive["context"]["request"]["provenance"],
            "User-provided"
        );
        assert_eq!(
            fs::read_to_string(dir.join("intent.md")).unwrap(),
            "# test-intent\n\ntest intent\n"
        );
        assert_eq!(audit_events(&root), ["intent.created", "stage.selected"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn next_does_not_advance_state_when_directive_construction_fails() {
        let root = temp();
        write_config(&root, "{}");
        start(&root, &embedded_graph(), "failed selection");
        let before = snapshot(&root);
        // The selected stage's artifacts cannot be resolved, so directive construction
        // fails after selection but before anything is persisted.
        let mut broken = embedded_graph();
        broken["outputs"]["problem-statement"]["path"] = json!("../outside/{output}.md");
        assert!(next(&root, &broken, NextOptions::default()).is_err());
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn next_includes_node_parity_stage_gate_and_artifact_metadata() {
        let root = temp();
        write_config(
            &root,
            r#"{"agentModels":{"brief-analyst":"haiku"},"gateModes":{"analysis-baseline":"human"}}"#,
        );
        let graph = embedded_graph();
        let engine = Engine::embedded(&root);
        let options = NextOptions {
            intent: Some("metadata parity".into()),
            mode: Some("ai".into()),
            ..NextOptions::default()
        };
        let outcome = orchestrate_next(&root, &graph, &config(&root), &engine, &options).unwrap();
        let NextOutcome::Directive(directive) = outcome else {
            panic!("no directive")
        };
        assert_eq!(directive["stage"]["id"], "problem-framing");
        assert_eq!(directive["stage"]["phaseGate"], false);
        assert_eq!(directive["gate"]["id"], "analysis-baseline");
        assert_eq!(directive["gate"]["phase"], "analyze");
        assert_eq!(directive["gate"]["finalStage"], "context-and-risk");
        assert_eq!(directive["approvalRequiredAfter"], "context-and-risk");
        assert_eq!(directive["gateMode"], "human");
        assert_eq!(directive["mode"], "ai");
        assert_eq!(directive["stage"]["model"], "haiku");
        assert_eq!(directive["stage"]["modelSource"], "project");
        assert_eq!(
            directive["warnings"],
            json!(["Intent mode 'ai' overrides the project mode 'human' from .aidlc/config.json (given by --mode)."])
        );
        assert_eq!(
            directive["stage"]["phaseFile"],
            engine.path("methodology/phases/analyze.md")
        );
        assert_eq!(
            directive["stage"]["agentFile"],
            engine.path("methodology/agents/brief-analyst.md")
        );
        assert_eq!(
            directive["contractsFile"],
            engine.path("methodology/contracts.md")
        );
        // The embedded methodology is materialized where the directive points.
        assert!(Path::new(directive["contractsFile"].as_str().unwrap()).is_file());
        let state = intent_state(&root, "metadata-parity");
        let stage = stage_by_id(&graph, "problem-framing").unwrap();
        let expected = expected_artifacts(&root, &graph, &state, stage).unwrap();
        let artifacts = directive["artifacts"].as_array().unwrap();
        assert_eq!(artifacts.len(), expected.len());
        let acceptance = artifacts
            .iter()
            .find(|artifact| artifact["output"] == "acceptance-criteria")
            .unwrap();
        assert_eq!(acceptance["kind"], "document");
        let rel = &expected
            .iter()
            .find(|artifact| artifact.output == "acceptance-criteria")
            .unwrap()
            .rel_path;
        assert_eq!(acceptance["path"], json!(rel));
        assert_eq!(
            acceptance["template"],
            engine.path("methodology/templates/acceptance-criteria.md")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn next_respects_explicit_context_grants() {
        let root = temp();
        write_config(&root, "{}");
        let graph = json!({
            "scopes":{"solo":["phase"]},
            "phases":[{"id":"phase"}],
            "stages":[{"id":"work","phase":"phase","order":1,"inputs":{"context":["project"]}}],
            "outputs":{}
        });
        let directive = start(&root, &graph, "test intent");
        assert_eq!(directive["context"]["granted"], json!(["project"]));
        assert!(directive["context"].get("request").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn complete_report_fails_closed_without_expected_artifact() {
        let root = temp();
        write_config(&root, "{}");
        let graph = two_stage_graph();
        start(&root, &graph, "test intent");
        let before = snapshot(&root);
        assert!(report(&root, &graph, result("complete"))
            .unwrap_err()
            .contains("cannot be completed"));
        assert_eq!(snapshot(&root), before);
        assert_eq!(intent_state(&root, "test-intent")["currentStage"], "work");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gate_approval_requires_explicit_approval_and_records_identity() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        start(&root, &graph, "test intent");
        write_artifact(
            &root,
            ".aidlc/intents/test-intent/phase/work/result.md",
            "done",
        );
        let before = snapshot(&root);
        assert!(report(&root, &graph, result("complete"))
            .unwrap_err()
            .contains("requires explicit --approval approved"));
        assert_eq!(snapshot(&root), before);

        let recorded = report(&root, &graph, approved("Owner")).unwrap();
        assert_eq!(recorded["phaseGate"], true);
        assert_eq!(recorded["gate"], "phase-gate");
        assert_eq!(recorded["approval"], "approved");
        assert_eq!(recorded["approvedBy"], "Owner");
        assert_eq!(recorded["intentComplete"], true);
        let state = intent_state(&root, "test-intent");
        assert_eq!(state["currentStage"], Value::Null);
        assert_eq!(state["status"], "complete");
        let gate = &state["gates"]["phase-gate"];
        assert_eq!(gate["decision"], "approved");
        assert_eq!(gate["by"], "Owner");
        assert_eq!(gate["bySource"], "flag");
        assert_eq!(gate["mode"], "human");
        assert_eq!(
            state["stages"]["work"]["reports"][0]["approval"]["by"],
            "Owner"
        );
        assert_eq!(
            audit_events(&root),
            [
                "intent.created",
                "stage.selected",
                "stage.reported",
                "gate.decided"
            ]
        );
        // A finished intent returns Node's completion payload.
        let done = next(&root, &graph, NextOptions::default()).unwrap();
        assert_eq!(done.value()["complete"], true);
        assert_eq!(done.value()["approvals"][0]["gate"], "phase-gate");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_policy_rejects_invalid_result_and_missing_blocked_detail_without_mutation() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        start(&root, &graph, "report validation");
        let before = snapshot(&root);
        assert!(report(&root, &graph, result("unknown"))
            .unwrap_err()
            .contains("--result must be complete, blocked, or skipped"));
        assert!(report(&root, &graph, result("blocked"))
            .unwrap_err()
            .contains("requires --detail"));
        let other_stage = ReportOptions {
            stage: Some("elsewhere".into()),
            ..result("complete")
        };
        assert_eq!(
            report(&root, &graph, other_stage).unwrap_err(),
            "Unknown stage 'elsewhere'"
        );
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_policy_rejects_approval_outside_gate_and_rejected_completion() {
        let root = temp();
        write_config(&root, "{}");
        let graph = two_stage_graph();
        start(&root, &graph, "report policy");
        write_artifact(
            &root,
            ".aidlc/intents/report-policy/phase/work/result.md",
            "done",
        );
        let before = snapshot(&root);
        let error = report(&root, &graph, approved("Owner")).unwrap_err();
        assert!(error.contains("is not a phase gate here"), "{error}");
        assert_eq!(snapshot(&root), before);

        report(&root, &graph, result("complete")).unwrap();
        start(&root, &graph, "report policy");
        let before = snapshot(&root);
        let rejected_complete = ReportOptions {
            approval: Some("rejected".into()),
            approved_by: Some("Owner".into()),
            ..result("complete")
        };
        let error = report(&root, &graph, rejected_complete).unwrap_err();
        assert!(
            error.contains("--approval rejected must be reported with --result blocked"),
            "{error}"
        );
        assert_eq!(snapshot(&root), before);

        // A rejection is recorded as a blocked report carrying the decision.
        let rejection = ReportOptions {
            detail: Some("owner feedback".into()),
            approval: Some("rejected".into()),
            approved_by: Some("Owner".into()),
            ..result("blocked")
        };
        let recorded = report(&root, &graph, rejection).unwrap();
        assert_eq!(recorded["approval"], "rejected");
        let state = intent_state(&root, "report-policy");
        assert_eq!(state["gates"]["phase-gate"]["decision"], "rejected");
        assert_eq!(state["stages"]["last"]["status"], "blocked");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blocked_report_validates_named_prerequisites_and_records_needs() {
        let root = temp();
        write_config(
            &root,
            r#"{"prerequisites":{"build-tools":{"check":"exit 0"}}}"#,
        );
        let graph = graph();
        start(&root, &graph, "blocked with needs");
        let blocked = ReportOptions {
            detail: Some("waiting on toolchain".into()),
            needs: vec!["build-tools".into(), "build-tools".into()],
            ..result("blocked")
        };
        let recorded = report(&root, &graph, blocked).unwrap();
        assert_eq!(recorded["needs"], json!(["build-tools"]));
        let state = intent_state(&root, "blocked-with-needs");
        assert_eq!(
            state["stages"]["work"]["reports"][0]["needs"],
            json!(["build-tools"])
        );
        // The next selection re-checks the named prerequisite and reports a rework.
        let outcome = next(&root, &graph, NextOptions::default()).unwrap();
        let NextOutcome::Directive(directive) = outcome else {
            panic!("expected directive")
        };
        assert_eq!(directive["stage"]["rework"]["attempt"], 2);
        assert_eq!(
            directive["stage"]["rework"]["needs"],
            json!(["build-tools"])
        );
        assert_eq!(directive["prerequisites"][0]["name"], "build-tools");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blocked_report_rejects_undeclared_prerequisites_without_mutation() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        start(&root, &graph, "unknown need");
        let before = snapshot(&root);
        let blocked = ReportOptions {
            detail: Some("waiting".into()),
            needs: vec!["not-declared".into()],
            ..result("blocked")
        };
        let error = report(&root, &graph, blocked).unwrap_err();
        assert!(error.contains("Unknown prerequisite"), "{error}");
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approval_note_is_validated_and_recorded_with_the_gate() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        start(&root, &graph, "approval note");
        let before = snapshot(&root);
        let note_only = ReportOptions {
            detail: Some("owner feedback".into()),
            approval_note: Some("Please revisit the risk section".into()),
            ..result("blocked")
        };
        let missing_decision = report(&root, &graph, note_only).unwrap_err();
        assert!(
            missing_decision.contains("require --approval"),
            "{missing_decision}"
        );
        assert_eq!(snapshot(&root), before);

        write_artifact(
            &root,
            ".aidlc/intents/approval-note/phase/work/result.md",
            "done",
        );
        let with_note = ReportOptions {
            approval_note: Some("Reviewed and accepted".into()),
            ..approved("Owner")
        };
        report(&root, &graph, with_note).unwrap();
        let state = intent_state(&root, "approval-note");
        assert_eq!(
            state["gates"]["phase-gate"]["note"],
            "Reviewed and accepted"
        );
        assert_eq!(
            state["stages"]["work"]["reports"][0]["approval"]["note"],
            "Reviewed and accepted"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_policy_enforces_configured_approvers() {
        let root = temp();
        write_config(&root, r#"{"roles":{"approvers":{"phase-gate":["Owner"]}}}"#);
        let graph = graph();
        start(&root, &graph, "configured approval");
        write_artifact(
            &root,
            ".aidlc/intents/configured-approval/phase/work/result.md",
            "done",
        );
        let before = snapshot(&root);
        assert_eq!(
            report(&root, &graph, approved("Someone Else")).unwrap_err(),
            "Approver 'Someone Else' is not allowed for gate phase-gate. Allowed: Owner"
        );
        let error = report(&root, &graph, approved(AI_ORCHESTRATOR)).unwrap_err();
        assert!(
            error.contains("cannot approve a human-mode gate"),
            "{error}"
        );
        assert_eq!(snapshot(&root), before);
        let recorded = report(&root, &graph, approved("Owner")).unwrap();
        assert_eq!(recorded["approvedBy"], "Owner");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ai_gate_approval_requires_a_classified_recommendation() {
        let root = temp();
        write_config(&root, r#"{"mode":"ai"}"#);
        let graph = graph();
        start(&root, &graph, "ai gate");
        write_artifact(&root, ".aidlc/intents/ai-gate/phase/work/result.md", "done");
        let approve = || report(&root, &graph, approved(AI_ORCHESTRATOR));
        assert!(approve()
            .unwrap_err()
            .contains("requires a gate-recommendation artifact"));
        let recommendation = ".aidlc/intents/ai-gate/phase/work/gate-recommendation.md";
        write_artifact(
            &root,
            recommendation,
            "# Rec\n\n## Open items\n- unlabelled\n",
        );
        assert!(approve()
            .unwrap_err()
            .contains("without a [written-approval] or [follow-up] tag"));
        write_artifact(
            &root,
            recommendation,
            "## Open items\n- [written-approval] budget\n",
        );
        assert!(approve()
            .unwrap_err()
            .contains("cannot be approved by the AI Orchestrator"));
        write_artifact(
            &root,
            recommendation,
            "## Open items\n- [follow-up] telemetry\n",
        );
        approve().unwrap();
        let gate = &intent_state(&root, "ai-gate")["gates"]["phase-gate"];
        assert_eq!(
            gate["openItems"],
            json!({"writtenApproval": 0, "followUp": 1})
        );
        assert_eq!(gate["mode"], "ai");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failing_prerequisite_blocks_a_new_intent_without_writing() {
        let root = temp();
        write_config(
            &root,
            r#"{"prerequisites":{"toolchain":{"check":"exit 7","hint":"Install it","stages":["work"]}}}"#,
        );
        let before = snapshot(&root);
        let options = NextOptions {
            intent: Some("needs tools".into()),
            ..NextOptions::default()
        };
        let NextOutcome::Blocked(payload) = next(&root, &graph(), options).unwrap() else {
            panic!("expected blocked")
        };
        assert_eq!(payload["intentId"], Value::Null);
        assert_eq!(payload["needs"][0]["exitCode"], 7);
        assert!(payload["hint"]
            .as_str()
            .unwrap()
            .contains("the intent was not created yet"));
        assert_eq!(snapshot(&root), before);

        let forced = NextOptions {
            intent: Some("needs tools".into()),
            force_resume: true,
            ..NextOptions::default()
        };
        assert_eq!(
            next(&root, &graph(), forced).unwrap_err(),
            "--force-resume requires --detail \"<reason>\""
        );
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_update_keeps_pointer_and_refuses_concurrent_changes() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        start(&root, &graph, "keep pointer");
        let pointer = fs::read(root.join(".aidlc/state.json")).unwrap();
        start(&root, &graph, "keep pointer");
        assert_eq!(fs::read(root.join(".aidlc/state.json")).unwrap(), pointer);

        let loaded = load_workflow(&root, &graph, false).unwrap();
        let state_path = root.join(".aidlc/intents/keep-pointer/state.json");
        let changed = [fs::read(&state_path).unwrap().as_slice(), b" "].concat();
        fs::write(&state_path, &changed).unwrap();
        let mut state = loaded.state.clone().unwrap();
        let error = persist_active(&root, &graph, &loaded, &mut state).unwrap_err();
        assert!(
            error.contains("changed while this command was running"),
            "{error}"
        );
        assert_eq!(fs::read(&state_path).unwrap(), changed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_intent_errors_follow_node_order_without_writes() {
        let root = temp();
        let graph = embedded_graph();
        let intent = |depth: Option<&str>, by: Option<&str>| NextOptions {
            intent: Some("x feature".into()),
            depth: depth.map(str::to_owned),
            approved_by: by.map(str::to_owned),
            ..NextOptions::default()
        };
        let uninitialized = next(&root, &graph, intent(None, None)).unwrap_err();
        assert!(
            uninitialized.starts_with("Project is not initialized at "),
            "{uninitialized}"
        );
        write_config(&root, "{}");
        let before = snapshot(&root);
        assert_eq!(
            next(&root, &graph, intent(Some("fast"), None)).unwrap_err(),
            "Creating an intent with --depth requires --approved-by NAME"
        );
        assert_eq!(
            next(&root, &graph, intent(Some("fast"), Some("bad\u{1}name"))).unwrap_err(),
            crate::identity::APPROVED_BY_ERROR
        );
        assert_eq!(snapshot(&root), before);
        assert_eq!(
            next(&root, &graph, NextOptions::default()).unwrap_err(),
            "No active intent. Start one with: dtech-aidlc orchestrate next --intent \"...\""
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_state_is_migrated_before_selection() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        fs::write(
            root.join(".aidlc/state.json"),
            br#"{"intent":"Legacy item","scope":"solo","stages":{},"createdAt":"2026-01-01T00:00:00.000Z"}"#,
        )
        .unwrap();
        let outcome = next(&root, &graph, NextOptions::default()).unwrap();
        let NextOutcome::Directive(directive) = outcome else {
            panic!("expected directive")
        };
        assert_eq!(directive["intentId"], "legacy-item");
        assert!(root.join(".aidlc/state.v1.bak.json").is_file());
        let pointer: Value =
            serde_json::from_slice(&fs::read(root.join(".aidlc/state.json")).unwrap()).unwrap();
        assert_eq!(pointer["activeIntentId"], "legacy-item");
        assert_eq!(audit_events(&root), ["state.migrated", "stage.selected"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn intent_text_helpers_follow_node() {
        assert_eq!(normalize_intent_text("\u{feff} a\r\nb \n").unwrap(), "a\nb");
        assert_eq!(
            normalize_intent_text("  ").unwrap_err(),
            "Intent text is empty"
        );
        assert!(normalize_intent_text("a\0b").unwrap_err().contains("NUL"));
        assert!(same_intent_text(" Build  a\tthing ", "Build a thing"));
        assert!(!same_intent_text("Build a thing", "Build another thing"));
        // Node normalizes both sides to NFC before comparing.
        assert!(same_intent_text("Caf\u{e9} menu", "Cafe\u{301} menu"));
        assert!(same_intent_text(
            "\u{212b}ngstr\u{f6}m",
            "A\u{30a}ngstro\u{308}m"
        ));
    }

    #[test]
    fn unit_split_threshold_follows_number_is_integer() {
        assert_eq!(
            unit_split_threshold(&json!({"unitSplitThreshold": 1.0})),
            1.0
        );
        assert_eq!(unit_split_threshold(&json!({"unitSplitThreshold": 3})), 3.0);
        for invalid in [json!(1.5), json!(0), json!(-2), json!("2"), json!(null)] {
            assert_eq!(
                unit_split_threshold(&json!({ "unitSplitThreshold": invalid })),
                DEFAULT_UNIT_SPLIT_THRESHOLD as f64
            );
        }
        assert_eq!(
            unit_split_threshold(&json!({})),
            DEFAULT_UNIT_SPLIT_THRESHOLD as f64
        );
        assert_eq!(
            format!(
                "{}",
                unit_split_threshold(&json!({"unitSplitThreshold": 1.0}))
            ),
            "1"
        );
    }

    #[test]
    fn resolve_like_node_drops_trailing_separators_and_dot_segments() {
        let base = std::path::absolute(temp()).unwrap();
        let expected = base.clone();
        let mut trailing = base.clone().into_os_string();
        trailing.push(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(resolve_like_node(Path::new(&trailing)).unwrap(), expected);
        let dotted = base.join("sub").join("..").join(".");
        assert_eq!(resolve_like_node(&dotted).unwrap(), expected);
        let mut forward = base.join("x").into_os_string();
        forward.push("/../");
        assert_eq!(resolve_like_node(Path::new(&forward)).unwrap(), expected);
        let root = expected.ancestors().last().unwrap().to_path_buf();
        assert_eq!(resolve_like_node(&root.join("..")).unwrap(), root);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn broken_audit_log_fails_next_before_any_state_is_written() {
        let root = temp();
        write_config(&root, "{}");
        fs::create_dir_all(root.join(".aidlc/audit.jsonl")).unwrap();
        let options = NextOptions {
            intent: Some("audit preflight".into()),
            ..NextOptions::default()
        };
        assert_eq!(
            next(&root, &graph(), options).unwrap_err(),
            "Refusing unsafe .aidlc/audit.jsonl path"
        );
        assert!(!root.join(".aidlc/state.json").exists());
        assert!(!root.join(".aidlc/intents").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn broken_audit_log_fails_resume_and_report_without_mutation() {
        let root = temp();
        write_config(&root, "{}");
        let graph = graph();
        start(&root, &graph, "audit preflight");
        fs::remove_file(root.join(".aidlc/audit.jsonl")).unwrap();
        fs::create_dir_all(root.join(".aidlc/audit.jsonl")).unwrap();
        let before = snapshot(&root);
        let resume = NextOptions {
            intent: Some("audit preflight".into()),
            ..NextOptions::default()
        };
        assert_eq!(
            next(&root, &graph, resume).unwrap_err(),
            "Refusing unsafe .aidlc/audit.jsonl path"
        );
        let blocked = ReportOptions {
            detail: Some("waiting".into()),
            ..result("blocked")
        };
        assert_eq!(
            report(&root, &graph, blocked).unwrap_err(),
            "Refusing unsafe .aidlc/audit.jsonl path"
        );
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gate_open_items_match_node_rules() {
        let items = gate_open_items(
            "x\r\n### Open Items\r\n- [Follow-up] a\r\n1. [written-approval] b\r\n* c\r\n## Next\r\n- d\r\n",
        );
        assert!(items.present && !items.none);
        assert_eq!(
            (items.written_approval, items.follow_up, items.untagged),
            (1, 1, 1)
        );
        let none = gate_open_items("## Open items\n- None.\n");
        assert!(none.present && none.none);
        assert!(!gate_open_items("# Open items\n- None\n").present);
    }

    #[test]
    fn projection_notices_report_missing_current_and_modified_skills() {
        let root = temp();
        write_config(&root, r#"{"harness":"claude"}"#);
        assert_eq!(
            projection_notices(&root, "0.7.2"),
            ["The projected /dtech-aidlc skill for claude is missing relative to engine 0.7.2. Ask the user to run: dtech-aidlc harness refresh"]
        );
        for (source, target) in [
            ("skills/aidlc/SKILL.md", "SKILL.md"),
            ("onboarding.md", "onboarding.md"),
        ] {
            let bytes = crate::scope::embedded_asset(&format!("harness/claude/{source}")).unwrap();
            let rel = format!(".claude/skills/dtech-aidlc/{target}");
            write_artifact(&root, &rel, &String::from_utf8_lossy(bytes));
        }
        assert!(projection_notices(&root, "0.7.2").is_empty());
        write_artifact(&root, ".claude/skills/dtech-aidlc/SKILL.md", "edited");
        assert!(projection_notices(&root, "0.7.2")[0].contains("is modified relative to engine"));
        fs::remove_dir_all(root).unwrap();
    }
}
