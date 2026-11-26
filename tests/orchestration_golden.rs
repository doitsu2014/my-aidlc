//! Golden lifecycle transcripts for `orchestrate next` / `orchestrate report`.
//!
//! Every scenario runs a command sequence with the native binary in a fresh temporary
//! project. Each step records the exit code, normalized stdout/stderr,
//! `.aidlc/state.json`, every intent `state.json` and the audit rows written after
//! onboarding; the transcript must equal the golden file
//! `tests/golden/orchestration_golden/<scenario>.json`, which was recorded from the
//! reference implementation running the same sequence. The scenario's own expectations
//! (exit codes, stage ids, gate records) are checked on every run as well.
//!
//! Set `DTECH_AIDLC_BLESS=1` to rewrite the golden files from the native CLI after an
//! intentional behaviour change.

#[path = "common/golden.rs"]
mod golden;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const INTENT: &str = "Build a sample feature";
const AI: &str = "AI Orchestrator";
const BLOCKED_EXIT: i32 = 3;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The native CLI, without an inherited `DTECH_AIDLC_HOME`.
fn native_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"));
    command.env_remove("DTECH_AIDLC_HOME");
    command
}

/// What one CLI invocation left behind, after normalization.
#[derive(Debug)]
struct Step {
    label: String,
    exit: Option<i32>,
    stdout: Value,
    stderr: String,
    pointer: Value,
    intents: BTreeMap<String, Value>,
    intent_files: BTreeMap<String, Vec<String>>,
    audit: Vec<Value>,
}

struct Project {
    root: PathBuf,
    audit_offset: usize,
    steps: Vec<Step>,
    users: Vec<String>,
    /// How `--project` is spelled on the command line (the root by default).
    project_arg: Option<std::ffi::OsString>,
    /// `DTECH_AIDLC_HOME` for the invocation (removed by default).
    home: Option<std::ffi::OsString>,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_root(scenario: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aidlc-orch-parity-{scenario}-{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

fn local_identities() -> Vec<String> {
    let mut users = Vec::new();
    for key in ["USERNAME", "USER"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                users.push(value.trim().to_owned());
            }
        }
    }
    if let Ok(output) = Command::new("git")
        .args(["config", "--get", "user.name"])
        .output()
    {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if output.status.success() && !name.is_empty() {
            users.push(name);
        }
    }
    users
}

impl Project {
    fn new(scenario: &str) -> Self {
        let root = unique_root(scenario);
        fs::create_dir_all(&root).unwrap();
        Project {
            root,
            audit_offset: 0,
            steps: Vec::new(),
            users: local_identities(),
            project_arg: None,
            home: None,
        }
    }

    fn who(&self) -> &'static str {
        "rust"
    }

    fn invoke(&self, args: &[&str]) -> std::process::Output {
        let mut command = native_command();
        if let Some(home) = &self.home {
            command.env("DTECH_AIDLC_HOME", home);
        }
        command
            .args(args)
            .arg("--project")
            .arg(self.project_arg.as_deref().unwrap_or(self.root.as_os_str()))
            .output()
            .unwrap_or_else(|error| panic!("[{}] could not start CLI: {error}", self.who()))
    }

    /// Onboards a greenfield solo project with this CLI. Orchestration audit rows are
    /// compared from the end of onboarding onwards.
    fn onboard(&mut self) {
        let output = self.invoke(&["onboard", "--harness", "claude", "--scope", "solo"]);
        assert!(
            output.status.success(),
            "[{}] onboard failed: {}",
            self.who(),
            String::from_utf8_lossy(&output.stderr)
        );
        self.audit_offset = self.audit_rows().len();
    }

