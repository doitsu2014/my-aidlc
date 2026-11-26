//! Unit-level scheduling of Develop's repeating stages (`red-tests`, `build-and-verify`).
//!
//! The intent `state.json` keeps `"version": 2` and gains one optional key,
//! `workUnits` (the "ledger"), with its own `schemaVersion: 1`. The ledger exists only
//! when the intent has real work units (a `work-units.json` listing units at a depth
//! that produces it). Fast depth and a `{"status": "not-applicable"}` file keep the
//! 1.0.0 implicit single-unit path, which creates no ledger.
//!
//! Between ledger creation and the next reconcile the ledger is the only source for
//! the unit order, `dependsOn` and test ownership; `work-units.json` and `ids.json` are
//! not re-read, and a change of their bytes is reported as drift.
//!
//! The 1.0.0 keys (`stages[s].units`, `stages[s].status` of repeating rows and
//! `state.units`) are kept as mirrors by [`sync_mirrors`], so a 1.0.0 binary can still
//! drive a migrated state, and removing `workUnits` is a real rollback.
//!
//! Everything here is a pure function over `serde_json::Value` plus file reads; no
//! function in this module writes project state.

use crate::artifacts::expected_artifacts;
use crate::ids::normalize_id;
use crate::workflow::{depth_of, stage_finished};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const LEDGER_KEY: &str = "workUnits";
pub const LEDGER_SCHEMA_VERSION: u64 = 1;
pub const DEFAULT_AUTO_PARK_THRESHOLD: u64 = 3;
/// How many work units may be in flight at once (`unitWorkers`), the current unit
/// included. `orchestrate next --claim` hands out units up to this number.
pub const DEFAULT_UNIT_WORKERS: u64 = 3;
pub const REPEAT_WORK_UNIT: &str = "work-unit";
const LOAD_ERROR: &str = "Develop cannot select a work unit: ";
const FINGERPRINT_TAG: &str = "aidlc-unit-fp/1\n";

/// Unit statuses that satisfy a `dependsOn` edge.
pub fn satisfied(status: Option<&str>) -> bool {
    matches!(status, Some("green" | "verified" | "done"))
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn remove_key(value: &mut Value, key: &str) {
    if let Some(map) = value.as_object_mut() {
        map.shift_remove(key);
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// JSON with object keys sorted at every level, without whitespace.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body = keys
                .iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical_json(&map[key.as_str()])
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Sort identifiers like `TS-2` before `TS-10`: by prefix, then by number.
fn id_order(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |id: &str| {
        let (prefix, number) = id.split_once('-').unwrap_or((id, ""));
        (
            prefix.to_owned(),
            number.parse::<u64>().unwrap_or(u64::MAX),
            id.to_owned(),
        )
    };
    key(a).cmp(&key(b))
}

// ---------------------------------------------------------------------------
// Graph helpers
// ---------------------------------------------------------------------------

/// The stages of an intent split around the repeat span.
pub struct Span<'a> {
    pub before: Vec<&'a Value>,
    pub repeating: Vec<&'a Value>,
    pub after: Vec<&'a Value>,
}

pub fn is_repeating(stage: &Value) -> bool {
    str_of(stage, "repeat") == Some(REPEAT_WORK_UNIT)
}

pub fn span(stages: &[Value]) -> Span<'_> {
    let first = stages.iter().position(is_repeating);
    let last = stages.iter().rposition(is_repeating);
    match (first, last) {
        (Some(first), Some(last)) => Span {
            before: stages[..first].iter().collect(),
            repeating: stages[first..=last]
                .iter()
                .filter(|s| is_repeating(s))
                .collect(),
            after: stages[last + 1..].iter().collect(),
        },
        _ => Span {
            before: stages.iter().collect(),
            repeating: Vec::new(),
            after: Vec::new(),
        },
    }
}

fn stage_id(stage: &Value) -> &str {
    str_of(stage, "id").unwrap_or("")
}

fn row_status<'a>(state: &'a Value, id: &str) -> Option<&'a str> {
    state
        .get("stages")
        .and_then(|rows| rows.get(id))
        .and_then(|row| row.get("status"))
        .and_then(Value::as_str)
}

fn row_exists(state: &Value, id: &str) -> bool {
    state
        .get("stages")
        .and_then(|rows| rows.get(id))
        .is_some_and(Value::is_object)
}

fn stage_by_id<'a>(graph: &'a Value, id: &str) -> Option<&'a Value> {
    graph
        .get("stages")
        .and_then(Value::as_array)?
        .iter()
        .find(|stage| stage_id(stage) == id)
}

// ---------------------------------------------------------------------------
// Ledger access
// ---------------------------------------------------------------------------

pub fn ledger(state: &Value) -> Option<&Value> {
    state.get(LEDGER_KEY).filter(|value| value.is_object())
}

pub fn has_ledger(state: &Value) -> bool {
    ledger(state).is_some()
}

/// The unit keys in scheduling order (as written in `work-units.json`).
pub fn order(state: &Value) -> Vec<String> {
    ledger(state)
        .and_then(|ledger| ledger.get("order"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

pub fn unit<'a>(state: &'a Value, key: &str) -> Option<&'a Value> {
    ledger(state)?.get("units")?.get(key)
}

fn unit_mut<'a>(state: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    state.get_mut(LEDGER_KEY)?.get_mut("units")?.get_mut(key)
}

fn removed(unit: &Value) -> bool {
    unit.get("removed") == Some(&Value::Bool(true))
}

/// The ledger key naming `id` (either spelling, `U-01` or `WU-01`).
pub fn resolve_unit(state: &Value, id: &str) -> Option<String> {
    let wanted = normalize_id(id.trim());
    order(state)
        .into_iter()
        .find(|key| normalize_id(key) == wanted)
}

pub fn active_units(state: &Value) -> Vec<(String, Value)> {
    order(state)
        .into_iter()
        .filter_map(|key| {
            let unit = unit(state, &key)?.clone();
            (!removed(&unit)).then_some((key, unit))
        })
        .collect()
}

pub fn status_of(unit: &Value) -> Option<&str> {
    str_of(unit, "status")
}

pub fn step_of<'a>(unit: &'a Value, stage: &str) -> Option<&'a str> {
    unit.get("steps")
        .and_then(|steps| steps.get(stage))
        .and_then(Value::as_str)
}

/// The first repeating stage this unit has not finished, in graph order.
pub fn next_step<'a>(unit: &Value, repeating: &[&'a Value]) -> Option<&'a Value> {
    repeating
        .iter()
        .find(|stage| !stage_finished(step_of(unit, stage_id(stage))))
        .copied()
}

fn deps_satisfied(state: &Value, unit: &Value) -> bool {
    waiting_on(state, unit).is_empty()
}

/// The unsatisfied dependencies of `unit` (ledger keys).
fn waiting_on(state: &Value, unit: &Value) -> Vec<String> {
    unit.get("dependsOn")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|dep| {
            let key = resolve_unit(state, dep)?;
            let target = self::unit(state, &key)?;
            (!removed(target) && !satisfied(status_of(target))).then_some(key)
        })
        .collect()
}

fn ready(state: &Value, unit: &Value) -> bool {
    !removed(unit)
        && !is_parked(unit)
        && matches!(status_of(unit), Some("pending" | "red" | "blocked"))
        && deps_satisfied(state, unit)
}

/// Whether the unit is parked: its status says so, or it still holds an open park
/// record (which only an inconsistent write can leave behind a non-parked status).
pub fn is_parked(unit: &Value) -> bool {
    status_of(unit) == Some("parked") || open_park(unit).is_some()
}

/// Why unit `key` cannot run step `stage` now, or `None` when the selector may pick
/// that step: no design rework is pending, the unit exists and is not removed, parked
/// or satisfied, its dependencies are satisfied and `stage` is its next step.
pub fn unit_step_problem(
    state: &Value,
    stages: &[Value],
    key: &str,
    stage: &str,
) -> Option<String> {
    if rework_pending(state) {
        return Some("a design rework is pending".into());
    }
    let Some(entry) = unit(state, key).filter(|entry| !removed(entry)) else {
        return Some(format!("{key} is not an active work unit"));
    };
    if is_parked(entry) {
        return Some(
            match open_park(entry).and_then(|park| str_of(park, "kind")) {
                Some(kind) => format!("{key} is parked ({kind})"),
                None => format!("{key} is parked"),
            },
        );
    }
    if satisfied(status_of(entry)) {
        return Some(format!("{key} is already done"));
    }
    let waiting = waiting_on(state, entry);
    if !waiting.is_empty() {
        return Some(format!("{key} waits on {}", waiting.join(", ")));
    }
    let span = span(stages);
    match next_step(entry, &span.repeating).map(stage_id) {
        Some(next) if next == stage => None,
        Some(next) => Some(format!("the next step of {key} is {next}, not {stage}")),
        None => Some(format!("{key} has no step left")),
    }
}

/// Clear `currentStage`/`currentUnit` when they name a work-unit step that cannot run
/// now (see [`unit_step_problem`]), for example a parked unit an older dtech-aidlc
/// selected by moving only the pointer.
fn clear_unrunnable_current(state: &mut Value, stages: &[Value]) {
    let Some(current) = str_of(state, "currentStage").map(str::to_owned) else {
        return;
    };
    if !span(stages)
        .repeating
        .iter()
        .any(|stage| stage_id(stage) == current)
    {
        return;
    }
    let Some(named) = str_of(state, "currentUnit")
        .filter(|unit| !unit.is_empty())
        .map(str::to_owned)
    else {
        return;
    };
    let key = resolve_unit(state, &named).unwrap_or(named);
    if unit_step_problem(state, stages, &key, &current).is_some() {
        state["currentStage"] = Value::Null;
        remove_key(state, "currentUnit");
    }
}

/// Units that are not satisfied (pending, red, blocked or parked), in order.
pub fn unsatisfied_units(state: &Value) -> Vec<String> {
    active_units(state)
        .into_iter()
        .filter(|(_, unit)| !satisfied(status_of(unit)))
        .map(|(key, _)| key)
        .collect()
}

pub fn sweep_status(state: &Value) -> &str {
    ledger(state)
        .and_then(|ledger| ledger.get("sweep"))
        .and_then(|sweep| str_of(sweep, "status"))
        .unwrap_or("pending")
}

pub fn rework_pending(state: &Value) -> bool {
    ledger(state)
        .and_then(|ledger| ledger.get("rework"))
        .is_some_and(|rework| !rework.is_null())
}

/// Whether `currentStage`/`currentUnit` describe the release sweep step: a ledger
/// exists, `currentStage` is the last repeating stage, `currentUnit` is absent and the
/// sweep was selected.
pub fn is_sweep_step(state: &Value, stages: &[Value]) -> bool {
    let Some(ledger) = ledger(state) else {
        return false;
    };
    let span = span(stages);
    let Some(last) = span.repeating.last() else {
        return false;
    };
    str_of(state, "currentStage") == Some(stage_id(last))
        && state
            .get("currentUnit")
            .is_none_or(|unit| unit.is_null() || unit == "")
        && ledger
            .pointer("/sweep/selectedAt")
            .is_some_and(|at| at.is_string())
}

/// `autoParkThreshold` from the configuration when it is a positive integer.
pub fn auto_park_threshold(config: &Value) -> u64 {
    config
        .get("autoParkThreshold")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && n.fract() == 0.0 && *n > 0.0)
        .map(|n| n as u64)
        .unwrap_or(DEFAULT_AUTO_PARK_THRESHOLD)
}

/// `unitWorkers` from the configuration when it is a positive integer.
pub fn unit_workers(config: &Value) -> u64 {
    config
        .get("unitWorkers")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && n.fract() == 0.0 && *n > 0.0)
        .map(|n| n as u64)
        .unwrap_or(DEFAULT_UNIT_WORKERS)
}

// ---------------------------------------------------------------------------
// Parallel work units: the claim pool
// ---------------------------------------------------------------------------
//
// The current unit (`currentStage`/`currentUnit`) stays the primary unit in flight,
// exactly as with one worker. `orchestrate next --claim` adds further ready units to
// `workUnits.claims` (`[{unit, stage, claimedAt}]`, claim order), up to `unitWorkers`
// units in flight in all. A claimed unit is reported, parked or printed again with
// `--unit`. A unit reported complete or skipped on a step that is not its last stays in
// flight on its next step; it leaves the pool when its last repeating step is
// reported, when it is reported blocked, when it is parked, or when it can no longer
// run. The key is removed when the pool is empty.
//
// `workUnits.parallelSince` marks a parallel run: it is set when a claim is first
// made and removed once nothing is in flight. While it is set, plain `next` resumes
// the current unit (or promotes the oldest claim) instead of re-selecting, a unit
// keeps its worker between its steps, and `lastSatisfiedAt` is stamped. An intent
// that never claims never gets either key, so its state keeps its exact shape.

fn claim_rows(state: &Value) -> Vec<Value> {
    ledger(state)
        .and_then(|ledger| ledger.get("claims"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn set_claim_rows(state: &mut Value, rows: Vec<Value>) {
    let Some(ledger) = state.get_mut(LEDGER_KEY).and_then(Value::as_object_mut) else {
        return;
    };
    if rows.is_empty() {
        ledger.remove("claims");
    } else {
        ledger.insert("claims".into(), Value::Array(rows));
    }
}

/// Whether a parallel run is in progress (`workUnits.parallelSince`).
pub fn parallel_run(state: &Value) -> bool {
    ledger(state)
        .and_then(|ledger| ledger.get("parallelSince"))
        .is_some_and(Value::is_string)
}

/// Start the parallel run marker when claims exist, and end it once nothing is in
/// flight. Called before every write of a command that can change the pool.
pub fn sync_parallel_run(state: &mut Value, stages: &[Value], now: &str) {
    if !has_ledger(state) {
        return;
    }
    let start = !claims(state).is_empty() && !parallel_run(state);
    let end = parallel_run(state) && in_flight(state, stages).is_empty();
    let Some(ledger) = state.get_mut(LEDGER_KEY).and_then(Value::as_object_mut) else {
        return;
    };
    if start {
        ledger.insert("parallelSince".into(), json!(now));
    } else if end {
        ledger.remove("parallelSince");
    }
}

/// Whether the selector's span is reached: the ledger exists, the repeating span is
/// not empty, no design rework is pending and every stage before the span finished.
pub fn span_reached(state: &Value, stages: &[Value]) -> bool {
    let span = span(stages);
    has_ledger(state)
        && !span.repeating.is_empty()
        && !rework_pending(state)
        && span
            .before
            .iter()
            .all(|stage| stage_finished(row_status(state, stage_id(stage))))
}

/// The automatic (blocker-done) revisits a claim makes before it looks for a unit, as
/// `select` does; returns the `unit.revisited` details.
pub fn claim_revisits(state: &mut Value, stages: &[Value], now: &str) -> Vec<Value> {
    if span_reached(state, stages) {
        auto_revisits(state, now)
    } else {
        Vec::new()
    }
}

/// One drain revisit (as in `select`) when nothing else can be claimed.
pub fn drain_one(state: &mut Value, stages: &[Value], now: &str) -> Option<Value> {
    if !span_reached(state, stages) {
        return None;
    }
    let key = drain_candidate(state)?;
    close_park(state, &key, "drain", None, None, now)
}

/// The claimed units (the current unit excluded) as `(unit, stage)`, in claim order.
pub fn claims(state: &Value) -> Vec<(String, String)> {
    claim_rows(state)
        .iter()
        .filter_map(|claim| {
            Some((
                str_of(claim, "unit")?.to_owned(),
                str_of(claim, "stage")?.to_owned(),
            ))
        })
        .collect()
}

/// Whether unit `key` is claimed (not counting the current unit).
pub fn is_claimed(state: &Value, key: &str) -> bool {
    claims(state)
        .iter()
        .any(|(unit, _)| normalize_id(unit) == normalize_id(key))
}

/// Remove the claim of `key`; returns its position and row.
pub fn take_claim(state: &mut Value, key: &str) -> Option<(usize, Value)> {
    let mut rows = claim_rows(state);
    let index = rows.iter().position(|row| {
        str_of(row, "unit").is_some_and(|unit| normalize_id(unit) == normalize_id(key))
    })?;
    let row = rows.remove(index);
    set_claim_rows(state, rows);
    Some((index, row))
}

/// Put a claim row back at `index` (or append it), with its `stage` set to `stage`.
pub fn put_claim(state: &mut Value, index: Option<usize>, mut row: Value, stage: &str) {
    row["stage"] = json!(stage);
    let mut rows = claim_rows(state);
    let at = index.unwrap_or(rows.len()).min(rows.len());
    rows.insert(at, row);
    set_claim_rows(state, rows);
}

/// Stamp unit `key`'s `selectedAt` (the freshness reference of its step's
/// `regression.json`) when it moves on to its next step without a new selection.
pub fn stamp_selected(state: &mut Value, key: &str, now: &str) {
    if let Some(entry) = unit_mut(state, key) {
        entry["selectedAt"] = json!(now);
    }
}

/// A new claim row for `key` on `stage`.
pub fn claim_row(key: &str, stage: &str, now: &str) -> Value {
    json!({"unit": key, "stage": stage, "claimedAt": now})
}

/// The current unit step `(stage, unit)` when `currentStage`/`currentUnit` name a
/// work-unit step that can run now.
pub fn primary_step(state: &Value, stages: &[Value]) -> Option<(String, String)> {
    let stage = str_of(state, "currentStage").filter(|stage| !stage.is_empty())?;
    if !span(stages)
        .repeating
        .iter()
        .any(|item| stage_id(item) == stage)
    {
        return None;
    }
    let named = str_of(state, "currentUnit").filter(|unit| !unit.is_empty())?;
    let key = resolve_unit(state, named)?;
    unit_step_problem(state, stages, &key, stage)
        .is_none()
        .then(|| (stage.to_owned(), key))
}

/// Every unit in flight as `(unit, stage)`: the current unit step first, then the
/// claims in claim order.
pub fn in_flight(state: &Value, stages: &[Value]) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = primary_step(state, stages)
        .map(|(stage, unit)| (unit, stage))
        .into_iter()
        .collect();
    rows.extend(claims(state));
    rows
}

/// Drop claims that cannot run their recorded step any more (parked, done, removed,
/// waiting, a pending design rework, a moved step) or that duplicate the current unit.
pub fn drop_unrunnable_claims(state: &mut Value, stages: &[Value]) {
    let rows = claim_rows(state);
    if rows.is_empty() {
        return;
    }
    let current = str_of(state, "currentUnit").map(normalize_id);
    let mut seen: Vec<String> = Vec::new();
    let mut kept: Vec<Value> = Vec::new();
    for row in rows {
        let (Some(unit), Some(stage)) = (str_of(&row, "unit"), str_of(&row, "stage")) else {
            continue;
        };
        let Some(key) = resolve_unit(state, unit) else {
            continue;
        };
        if current.as_deref() == Some(normalize_id(&key).as_str())
            || seen.contains(&key)
            || unit_step_problem(state, stages, &key, stage).is_some()
        {
            continue;
        }
        seen.push(key);
        kept.push(row);
    }
    set_claim_rows(state, kept);
}

/// Clear a current unit step that cannot run any more and drop claims that cannot,
/// after a command changed the ledger.
pub fn settle_pool(state: &mut Value, stages: &[Value]) {
    clear_unrunnable_current(state, stages);
    drop_unrunnable_claims(state, stages);
}

/// The in-flight view of unit directives and `orchestrate queue`.
pub fn parallel_view(state: &Value, stages: &[Value], workers: u64) -> Value {
    let rows: Vec<Value> = in_flight(state, stages)
        .into_iter()
        .map(|(unit, stage)| json!({"unit": unit, "step": stage}))
        .collect();
    let free = workers.saturating_sub(rows.len() as u64);
    json!({
        "workers": workers,
        "inFlight": rows,
        "free": free,
    })
}

/// The time a unit's `regression.json` must postdate, and whether that is the last
/// satisfaction rather than the step's selection: the later of the unit's
/// `selectedAt` and the ledger's `lastSatisfiedAt` (the last time any unit became
/// satisfied). With several units in flight this makes each unit's final run cover a
/// tree that already holds every satisfied unit's code; with one unit in flight the
/// selection is always the later time, so nothing changes.
pub fn freshness_reference(state: &Value, key: &str) -> (Option<String>, bool) {
    let selected = unit(state, key)
        .and_then(|unit| str_of(unit, "selectedAt"))
        .map(str::to_owned);
    let satisfied_at = ledger(state)
        .and_then(|ledger| str_of(ledger, "lastSatisfiedAt"))
        .map(str::to_owned);
    match (selected, satisfied_at) {
        (Some(selected), Some(at)) if at > selected => (Some(at), true),
        (selected, _) => (selected, false),
    }
}

// ---------------------------------------------------------------------------
// Strict loading of work-units.json (no one-unit placeholder)
// ---------------------------------------------------------------------------

/// One unit row of `work-units.json`.
#[derive(Clone, Debug)]
pub struct UnitRow {
    /// The id as written (`U-01` stays `U-01`).
    pub id: String,
    pub title: Value,
    pub depends_on: Vec<String>,
    /// The `tests` field of the row (TS ids as written).
    pub tests: Vec<String>,
    pub row: Value,
}

/// What [`load_work_units`] read.
#[derive(Clone, Debug)]
pub struct WorkUnitsInput {
    pub rel_path: String,
    pub sha256: String,
    pub registry_sha256: Option<String>,
    /// The parsed `ids.json` when it is a usable registry.
    pub registry: Option<Value>,
    /// The `work-units.md` text, when readable.
    pub markdown: Option<String>,
    pub units: Vec<UnitRow>,
}

struct ArtifactPaths {
    work_units_json: Option<(String, std::path::PathBuf)>,
    registry: Option<std::path::PathBuf>,
    markdown: Option<std::path::PathBuf>,
}

fn design_artifacts(root: &Path, graph: &Value, state: &Value) -> Result<ArtifactPaths, String> {
    let mut lookup = state.clone();
    remove_key(&mut lookup, "currentUnit");
    lookup["depth"] = json!(depth_of(state));
    let Some(design) = stage_by_id(graph, "implementation-design") else {
        return Ok(ArtifactPaths {
            work_units_json: None,
            registry: None,
            markdown: None,
        });
    };
    let expected = expected_artifacts(root, graph, &lookup, design)
        .map_err(|error| format!("{LOAD_ERROR}{error}"))?;
    let find = |output: &str| expected.iter().find(|item| item.output == output);
    Ok(ArtifactPaths {
        work_units_json: find("work-units-json")
            .map(|item| (item.rel_path.clone(), item.path.clone())),
        registry: find("id-registry").map(|item| item.path.clone()),
        markdown: find("work-units").map(|item| item.path.clone()),
    })
}

/// Read a regular file (never through a symbolic link); `Ok(None)` when it is missing.
pub fn read_regular(path: &Path) -> Result<Option<Vec<u8>>, &'static str> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("cannot be read"),
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
            Err("is not a regular file")
        }
        Ok(_) => fs::read(path).map(Some).map_err(|_| "cannot be read"),
    }
}

