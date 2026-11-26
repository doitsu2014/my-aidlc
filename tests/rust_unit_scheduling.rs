//! Unit-level scheduling. Milestone M1: the per-unit work-unit ledger, its derivation
//! of the 1.0.0 stage rows, migration from 1.0.0 state, `orchestrate report --unit`,
//! the per-unit views of `status`, `intent list` and `doctor`, and strict loading of
//! `work-units.json` (no one-unit placeholder). Milestone M2: the blocked-kind
//! taxonomy, `orchestrate park`, `queue` and `revisit`, automatic and drain revisits,
//! the stalled hold (exit 3) and auto-park by repeated blocked reports. Milestone M3:
//! attributed regression (`regression.json`), the deferred-regressions record and the
//! final strict sweep that gates release-candidate and reopens owning units.
//! Milestone M4: `intent rework --unit` and `intent split`, the reconcile that resets
//! only changed units (by `inputFingerprint`) and their started direct dependants, and
//! the re-sync of a ledger that an older dtech-aidlc changed afterwards. Milestone M5:
//! doctor's early warnings (inverted dependencies, array order, test ownership, waves,
//! parks, cycles, unconfirmed decisions, a stale ledger), the wave suggestion in the
//! first unit directive, the plan's section 5 scenarios end to end with their metrics,
//! and an opt-in replay of a real intent copied to a temporary folder.
//!
//! Fixtures under `tests/fixtures/unit_scheduling/` are structural copies of a real
//! sixteen-unit intent (ids, `dependsOn` edges, array order and test ownership kept;
//! titles and text neutral).

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "aidlc-units-{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/unit_scheduling")
            .join(name),
    )
    .expect("fixture")
}

fn run(project: &Path, args: &[&str]) -> Output {
    let mut all = args.to_vec();
    all.extend(["--project", project.to_str().unwrap()]);
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .current_dir(project)
        .args(&all)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run dtech-aidlc")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn json_ok(project: &Path, args: &[&str]) -> Value {
    let output = run(project, args);
    assert!(
        output.status.success(),
        "{args:?}: {}{}",
        stderr(&output),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

fn fails(project: &Path, args: &[&str]) -> String {
    let output = run(project, args);
    assert!(
        !output.status.success(),
        "{args:?} unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    stderr(&output)
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Every file under `.aidlc` with its bytes (the engine folder excluded).
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == "engine") {
                continue;
            }
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

fn audit_rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn events(root: &Path, event: &str) -> Vec<Value> {
    audit_rows(root)
        .into_iter()
        .filter(|row| row["event"] == event)
        .collect()
}

fn state_of(root: &Path, id: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(format!(".aidlc/intents/{id}/state.json"))).unwrap())
        .unwrap()
}

fn absolute(root: &Path, raw: &str) -> PathBuf {
    if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        root.join(raw)
    }
}

/// The design files of one variant: work-units.json, ids.json and work-units.md.
struct Design {
    work_units: String,
    registry: String,
    markdown: Option<String>,
}

impl Design {
    fn real() -> Self {
        Design {
            work_units: fixture("work-units.real.json"),
            registry: fixture("ids.json"),
            markdown: Some(fixture("work-units.md")),
        }
    }
    /// A small plan without test ownership and an empty registry.
    fn small(units: Value) -> Self {
        Design {
            work_units: format!("{}\n", json!({ "units": units })),
            registry: "{\"schemaVersion\":1,\"ids\":{}}\n".into(),
            markdown: None,
        }
    }
}

/// Write every artifact a directive asks for; implementation-design gets `design`.
fn write_artifacts(root: &Path, directive: &Value, design: Option<&Design>) {
    for item in directive["artifacts"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let path = absolute(root, item["path"].as_str().unwrap());
        let output = item["output"].as_str().unwrap();
        match (output, design) {
            ("work-units-json", Some(design)) => write(&path, &design.work_units),
            ("id-registry", Some(design)) => write(&path, &design.registry),
            (
                "work-units",
                Some(Design {
                    markdown: Some(text),
                    ..
                }),
            ) => write(&path, text),
            ("work-units-json", None) => {
                if !path.exists() {
                    write(&path, "{\"units\":[{\"id\":\"WU-01\"}]}\n")
                }
            }
            ("id-registry", None) => {
                if !path.exists() {
                    write(&path, "{\"schemaVersion\":1,\"ids\":{}}\n")
                }
            }
            ("knowledge-record", _) => {
                if !path.exists() {
                    let id = directive["intentId"].as_str().unwrap();
                    let scope = directive["scope"].as_str().unwrap();
                    write(
                        &path,
                        &format!("---\nid: {id}\ntitle: {id}\norigin: intent\nscope: {scope}\nclosed: 2026-11-26T00:00:00Z\nphases-run: analyze, ideate, develop, archived\n---\n\n# {id}\n\n## Outcome\n\nDone.\n\n## Decisions\n\nNone.\n\n## Limitations\n\nNone.\n\n## Follow-ups\n\nNone.\n\n## Links\n\nNone.\n"),
                    )
                }
            }
            _ => {
                if !path.exists() {
                    write(&path, &format!("# {output}\n\nDraft.\n"))
                }
            }
        }
    }
    let map = &directive["context"]["codebaseMap"];
    if map["required"] == json!(true) && map["exists"] == json!(false) {
        let project = PathBuf::from(directive["context"]["projectRoot"].as_str().unwrap());
        write(
            &project.join(map["path"].as_str().unwrap()),
            "# Codebase map\n",
        );
    }
    // A build-and-verify step of a ledger intent needs regression evidence: a clean
    // full run, written after the step was selected.
    write_regression(root, directive, &[]);
}

/// Write the `regression.json` a build-and-verify directive names (its `verification`
/// or `sweep` path) with `failures`; nothing for other directives.
fn write_regression(root: &Path, directive: &Value, failures: &[Value]) {
    let (path, unit) = if let Some(path) = directive["verification"]["path"].as_str() {
        (path, directive["currentUnit"].as_str().unwrap_or(""))
    } else if let Some(path) = directive["sweep"]["path"].as_str() {
        (path, "sweep")
    } else {
        return;
    };
    let body = json!({
        "schemaVersion": 1,
        "unit": unit,
        "scope": "full",
        "commands": ["cargo test --workspace --no-fail-fast"],
        "failures": failures,
    });
    write(&absolute(root, path), &format!("{body}\n"));
}

fn report_complete(root: &Path, directive: &Value) -> Value {
    let mut args = vec!["orchestrate", "report", "--result", "complete"];
    if directive["stage"]["phaseGate"] == json!(true) {
        args.extend(["--approval", "approved", "--approved-by", "Owner"]);
    }
    json_ok(root, &args)
}

/// Create an intent at full depth and walk it to implementation-design (selected, not
/// reported). Returns the design directive.
fn walk_to_design(root: &Path, intent: &str) -> Value {
    json_ok(root, &["init", "--harness", "claude", "--scope", "solo"]);
    let mut next = json_ok(root, &["orchestrate", "next", "--intent", intent]);
    for guard in 0.. {
        if next["stage"]["id"] == "implementation-design" {
            return next;
        }
        assert!(guard < 20, "implementation-design was never selected");
        write_artifacts(root, &next, None);
        report_complete(root, &next);
        next = json_ok(root, &["orchestrate", "next"]);
    }
    unreachable!()
}

/// An intent whose implementation-design is complete with `design`: the ledger exists.
fn seed_develop(root: &Path, intent: &str, design: &Design) -> String {
    let directive = walk_to_design(root, intent);
    write_artifacts(root, &directive, Some(design));
    let reported = report_complete(root, &directive);
    reported["intentId"].as_str().unwrap().to_owned()
}

/// Report the current step complete (writing its artifacts first) and return the
/// directive that was completed.
fn complete_next(root: &Path) -> Value {
    let next = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &next, None);
    report_complete(root, &next);
    next
}

fn step_of(directive: &Value) -> String {
    format!(
        "{}:{}",
        directive["currentUnit"].as_str().unwrap_or("-"),
        directive["stage"]["id"].as_str().unwrap()
    )
}

/// A project holding the sanitized 1.0.0 state at the repeat span, with the real
/// design files and an audit log of earlier rows.
fn seed_legacy(root: &Path) -> Vec<u8> {
    json_ok(root, &["init", "--harness", "claude", "--scope", "solo"]);
    let dir = root.join(".aidlc/intents/unit-replay");
    write(&dir.join("state.json"), &fixture("state-1.0.0.json"));
    write(&dir.join("intent.md"), "# unit-replay\n\nReplay\n");
    let design = dir.join("develop/implementation-design");
    write(
        &design.join("work-units.json"),
        &fixture("work-units.real.json"),
    );
    write(&design.join("ids.json"), &fixture("ids.json"));
    write(&design.join("work-units.md"), &fixture("work-units.md"));
    write(
        &root.join(".aidlc/state.json"),
        "{\n  \"version\": 2,\n  \"activeIntentId\": \"unit-replay\",\n  \"updatedAt\": \"2026-11-26T02:17:34.449Z\"\n}\n",
    );
    let audit = "{\"at\":\"2026-11-21T12:42:56.822Z\",\"event\":\"intent.created\",\"intentId\":\"unit-replay\"}\n{\"at\":\"2026-11-26T02:17:13.909Z\",\"event\":\"design.rework.requested\",\"intentId\":\"unit-replay\",\"fromStage\":\"build-and-verify\"}\n";
    fs::write(root.join(".aidlc/audit.jsonl"), audit).unwrap();
    audit.as_bytes().to_vec()
}

/// Every key and value of `original` is present in `updated`, except at `skip` paths.
fn assert_subset(original: &Value, updated: &Value, path: &str, skip: &dyn Fn(&str) -> bool) {
    if skip(path) {
        return;
    }
    match (original, updated) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let child = format!("{path}/{key}");
                if skip(&child) {
                    continue;
                }
                let other = b
                    .get(key)
                    .unwrap_or_else(|| panic!("{child} disappeared after migration"));
                assert_subset(value, other, &child, skip);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path} changed length");
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                assert_subset(x, y, &format!("{path}/{index}"), skip);
            }
        }
        (a, b) => assert_eq!(a, b, "{path} changed"),
    }
}

// ---------------------------------------------------------------------------
// Migration from 1.0.0
// ---------------------------------------------------------------------------

#[test]
fn migrates_1_0_0_state_losslessly() {
    let project = TempDir::new("migrate");
    let root = project.path();
    let old_audit = seed_legacy(root);
    let original: Value = serde_json::from_str(&fixture("state-1.0.0.json")).unwrap();

    let directive = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&directive), "WU-01:red-tests");
    assert_eq!(directive["unit"]["id"], "WU-01");
    assert_eq!(directive["queue"]["total"], 16);
    assert_eq!(directive["stage"]["phaseGate"], false);

    let state = state_of(root, "unit-replay");
    let ledger = &state["workUnits"];
    assert_eq!(ledger["schemaVersion"], 1);
    assert_eq!(ledger["order"].as_array().unwrap().len(), 16);
    assert_eq!(ledger["order"][5], "WU-16", "array order is kept");
    assert_eq!(ledger["migration"]["from"], "1.0.0-stage-rows");
    assert_eq!(ledger["migration"]["ignoredKeys"], json!(["_"]));
    assert_eq!(ledger["migration"]["priorResults"], 12);
    assert!(ledger["migration"]["preSha256"].as_str().unwrap().len() == 64);
    for (id, unit) in ledger["units"].as_object().unwrap() {
        assert_eq!(
            unit["status"], "pending",
            "{id}: the 1.0.0 rework reset every unit"
        );
    }
    assert_eq!(
        ledger["units"]["WU-06"]["priorResults"],
        json!([
            {"stage": "red-tests", "result": "complete", "at": "2026-11-24T17:06:28.829Z"},
            {"stage": "build-and-verify", "result": "blocked", "at": "2026-11-25T02:25:18.022Z"},
        ])
    );
    assert_eq!(
        ledger["units"]["WU-06"]["tests"],
        json!(["TS-42", "TS-44", "TS-56", "TS-65"])
    );
    assert!(ledger["units"]["WU-01"]["inputFingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    // The placeholder entry is left untouched in the stage row.
    assert_eq!(
        state["stages"]["red-tests"]["units"]["_"],
        original["stages"]["red-tests"]["units"]["_"]
    );
    assert_eq!(state["currentStage"], "red-tests");
    assert_eq!(state["currentUnit"], "WU-01");

    // Lossless: every original key and value survives, apart from updatedAt,
    // currentStage/currentUnit and the documented mirror keys.
    let mirror = |path: &str| {
        matches!(
            path,
            "/updatedAt" | "/currentStage" | "/currentUnit" | "/units"
        ) || path.starts_with("/units/")
            || ((path.starts_with("/stages/red-tests/")
                || path.starts_with("/stages/build-and-verify/"))
                && (path.ends_with("/status") && path.matches('/').count() == 3
                    || path.contains("/units/") && path.ends_with("/status")))
    };
    assert_subset(&original, &state, "", &mirror);
    assert_eq!(state["units"].as_array().unwrap().len(), 16);

    // The audit log keeps its old bytes and gains exactly one units.migrated row.
    let audit = fs::read(root.join(".aidlc/audit.jsonl")).unwrap();
    assert!(audit.starts_with(&old_audit));
    let migrated = events(root, "units.migrated");
    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0]["units"], 16);
    assert_eq!(migrated[0]["priorResults"], 12);
    assert_eq!(migrated[0]["ignoredKeys"], json!(["_"]));
    let selected = events(root, "stage.selected");
    assert_eq!(selected.last().unwrap()["unit"], "WU-01");

    // Idempotent: a second next migrates nothing again.
    json_ok(root, &["orchestrate", "next"]);
    assert_eq!(events(root, "units.migrated").len(), 1);
    // The migrated state still loads and validates.
    let status = json_ok(root, &["status"]);
    assert_eq!(status["units"]["total"], 16);
    assert!(status["units"].get("migration").is_none());
}

#[test]
fn report_on_a_1_0_0_step_the_selector_would_not_pick_is_refused_without_writes() {
    let project = TempDir::new("migrate-report");
    let root = project.path();
    seed_legacy(root);
    // 1.0.0 recorded WU-05 as current, but WU-01 is the first ready unit.
    let path = root.join(".aidlc/intents/unit-replay/state.json");
    let mut state: Value = serde_json::from_str(&fixture("state-1.0.0.json")).unwrap();
    state["currentUnit"] = json!("WU-05");
    write(&path, &serde_json::to_string_pretty(&state).unwrap());
    let before = snapshot(root);
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "x",
        ],
    );
    assert!(
        error.contains("No current stage; run orchestrate next first"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    // The consistent 1.0.0 step migrates and is reported in the same command.
    state["currentUnit"] = json!("WU-01");
    write(&path, &serde_json::to_string_pretty(&state).unwrap());
    let reported = json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "env down",
        ],
    );
    assert_eq!(reported["unit"], "WU-01");
    let rows: Vec<String> = audit_rows(root)
        .iter()
        .map(|row| row["event"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(rows[rows.len() - 2..], ["units.migrated", "stage.reported"]);
    let state = state_of(root, "unit-replay");
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "blocked");
    assert_eq!(
        state["stages"]["red-tests"]["reports"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["unit"],
        "WU-01"
    );
}

#[test]
fn a_work_unit_stage_without_a_current_unit_is_refused() {
    let project = TempDir::new("no-unit");
    let root = project.path();
    let id = seed_develop(
        root,
        "no current unit",
        &Design::small(json!([{"id": "WU-01"}])),
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &next, None);
    let path = root.join(format!(".aidlc/intents/{id}/state.json"));
    let mut state = state_of(root, &id);
    state.as_object_mut().unwrap().shift_remove("currentUnit");
    write(&path, &serde_json::to_string_pretty(&state).unwrap());
    let before = snapshot(root);
    let error = fails(root, &["orchestrate", "report", "--result", "complete"]);
    assert!(
        error.contains("No current work unit; run orchestrate next"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn migration_preview_is_read_only_and_status_shows_each_unit_state() {
    let project = TempDir::new("preview");
    let root = project.path();
    seed_legacy(root);
    let before = snapshot(root);
    let status = json_ok(root, &["status"]);
    assert_eq!(snapshot(root), before, "status never writes");
    let units = &status["units"];
    assert_eq!(units["migration"], "pending");
    assert_eq!(units["total"], 16);
    assert_eq!(units["byStatus"], json!({"pending": 16}));
    let rows = units["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 16);
    assert_eq!(
        rows[0],
        json!({"id": "WU-01", "status": "pending", "step": "red-tests", "dependsOn": [], "park": null})
    );
    json_ok(root, &["doctor"]);
    assert_eq!(snapshot(root), before, "doctor never writes");
    let list = json_ok(root, &["intent", "list"]);
    assert!(list[0].get("workUnits").is_none(), "no stored ledger yet");

    json_ok(root, &["orchestrate", "next"]);
    let list = json_ok(root, &["intent", "list"]);
    assert_eq!(list[0]["workUnits"]["total"], 16);
    assert_eq!(list[0]["workUnits"]["byStatus"]["pending"], 16);
}

#[test]
fn doctor_reports_the_placeholder_entry_and_no_false_failures() {
    let project = TempDir::new("doctor-legacy");
    let root = project.path();
    seed_legacy(root);
    let output = run(root, &["doctor"]);
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        text.contains("stage red-tests holds a placeholder entry \\\"_\\\"")
            || text.contains("stage red-tests holds a placeholder entry \"_\""),
        "{text}"
    );
    assert!(
        !text.contains("is not the first unfinished stage"),
        "{text}"
    );
}

#[test]
fn open_1_0_0_rework_is_migrated_at_design_complete() {
    let project = TempDir::new("open-rework");
    let root = project.path();
    seed_legacy(root);
    // A 1.0.0 `intent rework` that is still open: design in progress, units removed.
    let path = root.join(".aidlc/intents/unit-replay/state.json");
    let mut state: Value = serde_json::from_str(&fixture("state-1.0.0.json")).unwrap();
    state["stages"]["implementation-design"]["status"] = json!("in_progress");
    state["stages"]["implementation-design"]["reworkRequestedAt"] =
        json!("2026-11-26T03:00:00.000Z");
    state["designReworkPending"] = json!(true);
    state["designRework"] = json!({"requestedAt": "2026-11-26T03:00:00.000Z", "fromStage": "build-and-verify", "detail": "Redo"});
    state.as_object_mut().unwrap().shift_remove("units");
    state.as_object_mut().unwrap().shift_remove("currentUnit");
    state["currentStage"] = Value::Null;
    write(&path, &serde_json::to_string_pretty(&state).unwrap());

    let design = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(design["stage"]["id"], "implementation-design");
    assert_eq!(design["stage"]["rework"]["fromStage"], "build-and-verify");
    let after_next = state_of(root, "unit-replay");
    assert!(
        after_next.get("workUnits").is_none(),
        "no ledger while the design is open"
    );
    assert!(after_next.get("designReworkPending").is_none());

    write_artifacts(root, &design, Some(&Design::real()));
    report_complete(root, &design);
    let state = state_of(root, "unit-replay");
    assert!(state["workUnits"].is_object());
    assert_eq!(events(root, "units.migrated").len(), 1);
    assert!(events(root, "units.created").is_empty());
    assert_eq!(state["workUnits"]["migration"]["ignoredKeys"], json!(["_"]));
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:red-tests");
}

// ---------------------------------------------------------------------------
// Selector, mirrors and the release sweep
// ---------------------------------------------------------------------------

#[test]
fn selector_follows_dependencies_not_array_order() {
    let project = TempDir::new("deps");
    let root = project.path();
    let id = seed_develop(
        root,
        "dependency order",
        &Design::small(json!([
            {"id": "WU-02", "dependsOn": ["WU-03"]},
            {"id": "WU-01"},
            {"id": "WU-03"},
        ])),
    );
    assert_eq!(events(root, "units.created")[0]["units"], 3);
    let mut steps = Vec::new();
    for _ in 0..6 {
        steps.push(step_of(&complete_next(root)));
    }
    assert_eq!(
        steps,
        [
            "WU-01:red-tests",
            "WU-01:build-and-verify",
            "WU-03:red-tests",
            "WU-03:build-and-verify",
            "WU-02:red-tests",
            "WU-02:build-and-verify",
        ]
    );
    let state = state_of(root, &id);
    // Each unit's clean full regression run (M3) verifies it.
    for unit in ["WU-01", "WU-02", "WU-03"] {
        assert_eq!(state["workUnits"]["units"][unit]["status"], "verified");
    }
    // Reports carry the unit they were made for.
    let reports = state["stages"]["red-tests"]["reports"].as_array().unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|r| r["unit"].clone())
            .collect::<Vec<_>>(),
        [json!("WU-01"), json!("WU-03"), json!("WU-02")]
    );
    // red-tests is complete once every unit finished red; build-and-verify waits for the sweep.
    assert_eq!(state["stages"]["red-tests"]["status"], "complete");
    assert_eq!(state["stages"]["build-and-verify"]["status"], "in_progress");
}

#[test]
fn release_gate_moves_to_the_final_sweep() {
    let project = TempDir::new("sweep");
    let root = project.path();
    let id = seed_develop(
        root,
        "sweep gate",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02", "dependsOn": ["WU-01"]}])),
    );
    complete_next(root); // WU-01 red
    complete_next(root); // WU-01 build
    complete_next(root); // WU-02 red
    let last = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&last), "WU-02:build-and-verify");
    assert_eq!(
        last["stage"]["phaseGate"], false,
        "the last unit is not the gate"
    );
    write_artifacts(root, &last, None);
    let before = snapshot(root);
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--approval",
            "approved",
            "--approved-by",
            "Owner",
        ],
    );
    assert!(
        error.contains("only accepted on the final regression sweep of build-and-verify"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    json_ok(root, &["orchestrate", "report", "--result", "complete"]);

    // Every unit is satisfied: the mirrors project the gate onto the last unit.
    let state = state_of(root, &id);
    assert_eq!(
        state["stages"]["build-and-verify"]["units"]["WU-02"]["status"],
        "pending"
    );
    assert_eq!(state["units"][1]["status"], "in_progress");
    // A clean full regression run (M3) verifies the unit.
    assert_eq!(state["workUnits"]["units"]["WU-02"]["status"], "verified");

    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(sweep["stage"]["id"], "build-and-verify");
    assert!(sweep.get("currentUnit").is_none());
    assert_eq!(sweep["stage"]["phaseGate"], true);
    assert_eq!(sweep["sweep"]["units"], 2);
    let sweep_dir = format!(".aidlc/intents/{id}/develop/build-and-verify/");
    assert!(sweep["artifacts"].as_array().unwrap().iter().all(|a| {
        let path = a["path"].as_str().unwrap().replace('\\', "/");
        path.contains(&sweep_dir) && !path.contains("/WU-0")
    }));
    write_artifacts(root, &sweep, None);
    let before = snapshot(root);
    assert!(
        fails(root, &["orchestrate", "report", "--result", "complete"])
            .contains("requires explicit --approval approved")
    );
    assert!(fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "skipped",
            "--detail",
            "x",
            "--approval",
            "approved",
            "--approved-by",
            "Owner"
        ],
    )
    .contains("cannot be skipped"));
    assert!(fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--unit",
            "WU-02"
        ]
    )
    .contains("--unit applies only to a work-unit stage"));
    assert_eq!(snapshot(root), before);
    let gate = json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--approval",
            "approved",
            "--approved-by",
            "Owner",
        ],
    );
    assert_eq!(gate["gate"], "release-candidate");
    assert_eq!(gate["sweep"], true);
    let state = state_of(root, &id);
    assert_eq!(state["stages"]["build-and-verify"]["status"], "complete");
    assert_eq!(state["gates"]["release-candidate"]["decision"], "approved");
    assert_eq!(state["workUnits"]["sweep"]["status"], "green");
    for unit in ["WU-01", "WU-02"] {
        assert_eq!(state["workUnits"]["units"][unit]["status"], "done");
        assert_eq!(
            state["stages"]["build-and-verify"]["units"][unit]["status"],
            "complete"
        );
    }
    assert!(!state["stages"]["build-and-verify"]["units"]
        .as_object()
        .unwrap()
        .contains_key("_"));
    let after = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(after["stage"]["id"], "intent-record");
    let doctor = run(root, &["doctor"]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stdout)
    );
}

