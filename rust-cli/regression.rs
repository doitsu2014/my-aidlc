//! Attributed regression for unit-level scheduling (design section 8).
//!
//! The CLI never runs tests. After running the suites for a `build-and-verify` step, the
//! agent writes `regression.json` beside the step's regression report: per unit in
//! `.aidlc/intents/<id>/develop/build-and-verify/<WU>/regression.json`, and for the final
//! release sweep in `.aidlc/intents/<id>/develop/build-and-verify/regression.json`.
//!
//! This module parses and validates that file, infers TS numbers from test names,
//! attributes every failure to an owning work unit, and keeps the
//! `workUnits.deferredRegressions` record. A failure owned by the reporting unit (or by
//! a unit that is already finished) blocks the report; a failure owned by an unfinished
//! unit is deferred; a failure without an owner is recorded as unowned and blocks only
//! the release sweep. At the sweep every failure outside the declared baseline gates,
//! and a blocked sweep reopens the owning units.
//!
//! Everything here is a pure function over `serde_json::Value` plus file reads; no
//! function in this module writes project state.

use crate::artifacts::expected_artifacts;
use crate::ids::normalize_id;
use crate::work_units::{self, LEDGER_KEY};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u64 = 1;
pub const FILE_NAME: &str = "regression.json";
/// The `unit` value of the release sweep's `regression.json`.
pub const SWEEP: &str = "sweep";
pub const MAX_FAILURES: usize = 2000;
pub const MAX_TEST_CHARS: usize = 512;
/// How many failing tests an error message lists before "and N more".
const LISTED: usize = 20;

// ---------------------------------------------------------------------------
// TS identifiers
// ---------------------------------------------------------------------------

/// The number of a `TS-<n>` identifier (`TS-5` and `TS-05` name the same test).
pub fn ts_number(id: &str) -> Option<u64> {
    let digits = id.trim().strip_prefix("TS-")?;
    if digits.is_empty() || digits.len() > 5 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn ts_name(number: u64) -> String {
    format!("TS-{number:02}")
}

/// The TS identifier a test name carries: the last `::`, `.` or `/` segment must match
/// `^ts(\d{1,5})[a-z]?(_|$)`, case-insensitively. `ts27a_case` gives
/// `TS-27`; a name without the prefix gives `None`.
pub fn infer_ts(test: &str) -> Option<String> {
    let last = test.rsplit([':', '.', '/']).next().unwrap_or(test);
    let lower = last.to_ascii_lowercase();
    let rest = lower.strip_prefix("ts")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 5 {
        return None;
    }
    let after = &rest[digits..];
    let after = after
        .strip_prefix(|c: char| c.is_ascii_lowercase())
        .unwrap_or(after);
    if !(after.is_empty() || after.starts_with('_')) {
        return None;
    }
    rest[..digits].parse().ok().map(ts_name)
}

// ---------------------------------------------------------------------------
// Location and reading
// ---------------------------------------------------------------------------

/// Where the `regression.json` of a `build-and-verify` step lives.
#[derive(Clone, Debug)]
pub struct Location {
    pub rel_path: String,
    pub path: PathBuf,
}

/// The `regression.json` path of `stage` for the current step of `state`: beside the
/// expected `regression-report` artifact (the unit folder, or the stage folder for the
/// sweep, where `currentUnit` is absent).
pub fn location(
    root: &Path,
    graph: &Value,
    state: &Value,
    stage: &Value,
) -> Result<Location, String> {
    let expected = expected_artifacts(root, graph, state, stage)?;
    let anchor = expected
        .iter()
        .find(|item| item.output == "regression-report")
        .or_else(|| expected.first())
        .ok_or("build-and-verify produces no artifact to place regression.json beside")?;
    let rel_dir = anchor
        .rel_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let rel_path = if rel_dir.is_empty() {
        FILE_NAME.to_owned()
    } else {
        format!("{rel_dir}/{FILE_NAME}")
    };
    let path = anchor
        .path
        .parent()
        .map(|dir| dir.join(FILE_NAME))
        .unwrap_or_else(|| root.join(FILE_NAME));
    Ok(Location { rel_path, path })
}

/// What [`read`] found.
#[derive(Clone, Debug)]
pub enum Read {
    Missing,
    /// The file was written before the step was selected.
    Stale {
        written: String,
    },
    Fresh(String),
}

/// Read `regression.json` as a regular file (never through a symbolic link) and check
/// that it was written after `selected_at` (the step's selection time). Without a
/// selection time the freshness check is skipped.
pub fn read(root: &Path, location: &Location, selected_at: Option<&str>) -> Result<Read, String> {
    crate::workflow::assert_no_symlink_components(root, &location.rel_path)
        .map_err(|_| "is not a regular file".to_owned())?;
    let bytes = match work_units::read_regular(&location.path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Ok(Read::Missing),
        Err(problem) => return Err(problem.to_owned()),
    };
    let text = String::from_utf8(bytes).map_err(|_| "is not UTF-8 text".to_owned())?;
    if let Some(selected) = selected_at.filter(|at| !at.is_empty()) {
        let written = std::fs::metadata(&location.path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| crate::audit::format_iso_millis(elapsed.as_millis() as i64));
        if let Some(written) = written {
            if written.as_str() < selected {
                return Ok(Read::Stale { written });
            }
        }
    }
    Ok(Read::Fresh(text))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// One failing test of a `regression.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub test: String,
    pub suite: Option<String>,
    /// The explicit `ts` list (as written).
    pub ts: Vec<String>,
    pub owner: Option<String>,
    pub baseline: bool,
}

/// A validated `regression.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    pub unit: String,
    pub scope: String,
    pub commands: Vec<String>,
    pub summary: Option<Value>,
    pub failures: Vec<Failure>,
}