fn units_of(data: &Value) -> Vec<Value> {
    match data.get("units") {
        Some(Value::Array(units)) => units.clone(),
        _ => data.as_array().cloned().unwrap_or_default(),
    }
}

/// Load `work-units.json` strictly.
///
/// `Ok(None)` when the depth does not produce `work-units-json` (fast) or the file
/// declares `{"status": "not-applicable"}`: Develop then runs as one implicit unit.
/// Every other problem (missing, not a regular file, unreadable, invalid) is an error
/// that starts with "Develop cannot select a work unit: ".
pub fn load_work_units(
    root: &Path,
    graph: &Value,
    state: &Value,
) -> Result<Option<WorkUnitsInput>, String> {
    let paths = design_artifacts(root, graph, state)?;
    let Some((rel, path)) = paths.work_units_json else {
        return Ok(None);
    };
    crate::workflow::assert_no_symlink_components(root, &rel)
        .map_err(|_| format!("{LOAD_ERROR}{rel} is not a regular file"))?;
    let bytes = match read_regular(&path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Err(format!(
                "{LOAD_ERROR}{rel} is missing; it is required at {} depth. Restore it, or run 'dtech-aidlc intent rework --detail \"<reason>\"' and write it in implementation-design, or replace it with {{\"status\":\"not-applicable\",\"reason\":\"<why>\"}} to keep running Develop as one implicit unit.",
                depth_of(state)
            ))
        }
        Err(problem) => return Err(format!("{LOAD_ERROR}{rel} {problem}")),
    };
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| format!("{LOAD_ERROR}{rel} is not UTF-8 text"))?;
    let problems = crate::ids::work_unit_problems(&text);
    if let Some(first) = problems.first() {
        let more = if problems.len() > 1 {
            format!(" (and {} more)", problems.len() - 1)
        } else {
            String::new()
        };
        return Err(format!("{LOAD_ERROR}{rel}: {first}{more}"));
    }
    let data: Value = serde_json::from_str(&text)
        .map_err(|_| format!("{LOAD_ERROR}{rel}: work-units.json is not valid JSON"))?;
    if data.is_object() && str_of(&data, "status") == Some("not-applicable") {
        return Ok(None);
    }
    let units = units_of(&data)
        .into_iter()
        .map(|row| UnitRow {
            id: str_of(&row, "id").unwrap_or("").to_owned(),
            title: row.get("title").cloned().unwrap_or(Value::Null),
            depends_on: row
                .get("dependsOn")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            tests: row
                .get("tests")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            row,
        })
        .collect();
    let (registry_sha256, registry) = match paths
        .registry
        .as_deref()
        .map(read_regular)
        .and_then(Result::ok)
        .flatten()
    {
        Some(bytes) => {
            let parsed = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .filter(|data| data.get("ids").is_some_and(Value::is_object));
            (Some(sha256_hex(&bytes)), parsed)
        }
        None => (None, None),
    };
    let markdown = paths
        .markdown
        .as_deref()
        .map(read_regular)
        .and_then(Result::ok)
        .flatten()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    Ok(Some(WorkUnitsInput {
        rel_path: rel,
        sha256: sha256_hex(&bytes),
        registry_sha256,
        registry,
        markdown,
        units,
    }))
}

/// The TS numbers registered in the intent's `ids.json`, when it is a readable
/// registry (`regression.json` `ts` lists are checked against them).
pub fn registered_tests(root: &Path, graph: &Value, state: &Value) -> Option<BTreeSet<u64>> {
    let paths = design_artifacts(root, graph, state).ok()?;
    let bytes = read_regular(paths.registry.as_deref()?).ok()??;
    let registry: Value = serde_json::from_slice(&bytes).ok()?;
    let entries = registry.pointer("/ids/TS")?.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|entry| str_of(entry, "id"))
            .filter_map(crate::regression::ts_number)
            .collect(),
    )
}

/// Test ownership: the union of each unit's `tests` and the `ids.json` TS entries
/// whose `traces` name a work unit. Keys are TS ids, values normalized unit ids.
pub fn ownership(input: &WorkUnitsInput) -> BTreeMap<String, BTreeSet<String>> {
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for unit in &input.units {
        for test in &unit.tests {
            owners
                .entry(test.clone())
                .or_default()
                .insert(normalize_id(&unit.id));
        }
    }
    let entries = input
        .registry
        .as_ref()
        .and_then(|registry| registry.pointer("/ids/TS"))
        .and_then(Value::as_array);
    for entry in entries.into_iter().flatten() {
        let Some(test) = str_of(entry, "id") else {
            continue;
        };
        for target in entry
            .get("traces")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let target = normalize_id(target);
            if crate::ids::prefix_of(&target) == Some("WU") {
                owners.entry(test.to_owned()).or_default().insert(target);
            }
        }
    }
    owners
}

fn owned_tests(owners: &BTreeMap<String, BTreeSet<String>>, unit: &str) -> Vec<String> {
    let unit = normalize_id(unit);
    let mut tests: Vec<String> = owners
        .iter()
        .filter(|(_, units)| units.contains(&unit))
        .map(|(test, _)| test.clone())
        .collect();
    tests.sort_by(|a, b| id_order(a, b));
    tests
}

// ---------------------------------------------------------------------------
// Fingerprints (design inputs of one unit)
// ---------------------------------------------------------------------------

fn is_status_item(line: &str) -> bool {
    let rest = line.trim_start();
    let Some(rest) = rest.strip_prefix(['-', '*']) else {
        return false;
    };
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("**").unwrap_or(rest);
    let Some(after) = rest.strip_prefix("Status") else {
        return false;
    };
    if after
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return false;
    }
    after.contains(':')
}

/// The normalised `## <id>` section of `work-units.md` (empty when there is none).
pub fn unit_section(markdown: &str, id: &str) -> String {
    let wanted = normalize_id(id);
    let lines: Vec<String> = markdown
        .split('\n')
        .map(|line| {
            line.strip_suffix('\r')
                .unwrap_or(line)
                .trim_end()
                .to_owned()
        })
        .collect();
    let heading_id = |line: &str| -> Option<String> {
        let rest = line.strip_prefix("##")?;
        if rest.starts_with('#') || !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let token: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        Some(normalize_id(&token))
    };
    let Some(start) = lines
        .iter()
        .position(|line| heading_id(line).as_deref() == Some(wanted.as_str()))
    else {
        return String::new();
    };
    let mut section = vec![lines[start].clone()];
    for line in &lines[start + 1..] {
        if line.starts_with("## ") {
            break;
        }
        if !is_status_item(line) {
            section.push(line.clone());
        }
    }
    section.join("\n")
}

/// `sha256:<hex>` over the unit row, its Markdown section without `Status:` items, and
/// the registry entries of every FN/IF/TS/CFG id cited there or listed in `tests`.
pub fn input_fingerprint(input: &WorkUnitsInput, unit: &UnitRow, tests: &[String]) -> String {
    let section = input
        .markdown
        .as_deref()
        .map(|text| unit_section(text, &unit.id))
        .unwrap_or_default();
    let mut cited: BTreeSet<String> = crate::ids::cited_ids_ordered(&section)
        .into_iter()
        .filter(|id| matches!(crate::ids::prefix_of(id), Some("FN" | "IF" | "TS" | "CFG")))
        .collect();
    cited.extend(tests.iter().cloned());
    let mut cited: Vec<String> = cited.into_iter().collect();
    cited.sort_by(|a, b| id_order(a, b));
    let entries: Vec<Value> = cited
        .iter()
        .filter_map(|id| {
            let prefix = crate::ids::prefix_of(id)?;
            input
                .registry
                .as_ref()?
                .pointer(&format!("/ids/{prefix}"))?
                .as_array()?
                .iter()
                .find(|entry| str_of(entry, "id").map(normalize_id).as_deref() == Some(id))
                .cloned()
        })
        .collect();
    let text = format!(
        "{FINGERPRINT_TAG}{}\n{section}\n{}",
        canonical_json(&unit.row),
        canonical_json(&Value::Array(entries))
    );
    format!("sha256:{}", sha256_hex(text.as_bytes()))
}

// ---------------------------------------------------------------------------
// Ledger construction and 1.0.0 migration
// ---------------------------------------------------------------------------

/// What [`ensure_ledger`] (or a reconcile) did to the state.
#[derive(Clone, Debug, PartialEq)]
pub enum LedgerChange {
    None,
    Created {
        units: usize,
    },
    Migrated {
        units: usize,
        ignored_keys: Vec<String>,
        prior_results: usize,
    },
    /// A ledger that an older dtech-aidlc changed afterwards was brought up to date
    /// (section 3.6 of the design): units re-derived from newer stage entries, a 1.0.0
    /// design rework converted (and reconciled when 1.0.0 already completed the
    /// redesign), and a release-candidate gate approved by 1.0.0 accepted.
    Resynced {
        units: Vec<String>,
        legacy_rework: bool,
        legacy_gate: bool,
        /// The `units.reconciled` details when the resync reconciled a redesign.
        reconciled: Option<Value>,
    },
}

impl LedgerChange {
    /// The audit rows (`units.created`, `units.migrated`, or `units.resynced` plus
    /// `units.reconciled`) for this change, in the order they are appended.
    pub fn audit_rows(&self, intent_id: &str) -> Vec<(&'static str, Value)> {
        match self {
            LedgerChange::None => Vec::new(),
            LedgerChange::Created { units } => vec![(
                "units.created",
                json!({"intentId": intent_id, "units": units}),
            )],
            LedgerChange::Migrated {
                units,
                ignored_keys,
                prior_results,
            } => vec![(
                "units.migrated",
                json!({
                    "intentId": intent_id,
                    "units": units,
                    "from": "1.0.0-stage-rows",
                    "ignoredKeys": ignored_keys,
                    "priorResults": prior_results,
                }),
            )],
            LedgerChange::Resynced {
                units,
                legacy_rework,
                legacy_gate,
                reconciled,
            } => {
                let mut rows = vec![(
                    "units.resynced",
                    json!({
                        "intentId": intent_id,
                        "units": units,
                        "legacyRework": legacy_rework,
                        "legacyGate": legacy_gate,
                    }),
                )];
                if let Some(details) = reconciled {
                    rows.push(("units.reconciled", reconciled_row(intent_id, details)));
                }
                rows
            }
        }
    }
}

/// The `units.reconciled` audit row: `intentId` followed by the reconcile details.
pub fn reconciled_row(intent_id: &str, details: &Value) -> Value {
    let mut row = Map::new();
    row.insert("intentId".into(), json!(intent_id));
    if let Some(details) = details.as_object() {
        row.extend(details.clone());
    }
    Value::Object(row)
}

/// The stage entry 1.0.0 recorded for `id` (exact key first, then either spelling).
fn legacy_entry<'a>(state: &'a Value, stage: &str, id: &str) -> Option<&'a Value> {
    let units = state
        .get("stages")
        .and_then(|rows| rows.get(stage))
        .and_then(|row| row.get("units"))
        .and_then(Value::as_object)?;
    units.get(id).or_else(|| {
        let wanted = normalize_id(id);
        units
            .iter()
            .find(|(key, _)| normalize_id(key) == wanted)
            .map(|(_, entry)| entry)
    })
}

/// A unit's steps, status, history and prior results from the 1.0.0 stage rows.
fn migrate_unit(
    state: &Value,
    repeating: &[&Value],
    id: &str,
) -> (Map<String, Value>, String, Vec<Value>, Vec<Value>) {
    let mut steps = Map::new();
    let mut history = Vec::new();
    let mut prior = Vec::new();
    for stage in repeating {
        let sid = stage_id(stage);
        let entry = legacy_entry(state, sid, id);
        let status = entry.and_then(|entry| str_of(entry, "status"));
        let reported = entry.and_then(|entry| entry.get("reportedAt")).cloned();
        let step = match status {
            Some(done @ ("complete" | "skipped")) => {
                history.push(json!({"stage": sid, "result": done, "at": reported.clone().unwrap_or(Value::Null)}));
                json!(done)
            }
            Some("blocked") => json!("blocked"),
            _ => Value::Null,
        };
        steps.insert(sid.to_owned(), step);
        if status == Some("pending") {
            if let Some(at) = reported.as_ref().filter(|at| at.is_string()) {
                let reports = state
                    .get("stages")
                    .and_then(|rows| rows.get(sid))
                    .and_then(|row| row.get("reports"))
                    .and_then(Value::as_array);
                if let Some(report) = reports
                    .into_iter()
                    .flatten()
                    .rfind(|report| report.get("at") == Some(at))
                {
                    prior.push(json!({
                        "stage": sid,
                        "result": report.get("result").cloned().unwrap_or(Value::Null),
                        "at": at,
                    }));
                }
            }
        }
    }
    let legacy_done = state
        .get("units")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|row| str_of(row, "id").map(normalize_id) == Some(normalize_id(id)))
        .is_some_and(|row| stage_finished(str_of(row, "status")));
    let finished =
        |stage: &Value| stage_finished(steps.get(stage_id(stage)).and_then(Value::as_str));
    let blocked =
        |stage: &Value| steps.get(stage_id(stage)).and_then(Value::as_str) == Some("blocked");
    let last = repeating.last().copied();
    let first = repeating.first().copied();
    let status = if legacy_done || last.is_some_and(finished) {
        "green"
    } else if last.is_some_and(blocked) {
        "blocked"
    } else if first.is_some_and(finished) && repeating.len() > 1 {
        "red"
    } else if first.is_some_and(blocked) {
        "blocked"
    } else {
        "pending"
    };
    (steps, status.to_owned(), history, prior)
}

