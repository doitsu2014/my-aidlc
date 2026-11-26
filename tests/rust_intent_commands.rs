//! Parity tests for the native `intent` lifecycle commands
//! (`dtech_aidlc::intent_commands::run`) against the Node reference CLI
//! (`node core/tools/dtech-aidlc.mjs`).
//!
//! Every scenario builds the same fixture twice, runs Node on one copy and the
//! native handler on the other, then compares the JSON output (or the error text and
//! exit code), every file under `.aidlc` and every `audit.jsonl` row (ignoring `at`).
//! Timestamps, the corrupt-pointer quarantine stamp and the temp project path are
//! normalised. When `node` is not on PATH the Node half is skipped and only the
//! native assertions run.

use dtech_aidlc::args::{command_key, parse_args};
use dtech_aidlc::intent_commands;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dtech-aidlc-intent-{label}-{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The outcome of one command: `Ok(json)` or `Err((message, exit code))`.
type Outcome = Result<Value, (String, i32)>;

fn run_node(project: &Path, argv: &[&str]) -> Outcome {
    let output = Command::new("node")
        .arg(repo().join("core/tools/dtech-aidlc.mjs"))
        .args(argv)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run node");
    let code = output.status.code().unwrap_or(-1);
    if code == 0 {
        Ok(
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "node stdout is not JSON ({error}): {}",
                    String::from_utf8_lossy(&output.stdout)
                )
            }),
        )
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .trim_end()
            .strip_prefix("dtech-aidlc: ")
            .unwrap_or(&stderr)
            .to_owned();
        Err((message, code))
    }
}

fn run_rust(project: &Path, argv: &[&str]) -> Outcome {
    let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_owned()).collect();
    let parsed = parse_args(&argv).expect("parse args");
    let key = command_key(&parsed.positional).expect("command key");
    let words = key.split(' ').count();
    let args = parsed.positional[words.min(parsed.positional.len())..].to_vec();
    match intent_commands::run(&key, project, &args, &parsed.flags) {
        Ok((value, 0)) => Ok(value),
        Ok((value, code)) => panic!("unexpected exit code {code} with {value}"),
        Err(message) => Err((message, 1)),
    }
}

fn is_iso_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 24
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            19 => *byte == b'.',
            23 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

fn normalise(value: Value) -> Value {
    match value {
        Value::String(text) if is_iso_timestamp(&text) => json!("<ts>"),
        Value::Array(items) => Value::Array(items.into_iter().map(normalise).collect()),
        // `mtimeMs` only appears in onboarding's source snapshot (fixture setup).
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let v = if k == "mtimeMs" {
                        json!("<mtime>")
                    } else {
                        normalise(v)
                    };
                    (k, v)
                })
                .collect(),
        ),
        other => other,
    }
}

fn normalise_outcome(outcome: Outcome, project: &Path) -> Outcome {
    let shown = project.display().to_string();
    match outcome {
        Ok(value) => Ok(normalise(value)),
        Err((message, code)) => Err((message.replace(&shown, "<project>"), code)),
    }
}

/// Every file under `.aidlc`, keyed by its normalised relative path. JSON files are
/// compared as normalised JSON (key order included); `audit.jsonl` as rows without `at`.
fn snapshot(project: &Path) -> BTreeMap<String, Value> {
    fn walk(base: &Path, dir: &Path, project: &Path, out: &mut BTreeMap<String, Value>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.map(Result::unwrap) {
            let path = entry.path();
            let relative = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                out.insert(format!("{relative}/"), Value::Null);
                walk(base, &path, project, out);
                continue;
            }
            let relative = match relative.find("state.json.corrupt-") {
                Some(index) => format!("{}state.json.corrupt-<stamp>", &relative[..index]),
                None => relative,
            };
            let text = String::from_utf8_lossy(&fs::read(&path).unwrap())
                .replace(&project.display().to_string(), "<project>")
                .replace(
                    &project.display().to_string().replace('\\', "/"),
                    "<project>",
                )
                .replace(
                    &project.display().to_string().replace('\\', "\\\\"),
                    "<project>",
                );
            let value = if relative.ends_with("audit.jsonl") {
                Value::Array(
                    text.lines()
                        .map(|line| {
                            let mut row: Value = serde_json::from_str(line).unwrap();
                            row.as_object_mut().unwrap().shift_remove("at");
                            normalise(row)
                        })
                        .collect(),
                )
            } else if relative.ends_with(".json") {
                serde_json::from_str::<Value>(&text)
                    .map(normalise)
                    .map(|value| json!({"json": value, "raw-trailing-newline": text.ends_with('\n')}))
                    .unwrap_or(Value::String(text))
            } else {
                Value::String(text)
            };
            out.insert(relative, value);
        }
    }
    let mut out = BTreeMap::new();
    let base = project.join(".aidlc");
    walk(&base, &base, project, &mut out);
    out
}