impl Evidence {
    pub fn full(&self) -> bool {
        self.scope == "full"
    }
}

fn optional_string(entry: &Value, key: &str, test: &str) -> Result<Option<String>, String> {
    match entry.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.trim().to_owned())),
        Some(_) => Err(format!("failure {test} {key} must be a non-empty string")),
    }
}

/// Parse and validate `regression.json`. `expected_unit` is the reported unit key, or
/// `None` for the release sweep (which requires `"unit": "sweep"` and `"scope":
/// "full"`). `registered` holds the TS numbers of the intent's `ids.json`, when it is
/// readable. The error is the problem only ("schemaVersion must be 1"); the caller
/// prefixes it.
pub fn parse(
    text: &str,
    expected_unit: Option<&str>,
    registered: Option<&BTreeSet<u64>>,
) -> Result<Evidence, String> {
    let data: Value = serde_json::from_str(text).map_err(|_| "is not valid JSON".to_owned())?;
    if !data.is_object() {
        return Err("must be a JSON object".into());
    }
    if data.get("schemaVersion").and_then(Value::as_u64) != Some(SCHEMA_VERSION) {
        return Err(format!("schemaVersion must be {SCHEMA_VERSION}"));
    }
    let unit = data.get("unit").and_then(Value::as_str).unwrap_or("");
    match expected_unit {
        Some(expected) => {
            if normalize_id(unit.trim()) != normalize_id(expected) {
                return Err(format!(
                    "unit must be \"{expected}\" (the reported work unit), not \"{unit}\""
                ));
            }
        }
        None => {
            if unit != SWEEP {
                return Err(format!(
                    "unit must be \"{SWEEP}\" for the release sweep, not \"{unit}\""
                ));
            }
        }
    }
    let scope = data.get("scope").and_then(Value::as_str).unwrap_or("");
    if !matches!(scope, "full" | "affected") {
        return Err("scope must be \"full\" or \"affected\"".into());
    }
    if expected_unit.is_none() && scope != "full" {
        return Err("scope must be \"full\" for the release sweep".into());
    }
    let commands: Vec<String> = match data.get("commands") {
        Some(Value::Array(items))
            if !items.is_empty()
                && items
                    .iter()
                    .all(|item| item.as_str().is_some_and(|text| !text.trim().is_empty())) =>
        {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        }
        _ => return Err("commands must be a non-empty array of non-empty strings".into()),
    };
    let summary = match data.get("summary") {
        None | Some(Value::Null) => None,
        Some(Value::Object(fields)) => {
            if let Some((key, _)) = fields.iter().find(|(_, value)| value.as_u64().is_none()) {
                return Err(format!("summary.{key} must be a non-negative integer"));
            }
            Some(Value::Object(fields.clone()))
        }
        Some(_) => return Err("summary must be an object of non-negative integers".into()),
    };
    let entries = match data.get("failures") {
        Some(Value::Array(entries)) => entries,
        _ => return Err("failures must be an array (empty when every test passed)".into()),
    };
    if entries.len() > MAX_FAILURES {
        return Err(format!(
            "lists {} failures; at most {MAX_FAILURES} are accepted",
            entries.len()
        ));
    }
    let mut seen = BTreeSet::new();
    let mut failures = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        if !entry.is_object() {
            return Err(format!("failures[{index}] must be an object"));
        }
        let test = match entry.get("test").and_then(Value::as_str).map(str::trim) {
            Some(test) if !test.is_empty() => test.to_owned(),
            _ => return Err(format!("failures[{index}].test must be a non-empty string")),
        };
        if test.chars().count() > MAX_TEST_CHARS {
            return Err(format!(
                "failures[{index}].test is longer than {MAX_TEST_CHARS} characters"
            ));
        }
        if !seen.insert(test.clone()) {
            return Err(format!("failure {test} is listed twice"));
        }
        let suite = optional_string(entry, "suite", &test)?;
        let ts = match entry.get("ts") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => {
                let mut ids = Vec::new();
                for item in items {
                    let Some(id) = item.as_str().filter(|id| ts_number(id).is_some()) else {
                        return Err(format!(
                            "failure {test} ts must be an array of TS- identifiers"
                        ));
                    };
                    if let Some(registered) = registered {
                        if !ts_number(id).is_some_and(|n| registered.contains(&n)) {
                            return Err(format!("failure {test} cites unregistered {id}"));
                        }
                    }
                    ids.push(id.trim().to_owned());
                }
                ids
            }
            Some(_) => {
                return Err(format!(
                    "failure {test} ts must be an array of TS- identifiers"
                ))
            }
        };
        let owner = optional_string(entry, "owner", &test)?;
        if let Some(owner) = &owner {
            if crate::ids::prefix_of(owner) != Some("WU") {
                return Err(format!(
                    "failure {test} owner must be a work unit id (WU-xx), not \"{owner}\""
                ));
            }
        }
        let baseline = match entry.get("baseline") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(flag)) => *flag,
            Some(_) => return Err(format!("failure {test} baseline must be true or false")),
        };
        failures.push(Failure {
            test,
            suite,
            ts,
            owner,
            baseline,
        });
    }
    // The optional summary must not contradict the failures the gate relies on.
    if let Some(failed) = summary
        .as_ref()
        .and_then(|summary| summary.get("failed"))
        .and_then(Value::as_u64)
        .filter(|failed| *failed > 0)
    {
        if failures.is_empty() {
            return Err(format!(
                "summary.failed is {failed}, but failures lists no failing test; list every failing test in failures"
            ));
        }
    }
    Ok(Evidence {
        unit: unit.trim().to_owned(),
        scope: scope.to_owned(),
        commands,
        summary,
        failures,
    })
}