/// Build a ledger from `input`. With `migrate`, steps and statuses come from the
/// 1.0.0 stage rows (section 3.6 of the design); otherwise every unit is `pending`.
pub fn build_ledger(
    input: &WorkUnitsInput,
    state: &Value,
    stages: &[Value],
    now: &str,
    migrate: bool,
    pre_bytes: Option<&[u8]>,
) -> (Value, LedgerChange) {
    let span = span(stages);
    let owners = ownership(input);
    let mut units = Map::new();
    let mut prior_count = 0;
    let mut satisfied_count = 0;
    for row in &input.units {
        let tests = owned_tests(&owners, &row.id);
        let (steps, status, history, prior) = if migrate {
            migrate_unit(state, &span.repeating, &row.id)
        } else {
            (
                span.repeating
                    .iter()
                    .map(|stage| (stage_id(stage).to_owned(), Value::Null))
                    .collect(),
                "pending".to_owned(),
                Vec::new(),
                Vec::new(),
            )
        };
        prior_count += prior.len();
        if satisfied(Some(&status)) {
            satisfied_count += 1;
        }
        let mut unit = Map::new();
        unit.insert("title".into(), row.title.clone());
        unit.insert("dependsOn".into(), json!(row.depends_on));
        unit.insert("tests".into(), json!(tests));
        unit.insert("status".into(), json!(status));
        unit.insert("selectedAt".into(), Value::Null);
        unit.insert("steps".into(), Value::Object(steps));
        unit.insert("attempts".into(), json!({}));
        unit.insert("blockedStreak".into(), Value::Null);
        unit.insert("park".into(), Value::Null);
        unit.insert("parkHistory".into(), json!([]));
        unit.insert(
            "inputFingerprint".into(),
            json!(input_fingerprint(input, row, &tests)),
        );
        if migrate {
            unit.insert("priorResults".into(), Value::Array(prior));
        }
        unit.insert("history".into(), Value::Array(history));
        unit.insert("reopened".into(), json!([]));
        unit.insert("removed".into(), json!(false));
        unit.insert("splitInto".into(), Value::Null);
        unit.insert("updatedAt".into(), json!(now));
        units.insert(row.id.clone(), Value::Object(unit));
    }
    let known: BTreeSet<String> = input
        .units
        .iter()
        .map(|row| normalize_id(&row.id))
        .collect();
    let mut ignored: BTreeSet<String> = BTreeSet::new();
    if migrate {
        for stage in &span.repeating {
            if let Some(entries) = state
                .get("stages")
                .and_then(|rows| rows.get(stage_id(stage)))
                .and_then(|row| row.get("units"))
                .and_then(Value::as_object)
            {
                ignored.extend(
                    entries
                        .keys()
                        .filter(|key| !known.contains(&normalize_id(key)))
                        .cloned(),
                );
            }
        }
    }
    let ignored: Vec<String> = ignored.into_iter().collect();
    let mut ledger = Map::new();
    ledger.insert("schemaVersion".into(), json!(LEDGER_SCHEMA_VERSION));
    ledger.insert(
        "source".into(),
        json!({"path": input.rel_path, "sha256": input.sha256, "registrySha256": input.registry_sha256}),
    );
    ledger.insert(
        "order".into(),
        json!(input
            .units
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>()),
    );
    ledger.insert("progressSeq".into(), json!(satisfied_count));
    ledger.insert("syncedAt".into(), json!(now));
    ledger.insert("units".into(), Value::Object(units));
    ledger.insert("deferredRegressions".into(), json!([]));
    ledger.insert(
        "sweep".into(),
        json!({"status": "pending", "selectedAt": null, "at": null, "failures": [], "attempts": 0, "via": null}),
    );
    ledger.insert("baselineTests".into(), json!([]));
    ledger.insert("rework".into(), Value::Null);
    let change = if migrate {
        ledger.insert(
            "migration".into(),
            json!({
                "from": "1.0.0-stage-rows",
                "at": now,
                "ignoredKeys": ignored,
                "priorResults": prior_count,
                "preSha256": pre_bytes.map(sha256_hex),
            }),
        );
        LedgerChange::Migrated {
            units: input.units.len(),
            ignored_keys: ignored,
            prior_results: prior_count,
        }
    } else {
        LedgerChange::Created {
            units: input.units.len(),
        }
    };
    (Value::Object(ledger), change)
}

/// Create (or migrate to) the ledger when the intent has reached the repeat span and
/// has real work units. Runs only when `workUnits` is absent, every stage before the
/// span is finished and the span itself is not finished. A load error is returned.
///
/// A stored ledger is re-synced instead (see [`resync`]), and a recorded unit step
/// that cannot run any more (see [`unit_step_problem`]) is cleared.
///
/// Purity: the output differs from the input only in `workUnits`, plus
/// `currentStage`/`currentUnit` when they no longer describe the step the selector
/// would pick (both are then cleared).
pub fn ensure_ledger(
    root: &Path,
    graph: &Value,
    state: &mut Value,
    stages: &[Value],
    now: &str,
    pre_bytes: Option<&[u8]>,
) -> Result<LedgerChange, String> {
    if has_ledger(state) {
        let change = resync(root, graph, state, stages, now)?;
        settle_pool(state, stages);
        return Ok(change);
    }
    let span = span(stages);
    if span.repeating.is_empty()
        || span
            .before
            .iter()
            .any(|stage| !stage_finished(row_status(state, stage_id(stage))))
        || span
            .repeating
            .iter()
            .all(|stage| stage_finished(row_status(state, stage_id(stage))))
    {
        return Ok(LedgerChange::None);
    }
    let Some(input) = load_work_units(root, graph, state)? else {
        return Ok(LedgerChange::None);
    };
    let migrate = span
        .repeating
        .iter()
        .any(|stage| row_exists(state, stage_id(stage)));
    let (ledger, change) = build_ledger(&input, state, stages, now, migrate, pre_bytes);
    state[LEDGER_KEY] = ledger;
    clear_stale_current(state, stages);
    Ok(change)
}

/// Keep `currentStage`/`currentUnit` only when they name the step the selector picks;
/// a repeating `currentStage` that does not is cleared together with `currentUnit`.
fn clear_stale_current(state: &mut Value, stages: &[Value]) {
    let span = span(stages);
    if let Some(current) = str_of(state, "currentStage").map(str::to_owned) {
        if span
            .repeating
            .iter()
            .any(|stage| stage_id(stage) == current)
        {
            let current_unit = str_of(state, "currentUnit").map(normalize_id);
            let keep = match peek(state, stages) {
                Selection::Unit { stage, unit } => {
                    stage_id(&stage) == current && current_unit == Some(normalize_id(&unit))
                }
                Selection::Sweep(stage) => stage_id(&stage) == current && current_unit.is_none(),
                _ => false,
            };
            if !keep {
                state["currentStage"] = Value::Null;
                remove_key(state, "currentUnit");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Selector
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    /// A stage outside the repeat span.
    Stage(Value),
    /// A work unit's next step.
    Unit { stage: Value, unit: String },
    /// The final strict sweep on the last repeating stage.
    Sweep(Value),
    /// Nothing can be selected (parked units).
    Stalled {
        parked: Vec<String>,
        waiting: Vec<(String, Vec<String>)>,
    },
    /// Every stage is finished.
    Done,
}

/// The selector of a ledger intent, without changing the state.
pub fn peek(state: &Value, stages: &[Value]) -> Selection {
    peek_skipping(state, stages, &[])
}

/// [`peek`] that passes over the ready units in `skip` (ledger keys), for example units
/// held by a failing environment prerequisite. A skipped unit is still unsatisfied, so
/// the result is never the sweep while one is skipped.
pub fn peek_skipping(state: &Value, stages: &[Value], skip: &[String]) -> Selection {
    let span = span(stages);
    if let Some(stage) = span
        .before
        .iter()
        .find(|stage| !stage_finished(row_status(state, stage_id(stage))))
    {
        return Selection::Stage((*stage).clone());
    }
    if !span.repeating.is_empty() {
        for (key, unit) in active_units(state) {
            if !ready(state, &unit) || skip.contains(&key) {
                continue;
            }
            if let Some(stage) = next_step(&unit, &span.repeating) {
                return Selection::Unit {
                    stage: stage.clone(),
                    unit: key,
                };
            }
        }
        if sweep_status(state) != "green" {
            let units = active_units(state);
            if units
                .iter()
                .all(|(_, unit)| satisfied(status_of(unit)) && !is_parked(unit))
            {
                return Selection::Sweep((*span.repeating.last().unwrap()).clone());
            }
            let parked = units
                .iter()
                .filter(|(_, unit)| is_parked(unit))
                .map(|(key, _)| key.clone())
                .collect();
            let waiting = units
                .iter()
                .filter(|(_, unit)| !satisfied(status_of(unit)) && !is_parked(unit))
                .map(|(key, unit)| (key.clone(), waiting_on(state, unit)))
                .collect();
            return Selection::Stalled { parked, waiting };
        }
    }
    match span
        .after
        .iter()
        .find(|stage| !stage_finished(row_status(state, stage_id(stage))))
    {
        Some(stage) => Selection::Stage((*stage).clone()),
        None => Selection::Done,
    }
}

/// Apply a selection: set or clear `currentUnit` and stamp `selectedAt`.
///
/// `selectedAt` is the freshness reference of the step's `regression.json`, so it is
/// kept when `next` selects the step that is already current (no report or park came
/// in between, so it is the same attempt); every report or park clears
/// `currentStage`, and the following selection stamps a new time.
pub fn apply_selection(state: &mut Value, selection: &Selection, now: &str) {
    let current_stage = str_of(state, "currentStage").map(str::to_owned);
    match selection {
        Selection::Unit { stage, unit } => {
            let same = current_stage.as_deref() == Some(stage_id(stage))
                && str_of(state, "currentUnit").map(normalize_id) == Some(normalize_id(unit));
            state["currentUnit"] = json!(unit);
            if let Some(entry) = unit_mut(state, unit) {
                if !(same && entry.get("selectedAt").is_some_and(Value::is_string)) {
                    entry["selectedAt"] = json!(now);
                }
            }
        }
        Selection::Sweep(stage) => {
            let same = current_stage.as_deref() == Some(stage_id(stage))
                && state
                    .get("currentUnit")
                    .is_none_or(|unit| unit.is_null() || unit == "");
            remove_key(state, "currentUnit");
            if let Some(sweep) = state
                .get_mut(LEDGER_KEY)
                .and_then(|ledger| ledger.get_mut("sweep"))
            {
                if !(same && sweep.get("selectedAt").is_some_and(Value::is_string)) {
                    sweep["selectedAt"] = json!(now);
                }
            }
        }
        _ => remove_key(state, "currentUnit"),
    }
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// The signature of a blocked report: stage, unit, kind, needs and blockers. The
/// free-text detail is deliberately excluded.
pub fn blocked_signature(
    stage: &str,
    unit: &str,
    kind: &str,
    needs: &[String],
    blocked_by: &[String],
) -> String {
    let mut needs = needs.to_vec();
    needs.sort();
    sha256_hex(
        canonical_json(&json!({
            "stage": stage,
            "unit": normalize_id(unit),
            "kind": kind,
            "needs": needs,
            "blockedBy": blocked_by,
        }))
        .as_bytes(),
    )
}

/// The last report of `stage` for unit `key`; a migrated unit with no tagged report
/// falls back to the row's last report when it is untagged.
pub fn last_unit_report<'a>(state: &'a Value, stage: &str, key: &str) -> Option<&'a Value> {
    let reports = state
        .get("stages")?
        .get(stage)?
        .get("reports")?
        .as_array()?;
    let wanted = normalize_id(key);
    if let Some(report) = reports.iter().rfind(|report| {
        str_of(report, "unit").map(normalize_id).as_deref() == Some(wanted.as_str())
    }) {
        return Some(report);
    }
    reports
        .last()
        .filter(|report| report.get("unit").is_none() && report.get("sweep").is_none())
}

/// The last report of the release sweep on `stage`.
pub fn last_sweep_report<'a>(state: &'a Value, stage: &str) -> Option<&'a Value> {
    state
        .get("stages")?
        .get(stage)?
        .get("reports")?
        .as_array()?
        .iter()
        .rfind(|report| report.get("sweep") == Some(&Value::Bool(true)))
}

/// Record a unit report in the ledger. `last` is true on the last repeating stage.
pub fn record_unit_report(
    state: &mut Value,
    stage: &str,
    key: &str,
    result: &str,
    needs: &[String],
    last: bool,
    now: &str,
) {
    let mut became_satisfied = false;
    if let Some(unit) = unit_mut(state, key) {
        let was_satisfied = satisfied(status_of(unit));
        unit["steps"][stage] = json!(result);
        let attempts = unit["attempts"]
            .get(stage)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if !unit["attempts"].is_object() {
            unit["attempts"] = json!({});
        }
        unit["attempts"][stage] = json!(attempts + 1);
        if let Some(history) = unit.get_mut("history").and_then(Value::as_array_mut) {
            history.push(json!({"stage": stage, "result": result, "at": now}));
        } else {
            unit["history"] = json!([{"stage": stage, "result": result, "at": now}]);
        }
        match result {
            "blocked" => {
                unit["status"] = json!("blocked");
                if needs.is_empty() {
                    let signature = blocked_signature(stage, key, "unclassified", &[], &[]);
                    let count = match unit.get("blockedStreak") {
                        Some(streak) if str_of(streak, "signature") == Some(signature.as_str()) => {
                            streak.get("count").and_then(Value::as_u64).unwrap_or(0) + 1
                        }
                        _ => 1,
                    };
                    unit["blockedStreak"] = json!({"signature": signature, "count": count});
                }
            }
            _ => {
                unit["blockedStreak"] = Value::Null;
                unit["status"] = json!(if last { "green" } else { "red" });
                became_satisfied = last && !was_satisfied;
            }
        }
        unit["updatedAt"] = json!(now);
    }
    if became_satisfied {
        let parallel = parallel_run(state);
        if let Some(ledger) = state.get_mut(LEDGER_KEY) {
            let seq = ledger
                .get("progressSeq")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            ledger["progressSeq"] = json!(seq + 1);
            if parallel {
                ledger["lastSatisfiedAt"] = json!(now);
            }
        }
    }
}

