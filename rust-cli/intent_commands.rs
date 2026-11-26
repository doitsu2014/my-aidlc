//! Native `intent` lifecycle commands, ported from the `cmd === 'intent'` branch of
//! Node `core/tools/dtech-aidlc.mjs` and the `core/tools/workflow.mjs` functions it
//! calls (`changeIntentDepth`, `requestDesignRework`, `assignIntent`, pause, resume,
//! abandon).
//!
//! Entry point: [`run`]. `key` is the command key from `crate::args::command_key`
//! (`intent depth`, `intent rework`, `intent assign`, `intent pause`,
//! `intent resume`, `intent abandon`; `intent list` delegates to
//! [`crate::status::list_intents`]). `args` are the positional arguments after the
//! command words. Every success returns a JSON object for the router to print with
//! exit code 0; no case prints a plain string. Errors carry Node's exact text.
//!
//! State layout: `.aidlc/state.json` is the v2 pointer
//! `{version: 2, activeIntentId, updatedAt}`; each intent lives in
//! `.aidlc/intents/<id>/state.json`. A legacy v1 `.aidlc/state.json` is migrated
//! through [`crate::workflow::load_workflow`] (and so `crate::state_migration`)
//! wherever Node migrates.
//!
//! Safety differences from Node, all of which leave project state unchanged on
//! failure where Node could leave a partial write:
//! - `intent depth` validates `--approved-by` before persisting (Node persists the new
//!   depth, then rejects an invalid `--approved-by`). The error text is identical.
//! - Before any write, every target path is checked for symlink components and the
//!   files read at load time are re-read; if they changed in between, the command
//!   fails with "Workflow state changed while this command was running; ..." and
//!   writes nothing.
//! - Every command that appends an audit row checks first, before its first write,
//!   that `.aidlc/audit.jsonl` can be appended to ([`crate::audit::preflight_audit`]);
//!   Node writes the state and then fails on the audit append.
//! - An intent id that is not a safe identifier is rejected as an unknown intent
//!   without reading a path outside `.aidlc/intents` (Node reaches the same error
//!   after reading it).