// ---------------------------------------------------------------------------
// Attribution
// ---------------------------------------------------------------------------

/// How one failure counts for the report that listed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Declared baseline (failing before the intent): counted, never gating.
    Baseline,
    /// Owned by the reporting unit: blocks it.
    Own,
    /// Owned by a unit that is already finished: blocks the reporter, which most
    /// likely broke it.
    Finished,
    /// Owned by an unfinished unit: recorded, not blocking.
    Deferred,
    /// No owner: recorded, not blocking a unit; blocks the release sweep.
    Unowned,
    /// At the release sweep: owned by a unit, which the blocked sweep reopens.
    Reopen,
}

/// One attributed failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub test: String,
    pub suite: Option<String>,
    /// The TS set used for attribution (explicit, or inferred from the name).
    pub ts: Vec<String>,
    /// The owning unit (ledger key), when one was found.
    pub owner: Option<String>,
    pub class: Class,
}

/// The attribution of a whole `regression.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribution {
    pub reporter: Option<String>,
    pub items: Vec<Item>,
}

impl Attribution {
    fn count(&self, class: Class) -> usize {
        self.items.iter().filter(|item| item.class == class).count()
    }

    /// The failures that refuse `--result complete`: own and finished-unit failures for
    /// a unit; every failure outside the baseline for the sweep.
    pub fn blocking(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|item| match self.reporter {
                Some(_) => matches!(item.class, Class::Own | Class::Finished),
                None => item.class != Class::Baseline,
            })
            .collect()
    }

    /// Failures outside the declared baseline.
    pub fn failing(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|item| item.class != Class::Baseline)
            .collect()
    }

    pub fn baseline_tests(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| item.class == Class::Baseline)
            .map(|item| item.test.clone())
            .collect()
    }

    /// The owning units a blocked sweep reopens, with their failing tests, in ledger order.
    pub fn reopen(&self, state: &Value) -> Vec<(String, Vec<String>)> {
        work_units::order(state)
            .into_iter()
            .filter_map(|key| {
                let tests: Vec<String> = self
                    .items
                    .iter()
                    .filter(|item| item.class == Class::Reopen)
                    .filter(|item| item.owner.as_deref() == Some(key.as_str()))
                    .map(|item| item.test.clone())
                    .collect();
                (!tests.is_empty()).then_some((key, tests))
            })
            .collect()
    }
}

/// TS number to owning units (ledger keys of non-removed units), from the stored
/// `tests` of each unit.
pub fn ownership(state: &Value) -> BTreeMap<u64, BTreeSet<String>> {
    let mut owners: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for (key, unit) in work_units::active_units(state) {
        for test in unit
            .get("tests")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(number) = ts_number(test) {
                owners.entry(number).or_default().insert(key.clone());
            }
        }
    }
    owners
}