#[test]
fn report_unit_must_match_current_unit() {
    let project = TempDir::new("report-unit");
    let root = project.path();
    let id = seed_develop(
        root,
        "report unit",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(next["currentUnit"], "WU-01");
    write_artifacts(root, &next, None);
    let before = snapshot(root);
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--unit",
            "WU-02",
        ],
    );
    assert!(
        error.contains("Only the current work unit (WU-01) can be reported"),
        "{error}"
    );
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--unit",
            "WU-01",
            "--unit",
            "WU-02",
        ],
    );
    assert!(
        error.contains("--unit takes one work unit for orchestrate report"),
        "{error}"
    );
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--unit",
            "WU-01,WU-02",
        ],
    );
    assert!(
        error.contains("--unit takes one work unit for orchestrate report"),
        "{error}"
    );
    assert_eq!(snapshot(root), before, "refused reports write nothing");
    let reported = json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--unit",
            "WU-01",
        ],
    );
    assert_eq!(reported["unit"], "WU-01");
    // Omitted --unit means the current unit.
    let next = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &next, None);
    let reported = json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "flaky fixture",
        ],
    );
    assert_eq!(reported["unit"], "WU-01");
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-01"];
    assert_eq!(unit["status"], "blocked");
    assert_eq!(unit["attempts"]["build-and-verify"], 1);
    assert_eq!(unit["blockedStreak"]["count"], 1);
    assert_eq!(state["currentUnit"], "WU-01", "report keeps currentUnit");
    // The blocked unit is retried with per-unit rework information.
    let retry = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&retry), "WU-01:build-and-verify");
    assert_eq!(retry["stage"]["rework"]["unit"], "WU-01");
    assert_eq!(retry["stage"]["rework"]["attempt"], 2);
    assert_eq!(retry["stage"]["rework"]["blockedStreak"], 1);
    assert_eq!(retry["stage"]["rework"]["autoParkThreshold"], 3);
    assert_eq!(retry["stage"]["rework"]["detail"], "flaky fixture");
    assert_eq!(retry["unit"]["attempt"], 2);
    // --unit on a stage outside the repeat span is refused.
    let project = TempDir::new("report-unit-design");
    let root = project.path();
    walk_to_design(root, "design unit flag");
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "x",
            "--unit",
            "WU-01",
        ],
    );
    assert!(
        error.contains("--unit applies only to a work-unit stage"),
        "{error}"
    );
}

#[test]
fn design_rework_after_blocked_design_does_not_fall_back_to_one_unit() {
    let project = TempDir::new("rework-f");
    let root = project.path();
    let design = walk_to_design(root, "blocked design rework");
    let id = design["intentId"].as_str().unwrap().to_owned();
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "contract unclear",
        ],
    );
    json_ok(root, &["intent", "rework", "--detail", "Redo the contract"]);
    let again = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(again["stage"]["id"], "implementation-design");
    assert!(again["stage"]["rework"]["requestedAt"].is_string());
    assert_eq!(again["stage"]["rework"]["detail"], "Redo the contract");
    let state = state_of(root, &id);
    assert!(
        state.get("designReworkPending").is_none(),
        "the request is consumed"
    );
    assert!(state.get("designRework").is_none());
    write_artifacts(
        root,
        &again,
        Some(&Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}]))),
    );
    report_complete(root, &again);
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&red), "WU-01:red-tests");
    let state = state_of(root, &id);
    assert!(!state["stages"]["red-tests"]["units"]
        .as_object()
        .unwrap()
        .contains_key("_"));
    assert_eq!(state["workUnits"]["order"], json!(["WU-01", "WU-02"]));
}

#[test]
fn missing_or_invalid_work_units_json_errors_without_writes() {
    let cases: &[(&str, Option<&str>, &str)] = &[
        ("missing", None, "is missing; it is required at full depth"),
        (
            "invalid",
            Some("{not json"),
            "work-units.json is not valid JSON",
        ),
        (
            "cycle",
            Some(
                r#"{"units":[{"id":"WU-01","dependsOn":["WU-02"]},{"id":"WU-02","dependsOn":["WU-01"]}]}"#,
            ),
            "dependsOn forms a cycle",
        ),
        (
            "empty",
            Some(r#"{"units":[]}"#),
            "must list at least one unit",
        ),
    ];
    for (label, content, expected) in cases {
        let project = TempDir::new(label);
        let root = project.path();
        let id = seed_develop(
            root,
            &format!("broken {label}"),
            &Design::small(json!([{"id": "WU-01"}])),
        );
        let path = root.join(format!(
            ".aidlc/intents/{id}/develop/implementation-design/work-units.json"
        ));
        // Remove the ledger: the state is as a 1.0.0 binary left it after design.
        let state_path = root.join(format!(".aidlc/intents/{id}/state.json"));
        let mut state = state_of(root, &id);
        state.as_object_mut().unwrap().shift_remove("workUnits");
        write(&state_path, &serde_json::to_string_pretty(&state).unwrap());
        match content {
            Some(text) => write(&path, text),
            None => fs::remove_file(&path).unwrap(),
        }
        let before = snapshot(root);
        let error = fails(root, &["orchestrate", "next"]);
        assert!(
            error.contains("Develop cannot select a work unit: "),
            "{label}: {error}"
        );
        assert!(error.contains(expected), "{label}: {error}");
        assert_eq!(snapshot(root), before, "{label}: nothing is written");
        let status = json_ok(root, &["status"]);
        assert!(
            status["units"]["error"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{label}"
        );
        let doctor = String::from_utf8_lossy(&run(root, &["doctor"]).stdout).into_owned();
        assert!(
            doctor.contains("Develop cannot select a work unit"),
            "{label}: {doctor}"
        );
        assert_eq!(
            snapshot(root),
            before,
            "{label}: status and doctor write nothing"
        );
        if content.is_none() {
            // Rework is the way back although nothing is blocked.
            let rework = json_ok(root, &["intent", "rework", "--detail", "Restore the plan"]);
            assert_eq!(rework["reworkRequested"], true);
            assert_eq!(rework["fromStage"], "red-tests");
            let design = json_ok(root, &["orchestrate", "next"]);
            assert_eq!(design["stage"]["id"], "implementation-design");
        }
    }
    // A not-applicable file keeps Develop as one implicit unit, without a ledger.
    let project = TempDir::new("not-applicable");
    let root = project.path();
    let id = seed_develop(
        root,
        "implicit unit",
        &Design {
            work_units: "{\"status\":\"not-applicable\",\"reason\":\"one change\"}\n".into(),
            registry: "{\"schemaVersion\":1,\"ids\":{}}\n".into(),
            markdown: None,
        },
    );
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(red["stage"]["id"], "red-tests");
    assert!(red.get("currentUnit").is_none());
    assert!(red.get("unit").is_none());
    assert!(red["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| !w.as_str().unwrap().contains("implicit work unit")));
    assert!(state_of(root, &id).get("workUnits").is_none());
    assert!(json_ok(root, &["status"]).get("units").is_none());
}

#[test]
fn ledger_ignores_later_work_units_json_edits_and_warns() {
    let project = TempDir::new("drift");
    let root = project.path();
    let id = seed_develop(
        root,
        "drift",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    let path = root.join(format!(
        ".aidlc/intents/{id}/develop/implementation-design/work-units.json"
    ));
    // Reorder through dependsOn after the ledger exists: WU-01 now waits for WU-02.
    write(
        &path,
        r#"{"units":[{"id":"WU-01","dependsOn":["WU-02"]},{"id":"WU-02"}]}"#,
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(
        step_of(&next),
        "WU-01:red-tests",
        "the ledger is authoritative"
    );
    let warnings = next["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .starts_with("work-units.json changed since the work-unit ledger was built")),
        "{warnings:?}"
    );
    let doctor = String::from_utf8_lossy(&run(root, &["doctor"]).stdout).into_owned();
    assert!(
        doctor.contains("work-units.json changed since the work-unit ledger was built"),
        "{doctor}"
    );
    // A deleted file does not stop scheduling either.
    fs::remove_file(&path).unwrap();
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:red-tests");
}

#[test]
fn legacy_u_ids_keep_their_spelling() {
    let project = TempDir::new("legacy-ids");
    let root = project.path();
    let id = seed_develop(
        root,
        "legacy ids",
        &Design::small(json!([{"id": "U-01"}, {"id": "U-02", "dependsOn": ["WU-01"]}])),
    );
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["order"], json!(["U-01", "U-02"]));
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(next["currentUnit"], "U-01");
    let red_path = next["artifacts"][0]["path"]
        .as_str()
        .unwrap()
        .replace('\\', "/");
    assert!(red_path.contains("/red-tests/U-01/"), "{red_path}");
    write_artifacts(root, &next, None);
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--unit",
            "WU-01",
        ],
    );
    let state = state_of(root, &id);
    assert_eq!(
        state["stages"]["red-tests"]["units"]["U-01"]["status"],
        "complete"
    );
    assert_eq!(state["stages"]["red-tests"]["reports"][0]["unit"], "U-01");
    complete_next(root); // U-01 build
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(
        step_of(&next),
        "U-02:red-tests",
        "dependsOn WU-01 resolves to U-01"
    );
}

#[test]
fn doctor_accepts_build_and_verify_current_while_red_tests_in_progress() {
    let project = TempDir::new("doctor-interleave");
    let root = project.path();
    let id = seed_develop(
        root,
        "doctor interleave",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    complete_next(root); // WU-01 red
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:build-and-verify");
    let state = state_of(root, &id);
    assert_eq!(state["stages"]["red-tests"]["status"], "in_progress");
    assert_eq!(state["currentStage"], "build-and-verify");
    let doctor = run(root, &["doctor"]);
    let text = String::from_utf8_lossy(&doctor.stdout).into_owned();
    assert!(
        !text.contains("is not the first unfinished stage"),
        "{text}"
    );
    assert!(doctor.status.success(), "{text}");
}

#[test]
fn work_unit_tests_are_validated_against_the_registry() {
    let project = TempDir::new("tests-field");
    let root = project.path();
    let design = walk_to_design(root, "tests field");
    let bad_shape = Design {
        work_units: r#"{"units":[{"id":"WU-01","tests":["T-1"]}]}"#.into(),
        registry: r#"{"schemaVersion":1,"ids":{"TS":[{"id":"TS-01"}]}}"#.into(),
        markdown: None,
    };
    write_artifacts(root, &design, Some(&bad_shape));
    let error = fails(root, &["orchestrate", "report", "--result", "complete"]);
    assert!(
        error.contains("work-units.json WU-01 tests must be an array of TS- identifiers"),
        "{error}"
    );
    let unregistered = Design {
        work_units: r#"{"units":[{"id":"WU-01","tests":["TS-01","TS-99"]}]}"#.into(),
        ..bad_shape
    };
    write_artifacts(root, &design, Some(&unregistered));
    let error = fails(root, &["orchestrate", "report", "--result", "complete"]);
    assert!(
        error.contains("work-units.json WU-01 tests cite unregistered TS-99"),
        "{error}"
    );
    let fine = Design {
        work_units: r#"{"units":[{"id":"WU-01","tests":["TS-01"]}]}"#.into(),
        ..unregistered
    };
    write_artifacts(root, &design, Some(&fine));
    let reported = json_ok(root, &["orchestrate", "report", "--result", "complete"]);
    let state = state_of(root, reported["intentId"].as_str().unwrap());
    assert_eq!(
        state["workUnits"]["units"]["WU-01"]["tests"],
        json!(["TS-01"])
    );
}

#[test]
fn ledger_rework_resets_units_at_design_complete_and_keeps_history() {
    let project = TempDir::new("ledger-rework");
    let root = project.path();
    let id = seed_develop(
        root,
        "ledger rework",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    complete_next(root); // WU-01 red
    let next = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &next, None);
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            "design gap",
        ],
    );
    let rework = json_ok(
        root,
        &["intent", "rework", "--detail", "Fix the design gap"],
    );
    assert_eq!(rework["fromStage"], "build-and-verify");
    assert_eq!(rework["units"], json!([]));
    let state = state_of(root, &id);
    assert!(state["workUnits"]["rework"].is_object());
    assert!(state.get("designReworkPending").is_none());
    assert_eq!(
        state["units"].as_array().unwrap().len(),
        2,
        "state.units is kept"
    );
    assert_eq!(state["stages"]["red-tests"]["status"], "in_progress");
    assert!(fails(root, &["intent", "rework", "--detail", "again"]).contains("already pending"));
    let design = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(design["stage"]["id"], "implementation-design");
    assert_eq!(design["stage"]["rework"]["units"], json!([]));
    // The redesign changes WU-01's row (so its fingerprint changes; M4 resets only
    // changed units and their started direct dependants) and adds WU-03.
    write_artifacts(
        root,
        &design,
        Some(&Design::small(
            json!([{"id": "WU-01", "title": "Redesigned"}, {"id": "WU-02"}, {"id": "WU-03"}]),
        )),
    );
    report_complete(root, &design);
    let state = state_of(root, &id);
    let units = &state["workUnits"]["units"];
    assert_eq!(units["WU-01"]["status"], "pending");
    assert_eq!(units["WU-03"]["status"], "pending");
    assert_eq!(
        units["WU-01"]["history"].as_array().unwrap().len(),
        2,
        "history is kept"
    );
    assert!(state["workUnits"]["rework"].is_null());
    let reconciled = events(root, "units.reconciled");
    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0]["added"], json!(["WU-03"]));
    assert_eq!(reconciled[0]["reset"], json!(["WU-01"]));
    assert_eq!(reconciled[0]["kept"], json!(["WU-02"]));
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:red-tests");
}

#[test]
fn ensure_ledger_only_adds_the_ledger_key() {
    let project = TempDir::new("purity");
    let root = project.path();
    seed_legacy(root);
    let graph = dtech_aidlc::scope::read_graph(None).unwrap();
    let graph = graph.value();
    let original: Value = serde_json::from_str(&fixture("state-1.0.0.json")).unwrap();
    let mut state = original.clone();
    let stages: Vec<Value> = dtech_aidlc::workflow::ordered_stages(graph, "solo", "full")
        .into_iter()
        .cloned()
        .collect();
    let change = dtech_aidlc::work_units::ensure_ledger(
        root,
        graph,
        &mut state,
        &stages,
        "2026-11-26T04:00:00.000Z",
        Some(fixture("state-1.0.0.json").as_bytes()),
    )
    .unwrap();
    assert!(matches!(
        change,
        dtech_aidlc::work_units::LedgerChange::Migrated {
            units: 16,
            prior_results: 12,
            ..
        }
    ));
    let mut without = state.clone();
    without.as_object_mut().unwrap().shift_remove("workUnits");
    assert_eq!(without, original, "only workUnits is added");
    assert!(
        dtech_aidlc::workflow::validate_intent_state(&state, graph, Some("unit-replay"), true)
            .is_empty()
    );
    // Idempotent.
    let again = state.clone();
    let change =
        dtech_aidlc::work_units::ensure_ledger(root, graph, &mut state, &stages, "later", None)
            .unwrap();
    assert_eq!(change, dtech_aidlc::work_units::LedgerChange::None);
    assert_eq!(state, again);
}

// ---------------------------------------------------------------------------
// M2: blocked-kind taxonomy, park, queue, revisit, auto-park, drain and stalls
// ---------------------------------------------------------------------------

impl Design {
    /// The real graph before the inverted-dependency workaround: WU-06 does not
    /// depend on WU-16, and WU-16 comes after WU-11.
    fn uninverted() -> Self {
        Design {
            work_units: fixture("work-units.uninverted.json"),
            ..Design::real()
        }
    }
}

fn block(root: &Path, detail: &str) -> Value {
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            detail,
        ],
    )
}

fn park(root: &Path, args: &[&str]) -> Value {
    let mut all = vec!["orchestrate", "park"];
    all.extend_from_slice(args);
    json_ok(root, &all)
}

fn park_fails(root: &Path, args: &[&str]) -> String {
    let mut all = vec!["orchestrate", "park"];
    all.extend_from_slice(args);
    fails(root, &all)
}

/// `orchestrate next` that is held back with exit code 3; returns its payload.
fn stalled(root: &Path) -> Value {
    let output = run(root, &["orchestrate", "next"]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}{}",
        stderr(&output),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("JSON payload")
}