    /// Runs one command, asserts the expected exit code, records a normalized step and
    /// returns the raw stdout JSON (Null when stdout is empty).
    fn run(&mut self, label: &str, args: &[&str], expected_exit: i32) -> Value {
        let output = self.invoke(args);
        let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "[{}] step '{label}' ({}) exit code; stderr: {stderr_text}; stdout: {stdout_text}",
            self.who(),
            args.join(" ")
        );
        let raw = if stdout_text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&stdout_text).unwrap_or_else(|error| {
                panic!(
                    "[{}] step '{label}' stdout is not JSON ({error}): {stdout_text}",
                    self.who()
                )
            })
        };
        let step = Step {
            label: label.to_owned(),
            exit: output.status.code(),
            stdout: self.normalize(&raw),
            stderr: self.normalize_text(stderr_text.trim()),
            pointer: self.normalize(&self.read_json(".aidlc/state.json")),
            intents: self.intent_states(),
            intent_files: self.intent_files(),
            audit: self.audit_rows()[self.audit_offset..]
                .iter()
                .map(|row| self.normalize(row))
                .collect(),
        };
        self.steps.push(step);
        raw
    }

    fn last(&self) -> &Step {
        self.steps.last().expect("no step recorded")
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// Writes minimal, valid content for every artifact the directive lists.
    fn write_directive_artifacts(&self, directive: &Value) {
        let artifacts = directive["artifacts"]
            .as_array()
            .unwrap_or_else(|| panic!("[{}] directive has no artifacts: {directive}", self.who()));
        assert!(
            !artifacts.is_empty(),
            "[{}] empty artifact list",
            self.who()
        );
        for artifact in artifacts {
            let output = artifact["output"].as_str().unwrap();
            let rel = artifact["path"].as_str().unwrap();
            self.write(
                rel,
                &format!(
                    "# {output}\n\nMinimal {output} content for the orchestration parity test.\n"
                ),
            );
        }
    }

    fn read_json(&self, rel: &str) -> Value {
        match fs::read(self.root.join(rel)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .unwrap_or_else(|error| panic!("[{}] {rel} is not JSON: {error}", self.who())),
            Err(_) => Value::Null,
        }
    }

    fn intent_ids(&self) -> Vec<String> {
        let mut ids = match fs::read_dir(self.root.join(".aidlc/intents")) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        ids.sort();
        ids
    }

    fn intent_states(&self) -> BTreeMap<String, Value> {
        self.intent_ids()
            .into_iter()
            .map(|id| {
                let value = self.read_json(&format!(".aidlc/intents/{id}/state.json"));
                (id, self.normalize(&value))
            })
            .collect()
    }

    /// The files the CLI itself keeps beside each intent state (not the test's artifacts).
    fn intent_files(&self) -> BTreeMap<String, Vec<String>> {
        self.intent_ids()
            .into_iter()
            .map(|id| {
                let dir = self.root.join(".aidlc/intents").join(&id);
                let mut names = fs::read_dir(&dir)
                    .map(|entries| {
                        entries
                            .filter_map(Result::ok)
                            .filter(|entry| entry.path().is_file())
                            .map(|entry| entry.file_name().to_string_lossy().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                names.sort();
                (id, names)
            })
            .collect()
    }

    fn audit_rows(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join(".aidlc/audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line).unwrap_or_else(|error| {
                    panic!("[{}] audit row is not JSON ({error}): {line}", self.who())
                })
            })
            .collect()
    }

    fn audit_events(&self) -> Vec<String> {
        self.last()
            .audit
            .iter()
            .map(|row| row["event"].as_str().unwrap_or("").to_owned())
            .collect()
    }

    fn root_variants(&self) -> Vec<String> {
        let mut variants = Vec::new();
        let given = self.root.to_string_lossy().to_string();
        variants.push(given.clone());
        variants.push(given.replace('\\', "/"));
        if let Ok(canonical) = fs::canonicalize(&self.root) {
            let canonical = canonical.to_string_lossy().to_string();
            let stripped = canonical
                .strip_prefix(r"\\?\")
                .unwrap_or(&canonical)
                .to_owned();
            variants.push(canonical.clone());
            variants.push(stripped.replace('\\', "/"));
            variants.push(stripped);
        }
        variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
        variants.dedup();
        variants
    }

    fn normalize_text(&self, text: &str) -> String {
        // Repository files copied into the project (the scaffold) have the checkout's
        // line endings; the golden files hold the LF form.
        let text = &golden::checkout_text(text);
        // Engine paths first: the native CLI's engine lives under the project
        // (`<project>/.aidlc/engine`), so replacing the root first would leave
        // `<ROOT><ENGINE>/...` where the golden files (recorded with an installed
        // engine) hold `<ENGINE>/...`.
        let mut value = normalize_engine_paths(text);
        for variant in self.root_variants() {
            value = value.replace(&variant, "<ROOT>");
        }
        if value.contains("<ROOT>") {
            value = value.replace('\\', "/");
        }
        value
    }

    fn normalize(&self, value: &Value) -> Value {
        golden::checkout_value(&self.normalize_keyed(None, value))
    }

    fn normalize_keyed(&self, key: Option<&str>, value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut copy = Map::new();
                for (name, item) in map {
                    copy.insert(name.clone(), self.normalize_keyed(Some(name), item));
                }
                Value::Object(copy)
            }
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|item| self.normalize_keyed(None, item))
                    .collect(),
            ),
            Value::String(text) => {
                if key.is_some_and(is_timestamp_key) {
                    return json!("<TS>");
                }
                if matches!(key, Some("by" | "approvedBy")) && self.users.iter().any(|u| u == text)
                {
                    return json!("<USER>");
                }
                Value::String(self.normalize_text(text))
            }
            other => other.clone(),
        }
    }
}

fn is_timestamp_key(key: &str) -> bool {
    key == "at" || key.ends_with("At")
}