/// Build the fixture twice, run Node on one and the native handler on the other, and
/// assert identical outcomes and `.aidlc` trees. Returns the native outcome.
fn compare(label: &str, build: impl Fn(&Path), argv: &[&str]) -> Outcome {
    let rust_dir = TempDir::new(&format!("{label}-rs"));
    build(&rust_dir.0);
    let rust = run_rust(&rust_dir.0, argv);
    if node_available() {
        let node_dir = TempDir::new(&format!("{label}-js"));
        build(&node_dir.0);
        let node = run_node(&node_dir.0, argv);
        assert_eq!(
            normalise_outcome(rust.clone(), &rust_dir.0),
            normalise_outcome(node, &node_dir.0),
            "{label}: output differs for {argv:?}"
        );
        assert_eq!(
            snapshot(&rust_dir.0),
            snapshot(&node_dir.0),
            "{label}: .aidlc differs for {argv:?}"
        );
    }
    rust
}

/// Apply a sequence of commands to both copies, comparing after each step.
fn compare_steps(label: &str, build: impl Fn(&Path), steps: &[&[&str]]) -> Vec<Outcome> {
    let rust_dir = TempDir::new(&format!("{label}-rs"));
    build(&rust_dir.0);
    let node_dir = node_available().then(|| {
        let dir = TempDir::new(&format!("{label}-js"));
        build(&dir.0);
        dir
    });
    let mut outcomes = Vec::new();
    for argv in steps {
        let rust = run_rust(&rust_dir.0, argv);
        if let Some(node_dir) = &node_dir {
            let node = run_node(&node_dir.0, argv);
            assert_eq!(
                normalise_outcome(rust.clone(), &rust_dir.0),
                normalise_outcome(node, &node_dir.0),
                "{label}: output differs for {argv:?}"
            );
            assert_eq!(
                snapshot(&rust_dir.0),
                snapshot(&node_dir.0),
                "{label}: .aidlc differs after {argv:?}"
            );
        }
        outcomes.push(rust);
    }
    outcomes
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const STAMP: &str = "2026-01-02T03:04:05.000Z";

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
    )
    .unwrap();
}

fn write_pointer(project: &Path, id: Option<&str>) {
    write_json(
        &project.join(".aidlc/state.json"),
        &json!({"version": 2, "activeIntentId": id, "updatedAt": STAMP}),
    );
}

fn write_intent(project: &Path, id: &str, state: Value) {
    let dir = project.join(".aidlc/intents").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("intent.md"), format!("# {id}\n\nDo {id}\n")).unwrap();
    write_json(&dir.join("state.json"), &state);
}

fn intent_state(id: &str, stages: Value) -> Value {
    json!({
        "version": 2,
        "intentId": id,
        "intent": format!("Do {id}"),
        "scope": "solo",
        "mode": "human",
        "status": "active",
        "currentStage": "context-and-risk",
        "stages": stages,
        "dependencies": {},
        "createdAt": STAMP,
        "updatedAt": STAMP,
    })
}

fn active_stages() -> Value {
    json!({"problem-framing": {"status": "complete", "detail": null}})
}

const SOLO_STAGES: &[&str] = &[
    "problem-framing",
    "context-and-risk",
    "option-generation",
    "direction-selection",
    "implementation-design",
    "red-tests",
    "build-and-verify",
    "intent-record",
    "knowledge-writeback",
];