fn units_of(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["unit"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn replay_park_foreign_regression_moves_on_and_reopens() {
    let project = TempDir::new("replay-park");
    let root = project.path();
    let id = seed_develop(root, "replay park", &Design::uninverted());
    let mut steps = Vec::new();
    for _ in 0..11 {
        steps.push(step_of(&complete_next(root)));
    }
    assert_eq!(steps[9], "WU-05:build-and-verify");
    assert_eq!(steps[10], "WU-06:red-tests");

    // WU-06's build is blocked by tests owned by WU-16: park it.
    let parked = park(
        root,
        &[
            "--kind",
            "foreign-regression",
            "--blocked-by",
            "TS-20",
            "--detail",
            "13 lifecycle tests owned by WU-16 fail",
        ],
    );
    assert_eq!(parked["unit"], "WU-06", "the current unit is parked");
    assert_eq!(parked["stage"], "build-and-verify");
    assert_eq!(parked["blockedByUnits"], json!(["WU-16"]));
    assert_eq!(parked["attempts"], 0, "no step was selected, so no attempt");
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-06"];
    assert_eq!(unit["status"], "parked");
    assert_eq!(unit["park"]["kind"], "foreign-regression");
    assert_eq!(unit["park"]["resumeStatus"], "red");
    assert_eq!(
        unit["steps"],
        json!({"red-tests": "complete", "build-and-verify": "blocked"})
    );
    // The 1.0.0 mirrors: a parked unit is unfinished, its step blocked.
    assert_eq!(state["units"][5]["id"], "WU-06");
    assert_eq!(state["units"][5]["status"], "in_progress");
    assert_eq!(
        state["stages"]["build-and-verify"]["units"]["WU-06"]["status"],
        "blocked"
    );
    let reports = state["stages"]["build-and-verify"]["reports"]
        .as_array()
        .unwrap();
    let last = reports.last().unwrap();
    assert_eq!(last["unit"], "WU-06");
    assert_eq!(last["result"], "blocked");
    assert_eq!(
        last["parked"],
        json!({"kind": "foreign-regression", "blockedBy": ["TS-20"], "auto": false})
    );
    let audit = events(root, "unit.parked");
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0]["blockedByUnits"], json!(["WU-16"]));
    assert_eq!(audit[0]["auto"], false);
    assert_eq!(audit[0]["signature"].as_str().unwrap().len(), 64);

    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(units_of(&queue["ready"]), ["WU-07", "WU-11", "WU-16"]);
    assert_eq!(queue["parked"][0]["unit"], "WU-06");
    assert_eq!(queue["parked"][0]["revisit"], "auto when WU-16 is done");
    assert!(units_of(&queue["waiting"]).contains(&"WU-08".to_owned()));
    assert_eq!(queue["done"].as_array().unwrap().len(), 5);
    assert!(queue.get("migration").is_none());
    let doctor = run(root, &["doctor"]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stdout)
    );

    // The CLI moves on to the next ready units.
    let mut steps = Vec::new();
    for _ in 0..6 {
        steps.push(step_of(&complete_next(root)));
    }
    assert_eq!(
        steps,
        [
            "WU-07:red-tests",
            "WU-07:build-and-verify",
            "WU-11:red-tests",
            "WU-11:build-and-verify",
            "WU-16:red-tests",
            "WU-16:build-and-verify",
        ]
    );
    assert!(events(root, "unit.revisited").is_empty());

    // WU-16 is done: WU-06 is reopened at its build step (its red step is kept).
    let back = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&back), "WU-06:build-and-verify");
    let revisited = events(root, "unit.revisited");
    assert_eq!(revisited.len(), 1);
    assert_eq!(revisited[0]["unit"], "WU-06");
    assert_eq!(revisited[0]["reason"], "blocker-done");
    assert_eq!(revisited[0]["kind"], "foreign-regression");
    assert_eq!(back["stage"]["rework"]["revisit"]["reason"], "blocker-done");
    assert_eq!(
        back["stage"]["rework"]["revisit"]["kind"],
        "foreign-regression"
    );
    assert_eq!(back["unit"]["revisit"]["blockedBy"], json!(["TS-20"]));
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-06"];
    assert_eq!(unit["status"], "red");
    assert!(unit["park"].is_null());
    assert_eq!(unit["parkHistory"][0]["revisitReason"], "blocker-done");
    assert_eq!(unit["steps"]["red-tests"], "complete");
    // Its next report clears the revisit marker.
    write_artifacts(root, &back, None);
    report_complete(root, &back);
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-06"]["status"], "verified");
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-08:red-tests");
    assert!(next["unit"]["revisit"].is_null());
}

#[test]
fn park_refuses_waiting_on_a_dependent_unit() {
    let project = TempDir::new("park-inverted");
    let root = project.path();
    seed_develop(root, "park inverted", &Design::real());
    json_ok(root, &["orchestrate", "next"]);
    let before = snapshot(root);
    let error = park_fails(
        root,
        &[
            "--unit",
            "WU-16",
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-06",
            "--detail",
            "x",
        ],
    );
    assert!(
        error.contains("inverted dependency: WU-06 depends on WU-16, so WU-16 cannot wait for it"),
        "{error}"
    );
    let error = park_fails(
        root,
        &[
            "--unit",
            "WU-16",
            "--kind",
            "foreign-regression",
            "--blocked-by",
            "WU-06",
            "--detail",
            "x",
        ],
    );
    assert!(
        error.contains("WU-06 depends on WU-16, so WU-16 cannot wait for it; report complete; failures owned by WU-06 are deferred"),
        "{error}"
    );
    // A test the parked unit owns itself is its own failure.
    let error = park_fails(
        root,
        &[
            "--unit",
            "WU-06",
            "--kind",
            "foreign-regression",
            "--blocked-by",
            "TS-42",
            "--detail",
            "x",
        ],
    );
    assert!(error.contains("TS-42 is owned by WU-06 itself"), "{error}");
    assert_eq!(snapshot(root), before, "refused parks write nothing");
}

#[test]
fn third_identical_blocked_report_auto_parks() {
    let project = TempDir::new("auto-park");
    let root = project.path();
    let id = seed_develop(root, "auto park", &Design::real());
    for _ in 0..5 {
        complete_next(root); // WU-01 and WU-02 satisfied, WU-03 red
    }
    let mut outputs = Vec::new();
    for (attempt, detail) in ["fixture missing", "fixture still missing", "port conflict"]
        .into_iter()
        .enumerate()
    {
        let next = json_ok(root, &["orchestrate", "next"]);
        assert_eq!(step_of(&next), "WU-03:build-and-verify");
        if attempt > 0 {
            assert_eq!(next["stage"]["rework"]["blockedStreak"], attempt);
        }
        outputs.push(block(root, detail));
    }
    assert!(outputs[0].get("parked").is_none());
    assert!(outputs[1].get("parked").is_none());
    assert_eq!(outputs[2]["parked"]["auto"], true);
    assert_eq!(outputs[2]["parked"]["kind"], "owner-decision");
    assert_eq!(outputs[2]["parked"]["count"], 3);
    // WU-04, WU-05 and WU-07 depend on WU-03; WU-11 only on WU-01.
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-11:red-tests");
    assert_eq!(next["queue"]["parked"][0]["unit"], "WU-03");
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(queue["parked"][0]["kind"], "owner-decision");
    assert_eq!(queue["parked"][0]["auto"], true);
    let parked = events(root, "unit.parked");
    assert_eq!(parked.len(), 1);
    assert_eq!(parked[0]["auto"], true);
    assert_eq!(parked[0]["count"], 3);
    assert_eq!(parked[0]["unit"], "WU-03");
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-03"];
    assert!(unit["park"]["detail"].as_str().unwrap().starts_with(
        "Auto-parked after 3 consecutive blocked reports with the same blocker on build-and-verify: port conflict"
    ));
    assert_eq!(unit["attempts"]["build-and-verify"], 3);
    let last = state["stages"]["build-and-verify"]["reports"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["parked"]["auto"], true);

    // An owner decision brings it back only with the decision recorded.
    let error = fails(root, &["orchestrate", "revisit", "--unit", "WU-03"]);
    assert!(error.contains("waits for an owner decision"), "{error}");
    let revisit = json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-03",
            "--detail",
            "Use a fixed port",
            "--approved-by",
            "Owner",
        ],
    );
    assert_eq!(revisit["revisited"][0]["reason"], "manual");
    assert_eq!(revisit["revisited"][0]["decidedBy"], "Owner");
    let back = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&back), "WU-03:build-and-verify");
    assert_eq!(
        back["stage"]["rework"]["revisit"]["decision"],
        "Use a fixed port"
    );
    assert_eq!(
        back["stage"]["rework"]["blockedStreak"], 0,
        "a revisit clears the streak"
    );
    assert_eq!(back["stage"]["rework"]["attempt"], 4);
    let queue = json_ok(root, &["orchestrate", "queue"]);
    let decision = &queue["ownerDecisions"][0];
    assert_eq!(decision["unit"], "WU-03");
    assert_eq!(decision["decision"], "Use a fixed port");
    assert_eq!(decision["decidedBy"], "Owner");
    assert_eq!(decision["confirmed"], true);
    assert_eq!(decision["parkedFor"], unit["park"]["detail"]);
}

#[test]
fn auto_park_threshold_is_configurable_and_needs_never_count() {
    let project = TempDir::new("auto-park-five");
    let root = project.path();
    let id = seed_develop(root, "auto park five", &Design::real());
    let config_path = root.join(".aidlc/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["autoParkThreshold"] = json!(5);
    write(
        &config_path,
        &serde_json::to_string_pretty(&config).unwrap(),
    );
    json_ok(
        root,
        &[
            "config",
            "prerequisite",
            "set",
            "docker",
            "--check",
            "exit 0",
        ],
    );
    for _ in 0..5 {
        complete_next(root);
    }
    let streak =
        |root: &Path| state_of(root, &id)["workUnits"]["units"]["WU-03"]["blockedStreak"].clone();
    for detail in ["one", "two"] {
        json_ok(root, &["orchestrate", "next"]);
        assert!(block(root, detail).get("parked").is_none());
    }
    assert_eq!(streak(root)["count"], 2);
    // Environment prerequisite blocks never auto-park and leave the streak alone.
    for _ in 0..4 {
        let next = json_ok(root, &["orchestrate", "next"]);
        assert_eq!(step_of(&next), "WU-03:build-and-verify");
        let output = json_ok(
            root,
            &[
                "orchestrate",
                "report",
                "--result",
                "blocked",
                "--needs",
                "docker",
                "--detail",
                "docker is down",
            ],
        );
        assert!(output.get("parked").is_none());
        assert_eq!(streak(root)["count"], 2);
    }
    for detail in ["three", "four"] {
        json_ok(root, &["orchestrate", "next"]);
        assert!(block(root, detail).get("parked").is_none());
    }
    json_ok(root, &["orchestrate", "next"]);
    let fifth = block(root, "five");
    assert_eq!(fifth["parked"]["count"], 5);
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-11:red-tests");
    assert_eq!(next["unit"]["autoParkThreshold"], 5);
}

#[test]
fn auto_park_threshold_is_validated() {
    let project = TempDir::new("auto-park-invalid");
    let root = project.path();
    seed_develop(
        root,
        "auto park invalid",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    let config_path = root.join(".aidlc/config.json");
    let original: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    let message = "Invalid autoParkThreshold: expected a positive integer";
    for bad in [json!(0), json!(2.5), json!("3")] {
        let mut config = original.clone();
        config["autoParkThreshold"] = bad.clone();
        write(
            &config_path,
            &serde_json::to_string_pretty(&config).unwrap(),
        );
        assert!(
            fails(root, &["orchestrate", "next"]).contains(message),
            "{bad}"
        );
        assert!(
            fails(root, &["config", "agents"]).contains(message),
            "{bad}"
        );
        let doctor = run(root, &["doctor"]);
        assert!(!doctor.status.success(), "{bad}");
        assert!(
            String::from_utf8_lossy(&doctor.stdout).contains(message),
            "{bad}"
        );
    }
    let mut config = original.clone();
    config["autoParkThreshold"] = json!(2);
    write(
        &config_path,
        &serde_json::to_string_pretty(&config).unwrap(),
    );
    json_ok(root, &["orchestrate", "next"]);
    assert!(block(root, "one").get("parked").is_none());
    json_ok(root, &["orchestrate", "next"]);
    assert_eq!(block(root, "two")["parked"]["count"], 2);
    // orchestrate report reads the configuration unvalidated: an invalid value falls
    // back to 3 there.
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
    assert!(block(root, "a").get("parked").is_none());
    json_ok(root, &["orchestrate", "next"]);
    config["autoParkThreshold"] = json!("x");
    write(
        &config_path,
        &serde_json::to_string_pretty(&config).unwrap(),
    );
    let second = block(root, "b");
    assert!(
        second.get("parked").is_none(),
        "the second block would park at 2, but the invalid value means 3"
    );
}

#[test]
fn report_then_park_counts_one_attempt() {
    let project = TempDir::new("park-attempts");
    let root = project.path();
    let id = seed_develop(
        root,
        "park attempts",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    block(root, "needs an owner answer");
    let parked = park(
        root,
        &[
            "--kind",
            "owner-decision",
            "--blocked-by",
            "CFG-17",
            "--detail",
            "The owner must choose the retention period",
        ],
    );
    assert_eq!(
        parked["unit"], "WU-01",
        "defaults to the unit just reported"
    );
    assert_eq!(parked["attempts"], 1);
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-01"];
    assert_eq!(
        unit["attempts"]["red-tests"], 1,
        "report then park is one attempt"
    );
    assert_eq!(unit["park"]["resumeStatus"], "blocked");
    assert_eq!(unit["park"]["blockedBy"], json!(["CFG-17"]));
    assert_eq!(
        state["stages"]["red-tests"]["reports"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // Parking the selected step is the report of that attempt.
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
    let parked = park(
        root,
        &[
            "--kind",
            "design-defect",
            "--detail",
            "The contract contradicts the plan",
        ],
    );
    assert_eq!(parked["unit"], "WU-02");
    assert_eq!(parked["attempts"], 1);
    let state = state_of(root, &id);
    assert_eq!(
        state["workUnits"]["units"]["WU-02"]["attempts"]["red-tests"],
        1
    );
    assert!(
        state["currentStage"].is_null(),
        "park clears the current step"
    );
    assert_eq!(state["currentUnit"], "WU-02");
    assert!(
        fails(root, &["orchestrate", "report", "--result", "complete"])
            .contains("No current stage")
    );
    let error = fails(root, &["orchestrate", "revisit", "--unit", "WU-02"]);
    assert!(
        error.contains("WU-02 is parked as a design defect")
            && error.contains("intent rework --unit WU-02"),
        "{error}"
    );
    // Nothing can run: both units are parked.
    let hold = stalled(root);
    assert_eq!(hold["reason"], "queue-stalled");
    assert_eq!(hold["blocked"], true);
    assert_eq!(units_of(&hold["parked"]), ["WU-01", "WU-02"]);
    let hint = hold["hint"].as_str().unwrap();
    assert!(
        hint.contains("orchestrate revisit --unit WU-01 --detail"),
        "{hint}"
    );
    assert!(hint.contains("intent rework --unit WU-02"), "{hint}");
}

#[test]
fn park_refuses_satisfied_blockers_and_wait_cycles() {
    let project = TempDir::new("park-cycles");
    let root = project.path();
    seed_develop(root, "park cycles", &Design::uninverted());
    for _ in 0..11 {
        complete_next(root); // WU-01..WU-05 satisfied, WU-06 red
    }
    let before = snapshot(root);
    let error = park_fails(
        root,
        &[
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-03",
            "--detail",
            "x",
        ],
    );
    assert!(
        error.contains("WU-03 is already done; a failure owned by a finished unit blocks WU-06"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    let parked = park(
        root,
        &[
            "--unit",
            "WU-16",
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-06",
            "--detail",
            "needs the WU-06 contract",
        ],
    );
    assert!(parked["warnings"][0]
        .as_str()
        .unwrap()
        .contains("WU-16 does not list WU-06 in dependsOn"));
    let before = snapshot(root);
    let error = park_fails(
        root,
        &[
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-16",
            "--detail",
            "x",
        ],
    );
    assert!(
        error.contains("WU-16 is parked waiting on WU-06; parking WU-06 on WU-16 would deadlock"),
        "{error}"
    );
    let error = park_fails(
        root,
        &[
            "--kind",
            "foreign-regression",
            "--blocked-by",
            "TS-20",
            "--detail",
            "x",
        ],
    );
    assert!(error.contains("would deadlock"), "{error}");
    let error = park_fails(
        root,
        &[
            "--unit",
            "WU-16",
            "--kind",
            "owner-decision",
            "--detail",
            "x",
        ],
    );
    assert!(
        error.contains("WU-16 is already parked (dep-unit); run orchestrate revisit first"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn stall_after_drain_persists_the_revisit() {
    let project = TempDir::new("drain-stall");
    let root = project.path();
    let id = seed_develop(
        root,
        "drain stall",
        &Design::small(json!([
            {"id": "WU-01"},
            {"id": "WU-02", "dependsOn": ["WU-01"]},
            {"id": "WU-03"},
            {"id": "WU-04"},
        ])),
    );
    assert_eq!(
        step_of(&json_ok(root, &["orchestrate", "next"])),
        "WU-01:red-tests"
    );
    park(
        root,
        &["--kind", "owner-decision", "--detail", "Pick a vendor"],
    );
    park(
        root,
        &[
            "--unit",
            "WU-02",
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-03",
            "--detail",
            "Needs the WU-03 schema",
        ],
    );
    assert_eq!(
        step_of(&json_ok(root, &["orchestrate", "next"])),
        "WU-03:red-tests"
    );
    park(
        root,
        &["--kind", "owner-decision", "--detail", "Pick a file format"],
    );
    assert_eq!(step_of(&complete_next(root)), "WU-04:red-tests");
    assert_eq!(step_of(&complete_next(root)), "WU-04:build-and-verify");

    // Nothing is ready; drain revisits WU-02 (progress was made after it was parked),
    // but WU-02 still waits on WU-01, which waits for an owner decision.
    let hold = stalled(root);
    assert_eq!(hold["reason"], "queue-stalled");
    assert_eq!(units_of(&hold["parked"]), ["WU-01", "WU-03"]);
    assert_eq!(
        hold["waiting"],
        json!([{"unit": "WU-02", "waitingOn": ["WU-01"]}])
    );
    let revisited = events(root, "unit.revisited");
    assert_eq!(
        revisited.len(),
        1,
        "the drain revisit is persisted and audited"
    );
    assert_eq!(revisited[0]["unit"], "WU-02");
    assert_eq!(revisited[0]["reason"], "drain");
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-02"]["status"], "pending");
    assert!(state["currentStage"].is_null());
    assert_eq!(
        state["currentUnit"], "WU-04",
        "a stall leaves the current unit"
    );
    // A second next writes nothing.
    let before = snapshot(root);
    stalled(root);
    assert_eq!(snapshot(root), before);
}

#[test]
fn owner_decision_stalls_until_revisited_with_detail() {
    let project = TempDir::new("owner-decision");
    let root = project.path();
    let id = seed_develop(
        root,
        "owner decision",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    park(
        root,
        &[
            "--kind",
            "owner-decision",
            "--blocked-by",
            "CFG-17,RK-16",
            "--detail",
            "Retention needs an owner choice",
        ],
    );
    complete_next(root); // WU-02 red
    complete_next(root); // WU-02 build
    let before = snapshot(root);
    let hold = stalled(root);
    assert_eq!(hold["parked"][0]["blockedBy"], json!(["CFG-17", "RK-16"]));
    assert!(hold["waiting"].as_array().unwrap().is_empty());
    assert_eq!(
        snapshot(root),
        before,
        "a stall without revisits writes nothing"
    );
    assert!(fails(root, &["orchestrate", "revisit", "--unit", "WU-01"])
        .contains("waits for an owner decision"));
    // Without --unit only units whose blockers are done come back: none here.
    let none = json_ok(root, &["orchestrate", "revisit"]);
    assert_eq!(none["revisited"], json!([]));
    assert_eq!(snapshot(root), before);
    let revisit = json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-01",
            "--detail",
            "Keep 30 days",
            "--approved-by",
            "AI Orchestrator",
        ],
    );
    assert_eq!(revisit["revisited"][0]["decidedBy"], "AI Orchestrator");
    let back = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&back), "WU-01:red-tests");
    assert_eq!(back["unit"]["revisit"]["decision"], "Keep 30 days");
    assert_eq!(back["stage"]["rework"]["revisit"]["kind"], "owner-decision");
    let state = state_of(root, &id);
    let history = &state["workUnits"]["units"]["WU-01"]["parkHistory"][0];
    assert_eq!(history["revisitReason"], "manual");
    assert_eq!(history["revisitDetail"], "Keep 30 days");
    assert_eq!(history["decidedBy"], "AI Orchestrator");
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(
        queue["ownerDecisions"][0]["confirmed"], false,
        "an AI decision is unconfirmed"
    );
    let rows = events(root, "unit.revisited");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["decidedBy"], "AI Orchestrator");
    assert_eq!(rows[0]["detail"], "Keep 30 days");
}

#[test]
fn drain_revisits_once_per_progress() {
    let project = TempDir::new("drain-once");
    let root = project.path();
    let id = seed_develop(
        root,
        "drain once",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    park(
        root,
        &[
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-02",
            "--detail",
            "Needs WU-02",
        ],
    );
    assert_eq!(
        step_of(&json_ok(root, &["orchestrate", "next"])),
        "WU-02:red-tests"
    );
    park(
        root,
        &["--kind", "owner-decision", "--detail", "Owner choice"],
    );
    complete_next(root); // WU-03 red
    complete_next(root); // WU-03 build: progress after WU-01 was parked
    let drained = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&drained), "WU-01:red-tests");
    assert_eq!(drained["unit"]["revisit"]["reason"], "drain");
    assert_eq!(events(root, "unit.revisited").len(), 1);
    block(root, "Still needs WU-02");
    park(
        root,
        &[
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-02",
            "--detail",
            "Needs WU-02",
        ],
    );
    assert_eq!(
        state_of(root, &id)["workUnits"]["units"]["WU-01"]["park"]["seq"],
        1
    );
    // No progress since the second park: no second drain, no loop.
    let before = snapshot(root);
    let hold = stalled(root);
    assert_eq!(units_of(&hold["parked"]), ["WU-01", "WU-02"]);
    assert_eq!(snapshot(root), before);
    assert_eq!(events(root, "unit.revisited").len(), 1);
}

#[test]
fn queue_is_read_only() {
    let project = TempDir::new("queue-legacy");
    let root = project.path();
    seed_legacy(root);
    let before = snapshot(root);
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(snapshot(root), before, "queue never writes");
    assert_eq!(queue["migration"], "pending");
    assert_eq!(queue["intentId"], "unit-replay");
    assert_eq!(units_of(&queue["ready"]), ["WU-01"]);
    assert_eq!(queue["ready"][0]["step"], "red-tests");
    assert_eq!(queue["waiting"].as_array().unwrap().len(), 15);
    assert!(queue["parked"].as_array().unwrap().is_empty());
    assert_eq!(queue["sweep"], json!({"status": "pending"}));
    assert_eq!(
        queue["deferredRegressions"],
        json!({"open": 0, "byOwner": {}, "unowned": 0})
    );
    assert!(queue["drift"].is_null());
    // Without work units there is no queue.
    let project = TempDir::new("queue-none");
    let root = project.path();
    walk_to_design(root, "no queue yet");
    let before = snapshot(root);
    assert!(fails(root, &["orchestrate", "queue"]).contains("orchestrate queue needs work units"));
    assert!(park_fails(
        root,
        &[
            "--unit",
            "WU-01",
            "--kind",
            "owner-decision",
            "--detail",
            "x"
        ]
    )
    .contains("orchestrate park needs work units"));
    assert!(
        fails(root, &["orchestrate", "revisit"]).contains("orchestrate revisit needs work units")
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn park_on_a_1_0_0_state_migrates_first() {
    let project = TempDir::new("park-legacy");
    let root = project.path();
    let old_audit = seed_legacy(root);
    let parked = park(
        root,
        &[
            "--unit",
            "WU-02",
            "--kind",
            "owner-decision",
            "--detail",
            "Owner must decide",
        ],
    );
    assert_eq!(parked["unit"], "WU-02");
    assert_eq!(parked["attempts"], 0, "WU-02 is not the current step");
    let audit = fs::read(root.join(".aidlc/audit.jsonl")).unwrap();
    assert!(audit.starts_with(&old_audit));
    let rows: Vec<String> = audit_rows(root)
        .iter()
        .map(|row| row["event"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(rows[rows.len() - 2..], ["units.migrated", "unit.parked"]);
    let state = state_of(root, "unit-replay");
    assert_eq!(state["workUnits"]["units"]["WU-02"]["status"], "parked");
    assert_eq!(
        state["currentUnit"], "WU-01",
        "the consistent 1.0.0 step is kept"
    );
    assert_eq!(state["currentStage"], "red-tests");
    assert_eq!(
        state["stages"]["red-tests"]["units"]["WU-02"]["status"],
        "blocked"
    );
    assert_eq!(state["units"][1]["status"], "in_progress");
    let graph = dtech_aidlc::scope::read_graph(None).unwrap();
    assert!(dtech_aidlc::workflow::validate_intent_state(
        &state,
        graph.value(),
        Some("unit-replay"),
        true
    )
    .is_empty());
}

#[test]
fn park_and_revisit_fail_closed() {
    let project = TempDir::new("park-fail-closed");
    let root = project.path();
    let id = seed_develop(
        root,
        "park fail closed",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    let state_path = root.join(format!(".aidlc/intents/{id}/state.json"));
    let audit_path = root.join(".aidlc/audit.jsonl");
    let break_audit = || {
        let saved = fs::read(&audit_path).unwrap();
        fs::remove_file(&audit_path).unwrap();
        fs::create_dir(&audit_path).unwrap();
        saved
    };
    let restore_audit = |saved: Vec<u8>| {
        fs::remove_dir(&audit_path).unwrap();
        fs::write(&audit_path, saved).unwrap();
    };
    // A broken audit log refuses the write before anything changes.
    let state_before = fs::read(&state_path).unwrap();
    let saved = break_audit();
    park_fails(root, &["--kind", "owner-decision", "--detail", "x"]);
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    restore_audit(saved);
    park(
        root,
        &["--kind", "owner-decision", "--detail", "Owner choice"],
    );
    let state_before = fs::read(&state_path).unwrap();
    let saved = break_audit();
    fails(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-01",
            "--detail",
            "Decided",
            "--approved-by",
            "Owner",
        ],
    );
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    restore_audit(saved);

    // A state changed by someone else after it was loaded is never overwritten.
    let graph = dtech_aidlc::scope::read_graph(None).unwrap();
    let graph = graph.value();
    let loaded = dtech_aidlc::workflow::load_workflow(root, graph, true).unwrap();
    let changed = format!("{}\n", String::from_utf8(state_before.clone()).unwrap());
    fs::write(&state_path, &changed).unwrap();
    let options = dtech_aidlc::unit_commands::UnitOptions {
        units: vec!["WU-01".into()],
        detail: Some("Decided".into()),
        approved_by: Some("Owner".into()),
        ..Default::default()
    };
    let error =
        dtech_aidlc::unit_commands::revisit_loaded(root, graph, &loaded, &options).unwrap_err();
    assert!(error.contains("Workflow state changed"), "{error}");
    let options = dtech_aidlc::unit_commands::UnitOptions {
        units: vec!["WU-02".into()],
        kind: Some("owner-decision".into()),
        detail: Some("x".into()),
        ..Default::default()
    };
    let error =
        dtech_aidlc::unit_commands::park_loaded(root, graph, &loaded, &options).unwrap_err();
    assert!(error.contains("Workflow state changed"), "{error}");
    assert_eq!(fs::read_to_string(&state_path).unwrap(), changed);
    assert!(events(root, "unit.revisited").is_empty());
    assert_eq!(events(root, "unit.parked").len(), 1);
}

#[test]
fn queue_commands_validate_their_options() {
    let project = TempDir::new("park-cli");
    let root = project.path();
    seed_develop(
        root,
        "park cli",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    let before = snapshot(root);
    let cases: &[(&[&str], &str)] = &[
        (
            &["orchestrate", "park", "--kind", "sideways", "--detail", "x"],
            "--kind must be one of dep-unit, foreign-regression, owner-decision, design-defect",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--kind",
                "env-prerequisite",
                "--detail",
                "x",
            ],
            "use orchestrate report --result blocked --needs NAME",
        ),
        (
            &["orchestrate", "park", "--kind", "owner-decision"],
            "orchestrate park requires --detail",
        ),
        (
            &["orchestrate", "park", "--detail", "x"],
            "orchestrate park requires --kind",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--unit",
                "WU-01",
                "--unit",
                "WU-02",
                "--kind",
                "owner-decision",
                "--detail",
                "x",
            ],
            "--unit takes one work unit for orchestrate park",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--unit",
                "WU-01,WU-02",
                "--kind",
                "owner-decision",
                "--detail",
                "x",
            ],
            "--unit takes one work unit for orchestrate park",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--unit",
                "WU-09",
                "--kind",
                "owner-decision",
                "--detail",
                "x",
            ],
            "Unknown work unit WU-09",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--kind",
                "owner-decision",
                "--blocked-by",
                "not an id",
                "--detail",
                "x",
            ],
            "--blocked-by not an id is not an identifier",
        ),
        (
            &["orchestrate", "park", "--kind", "dep-unit", "--detail", "x"],
            "--kind dep-unit requires --blocked-by",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--kind",
                "dep-unit",
                "--blocked-by",
                "WU-01",
                "--detail",
                "x",
            ],
            "WU-01 cannot wait for itself",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--kind",
                "foreign-regression",
                "--blocked-by",
                "TS-99",
                "--detail",
                "x",
            ],
            "TS-99 has no owning unit in work-units.json",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--kind",
                "foreign-regression",
                "--blocked-by",
                "CFG-1",
                "--detail",
                "x",
            ],
            "--blocked-by CFG-1 is neither a TS- test id nor a work unit",
        ),
        (
            &[
                "orchestrate",
                "park",
                "--kind",
                "owner-decision",
                "--blocked-by",
                "A,",
                "--detail",
                "x",
            ],
            "--blocked-by has an empty item",
        ),
        (
            &["orchestrate", "revisit", "--unit", "WU-01"],
            "WU-01 is not parked",
        ),
        (
            &["orchestrate", "revisit", "--detail", "x"],
            "--detail and --approved-by apply to orchestrate revisit --unit ID",
        ),
        (
            &["orchestrate", "park", "--stage", "red-tests"],
            "Unknown option --stage for 'orchestrate park'",
        ),
        (
            &["orchestrate", "queue", "--unit", "WU-01"],
            "Unknown option --unit for 'orchestrate queue'",
        ),
        (
            &["orchestrate", "frobnicate"],
            "Use orchestrate next, report, park, queue or revisit",
        ),
    ];
    for (args, expected) in cases {
        let error = fails(root, args);
        assert!(error.contains(expected), "{args:?}: {error}");
    }
    assert_eq!(snapshot(root), before, "refused commands write nothing");
    // Comma lists and either id spelling work for revisit.
    park(root, &["--kind", "owner-decision", "--detail", "First"]);
    park(
        root,
        &[
            "--unit",
            "U-02",
            "--kind",
            "owner-decision",
            "--detail",
            "Second",
        ],
    );
    let revisit = json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-01,WU-02",
            "--detail",
            "Both decided",
            "--approved-by",
            "Owner",
        ],
    );
    assert_eq!(units_of(&revisit["revisited"]), ["WU-01", "WU-02"]);
}