fn owners_list(owners: &BTreeSet<String>) -> String {
    owners.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Attribute every failure of `evidence`. `reporter` is the reporting unit key, or
/// `None` for the release sweep. An explicit `owner` must be a work unit of the intent
/// and, when the stored ownership knows the failure's TS ids, one of their owners.
pub fn attribute(
    evidence: &Evidence,
    state: &Value,
    reporter: Option<&str>,
) -> Result<Attribution, String> {
    let ownership = ownership(state);
    let mut items = Vec::with_capacity(evidence.failures.len());
    for failure in &evidence.failures {
        let ts: Vec<String> = if failure.ts.is_empty() {
            infer_ts(&failure.test).into_iter().collect()
        } else {
            failure.ts.clone()
        };
        if failure.baseline {
            items.push(Item {
                test: failure.test.clone(),
                suite: failure.suite.clone(),
                ts,
                owner: None,
                class: Class::Baseline,
            });
            continue;
        }
        let known: BTreeSet<String> = ts
            .iter()
            .filter_map(|id| ts_number(id))
            .filter_map(|number| ownership.get(&number))
            .flatten()
            .cloned()
            .collect();
        let owner: Option<String> = match &failure.owner {
            Some(named) => {
                let key = work_units::resolve_unit(state, named)
                    .filter(|key| {
                        work_units::unit(state, key)
                            .is_some_and(|unit| unit.get("removed") != Some(&Value::Bool(true)))
                    })
                    .ok_or_else(|| {
                        format!(
                            "{} owner {named} is not a work unit of this intent",
                            failure.test
                        )
                    })?;
                if !known.is_empty() && !known.contains(&key) {
                    return Err(format!(
                        "{} owner {named} does not own {} (owners: {})",
                        failure.test,
                        ts.join(", "),
                        owners_list(&known)
                    ));
                }
                Some(key)
            }
            None if ownership.is_empty() => reporter.map(str::to_owned),
            None if known.len() == 1 => known.iter().next().cloned(),
            None if known.len() > 1 => reporter
                .filter(|reporter| known.contains(*reporter))
                .map(str::to_owned),
            None => None,
        };
        let class = match (&owner, reporter) {
            (None, _) => Class::Unowned,
            (Some(_), None) => Class::Reopen,
            (Some(owner), Some(reporter)) if owner == reporter => Class::Own,
            (Some(owner), Some(_)) => {
                let finished = work_units::unit(state, owner)
                    .is_some_and(|unit| work_units::satisfied(work_units::status_of(unit)));
                if finished {
                    Class::Finished
                } else {
                    Class::Deferred
                }
            }
        };
        items.push(Item {
            test: failure.test.clone(),
            suite: failure.suite.clone(),
            ts,
            owner,
            class,
        });
    }
    Ok(Attribution {
        reporter: reporter.map(str::to_owned),
        items,
    })
}

fn item_line(item: &Item) -> String {
    let owner = item.owner.as_deref().unwrap_or("unowned");
    let ts = if item.ts.is_empty() {
        String::new()
    } else {
        format!(" ({})", item.ts.join(", "))
    };
    let finished = if item.class == Class::Finished {
        " [finished unit]"
    } else {
        ""
    };
    format!("  {owner}: {}{ts}{finished}", item.test)
}

fn item_lines(items: &[&Item]) -> String {
    let mut lines: Vec<String> = items
        .iter()
        .take(LISTED)
        .map(|item| item_line(item))
        .collect();
    if items.len() > LISTED {
        lines.push(format!("  … and {} more", items.len() - LISTED));
    }
    lines.join("\n")
}

/// The refusal of a unit's `--result complete` with blocking failures.
pub fn unit_refusal(stage: &str, unit: &str, blocking: &[&Item]) -> String {
    let finished: BTreeSet<&str> = blocking
        .iter()
        .filter(|item| item.class == Class::Finished)
        .filter_map(|item| item.owner.as_deref())
        .collect();
    let mut message = format!(
        "Stage {stage} cannot be completed for {unit}: {} failing test(s) are owned by {unit} or by a finished unit.\n{}\nFix them, or report --result blocked --detail \"<reason>\". Name the owner in regression.json when a test belongs to another unit.",
        blocking.len(),
        item_lines(blocking)
    );
    if !finished.is_empty() {
        message.push_str(&format!(
            "\n{owners} is already finished, so {unit} most likely broke it; fix it, or, when the test ownership is wrong, report {unit} blocked and run 'dtech-aidlc intent rework --unit {unit} --detail \"<reason>\"' to correct the tests in work-units.json.",
            owners = finished.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    message
}

/// The refusal of a sweep `--result complete` with failures outside the baseline.
pub fn sweep_refusal(failing: &[&Item]) -> String {
    format!(
        "The release-candidate gate needs a green regression sweep; regression.json lists {} failing test(s) outside the baseline.\n{}\nReport the sweep --result blocked --detail \"<reason>\" with this regression.json: each owning work unit is reopened, and unowned failures keep the sweep red until an owner is named.",
        failing.len(),
        item_lines(failing)
    )
}

// ---------------------------------------------------------------------------
// Ledger effects (applied to the in-memory state; the caller persists)
// ---------------------------------------------------------------------------

fn ledger_array<'a>(state: &'a mut Value, key: &str) -> Option<&'a mut Vec<Value>> {
    let ledger = state.get_mut(LEDGER_KEY)?;
    if !ledger.get(key).is_some_and(Value::is_array) {
        ledger[key] = json!([]);
    }
    ledger.get_mut(key).and_then(Value::as_array_mut)
}

/// Add test names to `workUnits.baselineTests` (each once).
fn add_baseline(state: &mut Value, tests: &[String]) {
    if let Some(list) = ledger_array(state, "baselineTests") {
        for test in tests {
            if !list
                .iter()
                .any(|known| known.as_str() == Some(test.as_str()))
            {
                list.push(json!(test));
            }
        }
    }
}

/// Upsert recorded failures into `workUnits.deferredRegressions`, keyed by test name.
fn upsert(state: &mut Value, items: &[&Item], reporter: &str, now: &str) {
    let Some(list) = ledger_array(state, "deferredRegressions") else {
        return;
    };
    for item in items {
        let unowned = item.owner.is_none();
        match list
            .iter_mut()
            .find(|entry| entry.get("test").and_then(Value::as_str) == Some(item.test.as_str()))
        {
            Some(entry) => {
                entry["ts"] = json!(item.ts);
                entry["owner"] = json!(item.owner);
                entry["unowned"] = json!(unowned);
                entry["status"] = json!("open");
                entry["lastSeen"] = json!(now);
                entry["resolvedAt"] = Value::Null;
                if item.suite.is_some() {
                    entry["suite"] = json!(item.suite);
                }
                if !entry.get("reportedBy").is_some_and(Value::is_array) {
                    entry["reportedBy"] = json!([]);
                }
                if let Some(by) = entry["reportedBy"].as_array_mut() {
                    if !by.iter().any(|known| known.as_str() == Some(reporter)) {
                        by.push(json!(reporter));
                    }
                }
            }
            None => list.push(json!({
                "test": item.test,
                "ts": item.ts,
                "owner": item.owner,
                "unowned": unowned,
                "status": "open",
                "firstSeen": now,
                "lastSeen": now,
                "reportedBy": [reporter],
                "resolvedAt": null,
                "suite": item.suite,
            })),
        }
    }
}

/// Resolve every open item that a full run did not list. Returns the resolved count.
fn resolve_unlisted(state: &mut Value, listed: &BTreeSet<&str>, now: &str) -> usize {
    let Some(list) = ledger_array(state, "deferredRegressions") else {
        return 0;
    };
    let mut resolved = 0;
    for entry in list.iter_mut() {
        let open = entry.get("status").and_then(Value::as_str) == Some("open");
        let test = entry.get("test").and_then(Value::as_str).unwrap_or("");
        if open && !listed.contains(test) {
            entry["status"] = json!("resolved");
            entry["resolvedAt"] = json!(now);
            resolved += 1;
        }
    }
    resolved
}

/// Apply an accepted unit run (`--result complete` on a unit's `build-and-verify`):
/// declared baseline failures join `baselineTests`, deferred and unowned failures are
/// upserted, a full run resolves the open items it no longer lists, and a full run
/// without failures outside the baseline upgrades every `green` unit (and the reporter)
/// to `verified`. Returns the `regression.attributed` audit details (without
/// `intentId`).
pub fn apply_unit_run(
    state: &mut Value,
    stage: &str,
    reporter: &str,
    evidence: &Evidence,
    attribution: &Attribution,
    now: &str,
) -> Value {
    add_baseline(state, &attribution.baseline_tests());
    let recorded: Vec<&Item> = attribution
        .items
        .iter()
        .filter(|item| matches!(item.class, Class::Deferred | Class::Unowned))
        .collect();
    upsert(state, &recorded, reporter, now);
    let mut resolved = 0;
    let mut verified: Vec<String> = Vec::new();
    if evidence.full() {
        let listed: BTreeSet<&str> = evidence
            .failures
            .iter()
            .map(|failure| failure.test.as_str())
            .collect();
        resolved = resolve_unlisted(state, &listed, now);
        if attribution.failing().is_empty() {
            for key in work_units::order(state) {
                let Some(unit) = state
                    .get_mut(LEDGER_KEY)
                    .and_then(|ledger| ledger.get_mut("units"))
                    .and_then(|units| units.get_mut(&key))
                else {
                    continue;
                };
                if unit.get("removed") == Some(&Value::Bool(true)) {
                    continue;
                }
                if unit.get("status").and_then(Value::as_str) == Some("green") {
                    unit["status"] = json!("verified");
                    unit["updatedAt"] = json!(now);
                    verified.push(key);
                }
            }
        }
    }
    json!({
        "unit": reporter,
        "stage": stage,
        "own": attribution.count(Class::Own) + attribution.count(Class::Finished),
        "deferred": attribution.count(Class::Deferred),
        "unowned": attribution.count(Class::Unowned),
        "baseline": attribution.count(Class::Baseline),
        "resolved": resolved,
        "scope": evidence.scope,
        "verified": verified,
    })
}

/// Apply a sweep run. `complete` (an approved green sweep) resolves every open item it
/// does not list and records the commands and baseline failures. `blocked` reopens each
/// owning unit (`red`, its last repeating step cleared, a `reopened` row), records the
/// failures as open items, and turns the sweep `red`. Returns the `regression.attributed`
/// details and the reopened units with their tests (for `unit.reopened` rows).
pub fn apply_sweep_run(
    state: &mut Value,
    stage: &str,
    result: &str,
    evidence: &Evidence,
    attribution: &Attribution,
    now: &str,
) -> (Value, Vec<(String, Vec<String>)>) {
    let baseline_failures = attribution.baseline_tests();
    let failing: Vec<&Item> = attribution.failing();
    let listed: BTreeSet<&str> = evidence
        .failures
        .iter()
        .map(|failure| failure.test.as_str())
        .collect();
    let mut reopened = Vec::new();
    if result == "blocked" && !failing.is_empty() {
        upsert(state, &failing, SWEEP, now);
        reopened = attribution.reopen(state);
        for (key, tests) in &reopened {
            let Some(unit) = state
                .get_mut(LEDGER_KEY)
                .and_then(|ledger| ledger.get_mut("units"))
                .and_then(|units| units.get_mut(key))
            else {
                continue;
            };
            unit["status"] = json!("red");
            if !unit.get("steps").is_some_and(Value::is_object) {
                unit["steps"] = json!({});
            }
            unit["steps"][stage] = Value::Null;
            unit["blockedStreak"] = Value::Null;
            if !unit.get("reopened").is_some_and(Value::is_array) {
                unit["reopened"] = json!([]);
            }
            if let Some(rows) = unit["reopened"].as_array_mut() {
                rows.push(json!({"at": now, "by": SWEEP, "tests": tests}));
            }
            unit["updatedAt"] = json!(now);
        }
    }
    let resolved = resolve_unlisted(state, &listed, now);
    if let Some(sweep) = state
        .get_mut(LEDGER_KEY)
        .and_then(|ledger| ledger.get_mut("sweep"))
    {
        sweep["commands"] = json!(evidence.commands);
        sweep["baselineFailures"] = json!(baseline_failures);
        sweep["failures"] = Value::Array(
            failing
                .iter()
                .map(|item| json!({"test": item.test, "ts": item.ts, "owner": item.owner, "suite": item.suite}))
                .collect(),
        );
        if result == "blocked" && !failing.is_empty() {
            sweep["status"] = json!("red");
        }
    }
    let details = json!({
        "unit": SWEEP,
        "stage": stage,
        "own": attribution.count(Class::Reopen),
        "deferred": 0,
        "unowned": attribution.count(Class::Unowned),
        "baseline": attribution.count(Class::Baseline),
        "resolved": resolved,
        "scope": evidence.scope,
        "reopened": reopened.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
    });
    (details, reopened)
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// Every deferred-regression item, as stored.
pub fn deferred_items(state: &Value) -> Vec<Value> {
    work_units::ledger(state)
        .and_then(|ledger| ledger.get("deferredRegressions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn is_open(item: &Value) -> bool {
    item.get("status").and_then(Value::as_str) == Some("open")
}

fn is_unowned(item: &Value) -> bool {
    item.get("unowned") == Some(&Value::Bool(true))
        || item.get("owner").and_then(Value::as_str).is_none()
}

pub fn open_count(state: &Value) -> usize {
    deferred_items(state)
        .iter()
        .filter(|item| is_open(item))
        .count()
}

/// The open items without an owner (test names).
pub fn open_unowned(state: &Value) -> Vec<String> {
    deferred_items(state)
        .iter()
        .filter(|item| is_open(item) && is_unowned(item))
        .filter_map(|item| item.get("test").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

/// The compact item list shown to an approver: test, owner, status.
pub fn deferred_summary(state: &Value) -> Vec<Value> {
    deferred_items(state)
        .iter()
        .map(|item| {
            json!({
                "test": item.get("test").cloned().unwrap_or(Value::Null),
                "owner": item.get("owner").cloned().unwrap_or(Value::Null),
                "unowned": is_unowned(item),
                "status": item.get("status").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn ownership_note(state: &Value) -> &'static str {
    if ownership(state).is_empty() {
        "none declared: a failure without an explicit owner is charged to the reporting unit"
    } else {
        "work-units.json tests and ids.json TS traces"
    }
}

/// The `verification` object of a unit's `build-and-verify` directive.
pub fn directive_verification(state: &Value, location: &Location) -> Value {
    json!({
        "path": location.rel_path,
        "schemaVersion": SCHEMA_VERSION,
        "requiredFor": "complete",
        "scope": "full",
        "deferredOpen": open_count(state),
        "ownership": ownership_note(state),
    })
}

/// The `sweep` object of the release sweep directive: what the approver sees before
/// approving.
pub fn directive_sweep(state: &Value, location: &Location) -> Value {
    let queue = work_units::directive_queue(state);
    let reopened: Vec<Value> = work_units::active_units(state)
        .into_iter()
        .flat_map(|(key, unit)| {
            unit.get("reopened")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|row| row.get("by").and_then(Value::as_str) == Some(SWEEP))
                .map(move |row| {
                    json!({
                        "unit": key,
                        "at": row.get("at").cloned().unwrap_or(Value::Null),
                        "tests": row.get("tests").cloned().unwrap_or_else(|| json!([])),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let baseline = work_units::ledger(state)
        .and_then(|ledger| ledger.get("baselineTests"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    json!({
        "path": location.rel_path,
        "schemaVersion": SCHEMA_VERSION,
        "scope": "full",
        "units": queue["total"],
        "satisfied": queue["satisfied"],
        "deferredOpen": open_count(state),
        "unowned": open_unowned(state),
        "reopenedBefore": reopened,
        "baselineTests": baseline,
        "deferredRegressions": deferred_summary(state),
        "ownerDecisions": work_units::owner_decisions(state),
    })
}

/// The extra `stage.rework` keys of a sweep selected again after a red sweep: the
/// unowned failures and how to resolve them.
pub fn sweep_rework(state: &Value) -> Option<Map<String, Value>> {
    let failures = work_units::ledger(state)?
        .pointer("/sweep/failures")
        .and_then(Value::as_array)?;
    let unowned: Vec<Value> = failures
        .iter()
        .filter(|item| item.get("owner").and_then(Value::as_str).is_none())
        .filter_map(|item| item.get("test").cloned())
        .collect();
    if unowned.is_empty() {
        return None;
    }
    let mut info = Map::new();
    info.insert("unowned".into(), Value::Array(unowned));
    info.insert(
        "hint".into(),
        json!("Name `owner` in regression.json to reopen that unit, or assign owners in work-units.json `tests` through 'dtech-aidlc intent rework' (available after a blocked sweep)."),
    );
    Some(info)
}

/// Doctor warnings about the deferred-regressions record (on a ledger preview).
pub fn doctor_warnings(state: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let items = deferred_items(state);
    let mut by_owner: BTreeMap<String, usize> = BTreeMap::new();
    for item in items
        .iter()
        .filter(|item| is_open(item) && !is_unowned(item))
    {
        if let Some(owner) = item.get("owner").and_then(Value::as_str) {
            *by_owner.entry(owner.to_owned()).or_default() += 1;
        }
    }
    for key in work_units::order(state) {
        let Some(count) = by_owner.get(&key) else {
            continue;
        };
        let started = work_units::unit(state, &key).is_some_and(|unit| {
            unit.get("steps")
                .and_then(Value::as_object)
                .is_some_and(|steps| {
                    steps
                        .values()
                        .any(|step| crate::workflow::stage_finished(step.as_str()))
                })
        });
        if !started {
            warnings.push(format!(
                "{count} failing test(s) already in the tree belong to {key}, which has not started; consider running it earlier"
            ));
        }
    }
    let unowned = open_unowned(state);
    if !unowned.is_empty() {
        let mut listed: Vec<String> = unowned.iter().take(LISTED).cloned().collect();
        if unowned.len() > LISTED {
            listed.push(format!("… and {} more", unowned.len() - LISTED));
        }
        warnings.push(format!(
            "{} failing test(s) have no owning work unit and will block the release sweep: {}",
            unowned.len(),
            listed.join(", ")
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_inference_reads_the_last_segment() {
        assert_eq!(
            infer_ts("suite_a::tests::ts27a_case").as_deref(),
            Some("TS-27")
        );
        assert_eq!(infer_ts("ts20_case").as_deref(), Some("TS-20"));
        assert_eq!(infer_ts("a.b.TS5").as_deref(), Some("TS-05"));
        assert_eq!(infer_ts("suite/ts042_x").as_deref(), Some("TS-42"));
        assert_eq!(infer_ts("ts27ab_x"), None);
        assert_eq!(infer_ts("ts123456_x"), None);
        assert_eq!(infer_ts("tests_ts20_x"), None);
        assert_eq!(
            infer_ts("suite_b::tests::job_store_reconciliation_inventory"),
            None
        );
        assert_eq!(ts_number("TS-05"), ts_number("TS-5"));
        assert_eq!(ts_number("TS-"), None);
    }

    fn evidence(text: Value) -> Result<Evidence, String> {
        parse(&text.to_string(), Some("WU-06"), None)
    }

    #[test]
    fn parse_validates_every_field() {
        let ok = json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["cargo test"], "failures": []});
        assert!(evidence(ok.clone()).is_ok());
        let cases: Vec<(Value, &str)> = vec![
            (json!({"schemaVersion": 2}), "schemaVersion must be 1"),
            (
                json!({"schemaVersion": 1, "unit": "WU-07", "scope": "full", "commands": ["x"], "failures": []}),
                "unit must be \"WU-06\"",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "some", "commands": ["x"], "failures": []}),
                "scope must be",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": [], "failures": []}),
                "commands must be",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"]}),
                "failures must be an array",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "failures": [{"test": "a"}, {"test": "a"}]}),
                "listed twice",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "failures": [{"test": "a", "ts": ["FN-01"]}]}),
                "ts must be an array of TS- identifiers",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "failures": [{"test": "a", "owner": "CFG-1"}]}),
                "owner must be a work unit id",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "failures": [{"test": "a", "baseline": "yes"}]}),
                "baseline must be true or false",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "summary": {"passed": -1}, "failures": []}),
                "summary.passed must be a non-negative integer",
            ),
            (
                json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "failures": [{"test": "x".repeat(513)}]}),
                "longer than 512",
            ),
        ];
        for (input, expected) in cases {
            let error = evidence(input).unwrap_err();
            assert!(
                error.contains(expected),
                "{error} should contain {expected}"
            );
        }
        let registered: BTreeSet<u64> = [20].into_iter().collect();
        let error = parse(
            &json!({"schemaVersion": 1, "unit": "WU-06", "scope": "full", "commands": ["x"], "failures": [{"test": "a", "ts": ["TS-21"]}]}).to_string(),
            Some("WU-06"),
            Some(&registered),
        )
        .unwrap_err();
        assert!(error.contains("cites unregistered TS-21"), "{error}");
        let sweep = json!({"schemaVersion": 1, "unit": "sweep", "scope": "affected", "commands": ["x"], "failures": []});
        assert!(parse(&sweep.to_string(), None, None)
            .unwrap_err()
            .contains("\"full\" for the release sweep"));
        assert!(parse(&ok.to_string(), None, None)
            .unwrap_err()
            .contains("unit must be \"sweep\""));
    }

    fn ledger(units: Value) -> Value {
        let mut map = Map::new();
        let mut order = Vec::new();
        for unit in units.as_array().unwrap() {
            let id = unit["id"].as_str().unwrap().to_owned();
            order.push(json!(id));
            map.insert(id, unit.clone());
        }
        json!({"workUnits": {"order": order, "units": map, "deferredRegressions": [], "baselineTests": [], "sweep": {"status": "pending"}}})
    }

    fn failure(test: &str, owner: Option<&str>) -> Failure {
        Failure {
            test: test.into(),
            suite: None,
            ts: Vec::new(),
            owner: owner.map(str::to_owned),
            baseline: false,
        }
    }

    fn run(failures: Vec<Failure>) -> Evidence {
        Evidence {
            unit: "WU-06".into(),
            scope: "full".into(),
            commands: vec!["cargo test".into()],
            summary: None,
            failures,
        }
    }

    #[test]
    fn attribution_follows_the_design_order() {
        let state = ledger(json!([
            {"id": "WU-04", "status": "green", "tests": ["TS-30", "TS-56"]},
            {"id": "WU-06", "status": "red", "tests": ["TS-42", "TS-56"]},
            {"id": "WU-16", "status": "pending", "tests": ["TS-20", "TS-30", "TS-42", "TS-56"]},
        ]));
        let evidence = run(vec![
            failure("ts20_a", None),               // one owner, unfinished: deferred
            failure("ts56_b", None),               // several owners incl. reporter: own
            failure("ts30_c", None),               // several owners, not reporter: unowned
            failure("job_store_d", None),          // no TS: unowned
            failure("ts56_e", Some("WU-16")),      // explicit owner among owners: deferred
            failure("job_store_f", Some("WU-04")), // explicit finished owner: blocking
            Failure {
                baseline: true,
                ..failure("ts42_g", None)
            },
        ]);
        let attribution = attribute(&evidence, &state, Some("WU-06")).unwrap();
        let classes: Vec<Class> = attribution.items.iter().map(|item| item.class).collect();
        assert_eq!(
            classes,
            vec![
                Class::Deferred,
                Class::Own,
                Class::Unowned,
                Class::Unowned,
                Class::Deferred,
                Class::Finished,
                Class::Baseline
            ]
        );
        assert_eq!(attribution.blocking().len(), 2);
        let wrong = run(vec![failure("ts20_a", Some("WU-04"))]);
        let error = attribute(&wrong, &state, Some("WU-06")).unwrap_err();
        assert_eq!(
            error,
            "ts20_a owner WU-04 does not own TS-20 (owners: WU-16)"
        );
        let unknown = run(vec![failure("x", Some("WU-99"))]);
        assert!(attribute(&unknown, &state, Some("WU-06"))
            .unwrap_err()
            .contains("is not a work unit"));
        // The sweep gates every failure and reopens the owners it can name.
        let sweep = attribute(&evidence, &state, None).unwrap();
        assert_eq!(sweep.blocking().len(), 6);
        assert_eq!(
            sweep.reopen(&state),
            vec![
                ("WU-04".to_owned(), vec!["job_store_f".to_owned()]),
                (
                    "WU-16".to_owned(),
                    vec!["ts20_a".to_owned(), "ts56_e".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn empty_ownership_charges_the_reporter_and_leaves_the_sweep_unowned() {
        let state = ledger(json!([
            {"id": "WU-06", "status": "red", "tests": []},
            {"id": "WU-16", "status": "pending", "tests": []},
        ]));
        let evidence = run(vec![failure("ts20_a", None), failure("b", Some("WU-16"))]);
        let attribution = attribute(&evidence, &state, Some("WU-06")).unwrap();
        assert_eq!(attribution.items[0].class, Class::Own);
        assert_eq!(attribution.items[1].class, Class::Deferred);
        let sweep = attribute(&evidence, &state, None).unwrap();
        assert_eq!(sweep.items[0].class, Class::Unowned);
        assert_eq!(sweep.items[1].class, Class::Reopen);
    }

    #[test]
    fn accepted_runs_upsert_resolve_and_upgrade() {
        let mut state = ledger(json!([
            {"id": "WU-04", "status": "green", "tests": []},
            {"id": "WU-06", "status": "green", "tests": []},
            {"id": "WU-16", "status": "pending", "tests": []},
        ]));
        let first = run(vec![
            failure("a", Some("WU-16")),
            failure("b", Some("WU-16")),
        ]);
        let attribution = attribute(&first, &state, Some("WU-06")).unwrap();
        let details = apply_unit_run(
            &mut state,
            "build-and-verify",
            "WU-06",
            &first,
            &attribution,
            "t1",
        );
        assert_eq!(details["deferred"], 2);
        assert_eq!(details["verified"], json!([]));
        assert_eq!(open_count(&state), 2);
        // Seen again by another unit: one item each, reportedBy grows.
        let again = run(vec![failure("a", Some("WU-16"))]);
        let attribution = attribute(&again, &state, Some("WU-04")).unwrap();
        let details = apply_unit_run(
            &mut state,
            "build-and-verify",
            "WU-04",
            &again,
            &attribution,
            "t2",
        );
        assert_eq!(
            details["resolved"], 1,
            "b is no longer listed by a full run"
        );
        let items = deferred_items(&state);
        assert_eq!(items[0]["reportedBy"], json!(["WU-06", "WU-04"]));
        assert_eq!(items[0]["firstSeen"], "t1");
        assert_eq!(items[0]["lastSeen"], "t2");
        assert_eq!(items[1]["status"], "resolved");
        // A clean run of an affected subset resolves and verifies nothing.
        state["workUnits"]["units"]["WU-16"]["status"] = json!("green");
        let affected = Evidence {
            scope: "affected".into(),
            ..run(Vec::new())
        };
        let attribution = attribute(&affected, &state, Some("WU-16")).unwrap();
        let details = apply_unit_run(
            &mut state,
            "build-and-verify",
            "WU-16",
            &affected,
            &attribution,
            "t3",
        );
        assert_eq!(details["resolved"], 0);
        assert_eq!(details["verified"], json!([]));
        assert_eq!(open_count(&state), 1);
        // A clean full run resolves the rest and verifies every green unit.
        let clean = run(Vec::new());
        let attribution = attribute(&clean, &state, Some("WU-16")).unwrap();
        let details = apply_unit_run(
            &mut state,
            "build-and-verify",
            "WU-16",
            &clean,
            &attribution,
            "t3",
        );
        assert_eq!(details["resolved"], 1);
        assert_eq!(details["verified"], json!(["WU-04", "WU-06", "WU-16"]));
        assert_eq!(open_count(&state), 0);
    }
}
