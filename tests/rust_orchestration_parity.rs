//! Node-vs-Rust lifecycle parity for `orchestrate next` / `orchestrate report`.
//!
//! Every scenario runs the same command sequence twice, once with the Node reference CLI
//! (`core/tools/dtech-aidlc.mjs`) and once with the native binary, each in its own fresh
//! temporary project. The Node run finishes completely before the Rust run starts, so the
//! scenario's own expectations (exit codes, stage ids, gate records) are proven against Node
//! first. Each step records the exit code, normalized stdout/stderr, `.aidlc/state.json`,
//! every intent `state.json` and the audit rows written after onboarding; the two
//! transcripts must then match step for step.
//!
//! Native orchestration routing is enabled, so every scenario runs by default.

use serde_json::{json, Map, Value};
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cli {
    Node,
    Rust,
}

impl Cli {
    fn label(self) -> &'static str {
        match self {
            Cli::Node => "node",
            Cli::Rust => "rust",
        }
    }

    fn command(self) -> Command {
        // AIDLC_ORCH_PARITY_SELF_CHECK=1 runs Node for both halves, proving the
        // normalization removes every volatile value (Node vs Node must pass).
        let self_check = std::env::var_os("AIDLC_ORCH_PARITY_SELF_CHECK").is_some();
        let mut command = match self {
            _ if self == Cli::Node || self_check => {
                let mut command = Command::new("node");
                command.arg(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core/tools/dtech-aidlc.mjs"),
                );
                command
            }
            _ => Command::new(env!("CARGO_BIN_EXE_dtech-aidlc")),
        };
        command.env_remove("DTECH_AIDLC_HOME");
        command
    }
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
    cli: Cli,
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