#[test]
fn design_defect_park_is_cleared_by_rework() {
    let project = TempDir::new("defect-rework");
    let root = project.path();
    let design = Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}]));
    let id = seed_develop(root, "defect rework", &design);
    json_ok(root, &["orchestrate", "next"]);
    park(
        root,
        &[
            "--kind",
            "design-defect",
            "--detail",
            "The interface is wrong",
        ],
    );
    // A parked unit (nothing is blocked) is enough for a design rework; naming it
    // resets it even though its design files did not change.
    let rework = json_ok(
        root,
        &[
            "intent",
            "rework",
            "--unit",
            "WU-01",
            "--detail",
            "Fix the interface",
        ],
    );
    assert_eq!(rework["fromStage"], "red-tests");
    assert_eq!(rework["units"], json!(["WU-01"]));
    assert!(park_fails(
        root,
        &[
            "--unit",
            "WU-02",
            "--kind",
            "owner-decision",
            "--detail",
            "x"
        ]
    )
    .contains("A design rework is pending"));
    let again = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(again["stage"]["id"], "implementation-design");
    write_artifacts(root, &again, Some(&design));
    report_complete(root, &again);
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-01"];
    assert_eq!(unit["status"], "pending");
    assert!(unit["park"].is_null());
    assert_eq!(unit["parkHistory"][0]["revisitReason"], "rework");
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:red-tests");
}

// ---------------------------------------------------------------------------
// M3: attributed regression, deferred regressions, final strict sweep
// ---------------------------------------------------------------------------

/// Overwrite the `regression.json` a build-and-verify directive names.
fn write_failures(root: &Path, directive: &Value, failures: Value) {
    write_regression(root, directive, failures.as_array().unwrap());
}

fn regression_path(root: &Path, directive: &Value) -> PathBuf {
    let path = directive["verification"]["path"]
        .as_str()
        .or_else(|| directive["sweep"]["path"].as_str())
        .expect("a regression.json path");
    absolute(root, path)
}

fn complete_args() -> Vec<&'static str> {
    vec!["orchestrate", "report", "--result", "complete"]
}

fn approve_args() -> Vec<&'static str> {
    vec![
        "orchestrate",
        "report",
        "--result",
        "complete",
        "--approval",
        "approved",
        "--approved-by",
        "Owner",
    ]
}

fn block_sweep(root: &Path, detail: &str) -> Value {
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--detail",
            detail,
        ],
    )
}

fn open_items(state: &Value) -> Vec<Value> {
    state["workUnits"]["deferredRegressions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["status"] == "open")
        .cloned()
        .collect()
}

fn set_past_mtime(path: &Path) {
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(past)
        .unwrap();
}

/// Three units that declare their tests: WU-01 (TS-01), WU-02 (TS-02) and WU-04
/// (TS-04, after WU-01).
fn owned_design() -> Design {
    Design {
        work_units: format!(
            "{}\n",
            json!({"units": [
                {"id": "WU-01", "tests": ["TS-01"]},
                {"id": "WU-02", "tests": ["TS-02"]},
                {"id": "WU-04", "dependsOn": ["WU-01"], "tests": ["TS-04"]},
            ]})
        ),
        registry: format!(
            "{}\n",
            json!({"schemaVersion": 1, "ids": {"TS": [
                {"id": "TS-01", "title": "One"},
                {"id": "TS-02", "title": "Two"},
                {"id": "TS-04", "title": "Four"},
            ]}})
        ),
        markdown: None,
    }
}

#[test]
fn wu16_failures_do_not_block_wu06() {
    let project = TempDir::new("attributed");
    let root = project.path();
    let id = seed_develop(root, "attributed regression", &Design::uninverted());
    for _ in 0..11 {
        complete_next(root);
    }
    let build = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&build), "WU-06:build-and-verify");
    let verification = &build["verification"];
    assert_eq!(
        verification["path"],
        format!(".aidlc/intents/{id}/develop/build-and-verify/WU-06/regression.json")
    );
    assert_eq!(verification["requiredFor"], "complete");
    assert_eq!(verification["scope"], "full");
    assert_eq!(verification["deferredOpen"], 0);
    write_artifacts(root, &build, None);
    let path = regression_path(root, &build);

    // regression.json is required for complete.
    fs::remove_file(&path).unwrap();
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(error.contains("regression.json is missing"), "{error}");
    assert_eq!(snapshot(root), before);

    // An explicit owner must own the test (owner_must_own_the_test).
    write_failures(
        root,
        &build,
        json!([{"test": "suite_a::tests::ts20_case", "owner": "WU-07"}]),
    );
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("owner WU-07 does not own TS-20 (owners: WU-16)"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);

    // A test with several owners, one of them the reporter, charges the reporter
    // (multi_owner_test_without_owner_blocks_the_reporter).
    write_failures(root, &build, json!([{"test": "suite_a::tests::ts56_case"}]));
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("1 failing test(s) are owned by WU-06 or by a finished unit"),
        "{error}"
    );
    assert!(
        error.contains("WU-06: suite_a::tests::ts56_case (TS-56)"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);

    // A failure owned by a finished unit blocks the reporter
    // (failure_of_a_finished_unit_blocks_the_reporter).
    write_failures(
        root,
        &build,
        json!([{"test": "store::ts19_index_survives"}]),
    );
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("WU-04: store::ts19_index_survives (TS-19) [finished unit]"),
        "{error}"
    );
    assert!(error.contains("WU-04 is already finished"), "{error}");
    assert_eq!(snapshot(root), before);

    // The 13 + 3 real failures, all owned by WU-16, are deferred: WU-06 is green.
    write(&path, &fixture("regression.wu06.json"));
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["deferred"], 16);
    assert_eq!(reported["regression"]["own"], 0);
    assert_eq!(reported["regression"]["verified"], json!([]));
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-06"]["status"], "green");
    let items = open_items(&state);
    assert_eq!(items.len(), 16);
    assert!(items.iter().all(|item| item["owner"] == "WU-16"
        && item["unowned"] == false
        && item["reportedBy"] == json!(["WU-06"])));
    assert_eq!(
        items
            .iter()
            .filter(|item| item["suite"] == "normal")
            .count(),
        13
    );
    assert_eq!(
        items
            .iter()
            .filter(|item| item["suite"] == "ignored")
            .count(),
        3
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item["test"].as_str().unwrap().contains("ts27a_"))
            .unwrap()["ts"],
        json!(["TS-27"])
    );
    let attributed = events(root, "regression.attributed");
    assert_eq!(attributed.len(), 6, "one row per accepted unit run");
    let last = attributed.last().unwrap();
    assert_eq!(last["unit"], "WU-06");
    assert_eq!(last["deferred"], 16);
    assert_eq!(last["scope"], "full");
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(queue["deferredRegressions"]["open"], 16);
    assert_eq!(queue["deferredRegressions"]["byOwner"]["WU-16"], 16);
    let doctor = String::from_utf8_lossy(&run(root, &["doctor"]).stdout).into_owned();
    assert!(
        doctor.contains(
            "16 failing test(s) already in the tree belong to WU-16, which has not started"
        ),
        "{doctor}"
    );

    // WU-07's full run still sees the 16 WU-16 failures (deferred again) and one
    // failure no unit owns: recorded as unowned, not blocking it.
    let wu16_failures: Vec<Value> = serde_json::from_str::<Value>(&fixture("regression.wu06.json"))
        .unwrap()["failures"]
        .as_array()
        .unwrap()
        .clone();
    let mut seen_by_wu07 = wu16_failures.clone();
    seen_by_wu07.push(json!({"test": "infra::tests::job_store_unowned_check"}));
    let wu07 = complete_next(root);
    assert_eq!(step_of(&wu07), "WU-07:red-tests");
    let wu07 = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&wu07), "WU-07:build-and-verify");
    assert_eq!(wu07["verification"]["deferredOpen"], 16);
    write_artifacts(root, &wu07, None);
    write_failures(root, &wu07, Value::Array(seen_by_wu07.clone()));
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["unowned"], 1);
    assert_eq!(reported["regression"]["deferred"], 16);
    assert_eq!(reported["regression"]["resolved"], 0);
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-07"]["status"], "green");
    let unowned: Vec<Value> = open_items(&state)
        .into_iter()
        .filter(|item| item["unowned"] == true)
        .collect();
    assert_eq!(unowned.len(), 1);
    assert!(unowned[0]["owner"].is_null());
    let doctor = String::from_utf8_lossy(&run(root, &["doctor"]).stdout).into_owned();
    assert!(
        doctor.contains("1 failing test(s) have no owning work unit and will block the release sweep: infra::tests::job_store_unowned_check"),
        "{doctor}"
    );

    // WU-11 sees the same failures; then WU-16. WU-16's clean full run resolves every
    // open item and verifies every green unit, WU-06 included.
    assert_eq!(step_of(&complete_next(root)), "WU-11:red-tests");
    let wu11 = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&wu11), "WU-11:build-and-verify");
    write_artifacts(root, &wu11, None);
    write_failures(root, &wu11, Value::Array(seen_by_wu07));
    json_ok(root, &complete_args());
    let state = state_of(root, &id);
    assert_eq!(open_items(&state).len(), 17);
    assert_eq!(
        open_items(&state)[0]["reportedBy"],
        json!(["WU-06", "WU-07", "WU-11"])
    );
    assert_eq!(step_of(&complete_next(root)), "WU-16:red-tests");
    let wu16 = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&wu16), "WU-16:build-and-verify");
    assert_eq!(wu16["verification"]["deferredOpen"], 17);
    write_artifacts(root, &wu16, None);
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["resolved"], 17);
    let verified = reported["regression"]["verified"].as_array().unwrap();
    assert!(verified.contains(&json!("WU-06")) && verified.contains(&json!("WU-16")));
    let state = state_of(root, &id);
    assert!(open_items(&state).is_empty());
    for unit in ["WU-06", "WU-07", "WU-16"] {
        assert_eq!(state["workUnits"]["units"][unit]["status"], "verified");
    }
    assert!(state["workUnits"]["deferredRegressions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["status"] == "resolved" && item["resolvedAt"].is_string()));
}

#[test]
fn migrated_1_0_0_state_needs_regression_evidence_and_defers_foreign_failures() {
    let project = TempDir::new("legacy-regression");
    let root = project.path();
    let old_audit = seed_legacy(root);
    let red = complete_next(root);
    assert_eq!(step_of(&red), "WU-01:red-tests");
    let build = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&build), "WU-01:build-and-verify");
    assert!(build["verification"]["path"]
        .as_str()
        .unwrap()
        .ends_with("/develop/build-and-verify/WU-01/regression.json"));
    write_artifacts(root, &build, None);
    fs::remove_file(regression_path(root, &build)).unwrap();
    let before = snapshot(root);
    assert!(fails(root, &complete_args()).contains("regression.json is missing"));
    assert_eq!(snapshot(root), before);

    // A ledger written without the M3 keys (deferredRegressions, baselineTests)
    // still records the evidence.
    let state_path = root.join(".aidlc/intents/unit-replay/state.json");
    let mut state = state_of(root, "unit-replay");
    let ledger = state["workUnits"].as_object_mut().unwrap();
    ledger.shift_remove("deferredRegressions");
    ledger.shift_remove("baselineTests");
    write(&state_path, &serde_json::to_string_pretty(&state).unwrap());
    write_failures(
        root,
        &build,
        json!([
            {"test": "suite_a::tests::ts20_case"},
            {"test": "infra::tests::old_flaky", "baseline": true},
        ]),
    );
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["deferred"], 1);
    assert_eq!(reported["regression"]["baseline"], 1);
    let state = state_of(root, "unit-replay");
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "green");
    let items = open_items(&state);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["owner"], "WU-16");
    assert_eq!(
        state["workUnits"]["baselineTests"],
        json!(["infra::tests::old_flaky"])
    );
    let audit = fs::read(root.join(".aidlc/audit.jsonl")).unwrap();
    assert!(
        audit.starts_with(&old_audit),
        "older audit rows are untouched"
    );
    assert_eq!(events(root, "units.migrated").len(), 1);
}