fn complete_stages() -> Value {
    Value::Object(
        SOLO_STAGES
            .iter()
            .map(|id| {
                (
                    (*id).to_owned(),
                    json!({"status": "complete", "detail": null}),
                )
            })
            .collect(),
    )
}

/// One active intent `alpha` that the pointer names, with an audit log.
fn one_active(project: &Path) {
    write_intent(project, "alpha", intent_state("alpha", active_stages()));
    write_pointer(project, Some("alpha"));
    fs::write(
        project.join(".aidlc/audit.jsonl"),
        "{\"at\":\"2026-01-02T03:04:05.000Z\",\"event\":\"intent.created\",\"intentId\":\"alpha\"}\n",
    )
    .unwrap();
}

/// `alpha` active (pointer), `beta` paused, `done` complete, `gone` abandoned.
fn many_intents(project: &Path) {
    one_active(project);
    write_intent(project, "beta", intent_state("beta", active_stages()));
    let mut done = intent_state("done", complete_stages());
    done["currentStage"] = Value::Null;
    write_intent(project, "done", done);
    let mut gone = intent_state("gone", active_stages());
    gone["closed"] = json!({"kind": "abandoned", "at": STAMP, "detail": "old"});
    gone["currentStage"] = Value::Null;
    write_intent(project, "gone", gone);
}

fn legacy_v1(project: &Path) {
    write_json(
        &project.join(".aidlc/state.json"),
        &json!({
            "intent": "Legacy export work",
            "scope": "solo",
            "currentStage": "context-and-risk",
            "stages": {"problem-framing": {"status": "complete"}},
            "createdAt": STAMP,
            "project": "legacy",
        }),
    );
}

/// A Develop stage blocked after design: `red-tests` last reported `blocked`.
fn blocked_develop(project: &Path, status: &str, reset_at: Option<&str>) {
    let mut stages = serde_json::Map::new();
    for id in &SOLO_STAGES[..5] {
        stages.insert((*id).into(), json!({"status": "complete", "detail": null}));
    }
    let mut red = json!({
        "status": status,
        "detail": "split needs revision",
        "reports": [
            {"result": "complete", "at": "2026-01-02T03:00:00.000Z"},
            {"result": "blocked", "at": "2026-01-02T03:04:05.000Z", "detail": "split needs revision"},
        ],
        "units": {"U-01": {"status": "complete"}, "U-02": {"status": "blocked", "detail": "x"}},
    });
    if let Some(reset_at) = reset_at {
        red["reworkResetAt"] = json!(reset_at);
    }
    stages.insert("red-tests".into(), red);
    stages.insert(
        "build-and-verify".into(),
        json!({"status": "in_progress", "detail": null, "units": {"U-01": {"status": "complete"}}}),
    );
    let mut state = intent_state("alpha", Value::Object(stages));
    state["currentStage"] = json!("red-tests");
    state["units"] =
        json!([{"id": "U-01", "status": "complete"}, {"id": "U-02", "status": "in_progress"}]);
    state["currentUnit"] = json!("U-02");
    write_intent(project, "alpha", state);
    write_pointer(project, Some("alpha"));
}

fn assert_err(outcome: &Outcome, expected: &str) {
    match outcome {
        Err((message, code)) => {
            assert_eq!(message, expected);
            assert_eq!(*code, 1);
        }
        Ok(value) => panic!("expected error '{expected}', got {value}"),
    }
}

// ---------------------------------------------------------------------------
// intent depth
// ---------------------------------------------------------------------------

#[test]
fn depth_changes_active_intent_and_audits() {
    let out = compare(
        "depth-ok",
        one_active,
        &[
            "intent",
            "depth",
            "standard",
            "--approved-by",
            " Owner ",
            "--detail",
            "pilot",
        ],
    );
    assert_eq!(
        out.unwrap(),
        json!({"intentId": "alpha", "from": "full", "to": "standard"})
    );
    compare_steps(
        "depth-twice",
        one_active,
        &[
            &[
                "intent",
                "depth",
                "fast",
                "--approved-by",
                "Owner",
                "--detail",
                "a",
            ],
            &[
                "intent",
                "depth",
                "full",
                "--approved-by",
                "Owner",
                "--detail",
                "b",
            ],
        ],
    );
}