/// Rewrites absolute engine paths (`.../methodology/...`) to `<ENGINE>/methodology/...` with
/// forward slashes, so only the part after `methodology/` is compared.
fn normalize_engine_paths(text: &str) -> String {
    text.split(' ')
        .map(|token| {
            let marker = ["methodology\\", "methodology/"]
                .iter()
                .filter_map(|marker| token.find(marker))
                .min();
            let Some(at) = marker else {
                return token.to_owned();
            };
            let bytes = token.as_bytes();
            let start = (0..at).find(|&i| {
                bytes[i] == b'/'
                    || bytes[i] == b'\\'
                    || (bytes[i].is_ascii_alphabetic()
                        && bytes.get(i + 1) == Some(&b':')
                        && matches!(bytes.get(i + 2), Some(b'\\' | b'/')))
            });
            match start {
                Some(start) => format!(
                    "{}<ENGINE>/{}",
                    &token[..start],
                    token[at..].replace('\\', "/")
                ),
                None => token.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs the scenario with the native CLI and compares its transcript with the golden
/// file (or records it when blessing).
fn assert_lifecycle_golden(scenario: &str, drive: impl Fn(&mut Project)) {
    let rust = {
        let mut project = Project::new(scenario);
        project.onboard();
        drive(&mut project);
        std::mem::take(&mut project.steps)
    };
    let name = format!("{scenario}.json");
    if blessing() {
        write_golden(&name, &transcript(&rust));
        return;
    }
    let golden = read_golden(&name);
    let golden = golden.as_array().expect("golden transcript is an array");
    assert_eq!(
        golden
            .iter()
            .map(|step| step["label"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        rust.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
        "{scenario}: step sequence"
    );
    for (n, r) in golden.iter().zip(&rust) {
        let r = step_json(r);
        let label = format!("{scenario} / {}", r["label"].as_str().unwrap_or_default());
        assert_eq!(r["exit"], n["exit"], "{label}: exit code");
        assert_eq!(r["stderr"], n["stderr"], "{label}: stderr");
        assert_eq!(r["stdout"], n["stdout"], "{label}: stdout JSON");
        assert_eq!(r["pointer"], n["pointer"], "{label}: .aidlc/state.json");
        assert_eq!(
            r["intents"], n["intents"],
            "{label}: intent state.json files"
        );
        assert_eq!(
            r["intentFiles"], n["intentFiles"],
            "{label}: files beside intent state"
        );
        let events = |step: &Value| -> Vec<Value> {
            step["audit"]
                .as_array()
                .map(|rows| rows.iter().map(|row| row["event"].clone()).collect())
                .unwrap_or_default()
        };
        assert_eq!(events(&r), events(n), "{label}: audit event sequence");
        assert_eq!(r["audit"], n["audit"], "{label}: audit rows");
    }
}

fn start_intent(project: &mut Project, label: &str, extra: &[&str]) -> Value {
    let mut args = vec!["orchestrate", "next", "--intent", INTENT];
    args.extend_from_slice(extra);
    project.run(label, &args, 0)
}

fn complete_current(project: &mut Project, label: &str, directive: &Value) -> Value {
    project.write_directive_artifacts(directive);
    project.run(label, &["orchestrate", "report", "--result", "complete"], 0)
}

#[test]
fn orchestrate_next_new_intent_directive_matches_golden() {
    assert_lifecycle_golden("new-intent", |p| {
        let directive = start_intent(p, "next --intent", &[]);
        let who = p.who();
        assert_eq!(directive["intentId"], "build-a-sample-feature", "[{who}]");
        assert_eq!(directive["stage"]["id"], "problem-framing", "[{who}]");
        assert_eq!(directive["gate"]["id"], "analysis-baseline", "[{who}]");
        assert_eq!(directive["mode"], "human", "[{who}]");
        assert_eq!(
            p.last().pointer["activeIntentId"],
            "build-a-sample-feature",
            "[{who}]"
        );
        let state = &p.last().intents["build-a-sample-feature"];
        assert_eq!(state["currentStage"], "problem-framing", "[{who}]");
        assert_eq!(
            state["stages"]["problem-framing"]["status"], "in_progress",
            "[{who}]"
        );
        assert_eq!(
            p.audit_events(),
            ["intent.created", "stage.selected"],
            "[{who}]"
        );
    });
}

#[test]
fn orchestrate_next_resume_and_conflicting_intent_match_golden() {
    assert_lifecycle_golden("resume", |p| {
        start_intent(p, "next --intent", &[]);
        let resumed = p.run("next (resume)", &["orchestrate", "next"], 0);
        let who = p.who();
        assert_eq!(resumed["stage"]["id"], "problem-framing", "[{who}]");
        assert!(resumed["stage"].get("rework").is_none(), "[{who}]");
        assert_eq!(
            p.audit_events(),
            ["intent.created", "stage.selected", "stage.selected"],
            "[{who}]"
        );
        let same = p.run(
            "next --intent (same text)",
            &["orchestrate", "next", "--intent", INTENT],
            0,
        );
        assert_eq!(same["stage"]["id"], "problem-framing", "[{who}]");
        let before = p.last().intents.clone();
        let events_before = p.last().audit.len();
        let refused = p.run(
            "next --intent (different text)",
            &["orchestrate", "next", "--intent", "Build something else"],
            1,
        );
        assert_eq!(refused, Value::Null, "[{who}] no stdout on error");
        assert!(
            p.last().stderr.contains("is still in progress"),
            "[{who}] {}",
            p.last().stderr
        );
        assert_eq!(p.last().intents, before, "[{who}] state unchanged");
        assert_eq!(p.last().audit.len(), events_before, "[{who}] no audit row");
        assert_eq!(p.intent_ids(), ["build-a-sample-feature"], "[{who}]");
    });
}

#[test]
fn orchestrate_report_blocked_then_next_reworks_like_golden() {
    assert_lifecycle_golden("blocked-rework", |p| {
        start_intent(p, "next --intent", &[]);
        let missing_detail = p.run(
            "report blocked without detail",
            &["orchestrate", "report", "--result", "blocked"],
            1,
        );
        let who = p.who();
        assert_eq!(missing_detail, Value::Null, "[{who}]");
        let report = p.run(
            "report blocked",
            &[
                "orchestrate",
                "report",
                "--result",
                "blocked",
                "--detail",
                "x",
            ],
            0,
        );
        assert_eq!(report["recorded"], true, "[{who}]");
        assert_eq!(report["result"], "blocked", "[{who}]");
        assert_eq!(report["phaseGate"], false, "[{who}]");
        let state = &p.last().intents["build-a-sample-feature"];
        assert_eq!(
            state["stages"]["problem-framing"]["status"], "blocked",
            "[{who}]"
        );
        assert_eq!(state["currentStage"], Value::Null, "[{who}]");
        let rework = p.run("next after blocked", &["orchestrate", "next"], 0);
        assert_eq!(rework["stage"]["id"], "problem-framing", "[{who}]");
        assert_eq!(rework["stage"]["rework"]["detail"], "x", "[{who}]");
        assert_eq!(rework["stage"]["rework"]["attempt"], 2, "[{who}]");
        let state = &p.last().intents["build-a-sample-feature"];
        assert_eq!(
            state["stages"]["problem-framing"]["status"], "in_progress",
            "[{who}]"
        );
        let last = p.last().audit.last().unwrap();
        assert_eq!(last["event"], "stage.selected", "[{who}]");
        assert_eq!(last["rework"], true, "[{who}]");
        assert_eq!(
            p.audit_events(),
            [
                "intent.created",
                "stage.selected",
                "stage.reported",
                "stage.selected"
            ],
            "[{who}]"
        );
    });
}

#[test]
fn configured_failing_prerequisite_blocks_new_intent_like_golden() {
    let configure = |p: &Project, check: &str| {
        let mut config = p.read_json(".aidlc/config.json");
        assert!(config.is_object(), "[{}] config.json missing", p.who());
        config["prerequisites"] = json!({
            "toolchain": {
                "check": check,
                "hint": "Install the sample toolchain",
                "stages": ["problem-framing"]
            }
        });
        p.write(
            ".aidlc/config.json",
            &format!("{}\n", serde_json::to_string_pretty(&config).unwrap()),
        );
    };
    assert_lifecycle_golden("prerequisite", |p| {
        configure(p, "exit 7");
        let blocked = p.run(
            "next --intent (prerequisite failing)",
            &["orchestrate", "next", "--intent", INTENT],
            BLOCKED_EXIT,
        );
        let who = p.who();
        assert_eq!(blocked["blocked"], true, "[{who}]");
        assert_eq!(blocked["complete"], false, "[{who}]");
        assert_eq!(blocked["intentId"], Value::Null, "[{who}]");
        assert_eq!(blocked["stage"], "problem-framing", "[{who}]");
        assert_eq!(blocked["needs"][0]["name"], "toolchain", "[{who}]");
        assert_eq!(blocked["needs"][0]["ok"], false, "[{who}]");
        assert_eq!(blocked["needs"][0]["exitCode"], 7, "[{who}]");
        assert!(p.intent_ids().is_empty(), "[{who}] no intent folder");
        assert_eq!(p.last().pointer, Value::Null, "[{who}] no state pointer");
        assert!(p.last().audit.is_empty(), "[{who}] no audit row");
        configure(p, "exit 0");
        let directive = p.run(
            "next --intent (prerequisite passing)",
            &["orchestrate", "next", "--intent", INTENT],
            0,
        );
        assert_eq!(directive["stage"]["id"], "problem-framing", "[{who}]");
        assert_eq!(directive["prerequisites"][0]["ok"], true, "[{who}]");
        assert_eq!(
            p.audit_events(),
            ["intent.created", "prerequisite.checked", "stage.selected"],
            "[{who}]"
        );
    });
}

#[test]
fn completing_first_stage_with_artifacts_matches_golden() {
    assert_lifecycle_golden("complete-stage", |p| {
        let directive = start_intent(p, "next --intent", &[]);
        let who = p.who();
        let early = p.run(
            "report complete without artifacts",
            &["orchestrate", "report", "--result", "complete"],
            1,
        );
        assert_eq!(early, Value::Null, "[{who}]");
        assert!(
            p.last().stderr.contains("cannot be completed"),
            "[{who}] {}",
            p.last().stderr
        );
        let report = complete_current(p, "report complete", &directive);
        assert_eq!(report["result"], "complete", "[{who}]");
        assert_eq!(report["stage"], "problem-framing", "[{who}]");
        assert_eq!(report["intentComplete"], false, "[{who}]");
        assert_eq!(
            report["artifacts"].as_array().map(Vec::len),
            directive["artifacts"].as_array().map(Vec::len),
            "[{who}]"
        );
        let state = &p.last().intents["build-a-sample-feature"];
        assert_eq!(
            state["stages"]["problem-framing"]["status"], "complete",
            "[{who}]"
        );
        let next = p.run("next after complete", &["orchestrate", "next"], 0);
        assert_eq!(next["stage"]["id"], "context-and-risk", "[{who}]");
        assert_eq!(next["stage"]["phaseGate"], true, "[{who}]");
        assert_eq!(
            p.audit_events(),
            [
                "intent.created",
                "stage.selected",
                "stage.reported",
                "stage.selected"
            ],
            "[{who}]"
        );
    });
}

#[test]
fn ai_mode_analyze_gate_approval_matches_golden() {
    assert_lifecycle_golden("ai-gate", |p| {
        let first = start_intent(p, "next --intent --mode ai", &["--mode", "ai"]);
        let who = p.who();
        assert_eq!(first["mode"], "ai", "[{who}]");
        assert_eq!(first["gateMode"], "ai", "[{who}]");
        complete_current(p, "report problem-framing complete", &first);
        let gate_stage = p.run("next to analyze gate", &["orchestrate", "next"], 0);
        assert_eq!(gate_stage["stage"]["id"], "context-and-risk", "[{who}]");
        assert_eq!(gate_stage["stage"]["phaseGate"], true, "[{who}]");
        p.write_directive_artifacts(&gate_stage);
        let unapproved = p.run(
            "report gate without approval",
            &["orchestrate", "report", "--result", "complete"],
            1,
        );
        assert_eq!(unapproved, Value::Null, "[{who}]");
        let human = p.run(
            "report gate approved by a human name",
            &[
                "orchestrate",
                "report",
                "--result",
                "complete",
                "--approval",
                "approved",
                "--approved-by",
                "Pat Owner",
            ],
            1,
        );
        assert_eq!(human, Value::Null, "[{who}]");
        let no_recommendation = p.run(
            "report gate approved without recommendation",
            &[
                "orchestrate",
                "report",
                "--result",
                "complete",
                "--approval",
                "approved",
                "--approved-by",
                AI,
            ],
            1,
        );
        assert_eq!(no_recommendation, Value::Null, "[{who}]");
        assert!(
            p.last().stderr.contains("gate-recommendation"),
            "[{who}] {}",
            p.last().stderr
        );
        p.write(
            ".aidlc/intents/build-a-sample-feature/analyze/context-and-risk/gate-recommendation.md",
            "# Gate recommendation\n\nApprove the analysis baseline.\n\n## Open items\n\n- [follow-up] Track sample telemetry after launch\n",
        );
        let approved = p.run(
            "report gate approved",
            &[
                "orchestrate",
                "report",
                "--result",
                "complete",
                "--approval",
                "approved",
                "--approved-by",
                AI,
            ],
            0,
        );
        assert_eq!(approved["phaseGate"], true, "[{who}]");
        assert_eq!(approved["gate"], "analysis-baseline", "[{who}]");
        assert_eq!(approved["approval"], "approved", "[{who}]");
        assert_eq!(approved["approvedBy"], AI, "[{who}]");
        let state = &p.last().intents["build-a-sample-feature"];
        let gate = &state["gates"]["analysis-baseline"];
        assert_eq!(gate["decision"], "approved", "[{who}]");
        assert_eq!(gate["mode"], "ai", "[{who}]");
        assert_eq!(gate["by"], AI, "[{who}]");
        assert_eq!(gate["bySource"], "flag", "[{who}]");
        assert_eq!(
            gate["openItems"],
            json!({"writtenApproval": 0, "followUp": 1}),
            "[{who}]"
        );
        let decided = p
            .last()
            .audit
            .iter()
            .find(|row| row["event"] == "gate.decided")
            .unwrap_or_else(|| panic!("[{who}] no gate.decided audit row"));
        assert_eq!(decided["gate"], "analysis-baseline", "[{who}]");
        assert_eq!(decided["decision"], "approved", "[{who}]");
        assert_eq!(decided["by"], AI, "[{who}]");
        assert_eq!(decided["mode"], "ai", "[{who}]");
        assert_eq!(
            p.audit_events(),
            [
                "intent.created",
                "stage.selected",
                "stage.reported",
                "stage.selected",
                "stage.reported",
                "gate.decided"
            ],
            "[{who}]"
        );
        let ideate = p.run("next after gate", &["orchestrate", "next"], 0);
        assert_eq!(ideate["stage"]["id"], "option-generation", "[{who}]");
        assert_eq!(ideate["gate"]["id"], "solution-direction", "[{who}]");
    });
}

/// Regression: `orchestrate report` validated the whole project configuration, but the
/// reference implementation reads it unvalidated (malformed JSON is `{}`); and `next`
/// validated it before the knowledge preflight, where the reference validates it while
/// selecting the stage. Validation messages print values the way the reference does.
#[test]
fn configuration_is_read_in_reference_order_with_reference_messages() {
    assert_lifecycle_golden("config-order", |p| {
        let who = p.who();
        let original = fs::read_to_string(p.root.join(".aidlc/config.json")).unwrap();
        p.write(".aidlc/config.json", "{bad");
        p.run(
            "report (malformed config, no intent)",
            &["orchestrate", "report", "--result", "complete"],
            1,
        );
        assert_eq!(
            p.last().stderr,
            "dtech-aidlc: No active workflow state",
            "[{who}]"
        );
        p.run("next (malformed config)", &["orchestrate", "next"], 1);
        assert_eq!(
            p.last().stderr,
            "dtech-aidlc: Invalid project configuration JSON",
            "[{who}]"
        );
        p.write("aidlc.knowledge.json", "{oops");
        p.run(
            "next (malformed config and knowledge manifest)",
            &["orchestrate", "next"],
            1,
        );
        assert!(
            p.last().stderr.contains("knowledge manifest"),
            "[{who}] knowledge preflight runs first: {}",
            p.last().stderr
        );
        fs::remove_file(p.root.join("aidlc.knowledge.json")).unwrap();
        p.write(".aidlc/config.json", &original);
        start_intent(p, "next --intent", &[]);

        let mut config: Value = serde_json::from_str(&original).unwrap();
        config["gateModes"] = json!({"old-gate": "human"});
        p.write(".aidlc/config.json", &config.to_string());
        let reported = p.run(
            "report blocked (stale gate mode)",
            &[
                "orchestrate",
                "report",
                "--result",
                "blocked",
                "--detail",
                "Waiting on the owner",
            ],
            0,
        );
        assert_eq!(reported["recorded"], true, "[{who}]");
        assert_eq!(
            p.audit_events(),
            ["intent.created", "stage.selected", "stage.reported"],
            "[{who}]"
        );
        p.run("next (stale gate mode)", &["orchestrate", "next"], 1);
        assert_eq!(
            p.last().stderr,
            "dtech-aidlc: Unknown gate in gateModes: old-gate",
            "[{who}]"
        );
        config["gateModes"] = json!(["human"]);
        p.write(".aidlc/config.json", &config.to_string());
        p.run(
            "next (gateModes not an object)",
            &["orchestrate", "next"],
            1,
        );
        assert_eq!(
            p.last().stderr,
            "dtech-aidlc: Invalid gateModes: expected an object mapping gate ids to \"human\"",
            "[{who}]"
        );
        config.as_object_mut().unwrap().remove("gateModes");
        config["agentModels"] = json!({"brief-analyst": "gpt4"});
        p.write(".aidlc/config.json", &config.to_string());
        p.run("next (unknown model alias)", &["orchestrate", "next"], 1);
        assert_eq!(
            p.last().stderr,
            "dtech-aidlc: Invalid model alias for brief-analyst: gpt4; choose opus, sonnet, haiku, or fable",
            "[{who}]"
        );
        config["agentModels"] = json!({"brief-analyst": 2.0});
        p.write(".aidlc/config.json", &config.to_string());
        p.run("next (numeric model alias)", &["orchestrate", "next"], 1);
    });
}

/// Regression: a `--project` folder that does not exist leaked the OS error from the
/// knowledge preflight; the reference reports no active intent, or an uninitialized
/// project.
#[test]
fn missing_project_folder_is_reported_like_golden() {
    assert_lifecycle_golden("missing-project", |p| {
        let who = p.who();
        let missing = p.root.join("not-created");
        p.project_arg = Some(missing.clone().into_os_string());
        p.run("next (missing project)", &["orchestrate", "next"], 1);
        assert!(
            p.last()
                .stderr
                .starts_with("dtech-aidlc: No active intent. Start one with:"),
            "[{who}] {}",
            p.last().stderr
        );
        p.run(
            "next --intent (missing project)",
            &["orchestrate", "next", "--intent", INTENT],
            1,
        );
        assert!(
            p.last().stderr.starts_with(
                "dtech-aidlc: Project is not initialized at <ROOT>/not-created. Run 'dtech-aidlc onboard"
            ),
            "[{who}] {}",
            p.last().stderr
        );
        p.run(
            "report (missing project)",
            &["orchestrate", "report", "--result", "complete"],
            1,
        );
        assert_eq!(
            p.last().stderr,
            "dtech-aidlc: No active workflow state",
            "[{who}]"
        );
        assert!(!missing.exists(), "[{who}] nothing is created");
    });
}

/// Regression: the project root kept a trailing separator (and, on POSIX, `..`
/// segments) where the reference implementation normalises it; and intent text that
/// differs only by Unicode normalisation is the same intent (compared in NFC).
#[test]
fn project_path_spelling_and_nfc_intent_text_match_golden() {
    assert_lifecycle_golden("project-spelling", |p| {
        let who = p.who();
        let expected_dir = p.root.join(".aidlc").join("intents").join("cafe-menu");
        let mut trailing = p.root.clone().into_os_string();
        trailing.push(std::path::MAIN_SEPARATOR_STR);
        p.project_arg = Some(trailing);
        let directive = p.run(
            "next --intent (NFC text, trailing separator)",
            &["orchestrate", "next", "--intent", "Caf\u{e9} menu"],
            0,
        );
        assert_eq!(directive["intentId"], "cafe-menu", "[{who}]");
        assert_eq!(
            directive["intentDir"].as_str(),
            Some(expected_dir.to_string_lossy().as_ref()),
            "[{who}]"
        );
        let mut dotted = p.root.join("sub").into_os_string();
        dotted.push("/../");
        p.project_arg = Some(dotted);
        let resumed = p.run(
            "next --intent (NFD text, dotted path)",
            &["orchestrate", "next", "--intent", "Cafe\u{301} menu"],
            0,
        );
        assert_eq!(resumed["intentId"], "cafe-menu", "[{who}]");
        assert_eq!(
            resumed["intentDir"].as_str(),
            Some(expected_dir.to_string_lossy().as_ref()),
            "[{who}]"
        );
        assert_eq!(p.intent_ids(), ["cafe-menu"], "[{who}]");
    });
}

/// Regression: `DTECH_AIDLC_HOME` was used as spelled; it must be resolved, so directive
/// methodology paths are absolute and normalised (golden `engine-home.json`).
#[test]
fn engine_home_is_resolved_like_golden() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let spelled = format!("{}/./", repo.to_string_lossy().replace('\\', "/"));
    let mut p = Project::new("engine-home");
    p.onboard();
    p.home = Some(spelled.clone().into());
    let directive = start_intent(&mut p, "next --intent", &[]);
    let templates = directive["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| artifact["template"].clone())
        .collect::<Vec<_>>();
    let paths = json!({
        "contractsFile": directive["contractsFile"],
        "phaseFile": directive["stage"]["phaseFile"],
        "agentFile": directive["stage"]["agentFile"],
        "templates": templates,
    });
    let normalized = without_repo(&paths);
    if blessing() {
        write_golden("engine-home.json", &normalized);
    } else {
        assert_eq!(
            normalized,
            read_golden("engine-home.json"),
            "methodology paths"
        );
    }
    let contracts = repo.join("core").join("methodology").join("contracts.md");
    assert_eq!(
        paths["contractsFile"].as_str(),
        Some(contracts.to_string_lossy().as_ref())
    );
}

#[test]
fn engine_path_normalization_keeps_only_methodology_suffix() {
    assert_eq!(
        normalize_engine_paths(r"D:\x\core\methodology\agents\brief-analyst.md"),
        "<ENGINE>/methodology/agents/brief-analyst.md"
    );
    assert_eq!(
        normalize_engine_paths("write it from /opt/engine/methodology/templates/a.md now"),
        "write it from <ENGINE>/methodology/templates/a.md now"
    );
    assert_eq!(
        normalize_engine_paths("see methodology/contracts.md"),
        "see methodology/contracts.md"
    );
    assert!(is_timestamp_key("updatedAt") && is_timestamp_key("at"));
    assert!(!is_timestamp_key("format"));
}

/// Regression: intent ids for text outside ASCII/Latin-1/Vietnamese (Latin Extended-A,
/// full-width Latin, ligatures, decomposed marks) must match the recorded slugs, since
/// the id names the intent folder, the knowledge record and `suggestedBranch`.
#[test]
fn unicode_intent_ids_match_golden() {
    for (scenario, text) in [
        ("slug-czech", "Opravit chybu v přihlášení uživatele"),
        ("slug-polish", "Naprawić błąd logowania w źródle"),
        ("slug-fullwidth", "ＡＰＩ ｒｅｆａｃｔｏｒ"),
        ("slug-ligature", "ﬁle ﬂow Ⅻ ①②③ test"),
        ("slug-nfc-hash", "构建 e\u{0301}"),
    ] {
        assert_lifecycle_golden(scenario, |p| {
            let directive = p.run(
                "next --intent",
                &["orchestrate", "next", "--intent", text],
                0,
            );
            let id = directive["intentId"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let who = p.who();
            assert!(!id.is_empty(), "[{who}] {text}");
            assert_eq!(p.last().pointer["activeIntentId"], id, "[{who}] {text}");
        });
    }
}

fn hex_chars(text: &str) -> String {
    text.chars()
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

fn rust_unicode_line(key: &str, text: &str) -> String {
    let (base, kept, letters) = dtech_aidlc::workflow::intent_slug_parts(text);
    let nfc = dtech_aidlc::workflow::nfc(text);
    format!("{key}\t{base}\t{kept}\t{letters}\t{}", hex_chars(&nfc))
}

/// Every code point folds (NFKD, `\p{L}\p{N}` count) and normalises (NFC) as recorded
/// from the reference implementation (`unicode-codepoints.json`: line count, SHA-256 of
/// all lines, per-block hashes and samples).
#[test]
fn intent_slug_folding_and_nfc_match_golden_for_every_code_point() {
    let rust: Vec<String> = (0u32..0x110000)
        .filter_map(char::from_u32)
        .map(|c| rust_unicode_line(&format!("{:x}", c as u32), &c.to_string()))
        .collect();
    assert_lines_match_golden("unicode-codepoints.json", &rust);
}

/// Sequences (decomposed forms, reordered marks, Hangul jamo, mixed scripts) fold and
/// normalise as recorded from the reference implementation, so the fallback hash and the
/// base slug agree. The inputs are the seeded random sequences followed by the NFD and
/// NFKD form of every code point that has one (`unicode-decompositions.txt`, recorded
/// from the reference; blessing keeps that table and rewrites only the digest).
#[test]
fn intent_slug_folding_and_nfc_match_golden_for_sequences() {
    let pool: Vec<char> = [
        0x41..0x5B,
        0x61..0x7B,
        0xC0..0x180,
        0x300..0x370,
        0x591..0x5C8,
        0x610..0x61B,
        0x93C..0x94E,
        0x1100..0x1176,
        0x11A8..0x11C3,
        0x1AB0..0x1ACF,
        0x1DC0..0x1E00,
        0x1E00..0x1F00,
        0x1F00..0x2000,
        0x20D0..0x20F1,
        0x2160..0x2190,
        0x2460..0x24EA,
        0x3099..0x309B,
        0x304B..0x3050,
        0xAC00..0xAC40,
        0xFB00..0xFB07,
        0xFE20..0xFE30,
        0xFF21..0xFF5B,
        0x1D400..0x1D420,
        0x4E00..0x4E10,
    ]
    .into_iter()
    .flatten()
    .filter_map(char::from_u32)
    .collect();
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut inputs: Vec<String> = Vec::new();
    for _ in 0..20_000 {
        let len = 1 + (next() % 8) as usize;
        inputs.push(
            (0..len)
                .map(|_| pool[(next() % pool.len() as u64) as usize])
                .collect(),
        );
    }
    let mut all = inputs;
    all.extend(decomposed_inputs(&read_golden_text(DECOMPOSITIONS)));
    let rust: Vec<String> = all
        .iter()
        .map(|text| rust_unicode_line(&hex_chars(text), text))
        .collect();
    assert_lines_match_golden("unicode-sequences.json", &rust);
}

// ---------------------------------------------------------------------------
// Golden files: tests/golden/orchestration_golden/. The transcripts, engine paths
// and Unicode digests were recorded from the reference implementation; set
// DTECH_AIDLC_BLESS=1 to rewrite them from the native CLI after an intentional change.
// ---------------------------------------------------------------------------

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/orchestration_golden")
        .join(name)
}

fn blessing() -> bool {
    std::env::var_os("DTECH_AIDLC_BLESS").is_some_and(|value| value == "1")
}

fn write_golden_text(name: &str, text: &str) {
    let path = golden_path(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_golden(name: &str, value: &Value) {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    write_golden_text(name, &text);
}

/// The golden file's text with line endings normalized to `\n`.
fn read_golden_text(name: &str) -> String {
    let path = golden_path(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| {
            panic!(
                "cannot read golden {} ({error}); record it with DTECH_AIDLC_BLESS=1",
                path.display()
            )
        })
        .replace("\r\n", "\n")
}

fn read_golden(name: &str) -> Value {
    serde_json::from_str(&read_golden_text(name))
        .unwrap_or_else(|error| panic!("golden {name} is not JSON: {error}"))
}

/// One recorded step as JSON.
fn step_json(step: &Step) -> Value {
    json!({
        "label": step.label,
        "exit": step.exit,
        "stdout": step.stdout,
        "stderr": step.stderr,
        "pointer": step.pointer,
        "intents": step.intents,
        "intentFiles": step.intent_files,
        "audit": step.audit,
    })
}

fn transcript(steps: &[Step]) -> Value {
    Value::Array(steps.iter().map(step_json).collect())
}

/// Methodology paths with this repository as `<REPO>` and forward slashes.
fn without_repo(value: &Value) -> Value {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let native = repo.to_string_lossy().into_owned();
    let posix = native.replace('\\', "/");
    match value {
        Value::String(text) if text.contains(&native) || text.contains(&posix) => Value::String(
            text.replace(&native, "<REPO>")
                .replace(&posix, "<REPO>")
                .replace('\\', "/"),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_repo).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), without_repo(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

const DIGEST_BLOCK: usize = 4096;
const DIGEST_SAMPLE: usize = 32768;

fn sha256_hex(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// A compact record of a large line-oriented output: the line count, the SHA-256 of
/// the lines joined with `\n`, one SHA-256 per block of lines (to locate a mismatch)
/// and a sample of lines.
fn digest_lines(lines: &[String]) -> Value {
    let blocks: Vec<String> = lines
        .chunks(DIGEST_BLOCK)
        .map(|block| sha256_hex(&block.join("\n")))
        .collect();
    let samples: Map<String, Value> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| index % DIGEST_SAMPLE == 0 || *index + 1 == lines.len())
        .map(|(index, line)| (index.to_string(), json!(line)))
        .collect();
    json!({
        "lines": lines.len(),
        "sha256": sha256_hex(&lines.join("\n")),
        "blockLines": DIGEST_BLOCK,
        "blocks": blocks,
        "samples": samples,
    })
}

/// Compares the native lines with the golden digest `name` (or records it when
/// blessing), naming the first differing blocks and samples.
fn assert_lines_match_golden(name: &str, rust: &[String]) {
    let actual = digest_lines(rust);
    if blessing() {
        write_golden(name, &actual);
        return;
    }
    let expected = read_golden(name);
    if actual == expected {
        return;
    }
    let mut problems = Vec::new();
    if actual["lines"] != expected["lines"] {
        problems.push(format!(
            "line count: golden {} native {}",
            expected["lines"], actual["lines"]
        ));
    }
    let empty = Vec::new();
    let golden_blocks = expected["blocks"].as_array().unwrap_or(&empty);
    for (index, block) in actual["blocks"].as_array().unwrap().iter().enumerate() {
        if golden_blocks.get(index) != Some(block) && problems.len() < 20 {
            let first = index * DIGEST_BLOCK;
            problems.push(format!(
                "block {index} (lines {first}..{}) differs; first native line {:?}",
                first + DIGEST_BLOCK,
                rust[first]
            ));
        }
    }
    if let Some(samples) = expected["samples"].as_object() {
        for (index, line) in samples {
            let native = index.parse::<usize>().ok().and_then(|i| rust.get(i));
            if native.map(String::as_str) != line.as_str() && problems.len() < 40 {
                problems.push(format!("line {index}: golden {line} native {native:?}"));
            }
        }
    }
    panic!(
        "{name}: native output differs from the golden digest\n{}",
        problems.join("\n")
    );
}

/// Canonical and compatibility decompositions of every code point other than the
/// Hangul syllables, recorded from the reference implementation: one line per code
/// point whose NFD or NFKD differs from it, `cp<TAB>NFD<TAB>NFKD` in hex code points,
/// `-` when that form is the code point itself and `=` when NFKD equals NFD.
const DECOMPOSITIONS: &str = "unicode-decompositions.txt";

fn parse_hex_chars(text: &str) -> String {
    text.split(' ')
        .filter(|part| !part.is_empty())
        .map(|part| char::from_u32(u32::from_str_radix(part, 16).unwrap()).unwrap())
        .collect()
}

/// Hangul syllable decomposition (both NFD and NFKD), computed arithmetically.
fn hangul_decomposition(c: char) -> Option<String> {
    let s = (c as u32).checked_sub(0xAC00).filter(|s| *s < 11172)?;
    let mut out = String::new();
    out.push(char::from_u32(0x1100 + s / 588).unwrap());
    out.push(char::from_u32(0x1161 + (s % 588) / 28).unwrap());
    if s % 28 != 0 {
        out.push(char::from_u32(0x11A7 + s % 28).unwrap());
    }
    Some(out)
}

/// The decomposed inputs the sequence check appends after its random inputs: for each
/// code point in order, its NFD and then its NFKD form when that differs from it, each
/// alone and wrapped as `ab<form>\u{323}\u{301}cd`.
fn decomposed_inputs(table: &str) -> Vec<String> {
    let mut forms: BTreeMap<u32, (Option<String>, Option<String>)> = BTreeMap::new();
    for line in table.lines().filter(|line| !line.is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        assert_eq!(parts.len(), 3, "bad decomposition row {line:?}");
        let cp = u32::from_str_radix(parts[0], 16).unwrap();
        let nfd = (parts[1] != "-").then(|| parse_hex_chars(parts[1]));
        let nfkd = match parts[2] {
            "-" => None,
            "=" => nfd.clone(),
            hex => Some(parse_hex_chars(hex)),
        };
        forms.insert(cp, (nfd, nfkd));
    }
    let mut inputs = Vec::new();
    for c in (0u32..0x110000).filter_map(char::from_u32) {
        let (nfd, nfkd) = match hangul_decomposition(c) {
            Some(syllable) => (Some(syllable.clone()), Some(syllable)),
            None => forms.get(&(c as u32)).cloned().unwrap_or((None, None)),
        };
        for form in [nfd, nfkd].into_iter().flatten() {
            let wrapped = format!("ab{form}\u{323}\u{301}cd");
            inputs.push(form);
            inputs.push(wrapped);
        }
    }
    inputs
}