#[test]
fn release_gate_requires_empty_queue_and_green_sweep() {
    let project = TempDir::new("release-gate");
    let root = project.path();
    let id = seed_develop(root, "release gate", &owned_design());
    for _ in 0..5 {
        complete_next(root);
    }
    let last = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&last), "WU-04:build-and-verify");
    assert!(
        last.get("sweep").is_none(),
        "the sweep waits for every unit"
    );
    write_artifacts(root, &last, None);
    let before = snapshot(root);
    assert!(fails(root, &approve_args())
        .contains("only accepted on the final regression sweep of build-and-verify"));
    assert_eq!(snapshot(root), before);
    json_ok(root, &complete_args());

    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep.get("currentUnit").is_none());
    assert_eq!(sweep["stage"]["phaseGate"], true);
    assert_eq!(
        sweep["sweep"]["path"],
        format!(".aidlc/intents/{id}/develop/build-and-verify/regression.json")
    );
    assert_eq!(sweep["sweep"]["scope"], "full");
    assert_eq!(sweep["sweep"]["units"], 3);
    assert_eq!(sweep["sweep"]["ownerDecisions"], json!([]));
    assert_eq!(sweep["sweep"]["reopenedBefore"], json!([]));
    write_artifacts(root, &sweep, None);
    write_failures(
        root,
        &sweep,
        json!([
            {"test": "core::ts04_totals_add_up"},
            {"test": "net::flaky_unowned_probe"},
        ]),
    );

    // A sweep complete with failures is refused.
    let before = snapshot(root);
    let error = fails(root, &approve_args());
    assert!(error.contains("needs a green regression sweep"), "{error}");
    assert!(
        error.contains("WU-04: core::ts04_totals_add_up (TS-04)"),
        "{error}"
    );
    assert!(
        error.contains("unowned: net::flaky_unowned_probe"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);

    // A blocked sweep reopens the owner; the unowned failure keeps the sweep red.
    let blocked = block_sweep(root, "Two failures in the full run");
    assert_eq!(blocked["regression"]["reopened"], json!(["WU-04"]));
    assert_eq!(blocked["regression"]["unowned"], 1);
    let state = state_of(root, &id);
    let wu04 = &state["workUnits"]["units"]["WU-04"];
    assert_eq!(wu04["status"], "red");
    assert!(wu04["steps"]["build-and-verify"].is_null());
    assert_eq!(wu04["steps"]["red-tests"], "complete");
    assert_eq!(wu04["reopened"][0]["by"], "sweep");
    assert_eq!(
        wu04["reopened"][0]["tests"],
        json!(["core::ts04_totals_add_up"])
    );
    assert_eq!(state["workUnits"]["sweep"]["status"], "red");
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "verified");
    assert_eq!(state["stages"]["build-and-verify"]["status"], "in_progress");
    let reopened = events(root, "unit.reopened");
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0]["unit"], "WU-04");
    assert_eq!(reopened[0]["by"], "sweep");

    // The reopened unit runs first, then the sweep again.
    let again = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&again), "WU-04:build-and-verify");
    assert!(again["verification"].is_object());
    write_artifacts(root, &again, None);
    json_ok(root, &complete_args());
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(sweep["stage"]["id"], "build-and-verify");
    assert!(sweep.get("currentUnit").is_none());
    assert_eq!(sweep["sweep"]["reopenedBefore"][0]["unit"], "WU-04");
    assert_eq!(sweep["stage"]["rework"]["sweep"], true);
    assert_eq!(
        sweep["stage"]["rework"]["unowned"],
        json!(["net::flaky_unowned_probe"])
    );

    // Only the unowned failure is left: the sweep stays red, no unit is reopened.
    write_artifacts(root, &sweep, None);
    write_failures(root, &sweep, json!([{"test": "net::flaky_unowned_probe"}]));
    assert!(fails(root, &approve_args()).contains("needs a green regression sweep"));
    let blocked = block_sweep(root, "The probe still fails");
    assert_eq!(blocked["regression"]["reopened"], json!([]));
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["sweep"]["status"], "red");
    assert!(state["workUnits"]["units"]
        .as_object()
        .unwrap()
        .values()
        .all(|unit| unit["status"] == "verified"));
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep.get("currentUnit").is_none());
    assert_eq!(
        sweep["sweep"]["unowned"],
        json!(["net::flaky_unowned_probe"])
    );
    assert!(sweep["stage"]["rework"]["hint"]
        .as_str()
        .unwrap()
        .contains("owner"));

    // A green sweep with approval passes the release-candidate gate.
    write_artifacts(root, &sweep, None);
    let gate = json_ok(root, &approve_args());
    assert_eq!(gate["gate"], "release-candidate");
    assert_eq!(gate["regression"]["resolved"], 1);
    assert!(gate["deferredRegressions"].is_array());
    assert_eq!(gate["ownerDecisions"], json!([]));
    assert_eq!(
        gate["sweepRun"]["commands"],
        json!(["cargo test --workspace --no-fail-fast"])
    );
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["sweep"]["status"], "green");
    assert_eq!(state["stages"]["build-and-verify"]["status"], "complete");
    assert_eq!(state["gates"]["release-candidate"]["decision"], "approved");
    for unit in ["WU-01", "WU-02", "WU-04"] {
        assert_eq!(state["workUnits"]["units"][unit]["status"], "done");
    }
    assert!(open_items(&state).is_empty());
    let decided = events(root, "gate.decided");
    let release = decided
        .iter()
        .find(|row| row["gate"] == "release-candidate")
        .unwrap();
    assert_eq!(
        release["deferredRegressions"].as_array().unwrap().len(),
        2,
        "both sweep failures, now resolved"
    );
    assert!(release["ownerDecisions"].is_array());
    let attributed = events(root, "regression.attributed");
    assert_eq!(attributed.last().unwrap()["unit"], "sweep");
    let after = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(after["stage"]["id"], "intent-record");
}

#[test]
fn parked_unit_blocks_release_gate() {
    let project = TempDir::new("parked-gate");
    let root = project.path();
    let id = seed_develop(
        root,
        "parked gate",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    for _ in 0..4 {
        complete_next(root);
    }
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep["sweep"].is_object());
    write_artifacts(root, &sweep, None);
    // A hand-edited state parks WU-01 while the sweep is current.
    let state_path = root.join(format!(".aidlc/intents/{id}/state.json"));
    let mut state = state_of(root, &id);
    state["workUnits"]["units"]["WU-01"]["status"] = json!("parked");
    state["workUnits"]["units"]["WU-01"]["park"] = json!({
        "kind": "owner-decision", "blockedBy": [], "blockedByUnits": [], "signature": "x",
        "attempts": 1, "detail": "Owner must choose", "at": "2026-11-26T00:00:00.000Z",
        "step": "build-and-verify", "auto": false, "resumeStatus": "red", "seq": 0
    });
    write(&state_path, &serde_json::to_string_pretty(&state).unwrap());
    let before = snapshot(root);
    let error = fails(root, &approve_args());
    assert!(
        error.contains("needs no parked work unit; parked: WU-01 (owner-decision)"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    // The selector never offers the sweep while a unit is parked.
    let held = stalled(root);
    assert_eq!(held["reason"], "queue-stalled");
    assert_eq!(held["parked"][0]["unit"], "WU-01");
}

#[test]
fn sweep_cannot_be_skipped_or_relabelled() {
    let project = TempDir::new("sweep-relabel");
    let root = project.path();
    let id = seed_develop(
        root,
        "sweep relabel",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    complete_next(root); // WU-01 red
    let build = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&build), "WU-01:build-and-verify");
    write_artifacts(root, &build, None);
    // A unit run declares the baseline failure first.
    write_failures(
        root,
        &build,
        json!([{"test": "legacy::old_flaky", "baseline": true}]),
    );
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["baseline"], 1);
    assert_eq!(
        state_of(root, &id)["workUnits"]["units"]["WU-01"]["status"],
        "verified",
        "baseline failures do not count against a full run"
    );
    // A stale unit regression.json is refused.
    complete_next(root); // WU-02 red
    let build = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &build, None);
    set_past_mtime(&regression_path(root, &build));
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("regression.json predates this step"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    write_artifacts(root, &build, None);
    // Selecting the same step again (no report in between) keeps its selection time,
    // so evidence written for this attempt stays fresh.
    let again = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&again), "WU-02:build-and-verify");
    json_ok(root, &complete_args());

    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(
        sweep["sweep"]["baselineTests"],
        json!(["legacy::old_flaky"])
    );
    write_artifacts(root, &sweep, None);
    let path = regression_path(root, &sweep);
    let before = snapshot(root);
    assert!(fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "skipped",
            "--detail",
            "Nothing to sweep",
            "--approval",
            "approved",
            "--approved-by",
            "Owner",
        ],
    )
    .contains("cannot be skipped"));
    // A failure first called baseline at the sweep is refused.
    write_failures(
        root,
        &sweep,
        json!([{"test": "legacy::new_flaky", "baseline": true}]),
    );
    let error = fails(root, &approve_args());
    assert!(
        error.contains("marks legacy::new_flaky as baseline, but no earlier unit run did"),
        "{error}"
    );
    // The sweep needs a full run.
    write(
        &path,
        &json!({"schemaVersion": 1, "unit": "sweep", "scope": "affected", "commands": ["x"], "failures": []}).to_string(),
    );
    assert!(fails(root, &approve_args()).contains("scope must be \"full\" for the release sweep"));
    // A stale sweep regression.json is refused.
    write_failures(root, &sweep, json!([]));
    set_past_mtime(&path);
    let error = fails(root, &approve_args());
    assert!(
        error.contains("regression.json predates this step"),
        "{error}"
    );
    let without = |files: Vec<(PathBuf, Vec<u8>)>| -> Vec<(PathBuf, Vec<u8>)> {
        files
            .into_iter()
            .filter(|(file, _)| file != &path)
            .collect()
    };
    assert_eq!(
        without(snapshot(root)),
        without(before),
        "refusals write nothing"
    );
    // A blocked sweep ignores a stale file and reopens nothing.
    let blocked = block_sweep(root, "Runner unavailable");
    assert!(blocked.get("regression").is_none());
    assert!(blocked["warnings"][0]
        .as_str()
        .unwrap()
        .contains("predates this sweep step and was ignored"));
    assert!(events(root, "unit.reopened").is_empty());
    // The declared baseline passes the sweep.
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(sweep["stage"]["rework"]["detail"], "Runner unavailable");
    write_artifacts(root, &sweep, None);
    write_failures(
        root,
        &sweep,
        json!([{"test": "legacy::old_flaky", "baseline": true}]),
    );
    let gate = json_ok(root, &approve_args());
    assert_eq!(
        gate["sweepRun"]["baselineFailures"],
        json!(["legacy::old_flaky"])
    );
    assert_eq!(state_of(root, &id)["workUnits"]["sweep"]["status"], "green");
}

#[test]
fn rejected_release_gate_allows_rework() {
    let project = TempDir::new("rejected-gate");
    let root = project.path();
    let id = seed_develop(
        root,
        "rejected gate",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    for _ in 0..4 {
        complete_next(root);
    }
    let sweep = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &sweep, None);
    let rejected = json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--approval",
            "rejected",
            "--approved-by",
            "Owner",
            "--detail",
            "WU-02 needs another pass",
        ],
    );
    assert_eq!(rejected["approval"], "rejected");
    let state = state_of(root, &id);
    assert_eq!(state["gates"]["release-candidate"]["decision"], "rejected");
    assert_ne!(state["workUnits"]["sweep"]["status"], "green");
    assert!(state["workUnits"]["units"]
        .as_object()
        .unwrap()
        .values()
        .all(|unit| unit["status"] == "verified"));
    // No unit is blocked or parked, yet the blocked sweep opens a design rework.
    let rework = json_ok(
        root,
        &["intent", "rework", "--detail", "Rework WU-02 as asked"],
    );
    assert_eq!(rework["fromStage"], "build-and-verify");
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(next["stage"]["id"], "implementation-design");
}

#[test]
fn empty_ownership_charges_the_reporter() {
    let project = TempDir::new("empty-ownership");
    let root = project.path();
    let id = seed_develop(
        root,
        "empty ownership",
        &Design::small(json!([{"id": "WU-06"}, {"id": "WU-16"}])),
    );
    complete_next(root); // WU-06 red
    let build = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&build), "WU-06:build-and-verify");
    assert!(build["verification"]["ownership"]
        .as_str()
        .unwrap()
        .starts_with("none declared"));
    write_artifacts(root, &build, None);
    write_failures(root, &build, json!([{"test": "suite_a::ts20_case"}]));
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("WU-06: suite_a::ts20_case (TS-20)"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    write_failures(
        root,
        &build,
        json!([{"test": "suite_a::ts20_case", "owner": "WU-16"}]),
    );
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["deferred"], 1);
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-06"]["status"], "green");
    assert_eq!(open_items(&state)[0]["owner"], "WU-16");
}

// ---------------------------------------------------------------------------
// M4: scoped rework, fingerprints, split, re-sync after a 1.0.0 write
// ---------------------------------------------------------------------------

impl Design {
    /// A small plan with a `## <id>` section per unit in work-units.md.
    fn small_md(units: Value) -> Self {
        let mut markdown = String::from("# Work units\n");
        for unit in units.as_array().unwrap() {
            let id = unit["id"].as_str().unwrap();
            markdown.push_str(&format!(
                "\n## {id} Unit {id}\n\n- Status: pending\n- Builds part {id}.\n"
            ));
        }
        Design {
            markdown: Some(markdown),
            ..Design::small(units)
        }
    }
}

/// `markdown` with one line added to the `## <id>` section (a design change).
fn change_section(markdown: &str, id: &str, line: &str) -> String {
    let heading = format!("## {id} ");
    let start = markdown.find(&heading).expect("section");
    let end = markdown[start..]
        .find('\n')
        .map(|offset| start + offset + 1)
        .unwrap();
    format!("{}{line}\n{}", &markdown[..end], &markdown[end..])
}

/// `markdown` with the `Status:` line of the `## <id>` section rewritten (not a
/// design change: Status items are not fingerprinted).
fn change_status_line(markdown: &str, id: &str, status: &str) -> String {
    let heading = format!("## {id} ");
    let start = markdown.find(&heading).expect("section");
    let at = start + markdown[start..].find("- Status: ").unwrap();
    let end = at + markdown[at..].find('\n').unwrap();
    format!("{}- Status: {status}{}", &markdown[..at], &markdown[end..])
}

fn edit_state(root: &Path, id: &str, edit: impl FnOnce(&mut Value)) {
    let mut state = state_of(root, id);
    edit(&mut state);
    write(
        &root.join(format!(".aidlc/intents/{id}/state.json")),
        &format!("{}\n", serde_json::to_string_pretty(&state).unwrap()),
    );
}

/// An ISO timestamp `ms` milliseconds after `iso` (`YYYY-MM-DDTHH:MM:SS.mmmZ`).
fn later(iso: &str, ms: i64) -> String {
    let field = |range: std::ops::Range<usize>| iso[range].parse::<i64>().unwrap();
    let (y, m, d) = (field(0..4), field(5..7), field(8..10));
    let days_from_civil = |y: i64, m: i64, d: i64| {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    };
    let total = days_from_civil(y, m, d) * 86_400_000
        + field(11..13) * 3_600_000
        + field(14..16) * 60_000
        + field(17..19) * 1000
        + field(20..23)
        + ms;
    let (days, rest) = (total.div_euclid(86_400_000), total.rem_euclid(86_400_000));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

/// Reports of the repeating stages that name one of `units`.
fn unit_report_count(state: &Value, units: &[&str]) -> usize {
    ["red-tests", "build-and-verify"]
        .iter()
        .flat_map(|stage| {
            state["stages"][stage]["reports"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter(|report| {
            report["unit"]
                .as_str()
                .is_some_and(|unit| units.contains(&unit))
        })
        .count()
}

fn rework(root: &Path, args: &[&str]) -> Value {
    let mut all = vec!["intent", "rework"];
    all.extend_from_slice(args);
    json_ok(root, &all)
}

#[test]
fn rework_one_unit_keeps_green_units() {
    let project = TempDir::new("scoped-rework");
    let root = project.path();
    let design = Design::uninverted();
    let id = seed_develop(root, "scoped rework", &design);
    for _ in 0..10 {
        complete_next(root); // WU-01..WU-05, red then build
    }
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&red), "WU-06:red-tests");
    park(
        root,
        &[
            "--kind",
            "design-defect",
            "--detail",
            "The allocation contract cannot hold",
        ],
    );
    let kept = ["WU-01", "WU-02", "WU-03", "WU-04", "WU-05"];
    let before = state_of(root, &id);
    let reports_before = unit_report_count(&before, &kept);
    assert_eq!(reports_before, 10);
    let fingerprints_before: Vec<Value> = kept
        .iter()
        .map(|unit| before["workUnits"]["units"][unit]["inputFingerprint"].clone())
        .collect();

    let requested = rework(
        root,
        &["--unit", "WU-06", "--detail", "Redesign the contract"],
    );
    assert_eq!(requested["units"], json!(["WU-06"]));
    assert_eq!(requested["fromStage"], "red-tests");
    assert_eq!(
        events(root, "design.rework.requested")[0]["units"],
        json!(["WU-06"])
    );
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["rework"]["units"], json!(["WU-06"]));
    assert_eq!(state["designRework"]["units"], json!(["WU-06"]));

    let directive = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(directive["stage"]["id"], "implementation-design");
    assert_eq!(directive["stage"]["rework"]["units"], json!(["WU-06"]));
    assert!(directive["stage"]["rework"]["instruction"]
        .as_str()
        .unwrap()
        .contains("Change only the design of WU-06"));
    // Only WU-06's row and section change; WU-02's Status line is rewritten too.
    let mut rows: Value = serde_json::from_str(&design.work_units).unwrap();
    for row in rows["units"].as_array_mut().unwrap() {
        if row["id"] == "WU-06" {
            row["title"] = json!("Allocation contract, second design");
        }
    }
    let markdown = change_section(
        &change_status_line(design.markdown.as_deref().unwrap(), "WU-02", "verified"),
        "WU-06",
        "- Contract: reconciliation keeps one owner per allocation.",
    );
    let redesign = Design {
        work_units: format!("{}\n", serde_json::to_string_pretty(&rows).unwrap()),
        registry: design.registry.clone(),
        markdown: Some(markdown),
    };
    write_artifacts(root, &directive, Some(&redesign));
    report_complete(root, &directive);

    let state = state_of(root, &id);
    let units = &state["workUnits"]["units"];
    for (unit, fingerprint) in kept.iter().zip(&fingerprints_before) {
        assert_eq!(
            units[unit]["status"], before["workUnits"]["units"][unit]["status"],
            "{unit}"
        );
        assert_eq!(&units[unit]["inputFingerprint"], fingerprint, "{unit}");
        assert_eq!(units[unit]["reopened"], json!([]), "{unit}");
    }
    assert_eq!(units["WU-06"]["status"], "pending");
    assert!(units["WU-06"]["park"].is_null());
    assert_eq!(units["WU-06"]["parkHistory"][0]["revisitReason"], "rework");
    assert_eq!(units["WU-06"]["reopened"][0]["by"], "rework");
    assert_eq!(
        unit_report_count(&state, &kept),
        reports_before,
        "no report is appended for kept units"
    );
    let reconciled = events(root, "units.reconciled");
    assert_eq!(reconciled.len(), 1);
    let kept_ids: Vec<&str> = reconciled[0]["kept"]
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| unit.as_str().unwrap())
        .collect();
    for unit in kept {
        assert!(kept_ids.contains(&unit), "{unit} is kept");
    }
    // WU-08 depends on WU-06 but had not started, so it has nothing to reset.
    assert_eq!(reconciled[0]["reset"], json!(["WU-06"]));
    assert_eq!(reconciled[0]["changed"], json!(["WU-06"]));
    assert_eq!(reconciled[0]["added"], json!([]));
    assert_eq!(reconciled[0]["removed"], json!([]));
    assert!(state["workUnits"]["rework"].is_null());
    assert!(state.get("designRework").is_none());
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-06:red-tests");
    // No skipped report anywhere: nothing had to be replayed.
    let state = state_of(root, &id);
    for stage in ["red-tests", "build-and-verify"] {
        assert!(state["stages"][stage]["reports"]
            .as_array()
            .unwrap()
            .iter()
            .all(|report| report["result"] != "skipped"));
    }
}