/// Record a release sweep report. An approved `complete` turns the sweep green and
/// every satisfied unit `done`.
pub fn record_sweep_report(state: &mut Value, result: &str, now: &str) {
    let keys: Vec<String> = active_units(state)
        .into_iter()
        .filter(|(_, unit)| satisfied(status_of(unit)))
        .map(|(key, _)| key)
        .collect();
    let Some(ledger) = state.get_mut(LEDGER_KEY) else {
        return;
    };
    let attempts = ledger
        .pointer("/sweep/attempts")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if !ledger["sweep"].is_object() {
        ledger["sweep"] = json!({});
    }
    ledger["sweep"]["attempts"] = json!(attempts + 1);
    if result == "complete" {
        ledger["sweep"]["status"] = json!("green");
        ledger["sweep"]["at"] = json!(now);
        for key in keys {
            if let Some(unit) = ledger["units"].get_mut(&key) {
                unit["status"] = json!("done");
                unit["updatedAt"] = json!(now);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mirrors for 1.0.0
// ---------------------------------------------------------------------------

/// Rewrite the 1.0.0 mirror keys from the ledger: `stages[s].units[U].status` and
/// `stages[s].status` for repeating rows that exist, and `state.units`. When every
/// unit is satisfied but the sweep is not green, the gate unit (the last one) is
/// mirrored as unfinished on the last repeating stage, so a 1.0.0 binary stops at its
/// release-candidate gate. Stamps `workUnits.syncedAt`.
pub fn sync_mirrors(state: &mut Value, stages: &[Value], now: &str) {
    if !has_ledger(state) {
        return;
    }
    let span = span(stages);
    let repeating: Vec<String> = span
        .repeating
        .iter()
        .map(|s| stage_id(s).to_owned())
        .collect();
    let last = repeating.last().cloned();
    let units = active_units(state);
    let all_satisfied =
        !units.is_empty() && units.iter().all(|(_, unit)| satisfied(status_of(unit)));
    let sweep_green = sweep_status(state) == "green";
    let gate_unit = (all_satisfied && !sweep_green)
        .then(|| units.last().map(|(key, _)| key.clone()))
        .flatten();
    let rework_at = ledger(state)
        .and_then(|ledger| ledger.get("rework"))
        .filter(|rework| !rework.is_null())
        .map(|rework| rework.get("requestedAt").cloned().unwrap_or(Value::Null));
    for sid in &repeating {
        if !row_exists(state, sid) {
            continue;
        }
        let is_last = last.as_deref() == Some(sid.as_str());
        let mut entries = state["stages"][sid]
            .get("units")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, unit) in &units {
            let step = step_of(unit, sid);
            let projected = is_last && gate_unit.as_deref() == Some(key.as_str());
            let mirror = if projected {
                "pending"
            } else {
                step.unwrap_or("pending")
            };
            match entries.get_mut(key) {
                Some(entry) if entry.is_object() => entry["status"] = json!(mirror),
                _ if step.is_some() => {
                    entries.insert(key.clone(), json!({"status": mirror}));
                }
                _ => {}
            }
        }
        let status = if rework_at.is_some() {
            "in_progress"
        } else if is_last {
            if sweep_green {
                "complete"
            } else {
                "in_progress"
            }
        } else {
            let steps: Vec<Option<&str>> =
                units.iter().map(|(_, unit)| step_of(unit, sid)).collect();
            if !steps.is_empty() && steps.iter().all(|step| stage_finished(*step)) {
                if steps.contains(&Some("complete")) {
                    "complete"
                } else {
                    "skipped"
                }
            } else {
                "in_progress"
            }
        };
        let row = &mut state["stages"][sid];
        row["units"] = Value::Object(entries);
        row["status"] = json!(status);
        if let Some(at) = &rework_at {
            row["reworkResetAt"] = at.clone();
        }
    }
    let mirror: Vec<Value> = units
        .iter()
        .map(|(key, unit)| {
            let done = satisfied(status_of(unit)) && gate_unit.as_deref() != Some(key.as_str());
            json!({
                "id": key,
                "status": if done { "complete" } else { "in_progress" },
                "dependsOn": unit.get("dependsOn").cloned().unwrap_or_else(|| json!([])),
            })
        })
        .collect();
    state["units"] = Value::Array(mirror);
    state[LEDGER_KEY]["syncedAt"] = json!(now);
}

// ---------------------------------------------------------------------------
// Views (directive, status, intent list) and drift
// ---------------------------------------------------------------------------

/// The `unit` object of a directive for unit `key` on `stage`.
pub fn directive_unit(state: &Value, key: &str, stage: &str, threshold: u64) -> Value {
    let Some(unit) = unit(state, key) else {
        return Value::Null;
    };
    let attempts = unit
        .get("attempts")
        .and_then(|attempts| attempts.get(stage))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let streak = unit
        .get("blockedStreak")
        .and_then(|streak| streak.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "id": key,
        "title": unit.get("title").cloned().unwrap_or(Value::Null),
        "status": unit.get("status").cloned().unwrap_or(Value::Null),
        "step": stage,
        "dependsOn": unit.get("dependsOn").cloned().unwrap_or_else(|| json!([])),
        "tests": unit.get("tests").cloned().unwrap_or_else(|| json!([])),
        "attempt": attempts + 1,
        "blockedStreak": streak,
        "autoParkThreshold": threshold,
        "revisit": pending_revisit(state, key),
        // The unit commands a harness runs for this step (section 15 of the design).
        "commands": {
            "report": format!("dtech-aidlc orchestrate report --unit {key} --result complete|blocked|skipped"),
            "park": format!("dtech-aidlc orchestrate park --unit {key} --kind dep-unit|foreign-regression|owner-decision|design-defect [--blocked-by ID]... --detail \"<reason>\""),
            "queue": "dtech-aidlc orchestrate queue",
        },
    })
}

/// The compact `queue` object of a directive.
pub fn directive_queue(state: &Value) -> Value {
    let units = active_units(state);
    let ready: Vec<&String> = units
        .iter()
        .filter(|(_, unit)| ready(state, unit))
        .map(|(key, _)| key)
        .collect();
    let parked: Vec<Value> = units
        .iter()
        .filter(|(_, unit)| status_of(unit) == Some("parked"))
        .map(|(key, unit)| {
            json!({
                "unit": key,
                "kind": unit.pointer("/park/kind").cloned().unwrap_or(Value::Null),
                "blockedBy": unit.pointer("/park/blockedBy").cloned().unwrap_or_else(|| json!([])),
            })
        })
        .collect();
    let satisfied_count = units
        .iter()
        .filter(|(_, unit)| satisfied(status_of(unit)))
        .count();
    let waiting = units
        .iter()
        .filter(|(_, unit)| {
            matches!(status_of(unit), Some("pending" | "red" | "blocked"))
                && !deps_satisfied(state, unit)
        })
        .count();
    json!({
        "ready": ready,
        "parked": parked,
        "waiting": waiting,
        "satisfied": satisfied_count,
        "total": units.len(),
    })
}

/// The per-unit summary shown by `status` (and, without rows, by `intent list`).
pub fn units_summary(state: &Value, stages: &[Value], rows: bool) -> Value {
    let span = span(stages);
    let units = active_units(state);
    let mut by_status: BTreeMap<String, u64> = BTreeMap::new();
    for (_, unit) in &units {
        *by_status
            .entry(status_of(unit).unwrap_or("unknown").to_owned())
            .or_default() += 1;
    }
    let mut summary = Map::new();
    summary.insert("total".into(), json!(units.len()));
    summary.insert("byStatus".into(), json!(by_status));
    summary.insert("sweep".into(), json!(sweep_status(state)));
    if rows {
        let rows: Vec<Value> = units
            .iter()
            .map(|(key, unit)| {
                let step = if satisfied(status_of(unit)) {
                    Value::Null
                } else {
                    next_step(unit, &span.repeating)
                        .map(|stage| json!(stage_id(stage)))
                        .unwrap_or(Value::Null)
                };
                let park = unit
                    .get("park")
                    .filter(|park| park.is_object())
                    .map(|park| {
                        json!({
                            "kind": park.get("kind").cloned().unwrap_or(Value::Null),
                            "blockedBy": park.get("blockedBy").cloned().unwrap_or_else(|| json!([])),
                        })
                    })
                    .unwrap_or(Value::Null);
                json!({
                    "id": key,
                    "status": unit.get("status").cloned().unwrap_or(Value::Null),
                    "step": step,
                    "dependsOn": unit.get("dependsOn").cloned().unwrap_or_else(|| json!([])),
                    "park": park,
                })
            })
            .collect();
        summary.insert("rows".into(), Value::Array(rows));
    }
    Value::Object(summary)
}

/// What drifted since the ledger was built: `(work-units.json, ids.json)`.
pub fn drift(root: &Path, graph: &Value, state: &Value) -> Option<(bool, bool)> {
    let source = ledger(state)?.get("source")?;
    let paths = design_artifacts(root, graph, state).ok()?;
    let hash = |path: Option<&Path>| {
        path.map(read_regular)
            .and_then(Result::ok)
            .flatten()
            .map(|bytes| sha256_hex(&bytes))
    };
    let units = hash(
        paths
            .work_units_json
            .as_ref()
            .map(|(_, path)| path.as_path()),
    );
    let registry = hash(paths.registry.as_deref());
    let units_drift = units.as_deref() != str_of(source, "sha256");
    let registry_drift = registry.as_deref() != str_of(source, "registrySha256");
    (units_drift || registry_drift).then_some((units_drift, registry_drift))
}

/// Drift warnings for directives and doctor.
pub fn drift_warnings(root: &Path, graph: &Value, state: &Value) -> Vec<String> {
    let Some((units, registry)) = drift(root, graph, state) else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if units {
        warnings.push("work-units.json changed since the work-unit ledger was built; the change takes effect only through 'dtech-aidlc intent rework' or 'intent split'.".to_owned());
    }
    if registry {
        warnings.push("ids.json changed since the work-unit ledger was built; the change takes effect only through 'dtech-aidlc intent rework' or 'intent split'.".to_owned());
    }
    warnings
}

/// A read-only preview: the state with its ledger, as the next orchestrate command
/// would build it. `Ok(None)` when the intent has no work-unit ledger to show.
///
/// A stored ledger that an older dtech-aidlc changed afterwards is shown re-synced
/// (in memory); when that re-sync cannot run, the stored ledger is shown.
pub fn preview(
    root: &Path,
    graph: &Value,
    state: &Value,
    stages: &[Value],
) -> Result<Option<(Value, bool)>, String> {
    Ok(preview_with_change(root, graph, state, stages)?
        .map(|(preview, pending, _)| (preview, pending)))
}

/// [`preview`] plus the [`LedgerChange`] the next writing command would make: a
/// creation or migration (`pending` is then true), a re-sync of a stored ledger that
/// an older dtech-aidlc changed, or [`LedgerChange::None`].
pub fn preview_with_change(
    root: &Path,
    graph: &Value,
    state: &Value,
    stages: &[Value],
) -> Result<Option<(Value, bool, LedgerChange)>, String> {
    let mut copy = state.clone();
    let now = crate::audit::now_iso();
    if has_ledger(state) {
        return Ok(Some(
            match ensure_ledger(root, graph, &mut copy, stages, &now, None) {
                Ok(change) => (copy, false, change),
                Err(_) => (state.clone(), false, LedgerChange::None),
            },
        ));
    }
    match ensure_ledger(root, graph, &mut copy, stages, &now, None)? {
        LedgerChange::None => Ok(None),
        change => Ok(Some((copy, true, change))),
    }
}

// ---------------------------------------------------------------------------
// Design rework and split: requests, reconcile by fingerprints, re-sync after 1.0.0
// ---------------------------------------------------------------------------

/// A pending split request: the unit that is replaced and the units replacing it.
pub struct SplitRequest<'a> {
    pub unit: &'a str,
    pub into: &'a [String],
}

/// Record a pending ledger rework (or split) request. `units` are the named units
/// (ledger keys); a rework without `--unit` names none. The stored fingerprints are the
/// reference the reconcile compares the redesigned units against.
pub fn request_rework(
    state: &mut Value,
    now: &str,
    units: &[String],
    split: Option<SplitRequest<'_>>,
) {
    let fingerprints: Map<String, Value> = order(state)
        .into_iter()
        .filter_map(|key| {
            let fp = unit(state, &key)?.get("inputFingerprint")?.clone();
            Some((key, fp))
        })
        .collect();
    let mut request = Map::new();
    request.insert("requestedAt".into(), json!(now));
    request.insert("units".into(), json!(units));
    match split {
        Some(split) => {
            request.insert("kind".into(), json!("split"));
            request.insert("unit".into(), json!(split.unit));
            request.insert("into".into(), json!(split.into));
        }
        None => {
            request.insert("kind".into(), json!("rework"));
        }
    }
    request.insert("fingerprints".into(), Value::Object(fingerprints));
    if let Some(ledger) = state.get_mut(LEDGER_KEY) {
        ledger["rework"] = Value::Object(request);
    }
}

/// What the implementation-design directive of a ledger rework or split asks for
/// (`stage.rework.instruction`); `None` for a 1.0.0 rework request (no `units` key).
pub fn rework_instruction(request: &Value) -> Option<String> {
    let units = request.get("units")?.as_array()?;
    let units: Vec<&str> = units.iter().filter_map(Value::as_str).collect();
    if let Some(split) = request.get("split").filter(|split| split.is_object()) {
        let unit = str_of(split, "unit").unwrap_or("");
        let into = strings(split.get("into")).join(", ");
        return Some(format!(
            "Replace {unit} with {into}: give each of them its row in work-units.json, its '## <id>' section in work-units.md and its WU entry in ids.json, and drop {unit} unless it is listed among them. Keep every other unit's row and section byte-identical so its results are kept."
        ));
    }
    if units.is_empty() {
        return Some("Change only the design that needs it. Units whose work-units.json row, '## <id>' section of work-units.md or cited registry entries change are reset, together with their started direct dependants; every other unit keeps its results, so keep its row and section byte-identical.".into());
    }
    Some(format!(
        "Change only the design of {}: its work-units.json row and its '## <id>' section of work-units.md. Keep every other unit's row and section byte-identical; only the named units and their started direct dependants are reset.",
        units.join(", ")
    ))
}

/// The stage a blocked or parked unit points back from: its blocked step, else its
/// next step, else the first repeating stage.
fn unit_source_stage(unit: &Value, repeating: &[&Value]) -> Option<String> {
    repeating
        .iter()
        .find(|stage| step_of(unit, stage_id(stage)) == Some("blocked"))
        .copied()
        .or_else(|| next_step(unit, repeating))
        .or_else(|| repeating.first().copied())
        .map(|stage| stage_id(stage).to_owned())
}

/// Whether the ledger meets a rework precondition: a unit is blocked or parked (with
/// `named`, one of the named units), or the last release sweep report was blocked.
/// Returns the stage to name as `fromStage`.
pub fn rework_source(state: &Value, stages: &[Value], named: &[String]) -> Option<String> {
    let span = span(stages);
    let wanted: BTreeSet<String> = named.iter().map(|key| normalize_id(key)).collect();
    for (key, unit) in active_units(state) {
        if !wanted.is_empty() && !wanted.contains(&normalize_id(&key)) {
            continue;
        }
        if matches!(status_of(&unit), Some("blocked" | "parked")) {
            return unit_source_stage(&unit, &span.repeating);
        }
    }
    let last = span.repeating.last()?;
    last_sweep_report(state, stage_id(last))
        .filter(|report| str_of(report, "result") == Some("blocked"))
        .map(|_| stage_id(last).to_owned())
}

/// Resolve the units named by `intent rework --unit`: each must be a work unit of the
/// intent that was not removed. Returns the ledger keys, each once, in the given order.
pub fn resolve_named_units(state: &Value, names: &[String]) -> Result<Vec<String>, String> {
    let mut keys: Vec<String> = Vec::new();
    for name in names {
        let key = resolve_unit(state, name)
            .ok_or_else(|| format!("Unknown work unit {}", name.trim()))?;
        if unit(state, &key).is_some_and(removed) {
            return Err(format!(
                "{key} was removed from the work units by an earlier rework"
            ));
        }
        push_unique(&mut keys, key);
    }
    Ok(keys)
}

/// Validate `intent split ID --into ID...` against the ledger. Returns the unit's
/// ledger key, the `--into` ids (trimmed, each once) and the stage to name as
/// `fromStage`.
pub fn validate_split(
    state: &Value,
    stages: &[Value],
    unit_id: &str,
    into: &[String],
) -> Result<(String, Vec<String>, String), String> {
    let key = resolve_unit(state, unit_id)
        .ok_or_else(|| format!("Unknown work unit {}", unit_id.trim()))?;
    let entry = unit(state, &key).cloned().unwrap_or(Value::Null);
    if removed(&entry) {
        return Err(format!(
            "{key} was removed from the work units by an earlier rework"
        ));
    }
    if satisfied(status_of(&entry)) {
        return Err(format!(
            "{key} is {}; a finished unit cannot be split. Use dtech-aidlc intent rework --unit {key} --detail \"<reason>\"",
            status_of(&entry).unwrap_or("done")
        ));
    }
    let mut ids: Vec<String> = Vec::new();
    for id in into {
        let id = id.trim();
        if !crate::ids::valid_unit_id(id) {
            return Err(format!(
                "intent split --into takes work unit ids like WU-17 (got {id})"
            ));
        }
        if ids
            .iter()
            .any(|known| normalize_id(known) == normalize_id(id))
        {
            continue;
        }
        ids.push(id.to_owned());
    }
    if ids.len() < 2 {
        return Err("intent split needs at least two distinct work units in --into".into());
    }
    for id in &ids {
        if normalize_id(id) == normalize_id(&key) {
            continue;
        }
        if let Some(existing) = resolve_unit(state, id) {
            return Err(format!(
                "{id} already names a work unit of this intent ({existing}); choose a new id for --into"
            ));
        }
    }
    let span = span(stages);
    let from = unit_source_stage(&entry, &span.repeating)
        .ok_or("The active intent has no work-unit stage")?;
    Ok((key, ids, from))
}

fn started(unit: &Map<String, Value>) -> bool {
    unit.get("steps")
        .and_then(Value::as_object)
        .is_some_and(|steps| {
            steps
                .values()
                .any(|step| matches!(step.as_str(), Some("complete" | "skipped" | "blocked")))
        })
}

/// Close the open park of a unit map (kept in `parkHistory`).
fn close_park_in(unit: &mut Map<String, Value>, reason: &str, now: &str) {
    if !unit.get("park").is_some_and(Value::is_object) {
        return;
    }
    let mut closed = unit["park"].clone();
    closed["revisitedAt"] = json!(now);
    closed["revisitReason"] = json!(reason);
    closed["revisitDetail"] = Value::Null;
    closed["decidedBy"] = Value::Null;
    let mut history = unit
        .get("parkHistory")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    history.push(closed);
    unit.insert("parkHistory".into(), Value::Array(history));
    unit.insert("park".into(), Value::Null);
}

fn push_reopened(unit: &mut Map<String, Value>, by: &str, now: &str) {
    let mut reopened = unit
        .get("reopened")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    reopened.push(json!({"at": now, "by": by, "tests": []}));
    unit.insert("reopened".into(), Value::Array(reopened));
}

/// A unit of a reconcile that was not in the ledger before (keys in ledger order).
fn fresh_unit() -> Map<String, Value> {
    let mut fresh = Map::new();
    for key in ["title", "dependsOn", "tests", "status", "selectedAt"] {
        fresh.insert(key.into(), Value::Null);
    }
    fresh.insert("steps".into(), json!({}));
    fresh.insert("attempts".into(), json!({}));
    fresh.insert("blockedStreak".into(), Value::Null);
    fresh.insert("park".into(), Value::Null);
    fresh.insert("parkHistory".into(), json!([]));
    fresh.insert("inputFingerprint".into(), Value::Null);
    fresh.insert("history".into(), json!([]));
    fresh.insert("reopened".into(), json!([]));
    fresh.insert("removed".into(), json!(false));
    fresh.insert("splitInto".into(), Value::Null);
    fresh
}

/// Reconcile a pending rework or split at the `implementation-design` complete report
/// (section 7.1 of the design). The new `work-units.json` is loaded strictly and every
/// unit's `inputFingerprint` recomputed:
///
/// - `changed` = the named units (for a split, the split unit and its `--into` units)
///   plus every unit whose fingerprint differs from the one stored with the request;
/// - `reset` = `changed` plus each direct dependant of a changed or added unit that has
///   a finished or blocked step. A changed unit's open park is closed; a reset
///   dependant keeps its open park and resumes as `pending`;
/// - new ids are added as `pending`; missing ids are kept as `removed` (a dropped split
///   unit records `splitInto`), and their open deferred regressions become unowned;
/// - every other unit is kept as it is, and no report is appended for it.
///
/// A split whose `--into` units are not all listed, or whose replaced unit is still
/// listed without being named in `--into`, is refused. Returns the
/// `units.reconciled` audit details.
pub fn reconcile_rework(
    root: &Path,
    graph: &Value,
    state: &mut Value,
    stages: &[Value],
    now: &str,
) -> Result<Value, String> {
    let input = load_work_units(root, graph, state)?.ok_or_else(|| {
        format!(
            "{LOAD_ERROR}work-units.json no longer lists work units; a ledger intent needs them"
        )
    })?;
    let span = span(stages);
    let owners = ownership(&input);
    let old_order = order(state);
    let old_units = ledger(state)
        .and_then(|ledger| ledger.get("units"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let request = ledger(state)
        .and_then(|ledger| ledger.get("rework"))
        .filter(|rework| rework.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let split = str_of(&request, "kind") == Some("split");
    let by = if split { "split" } else { "rework" };
    let fingerprints = request
        .get("fingerprints")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut named: BTreeSet<String> = strings(request.get("units"))
        .iter()
        .map(|id| normalize_id(id))
        .collect();
    let new_ids: BTreeSet<String> = input
        .units
        .iter()
        .map(|row| normalize_id(&row.id))
        .collect();
    let split_unit = str_of(&request, "unit").unwrap_or("").to_owned();
    let split_into = strings(request.get("into"));
    if split {
        let refused = "Stage implementation-design cannot be completed";
        for id in &split_into {
            if !new_ids.contains(&normalize_id(id)) {
                return Err(format!(
                    "{refused}: intent split {split_unit} into {} needs {id} in work-units.json",
                    split_into.join(", ")
                ));
            }
        }
        let into: BTreeSet<String> = split_into.iter().map(|id| normalize_id(id)).collect();
        if new_ids.contains(&normalize_id(&split_unit))
            && !into.contains(&normalize_id(&split_unit))
        {
            return Err(format!(
                "{refused}: intent split replaces {split_unit}, but work-units.json still lists it; remove it, or request the split again with {split_unit} in --into"
            ));
        }
        named.insert(normalize_id(&split_unit));
        named.extend(into);
    }

    // Each new row with its previous ledger entry (either id spelling).
    struct Planned<'a> {
        row: &'a UnitRow,
        tests: Vec<String>,
        fingerprint: String,
        previous: Option<Map<String, Value>>,
    }
    let planned: Vec<Planned> = input
        .units
        .iter()
        .map(|row| {
            let tests = owned_tests(&owners, &row.id);
            let fingerprint = input_fingerprint(&input, row, &tests);
            let previous = old_order
                .iter()
                .find(|key| normalize_id(key) == normalize_id(&row.id))
                .and_then(|key| {
                    let mut unit = old_units.get(key)?.as_object()?.clone();
                    // The fingerprint stored with the request is the reference.
                    if let Some(fp) = fingerprints.get(key) {
                        unit.insert("inputFingerprint".into(), fp.clone());
                    }
                    Some(unit)
                });
            Planned {
                row,
                tests,
                fingerprint,
                previous,
            }
        })
        .collect();
    let mut changed: BTreeSet<String> = BTreeSet::new();
    let mut added: BTreeSet<String> = BTreeSet::new();
    for item in &planned {
        let id = normalize_id(&item.row.id);
        match &item.previous {
            None => {
                added.insert(id);
            }
            Some(unit) => {
                let old_fp = unit.get("inputFingerprint").and_then(Value::as_str);
                if named.contains(&id)
                    || old_fp != Some(item.fingerprint.as_str())
                    || unit.get("removed") == Some(&Value::Bool(true))
                {
                    changed.insert(id);
                }
            }
        }
    }
    let triggers: BTreeSet<String> = changed.union(&added).cloned().collect();
    let mut dependants: BTreeSet<String> = BTreeSet::new();
    for item in &planned {
        let id = normalize_id(&item.row.id);
        let Some(unit) = &item.previous else {
            continue;
        };
        if changed.contains(&id) || unit.get("removed") == Some(&Value::Bool(true)) {
            continue;
        }
        let depends_on_trigger = item
            .row
            .depends_on
            .iter()
            .any(|dep| triggers.contains(&normalize_id(dep)));
        if depends_on_trigger && started(unit) {
            dependants.insert(id);
        }
    }

    let empty_steps: Map<String, Value> = span
        .repeating
        .iter()
        .map(|stage| (stage_id(stage).to_owned(), Value::Null))
        .collect();
    let mut units = Map::new();
    let (mut kept, mut reset, mut added_ids, mut changed_ids) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for item in planned {
        let id = normalize_id(&item.row.id);
        let is_new = item.previous.is_none();
        let mut unit = item.previous.unwrap_or_else(fresh_unit);
        unit.insert("title".into(), item.row.title.clone());
        unit.insert("dependsOn".into(), json!(item.row.depends_on));
        unit.insert("tests".into(), json!(item.tests));
        unit.insert("inputFingerprint".into(), json!(item.fingerprint));
        if is_new {
            unit.insert("status".into(), json!("pending"));
            unit.insert("steps".into(), Value::Object(empty_steps.clone()));
            unit.insert("updatedAt".into(), json!(now));
            added_ids.push(item.row.id.clone());
        } else if changed.contains(&id) || dependants.contains(&id) {
            unit.insert("steps".into(), Value::Object(empty_steps.clone()));
            unit.insert("attempts".into(), json!({}));
            unit.insert("blockedStreak".into(), Value::Null);
            unit.insert("removed".into(), json!(false));
            let open_park = unit.get("park").is_some_and(Value::is_object);
            if changed.contains(&id) || !open_park {
                // A redesigned unit starts again; its park is resolved by the rework.
                close_park_in(&mut unit, "rework", now);
                unit.insert("status".into(), json!("pending"));
            } else {
                // A reset dependant keeps its open park (an owner decision is not lost
                // because a dependency was redesigned) and resumes from the start.
                unit["park"]["resumeStatus"] = json!("pending");
                unit.insert("status".into(), json!("parked"));
            }
            push_reopened(&mut unit, by, now);
            unit.insert("updatedAt".into(), json!(now));
            reset.push(item.row.id.clone());
            if changed.contains(&id) {
                changed_ids.push(item.row.id.clone());
            }
        } else {
            kept.push(item.row.id.clone());
        }
        units.insert(item.row.id.clone(), Value::Object(unit));
    }
    let mut removed_ids = Vec::new();
    let mut order_ids: Vec<String> = input.units.iter().map(|row| row.id.clone()).collect();
    for key in &old_order {
        if units
            .keys()
            .any(|new| normalize_id(new) == normalize_id(key))
        {
            continue;
        }
        let Some(old) = old_units.get(key).and_then(Value::as_object) else {
            continue;
        };
        let mut old = old.clone();
        if old.get("removed") != Some(&Value::Bool(true)) {
            removed_ids.push(key.clone());
            close_park_in(&mut old, by, now);
            if split && normalize_id(key) == normalize_id(&split_unit) {
                old.insert("splitInto".into(), json!(split_into));
            }
            old.insert("removed".into(), json!(true));
            old.insert("updatedAt".into(), json!(now));
        }
        units.insert(key.clone(), Value::Object(old));
        order_ids.push(key.clone());
    }
    let anything_changed = !reset.is_empty() || !added_ids.is_empty() || !removed_ids.is_empty();
    let ledger = state
        .get_mut(LEDGER_KEY)
        .ok_or("work-unit ledger is missing")?;
    ledger["units"] = Value::Object(units);
    ledger["order"] = json!(order_ids);
    ledger["source"] = json!({"path": input.rel_path, "sha256": input.sha256, "registrySha256": input.registry_sha256});
    if anything_changed
        && ledger
            .pointer("/sweep/status")
            .and_then(Value::as_str)
            .is_some_and(|status| status == "green")
    {
        ledger["sweep"]["status"] = json!("pending");
        ledger["sweep"]["selectedAt"] = Value::Null;
    }
    ledger["rework"] = Value::Null;
    // Open deferred regressions of a removed unit have no owner any more.
    if let Some(items) = ledger
        .get_mut("deferredRegressions")
        .and_then(Value::as_array_mut)
    {
        for item in items.iter_mut() {
            let open = str_of(item, "status") == Some("open");
            let gone = str_of(item, "owner").is_some_and(|owner| {
                removed_ids
                    .iter()
                    .any(|key| normalize_id(key) == normalize_id(owner))
            });
            if open && gone {
                item["owner"] = Value::Null;
                item["unowned"] = json!(true);
            }
        }
    }
    remove_key(state, "designRework");
    remove_key(state, "designReworkPending");
    let mut details = json!({
        "kept": kept,
        "reset": reset,
        "added": added_ids,
        "removed": removed_ids,
        "changed": changed_ids,
    });
    if split {
        details["split"] = json!({"unit": split_unit, "into": split_into});
    }
    Ok(details)
}

/// Whether the ISO timestamp `value` is later than `synced`.
fn later_than(value: Option<&Value>, synced: &str) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|at| !at.is_empty() && at > synced)
}

fn stage_row<'a>(state: &'a Value, id: &str) -> Option<&'a Value> {
    state
        .get("stages")
        .and_then(|rows| rows.get(id))
        .filter(|row| row.is_object())
}