fn unique_root(cli: Cli, scenario: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aidlc-orch-parity-{scenario}-{}-{}-{nanos}-{}",
        cli.label(),
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
    fn new(cli: Cli, scenario: &str) -> Self {
        let root = unique_root(cli, scenario);
        fs::create_dir_all(&root).unwrap();
        Project {
            cli,
            root,
            audit_offset: 0,
            steps: Vec::new(),
            users: local_identities(),
            project_arg: None,
            home: None,
        }
    }

    fn who(&self) -> &'static str {
        self.cli.label()
    }

    fn invoke(&self, args: &[&str]) -> std::process::Output {
        let mut command = self.cli.command();
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
        // Engine paths first: the native CLI's engine lives under the project
        // (`<project>/.aidlc/engine`), so replacing the root first would leave
        // `<ROOT><ENGINE>/...` where Node's installed engine gives `<ENGINE>/...`.
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
        self.normalize_keyed(None, value)
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

/// Runs the scenario completely with Node, then with Rust, and compares the transcripts.
fn assert_lifecycle_parity(scenario: &str, drive: impl Fn(&mut Project)) {
    let node = {
        let mut project = Project::new(Cli::Node, scenario);
        project.onboard();
        drive(&mut project);
        std::mem::take(&mut project.steps)
    };
    let rust = {
        let mut project = Project::new(Cli::Rust, scenario);
        project.onboard();
        drive(&mut project);
        std::mem::take(&mut project.steps)
    };
    assert_eq!(
        node.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
        rust.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
        "{scenario}: step sequence"
    );
    for (n, r) in node.iter().zip(&rust) {
        let label = format!("{scenario} / {}", n.label);
        assert_eq!(r.exit, n.exit, "{label}: exit code");
        assert_eq!(r.stderr, n.stderr, "{label}: stderr");
        assert_eq!(r.stdout, n.stdout, "{label}: stdout JSON");
        assert_eq!(r.pointer, n.pointer, "{label}: .aidlc/state.json");
        assert_eq!(r.intents, n.intents, "{label}: intent state.json files");
        assert_eq!(
            r.intent_files, n.intent_files,
            "{label}: files beside intent state"
        );
        let events = |steps: &Step| {
            steps
                .audit
                .iter()
                .map(|row| row["event"].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(events(r), events(n), "{label}: audit event sequence");
        assert_eq!(r.audit, n.audit, "{label}: audit rows");
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
fn orchestrate_next_new_intent_directive_matches_node() {
    assert_lifecycle_parity("new-intent", |p| {
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
fn orchestrate_next_resume_and_conflicting_intent_match_node() {
    assert_lifecycle_parity("resume", |p| {
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
fn orchestrate_report_blocked_then_next_reworks_like_node() {
    assert_lifecycle_parity("blocked-rework", |p| {
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
fn configured_failing_prerequisite_blocks_new_intent_like_node() {
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
    assert_lifecycle_parity("prerequisite", |p| {
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
fn completing_first_stage_with_artifacts_matches_node() {
    assert_lifecycle_parity("complete-stage", |p| {
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
fn ai_mode_analyze_gate_approval_matches_node() {
    assert_lifecycle_parity("ai-gate", |p| {
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

/// Regression: `orchestrate report` validated the whole project configuration, but Node
/// reads it with the unvalidated `loadConfig` (malformed JSON is `{}`); and `next`
/// validated it before the knowledge preflight, where Node validates it inside
/// `orchestrateNext`. Validation messages print values the way Node's `String()` does.
#[test]
fn configuration_is_read_in_node_order_with_node_messages() {
    assert_lifecycle_parity("config-order", |p| {
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
/// knowledge preflight; Node reports no active intent, or an uninitialized project.
#[test]
fn missing_project_folder_is_reported_like_node() {
    assert_lifecycle_parity("missing-project", |p| {
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
/// segments) where Node's `path.resolve` normalises it; and intent text that differs
/// only by Unicode normalisation is the same intent in Node (`sameIntentText` uses NFC).
#[test]
fn project_path_spelling_and_nfc_intent_text_match_node() {
    assert_lifecycle_parity("project-spelling", |p| {
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

/// Regression: `DTECH_AIDLC_HOME` was used as spelled; Node resolves it with
/// `path.resolve`, so directive methodology paths are absolute and normalised.
#[test]
fn engine_home_is_resolved_like_node() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let spelled = format!("{}/./", repo.to_string_lossy().replace('\\', "/"));
    let paths = [Cli::Node, Cli::Rust].map(|cli| {
        let mut p = Project::new(cli, "engine-home");
        p.onboard();
        p.home = Some(spelled.clone().into());
        let directive = start_intent(&mut p, "next --intent", &[]);
        let templates = directive["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|artifact| artifact["template"].clone())
            .collect::<Vec<_>>();
        json!({
            "contractsFile": directive["contractsFile"],
            "phaseFile": directive["stage"]["phaseFile"],
            "agentFile": directive["stage"]["agentFile"],
            "templates": templates,
        })
    });
    assert_eq!(paths[1], paths[0], "rust vs node methodology paths");
    let contracts = repo.join("core").join("methodology").join("contracts.md");
    assert_eq!(
        paths[1]["contractsFile"].as_str(),
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
/// full-width Latin, ligatures, decomposed marks) must match Node's `intentSlug`, since
/// the id names the intent folder, the knowledge record and `suggestedBranch`.
#[test]
fn unicode_intent_ids_match_node() {
    for (scenario, text) in [
        ("slug-czech", "Opravit chybu v přihlášení uživatele"),
        ("slug-polish", "Naprawić błąd logowania w źródle"),
        ("slug-fullwidth", "ＡＰＩ ｒｅｆａｃｔｏｒ"),
        ("slug-ligature", "ﬁle ﬂow Ⅻ ①②③ test"),
        ("slug-nfc-hash", "构建 e\u{0301}"),
    ] {
        assert_lifecycle_parity(scenario, |p| {
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

/// Node's `intentSlug` folding and NFC, evaluated by Node itself, for parity checks of
/// `intent_slug_parts` and `nfc`.
const NODE_UNICODE_SCRIPT: &str = r#"
const { readFileSync } = await import('node:fs');
const parts = (source) => {
  const folded = source.normalize('NFKD').replace(/[̀-ͯ]/g, '').replace(/[đĐ]/g, 'd').toLowerCase();
  const letters = (folded.match(/[\p{L}\p{N}]/gu) || []).length;
  const kept = (folded.match(/[a-z0-9]/g) || []).length;
  const base = folded.replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48).replace(/-+$/, '');
  return [base, kept, letters];
};
const hex = (text) => [...text].map((c) => c.codePointAt(0).toString(16)).join(' ');
const out = [];
if (process.env.AIDLC_MODE === 'codepoints') {
  for (let cp = 0; cp < 0x110000; cp++) {
    if (cp >= 0xd800 && cp <= 0xdfff) continue;
    const text = String.fromCodePoint(cp);
    const [base, kept, letters] = parts(text);
    out.push(`${cp.toString(16)}\t${base}\t${kept}\t${letters}\t${hex(text.normalize('NFC'))}`);
  }
} else {
  const inputs = JSON.parse(readFileSync(process.env.AIDLC_INPUT, 'utf8'));
  for (let cp = 0; cp < 0x110000; cp++) {
    if (cp >= 0xd800 && cp <= 0xdfff) continue;
    const text = String.fromCodePoint(cp);
    for (const form of ['NFD', 'NFKD']) {
      const decomposed = text.normalize(form);
      if (decomposed !== text) inputs.push(decomposed, `ab${decomposed}̣́cd`);
    }
  }
  for (const text of inputs) {
    const [base, kept, letters] = parts(text);
    out.push(`${hex(text)}\t${base}\t${kept}\t${letters}\t${hex(text.normalize('NFC'))}`);
  }
}
process.stdout.write(out.join('\n'));
"#;

fn node_unicode(mode: &str, input: Option<&std::path::Path>) -> String {
    let mut command = Command::new("node");
    command
        .args(["--input-type=module", "-e", NODE_UNICODE_SCRIPT])
        .env("AIDLC_MODE", mode);
    if let Some(input) = input {
        command.env("AIDLC_INPUT", input);
    }
    let output = command.output().expect("Node must be available");
    assert!(
        output.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
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

fn assert_lines_match(node: &str, rust: impl Iterator<Item = String>) {
    let mut mismatches = Vec::new();
    let mut count = 0usize;
    let mut node_lines = node.split('\n');
    for rust_line in rust {
        count += 1;
        let node_line = node_lines.next().unwrap_or("<missing>");
        if node_line != rust_line && mismatches.len() < 20 {
            mismatches.push(format!("node {node_line:?}\nrust {rust_line:?}"));
        }
    }
    assert!(node_lines.next().is_none(), "Node produced extra lines");
    assert!(
        mismatches.is_empty(),
        "{count} checked; first mismatches:\n{}",
        mismatches.join("\n")
    );
}

/// Every code point folds (NFKD, `\p{L}\p{N}` count) and normalises (NFC) as in Node.
#[test]
fn intent_slug_folding_and_nfc_match_node_for_every_code_point() {
    let node = node_unicode("codepoints", None);
    let rust = (0u32..0x110000).filter_map(char::from_u32).map(|c| {
        let text = c.to_string();
        rust_unicode_line(&format!("{:x}", c as u32), &text)
    });
    assert_lines_match(&node, rust);
}

/// Sequences (decomposed forms, reordered marks, Hangul jamo, mixed scripts) fold and
/// normalise as in Node, so the fallback hash and the base slug agree.
#[test]
fn intent_slug_folding_and_nfc_match_node_for_sequences() {
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
    let dir = unique_root(Cli::Rust, "unicode-sequences");
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("inputs.json");
    fs::write(&input, serde_json::to_string(&inputs).unwrap()).unwrap();
    let node = node_unicode("sequences", Some(&input));
    let _ = fs::remove_dir_all(&dir);
    // Node appends the NFD/NFKD forms it found; rebuild each input from its hex key.
    let rust = node.split('\n').map(|line| {
        let key = line.split('\t').next().unwrap_or_default();
        let text: String = key
            .split(' ')
            .filter(|part| !part.is_empty())
            .filter_map(|part| u32::from_str_radix(part, 16).ok())
            .filter_map(char::from_u32)
            .collect();
        rust_unicode_line(key, &text)
    });
    let rust: Vec<String> = rust.collect();
    assert!(rust.len() > inputs.len());
    for (input, line) in inputs.iter().zip(&rust) {
        assert_eq!(
            line.split('\t').next().unwrap_or_default(),
            hex_chars(input),
            "Node echoed a different input"
        );
    }
    assert_lines_match(&node, rust.into_iter());
}