#[test]
fn fingerprint_change_resets_unit_and_direct_dependents_only() {
    let project = TempDir::new("fingerprint");
    let root = project.path();
    let design = Design::small_md(json!([
        {"id": "WU-01"},
        {"id": "WU-02", "dependsOn": ["WU-01"]},
        {"id": "WU-03", "dependsOn": ["WU-02"]},
    ]));
    let id = seed_develop(root, "fingerprint chain", &design);
    for _ in 0..6 {
        complete_next(root);
    }
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep.get("sweep").is_some());
    write_artifacts(root, &sweep, None);
    block_sweep(root, "The totals of WU-01 are designed wrong");
    // No unit is blocked; the blocked sweep opens a rework. Without --unit only
    // fingerprint changes count.
    let requested = rework(root, &["--detail", "Fix WU-01"]);
    assert_eq!(requested["units"], json!([]));
    let directive = json_ok(root, &["orchestrate", "next"]);
    assert!(directive["stage"]["rework"]["instruction"]
        .as_str()
        .unwrap()
        .contains("started direct dependants"));
    let redesign = Design {
        markdown: Some(change_section(
            design.markdown.as_deref().unwrap(),
            "WU-01",
            "- Totals are kept per account.",
        )),
        ..Design::small_md(json!([
            {"id": "WU-01"},
            {"id": "WU-02", "dependsOn": ["WU-01"]},
            {"id": "WU-03", "dependsOn": ["WU-02"]},
        ]))
    };
    write_artifacts(root, &directive, Some(&redesign));
    report_complete(root, &directive);
    let reconciled = &events(root, "units.reconciled")[0];
    assert_eq!(reconciled["changed"], json!(["WU-01"]));
    assert_eq!(reconciled["reset"], json!(["WU-01", "WU-02"]));
    assert_eq!(reconciled["kept"], json!(["WU-03"]));
    let state = state_of(root, &id);
    let units = &state["workUnits"]["units"];
    assert_eq!(units["WU-01"]["status"], "pending");
    assert_eq!(units["WU-02"]["status"], "pending");
    assert_eq!(units["WU-03"]["status"], "verified");
    assert_eq!(units["WU-02"]["steps"]["red-tests"], Value::Null);
    assert_eq!(
        units["WU-02"]["history"].as_array().unwrap().len(),
        2,
        "history is kept"
    );
    // The 1.0.0 mirrors follow the ledger: WU-03 stays complete.
    assert_eq!(
        state["stages"]["red-tests"]["units"]["WU-03"]["status"],
        "complete"
    );
    assert_eq!(
        state["stages"]["red-tests"]["units"]["WU-01"]["status"],
        "pending"
    );
    let order: Vec<String> = (0..4).map(|_| step_of(&complete_next(root))).collect();
    assert_eq!(
        order,
        [
            "WU-01:red-tests",
            "WU-01:build-and-verify",
            "WU-02:red-tests",
            "WU-02:build-and-verify"
        ]
    );
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep.get("sweep").is_some(), "the sweep is offered again");
}

#[test]
fn rework_without_unit_on_ledger_keeps_unchanged_units() {
    let project = TempDir::new("rework-unchanged");
    let root = project.path();
    let design = Design::small_md(json!([{"id": "WU-01"}, {"id": "WU-02"}]));
    let id = seed_develop(root, "rework unchanged", &design);
    complete_next(root);
    complete_next(root); // WU-01 verified
    complete_next(root); // WU-02 red
    let build = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &build, None);
    block(root, "The design looks incomplete");
    let before = state_of(root, &id);
    rework(root, &["--detail", "Check the design"]);
    let directive = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &directive, Some(&design));
    report_complete(root, &directive);
    let reconciled = &events(root, "units.reconciled")[0];
    assert_eq!(reconciled["kept"], json!(["WU-01", "WU-02"]));
    assert_eq!(reconciled["reset"], json!([]));
    assert_eq!(reconciled["changed"], json!([]));
    let state = state_of(root, &id);
    for unit in ["WU-01", "WU-02"] {
        assert_eq!(
            state["workUnits"]["units"][unit]["status"],
            before["workUnits"]["units"][unit]["status"]
        );
        assert_eq!(
            state["workUnits"]["units"][unit]["steps"],
            before["workUnits"]["units"][unit]["steps"]
        );
    }
    assert_eq!(state["workUnits"]["units"]["WU-02"]["status"], "blocked");
    // The blocked unit is retried where it stopped.
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:build-and-verify");
    assert_eq!(next["stage"]["rework"]["attempt"], 2);
}

#[test]
fn rework_unit_is_validated_without_writes() {
    let project = TempDir::new("rework-validation");
    let root = project.path();
    let id = seed_develop(
        root,
        "rework validation",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}])),
    );
    complete_next(root); // WU-01 red
    let build = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &build, None);
    block(root, "Blocked on the design");
    let before = snapshot(root);
    for (args, expected) in [
        (
            vec!["--unit", "WU-09", "--detail", "x"],
            "Unknown work unit WU-09",
        ),
        (
            vec!["--unit", "WU-02", "--detail", "x"],
            "Design rework of WU-02 is only available when one of the named units is blocked or parked",
        ),
        (vec!["--unit", "WU-01,", "--detail", "x"], "--unit has an empty item"),
    ] {
        let error = fails(root, &[&["intent", "rework"][..], &args[..]].concat());
        assert!(error.contains(expected), "{args:?}: {error}");
        assert_eq!(snapshot(root), before, "{args:?} wrote nothing");
    }
    // A comma list and the legacy spelling name the same units, each once.
    let requested = rework(
        root,
        &[
            "--unit",
            "U-01,WU-02",
            "--unit",
            "WU-01",
            "--detail",
            "Both",
        ],
    );
    assert_eq!(requested["units"], json!(["WU-01", "WU-02"]));
    assert_eq!(requested["fromStage"], "build-and-verify");
    let before = snapshot(root);
    assert!(fails(
        root,
        &["intent", "rework", "--unit", "WU-01", "--detail", "again"]
    )
    .contains("already pending"));
    assert_eq!(snapshot(root), before);
    let state = state_of(root, &id);
    assert_eq!(
        state["workUnits"]["rework"]["units"],
        json!(["WU-01", "WU-02"])
    );

    // An intent without a work-unit ledger yet refuses --unit.
    let early = TempDir::new("rework-early");
    let root = early.path();
    walk_to_design(root, "early rework");
    let before = snapshot(root);
    let error = fails(
        root,
        &["intent", "rework", "--unit", "WU-01", "--detail", "x"],
    );
    assert!(
        error.contains("intent rework --unit needs work units"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn rework_unit_on_a_1_0_0_state_migrates_first() {
    let project = TempDir::new("rework-legacy");
    let root = project.path();
    let old_audit = seed_legacy(root);
    let before = snapshot(root);
    // Every unit migrates to pending: nothing is blocked, so the request is refused
    // and the migration is not written either.
    let error = fails(
        root,
        &["intent", "rework", "--unit", "WU-06", "--detail", "x"],
    );
    assert!(error.contains("Design rework of WU-06"), "{error}");
    assert_eq!(snapshot(root), before);
    // Park WU-01 (the current unit), then a scoped rework is accepted on the
    // migrated ledger; older audit rows stay untouched.
    park(
        root,
        &["--kind", "design-defect", "--detail", "Wrong split"],
    );
    let requested = rework(root, &["--unit", "U-01", "--detail", "Redesign"]);
    assert_eq!(requested["units"], json!(["WU-01"]));
    let audit = fs::read(root.join(".aidlc/audit.jsonl")).unwrap();
    assert!(audit.starts_with(&old_audit));
    assert_eq!(events(root, "units.migrated").len(), 1);
    let state = state_of(root, "unit-replay");
    assert_eq!(state["workUnits"]["rework"]["units"], json!(["WU-01"]));
    assert!(state.get("designReworkPending").is_none());
}

#[test]
fn rejected_release_gate_allows_scoped_rework() {
    let project = TempDir::new("rejected-scoped");
    let root = project.path();
    let design = Design::small_md(json!([{"id": "WU-01"}, {"id": "WU-02"}]));
    let id = seed_develop(root, "rejected scoped", &design);
    for _ in 0..4 {
        complete_next(root);
    }
    let sweep = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &sweep, None);
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--approval",
            "rejected",
            "--approved-by",
            "Owner",
            "--detail",
            "WU-02 needs another pass",
        ],
    );
    let requested = rework(
        root,
        &["--unit", "WU-02", "--detail", "Rework WU-02 as asked"],
    );
    assert_eq!(requested["fromStage"], "build-and-verify");
    assert_eq!(requested["units"], json!(["WU-02"]));
    let directive = json_ok(root, &["orchestrate", "next"]);
    // The design files are unchanged; the named unit is reset anyway.
    write_artifacts(root, &directive, Some(&design));
    report_complete(root, &directive);
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "verified");
    assert_eq!(state["workUnits"]["units"]["WU-02"]["status"], "pending");
    assert_eq!(
        events(root, "units.reconciled")[0]["reset"],
        json!(["WU-02"])
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
}

#[test]
fn rework_keeps_owner_decision_parks_on_dependents() {
    let project = TempDir::new("rework-parks");
    let root = project.path();
    let design = Design::small_md(json!([
        {"id": "WU-01"},
        {"id": "WU-02", "dependsOn": ["WU-01"]},
    ]));
    let id = seed_develop(root, "rework parks", &design);
    complete_next(root);
    complete_next(root); // WU-01 verified
    complete_next(root); // WU-02 red
    json_ok(root, &["orchestrate", "next"]); // WU-02 build-and-verify
    park(
        root,
        &[
            "--kind",
            "owner-decision",
            "--blocked-by",
            "CFG-17",
            "--detail",
            "Which retention period applies?",
        ],
    );
    // The parked dependant opens the rework; only WU-01's design changes.
    rework(root, &["--detail", "WU-01 needs a new interface"]);
    let directive = json_ok(root, &["orchestrate", "next"]);
    let redesign = Design {
        markdown: Some(change_section(
            design.markdown.as_deref().unwrap(),
            "WU-01",
            "- Interface: streams instead of buffers.",
        )),
        ..Design::small_md(json!([
            {"id": "WU-01"},
            {"id": "WU-02", "dependsOn": ["WU-01"]},
        ]))
    };
    write_artifacts(root, &directive, Some(&redesign));
    report_complete(root, &directive);
    let reconciled = &events(root, "units.reconciled")[0];
    assert_eq!(reconciled["reset"], json!(["WU-01", "WU-02"]));
    assert_eq!(reconciled["changed"], json!(["WU-01"]));
    let state = state_of(root, &id);
    let dependant = &state["workUnits"]["units"]["WU-02"];
    assert_eq!(dependant["status"], "parked");
    assert_eq!(dependant["park"]["kind"], "owner-decision");
    assert_eq!(dependant["park"]["resumeStatus"], "pending");
    assert_eq!(dependant["steps"]["red-tests"], Value::Null);
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "pending");
    // WU-01 runs again; the owner decision is still needed for WU-02.
    complete_next(root);
    complete_next(root);
    let held = stalled(root);
    assert_eq!(held["parked"][0]["unit"], "WU-02");
    json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-02",
            "--detail",
            "Keep 30 days",
            "--approved-by",
            "Owner",
        ],
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
}

#[test]
fn split_replaces_a_unit() {
    let project = TempDir::new("split");
    let root = project.path();
    let units = json!([
        {"id": "WU-01"},
        {"id": "WU-06", "dependsOn": ["WU-01"]},
        {"id": "WU-08", "dependsOn": ["WU-06"]},
    ]);
    let id = seed_develop(root, "split unit", &Design::small_md(units));
    complete_next(root);
    complete_next(root); // WU-01 verified
    complete_next(root); // WU-06 red
    let before = snapshot(root);
    for (args, expected) in [
        (
            vec!["split", "WU-01", "--into", "WU-01,WU-17", "--detail", "x"],
            "WU-01 is verified; a finished unit cannot be split",
        ),
        (
            vec!["split", "WU-06", "--into", "WU-17", "--detail", "x"],
            "at least two distinct work units",
        ),
        (
            vec!["split", "WU-06", "--into", "WU-06,WU-08", "--detail", "x"],
            "WU-08 already names a work unit",
        ),
        (
            vec!["split", "WU-06", "--into", "WU-06,TS-17", "--detail", "x"],
            "--into takes work unit ids",
        ),
        (
            vec!["split", "WU-99", "--into", "WU-06,WU-17", "--detail", "x"],
            "Unknown work unit WU-99",
        ),
        (
            vec!["split", "WU-06", "--detail", "x"],
            "Usage: intent split ID --into ID...",
        ),
    ] {
        let error = fails(root, &[&["intent"][..], &args[..]].concat());
        assert!(error.contains(expected), "{args:?}: {error}");
        assert_eq!(snapshot(root), before, "{args:?} wrote nothing");
    }
    let split = json_ok(
        root,
        &[
            "intent",
            "split",
            "WU-06",
            "--into",
            "WU-06",
            "--into",
            "WU-17",
            "--detail",
            "Too large",
        ],
    );
    assert_eq!(split["splitRequested"], true);
    assert_eq!(split["into"], json!(["WU-06", "WU-17"]));
    assert_eq!(split["fromStage"], "build-and-verify");
    let requested = events(root, "intent.split.requested");
    assert_eq!(requested[0]["unit"], "WU-06");
    assert_eq!(requested[0]["into"], json!(["WU-06", "WU-17"]));
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["rework"]["kind"], "split");
    assert!(
        fails(root, &["intent", "rework", "--detail", "x"]).contains("already pending"),
        "a second request waits for the first"
    );
    let directive = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(directive["stage"]["id"], "implementation-design");
    assert_eq!(
        directive["stage"]["rework"]["split"],
        json!({"unit": "WU-06", "into": ["WU-06", "WU-17"]})
    );
    assert!(directive["stage"]["rework"]["instruction"]
        .as_str()
        .unwrap()
        .starts_with("Replace WU-06 with WU-06, WU-17"));

    // Without WU-17 in work-units.json the report is refused and nothing is written.
    write_artifacts(
        root,
        &directive,
        Some(&Design::small_md(json!([
            {"id": "WU-01"},
            {"id": "WU-06", "dependsOn": ["WU-01"]},
            {"id": "WU-08", "dependsOn": ["WU-06"]},
        ]))),
    );
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(error.contains("needs WU-17 in work-units.json"), "{error}");
    assert_eq!(snapshot(root), before);

    let with_new = Design::small_md(json!([
        {"id": "WU-01"},
        {"id": "WU-06", "dependsOn": ["WU-01"]},
        {"id": "WU-17", "dependsOn": ["WU-06"]},
        {"id": "WU-08", "dependsOn": ["WU-06"]},
    ]));
    write_artifacts(root, &directive, Some(&with_new));
    report_complete(root, &directive);
    let reconciled = &events(root, "units.reconciled")[0];
    assert_eq!(reconciled["added"], json!(["WU-17"]));
    assert_eq!(reconciled["reset"], json!(["WU-06"]));
    assert_eq!(reconciled["kept"], json!(["WU-01", "WU-08"]));
    assert_eq!(
        reconciled["split"],
        json!({"unit": "WU-06", "into": ["WU-06", "WU-17"]})
    );
    let state = state_of(root, &id);
    let ledger = &state["workUnits"];
    assert_eq!(ledger["units"]["WU-17"]["status"], "pending");
    assert_eq!(ledger["units"]["WU-06"]["status"], "pending");
    assert_eq!(ledger["units"]["WU-06"]["reopened"][0]["by"], "split");
    assert_eq!(ledger["units"]["WU-01"]["status"], "verified");
    assert_eq!(ledger["order"], json!(["WU-01", "WU-06", "WU-17", "WU-08"]));
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-06:red-tests");
}

#[test]
fn split_can_drop_the_replaced_unit() {
    let project = TempDir::new("split-drop");
    let root = project.path();
    let units = json!([
        {"id": "WU-01"},
        {"id": "WU-06", "dependsOn": ["WU-01"]},
        {"id": "WU-08", "dependsOn": ["WU-06"]},
    ]);
    let id = seed_develop(root, "split drop", &Design::small_md(units));
    complete_next(root); // WU-01 red
    json_ok(
        root,
        &[
            "intent",
            "split",
            "WU-06",
            "--into",
            "WU-17,WU-18",
            "--detail",
            "Two halves",
        ],
    );
    let directive = json_ok(root, &["orchestrate", "next"]);
    // WU-06 is still listed although --into does not keep it: refused.
    write_artifacts(
        root,
        &directive,
        Some(&Design::small_md(json!([
            {"id": "WU-01"},
            {"id": "WU-06", "dependsOn": ["WU-01"]},
            {"id": "WU-17", "dependsOn": ["WU-01"]},
            {"id": "WU-18", "dependsOn": ["WU-17"]},
            {"id": "WU-08", "dependsOn": ["WU-06"]},
        ]))),
    );
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("replaces WU-06, but work-units.json still lists it"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    // A dependency on the dropped id is rejected by work-units.json validation.
    write_artifacts(
        root,
        &directive,
        Some(&Design::small_md(json!([
            {"id": "WU-01"},
            {"id": "WU-17", "dependsOn": ["WU-01"]},
            {"id": "WU-18", "dependsOn": ["WU-17"]},
            {"id": "WU-08", "dependsOn": ["WU-06"]},
        ]))),
    );
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(error.contains("depends on unknown unit WU-06"), "{error}");
    assert_eq!(snapshot(root), before);
    write_artifacts(
        root,
        &directive,
        Some(&Design::small_md(json!([
            {"id": "WU-01"},
            {"id": "WU-17", "dependsOn": ["WU-01"]},
            {"id": "WU-18", "dependsOn": ["WU-17"]},
            {"id": "WU-08", "dependsOn": ["WU-18"]},
        ]))),
    );
    report_complete(root, &directive);
    let reconciled = &events(root, "units.reconciled")[0];
    assert_eq!(reconciled["removed"], json!(["WU-06"]));
    assert_eq!(reconciled["added"], json!(["WU-17", "WU-18"]));
    let state = state_of(root, &id);
    let ledger = &state["workUnits"];
    assert_eq!(ledger["units"]["WU-06"]["removed"], true);
    assert_eq!(
        ledger["units"]["WU-06"]["splitInto"],
        json!(["WU-17", "WU-18"])
    );
    assert_eq!(ledger["units"]["WU-01"]["status"], "red", "WU-01 is kept");
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:build-and-verify");
    // A removed unit cannot be named again.
    assert!(fails(
        root,
        &[
            "intent",
            "split",
            "WU-08",
            "--into",
            "WU-08,WU-06",
            "--detail",
            "x"
        ]
    )
    .contains("WU-06 already names a work unit"));
}

#[test]
fn legacy_rework_marker_is_converted() {
    let project = TempDir::new("legacy-marker");
    let root = project.path();
    let design = Design::small_md(json!([{"id": "WU-01"}, {"id": "WU-02"}]));
    let id = seed_develop(root, "legacy marker", &design);
    complete_next(root);
    complete_next(root); // WU-01 verified
    complete_next(root); // WU-02 red
    let build = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &build, None);
    block(root, "Blocked by the design");
    // A 1.0.0 binary runs `intent rework` on the migrated state: it resets every unit
    // entry (without touching reportedAt), removes state.units and sets the marker.
    edit_state(root, &id, |state| {
        let at = later(state["workUnits"]["syncedAt"].as_str().unwrap(), 1);
        state["stages"]["implementation-design"]["status"] = json!("in_progress");
        state["stages"]["implementation-design"]["reworkRequestedAt"] = json!(at);
        for stage in ["red-tests", "build-and-verify"] {
            let row = &mut state["stages"][stage];
            row["status"] = json!("in_progress");
            row["reworkResetAt"] = json!(at);
            for entry in row["units"].as_object_mut().unwrap().values_mut() {
                entry["status"] = json!("pending");
            }
        }
        let map = state.as_object_mut().unwrap();
        map.shift_remove("units");
        map.shift_remove("currentUnit");
        map.insert("currentStage".into(), Value::Null);
        map.insert("designReworkPending".into(), json!(true));
        map.insert(
            "designRework".into(),
            json!({"requestedAt": at, "fromStage": "build-and-verify", "detail": "1.0.0 rework"}),
        );
    });
    let directive = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(directive["stage"]["id"], "implementation-design");
    assert_eq!(directive["stage"]["rework"]["units"], json!([]));
    assert_eq!(directive["stage"]["rework"]["detail"], "1.0.0 rework");
    let resynced = events(root, "units.resynced");
    assert_eq!(resynced.len(), 1);
    assert_eq!(resynced[0]["legacyRework"], true);
    let state = state_of(root, &id);
    assert!(state.get("designReworkPending").is_none());
    assert_eq!(state["workUnits"]["rework"]["units"], json!([]));
    assert_eq!(state["units"].as_array().unwrap().len(), 2);
    for stage in ["red-tests", "build-and-verify"] {
        assert!(state["stages"][stage]["units"].get("_").is_none());
    }
    // The ledger kept the real results; the unchanged redesign keeps both units.
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "verified");
    write_artifacts(root, &directive, Some(&design));
    report_complete(root, &directive);
    assert_eq!(events(root, "units.reconciled")[0]["reset"], json!([]));
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:build-and-verify");
    let state = state_of(root, &id);
    assert_eq!(
        state["stages"]["red-tests"]["units"]["WU-01"]["status"], "complete",
        "the mirrors show the kept results again"
    );
    assert_eq!(events(root, "units.resynced").len(), 1, "resynced once");
}