/// Bring a ledger up to date after an older dtech-aidlc changed the state (section 3.6
/// of the design). Every unit-aware write stamps `workUnits.syncedAt` with the same
/// time it uses for the stage records, so only a write by an older binary leaves a
/// stage record newer than `syncedAt`. In order:
///
/// 1. **Legacy rework.** A 1.0.0 `intent rework` (a `designReworkPending` marker, or a
///    newer `reworkRequestedAt`/`reworkResetAt`) becomes a ledger rework request with
///    no named units; when 1.0.0 already completed `implementation-design` after it, or
///    completed it while a ledger rework was pending, the reconcile runs now, so only
///    units whose design changed are reset.
/// 2. **Per unit.** Each step whose stage entry is newer is re-derived from that entry;
///    the unit's status follows and its open park is closed (`resync`).
/// 3. **Legacy gate.** A release-candidate gate approved by 1.0.0 after `syncedAt` is
///    accepted as the sweep result: the sweep turns green (`via: "1.0.0-gate"`) and
///    every satisfied unit becomes `done`.
pub fn resync(
    root: &Path,
    graph: &Value,
    state: &mut Value,
    stages: &[Value],
    now: &str,
) -> Result<LedgerChange, String> {
    let span = span(stages);
    let synced = ledger(state)
        .and_then(|ledger| str_of(ledger, "syncedAt"))
        .unwrap_or("")
        .to_owned();
    let newer = |value: Option<&Value>| !synced.is_empty() && later_than(value, &synced);
    let pending = rework_pending(state);
    let design = stage_row(state, "implementation-design")
        .cloned()
        .unwrap_or(Value::Null);
    let marker = matches!(state.get("designReworkPending"), Some(Value::Bool(true)));
    let legacy_request = !pending
        && (marker
            || newer(design.get("reworkRequestedAt"))
            || span.repeating.iter().any(|stage| {
                newer(stage_row(state, stage_id(stage)).and_then(|row| row.get("reworkResetAt")))
            }));
    let design_finished = stage_finished(str_of(&design, "status"));
    let design_done_under_legacy = pending && design_finished && newer(design.get("reportedAt"));
    let newer_units = |state: &Value| -> Vec<String> {
        active_units(state)
            .into_iter()
            .filter(|(key, _)| {
                span.repeating.iter().any(|stage| {
                    newer(
                        legacy_entry(state, stage_id(stage), key).and_then(|e| e.get("reportedAt")),
                    )
                })
            })
            .map(|(key, _)| key)
            .collect()
    };
    let gate = state
        .get("gates")
        .and_then(|gates| gates.get("release-candidate"))
        .cloned()
        .unwrap_or(Value::Null);
    let legacy_gate = newer(gate.get("at")) && str_of(&gate, "decision") == Some("approved");
    if !legacy_request && !design_done_under_legacy && !legacy_gate && newer_units(state).is_empty()
    {
        return Ok(LedgerChange::None);
    }

    // 1. Legacy rework.
    let mut reconciled = None;
    if legacy_request {
        let at = str_of(&design, "reworkRequestedAt")
            .or_else(|| {
                state
                    .get("designRework")
                    .and_then(|rework| str_of(rework, "requestedAt"))
            })
            .unwrap_or(now)
            .to_owned();
        request_rework(state, &at, &[], None);
        remove_key(state, "designReworkPending");
        if let Some(rework) = state
            .get_mut("designRework")
            .filter(|rework| rework.is_object())
        {
            if rework.get("units").is_none() {
                rework["units"] = json!([]);
            }
        }
        if design_finished && later_than(design.get("reportedAt"), &at) {
            reconciled = Some(reconcile_rework(root, graph, state, stages, now)?);
        }
    } else if design_done_under_legacy {
        reconciled = Some(reconcile_rework(root, graph, state, stages, now)?);
    }

    // 2. Per unit: steps reported by the older binary.
    let units = newer_units(state);
    let mut became_satisfied = 0;
    for key in &units {
        let mut entries = Vec::new();
        for stage in &span.repeating {
            let sid = stage_id(stage);
            if let Some(entry) =
                legacy_entry(state, sid, key).filter(|entry| newer(entry.get("reportedAt")))
            {
                entries.push((
                    sid.to_owned(),
                    str_of(entry, "status").map(str::to_owned),
                    entry.get("reportedAt").cloned().unwrap_or(Value::Null),
                ));
            }
        }
        let Some(unit) = unit_mut(state, key) else {
            continue;
        };
        let was_satisfied = satisfied(status_of(unit));
        if !unit["steps"].is_object() {
            unit["steps"] = json!({});
        }
        for (sid, status, at) in entries {
            let step = match status.as_deref() {
                Some(done @ ("complete" | "skipped" | "blocked")) => json!(done),
                _ => Value::Null,
            };
            if !step.is_null() {
                let row = json!({"stage": sid, "result": step, "at": at, "via": "1.0.0"});
                match unit.get_mut("history").and_then(Value::as_array_mut) {
                    Some(history) => history.push(row),
                    None => unit["history"] = json!([row]),
                }
            }
            unit["steps"][&sid] = step;
        }
        let finished = |stage: &Value| stage_finished(step_of(unit, stage_id(stage)));
        let blocked = |stage: &Value| step_of(unit, stage_id(stage)) == Some("blocked");
        let last = span.repeating.last().copied();
        let first = span.repeating.first().copied();
        let status = if last.is_some_and(finished) {
            if was_satisfied {
                status_of(unit).unwrap_or("green").to_owned()
            } else {
                "green".to_owned()
            }
        } else if last.is_some_and(blocked) {
            "blocked".to_owned()
        } else if first.is_some_and(finished) && span.repeating.len() > 1 {
            "red".to_owned()
        } else if first.is_some_and(blocked) {
            "blocked".to_owned()
        } else {
            "pending".to_owned()
        };
        let mut map = unit.as_object().cloned().unwrap_or_default();
        close_park_in(&mut map, "resync", now);
        *unit = Value::Object(map);
        unit["status"] = json!(status);
        unit["blockedStreak"] = Value::Null;
        unit["updatedAt"] = json!(now);
        if !was_satisfied && satisfied(Some(&status)) {
            became_satisfied += 1;
        }
    }
    if became_satisfied > 0 {
        if let Some(ledger) = state.get_mut(LEDGER_KEY) {
            ledger["progressSeq"] = json!(progress_seq_of(ledger) + became_satisfied);
        }
    }

    // 3. Legacy gate: a human decision recorded by 1.0.0 is never discarded.
    if legacy_gate {
        let keys: Vec<String> = active_units(state)
            .into_iter()
            .filter(|(_, unit)| satisfied(status_of(unit)))
            .map(|(key, _)| key)
            .collect();
        if let Some(ledger) = state.get_mut(LEDGER_KEY) {
            if !ledger["sweep"].is_object() {
                ledger["sweep"] = json!({"attempts": 0, "failures": []});
            }
            ledger["sweep"]["status"] = json!("green");
            ledger["sweep"]["via"] = json!("1.0.0-gate");
            ledger["sweep"]["at"] = gate.get("at").cloned().unwrap_or(Value::Null);
            for key in keys {
                if let Some(unit) = ledger["units"].get_mut(&key) {
                    unit["status"] = json!("done");
                    unit["updatedAt"] = json!(now);
                }
            }
        }
    }
    // During a parallel run the current unit need not be the selector's first pick
    // (it was promoted or continued); keep it while it can still run.
    if parallel_run(state) {
        clear_unrunnable_current(state, stages);
    } else {
        clear_stale_current(state, stages);
    }
    Ok(LedgerChange::Resynced {
        units,
        legacy_rework: legacy_request || design_done_under_legacy,
        legacy_gate,
        reconciled,
    })
}