#[test]
fn depth_migrates_legacy_state_first() {
    let out = compare(
        "depth-legacy",
        legacy_v1,
        &[
            "intent",
            "depth",
            "fast",
            "--approved-by",
            "Owner",
            "--detail",
            "x",
        ],
    );
    assert_eq!(out.unwrap()["intentId"], json!("legacy-export-work"));
}

#[test]
fn depth_error_paths_match_node() {
    assert_err(
        &compare("depth-usage", one_active, &["intent", "depth"]),
        "Usage: intent depth NAME --approved-by NAME --detail TEXT",
    );
    assert_err(
        &compare(
            "depth-unknown",
            one_active,
            &[
                "intent",
                "depth",
                "turbo",
                "--approved-by",
                "O",
                "--detail",
                "x",
            ],
        ),
        "Unknown depth 'turbo'. Known depths: full, standard, fast",
    );
    assert_err(
        &compare(
            "depth-none",
            |_| {},
            &[
                "intent",
                "depth",
                "fast",
                "--approved-by",
                "O",
                "--detail",
                "x",
            ],
        ),
        "No active intent",
    );
    assert_err(
        &compare(
            "depth-paused",
            |p| {
                one_active(p);
                write_pointer(p, None);
            },
            &[
                "intent",
                "depth",
                "fast",
                "--approved-by",
                "O",
                "--detail",
                "x",
            ],
        ),
        "No active intent",
    );
    assert_err(
        &compare(
            "depth-no-approver",
            one_active,
            &["intent", "depth", "fast", "--detail", "x"],
        ),
        "intent depth requires --approved-by NAME",
    );
    assert_err(
        &compare(
            "depth-no-detail",
            one_active,
            &["intent", "depth", "fast", "--approved-by", "O"],
        ),
        "intent depth requires --detail TEXT",
    );
    let corrupt = compare(
        "depth-corrupt",
        |p| {
            one_active(p);
            fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
        },
        &[
            "intent",
            "depth",
            "fast",
            "--approved-by",
            "O",
            "--detail",
            "x",
        ],
    );
    assert_err(
        &corrupt,
        ".aidlc/state.json cannot be parsed. No changes were made. Intents on disk: alpha. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
    );
    assert_err(
        &compare(
            "depth-missing-state",
            |p| write_pointer(p, Some("ghost")),
            &["intent", "depth", "fast", "--approved-by", "O", "--detail", "x"],
        ),
        "Active intent 'ghost' has no state file at .aidlc/intents/ghost/state.json. No changes were made. Run 'dtech-aidlc intent list', then 'dtech-aidlc intent resume <id>' or 'dtech-aidlc intent pause'.",
    );
}

/// Node persists the new depth before rejecting an invalid `--approved-by`; the native
/// handler validates first and leaves state unchanged. Only the error text is shared.
#[test]
fn depth_invalid_approver_fails_without_writing() {
    let dir = TempDir::new("depth-bad-approver");
    one_active(&dir.0);
    let before = snapshot(&dir.0);
    let argv = [
        "intent",
        "depth",
        "fast",
        "--approved-by",
        "bad\u{1}name",
        "--detail",
        "x",
    ];
    let rust = run_rust(&dir.0, &argv);
    assert_err(
        &rust,
        "--approved-by must be 1 to 200 characters without control characters",
    );
    assert_eq!(snapshot(&dir.0), before);
    if node_available() {
        let node_dir = TempDir::new("depth-bad-approver-js");
        one_active(&node_dir.0);
        assert_eq!(run_node(&node_dir.0, &argv), rust);
    }
}

// ---------------------------------------------------------------------------
// intent rework
// ---------------------------------------------------------------------------