#[test]
fn legacy_rework_already_completed_by_1_0_0_is_reconciled() {
    let project = TempDir::new("legacy-completed");
    let root = project.path();
    let units = json!([
        {"id": "WU-01"},
        {"id": "WU-02", "dependsOn": ["WU-01"]},
        {"id": "WU-03", "dependsOn": ["WU-02"]},
    ]);
    let design = Design::small_md(units.clone());
    let id = seed_develop(root, "legacy completed", &design);
    for _ in 0..4 {
        complete_next(root); // WU-01 and WU-02 verified
    }
    let red = json_ok(root, &["orchestrate", "next"]);
    write_artifacts(root, &red, None);
    block(root, "WU-03 needs a different WU-02 interface");
    // 1.0.0: `intent rework`, then `next` (clears the marker and the request), then
    // implementation-design complete (WU-02's section changed), then `next` selects
    // red-tests for WU-01 again.
    let paths = design_paths(root, &id);
    fs::write(
        &paths.markdown,
        change_section(
            design.markdown.as_deref().unwrap(),
            "WU-02",
            "- Interface: returns a cursor.",
        ),
    )
    .unwrap();
    edit_state(root, &id, |state| {
        let synced = state["workUnits"]["syncedAt"].as_str().unwrap().to_owned();
        let (requested, completed) = (later(&synced, 1), later(&synced, 2));
        let design_row = &mut state["stages"]["implementation-design"];
        design_row["status"] = json!("complete");
        design_row["reworkRequestedAt"] = json!(requested);
        design_row["reportedAt"] = json!(completed);
        for stage in ["red-tests", "build-and-verify"] {
            let row = &mut state["stages"][stage];
            row["status"] = json!("in_progress");
            row["reworkResetAt"] = json!(requested);
            for entry in row["units"].as_object_mut().unwrap().values_mut() {
                entry["status"] = json!("pending");
            }
        }
        state["units"] = json!([
            {"id": "WU-01", "status": "in_progress", "dependsOn": []},
            {"id": "WU-02", "status": "in_progress", "dependsOn": ["WU-01"]},
            {"id": "WU-03", "status": "in_progress", "dependsOn": ["WU-02"]},
        ]);
        state["currentStage"] = json!("red-tests");
        state["currentUnit"] = json!("WU-01");
    });
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
    let resynced = events(root, "units.resynced");
    assert_eq!(resynced.len(), 1);
    assert_eq!(resynced[0]["legacyRework"], true);
    let reconciled = events(root, "units.reconciled");
    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0]["changed"], json!(["WU-02"]));
    assert_eq!(reconciled[0]["reset"], json!(["WU-02", "WU-03"]));
    assert_eq!(reconciled[0]["kept"], json!(["WU-01"]));
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["units"]["WU-01"]["status"], "verified");
    assert!(state["workUnits"]["rework"].is_null());
    assert_eq!(
        state["stages"]["build-and-verify"]["units"]["WU-01"]["status"],
        "complete"
    );
    // The next command finds nothing newer.
    complete_next(root);
    assert_eq!(events(root, "units.resynced").len(), 1);
}

struct DesignPaths {
    markdown: PathBuf,
}

fn design_paths(root: &Path, id: &str) -> DesignPaths {
    DesignPaths {
        markdown: root.join(format!(
            ".aidlc/intents/{id}/develop/implementation-design/work-units.md"
        )),
    }
}

#[test]
fn stale_ledger_is_resynced() {
    let project = TempDir::new("stale-ledger");
    let root = project.path();
    let id = seed_develop(
        root,
        "stale ledger",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&red), "WU-01:red-tests");
    // A 1.0.0 binary reports WU-01's red step complete (no unit tag, no ledger write).
    edit_state(root, &id, |state| {
        let at = later(state["workUnits"]["syncedAt"].as_str().unwrap(), 1);
        state["stages"]["red-tests"]["units"]["WU-01"] =
            json!({"status": "complete", "reportedAt": at});
        state["stages"]["red-tests"]["reports"] =
            json!([{"at": at, "result": "complete", "detail": null}]);
        state["currentStage"] = Value::Null;
    });
    // The read-only queue shows the re-synced view and writes nothing.
    let before = snapshot(root);
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(queue["ready"][0]["unit"], "WU-01");
    assert_eq!(queue["ready"][0]["step"], "build-and-verify");
    assert_eq!(snapshot(root), before);
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:build-and-verify");
    let resynced = events(root, "units.resynced");
    assert_eq!(resynced.len(), 1);
    assert_eq!(resynced[0]["units"], json!(["WU-01"]));
    assert_eq!(resynced[0]["legacyRework"], false);
    assert_eq!(resynced[0]["legacyGate"], false);
    let state = state_of(root, &id);
    let unit = &state["workUnits"]["units"]["WU-01"];
    assert_eq!(unit["status"], "red");
    assert_eq!(unit["steps"]["red-tests"], "complete");
    assert_eq!(
        unit["history"].as_array().unwrap().last().unwrap()["via"],
        "1.0.0"
    );
    json_ok(root, &["orchestrate", "next"]);
    assert_eq!(events(root, "units.resynced").len(), 1, "no second resync");
}

#[test]
fn legacy_release_gate_is_accepted() {
    let project = TempDir::new("legacy-gate");
    let root = project.path();
    let id = seed_develop(
        root,
        "legacy gate",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    for _ in 0..4 {
        complete_next(root);
    }
    let state = state_of(root, &id);
    assert_eq!(
        state["stages"]["build-and-verify"]["units"]["WU-02"]["status"], "pending",
        "the gate unit is projected as unfinished"
    );
    // 1.0.0 selects the projected gate unit and records the release-candidate gate.
    let mut gate = Value::Null;
    edit_state(root, &id, |state| {
        let at = later(state["workUnits"]["syncedAt"].as_str().unwrap(), 1);
        state["stages"]["build-and-verify"]["units"]["WU-02"] =
            json!({"status": "complete", "reportedAt": at});
        state["stages"]["build-and-verify"]["status"] = json!("complete");
        state["units"][1]["status"] = json!("complete");
        gate = json!({
            "phase": "develop", "stage": "build-and-verify", "decision": "approved",
            "result": "complete", "by": "Owner", "bySource": "flag", "mode": "human",
            "at": at, "note": null, "detail": null, "skippedStages": [], "artifacts": []
        });
        state["gates"]["release-candidate"] = gate.clone();
        state["currentStage"] = Value::Null;
    });
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_ne!(next["stage"]["phase"], "develop", "Develop is finished");
    let resynced = events(root, "units.resynced");
    assert_eq!(resynced.len(), 1);
    assert_eq!(resynced[0]["legacyGate"], true);
    let state = state_of(root, &id);
    assert_eq!(state["workUnits"]["sweep"]["status"], "green");
    assert_eq!(state["workUnits"]["sweep"]["via"], "1.0.0-gate");
    for unit in ["WU-01", "WU-02"] {
        assert_eq!(state["workUnits"]["units"][unit]["status"], "done");
    }
    assert_eq!(
        state["gates"]["release-candidate"], gate,
        "the gate row is untouched"
    );
    assert_eq!(state["stages"]["build-and-verify"]["status"], "complete");
}

// ---------------------------------------------------------------------------
// M5: doctor early warnings, wave suggestion, end-to-end scenarios and replay
// ---------------------------------------------------------------------------

/// `doctor`'s rows as `(status, message)`; doctor never writes.
fn doctor_rows(root: &Path) -> Vec<(String, String)> {
    let output = run(root, &["doctor"]);
    let report: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["status"].as_str().unwrap().to_owned(),
                row["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn warned(rows: &[(String, String)], needle: &str) -> Vec<String> {
    rows.iter()
        .filter(|(status, message)| status == "warn" && message.contains(needle))
        .map(|(_, message)| message.clone())
        .collect()
}

/// Workflow failures doctor reports for an intent.
fn intent_failures(rows: &[(String, String)]) -> Vec<String> {
    rows.iter()
        .filter(|(status, message)| status == "fail" && message.starts_with("Intent "))
        .map(|(_, message)| message.clone())
        .collect()
}

const REAL_WAVES: &str = "1) WU-01, WU-02, WU-03, WU-04, WU-07, WU-11; 2) WU-05, WU-16, WU-06, WU-08, WU-09, WU-12; 3) WU-10, WU-13, WU-14, WU-15";

fn set_config(root: &Path, key: &str, value: Value) {
    let path = root.join(".aidlc/config.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config[key] = value;
    write(&path, &serde_json::to_string_pretty(&config).unwrap());
}

#[test]
fn doctor_reports_inverted_dependency_waves_and_unassigned_tests() {
    // The 1.0.0 state with the real graph: WU-06 finished red (a prior result kept by
    // the migration) while WU-16, which it depends on, has done nothing.
    let project = TempDir::new("doctor-early");
    let root = project.path();
    seed_legacy(root);
    let before = snapshot(root);
    let rows = doctor_rows(root);
    assert_eq!(snapshot(root), before, "doctor never writes");
    assert_eq!(intent_failures(&rows), Vec::<String>::new());
    let inverted = warned(&rows, "the dependency may be inverted");
    assert_eq!(
        inverted,
        ["Intent unit-replay: WU-06 dependsOn WU-16, but WU-06 already has finished work (red-tests 2026-11-24T17:06:28.829Z) and WU-16 has none; the dependency may be inverted. Prefer attributed regression (deferred failures in regression.json) to reordering."]
    );
    assert_eq!(
        warned(&rows, "exceed the split threshold"),
        [format!(
            "Intent unit-replay: 16 work units exceed the split threshold 6. Suggested waves: {REAL_WAVES}. Or split the intent."
        )]
    );
    assert_eq!(
        warned(&rows, "registered test(s) are not owned"),
        ["Intent unit-replay: 1 registered test(s) are not owned by any work unit: TS-54 (add them to \"tests\" in work-units.json)"]
    );
    assert!(warned(&rows, "is listed before its dependency").is_empty());
    assert!(warned(&rows, "no test ownership is declared").is_empty());
    // The stored ledger keeps the warnings after the migration.
    json_ok(root, &["orchestrate", "next"]);
    let rows = doctor_rows(root);
    assert_eq!(warned(&rows, "the dependency may be inverted").len(), 1);
    assert!(
        warned(&rows, "older than").is_empty(),
        "the ledger is fresh"
    );
    // A higher split threshold drops the wave suggestion.
    set_config(root, "unitSplitThreshold", json!(16));
    assert!(warned(&doctor_rows(root), "Suggested waves").is_empty());

    // Array order that contradicts dependsOn: WU-08 is listed before WU-16.
    let project = TempDir::new("doctor-order");
    let root = project.path();
    seed_develop(root, "doctor order", &Design::uninverted());
    let rows = doctor_rows(root);
    assert_eq!(
        warned(&rows, "is listed before its dependency")
            .iter()
            .map(|message| message.split_once(": ").unwrap().1.to_owned())
            .collect::<Vec<_>>(),
        ["WU-08 is listed before its dependency WU-16 in work-units.json"]
    );
    assert!(
        warned(&rows, "may be inverted").is_empty(),
        "nothing has finished yet"
    );
    assert_eq!(intent_failures(&rows), Vec::<String>::new());

    // No declared ownership at all (the owner's real intent).
    let project = TempDir::new("doctor-unowned");
    let root = project.path();
    seed_develop(
        root,
        "doctor unowned",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02", "dependsOn": ["WU-01"]}])),
    );
    let rows = doctor_rows(root);
    assert_eq!(warned(&rows, "no test ownership is declared").len(), 1);
    assert!(warned(&rows, "registered test(s) are not owned").is_empty());
    assert!(warned(&rows, "split threshold").is_empty());
}

#[test]
fn first_unit_directive_suggests_waves_beside_the_split_warning() {
    let project = TempDir::new("waves");
    let root = project.path();
    seed_develop(root, "waves", &Design::real());
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&red), "WU-01:red-tests");
    let warnings: Vec<&str> = red["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_str().unwrap())
        .collect();
    assert_eq!(
        warnings,
        [
            "This intent has 16 work units, more than the split threshold of 6. Consider asking the owner to split it into separate intents (for example backend, then API/UI) before building.",
            &format!("Suggested waves of at most 6 units: {REAL_WAVES}. The CLI runs units in dependency order either way; the owner may split the intent along these waves, or review the work wave by wave."),
        ]
    );
    // The harness gets the unit's commands.
    assert_eq!(
        red["unit"]["commands"],
        json!({
            "report": "dtech-aidlc orchestrate report --unit WU-01 --result complete|blocked|skipped",
            "park": "dtech-aidlc orchestrate park --unit WU-01 --kind dep-unit|foreign-regression|owner-decision|design-defect [--blocked-by ID]... --detail \"<reason>\"",
            "queue": "dtech-aidlc orchestrate queue",
        })
    );
    // Later directives carry no wave line.
    write_artifacts(root, &red, None);
    report_complete(root, &red);
    let build = json_ok(root, &["orchestrate", "next"]);
    assert!(!build["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row.as_str().unwrap().contains("Suggested waves")));
}

#[test]
fn doctor_reports_parks_cycles_unconfirmed_decisions_and_stale_ledger() {
    let project = TempDir::new("doctor-parks");
    let root = project.path();
    let id = seed_develop(
        root,
        "doctor parks",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    park(
        root,
        &[
            "--kind",
            "owner-decision",
            "--blocked-by",
            "CFG-17",
            "--detail",
            "Retention needs an owner choice",
        ],
    );
    let at = state_of(root, &id)["workUnits"]["units"]["WU-01"]["park"]["at"]
        .as_str()
        .unwrap()
        .to_owned();
    let rows = doctor_rows(root);
    assert_eq!(
        warned(&rows, "is parked"),
        [format!(
            "Intent {id}: WU-01 is parked (owner-decision) since {at}: Retention needs an owner choice"
        )]
    );
    json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-01",
            "--detail",
            "Keep 30 days",
            "--approved-by",
            "AI Orchestrator",
        ],
    );
    let rows = doctor_rows(root);
    assert!(warned(&rows, "is parked").is_empty());
    assert_eq!(
        warned(&rows, "await owner confirmation"),
        [format!("Intent {id}: 1 owner decision(s) were recorded by AI Orchestrator and await owner confirmation: WU-01 (Keep 30 days)")]
    );

    // A park whose blocker depends on the parked unit (possible after a hand edit or
    // a reconcile that changed dependsOn; `park` itself refuses it).
    park(
        root,
        &[
            "--unit",
            "WU-02",
            "--kind",
            "dep-unit",
            "--blocked-by",
            "WU-03",
            "--detail",
            "Needs WU-03",
        ],
    );
    let parked = state_of(root, &id);
    edit_state(root, &id, |state| {
        state["workUnits"]["units"]["WU-03"]["dependsOn"] = json!(["WU-02"]);
    });
    let rows = doctor_rows(root);
    assert_eq!(
        warned(&rows, "waits on"),
        [format!(
            "Intent {id}: parked WU-02 waits on WU-03, which depends on WU-02"
        )]
    );
    // Two units parked on each other.
    edit_state(root, &id, |state| {
        *state = parked.clone();
        let mut park = state["workUnits"]["units"]["WU-02"]["park"].clone();
        park["blockedBy"] = json!(["WU-02"]);
        park["blockedByUnits"] = json!(["WU-02"]);
        park["resumeStatus"] = json!("pending");
        state["workUnits"]["units"]["WU-03"]["park"] = park;
        state["workUnits"]["units"]["WU-03"]["status"] = json!("parked");
    });
    let rows = doctor_rows(root);
    let cycles = warned(&rows, "waits on");
    assert!(
        cycles.contains(&format!(
            "Intent {id}: parked WU-02 waits on WU-03, which waits on WU-02"
        )),
        "{cycles:?}"
    );
    assert!(
        cycles.contains(&format!(
            "Intent {id}: parked WU-03 waits on WU-02, which waits on WU-03"
        )),
        "{cycles:?}"
    );

    // A ledger that an older dtech-aidlc changed afterwards.
    edit_state(root, &id, |state| {
        *state = parked.clone();
        let at = later(state["workUnits"]["syncedAt"].as_str().unwrap(), 1);
        state["stages"]["red-tests"]["units"]["WU-01"] =
            json!({"status": "complete", "reportedAt": at});
    });
    let before = snapshot(root);
    let rows = doctor_rows(root);
    assert_eq!(snapshot(root), before, "doctor never writes");
    assert_eq!(
        warned(&rows, "older than"),
        [format!("Intent {id}: work-unit ledger is older than stage records for WU-01 (written by an older dtech-aidlc); it is re-synced by the next orchestrate command")]
    );
    json_ok(root, &["orchestrate", "next"]);
    assert!(warned(&doctor_rows(root), "older than").is_empty());
}

/// Reports per unit (repeating stages) and the share of skipped reports whose detail
/// starts with "Skip redundant" (plan section 5 metrics).
fn unit_metrics(state: &Value) -> (std::collections::BTreeMap<String, usize>, usize, usize) {
    let mut per_unit = std::collections::BTreeMap::new();
    let (mut skipped, mut redundant) = (0, 0);
    for stage in ["red-tests", "build-and-verify"] {
        for report in state["stages"][stage]["reports"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            if let Some(unit) = report["unit"].as_str() {
                *per_unit.entry(unit.to_owned()).or_insert(0) += 1;
            }
            if report["result"] == "skipped" {
                skipped += 1;
                if report["detail"]
                    .as_str()
                    .is_some_and(|detail| detail.starts_with("Skip redundant"))
                {
                    redundant += 1;
                }
            }
        }
    }
    (per_unit, skipped, redundant)
}

/// Plan section 5, scenarios 1 to 3 chained in one intent through to `complete: true`:
/// the WU-06/WU-16 replay (park, move on, reopen), attributed regression, auto-park on
/// a repeated blocker with the stalled hold, a scoped rework that replays nothing, the
/// final sweep and the metrics.
#[test]
fn end_to_end_unit_scheduling() {
    let project = TempDir::new("end-to-end");
    let root = project.path();
    let design = Design::uninverted();
    let id = seed_develop(root, "end to end", &design);
    let mut steps = Vec::new();
    for _ in 0..11 {
        steps.push(step_of(&complete_next(root)));
    }
    assert_eq!(steps[9], "WU-05:build-and-verify");
    assert_eq!(steps[10], "WU-06:red-tests");

    // Scenario 1: WU-05 done, WU-16 not started, WU-06's lifecycle tests are red.
    let build = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&build), "WU-06:build-and-verify");
    let parked = park(
        root,
        &[
            "--kind",
            "foreign-regression",
            "--blocked-by",
            "TS-20",
            "--detail",
            "13 lifecycle tests owned by WU-16 fail",
        ],
    );
    assert_eq!(parked["unit"], "WU-06");
    assert_eq!(parked["blockedByUnits"], json!(["WU-16"]));

    // Scenario 3: the CLI moves on; WU-07 repeats one blocker and is auto-parked on
    // the third report instead of being selected with a growing attempt.
    assert_eq!(step_of(&complete_next(root)), "WU-07:red-tests");
    let mut outputs = Vec::new();
    for detail in ["port in use", "port still in use", "port conflict again"] {
        let next = json_ok(root, &["orchestrate", "next"]);
        assert_eq!(step_of(&next), "WU-07:build-and-verify");
        outputs.push(block(root, detail));
    }
    assert!(outputs[1].get("parked").is_none());
    assert_eq!(outputs[2]["parked"]["auto"], true);
    assert_eq!(outputs[2]["parked"]["count"], 3);

    // WU-11 defers a failure owned by WU-16 (attributed regression).
    assert_eq!(step_of(&complete_next(root)), "WU-11:red-tests");
    let build = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&build), "WU-11:build-and-verify");
    write_artifacts(root, &build, None);
    write_failures(
        root,
        &build,
        json!([{
            "test": "suite_a::tests::ts20_case",
            "suite": "normal",
            "owner": "WU-16",
        }]),
    );
    let reported = json_ok(root, &complete_args());
    assert_eq!(reported["regression"]["deferred"], 1);
    assert_eq!(
        state_of(root, &id)["workUnits"]["units"]["WU-11"]["status"],
        "green"
    );
    assert_eq!(step_of(&complete_next(root)), "WU-16:red-tests");
    assert_eq!(step_of(&complete_next(root)), "WU-16:build-and-verify");
    let state = state_of(root, &id);
    assert!(
        open_items(&state).is_empty(),
        "WU-16's clean run resolves it"
    );
    assert_eq!(state["workUnits"]["units"]["WU-11"]["status"], "verified");

    // WU-16 is done: WU-06 comes back by itself at its build step.
    let back = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&back), "WU-06:build-and-verify");
    assert_eq!(back["stage"]["rework"]["revisit"]["reason"], "blocker-done");
    write_artifacts(root, &back, None);
    report_complete(root, &back);

    // Only WU-07's owner decision is left: the queue stalls, and nothing is written.
    let before = snapshot(root);
    let hold = stalled(root);
    assert_eq!(hold["reason"], "queue-stalled");
    assert_eq!(units_of(&hold["parked"]), ["WU-07"]);
    assert_eq!(hold["parked"][0]["kind"], "owner-decision");
    assert!(units_of(&hold["waiting"]).contains(&"WU-08".to_owned()));
    assert_eq!(snapshot(root), before);
    let rows = doctor_rows(root);
    assert_eq!(warned(&rows, "WU-07 is parked (owner-decision)").len(), 1);
    json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-07",
            "--detail",
            "Use a fixed test port",
            "--approved-by",
            "Owner",
        ],
    );
    let resumed = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&resumed), "WU-07:build-and-verify");
    assert_eq!(resumed["stage"]["rework"]["attempt"], 4);
    write_artifacts(root, &resumed, None);
    report_complete(root, &resumed);
    assert_eq!(step_of(&complete_next(root)), "WU-08:red-tests");
    assert_eq!(step_of(&complete_next(root)), "WU-08:build-and-verify");

    // Scenario 2: WU-09's design is wrong; a scoped rework replays nothing else.
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&red), "WU-09:red-tests");
    park(
        root,
        &[
            "--kind",
            "design-defect",
            "--detail",
            "The scheduler contract cannot hold",
        ],
    );
    let kept = [
        "WU-01", "WU-02", "WU-03", "WU-04", "WU-05", "WU-06", "WU-07", "WU-08", "WU-11", "WU-16",
    ];
    let before = state_of(root, &id);
    let reports_before = unit_report_count(&before, &kept);
    rework(
        root,
        &["--unit", "WU-09", "--detail", "Redesign the scheduler"],
    );
    let directive = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(directive["stage"]["id"], "implementation-design");
    assert_eq!(directive["stage"]["rework"]["units"], json!(["WU-09"]));
    let markdown = change_section(
        &change_status_line(design.markdown.as_deref().unwrap(), "WU-02", "verified"),
        "WU-09",
        "- Contract: one scheduler tick per lease.",
    );
    let redesign = Design {
        markdown: Some(markdown),
        ..Design::uninverted()
    };
    write_artifacts(root, &directive, Some(&redesign));
    report_complete(root, &directive);
    let reconciled = events(root, "units.reconciled");
    assert_eq!(reconciled.last().unwrap()["reset"], json!(["WU-09"]));
    let state = state_of(root, &id);
    for unit in kept {
        assert_eq!(
            state["workUnits"]["units"][unit]["status"],
            before["workUnits"]["units"][unit]["status"],
            "{unit} keeps its status"
        );
    }
    assert_eq!(unit_report_count(&state, &kept), reports_before);

    // The rest runs in dependency order, then the final sweep carries the gate.
    let mut rest = Vec::new();
    let mut sweep = Value::Null;
    let complete = loop {
        let next = json_ok(root, &["orchestrate", "next"]);
        if next["complete"] == json!(true) {
            break next;
        }
        assert!(rest.len() < 40, "the intent never completed: {rest:?}");
        if next["sweep"].is_object() {
            assert_eq!(next["stage"]["phaseGate"], true);
            assert!(next.get("currentUnit").is_none());
            sweep = next.clone();
        }
        rest.push(step_of(&next));
        write_artifacts(root, &next, None);
        report_complete(root, &next);
    };
    assert_eq!(
        rest[..13],
        [
            "WU-09:red-tests",
            "WU-09:build-and-verify",
            "WU-10:red-tests",
            "WU-10:build-and-verify",
            "WU-12:red-tests",
            "WU-12:build-and-verify",
            "WU-13:red-tests",
            "WU-13:build-and-verify",
            "WU-14:red-tests",
            "WU-14:build-and-verify",
            "WU-15:red-tests",
            "WU-15:build-and-verify",
            "-:build-and-verify",
        ]
    );
    // After the sweep only the stages after Develop remain.
    assert!(rest[13..]
        .iter()
        .all(|step| step.starts_with("-:") && !step.ends_with("build-and-verify")));
    assert!(sweep.is_object(), "the final sweep was selected");
    assert_eq!(sweep["sweep"]["ownerDecisions"][0]["unit"], "WU-07");
    assert_eq!(sweep["sweep"]["ownerDecisions"][0]["confirmed"], true);
    assert_eq!(complete["complete"], true);
    assert_eq!(complete["units"]["byStatus"], json!({"done": 16}));
    assert_eq!(
        complete["ownerDecisions"][0]["decision"],
        "Use a fixed test port"
    );
    assert_eq!(complete["deferredRegressions"][0]["status"], "resolved");
    let gate = events(root, "gate.decided")
        .into_iter()
        .find(|row| row["gate"] == "release-candidate")
        .expect("release-candidate gate");
    assert_eq!(gate["ownerDecisions"][0]["unit"], "WU-07");
    assert_eq!(intent_failures(&doctor_rows(root)), Vec::<String>::new());

    // Metrics (plan section 5.5).
    let state = state_of(root, &id);
    let (per_unit, skipped, redundant) = unit_metrics(&state);
    println!(
        "reports per unit: {per_unit:?}; skipped reports: {skipped}; skip-redundant: {redundant}"
    );
    assert_eq!(per_unit.len(), 16);
    for (unit, count) in &per_unit {
        let expected = match unit.as_str() {
            "WU-06" => 3, // red, park, build
            "WU-07" => 5, // red, three blocked (the third auto-parks), build
            "WU-09" => 3, // red park, then red and build after the rework
            _ => 2,       // happy path: red and build
        };
        assert_eq!(*count, expected, "{unit}");
    }
    assert_eq!(skipped, 0);
    assert_eq!(redundant, 0, "no skip-redundant replay");
}