fn progress_seq_of(ledger: &Value) -> u64 {
    ledger
        .get("progressSeq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Blocked-kind taxonomy: park, revisit, auto-park, drain and the stalled outcome
// ---------------------------------------------------------------------------

/// The kinds `orchestrate park` accepts. An environment prerequisite is not a park: it
/// is reported with `orchestrate report --result blocked --needs NAME`.
pub const PARK_KINDS: [&str; 4] = [
    "dep-unit",
    "foreign-regression",
    "owner-decision",
    "design-defect",
];
const MAX_BLOCKED_BY: usize = 20;

/// Park kinds that wait for other units (revisited automatically and by drain).
fn waits_on_units(kind: &str) -> bool {
    matches!(kind, "dep-unit" | "foreign-regression")
}

fn progress_seq(state: &Value) -> u64 {
    ledger(state)
        .and_then(|ledger| ledger.get("progressSeq"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn open_park(unit: &Value) -> Option<&Value> {
    unit.get("park").filter(|park| park.is_object())
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn push_unique(items: &mut Vec<String>, item: String) {
    if !items.contains(&item) {
        items.push(item);
    }
}

/// A short identifier such as `RK-16`, `CFG-17` or `Q-04`
/// (`^[A-Za-z][A-Za-z0-9]*-[A-Za-z0-9.]{1,32}$`).
fn plain_identifier(token: &str) -> bool {
    let Some((prefix, rest)) = token.split_once('-') else {
        return false;
    };
    let mut chars = prefix.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric())
        && (1..=32).contains(&rest.len())
        && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// Whether a ledger key names a unit that no longer blocks anyone: satisfied, or
/// removed from the work units.
fn no_longer_blocks(state: &Value, key: &str) -> bool {
    unit(state, key).is_none_or(|unit| removed(unit) || satisfied(status_of(unit)))
}

/// The ledger keys `unit` lists in `dependsOn` (resolved, either spelling).
fn dependency_keys(state: &Value, unit: &Value) -> Vec<String> {
    strings(unit.get("dependsOn"))
        .iter()
        .filter_map(|dep| resolve_unit(state, dep))
        .collect()
}

/// Whether `from` depends on `target`, directly or transitively.
fn depends_transitively(state: &Value, from: &str, target: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from.to_owned()];
    while let Some(key) = stack.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some(entry) = unit(state, &key) else {
            continue;
        };
        for dep in dependency_keys(state, entry) {
            if dep == target {
                return true;
            }
            stack.push(dep);
        }
    }
    false
}

/// Whether `from` waits on `target` through unsatisfied dependencies and the
/// `blockedByUnits` of open parks.
fn waits_through(state: &Value, from: &str, target: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from.to_owned()];
    while let Some(key) = stack.pop() {
        if !seen.insert(key.clone()) || no_longer_blocks(state, &key) {
            continue;
        }
        let Some(entry) = unit(state, &key) else {
            continue;
        };
        let mut next = dependency_keys(state, entry);
        if let Some(park) = open_park(entry) {
            next.extend(strings(park.get("blockedByUnits")));
        }
        for other in next {
            if other == target {
                return true;
            }
            stack.push(other);
        }
    }
    false
}

/// The units whose stored `tests` own `test` (ledger keys, in order, not removed).
fn test_owners(state: &Value, test: &str) -> Vec<String> {
    let wanted = normalize_id(test);
    active_units(state)
        .into_iter()
        .filter(|(_, unit)| {
            strings(unit.get("tests"))
                .iter()
                .any(|owned| normalize_id(owned) == wanted)
        })
        .map(|(key, _)| key)
        .collect()
}

/// Refuse unit-queue commands (park, revisit) where they do not apply: no ledger, a
/// pending ledger rework, or an intent that has not reached its work units.
pub fn require_unit_queue(state: &Value, stages: &[Value], command: &str) -> Result<(), String> {
    if !has_ledger(state) {
        return Err(format!(
            "orchestrate {command} needs work units (Develop with work-units.json)"
        ));
    }
    if rework_pending(state) {
        return Err("A design rework is pending; complete implementation-design first".into());
    }
    let span = span(stages);
    if span.repeating.is_empty()
        || span
            .before
            .iter()
            .any(|stage| !stage_finished(row_status(state, stage_id(stage))))
    {
        return Err(format!(
            "orchestrate {command} is available once Develop reaches its work units; finish the stages before them first"
        ));
    }
    Ok(())
}

/// What `orchestrate park` (or an auto-park) recorded.
#[derive(Clone, Debug, PartialEq)]
pub struct Parked {
    pub unit: String,
    pub stage: String,
    pub kind: String,
    pub blocked_by: Vec<String>,
    pub blocked_by_units: Vec<String>,
    pub signature: String,
    pub attempts: u64,
    pub detail: String,
    pub auto: bool,
    /// The blocked-report streak that triggered an auto-park.
    pub count: Option<u64>,
    pub warnings: Vec<String>,
}

impl Parked {
    /// The `unit.parked` audit row.
    pub fn audit_row(&self, intent_id: &str) -> Value {
        let mut row = json!({
            "intentId": intent_id,
            "unit": self.unit,
            "stage": self.stage,
            "kind": self.kind,
            "blockedBy": self.blocked_by,
            "blockedByUnits": self.blocked_by_units,
            "signature": self.signature,
            "attempts": self.attempts,
            "detail": self.detail,
            "auto": self.auto,
        });
        if let Some(count) = self.count {
            row["count"] = json!(count);
        }
        row
    }
}

/// An `orchestrate park` request.
pub struct ParkRequest<'a> {
    /// `--unit` (either spelling); the current unit when absent.
    pub unit: Option<&'a str>,
    pub kind: &'a str,
    pub blocked_by: &'a [String],
    pub detail: &'a str,
}

/// Validate and apply `orchestrate park` on the state (no file is written here). The
/// caller runs [`sync_mirrors`] and persists.
pub fn park(
    state: &mut Value,
    stages: &[Value],
    request: &ParkRequest,
    now: &str,
) -> Result<Parked, String> {
    require_unit_queue(state, stages, "park")?;
    let kind = request.kind.trim();
    if kind == "env-prerequisite" {
        return Err("An environment prerequisite is not a park; use orchestrate report --result blocked --needs NAME".into());
    }
    if !PARK_KINDS.contains(&kind) {
        return Err(format!("--kind must be one of {}", PARK_KINDS.join(", ")));
    }
    let detail = request.detail.trim();
    if detail.is_empty() {
        return Err("orchestrate park requires --detail \"<reason>\"".into());
    }
    let key = match request.unit.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => resolve_unit(state, id).ok_or_else(|| format!("Unknown work unit {id}"))?,
        None => {
            let claimed = claims(state);
            if !claimed.is_empty() {
                let mut names: Vec<String> = primary_step(state, stages)
                    .map(|(_, unit)| unit)
                    .into_iter()
                    .collect();
                names.extend(claimed.into_iter().map(|(unit, _)| unit));
                return Err(format!(
                    "Several work units are in flight ({}); name the one you park with --unit",
                    names.join(", ")
                ));
            }
            str_of(state, "currentUnit")
                .and_then(|id| resolve_unit(state, id))
                .ok_or("orchestrate park needs --unit ID; there is no current work unit")?
        }
    };
    let entry = unit(state, &key).cloned().unwrap_or(Value::Null);
    if removed(&entry) {
        return Err(format!("{key} was removed from the work units"));
    }
    let status = status_of(&entry).unwrap_or("pending").to_owned();
    if satisfied(Some(&status)) {
        return Err(format!(
            "{key} is already done ({status}); only unfinished work units can be parked"
        ));
    }
    if let Some(open) = open_park(&entry) {
        return Err(format!(
            "{key} is already parked ({}); run orchestrate revisit first",
            str_of(open, "kind").unwrap_or("unknown")
        ));
    }
    let blocked_by: Vec<String> = request
        .blocked_by
        .iter()
        .map(|token| token.trim().to_owned())
        .collect();
    if blocked_by.len() > MAX_BLOCKED_BY {
        return Err(format!(
            "--blocked-by takes at most {MAX_BLOCKED_BY} identifiers"
        ));
    }
    let mut units: Vec<String> = Vec::new();
    let other_unit = |token: &str| {
        resolve_unit(state, token).filter(|other| unit(state, other).is_some_and(|u| !removed(u)))
    };
    match kind {
        "dep-unit" => {
            if blocked_by.is_empty() {
                return Err(
                    "--kind dep-unit requires --blocked-by WU-xx (the unit it waits for)".into(),
                );
            }
            for token in &blocked_by {
                let other = other_unit(token).ok_or_else(|| {
                    format!("--blocked-by {token} is not a work unit of this intent")
                })?;
                if other == key {
                    return Err(format!("{key} cannot wait for itself"));
                }
                push_unique(&mut units, other);
            }
        }
        "foreign-regression" => {
            if blocked_by.is_empty() {
                return Err(
                    "--kind foreign-regression requires --blocked-by TS-xx (a failing test) or WU-xx (its unit)"
                        .into(),
                );
            }
            for token in &blocked_by {
                if crate::ids::prefix_of(token) == Some("TS") {
                    let owners = test_owners(state, token);
                    if owners.is_empty() {
                        return Err(format!("{token} has no owning unit in work-units.json; name the unit with --blocked-by WU-xx"));
                    }
                    if owners.contains(&key) {
                        return Err(format!(
                            "{token} is owned by {key} itself (owners: {}); a failure of its own test blocks {key}. Name the other owner with --blocked-by WU-xx",
                            owners.join(", ")
                        ));
                    }
                    for owner in owners {
                        push_unique(&mut units, owner);
                    }
                } else if let Some(other) = other_unit(token) {
                    if other == key {
                        return Err(format!("{key} cannot wait for itself"));
                    }
                    push_unique(&mut units, other);
                } else {
                    return Err(format!(
                        "--blocked-by {token} is neither a TS- test id nor a work unit of this intent"
                    ));
                }
            }
        }
        _ => {
            if let Some(token) = blocked_by.iter().find(|token| !plain_identifier(token)) {
                return Err(format!(
                    "--blocked-by {token} is not an identifier such as CFG-17 or RK-16"
                ));
            }
        }
    }
    let mut warnings = Vec::new();
    if waits_on_units(kind) {
        if let Some(blocker) = units
            .iter()
            .find(|blocker| depends_transitively(state, blocker, &key))
        {
            return Err(if kind == "dep-unit" {
                format!("inverted dependency: {blocker} depends on {key}, so {key} cannot wait for it; fix the order with intent rework --unit")
            } else {
                format!("{blocker} depends on {key}, so {key} cannot wait for it; report complete; failures owned by {blocker} are deferred (attributed regression)")
            });
        }
        if units.iter().all(|blocker| no_longer_blocks(state, blocker)) {
            let (names, verb) = if units.len() == 1 {
                (units[0].clone(), "is")
            } else {
                (units.join(", "), "are")
            };
            return Err(format!("{names} {verb} already done; a failure owned by a finished unit blocks {key} — fix it, or park it as design-defect and run intent rework --unit"));
        }
        if let Some(blocker) = units
            .iter()
            .find(|blocker| waits_through(state, blocker, &key))
        {
            let how = if unit(state, blocker).is_some_and(|u| open_park(u).is_some()) {
                "is parked waiting on"
            } else {
                "waits on"
            };
            return Err(format!(
                "{blocker} {how} {key}; parking {key} on {blocker} would deadlock"
            ));
        }
        if kind == "dep-unit" {
            let deps = dependency_keys(state, &entry);
            for blocker in units.iter().filter(|blocker| !deps.contains(blocker)) {
                warnings.push(format!("{key} does not list {blocker} in dependsOn; declare the dependency through intent rework --unit if it is real."));
            }
        }
    }

    let span = span(stages);
    let step = next_step(&entry, &span.repeating)
        .or_else(|| span.repeating.last().copied())
        .map(|stage| stage_id(stage).to_owned())
        .unwrap_or_default();
    let claimed = is_claimed(state, &key);
    let counted = claimed
        || (str_of(state, "currentUnit").map(normalize_id) == Some(normalize_id(&key))
            && str_of(state, "currentStage").is_some_and(|stage| !stage.is_empty()));
    let signature = blocked_signature(&step, &key, kind, &[], &blocked_by);
    let seq = progress_seq(state);
    let entry = unit_mut(state, &key).ok_or("work-unit ledger is missing")?;
    if !entry["attempts"].is_object() {
        entry["attempts"] = json!({});
    }
    let mut attempts = entry["attempts"]
        .get(&step)
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if counted {
        attempts += 1;
        entry["attempts"][&step] = json!(attempts);
        let row = json!({"stage": step, "result": "blocked", "at": now});
        match entry.get_mut("history").and_then(Value::as_array_mut) {
            Some(history) => history.push(row),
            None => entry["history"] = json!([row]),
        }
    }
    if !entry["steps"].is_object() {
        entry["steps"] = json!({});
    }
    entry["steps"][&step] = json!("blocked");
    entry["park"] = json!({
        "kind": kind,
        "blockedBy": blocked_by,
        "blockedByUnits": units,
        "signature": signature,
        "attempts": attempts,
        "detail": detail,
        "at": now,
        "step": step,
        "auto": false,
        "resumeStatus": status,
        "seq": seq,
    });
    entry["status"] = json!("parked");
    entry["updatedAt"] = json!(now);
    if claimed {
        take_claim(state, &key);
    } else if counted {
        state["currentStage"] = Value::Null;
    }
    append_park_report(
        state,
        &step,
        &key,
        detail,
        json!({"kind": kind, "blockedBy": blocked_by, "auto": false}),
        now,
    );
    Ok(Parked {
        unit: key,
        stage: step,
        kind: kind.to_owned(),
        blocked_by,
        blocked_by_units: units,
        signature,
        attempts,
        detail: detail.to_owned(),
        auto: false,
        count: None,
        warnings,
    })
}

/// Append the report of a park to its stage row (created when missing) and stamp the
/// unit's stage entry.
fn append_park_report(
    state: &mut Value,
    stage: &str,
    key: &str,
    detail: &str,
    parked: Value,
    now: &str,
) {
    if !state["stages"].is_object() {
        state["stages"] = json!({});
    }
    if !state["stages"].get(stage).is_some_and(Value::is_object) {
        state["stages"][stage] = json!({"status": "in_progress", "startedAt": now});
    }
    let row = &mut state["stages"][stage];
    let report =
        json!({"at": now, "result": "blocked", "detail": detail, "unit": key, "parked": parked});
    match row.get_mut("reports").and_then(Value::as_array_mut) {
        Some(reports) => reports.push(report),
        None => row["reports"] = json!([report]),
    }
    if !row["units"].is_object() {
        row["units"] = json!({});
    }
    row["units"][key] = json!({"status": "blocked", "reportedAt": now});
    row["reportedAt"] = json!(now);
    row["detail"] = json!(detail);
}

/// Auto-park after a blocked report: when the unit's same-signature streak reached
/// `threshold`, park it as an `owner-decision` (`auto: true`). Blocked reports with
/// `--needs` never count, so they never auto-park.
pub fn auto_park(
    state: &mut Value,
    stage: &str,
    key: &str,
    threshold: u64,
    last_detail: &str,
    now: &str,
) -> Option<Parked> {
    let entry = unit(state, key)?;
    if status_of(entry) != Some("blocked") {
        return None;
    }
    let count = entry
        .pointer("/blockedStreak/count")
        .and_then(Value::as_u64)?;
    if count < threshold.max(1) {
        return None;
    }
    let attempts = entry
        .pointer(&format!("/attempts/{stage}"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let detail = format!(
        "Auto-parked after {count} consecutive blocked reports with the same blocker on {stage}: {last_detail}"
    );
    let signature = blocked_signature(stage, key, "owner-decision", &[], &[]);
    let seq = progress_seq(state);
    let entry = unit_mut(state, key)?;
    entry["park"] = json!({
        "kind": "owner-decision",
        "blockedBy": [],
        "blockedByUnits": [],
        "signature": signature,
        "attempts": attempts,
        "detail": detail,
        "at": now,
        "step": stage,
        "auto": true,
        "resumeStatus": "blocked",
        "seq": seq,
    });
    entry["status"] = json!("parked");
    entry["updatedAt"] = json!(now);
    Some(Parked {
        unit: key.to_owned(),
        stage: stage.to_owned(),
        kind: "owner-decision".into(),
        blocked_by: Vec::new(),
        blocked_by_units: Vec::new(),
        signature,
        attempts,
        detail,
        auto: true,
        count: Some(count),
        warnings: Vec::new(),
    })
}

/// Close the open park of `key`: the record moves to `parkHistory`, the unit gets its
/// `resumeStatus` back and its blocked streak is cleared. Returns the `unit.revisited`
/// audit details (without `intentId`), or `None` when the unit is not parked.
pub fn close_park(
    state: &mut Value,
    key: &str,
    reason: &str,
    detail: Option<&str>,
    decided_by: Option<&str>,
    now: &str,
) -> Option<Value> {
    let entry = unit_mut(state, key)?;
    let mut record = entry.get("park").filter(|park| park.is_object())?.clone();
    let kind = record.get("kind").cloned().unwrap_or(Value::Null);
    record["revisitedAt"] = json!(now);
    record["revisitReason"] = json!(reason);
    record["revisitDetail"] = json!(detail);
    record["decidedBy"] = json!(decided_by);
    let resume = str_of(&record, "resumeStatus")
        .filter(|status| matches!(*status, "pending" | "red" | "blocked"))
        .unwrap_or("pending")
        .to_owned();
    match entry.get_mut("parkHistory").and_then(Value::as_array_mut) {
        Some(history) => history.push(record.clone()),
        None => entry["parkHistory"] = json!([record.clone()]),
    }
    entry["park"] = Value::Null;
    entry["status"] = json!(resume);
    entry["blockedStreak"] = Value::Null;
    entry["updatedAt"] = json!(now);
    let mut row = json!({
        "unit": key,
        "kind": kind,
        "reason": reason,
        "detail": detail,
    });
    if let Some(by) = decided_by {
        row["decidedBy"] = json!(by);
    }
    Some(row)
}

/// The parked `dep-unit` / `foreign-regression` units whose blockers are all done
/// (satisfied or removed), in order.
pub fn revisitable_units(state: &Value) -> Vec<String> {
    active_units(state)
        .into_iter()
        .filter(|(_, unit)| {
            open_park(unit).is_some_and(|park| {
                let blockers = strings(park.get("blockedByUnits"));
                waits_on_units(str_of(park, "kind").unwrap_or(""))
                    && !blockers.is_empty()
                    && blockers
                        .iter()
                        .all(|blocker| no_longer_blocks(state, blocker))
            })
        })
        .map(|(key, _)| key)
        .collect()
}

/// Revisit every parked unit whose blockers are done (`revisitReason: "blocker-done"`).
pub fn auto_revisits(state: &mut Value, now: &str) -> Vec<Value> {
    revisitable_units(state)
        .into_iter()
        .filter_map(|key| close_park(state, &key, "blocker-done", None, None, now))
        .collect()
}

/// The first parked unit drain may revisit: a `dep-unit` / `foreign-regression` park
/// made before the last progress event (`park.seq < progressSeq`).
fn drain_candidate(state: &Value) -> Option<String> {
    let seq = progress_seq(state);
    active_units(state)
        .into_iter()
        .find(|(_, unit)| {
            open_park(unit).is_some_and(|park| {
                waits_on_units(str_of(park, "kind").unwrap_or(""))
                    && park.get("seq").and_then(Value::as_u64).unwrap_or(0) < seq
            })
        })
        .map(|(key, _)| key)
}

/// The selector of a ledger intent (section 4.2 of the design): automatic revisits of
/// parked units whose blockers are done, the first ready unit in order, drain revisits
/// when nothing is ready, then the sweep, a stall or the stages after the span. Returns
/// the selection and the `unit.revisited` details of every revisit it made.
pub fn select(state: &mut Value, stages: &[Value], now: &str) -> (Selection, Vec<Value>) {
    let reached = span_reached(state, stages);
    let mut revisits = Vec::new();
    if reached {
        revisits.extend(auto_revisits(state, now));
    }
    loop {
        let selection = peek(state, stages);
        if reached && matches!(selection, Selection::Stalled { .. }) {
            if let Some(key) = drain_candidate(state) {
                if let Some(row) = close_park(state, &key, "drain", None, None, now) {
                    revisits.push(row);
                    continue;
                }
            }
        }
        return (selection, revisits);
    }
}

/// The revisit a unit's next step resumes from: its last closed park, when no report
/// was made for the unit since.
pub fn pending_revisit(state: &Value, key: &str) -> Option<Value> {
    let entry = unit(state, key)?;
    let record = entry.get("parkHistory").and_then(Value::as_array)?.last()?;
    let revisited = str_of(record, "revisitedAt")?;
    let last_report = entry
        .get("history")
        .and_then(Value::as_array)
        .and_then(|history| history.last())
        .and_then(|row| str_of(row, "at"));
    if last_report.is_some_and(|at| at > revisited) {
        return None;
    }
    Some(json!({
        "kind": record.get("kind").cloned().unwrap_or(Value::Null),
        "blockedBy": record.get("blockedBy").cloned().unwrap_or_else(|| json!([])),
        "detail": record.get("detail").cloned().unwrap_or(Value::Null),
        "parkedAt": record.get("at").cloned().unwrap_or(Value::Null),
        "reason": record.get("revisitReason").cloned().unwrap_or(Value::Null),
        "decision": record.get("revisitDetail").cloned().unwrap_or(Value::Null),
        "decidedBy": record.get("decidedBy").cloned().unwrap_or(Value::Null),
    }))
}

fn park_row(key: &str, park: &Value) -> Value {
    json!({
        "unit": key,
        "kind": park.get("kind").cloned().unwrap_or(Value::Null),
        "blockedBy": park.get("blockedBy").cloned().unwrap_or_else(|| json!([])),
        "blockedByUnits": park.get("blockedByUnits").cloned().unwrap_or_else(|| json!([])),
        "detail": park.get("detail").cloned().unwrap_or(Value::Null),
        "at": park.get("at").cloned().unwrap_or(Value::Null),
        "auto": park.get("auto").cloned().unwrap_or(Value::Bool(false)),
    })
}

/// How a parked unit comes back, in words.
fn revisit_route(key: &str, park: &Value) -> String {
    let blockers = strings(park.get("blockedByUnits"));
    match str_of(park, "kind").unwrap_or("") {
        kind if waits_on_units(kind) => format!("auto when {} done", {
            if blockers.len() == 1 {
                format!("{} is", blockers[0])
            } else {
                format!("{} are", blockers.join(", "))
            }
        }),
        "design-defect" => format!("dtech-aidlc intent rework --unit {key} --detail \"<reason>\""),
        _ => format!("dtech-aidlc orchestrate revisit --unit {key} --detail \"<decision>\""),
    }
}

/// The hold payload of an `orchestrate next` that cannot select any work unit.
pub fn stalled_payload(
    state: &Value,
    intent_id: &str,
    parked: &[String],
    waiting: &[(String, Vec<String>)],
) -> Value {
    let mut rows = Vec::new();
    let mut hints = Vec::new();
    for key in parked {
        let Some(park) = unit(state, key).and_then(open_park) else {
            continue;
        };
        rows.push(park_row(key, park));
        let blockers = strings(park.get("blockedByUnits"));
        hints.push(match str_of(park, "kind").unwrap_or("") {
            "owner-decision" => format!("{key} waits for an owner decision: record it, then run 'dtech-aidlc orchestrate revisit --unit {key} --detail \"<decision>\"'."),
            "design-defect" => format!("{key} has a design defect: run 'dtech-aidlc intent rework --unit {key} --detail \"<reason>\"'."),
            _ => format!(
                "{key} waits on {}, which cannot run; revisit {key} with 'dtech-aidlc orchestrate revisit --unit {key}' or resolve {} first.",
                blockers.join(", "),
                blockers.join(", ")
            ),
        });
    }
    let waiting: Vec<Value> = waiting
        .iter()
        .map(|(key, on)| json!({"unit": key, "waitingOn": on}))
        .collect();
    json!({
        "complete": false,
        "blocked": true,
        "reason": "queue-stalled",
        "intentId": intent_id,
        "parked": rows,
        "waiting": waiting,
        "hint": format!("No work unit can run. {}", hints.join(" ")),
    })
}

/// Closed `owner-decision` parks with their decision; `confirmed` is false when the
/// AI orchestrator recorded the decision.
pub fn owner_decisions(state: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    for (key, unit) in active_units(state) {
        for record in unit
            .get("parkHistory")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if str_of(record, "kind") != Some("owner-decision")
                || str_of(record, "revisitReason") != Some("manual")
            {
                continue;
            }
            let by = str_of(record, "decidedBy");
            rows.push(json!({
                "unit": key,
                "decision": record.get("revisitDetail").cloned().unwrap_or(Value::Null),
                "at": record.get("revisitedAt").cloned().unwrap_or(Value::Null),
                "decidedBy": by,
                "confirmed": by.is_some_and(|by| by != crate::orchestrate::AI_ORCHESTRATOR),
                "parkedFor": record.get("detail").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    rows
}

/// The results a 1.0.0 whole-intent rework had reset before the migration (each unit's
/// `priorResults`), in order: `{unit, stage, result, at}`. Information only; they may
/// name the wrong unit and never restore a status.
pub fn prior_results(state: &Value) -> Vec<Value> {
    active_units(state)
        .into_iter()
        .flat_map(|(key, unit)| {
            unit.get("priorResults")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(move |row| {
                    json!({
                        "unit": key,
                        "stage": row.get("stage").cloned().unwrap_or(Value::Null),
                        "result": row.get("result").cloned().unwrap_or(Value::Null),
                        "at": row.get("at").cloned().unwrap_or(Value::Null),
                    })
                })
        })
        .collect()
}

/// The `orchestrate queue` view: ready, waiting, blocked, parked and done units.
pub fn queue_view(state: &Value, stages: &[Value], threshold: u64) -> Value {
    let span = span(stages);
    let units = active_units(state);
    let step_name = |unit: &Value| {
        next_step(unit, &span.repeating)
            .map(|stage| json!(stage_id(stage)))
            .unwrap_or(Value::Null)
    };
    let mut ready_rows = Vec::new();
    let mut waiting_rows = Vec::new();
    let mut blocked_rows = Vec::new();
    let mut parked_rows = Vec::new();
    let mut done_rows = Vec::new();
    for (key, unit) in &units {
        let status = status_of(unit).unwrap_or("pending");
        if satisfied(Some(status)) {
            done_rows.push(json!({"unit": key, "status": status}));
            continue;
        }
        if let Some(park) = open_park(unit) {
            let mut row = park_row(key, park);
            row["revisit"] = json!(revisit_route(key, park));
            parked_rows.push(row);
            continue;
        }
        let waiting = waiting_on(state, unit);
        if !waiting.is_empty() {
            waiting_rows.push(json!({"unit": key, "waitingOn": waiting}));
            continue;
        }
        let step = step_name(unit);
        ready_rows.push(json!({"unit": key, "step": step, "status": status}));
        if status == "blocked" {
            let attempts = step
                .as_str()
                .and_then(|step| unit.pointer(&format!("/attempts/{step}")))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            blocked_rows.push(json!({
                "unit": key,
                "step": step,
                "attempts": attempts,
                "blockedStreak": unit.pointer("/blockedStreak/count").and_then(Value::as_u64).unwrap_or(0),
                "autoParkThreshold": threshold,
            }));
        }
    }
    let deferred: Vec<Value> = ledger(state)
        .and_then(|ledger| ledger.get("deferredRegressions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut by_owner: BTreeMap<String, u64> = BTreeMap::new();
    let mut open = 0;
    let mut unowned = 0;
    for item in deferred
        .iter()
        .filter(|item| str_of(item, "status") == Some("open"))
    {
        open += 1;
        match str_of(item, "owner").filter(|_| item.get("unowned") != Some(&Value::Bool(true))) {
            Some(owner) => *by_owner.entry(owner.to_owned()).or_default() += 1,
            None => unowned += 1,
        }
    }
    json!({
        "intentId": state.get("intentId").cloned().unwrap_or(Value::Null),
        "currentStage": state.get("currentStage").cloned().unwrap_or(Value::Null),
        "currentUnit": state.get("currentUnit").cloned().unwrap_or(Value::Null),
        "ready": ready_rows,
        "waiting": waiting_rows,
        "blocked": blocked_rows,
        "parked": parked_rows,
        "done": done_rows,
        "sweep": {"status": sweep_status(state)},
        "deferredRegressions": {"open": open, "byOwner": by_owner, "unowned": unowned},
        "ownerDecisions": owner_decisions(state),
        "priorResults": prior_results(state),
    })
}

// ---------------------------------------------------------------------------
// Waves and doctor early warnings (section 14 of the design)
// ---------------------------------------------------------------------------

/// Suggested waves of at most `threshold` units (section 14 of the design).
///
/// `level(u)` is 0 for a unit without (active) dependencies, else one more than the
/// highest level of its dependencies. Whole levels are added to the current wave while
/// it stays within the threshold; a level that does not fit starts a new wave, and a
/// level larger than the threshold is cut into chunks of `threshold` units (units of
/// one level never depend on each other). Each wave keeps the ledger `order`.
pub fn waves(state: &Value, threshold: usize) -> Vec<Vec<String>> {
    let threshold = threshold.max(1);
    let units = active_units(state);
    let keys: Vec<String> = units.iter().map(|(key, _)| key.clone()).collect();
    let mut levels: BTreeMap<String, usize> = BTreeMap::new();
    // Dependencies are acyclic (validated when the ledger is built); the pass count
    // bounds the loop anyway, so a hand-edited cycle cannot hang doctor.
    for _ in 0..=units.len() {
        let mut changed = false;
        for (key, unit) in &units {
            let level = dependency_keys(state, unit)
                .iter()
                .filter(|dep| keys.contains(dep))
                .map(|dep| levels.get(dep).copied().unwrap_or(0) + 1)
                .max()
                .unwrap_or(0);
            if levels.get(key) != Some(&level) {
                levels.insert(key.clone(), level);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let top = levels.values().copied().max().unwrap_or(0);
    let mut waves: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for level in 0..=top {
        let members: Vec<String> = keys
            .iter()
            .filter(|key| levels.get(*key) == Some(&level))
            .cloned()
            .collect();
        if members.is_empty() {
            continue;
        }
        if current.len() + members.len() <= threshold {
            current.extend(members);
            continue;
        }
        if !current.is_empty() {
            waves.push(std::mem::take(&mut current));
        }
        let mut chunks: Vec<Vec<String>> =
            members.chunks(threshold).map(<[String]>::to_vec).collect();
        // The last (possibly partial) chunk stays open for the next levels.
        current = chunks.pop().unwrap_or_default();
        waves.extend(chunks);
    }
    if !current.is_empty() {
        waves.push(current);
    }
    let position = |key: &String| keys.iter().position(|k| k == key).unwrap_or(usize::MAX);
    for wave in &mut waves {
        wave.sort_by_key(position);
    }
    waves
}

/// `1) WU-01, WU-02; 2) WU-03` for the waves of `state`.
pub fn waves_text(state: &Value, threshold: usize) -> String {
    waves(state, threshold)
        .iter()
        .enumerate()
        .map(|(index, wave)| format!("{}) {}", index + 1, wave.join(", ")))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The directive warning that accompanies the unchanged split-threshold warning.
pub fn wave_warning(state: &Value, threshold: usize) -> String {
    format!(
        "Suggested waves of at most {threshold} units: {}. The CLI runs units in dependency order either way; the owner may split the intent along these waves, or review the work wave by wave.",
        waves_text(state, threshold)
    )
}

/// The first finished step of a unit (`history`, then migrated `priorResults`):
/// `(stage, at)`.
fn first_finished_step(unit: &Value) -> Option<(String, String)> {
    ["history", "priorResults"]
        .iter()
        .flat_map(|key| {
            unit.get(*key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|row| matches!(str_of(row, "result"), Some("complete" | "skipped")))
        .filter_map(|row| {
            Some((
                str_of(row, "stage")?.to_owned(),
                str_of(row, "at").unwrap_or("").to_owned(),
            ))
        })
        .min_by(|a, b| a.1.cmp(&b.1))
}

/// The TS ids registered in the intent's `ids.json`, as written there.
fn registered_test_ids(root: &Path, graph: &Value, state: &Value) -> Option<Vec<String>> {
    let paths = design_artifacts(root, graph, state).ok()?;
    let bytes = read_regular(paths.registry.as_deref()?).ok()??;
    let registry: Value = serde_json::from_slice(&bytes).ok()?;
    Some(
        registry
            .pointer("/ids/TS")?
            .as_array()?
            .iter()
            .filter_map(|entry| str_of(entry, "id"))
            .map(str::to_owned)
            .collect(),
    )
}

fn listed(items: &[String], limit: usize) -> String {
    let mut shown: Vec<String> = items.iter().take(limit).cloned().collect();
    if items.len() > limit {
        shown.push(format!("… and {} more", items.len() - limit));
    }
    shown.join(", ")
}

/// Why a stored ledger is stale: the units an older dtech-aidlc reported, a design
/// rework or a release-candidate gate it recorded (from a re-sync `change`).
pub fn stale_warning(change: &LedgerChange) -> Option<String> {
    let LedgerChange::Resynced {
        units,
        legacy_rework,
        legacy_gate,
        ..
    } = change
    else {
        return None;
    };
    let mut subjects = Vec::new();
    if !units.is_empty() {
        subjects.push(format!("stage records for {}", listed(units, 10)));
    }
    if *legacy_rework {
        subjects.push("a design rework".to_owned());
    }
    if *legacy_gate {
        subjects.push("a release-candidate gate decision".to_owned());
    }
    if subjects.is_empty() {
        return None;
    }
    Some(format!(
        "work-unit ledger is older than {} (written by an older dtech-aidlc); it is re-synced by the next orchestrate command",
        subjects.join(" and ")
    ))
}

/// Doctor's early warnings about the work units of a ledger (or a ledger preview):
/// dependency order and inversions, test ownership, waves above the split threshold,
/// open parks and wait cycles, and owner decisions awaiting confirmation. Messages
/// have no `Intent X:` prefix; doctor adds it.
pub fn doctor_warnings(
    root: &Path,
    graph: &Value,
    state: &Value,
    split_threshold: usize,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let units = active_units(state);
    let keys: Vec<String> = units.iter().map(|(key, _)| key.clone()).collect();

    // Array order that contradicts dependsOn.
    for (index, (key, unit)) in units.iter().enumerate() {
        for dep in dependency_keys(state, unit) {
            if keys
                .iter()
                .position(|k| *k == dep)
                .is_some_and(|at| at > index)
            {
                warnings.push(format!(
                    "{key} is listed before its dependency {dep} in work-units.json"
                ));
            }
        }
    }

    // Inverted dependency: the dependant already finished work, the dependency none.
    for (key, unit) in &units {
        let Some((stage, at)) = first_finished_step(unit) else {
            continue;
        };
        for dep in dependency_keys(state, unit) {
            let Some(target) = self::unit(state, &dep).filter(|target| !removed(target)) else {
                continue;
            };
            if first_finished_step(target).is_none() && !satisfied(status_of(target)) {
                let when = if at.is_empty() {
                    stage.clone()
                } else {
                    format!("{stage} {at}")
                };
                warnings.push(format!(
                    "{key} dependsOn {dep}, but {key} already has finished work ({when}) and {dep} has none; the dependency may be inverted. Prefer attributed regression (deferred failures in regression.json) to reordering."
                ));
            }
        }
    }

    // Test ownership.
    let owners = crate::regression::ownership(state);
    if owners.is_empty() {
        if units.len() >= 2 {
            warnings.push("no test ownership is declared (work-units.json \"tests\" or ids.json TS traces to WU ids), so failing tests cannot be attributed to work units: a failure without an explicit owner in regression.json is charged to the unit that reports it".to_owned());
        }
    } else if let Some(registered) = registered_test_ids(root, graph, state) {
        let mut unowned: Vec<String> = registered
            .into_iter()
            .filter(|id| {
                crate::regression::ts_number(id).is_some_and(|number| !owners.contains_key(&number))
            })
            .collect();
        unowned.sort_by(|a, b| id_order(a, b));
        unowned.dedup();
        if !unowned.is_empty() {
            warnings.push(format!(
                "{} registered test(s) are not owned by any work unit: {} (add them to \"tests\" in work-units.json)",
                unowned.len(),
                listed(&unowned, 20)
            ));
        }
    }

    // Waves above the split threshold, while Develop still runs.
    if units.len() > split_threshold && sweep_status(state) != "green" {
        warnings.push(format!(
            "{} work units exceed the split threshold {split_threshold}. Suggested waves: {}. Or split the intent.",
            units.len(),
            waves_text(state, split_threshold)
        ));
    }

    // Open parks, and park or wait cycles.
    for (key, unit) in &units {
        let Some(park) = open_park(unit) else {
            continue;
        };
        let kind = str_of(park, "kind").unwrap_or("unclassified");
        let mut line = format!("{key} is parked ({kind})");
        if let Some(at) = str_of(park, "at") {
            line.push_str(&format!(" since {at}"));
        }
        if let Some(detail) = str_of(park, "detail").filter(|detail| !detail.is_empty()) {
            line.push_str(&format!(": {detail}"));
        }
        warnings.push(line);
        for blocker in strings(park.get("blockedByUnits")) {
            if blocker == *key || no_longer_blocks(state, &blocker) {
                continue;
            }
            if depends_transitively(state, &blocker, key) {
                warnings.push(format!(
                    "parked {key} waits on {blocker}, which depends on {key}"
                ));
            } else if waits_through(state, &blocker, key) {
                warnings.push(format!(
                    "parked {key} waits on {blocker}, which waits on {key}"
                ));
            }
        }
    }

    // Results reset by a 1.0.0 whole-intent rework before the migration (D16).
    let prior: Vec<String> = prior_results(state)
        .iter()
        .map(|row| {
            [
                str_of(row, "unit"),
                str_of(row, "stage"),
                str_of(row, "result"),
                str_of(row, "at"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
        })
        .collect();
    if !prior.is_empty() && sweep_status(state) != "green" {
        warnings.push(format!(
            "{} results from before a 1.0.0 whole-intent rework are kept as priorResults (information only: they may name the wrong unit and restore nothing, so those steps run again): {}",
            prior.len(),
            listed(&prior, 20)
        ));
    }

    // Owner decisions recorded by the AI orchestrator.
    let unconfirmed: Vec<String> = owner_decisions(state)
        .iter()
        .filter(|row| row.get("confirmed") == Some(&Value::Bool(false)))
        .map(|row| {
            let unit = str_of(row, "unit").unwrap_or("");
            match str_of(row, "decision") {
                Some(decision) => format!("{unit} ({decision})"),
                None => unit.to_owned(),
            }
        })
        .collect();
    if !unconfirmed.is_empty() {
        warnings.push(format!(
            "{} owner decision(s) were recorded by {} and await owner confirmation: {}",
            unconfirmed.len(),
            crate::orchestrate::AI_ORCHESTRATOR,
            listed(&unconfirmed, 10)
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_keys_at_every_level() {
        let value = json!({"b": 1, "a": {"d": [2, {"z": 1, "y": 2}], "c": null}});
        assert_eq!(
            canonical_json(&value),
            r#"{"a":{"c":null,"d":[2,{"y":2,"z":1}]},"b":1}"#
        );
    }

    #[test]
    fn unit_section_drops_status_items_and_matches_legacy_ids() {
        let text = "# Units\r\n\r\n## U-01 First  \r\n- **Status:** red\r\n- Goal: x\r\n* Status (2026): done\r\n- Statuses are fine\r\n## WU-02 Second\r\nbody\r\n";
        assert_eq!(
            unit_section(text, "WU-01"),
            "## U-01 First\n- Goal: x\n- Statuses are fine"
        );
        assert_eq!(unit_section(text, "WU-02"), "## WU-02 Second\nbody\n");
        assert_eq!(unit_section(text, "WU-03"), "");
        assert_eq!(unit_section("### WU-01 deep\n", "WU-01"), "");
    }

    #[test]
    fn signature_ignores_need_order_and_unit_spelling() {
        let a = blocked_signature(
            "red-tests",
            "U-01",
            "unclassified",
            &["b".into(), "a".into()],
            &[],
        );
        let b = blocked_signature(
            "red-tests",
            "WU-01",
            "unclassified",
            &["a".into(), "b".into()],
            &[],
        );
        assert_eq!(a, b);
        assert_ne!(
            a,
            blocked_signature("build-and-verify", "WU-01", "unclassified", &[], &[])
        );
    }

    #[test]
    fn threshold_falls_back_to_three() {
        assert_eq!(auto_park_threshold(&json!({})), 3);
        assert_eq!(auto_park_threshold(&json!({"autoParkThreshold": 5})), 5);
        for invalid in [json!(0), json!(-1), json!(1.5), json!("4")] {
            assert_eq!(
                auto_park_threshold(&json!({"autoParkThreshold": invalid})),
                3
            );
        }
    }

    fn stages() -> Vec<Value> {
        vec![
            json!({"id": "design", "phase": "develop"}),
            json!({"id": "red", "phase": "develop", "repeat": "work-unit"}),
            json!({"id": "build", "phase": "develop", "repeat": "work-unit"}),
            json!({"id": "release", "phase": "launch"}),
        ]
    }

    fn input(rows: Value) -> WorkUnitsInput {
        WorkUnitsInput {
            rel_path: "w.json".into(),
            sha256: "x".into(),
            registry_sha256: None,
            registry: None,
            markdown: None,
            units: rows
                .as_array()
                .unwrap()
                .iter()
                .map(|row| UnitRow {
                    id: row["id"].as_str().unwrap().into(),
                    title: Value::Null,
                    depends_on: row["dependsOn"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|dep| dep.as_str().unwrap().to_owned())
                        .collect(),
                    tests: Vec::new(),
                    row: row.clone(),
                })
                .collect(),
        }
    }

    #[test]
    fn selector_follows_dependencies_then_array_order_and_ends_with_the_sweep() {
        let stages = stages();
        let mut state = json!({"stages": {"design": {"status": "complete"}}});
        let rows = json!([
            {"id": "WU-02", "dependsOn": ["WU-03"]},
            {"id": "WU-01"},
            {"id": "WU-03"},
        ]);
        let (ledger, change) = build_ledger(&input(rows), &state, &stages, "t", false, None);
        assert_eq!(change, LedgerChange::Created { units: 3 });
        state[LEDGER_KEY] = ledger;
        let mut picked = Vec::new();
        loop {
            match peek(&state, &stages) {
                Selection::Unit { stage, unit } => {
                    picked.push(format!("{unit}:{}", stage["id"].as_str().unwrap()));
                    let last = stage["id"] == "build";
                    record_unit_report(
                        &mut state,
                        stage["id"].as_str().unwrap(),
                        &unit,
                        "complete",
                        &[],
                        last,
                        "t",
                    );
                }
                Selection::Sweep(stage) => {
                    assert_eq!(stage["id"], "build");
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(
            picked,
            [
                "WU-01:red",
                "WU-01:build",
                "WU-03:red",
                "WU-03:build",
                "WU-02:red",
                "WU-02:build"
            ]
        );
        assert_eq!(state[LEDGER_KEY]["progressSeq"], 3);
    }

    #[test]
    fn mirrors_project_the_release_gate_onto_the_last_unit() {
        let stages = stages();
        let mut state = json!({"stages": {
            "design": {"status": "complete"},
            "red": {"status": "in_progress", "units": {}},
            "build": {"status": "in_progress", "units": {}},
        }});
        let (ledger, _) = build_ledger(
            &input(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
            &state,
            &stages,
            "t",
            false,
            None,
        );
        state[LEDGER_KEY] = ledger;
        for unit in ["WU-01", "WU-02"] {
            record_unit_report(&mut state, "red", unit, "complete", &[], false, "t");
            record_unit_report(&mut state, "build", unit, "complete", &[], true, "t");
            state["stages"]["build"]["units"][unit] =
                json!({"status": "complete", "reportedAt": "t"});
        }
        sync_mirrors(&mut state, &stages, "t2");
        assert_eq!(state["stages"]["red"]["status"], "complete");
        assert_eq!(state["stages"]["build"]["status"], "in_progress");
        assert_eq!(
            state["stages"]["build"]["units"]["WU-01"]["status"],
            "complete"
        );
        assert_eq!(
            state["stages"]["build"]["units"]["WU-02"]["status"],
            "pending"
        );
        assert_eq!(
            state["stages"]["build"]["units"]["WU-02"]["reportedAt"],
            "t"
        );
        assert_eq!(state["units"][1]["status"], "in_progress");
        assert_eq!(state["units"][0]["status"], "complete");
        record_sweep_report(&mut state, "complete", "t3");
        sync_mirrors(&mut state, &stages, "t3");
        assert_eq!(state["stages"]["build"]["status"], "complete");
        assert_eq!(
            state["stages"]["build"]["units"]["WU-02"]["status"],
            "complete"
        );
        assert_eq!(state["units"][1]["status"], "complete");
        assert_eq!(state[LEDGER_KEY]["units"]["WU-02"]["status"], "done");
        assert_eq!(peek(&state, &stages), Selection::Stage(stages[3].clone()));
    }

    #[test]
    fn plain_identifiers_follow_the_decision_id_pattern() {
        for good in ["CFG-17", "RK-16", "Q-04", "ADR-1.2", "a-b"] {
            assert!(plain_identifier(good), "{good}");
        }
        let long = format!("X-{}", "1".repeat(33));
        for bad in [
            "17",
            "-1",
            "CFG-",
            "1A-2",
            "CFG-1-2",
            "CFG_1",
            "not an id",
            long.as_str(),
        ] {
            assert!(!plain_identifier(bad), "{bad}");
        }
    }

    /// A ledger of `rows` with the design stage complete and the red/build rows started.
    fn ledger_state(rows: Value) -> (Vec<Value>, Value) {
        let stages = stages();
        let mut state = json!({"stages": {
            "design": {"status": "complete"},
            "red": {"status": "in_progress", "units": {}},
        }});
        let (ledger, _) = build_ledger(&input(rows), &state, &stages, "t", false, None);
        state[LEDGER_KEY] = ledger;
        (stages, state)
    }

    fn finish(state: &mut Value, unit: &str) {
        record_unit_report(state, "red", unit, "complete", &[], false, "t");
        record_unit_report(state, "build", unit, "complete", &[], true, "t");
    }

    fn request<'a>(unit: &'a str, kind: &'a str, blocked_by: &'a [String]) -> ParkRequest<'a> {
        ParkRequest {
            unit: Some(unit),
            kind,
            blocked_by,
            detail: "why",
        }
    }

    #[test]
    fn select_revisits_parks_whose_blockers_are_done_and_drains_once_per_progress() {
        let (stages, mut state) =
            ledger_state(json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}]));
        let on_two = vec!["WU-02".to_owned()];
        let parked = park(
            &mut state,
            &stages,
            &request("WU-01", "dep-unit", &on_two),
            "t1",
        )
        .unwrap();
        assert_eq!(parked.blocked_by_units, ["WU-02"]);
        assert_eq!(parked.attempts, 0);
        assert_eq!(state[LEDGER_KEY]["units"]["WU-01"]["park"]["seq"], 0);
        // WU-02 is ready: no revisit, WU-02 is selected.
        let (selection, revisits) = select(&mut state, &stages, "t2");
        assert!(revisits.is_empty());
        assert!(matches!(selection, Selection::Unit { ref unit, .. } if unit == "WU-02"));
        // WU-02 finished: WU-01 comes back automatically.
        finish(&mut state, "WU-02");
        let (selection, revisits) = select(&mut state, &stages, "t3");
        assert_eq!(revisits.len(), 1);
        assert_eq!(revisits[0]["reason"], "blocker-done");
        assert!(matches!(selection, Selection::Unit { ref unit, .. } if unit == "WU-01"));
        assert_eq!(state[LEDGER_KEY]["units"]["WU-01"]["status"], "pending");

        // Drain: WU-01 waits on WU-03, which is parked for an owner decision.
        let (stages, mut state) =
            ledger_state(json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}]));
        let on_three = vec!["WU-03".to_owned()];
        park(
            &mut state,
            &stages,
            &request("WU-01", "dep-unit", &on_three),
            "t1",
        )
        .unwrap();
        park(
            &mut state,
            &stages,
            &request("WU-03", "owner-decision", &[]),
            "t1",
        )
        .unwrap();
        finish(&mut state, "WU-02");
        let (selection, revisits) = select(&mut state, &stages, "t2");
        assert_eq!(revisits.len(), 1);
        assert_eq!(revisits[0]["reason"], "drain");
        assert!(matches!(selection, Selection::Unit { ref unit, .. } if unit == "WU-01"));
        // Parked again with no further progress: a stall, no second drain.
        park(
            &mut state,
            &stages,
            &request("WU-01", "dep-unit", &on_three),
            "t3",
        )
        .unwrap();
        let (selection, revisits) = select(&mut state, &stages, "t4");
        assert!(revisits.is_empty());
        assert!(
            matches!(selection, Selection::Stalled { ref parked, .. } if parked == &["WU-01", "WU-03"])
        );
    }

    #[test]
    fn auto_park_needs_the_threshold_and_a_plain_blocked_unit() {
        let (_, mut state) = ledger_state(json!([{"id": "WU-01"}]));
        for count in 1..=3u64 {
            record_unit_report(&mut state, "red", "WU-01", "blocked", &[], false, "t");
            let parked = auto_park(&mut state, "red", "WU-01", 3, "same", "t");
            assert_eq!(parked.is_some(), count == 3, "{count}");
        }
        let unit = &state[LEDGER_KEY]["units"]["WU-01"];
        assert_eq!(unit["status"], "parked");
        assert_eq!(unit["park"]["auto"], true);
        assert_eq!(unit["park"]["resumeStatus"], "blocked");
        assert_eq!(
            close_park(
                &mut state,
                "WU-01",
                "manual",
                Some("ok"),
                Some("Owner"),
                "t2"
            )
            .unwrap()["decidedBy"],
            "Owner"
        );
        let unit = &state[LEDGER_KEY]["units"]["WU-01"];
        assert_eq!(unit["status"], "blocked");
        assert!(unit["blockedStreak"].is_null());
        assert!(close_park(&mut state, "WU-01", "manual", None, None, "t3").is_none());
    }

    #[test]
    fn migration_maps_entries_prior_results_and_ignored_keys() {
        let stages = stages();
        let state = json!({
            "stages": {
                "design": {"status": "complete"},
                "red": {"status": "in_progress", "units": {
                    "_": {"status": "pending", "reportedAt": "a"},
                    "WU-01": {"status": "complete", "reportedAt": "b"},
                    "WU-02": {"status": "pending", "reportedAt": "c"},
                    "WU-03": {"status": "blocked", "reportedAt": "d"},
                }, "reports": [
                    {"at": "a", "result": "blocked"},
                    {"at": "b", "result": "complete"},
                    {"at": "c", "result": "skipped"},
                    {"at": "d", "result": "blocked"},
                ]},
                "build": {"status": "in_progress", "units": {"WU-01": {"status": "blocked", "reportedAt": "e"}}, "reports": [{"at": "e", "result": "blocked"}]},
            },
            "units": [{"id": "WU-01", "status": "in_progress"}, {"id": "WU-02", "status": "in_progress"}, {"id": "WU-03", "status": "in_progress"}, {"id": "WU-04", "status": "complete"}],
        });
        let rows = json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}, {"id": "WU-04"}]);
        let (ledger, change) = build_ledger(&input(rows), &state, &stages, "t", true, Some(b"x"));
        assert_eq!(
            change,
            LedgerChange::Migrated {
                units: 4,
                ignored_keys: vec!["_".into()],
                prior_results: 1
            }
        );
        let units = &ledger["units"];
        assert_eq!(units["WU-01"]["status"], "blocked");
        assert_eq!(
            units["WU-01"]["steps"],
            json!({"red": "complete", "build": "blocked"})
        );
        assert_eq!(units["WU-02"]["status"], "pending");
        assert_eq!(
            units["WU-02"]["priorResults"],
            json!([{"stage": "red", "result": "skipped", "at": "c"}])
        );
        assert_eq!(units["WU-03"]["status"], "blocked");
        assert_eq!(units["WU-04"]["status"], "green");
        assert_eq!(ledger["migration"]["preSha256"], sha256_hex(b"x"));
        assert_eq!(ledger["progressSeq"], 1);
    }

    /// A two-unit ledger state synced at `at`, with both repeating rows present.
    fn synced_state(at: &str) -> (Value, Vec<Value>) {
        let stages = stages();
        let mut state = json!({"stages": {
            "design": {"status": "complete"},
            "red": {"status": "in_progress", "units": {}},
            "build": {"status": "in_progress", "units": {}},
        }});
        let rows = json!([{"id": "WU-01"}, {"id": "WU-02"}]);
        let (ledger, _) = build_ledger(&input(rows), &state, &stages, at, false, None);
        state[LEDGER_KEY] = ledger;
        record_unit_report(&mut state, "red", "WU-01", "complete", &[], false, at);
        state["stages"]["red"]["units"]["WU-01"] = json!({"status": "complete", "reportedAt": at});
        sync_mirrors(&mut state, &stages, at);
        (state, stages)
    }

    #[test]
    fn resync_rederives_only_steps_an_older_binary_reported_later() {
        let at = "2026-11-26T01:00:00.000Z";
        let (mut state, stages) = synced_state(at);
        let root = Path::new("does-not-exist");
        let untouched = state.clone();
        // Records written with the ledger's own time are not newer.
        assert_eq!(
            resync(root, &json!({}), &mut state, &stages, "later").unwrap(),
            LedgerChange::None
        );
        assert_eq!(state, untouched);
        // A 1.0.0 report of WU-01's build step (and a reset entry that keeps its old
        // reportedAt, which is not taken literally).
        let newer = "2026-11-26T01:00:00.001Z";
        state["stages"]["build"]["units"]["WU-01"] =
            json!({"status": "complete", "reportedAt": newer});
        state["stages"]["red"]["units"]["WU-01"]["status"] = json!("pending");
        let change = resync(
            root,
            &json!({}),
            &mut state,
            &stages,
            "2026-11-26T02:00:00.000Z",
        )
        .unwrap();
        assert_eq!(
            change,
            LedgerChange::Resynced {
                units: vec!["WU-01".into()],
                legacy_rework: false,
                legacy_gate: false,
                reconciled: None,
            }
        );
        let unit = &state[LEDGER_KEY]["units"]["WU-01"];
        assert_eq!(unit["steps"]["red"], "complete");
        assert_eq!(unit["steps"]["build"], "complete");
        assert_eq!(unit["status"], "green");
        assert_eq!(state[LEDGER_KEY]["progressSeq"], 1);
        let rows = change.audit_rows("i");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "units.resynced");
        assert_eq!(rows[0].1["units"], json!(["WU-01"]));
    }

    #[test]
    fn resync_accepts_a_release_gate_approved_by_an_older_binary() {
        let at = "2026-11-26T01:00:00.000Z";
        let (mut state, stages) = synced_state(at);
        for (key, status) in [("WU-01", "verified"), ("WU-02", "green")] {
            state[LEDGER_KEY]["units"][key]["status"] = json!(status);
        }
        let root = Path::new("does-not-exist");
        // A rejected gate is not a sweep result.
        state["gates"] = json!({"release-candidate": {"decision": "rejected", "at": "2026-11-26T01:30:00.000Z"}});
        assert_eq!(
            resync(root, &json!({}), &mut state, &stages, "t").unwrap(),
            LedgerChange::None
        );
        state["gates"]["release-candidate"]["decision"] = json!("approved");
        let change = resync(root, &json!({}), &mut state, &stages, "t").unwrap();
        assert!(matches!(
            change,
            LedgerChange::Resynced {
                legacy_gate: true,
                ..
            }
        ));
        assert_eq!(state[LEDGER_KEY]["sweep"]["status"], "green");
        assert_eq!(state[LEDGER_KEY]["sweep"]["via"], "1.0.0-gate");
        for key in ["WU-01", "WU-02"] {
            assert_eq!(state[LEDGER_KEY]["units"][key]["status"], "done");
        }
    }

    #[test]
    fn rework_instruction_is_only_given_for_ledger_requests() {
        assert_eq!(rework_instruction(&json!({"detail": "x"})), None);
        assert!(rework_instruction(&json!({"units": []}))
            .unwrap()
            .starts_with("Change only the design that needs it"));
        assert!(rework_instruction(&json!({"units": ["WU-06"]}))
            .unwrap()
            .starts_with("Change only the design of WU-06:"));
        assert!(rework_instruction(
            &json!({"units": ["WU-06"], "split": {"unit": "WU-06", "into": ["WU-17", "WU-18"]}})
        )
        .unwrap()
        .starts_with("Replace WU-06 with WU-17, WU-18"));
    }

    /// A ledger with `units` as `(id, dependsOn)` in order.
    fn graph_state(units: &[(&str, &[&str])]) -> Value {
        let mut map = Map::new();
        for (id, deps) in units {
            map.insert(
                (*id).into(),
                json!({"status": "pending", "dependsOn": deps}),
            );
        }
        let order: Vec<&str> = units.iter().map(|(id, _)| *id).collect();
        json!({LEDGER_KEY: {"order": order, "units": map}})
    }

    #[test]
    fn waves_follow_dependency_levels_within_the_threshold() {
        // The real sixteen-unit graph (WU-06 depends on WU-16, listed before it).
        let state = graph_state(&[
            ("WU-01", &[]),
            ("WU-02", &["WU-01"]),
            ("WU-03", &["WU-02"]),
            ("WU-04", &["WU-02", "WU-03"]),
            ("WU-05", &["WU-03", "WU-04"]),
            ("WU-16", &["WU-05"]),
            ("WU-06", &["WU-03", "WU-04", "WU-16"]),
            ("WU-07", &["WU-03"]),
            ("WU-08", &["WU-06", "WU-16", "WU-07"]),
            ("WU-09", &["WU-08"]),
            ("WU-10", &["WU-09"]),
            ("WU-11", &["WU-01"]),
            ("WU-12", &["WU-08"]),
            ("WU-13", &["WU-12"]),
            ("WU-14", &["WU-13"]),
            ("WU-15", &["WU-10", "WU-11", "WU-14"]),
        ]);
        assert_eq!(
            waves_text(&state, 6),
            "1) WU-01, WU-02, WU-03, WU-04, WU-07, WU-11; 2) WU-05, WU-16, WU-06, WU-08, WU-09, WU-12; 3) WU-10, WU-13, WU-14, WU-15"
        );
        // A removed unit is left out, and a legacy U-01 dependency still resolves.
        let mut state = graph_state(&[("U-01", &[]), ("WU-02", &["WU-01"]), ("WU-03", &[])]);
        state[LEDGER_KEY]["units"]["WU-03"]["removed"] = json!(true);
        assert_eq!(waves(&state, 1), vec![vec!["U-01"], vec!["WU-02"]]);
    }

    #[test]
    fn a_level_larger_than_the_threshold_is_cut_into_chunks() {
        let state = graph_state(&[
            ("WU-01", &[]),
            ("WU-02", &[]),
            ("WU-03", &[]),
            ("WU-04", &["WU-01"]),
        ]);
        assert_eq!(
            waves(&state, 2),
            vec![vec!["WU-01", "WU-02"], vec!["WU-03", "WU-04"]]
        );
        assert_eq!(waves_text(&state, 4), "1) WU-01, WU-02, WU-03, WU-04");
        assert!(waves(&json!({}), 6).is_empty());
        // A hand-edited cycle cannot hang the computation.
        let cycle = graph_state(&[("WU-01", &["WU-02"]), ("WU-02", &["WU-01"])]);
        assert_eq!(waves(&cycle, 6).concat().len(), 2);
    }

    #[test]
    fn stale_warning_names_what_the_older_binary_wrote() {
        assert_eq!(stale_warning(&LedgerChange::None), None);
        let change = LedgerChange::Resynced {
            units: vec!["WU-03".into()],
            legacy_rework: true,
            legacy_gate: false,
            reconciled: None,
        };
        assert_eq!(
            stale_warning(&change).unwrap(),
            "work-unit ledger is older than stage records for WU-03 and a design rework (written by an older dtech-aidlc); it is re-synced by the next orchestrate command"
        );
    }
}