#[test]
fn rework_returns_blocked_develop_stage_to_design() {
    let out = compare(
        "rework-blocked-status",
        |p| blocked_develop(p, "blocked", None),
        &[
            "intent",
            "rework",
            "--detail",
            "  Owner authorized splitting WU-06 ",
        ],
    );
    assert_eq!(
        out.unwrap(),
        json!({"reworkRequested": true, "intentId": "alpha", "fromStage": "red-tests", "nextStage": "implementation-design"})
    );
    // Status in_progress, but the last report (after no reset) was blocked.
    compare(
        "rework-blocked-report",
        |p| blocked_develop(p, "in_progress", None),
        &["intent", "rework", "--detail", "again"],
    )
    .unwrap();
    // A reset older than the blocked report still counts the report.
    compare(
        "rework-report-after-reset",
        |p| blocked_develop(p, "in_progress", Some("2026-01-02T03:01:00.000Z")),
        &["intent", "rework", "--detail", "again"],
    )
    .unwrap();
    // Twice in a row: the second request finds no blocked stage.
    let steps = compare_steps(
        "rework-twice",
        |p| blocked_develop(p, "blocked", None),
        &[
            &["intent", "rework", "--detail", "first"],
            &["intent", "rework", "--detail", "second"],
        ],
    );
    assert_err(
        &steps[1],
        "Design rework is only available after a blocked Develop stage",
    );
}

#[test]
fn rework_error_paths_match_node() {
    assert_err(
        &compare(
            "rework-usage",
            |p| blocked_develop(p, "blocked", None),
            &["intent", "rework"],
        ),
        "Usage: intent rework --detail \"<reason>\"",
    );
    assert_err(
        &compare(
            "rework-blank",
            |p| blocked_develop(p, "blocked", None),
            &["intent", "rework", "--detail", "   "],
        ),
        "intent rework requires --detail \"<reason>\"",
    );
    assert_err(
        &compare(
            "rework-none",
            |_| {},
            &["intent", "rework", "--detail", "x"],
        ),
        "No active workflow state",
    );
    assert_err(
        &compare(
            "rework-not-blocked",
            one_active,
            &["intent", "rework", "--detail", "x"],
        ),
        "Design rework is only available after a blocked Develop stage",
    );
    assert_err(
        &compare(
            "rework-reset-after-report",
            |p| blocked_develop(p, "in_progress", Some("2026-01-02T04:00:00.000Z")),
            &["intent", "rework", "--detail", "x"],
        ),
        "Design rework is only available after a blocked Develop stage",
    );
    assert_err(
        &compare(
            "rework-complete",
            |p| {
                write_intent(p, "done", intent_state("done", complete_stages()));
                write_pointer(p, Some("done"));
            },
            &["intent", "rework", "--detail", "x"],
        ),
        "Design rework requires an active intent",
    );
}

// ---------------------------------------------------------------------------
// intent assign
// ---------------------------------------------------------------------------

#[test]
fn assign_matches_node() {
    let out = compare("assign-ok", one_active, &["intent", "assign", "  Dana  "]);
    assert_eq!(
        out.unwrap(),
        json!({"intentId": "alpha", "assignee": "Dana"})
    );
    compare("assign-legacy", legacy_v1, &["intent", "assign", "Dana"]).unwrap();
    assert_err(
        &compare("assign-usage", one_active, &["intent", "assign"]),
        "Usage: intent assign NAME",
    );
    assert_err(
        &compare("assign-blank", one_active, &["intent", "assign", "   "]),
        "intent assign requires a name",
    );
    assert_err(
        &compare("assign-none", |_| {}, &["intent", "assign", "Dana"]),
        "No active intent",
    );
}

// ---------------------------------------------------------------------------
// intent pause
// ---------------------------------------------------------------------------

#[test]
fn pause_matches_node() {
    let out = compare(
        "pause-ok",
        one_active,
        &["intent", "pause", "--detail", "lunch"],
    );
    assert_eq!(
        out.unwrap(),
        json!({"paused": true, "intentId": "alpha", "resumeWith": "dtech-aidlc intent resume alpha"})
    );
    compare("pause-no-detail", one_active, &["intent", "pause"]).unwrap();
    // A complete intent named by the pointer can still be paused (Node keeps it loaded).
    compare(
        "pause-complete",
        |p| {
            write_intent(p, "done", intent_state("done", complete_stages()));
            write_pointer(p, Some("done"));
        },
        &["intent", "pause"],
    )
    .unwrap();
    // The pointer names an intent whose state file is missing: pause clears it.
    let ghost = compare(
        "pause-ghost",
        |p| write_pointer(p, Some("ghost")),
        &["intent", "pause"],
    );
    assert_eq!(ghost.unwrap()["intentId"], json!("ghost"));
    let legacy = compare("pause-legacy", legacy_v1, &["intent", "pause"]);
    assert_eq!(legacy.unwrap()["intentId"], json!("legacy-export-work"));
}