/// Copies the files of a project's active intent that a replay needs (config without
/// prerequisites, the state pointer, the audit log and the intent folder; no
/// credentials or `.env` files) from `source` into `target`. Returns the intent id.
fn copy_replay_project(source: &Path, target: &Path) -> String {
    fn skip(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower.starts_with(".env")
            || lower.contains("credential")
            || lower.contains("secret")
            || lower == ".git"
    }
    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type().unwrap();
            if skip(&name) || kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                copy_tree(&entry.path(), &to.join(&name));
            } else if kind.is_file() {
                fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let aidlc = source.join(".aidlc");
    let pointer: Value =
        serde_json::from_str(&fs::read_to_string(aidlc.join("state.json")).unwrap()).unwrap();
    let id = pointer["activeIntentId"]
        .as_str()
        .expect("an active intent")
        .to_owned();
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(aidlc.join("config.json")).unwrap()).unwrap();
    // No owner check command runs in the replay.
    config.as_object_mut().unwrap().remove("prerequisites");
    write(
        &target.join(".aidlc/config.json"),
        &serde_json::to_string_pretty(&config).unwrap(),
    );
    fs::copy(aidlc.join("state.json"), target.join(".aidlc/state.json")).unwrap();
    if aidlc.join("audit.jsonl").is_file() {
        fs::copy(aidlc.join("audit.jsonl"), target.join(".aidlc/audit.jsonl")).unwrap();
    }
    copy_tree(
        &aidlc.join("intents").join(&id),
        &target.join(".aidlc/intents").join(&id),
    );
    id
}

/// Replay against a copy of a real in-flight 1.0.0 intent. Set
/// `DTECH_AIDLC_REPLAY_PROJECT` to the project root; the active intent is copied into
/// a temporary folder (the source is only read) and the unit-aware CLI runs there:
/// read-only `status`, `queue` and `doctor`, then the migrating `orchestrate next`, and
/// on the first unit's build step the real foreign failures (`regression.wu06.json`,
/// owned by WU-16) are deferred instead of blocking. Skipped when the variable is unset.
#[test]
fn replay_of_a_real_intent_copied_to_a_temp_fixture() {
    let Some(source) = std::env::var_os("DTECH_AIDLC_REPLAY_PROJECT") else {
        println!(
            "skipped: set DTECH_AIDLC_REPLAY_PROJECT to a project root to replay its active intent"
        );
        return;
    };
    let project = TempDir::new("real-replay");
    let root = project.path();
    let id = copy_replay_project(Path::new(&source), root);
    let state_path = root.join(format!(".aidlc/intents/{id}/state.json"));
    let original: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    let audit_before = fs::read(root.join(".aidlc/audit.jsonl")).unwrap_or_default();
    let units: Value = serde_json::from_str(
        &fs::read_to_string(root.join(format!(
            ".aidlc/intents/{id}/develop/implementation-design/work-units.json"
        )))
        .unwrap(),
    )
    .unwrap();
    let total = units["units"].as_array().unwrap().len();

    // Read-only views preview the migration and write nothing.
    let before = snapshot(root);
    let status = json_ok(root, &["status"]);
    let queue = json_ok(root, &["orchestrate", "queue"]);
    let rows = doctor_rows(root);
    assert_eq!(snapshot(root), before, "read-only commands never write");
    assert_eq!(status["units"]["total"], total);
    println!("status units: {}", status["units"]["byStatus"]);
    println!("queue ready: {}", queue["ready"]);
    for (status, message) in rows.iter().filter(|(_, m)| m.starts_with("Intent ")) {
        println!("doctor {status}: {message}");
    }
    assert_eq!(intent_failures(&rows), Vec::<String>::new());

    // The first orchestrate command migrates: only `workUnits` is added, the audit log
    // keeps its bytes as a prefix and gains one units.migrated row.
    let first = json_ok(root, &["orchestrate", "next"]);
    println!("first directive: {}", step_of(&first));
    let migrated = events(root, "units.migrated");
    if original.get("workUnits").is_none() {
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0]["units"], total);
        println!("units.migrated: {}", migrated[0]);
    }
    let audit_after = fs::read(root.join(".aidlc/audit.jsonl")).unwrap();
    assert!(
        audit_after.starts_with(&audit_before),
        "audit rows untouched"
    );
    let state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    assert_subset(&original, &state, "", &|path: &str| {
        path == "/updatedAt"
            || path == "/currentStage"
            || path == "/currentUnit"
            || path == "/units"
            || path.starts_with("/stages/red-tests")
            || path.starts_with("/stages/build-and-verify")
    });

    // Attributed regression on the real failures: deferred to WU-16 while it has not
    // run, instead of blocking the unit that reports them.
    let wu16_done = state["workUnits"]["units"]["WU-16"]["status"]
        .as_str()
        .is_some_and(|status| matches!(status, "green" | "verified" | "done"));
    let mut directive = first;
    if directive["stage"]["id"] == "red-tests" && !wu16_done {
        write_artifacts(root, &directive, None);
        report_complete(root, &directive);
        directive = json_ok(root, &["orchestrate", "next"]);
    }
    if directive["stage"]["id"] == "build-and-verify"
        && directive["currentUnit"].is_string()
        && !wu16_done
    {
        let unit = directive["currentUnit"].as_str().unwrap().to_owned();
        write_artifacts(root, &directive, None);
        let failures: Value = serde_json::from_str(&fixture("regression.wu06.json")).unwrap();
        write_failures(root, &directive, failures["failures"].clone());
        let reported = json_ok(root, &complete_args());
        assert_eq!(reported["regression"]["deferred"], 16);
        let state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(state["workUnits"]["units"][&unit]["status"], "green");
        let rows = doctor_rows(root);
        assert_eq!(
            warned(
                &rows,
                "16 failing test(s) already in the tree belong to WU-16"
            )
            .len(),
            1
        );
        let next = json_ok(root, &["orchestrate", "next"]);
        println!("after deferring 16 failures on {unit}: {}", step_of(&next));
        assert_ne!(next["currentUnit"], json!(unit), "the CLI moved on");
    }
}

// ---------------------------------------------------------------------------
// Review regressions
// ---------------------------------------------------------------------------

#[test]
fn skipped_design_while_a_ledger_rework_is_pending_is_refused() {
    let project = TempDir::new("skip-rework");
    let root = project.path();
    let id = seed_develop(
        root,
        "skip rework",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02", "dependsOn": ["WU-01"]}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    block(root, "the interface cannot hold");
    rework(root, &["--unit", "WU-01", "--detail", "Fix the interface"]);
    let design = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(design["stage"]["id"], "implementation-design");
    let before = snapshot(root);
    let error = fails(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "skipped",
            "--detail",
            "nothing to change",
        ],
    );
    assert!(
        error.contains("implementation-design cannot be skipped while a design rework"),
        "{error}"
    );
    assert_eq!(snapshot(root), before, "a refused skip writes nothing");
    // Completing the design with unchanged files reconciles the pending request.
    write_artifacts(root, &design, None);
    report_complete(root, &design);
    let state = state_of(root, &id);
    assert!(
        state["workUnits"]["rework"].is_null(),
        "the request is consumed"
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-01:red-tests");
    // The queue commands work again.
    park(
        root,
        &["--kind", "owner-decision", "--detail", "Owner must choose"],
    );
}

#[test]
fn report_on_a_parked_unit_selected_by_an_older_binary_is_refused() {
    let project = TempDir::new("parked-current");
    let root = project.path();
    let id = seed_develop(
        root,
        "parked current",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    json_ok(root, &["orchestrate", "next"]);
    park(
        root,
        &["--kind", "owner-decision", "--detail", "need owner"],
    );
    let second = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&second), "WU-02:red-tests");
    // A 1.0.0 `orchestrate next` moves only the pointer, here to the parked WU-01.
    edit_state(root, &id, |state| {
        state["currentStage"] = json!("red-tests");
        state["currentUnit"] = json!("WU-01");
    });
    write_artifacts(root, &second, None);
    let before = snapshot(root);
    let error = fails(root, &complete_args());
    assert!(
        error.contains("No current stage; run orchestrate next first")
            && error.contains("WU-01 is parked (owner-decision)"),
        "{error}"
    );
    assert_eq!(
        snapshot(root),
        before,
        "nothing is recorded on a parked unit"
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
    report_complete(root, &next);
    complete_next(root); // WU-02 build
    assert_eq!(stalled(root)["parked"][0]["unit"], "WU-01");
    // A unit whose status says done while its park record is still open counts as
    // parked: the selector does not offer the sweep, and the release gate refuses.
    let park_record = state_of(root, &id)["workUnits"]["units"]["WU-01"]["park"].clone();
    assert!(park_record.is_object());
    edit_state(root, &id, |state| {
        let unit = &mut state["workUnits"]["units"]["WU-01"];
        unit["status"] = json!("green");
        unit["steps"] = json!({"red-tests": "complete", "build-and-verify": "complete"});
    });
    assert_eq!(stalled(root)["parked"][0]["unit"], "WU-01");
    edit_state(root, &id, |state| {
        state["workUnits"]["units"]["WU-01"]["park"] = Value::Null;
    });
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep["sweep"].is_object());
    write_artifacts(root, &sweep, None);
    edit_state(root, &id, |state| {
        state["workUnits"]["units"]["WU-01"]["park"] = park_record;
    });
    let before = snapshot(root);
    let error = fails(root, &approve_args());
    assert!(
        error.contains("needs no parked work unit; parked: WU-01 (owner-decision)"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn ai_mode_owner_decisions_are_recorded_as_unconfirmed() {
    let project = TempDir::new("ai-decision");
    let root = project.path();
    let id = seed_develop(
        root,
        "ai decision",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}])),
    );
    edit_state(root, &id, |state| state["mode"] = json!("ai"));
    json_ok(root, &["orchestrate", "next"]);
    park(
        root,
        &["--kind", "owner-decision", "--detail", "second decision"],
    );
    let before = snapshot(root);
    let error = fails(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-01",
            "--approved-by",
            "Owner",
            "--detail",
            "x",
        ],
    );
    assert!(error.contains("AI orchestrator mode"), "{error}");
    assert_eq!(snapshot(root), before);
    let revisit = json_ok(
        root,
        &[
            "orchestrate",
            "revisit",
            "--unit",
            "WU-01",
            "--detail",
            "agent decided",
        ],
    );
    assert_eq!(revisit["revisited"][0]["decidedBy"], "AI Orchestrator");
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(queue["ownerDecisions"][0]["decidedBy"], "AI Orchestrator");
    assert_eq!(queue["ownerDecisions"][0]["confirmed"], false);
    let rows = events(root, "unit.revisited");
    assert_eq!(rows[0]["decidedBy"], "AI Orchestrator");
}

#[test]
fn env_prerequisite_holds_only_its_unit() {
    let project = TempDir::new("env-hold");
    let root = project.path();
    seed_develop(
        root,
        "env hold",
        &Design::small(json!([{"id": "WU-01"}, {"id": "WU-02"}, {"id": "WU-03"}])),
    );
    json_ok(
        root,
        &["config", "prerequisite", "set", "vpn", "--check", "exit 1"],
    );
    json_ok(root, &["orchestrate", "next"]);
    json_ok(
        root,
        &[
            "orchestrate",
            "report",
            "--result",
            "blocked",
            "--needs",
            "vpn",
            "--detail",
            "vpn is down",
        ],
    );
    let next = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&next), "WU-02:red-tests");
    assert!(
        next["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("WU-01 is held")),
        "{}",
        next["warnings"]
    );
    let held = events(root, "prerequisite.checked");
    assert_eq!(held.last().unwrap()["unit"], "WU-01");
    write_artifacts(root, &next, None);
    report_complete(root, &next);
    let mut steps = Vec::new();
    for _ in 0..3 {
        steps.push(step_of(&complete_next(root)));
    }
    assert_eq!(
        steps,
        [
            "WU-02:build-and-verify",
            "WU-03:red-tests",
            "WU-03:build-and-verify"
        ]
    );
    // Only the held unit is left: next holds it with exit 3, as before.
    let hold = stalled(root);
    assert_eq!(hold["stage"], "red-tests");
    assert_eq!(hold["currentUnit"], "WU-01");
    assert!(hold["hint"].as_str().unwrap().contains("needs vpn"));
    json_ok(
        root,
        &["config", "prerequisite", "set", "vpn", "--check", "exit 0"],
    );
    let back = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(step_of(&back), "WU-01:red-tests");
    assert_eq!(back["stage"]["rework"]["needs"], json!(["vpn"]));
}

#[test]
fn sweep_summary_that_contradicts_its_failures_is_refused() {
    let project = TempDir::new("sweep-summary");
    let root = project.path();
    let id = seed_develop(
        root,
        "sweep summary",
        &Design::small(json!([{"id": "WU-01"}])),
    );
    complete_next(root);
    complete_next(root);
    let sweep = json_ok(root, &["orchestrate", "next"]);
    assert!(sweep["sweep"].is_object());
    write_artifacts(root, &sweep, None);
    let body = json!({
        "schemaVersion": 1, "unit": "sweep", "scope": "full", "commands": ["x"],
        "failures": [], "summary": {"passed": 1, "failed": 3},
    });
    write(&regression_path(root, &sweep), &format!("{body}\n"));
    let before = snapshot(root);
    let error = fails(root, &approve_args());
    assert!(
        error.contains("summary.failed is 3, but failures lists no failing test"),
        "{error}"
    );
    assert_eq!(snapshot(root), before);
    assert_ne!(
        state_of(root, &id)["gates"]["release-candidate"]["decision"],
        "approved"
    );
}

#[test]
fn migrated_prior_results_are_shown_by_queue_and_doctor() {
    let project = TempDir::new("prior-results");
    let root = project.path();
    seed_legacy(root);
    let queue = json_ok(root, &["orchestrate", "queue"]);
    let prior = queue["priorResults"].as_array().unwrap();
    assert_eq!(prior.len(), 12, "{}", queue["priorResults"]);
    assert!(prior.contains(&json!({
        "unit": "WU-06", "stage": "red-tests", "result": "complete",
        "at": "2026-11-24T17:06:28.829Z"
    })));
    let rows = doctor_rows(root);
    let shown = warned(&rows, "results from before a 1.0.0 whole-intent rework");
    assert_eq!(shown.len(), 1, "{rows:?}");
    assert!(
        shown[0].contains("WU-06 red-tests complete"),
        "{}",
        shown[0]
    );
    // The migrated ledger shows the same list.
    json_ok(root, &["orchestrate", "next"]);
    let queue = json_ok(root, &["orchestrate", "queue"]);
    assert_eq!(queue["priorResults"].as_array().unwrap().len(), 12);
}