use crate::args::FlagValue;
use crate::workflow::{
    depth_of, derive_status, intent_state_path, load_intent, load_workflow, ordered_stages,
    read_pointer, save_intent, save_pointer, state_path, valid_intent_id, validate_intent_state,
    LoadedWorkflow, Pointer,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const USE_MESSAGE: &str =
    "Use intent list, intent pause, intent resume, intent abandon, intent depth, or intent assign";
const CHANGED: &str =
    "Workflow state changed while this command was running; no changes were made. Run the command again.";

/// Run one `intent` command. Returns the JSON to print and the exit code (always 0
/// on success; Node uses no other exit code for these commands).
pub fn run(
    key: &str,
    project: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    // Node: `resolve(flags.project || process.cwd())`, then `assertSafeStateDir(root)`,
    // then `readGraph()` before dispatching on the action.
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let graph = load_graph()?;
    let arg = args
        .first()
        .map(String::as_str)
        .filter(|value| !value.is_empty());
    let detail = string_flag(flags, "detail");
    let output = match key {
        "intent list" => crate::status::list_intents(&root, &graph)?,
        "intent depth" => {
            let Some(name) = arg else {
                return Err("Usage: intent depth NAME --approved-by NAME --detail TEXT".into());
            };
            change_intent_depth(
                &root,
                &graph,
                name,
                string_flag(flags, "approved-by"),
                detail,
            )?
        }
        "intent rework" => {
            let Some(detail) = detail else {
                return Err("Usage: intent rework --detail \"<reason>\"".into());
            };
            let graph = graph.value();
            let loaded = load_workflow(&root, graph, true)?;
            let Some(state) = loaded.state.clone() else {
                return Err("No active workflow state".into());
            };
            request_design_rework(&root, graph, &loaded, state, detail)?
        }
        "intent assign" => {
            let Some(name) = arg else {
                return Err("Usage: intent assign NAME".into());
            };
            assign_intent(&root, graph.value(), name)?
        }
        "intent pause" => pause(&root, graph.value(), detail)?,
        "intent resume" => {
            let Some(id) = arg else {
                return Err("Usage: intent resume <id>".into());
            };
            resume(&root, graph.value(), id)?
        }
        "intent abandon" => {
            let Some(detail) = detail else {
                return Err("intent abandon requires --detail \"<reason>\"".into());
            };
            abandon(&root, graph.value(), arg, detail)?
        }
        _ => return Err(USE_MESSAGE.into()),
    };
    Ok((output, 0))
}

fn string_flag<'a>(flags: &'a BTreeMap<String, FlagValue>, name: &str) -> Option<&'a str> {
    match flags.get(name) {
        Some(FlagValue::String(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Node `readGraph()`: the methodology graph from `DTECH_AIDLC_HOME` when it names an
/// engine, otherwise the graph embedded in this executable. Node's `readGraph` never
/// reads the project configuration, so the loader is pointed at a project path that
/// does not exist: the graph is validated, the project's `config.json` is not.
fn load_graph() -> Result<crate::scope::Graph, String> {
    let sentinel = configless_root();
    let loaded = match external_engine() {
        Some(engine) => crate::scope::load_graph(engine, &sentinel),
        None => crate::scope::load_embedded_graph(&sentinel),
    };
    loaded.map(|(graph, _)| graph)
}

fn configless_root() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    loop {
        let candidate = std::env::temp_dir().join(format!(
            "dtech-aidlc-no-project-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
}

/// The installed engine named by `DTECH_AIDLC_HOME` (Node `engineRoot()`), if any;
/// the same resolution as the router uses for other native commands.
fn external_engine() -> Option<PathBuf> {
    std::env::var_os("DTECH_AIDLC_HOME")
        .map(PathBuf::from)
        .and_then(|home| {
            [home.join("core"), home].into_iter().find(|candidate| {
                candidate.join("data/phase-graph.json").exists()
                    || candidate.join("methodology").exists()
            })
        })
}

// ---------------------------------------------------------------------------
// Loading and persistence
// ---------------------------------------------------------------------------

/// Node `loadWorkflow(root, graph, { migrate, missingActive: 'null' })`. Returns the
/// loaded workflow and whether the pointer names an intent whose state file is missing.
fn load_workflow_lenient(
    root: &Path,
    graph: &Value,
    migrate: bool,
) -> Result<(LoadedWorkflow, bool), String> {
    let (pointer, pointer_bytes) = read_pointer(root);
    let Pointer::Active(Some(id)) = pointer else {
        return load_workflow(root, graph, migrate).map(|loaded| (loaded, false));
    };
    match load_intent(root, graph, &id) {
        Ok((state, bytes)) => {
            if derive_status(&state, graph) == "abandoned" {
                return Ok((
                    LoadedWorkflow {
                        pointer_bytes,
                        ..LoadedWorkflow::default()
                    },
                    false,
                ));
            }
            Ok((
                LoadedWorkflow {
                    active_intent_id: Some(id),
                    state: Some(state),
                    pointer_bytes,
                    state_bytes: Some(bytes),
                },
                false,
            ))
        }
        Err(error) if error.contains("has no state file") => Ok((
            LoadedWorkflow {
                active_intent_id: Some(id),
                pointer_bytes,
                ..LoadedWorkflow::default()
            },
            true,
        )),
        Err(error) => Err(error),
    }
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Refuse to write when the pointer, or the given intent's state file, no longer has
/// the bytes that were read when the command loaded it.
fn ensure_unchanged(
    root: &Path,
    pointer_bytes: &Option<Vec<u8>>,
    intent: Option<(&str, &Option<Vec<u8>>)>,
) -> Result<(), String> {
    if &read_bytes(&state_path(root))? != pointer_bytes {
        return Err(CHANGED.into());
    }
    if let Some((id, bytes)) = intent {
        if &read_bytes(&intent_state_path(root, id))? != bytes {
            return Err(CHANGED.into());
        }
    }
    Ok(())
}

fn check_writable_intent(root: &Path, graph: &Value, state: &Value) -> Result<String, String> {
    let id = state
        .get("intentId")
        .and_then(Value::as_str)
        .filter(|id| valid_intent_id(id))
        .ok_or("intentId is not a safe identifier")?
        .to_owned();
    if let Some(problem) = validate_intent_state(state, graph, Some(&id), true).first() {
        return Err(format!(
            ".aidlc/intents/{id}/state.json would be invalid: {problem}. No changes were made."
        ));
    }
    crate::workflow::assert_no_symlink_components(
        root,
        &format!(".aidlc/intents/{id}/state.json"),
    )?;
    Ok(id)
}

/// Node `persistWorkflow(root, state, graph)`: write the active intent's state, then
/// re-point `.aidlc/state.json` at it. Everything is validated before either write.
fn persist_workflow(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    state: &mut Value,
) -> Result<(), String> {
    let id = check_writable_intent(root, graph, state)?;
    if loaded.active_intent_id.as_deref() != Some(id.as_str()) {
        return Err(format!(
            "Intent '{id}' is not the active intent; no changes were made"
        ));
    }
    crate::workflow::assert_no_symlink_components(root, ".aidlc/state.json")?;
    ensure_unchanged(
        root,
        &loaded.pointer_bytes,
        Some((&id, &loaded.state_bytes)),
    )?;
    save_intent(root, state, Some(graph))?;
    save_pointer(root, Some(&id))
}

fn unknown_intent(id: &str) -> String {
    format!("Unknown intent '{id}'. Run 'dtech-aidlc intent list'.")
}

/// Node `loadIntent` for a user-named intent; every failure is "Unknown intent".
fn load_named_intent(root: &Path, graph: &Value, id: &str) -> Result<(Value, Vec<u8>), String> {
    if !valid_intent_id(id) {
        return Err(unknown_intent(id));
    }
    load_intent(root, graph, id).map_err(|_| unknown_intent(id))
}

/// Node `quarantineCorruptPointer`: rename `.aidlc/state.json` to
/// `.aidlc/state.json.corrupt-<YYYYMMDDTHHMMSSZ>`.
fn quarantine_corrupt_pointer(root: &Path) -> Result<PathBuf, String> {
    let from = state_path(root);
    let iso = crate::audit::now_iso();
    let stamp = format!("{}Z", &iso[..19]).replace(['-', ':'], "");
    let to = root
        .join(".aidlc")
        .join(format!("state.json.corrupt-{stamp}"));
    fs::rename(&from, &to).map_err(|error| format!("{}: {error}", from.display()))?;
    Ok(to)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Node `changeIntentDepth(root, name, { approvedBy, detail })`.
fn change_intent_depth(
    root: &Path,
    graph: &crate::scope::Graph,
    name: &str,
    approved_by: Option<&str>,
    detail: Option<&str>,
) -> Result<Value, String> {
    let depth = graph.resolve_depth(Some(name))?.to_owned();
    let graph = graph.value();
    let loaded = load_workflow(root, graph, true)?;
    let Some(mut state) = loaded.state.clone() else {
        return Err("No active intent".into());
    };
    let Some(approved_by) = approved_by else {
        return Err("intent depth requires --approved-by NAME".into());
    };
    let Some(detail) = detail else {
        return Err("intent depth requires --detail TEXT".into());
    };
    let who = crate::identity::resolve_approver(root, Some(approved_by))?;
    let from = depth_of(&state).to_owned();
    state["depth"] = json!(depth);
    crate::audit::preflight_audit(root)?;
    persist_workflow(root, graph, &loaded, &mut state)?;
    let intent_id = state["intentId"].clone();
    crate::audit::audit(
        root,
        "depth.changed",
        json!({"intentId": intent_id, "from": from, "to": depth, "by": who.by, "bySource": who.by_source, "detail": detail}),
    )?;
    Ok(json!({"intentId": intent_id, "from": from, "to": depth}))
}

/// JavaScript object spread `{ ...value }` for the JSON values a state file can hold.
fn spread(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (index.to_string(), item.clone()))
            .collect(),
        Some(Value::String(text)) => text
            .encode_utf16()
            .enumerate()
            .map(|(index, unit)| (index.to_string(), json!(String::from_utf16_lossy(&[unit]))))
            .collect(),
        _ => Map::new(),
    }
}

/// `row.reports?.at(-1)` for an array of reports.
fn last_report(row: Option<&Value>) -> Option<&Value> {
    row?.get("reports")?.as_array()?.last()
}

fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// Node `requestDesignRework(root, state, graph, detail)`.
fn request_design_rework(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    mut state: Value,
    detail: &str,
) -> Result<Value, String> {
    let detail = crate::identity::js_trim(detail);
    if detail.is_empty() {
        return Err("intent rework requires --detail \"<reason>\"".into());
    }
    if !js_truthy(state.get("intentId")) || derive_status(&state, graph) != "active" {
        return Err("Design rework requires an active intent".into());
    }
    let scope = state.get("scope").and_then(Value::as_str).unwrap_or("");
    let develop: Vec<String> = ordered_stages(graph, scope, depth_of(&state))
        .into_iter()
        .filter(|stage| stage.get("phase").and_then(Value::as_str) == Some("develop"))
        .filter_map(|stage| stage.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    if !develop.iter().any(|id| id == "implementation-design") {
        return Err("The active intent has no implementation-design stage".into());
    }
    let blocked = develop
        .iter()
        .find(|id| {
            let row = state.get("stages").and_then(|rows| rows.get(id.as_str()));
            let last = last_report(row);
            let reset_at = row.and_then(|row| row.get("reworkResetAt"));
            let reported_after_reset = !js_truthy(reset_at)
                || (js_truthy(last.and_then(|report| report.get("at"))) && {
                    let at = last
                        .and_then(|report| report.get("at"))
                        .and_then(js_date_parse);
                    let reset = reset_at.and_then(js_date_parse);
                    matches!((at, reset), (Some(at), Some(reset)) if at > reset)
                });
            row.and_then(|row| row.get("status"))
                .and_then(Value::as_str)
                == Some("blocked")
                || (reported_after_reset
                    && last
                        .and_then(|report| report.get("result"))
                        .and_then(Value::as_str)
                        == Some("blocked"))
        })
        .cloned();
    let Some(from_stage) = blocked else {
        return Err("Design rework is only available after a blocked Develop stage".into());
    };

    let now = crate::audit::now_iso();
    let stages = state
        .get_mut("stages")
        .and_then(Value::as_object_mut)
        .ok_or("stages is not an object")?;
    let mut design = spread(stages.get("implementation-design"));
    design.insert("status".into(), json!("in_progress"));
    design.insert("reworkRequestedAt".into(), json!(now));
    stages.insert("implementation-design".into(), Value::Object(design));
    for id in develop.iter().filter(|id| *id != "implementation-design") {
        let Some(row) = stages.get(id.as_str()) else {
            continue;
        };
        if row.is_null() {
            continue;
        }
        let units: Map<String, Value> = spread(row.get("units"))
            .into_iter()
            .map(|(unit_id, unit)| {
                let mut unit = spread(Some(&unit));
                unit.insert("status".into(), json!("pending"));
                (unit_id, Value::Object(unit))
            })
            .collect();
        let mut next = spread(Some(row));
        next.insert("status".into(), json!("in_progress"));
        next.insert("reworkResetAt".into(), json!(now));
        next.insert("units".into(), Value::Object(units));
        stages.insert(id.clone(), Value::Object(next));
    }
    let map = state.as_object_mut().ok_or("state is not an object")?;
    // `state.units = undefined` / `state.currentUnit = undefined`: JSON drops the keys.
    map.shift_remove("units");
    map.insert("designReworkPending".into(), json!(true));
    map.shift_remove("currentUnit");
    map.insert("currentStage".into(), Value::Null);
    map.insert(
        "designRework".into(),
        json!({"requestedAt": now, "fromStage": from_stage, "detail": detail}),
    );
    crate::audit::preflight_audit(root)?;
    persist_workflow(root, graph, loaded, &mut state)?;
    let intent_id = state["intentId"].clone();
    crate::audit::audit(
        root,
        "design.rework.requested",
        json!({"intentId": intent_id, "fromStage": from_stage, "detail": detail}),
    )?;
    Ok(json!({
        "reworkRequested": true,
        "intentId": intent_id,
        "fromStage": from_stage,
        "nextStage": "implementation-design",
    }))
}

/// Node `assignIntent(root, name)`. Node records no audit row for an assignment.
fn assign_intent(root: &Path, graph: &Value, name: &str) -> Result<Value, String> {
    let loaded = load_workflow(root, graph, true)?;
    let Some(mut state) = loaded.state.clone() else {
        return Err("No active intent".into());
    };
    let assignee = crate::identity::js_trim(name);
    if assignee.is_empty() {
        return Err("intent assign requires a name".into());
    }
    state["assignee"] = json!(assignee);
    persist_workflow(root, graph, &loaded, &mut state)?;
    Ok(json!({"intentId": state["intentId"], "assignee": assignee}))
}

/// Node `intent pause`: clear the pointer (the intent itself is untouched).
fn pause(root: &Path, graph: &Value, detail: Option<&str>) -> Result<Value, String> {
    let (loaded, missing) = load_workflow_lenient(root, graph, true)?;
    if loaded.state.is_none() && !missing {
        return Err("No unfinished active intent to pause".into());
    }
    let id = loaded
        .state
        .as_ref()
        .and_then(|state| state.get("intentId"))
        .filter(|id| !id.is_null())
        .cloned()
        .or_else(|| loaded.active_intent_id.clone().map(Value::String))
        .unwrap_or(Value::Null);
    crate::workflow::assert_no_symlink_components(root, ".aidlc/state.json")?;
    ensure_unchanged(root, &loaded.pointer_bytes, None)?;
    crate::audit::preflight_audit(root)?;
    save_pointer(root, None)?;
    crate::audit::audit(
        root,
        "intent.paused",
        json!({"intentId": id, "detail": detail}),
    )?;
    let shown = match &id {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    Ok(json!({
        "paused": true,
        "intentId": id,
        "resumeWith": format!("dtech-aidlc intent resume {shown}"),
    }))
}

/// Node `intent resume <id>`.
fn resume(root: &Path, graph: &Value, id: &str) -> Result<Value, String> {
    let (pointer, _) = read_pointer(root);
    if matches!(pointer, Pointer::Corrupt(_)) {
        // The quarantine rename is a write; refuse a broken audit log before it.
        crate::audit::preflight_audit(root)?;
        quarantine_corrupt_pointer(root)?;
    } else {
        load_workflow_lenient(root, graph, true)?;
    }
    let (target, _) = load_named_intent(root, graph, id)?;
    match derive_status(&target, graph) {
        "complete" => return Err(format!("Intent '{id}' is complete and cannot be resumed")),
        "abandoned" => return Err(format!("Intent '{id}' was abandoned and cannot be resumed")),
        _ => {}
    }
    let (loaded, _) = load_workflow_lenient(root, graph, false)?;
    if let Some(state) = &loaded.state {
        let other = state.get("intentId").and_then(Value::as_str);
        if other != Some(id) && derive_status(state, graph) == "active" {
            return Err(format!(
                "Intent '{}' is active; pause or finish it first",
                other.unwrap_or("undefined")
            ));
        }
    }
    crate::workflow::assert_no_symlink_components(root, ".aidlc/state.json")?;
    ensure_unchanged(root, &loaded.pointer_bytes, None)?;
    crate::audit::preflight_audit(root)?;
    save_pointer(root, Some(id))?;
    crate::audit::audit(
        root,
        "intent.resumed",
        json!({"intentId": id, "detail": Value::Null}),
    )?;
    let mut output = json!({"resumed": true, "intentId": id});
    // `currentStage: target.currentStage` is dropped by JSON.stringify when undefined.
    if let Some(stage) = target.get("currentStage") {
        output["currentStage"] = stage.clone();
    }
    Ok(output)
}

/// Node `intent abandon [id] --detail`.
fn abandon(root: &Path, graph: &Value, id: Option<&str>, detail: &str) -> Result<Value, String> {
    load_workflow_lenient(root, graph, true)?;
    let (loaded, _) = load_workflow_lenient(root, graph, false)?;
    let target_id = match id {
        Some(id) => id.to_owned(),
        None => match loaded
            .state
            .as_ref()
            .and_then(|state| state.get("intentId"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            Some(id) => id.to_owned(),
            None => return Err("No active intent; name the intent to abandon".into()),
        },
    };
    let (mut target, target_bytes) = load_named_intent(root, graph, &target_id)?;
    match derive_status(&target, graph) {
        "complete" => return Err(format!("Intent '{target_id}' is already complete")),
        "abandoned" => return Err(format!("Intent '{target_id}' is already abandoned")),
        _ => {}
    }
    target["closed"] =
        json!({"kind": "abandoned", "at": crate::audit::now_iso(), "detail": detail});
    target["currentStage"] = Value::Null;
    check_writable_intent(root, graph, &target)?;
    let clear_pointer = loaded.active_intent_id.as_deref() == Some(target_id.as_str());
    if clear_pointer {
        crate::workflow::assert_no_symlink_components(root, ".aidlc/state.json")?;
    }
    ensure_unchanged(
        root,
        &loaded.pointer_bytes,
        Some((&target_id, &Some(target_bytes))),
    )?;
    crate::audit::preflight_audit(root)?;
    save_intent(root, &mut target, Some(graph))?;
    if clear_pointer {
        save_pointer(root, None)?;
    }
    crate::audit::audit(
        root,
        "intent.abandoned",
        json!({"intentId": target_id, "detail": detail}),
    )?;
    Ok(json!({"abandoned": true, "intentId": target_id}))
}

// ---------------------------------------------------------------------------
// Date.parse
// ---------------------------------------------------------------------------

/// JavaScript `Date.parse` for the ECMAScript date-time string format
/// (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`, optionally followed by `THH:mm`, `:ss`, `.sss`
/// and `Z` or `+HH:mm`; also `+YYYYYY`/`-YYYYYY` years). Returns milliseconds since
/// the epoch, or `None` where JavaScript returns `NaN`. A date-time without an offset
/// is local time in JavaScript; it is read as UTC here (the CLI always writes `Z`).
fn js_date_parse(value: &Value) -> Option<f64> {
    let text = value.as_str()?;
    let bytes = text.as_bytes();
    let mut pos = 0;
    let digits = |pos: &mut usize, count: usize| -> Option<i64> {
        let slice = bytes.get(*pos..*pos + count)?;
        if !slice.iter().all(u8::is_ascii_digit) {
            return None;
        }
        *pos += count;
        std::str::from_utf8(slice).ok()?.parse().ok()
    };
    let year = match bytes.first()? {
        b'+' | b'-' => {
            let negative = bytes[0] == b'-';
            pos = 1;
            let year = digits(&mut pos, 6)?;
            if negative && year == 0 {
                return None;
            }
            if negative {
                -year
            } else {
                year
            }
        }
        _ => digits(&mut pos, 4)?,
    };
    let (mut month, mut day) = (1, 1);
    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
        month = digits(&mut pos, 2)?;
        if bytes.get(pos) == Some(&b'-') {
            pos += 1;
            day = digits(&mut pos, 2)?;
        }
    }
    // V8 accepts any day 1-31 and rolls it over (2026-02-31 is 2026-03-03).
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (mut hour, mut minute, mut second, mut millis, mut offset) = (0, 0, 0, 0.0, 0);
    if bytes.get(pos) == Some(&b'T') {
        pos += 1;
        hour = digits(&mut pos, 2)?;
        if bytes.get(pos) != Some(&b':') {
            return None;
        }
        pos += 1;
        minute = digits(&mut pos, 2)?;
        if bytes.get(pos) == Some(&b':') {
            pos += 1;
            second = digits(&mut pos, 2)?;
            if bytes.get(pos) == Some(&b'.') {
                pos += 1;
                let start = pos;
                while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
                    pos += 1;
                }
                if pos == start {
                    return None;
                }
                let fraction = &text[start..pos.min(start + 3)];
                millis = format!("{fraction:0<3}").parse::<f64>().ok()?;
            }
        }
        match bytes.get(pos) {
            Some(b'Z') => pos += 1,
            Some(sign @ (b'+' | b'-')) => {
                let sign = if *sign == b'-' { -1 } else { 1 };
                pos += 1;
                let hours = digits(&mut pos, 2)?;
                if bytes.get(pos) != Some(&b':') {
                    return None;
                }
                pos += 1;
                let minutes = digits(&mut pos, 2)?;
                if hours > 23 || minutes > 59 {
                    return None;
                }
                offset = sign * (hours * 60 + minutes);
            }
            _ => {}
        }
        let end_of_day = hour == 24 && minute == 0 && second == 0 && millis == 0.0;
        if !(hour < 24 || end_of_day) || minute > 59 || second > 59 {
            return None;
        }
    }
    if pos != bytes.len() {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let millis_total =
        (((days * 24 + hour) * 60 + minute - offset) * 60 + second) as f64 * 1000.0 + millis;
    (millis_total.abs() <= 8.64e15).then_some(millis_total)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_parse_matches_javascript_for_iso_strings() {
        let parse = |text: &str| js_date_parse(&json!(text));
        assert_eq!(parse("1970-01-01T00:00:00.000Z"), Some(0.0));
        // Reference values from Node's Date.parse.
        assert_eq!(parse("2026-11-26T10:20:30.456Z"), Some(1_795_688_430_456.0));
        assert_eq!(parse("2026-11-26"), Some(1_795_651_200_000.0));
        assert_eq!(
            parse("2026-11-26T11:20:30.456+01:00"),
            Some(1_795_688_430_456.0)
        );
        assert_eq!(parse("2026-11-26T10:20Z"), Some(1_795_688_400_000.0));
        assert_eq!(parse("2026-02-31"), Some(1_772_496_000_000.0));
        assert_eq!(parse("2026-11-26T24:00:00Z"), Some(1_795_737_600_000.0));
        assert_eq!(
            parse("2026-11-26T10:20:30.45678Z"),
            Some(1_795_688_430_456.0)
        );
        assert_eq!(parse("+002026-11-26T10:20:30Z"), Some(1_795_688_430_000.0));
        assert_eq!(parse("2026-02-32"), None);
        assert_eq!(parse("2026-11-26T24:00:01Z"), None);
        assert_eq!(parse("2026-11-26T10:20:60Z"), None);
        assert_eq!(parse("not a date"), None);
        assert_eq!(parse(""), None);
        assert_eq!(js_date_parse(&json!(5)), None);
    }

    #[test]
    fn ensure_unchanged_refuses_files_changed_after_load() {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "dtech-aidlc-intent-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        fs::create_dir_all(root.join(".aidlc/intents/alpha")).unwrap();
        fs::write(root.join(".aidlc/state.json"), "a").unwrap();
        fs::write(root.join(".aidlc/intents/alpha/state.json"), "b").unwrap();
        let (pointer, state) = (Some(b"a".to_vec()), Some(b"b".to_vec()));
        assert!(ensure_unchanged(&root, &pointer, Some(("alpha", &state))).is_ok());
        fs::write(root.join(".aidlc/intents/alpha/state.json"), "c").unwrap();
        assert_eq!(
            ensure_unchanged(&root, &pointer, Some(("alpha", &state))),
            Err(CHANGED.to_owned())
        );
        fs::remove_file(root.join(".aidlc/state.json")).unwrap();
        assert_eq!(
            ensure_unchanged(&root, &pointer, None),
            Err(CHANGED.to_owned())
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn spread_follows_object_spread_rules() {
        assert_eq!(
            spread(Some(&json!({"a": 1}))),
            json!({"a": 1}).as_object().unwrap().clone()
        );
        assert_eq!(
            Value::Object(spread(Some(&json!(["x"])))),
            json!({"0": "x"})
        );
        assert!(spread(Some(&json!(3))).is_empty());
        assert!(spread(None).is_empty());
    }
}