#[test]
fn pause_error_paths_match_node() {
    for (label, build) in [
        ("pause-none", (|_: &Path| {}) as fn(&Path)),
        ("pause-paused", |p: &Path| {
            one_active(p);
            write_pointer(p, None);
        }),
        ("pause-abandoned", |p: &Path| {
            many_intents(p);
            write_pointer(p, Some("gone"));
        }),
    ] {
        assert_err(
            &compare(label, build, &["intent", "pause"]),
            "No unfinished active intent to pause",
        );
    }
    let corrupt = compare(
        "pause-corrupt",
        |p| {
            one_active(p);
            fs::write(p.join(".aidlc/state.json"), "[1]").unwrap();
        },
        &["intent", "pause"],
    );
    assert_err(
        &corrupt,
        ".aidlc/state.json is not a recognised state file. No changes were made. Intents on disk: alpha. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
    );
    let invalid = compare(
        "pause-invalid-intent",
        |p| {
            let mut state = intent_state("alpha", active_stages());
            state["scope"] = json!("galaxy");
            write_intent(p, "alpha", state);
            write_pointer(p, Some("alpha"));
        },
        &["intent", "pause"],
    );
    assert_err(
        &invalid,
        ".aidlc/intents/alpha/state.json is invalid: scope is not a key of graph.scopes. No changes were made.",
    );
}

// ---------------------------------------------------------------------------
// intent resume
// ---------------------------------------------------------------------------

#[test]
fn resume_matches_node() {
    let steps = compare_steps(
        "resume-cycle",
        many_intents,
        &[
            &["intent", "pause", "--detail", "switch"],
            &["intent", "resume", "beta"],
            &["intent", "resume", "beta"],
            &["intent", "resume", "alpha"],
            &["intent", "pause"],
            &["intent", "resume", "alpha"],
        ],
    );
    assert_eq!(
        steps[1].clone().unwrap(),
        json!({"resumed": true, "intentId": "beta", "currentStage": "context-and-risk"})
    );
    assert_err(
        &steps[3],
        "Intent 'beta' is active; pause or finish it first",
    );
    // currentStage missing from the state file is omitted from the output.
    let out = compare(
        "resume-no-current",
        |p| {
            let mut state = intent_state("alpha", active_stages());
            state.as_object_mut().unwrap().shift_remove("currentStage");
            write_intent(p, "alpha", state);
        },
        &["intent", "resume", "alpha"],
    );
    assert_eq!(out.unwrap(), json!({"resumed": true, "intentId": "alpha"}));
    // The pointer names a missing intent: resume replaces it.
    compare(
        "resume-over-ghost",
        |p| {
            many_intents(p);
            write_pointer(p, Some("ghost"));
        },
        &["intent", "resume", "beta"],
    )
    .unwrap();
    // The pointer names a complete intent: it is not "active", so resume proceeds.
    compare(
        "resume-over-complete",
        |p| {
            many_intents(p);
            write_pointer(p, Some("done"));
        },
        &["intent", "resume", "beta"],
    )
    .unwrap();
    // Legacy state migrates first, then the migrated intent blocks another resume.
    let legacy = compare_steps(
        "resume-legacy",
        |p| {
            legacy_v1(p);
            write_intent(p, "beta", intent_state("beta", active_stages()));
        },
        &[
            &["intent", "resume", "beta"],
            &["intent", "resume", "legacy-export-work"],
        ],
    );
    assert_err(
        &legacy[0],
        "Intent 'legacy-export-work' is active; pause or finish it first",
    );
}

#[test]
fn resume_quarantines_a_corrupt_pointer() {
    let out = compare(
        "resume-corrupt",
        |p| {
            many_intents(p);
            fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
        },
        &["intent", "resume", "beta"],
    );
    assert_eq!(out.unwrap()["resumed"], json!(true));
    // Quarantine happens even when the target is then rejected.
    let rejected = compare(
        "resume-corrupt-unknown",
        |p| {
            many_intents(p);
            fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
        },
        &["intent", "resume", "nope"],
    );
    assert_err(
        &rejected,
        "Unknown intent 'nope'. Run 'dtech-aidlc intent list'.",
    );
}

#[test]
fn resume_error_paths_match_node() {
    assert_err(
        &compare("resume-usage", many_intents, &["intent", "resume"]),
        "Usage: intent resume <id>",
    );
    for id in ["nope", "../alpha", "Alpha"] {
        assert_err(
            &compare("resume-unknown", many_intents, &["intent", "resume", id]),
            &format!("Unknown intent '{id}'. Run 'dtech-aidlc intent list'."),
        );
    }
    assert_err(
        &compare(
            "resume-complete",
            many_intents,
            &["intent", "resume", "done"],
        ),
        "Intent 'done' is complete and cannot be resumed",
    );
    assert_err(
        &compare(
            "resume-abandoned",
            many_intents,
            &["intent", "resume", "gone"],
        ),
        "Intent 'gone' was abandoned and cannot be resumed",
    );
    assert_err(
        &compare(
            "resume-other-active",
            many_intents,
            &["intent", "resume", "beta"],
        ),
        "Intent 'alpha' is active; pause or finish it first",
    );
    assert_err(
        &compare(
            "resume-invalid-active",
            |p| {
                many_intents(p);
                let mut state = intent_state("alpha", active_stages());
                state["stages"] = json!({"problem-framing": {"status": "done"}});
                write_intent(p, "alpha", state);
            },
            &["intent", "resume", "beta"],
        ),
        ".aidlc/intents/alpha/state.json is invalid: stage problem-framing has an invalid status. No changes were made.",
    );
}

// ---------------------------------------------------------------------------
// intent abandon
// ---------------------------------------------------------------------------

#[test]
fn abandon_matches_node() {
    let out = compare(
        "abandon-active",
        many_intents,
        &["intent", "abandon", "--detail", "dropped"],
    );
    assert_eq!(
        out.unwrap(),
        json!({"abandoned": true, "intentId": "alpha"})
    );
    // Naming the active intent clears the pointer; naming another keeps it.
    compare(
        "abandon-named-active",
        many_intents,
        &["intent", "abandon", "alpha", "--detail", "dropped"],
    )
    .unwrap();
    compare(
        "abandon-named-other",
        many_intents,
        &["intent", "abandon", "beta", "--detail", "dropped"],
    )
    .unwrap();
    let steps = compare_steps(
        "abandon-then",
        many_intents,
        &[
            &["intent", "abandon", "--detail", "dropped"],
            &["intent", "abandon", "--detail", "again"],
            &["intent", "resume", "alpha"],
            &["intent", "resume", "beta"],
        ],
    );
    assert_err(&steps[1], "No active intent; name the intent to abandon");
    assert_err(
        &steps[2],
        "Intent 'alpha' was abandoned and cannot be resumed",
    );
    compare(
        "abandon-legacy",
        legacy_v1,
        &["intent", "abandon", "--detail", "old"],
    )
    .unwrap();
}

#[test]
fn abandon_error_paths_match_node() {
    assert_err(
        &compare("abandon-usage", many_intents, &["intent", "abandon"]),
        "intent abandon requires --detail \"<reason>\"",
    );
    assert_err(
        &compare(
            "abandon-none",
            |_| {},
            &["intent", "abandon", "--detail", "x"],
        ),
        "No active intent; name the intent to abandon",
    );
    assert_err(
        &compare(
            "abandon-ghost",
            |p| write_pointer(p, Some("ghost")),
            &["intent", "abandon", "--detail", "x"],
        ),
        "No active intent; name the intent to abandon",
    );
    assert_err(
        &compare(
            "abandon-unknown",
            many_intents,
            &["intent", "abandon", "nope", "--detail", "x"],
        ),
        "Unknown intent 'nope'. Run 'dtech-aidlc intent list'.",
    );
    assert_err(
        &compare(
            "abandon-complete",
            many_intents,
            &["intent", "abandon", "done", "--detail", "x"],
        ),
        "Intent 'done' is already complete",
    );
    assert_err(
        &compare(
            "abandon-again",
            many_intents,
            &["intent", "abandon", "gone", "--detail", "x"],
        ),
        "Intent 'gone' is already abandoned",
    );
    assert_err(
        &compare(
            "abandon-corrupt",
            |p| {
                many_intents(p);
                fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
            },
            &["intent", "abandon", "beta", "--detail", "x"],
        ),
        ".aidlc/state.json cannot be parsed. No changes were made. Intents on disk: alpha, beta, done, gone. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
    );
}

// ---------------------------------------------------------------------------
// Unknown actions, onboarded project, safety
// ---------------------------------------------------------------------------

#[test]
fn unknown_action_matches_node() {
    let expected =
        "Use intent list, intent pause, intent resume, intent abandon, intent depth, or intent assign";
    assert_err(
        &compare("action-missing", one_active, &["intent"]),
        expected,
    );
    assert_err(
        &compare("action-unknown", one_active, &["intent", "frobnicate"]),
        expected,
    );
}

#[test]
fn lifecycle_on_onboarded_project_matches_node() {
    if !node_available() {
        return;
    }
    let build = |project: &Path| {
        for argv in [
            &["onboard", "--harness", "claude", "--scope", "solo"][..],
            &["orchestrate", "next", "--intent", "Parity intent one"][..],
        ] {
            run_node(project, argv).unwrap_or_else(|error| panic!("{argv:?}: {error:?}"));
        }
    };
    compare_steps(
        "onboarded",
        build,
        &[
            &["intent", "assign", "Dana"],
            &[
                "intent",
                "depth",
                "standard",
                "--approved-by",
                "Owner",
                "--detail",
                "pilot",
            ],
            &["intent", "pause", "--detail", "later"],
            &["intent", "resume", "parity-intent-one"],
            &["intent", "rework", "--detail", "x"],
            &["intent", "abandon", "--detail", "superseded"],
            &["intent", "pause"],
        ],
    );
}

#[test]
fn symlinked_state_directory_is_refused() {
    let dir = TempDir::new("symlink-aidlc");
    let target = TempDir::new("symlink-target");
    one_active(&target.0);
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(target.0.join(".aidlc"), dir.0.join(".aidlc")).is_ok();
    #[cfg(windows)]
    let linked =
        std::os::windows::fs::symlink_dir(target.0.join(".aidlc"), dir.0.join(".aidlc")).is_ok();
    if !linked {
        return; // Symlink creation needs privileges on Windows.
    }
    let before = snapshot(&target.0);
    assert_err(
        &run_rust(&dir.0, &["intent", "pause"]),
        "Refusing to access symlink .aidlc directory",
    );
    assert_eq!(snapshot(&target.0), before);
}

#[test]
fn symlinked_pointer_is_not_written_through() {
    let dir = TempDir::new("symlink-pointer");
    one_active(&dir.0);
    let outside = TempDir::new("symlink-pointer-target");
    let real = outside.0.join("pointer.json");
    fs::rename(dir.0.join(".aidlc/state.json"), &real).unwrap();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&real, dir.0.join(".aidlc/state.json")).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&real, dir.0.join(".aidlc/state.json")).is_ok();
    if !linked {
        return;
    }
    let pointer = fs::read(&real).unwrap();
    let state = fs::read(dir.0.join(".aidlc/intents/alpha/state.json")).unwrap();
    let outcome = run_rust(&dir.0, &["intent", "assign", "Dana"]);
    assert!(
        matches!(&outcome, Err((message, 1)) if message.starts_with("Refusing to write through symlink")),
        "{outcome:?}"
    );
    assert_eq!(fs::read(&real).unwrap(), pointer);
    assert_eq!(
        fs::read(dir.0.join(".aidlc/intents/alpha/state.json")).unwrap(),
        state,
        "intent state must not be written when the pointer write would be refused"
    );
}
