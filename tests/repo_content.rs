//! Repository content checks: methodology links and wording, the phase graph and its
//! validation, harness skills, onboarding docs and manifests, README/CHANGELOG, the
//! zsh completion and help text, and the methodology lint rules. Ported from the former
//! Node test suite (`tests/*.mjs`) and `scripts/lint.mjs`.

use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn read_lf(rel: &str) -> String {
    read(rel).replace("\r\n", "\n")
}

fn graph() -> Value {
    serde_json::from_str(&read("core/data/phase-graph.json")).expect("phase-graph.json")
}

fn rows<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be an array"))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "aidlc-content-{prefix}-{}-{}",
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

/// Runs the binary in `cwd` with `DTECH_AIDLC_HOME` cleared unless `envs` sets it.
fn cli_in(cwd: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"));
    command
        .current_dir(cwd)
        .args(args)
        .env_remove("DTECH_AIDLC_HOME");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run dtech-aidlc")
}

fn run(project: &Path, args: &[&str]) -> Output {
    let mut all = args.to_vec();
    all.extend(["--project", project.to_str().unwrap()]);
    cli_in(project, &all, &[])
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
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
        stdout(&output)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

fn init_project(project: &Path, scope: &str, harness: &str) -> Value {
    json_ok(project, &["init", "--harness", harness, "--scope", scope])
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Writes every artifact a directive asks for (port of the Node `writeArtifacts` helper).
fn write_artifacts(project: &Path, directive: &Value) {
    let items = match &directive["artifacts"] {
        Value::Array(items) => items.clone(),
        other => other["expected"].as_array().cloned().unwrap_or_default(),
    };
    for item in items {
        let raw = text(&item, "path");
        let path = if raw.starts_with('/') || Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            project.join(raw)
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if path.exists() {
            continue;
        }
        let output = text(&item, "output");
        if output == "knowledge-record"
            || raw.contains("/.aidlc/knowledge/")
            || raw.starts_with(".aidlc/knowledge/")
        {
            let id = text(directive, "intentId");
            let title = Some(text(directive, "intent"))
                .filter(|t| !t.is_empty())
                .unwrap_or(id);
            let scope = text(directive, "scope");
            fs::write(&path, format!("---\nid: {id}\ntitle: {title}\norigin: intent\nscope: {scope}\nclosed: 2026-11-20T00:00:00Z\nphases-run: analyze, ideate, develop, archived\n---\n\n# {title}\n\n## Outcome\n\nDone.\n\n## Decisions\n\nNone.\n\n## Limitations\n\nNone.\n\n## Follow-ups\n\nNone.\n\n## Links\n\nNone.\n")).unwrap();
        } else if output == "work-units-json" {
            let units = json!({"units": [{"id": "U-01"}]});
            fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&units).unwrap()),
            )
            .unwrap();
        } else if output == "id-registry" {
            let registry = json!({"schemaVersion": 1, "ids": {}});
            fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&registry).unwrap()),
            )
            .unwrap();
        } else {
            fs::write(&path, format!("# {output}\n\nDraft.\n")).unwrap();
            if output == "work-units" {
                let units = json!({"units": [{"id": "U-01"}]});
                fs::write(
                    path.with_extension("json"),
                    format!("{}\n", serde_json::to_string_pretty(&units).unwrap()),
                )
                .unwrap();
            }
        }
    }
    let map = &directive["context"]["codebaseMap"];
    if map["required"] == json!(true) && map["exists"] == json!(false) {
        let root = PathBuf::from(text(&directive["context"], "projectRoot"));
        write(&root.join(text(map, "path")), "# Codebase map\n");
    }
}

/// Port of the Node `completeStage` helper.
fn complete_stage(project: &Path, extra: &[&str]) -> Value {
    let next = json_ok(project, &["orchestrate", "next"]);
    if next["complete"] == json!(true) {
        return next;
    }
    write_artifacts(project, &next);
    let mut args = vec!["orchestrate", "report", "--result", "complete"];
    args.extend_from_slice(extra);
    let mut report = run(project, &args);
    if !report.status.success() && stderr(&report).contains("requires explicit --approval approved")
    {
        let mut args = vec![
            "orchestrate",
            "report",
            "--result",
            "complete",
            "--approval",
            "approved",
        ];
        args.extend_from_slice(extra);
        report = run(project, &args);
    }
    assert!(report.status.success(), "{}", stderr(&report));
    next
}

fn help_text() -> String {
    stdout(&cli_in(&repo(), &["help"], &[]))
}

/// Port of the Node `helpCommands` helper: the first word of every command line in the
/// help text's `Commands:` block.
fn help_commands(help: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = help.lines().collect();
    let start = lines
        .iter()
        .position(|line| *line == "Commands:")
        .expect("Commands: block");
    lines[start + 1..]
        .iter()
        .take_while(|line| !line.is_empty())
        .filter(|line| line.starts_with("  ") && line.as_bytes().get(2).is_some_and(|b| *b != b' '))
        .map(|line| {
            line.split_whitespace()
                .next()
                .unwrap()
                .trim_end_matches(',')
                .to_owned()
        })
        .collect()
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// True when one line of `text` contains every part, in order (a `a.*b.*c` regex).
fn line_has_in_order(text: &str, parts: &[&str]) -> bool {
    text.split('\n').any(|line| {
        let line = line.split('\r').next().unwrap_or_default();
        let mut rest = line;
        parts.iter().all(|part| match rest.find(part) {
            Some(index) => {
                rest = &rest[index + part.len()..];
                true
            }
            None => false,
        })
    })
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True when `word` occurs delimited by non-`[A-Za-z0-9_-]` characters or the text ends.
fn has_delimited_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let delimiter = |b: u8| !(is_word_byte(b) || b == b'-');
    text.match_indices(word).any(|(index, _)| {
        let before = index == 0 || delimiter(bytes[index - 1]);
        let end = index + word.len();
        let after = end == bytes.len() || delimiter(bytes[end]);
        before && after
    })
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().map(Result::unwrap).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            markdown_files(&entry.path(), out);
        } else if kind.is_file() && entry.file_name().to_string_lossy().ends_with(".md") {
            out.push(entry.path());
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().map(Result::unwrap) {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

const SKILL_HARNESSES: [&str; 2] = ["claude", "deepseek"];

fn skills() -> Vec<String> {
    SKILL_HARNESSES
        .iter()
        .map(|harness| read(&format!("harness/{harness}/skills/aidlc/SKILL.md")))
        .collect()
}

// ---------------------------------------------------------------------------
// Methodology links and wording
// ---------------------------------------------------------------------------

#[test]
fn relative_markdown_links_in_methodology_resolve_outside_fenced_code() {
    let mut files = Vec::new();
    markdown_files(&repo().join("core/methodology"), &mut files);
    assert!(!files.is_empty());
    let mut missing = Vec::new();
    for file in files {
        let content = fs::read_to_string(&file).unwrap();
        let mut fence: Option<char> = None;
        for (index, line) in content.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let trimmed = line.trim_start();
            let marker = ['`', '~']
                .into_iter()
                .find(|c| trimmed.starts_with(&c.to_string().repeat(3)));
            if let Some(marker) = marker {
                fence = match fence {
                    None => Some(marker),
                    Some(open) if open == marker => None,
                    Some(open) => Some(open),
                };
                continue;
            }
            if fence.is_some() {
                continue;
            }
            let mut rest = line;
            while let Some(start) = rest.find("](") {
                let after = &rest[start + 2..];
                let Some(end) = after.find(')') else { break };
                rest = &after[end..];
                if end == 0 {
                    continue;
                }
                let target = after[..end].split(['?', '#']).next().unwrap_or_default();
                let lower = target.to_ascii_lowercase();
                if target.is_empty()
                    || ["http:", "https:", "#", "mailto:"]
                        .iter()
                        .any(|scheme| lower.starts_with(scheme))
                {
                    continue;
                }
                let resolved = file.parent().unwrap().join(target);
                if !(resolved.is_file() || resolved.is_dir()) {
                    missing.push(format!("{}:{} -> {target}", file.display(), index + 1));
                }
            }
        }
    }
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn brownfield_methodology_describes_the_one_time_codebase_map() {
    let template = read("core/methodology/templates/codebase-map.md");
    for section in 1..=10 {
        assert!(
            template.contains(&format!("## {section}.")),
            "section {section}"
        );
    }
    assert!(template.contains("relative to the project root"));
    let analyze = read("core/methodology/phases/analyze.md");
    for needle in [
        "## Existing projects (brownfield)",
        "context.codebaseMap",
        "_codebase-map.md",
        "(../templates/codebase-map.md)",
    ] {
        assert!(analyze.contains(needle), "{needle}");
    }
    let risk = read("core/methodology/agents/context-risk-analyst.md");
    assert!(line_has_in_order(
        &risk,
        &[
            "Codebase map",
            "[the codebase-map template](../templates/codebase-map.md)"
        ]
    ));
    assert!(risk.contains("Re-map a codebase that already has a map"));
    let context_line = risk
        .lines()
        .find(|line| line.starts_with("- Project context"))
        .expect("Project context line");
    assert!(context_line.contains("codebase map"));
    let brief = read("core/methodology/agents/brief-analyst.md");
    let available = brief
        .lines()
        .find(|line| line.starts_with("- Available project context"))
        .expect("Available project context line");
    assert!(available.contains("codebase map"));
    let readme = read("core/methodology/README.md");
    // A greenfield kickoff section is optional, but when present it comes first.
    let existing = readme
        .find("## Kickoff: existing project (brownfield)")
        .expect("brownfield kickoff");
    if let Some(new) = readme.find("## Kickoff: new project") {
        assert!(new < existing);
    }
    let project = TempDir::new("scope");
    assert!(run(project.path(), &["scope"]).status.success());
}

#[test]
fn registered_source_guidance_and_decision_continuity_are_documented() {
    let analyze = read("core/methodology/phases/analyze.md");
    let brief = read("core/methodology/agents/brief-analyst.md");
    let risk = read("core/methodology/agents/context-risk-analyst.md");
    let ideate = read("core/methodology/phases/ideate.md");
    let explorer = read("core/methodology/agents/solution-explorer.md");
    let facilitator = read("core/methodology/agents/decision-facilitator.md");
    let architecture = read("core/methodology/templates/architecture-design.md");
    assert!(analyze.contains("Do not pick one silently"));
    assert!(line_has_in_order(
        &brief,
        &["- Available project context", "`requirements`", "`readme`"]
    ));
    assert!(brief.contains("reported as not read"));
    assert!(risk.contains("one row in the evidence log"));
    assert!(risk.contains("Constraints found in decision records"));
    assert!(ideate.contains("would supersede"));
    for agent in [&explorer, &facilitator] {
        assert!(line_has_in_order(
            agent,
            &["`adr`", "`architecture`", "`api-spec`"]
        ));
    }
    assert!(explorer.contains("would supersede"));
    assert!(facilitator.contains("Never restart at ADR-001"));
    assert!(facilitator.contains("does not restate it"));
    assert_eq!(count(&architecture, "does not restate it"), 1);
    assert_eq!(count(&facilitator, "does not restate it"), 1);
    assert!(architecture.contains("next free number"));
    assert!(architecture.contains("see decision-record.md"));
    assert!(!architecture.contains("- Consequences:"));
    assert!(!architecture.contains("Record each significant decision as an ADR"));
    assert_eq!(count(&architecture, "ADR-001"), 2);
}

#[test]
fn methodology_approval_record_fields_match_the_gate_decided_audit_row() {
    let methodology = read_lf("core/methodology/README.md");
    let marker = "record at minimum: ";
    let start = methodology
        .find(marker)
        .expect("approval sentence not found")
        + marker.len();
    let sentence = &methodology[start..start + methodology[start..].find('.').unwrap()];
    let fields: Vec<&str> = sentence.split(',').map(str::trim).collect();
    assert_eq!(
        fields,
        [
            "gate",
            "phase",
            "stage",
            "decision",
            "result",
            "by",
            "bySource",
            "mode",
            "at",
            "note",
            "detail",
            "skippedStages",
            "artifacts"
        ]
    );
    let project = TempDir::new("approval");
    init_project(project.path(), "solo", "claude");
    json_ok(
        project.path(),
        &["orchestrate", "next", "--intent", "Approval fields"],
    );
    complete_stage(project.path(), &[]);
    complete_stage(
        project.path(),
        &["--approval", "approved", "--approved-by", "Reviewer"],
    );
    let audit = fs::read_to_string(project.path().join(".aidlc/audit.jsonl")).unwrap();
    let row = audit
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["event"] == "gate.decided")
        .expect("gate.decided row");
    for name in fields {
        assert!(row.get(name).is_some(), "gate.decided lacks {name}");
    }
}

fn set_unit_split_threshold(project: &Path, threshold: u64) {
    let path = project.join(".aidlc/config.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["unitSplitThreshold"] = json!(threshold);
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// Former Node test (pilot-feedback.test.mjs): `red-tests` warns when the plan has more
/// work units than the split threshold, and points at one baseline.
#[test]
fn red_tests_warns_above_the_unit_split_threshold_and_points_at_one_baseline() {
    let project = TempDir::new("split");
    let root = project.path();
    init_project(root, "solo", "claude");
    set_unit_split_threshold(root, 1);
    let mut next = json_ok(root, &["orchestrate", "next", "--intent", "split warning"]);
    for guard in 0.. {
        if next["stage"]["id"] == "implementation-design" {
            break;
        }
        assert!(guard < 30, "implementation-design was never selected");
        write_artifacts(root, &next);
        let mut args = vec!["orchestrate", "report", "--result", "complete"];
        if next["stage"]["phaseGate"] == json!(true) {
            args.extend(["--approval", "approved", "--approved-by", "Owner"]);
        }
        json_ok(root, &args);
        next = json_ok(root, &["orchestrate", "next"]);
    }
    write_artifacts(root, &next);
    let units = next["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["output"] == "work-units-json")
        .expect("implementation-design expects work-units-json");
    let units = text(units, "path");
    let units = if Path::new(units).is_absolute() {
        PathBuf::from(units)
    } else {
        root.join(units)
    };
    write(&units, r#"{"units":[{"id":"WU-01"},{"id":"WU-02"}]}"#);
    json_ok(root, &["orchestrate", "report", "--result", "complete"]);

    let warning = "This intent has 2 work units, more than the split threshold of 1. Consider \
                   asking the owner to split it into separate intents (for example backend, \
                   then API/UI) before building.";
    let has_warning = |directive: &Value| {
        directive["warnings"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row == warning))
    };
    let red = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(red["stage"]["id"], "red-tests");
    assert_eq!(red["currentUnit"], "WU-01");
    assert!(has_warning(&red), "{}", red["warnings"]);
    let intent = text(&red, "intentId");
    assert_eq!(
        red["context"]["baseline"]["path"],
        format!(".aidlc/intents/{intent}/develop/baseline.md")
    );
    assert_eq!(red["context"]["baseline"]["exists"], false);

    // Two units are not more than a threshold of 2.
    set_unit_split_threshold(root, 2);
    let resumed = json_ok(root, &["orchestrate", "next"]);
    assert_eq!(resumed["stage"]["id"], "red-tests");
    assert_eq!(resumed["currentUnit"], "WU-01");
    assert!(
        !resumed["warnings"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row
                .as_str()
                .is_some_and(|row| row.contains("split threshold")))),
        "{}",
        resumed["warnings"]
    );
}

// ---------------------------------------------------------------------------
// Phase graph
// ---------------------------------------------------------------------------

#[test]
fn repository_graph_validates_against_its_methodology_files() {
    let graph = dtech_aidlc::scope::read_graph(Some(&repo().join("core")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(graph.value(), &self::graph());
    for row in rows(graph.value(), "phases")
        .iter()
        .chain(rows(graph.value(), "agents"))
    {
        assert!(
            repo().join("core").join(text(row, "file")).is_file(),
            "{}",
            text(row, "file")
        );
    }
}

#[test]
fn phase_graph_schema_declares_inputs_on_every_stage() {
    let graph = graph();
    let version = text(&graph, "schemaVersion");
    let parts: Vec<&str> = version.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "{version}"
    );
    for stage in rows(&graph, "stages") {
        assert!(stage["inputs"]["artifacts"].is_array(), "{}", stage["id"]);
        assert!(stage["inputs"]["context"].is_array(), "{}", stage["id"]);
    }
}

fn graph_section(text: &str, heading: &str) -> String {
    let Some(start) = text.find(heading) else {
        return String::new();
    };
    let rest = &text[start + heading.len()..];
    let body = match rest.find("\n## ") {
        Some(next) => &rest[..next],
        None => rest,
    };
    body.to_lowercase().replace('-', " ")
}

#[test]
fn every_produced_output_is_named_in_the_agent_outputs_section() {
    let graph = graph();
    for stage in rows(&graph, "stages") {
        let agent = rows(&graph, "agents")
            .iter()
            .find(|agent| agent["id"] == stage["agent"])
            .expect("stage agent");
        let body = read(&format!("core/{}", text(agent, "file")));
        let outputs = graph_section(&body, "## Outputs");
        for id in rows(stage, "produces") {
            let id = id.as_str().unwrap();
            assert!(
                outputs.contains(&id.replace('-', " ")),
                "{} missing {id} in {}",
                stage["id"],
                text(agent, "file")
            );
        }
    }
}

#[test]
fn builder_verifier_names_the_trace_table_columns() {
    let content = read("core/methodology/agents/builder-verifier.md").to_lowercase();
    assert!(content.contains("trace table"));
    for column in [
        "story identifier",
        "criterion identifier",
        "function identifier",
        "test name",
        "result",
    ] {
        assert!(content.contains(column), "{column}");
    }
}

#[test]
fn phase_graph_owns_agent_model_aliases_and_agent_markdown_declares_none() {
    let graph = graph();
    let mut graph_agents: HashMap<String, Value> = rows(&graph, "agents")
        .iter()
        .map(|agent| (text(agent, "id").to_owned(), agent.clone()))
        .collect();
    let dir = repo().join("core/methodology/agents");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md"))
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        graph_agents.len(),
        "each graph agent must have an agent Markdown file"
    );
    for file in files {
        let content = fs::read_to_string(dir.join(&file)).unwrap();
        let body = content
            .strip_prefix("---\n")
            .or_else(|| content.strip_prefix("---\r\n"))
            .unwrap_or_else(|| panic!("{file} must have YAML frontmatter"));
        let end = body
            .find("\n---")
            .unwrap_or_else(|| panic!("{file} must have YAML frontmatter"));
        let frontmatter = body[..end].strip_suffix('\r').unwrap_or(&body[..end]);
        let lines: Vec<&str> = frontmatter
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect();
        let id = lines
            .iter()
            .filter_map(|line| line.strip_prefix("id:"))
            .map(str::trim)
            .find(|value| !value.is_empty() && !value.contains(char::is_whitespace))
            .unwrap_or_else(|| panic!("{file} must declare an agent id"))
            .to_owned();
        assert!(
            !lines.iter().any(|line| line.starts_with("model:")),
            "{file} must not declare a model; phase-graph.json owns it"
        );
        let agent = graph_agents
            .remove(&id)
            .unwrap_or_else(|| panic!("{file} agent {id} must exist in the phase graph"));
        assert!(
            dtech_aidlc::scope::MODEL_ALIASES.contains(&text(&agent, "model")),
            "{id} must have a valid graph model alias"
        );
        assert_eq!(text(&agent, "file"), format!("methodology/agents/{id}.md"));
    }
    assert!(
        graph_agents.is_empty(),
        "all graph agents must have Markdown files: {:?}",
        graph_agents.keys()
    );
}

/// An engine fixture holding the graph and a copy of the methodology. The native graph
/// validation also checks output templates, so the whole methodology is copied and the
/// unmutated fixture is checked to pass before any mutation is tried.
fn graph_fixture(base: &Value) -> TempDir {
    let dir = TempDir::new("graph");
    let core = dir.path().join("core");
    copy_dir(&repo().join("core/methodology"), &core.join("methodology"));
    write(
        &core.join("data/phase-graph.json"),
        &serde_json::to_string(base).unwrap(),
    );
    dir
}

fn scope_with_engine(root: &Path) -> Output {
    let core = root.join("core");
    cli_in(
        root,
        &["scope", "--project", root.to_str().unwrap()],
        &[("DTECH_AIDLC_HOME", &core)],
    )
}

#[test]
fn graph_rejects_invalid_schema_paths_models_and_duplicate_identifiers() {
    let base = graph();
    let pristine = graph_fixture(&base);
    let ok = scope_with_engine(pristine.path());
    assert!(ok.status.success(), "{}", stderr(&ok));
    let stage_ids = |g: &mut Value, drop: &str| {
        g["depths"]["fast"]["stages"]
            .as_array_mut()
            .unwrap()
            .retain(|id| id != drop);
    };
    type Mutation = Box<dyn Fn(&mut Value)>;
    let mutations: Vec<(&str, Mutation)> = vec![
        (
            "schema 1.0.0",
            Box::new(|g| g["schemaVersion"] = json!("1.0.0")),
        ),
        (
            "schema 1.2.0",
            Box::new(|g| g["schemaVersion"] = json!("1.2.0")),
        ),
        (
            "unknown depth stage",
            Box::new(|g| {
                g["depths"]["fast"]["stages"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("no-such-stage"))
            }),
        ),
        (
            "fast without build-and-verify",
            Box::new(move |g| stage_ids(g, "build-and-verify")),
        ),
        (
            "fast without knowledge-writeback",
            Box::new(move |g| stage_ids(g, "knowledge-writeback")),
        ),
        (
            "unknown depth output",
            Box::new(|g| {
                g["depths"]["fast"]["produces"]["problem-framing"] = json!(["not-an-output"])
            }),
        ),
        (
            "bad repeat",
            Box::new(|g| {
                let stage = g["stages"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|stage| stage["id"] == "red-tests")
                    .unwrap();
                stage["repeat"] = json!("twice");
            }),
        ),
        (
            "phase file outside",
            Box::new(|g| g["phases"][0]["file"] = json!("../../outside.md")),
        ),
        (
            "phase file missing",
            Box::new(|g| g["phases"][0]["file"] = json!("methodology/phases/missing.md")),
        ),
        (
            "unknown agent model",
            Box::new(|g| g["agents"][0]["model"] = json!("unknown")),
        ),
        (
            "unknown model alias",
            Box::new(|g| g["modelAliases"] = json!(["opus", "other"])),
        ),
        (
            "duplicate phase id",
            Box::new(|g| g["phases"][1]["id"] = g["phases"][0]["id"].clone()),
        ),
        (
            "duplicate agent id",
            Box::new(|g| g["agents"][1]["id"] = g["agents"][0]["id"].clone()),
        ),
        (
            "duplicate stage id",
            Box::new(|g| g["stages"][1]["id"] = g["stages"][0]["id"].clone()),
        ),
        (
            "phase agents reordered",
            Box::new(|g| g["phases"][0]["agents"].as_array_mut().unwrap().reverse()),
        ),
    ];
    for (label, mutate) in mutations {
        let fixture = graph_fixture(&base);
        let mut mutated = base.clone();
        mutate(&mut mutated);
        assert_ne!(mutated, base, "{label}: mutation must change the graph");
        write(
            &fixture.path().join("core/data/phase-graph.json"),
            &serde_json::to_string(&mutated).unwrap(),
        );
        let result = scope_with_engine(fixture.path());
        assert!(!result.status.success(), "{label}: {}", stdout(&result));
        assert!(
            stderr(&result).contains("Invalid phase-graph.json"),
            "{label}: {}",
            stderr(&result)
        );
    }
}

#[test]
fn graph_references_existing_methodology_and_directive_paths_are_absolute() {
    let graph = graph();
    let project = TempDir::new("directive");
    init_project(project.path(), "solo", "claude");
    let root = repo();
    let output = cli_in(
        project.path(),
        &[
            "orchestrate",
            "next",
            "--intent",
            "fixture",
            "--project",
            project.path().to_str().unwrap(),
        ],
        &[("DTECH_AIDLC_HOME", &root)],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let directive: Value = serde_json::from_slice(&output.stdout).unwrap();
    let stage = &directive["stage"];
    let phase_file = repo()
        .join("core")
        .join(text(&rows(&graph, "phases")[0], "file"));
    let agent_file = repo()
        .join("core")
        .join(text(&rows(&graph, "agents")[0], "file"));
    assert_eq!(Path::new(text(stage, "phaseFile")), phase_file.as_path());
    assert_eq!(Path::new(text(stage, "agentFile")), agent_file.as_path());
    assert!(Path::new(text(stage, "phaseFile")).is_absolute());
}

// ---------------------------------------------------------------------------
// Harness skills, manifests and onboarding docs
// ---------------------------------------------------------------------------

#[test]
fn skills_use_file_based_requests_and_current_directive_report_contracts() {
    for skill in skills() {
        assert_eq!(count(&skill, "{{"), 0);
        for forbidden in [
            "--intent \"",
            "in `<phase>/<stage>/`",
            "notices",
            "dtech-aidlc init",
            "core/methodology/contracts.md",
        ] {
            assert!(!skill.contains(forbidden), "unexpected {forbidden}");
        }
        for required in [
            "--intent-file .aidlc/request.md",
            "`warnings`",
            "`context.warnings`",
            "the directive's `artifacts` list",
            "`template`",
            "`stage.phaseGate`",
            "`gateMode`",
            "--needs <prerequisite>",
            "dtech-aidlc ids check",
            "never mints one",
            "`stage.rework`",
            "`suggestedBranch`",
            "exits with status 3",
            "Do not pass `--force-resume`",
            "Check the environment on turn one",
            "Recommend `standard` for a pilot",
            "`gate.id`",
            "--approved-by",
            "dtech-aidlc intent abandon",
            "`complete: true`",
            "a `hint`",
            "AskUserQuestion",
            "(Recommended)",
            "`Other`",
        ] {
            assert!(skill.contains(required), "missing {required}");
        }
    }
}

#[test]
fn claude_skill_uses_model_preferences_safely_for_stage_delegation() {
    let claude = &skills()[0];
    for required in [
        "`stage.model`",
        "Claude Code Agent tool",
        "passing `stage.model` explicitly as the requested model",
        "`stage.model` takes precedence over any `model` recommendation in `stage.agentFile`",
        "directive carries the effective configured preference",
        "verify that the Agent tool call actually includes the directive's `stage.model`",
        "requested model",
        "expected artifacts",
        "do the stage directly and disclose that the preference could not be honored",
        "must not run orchestration/report commands",
        "must not",
        "human-mode gates still require explicit human approval",
        "AI-mode gates still require the conductor",
    ] {
        assert!(claude.contains(required), "missing {required}");
    }
}

#[test]
fn claude_ai_mode_runs_one_dynamic_workflow_per_stage_directive() {
    let claude = &skills()[0];
    for required in [
        "In AI mode, use Claude Code's dynamic `Workflow` tool",
        "Analyze, Ideate, Develop, Launch, and Curate",
        "one workflow for each stage directive",
        "do not run workflows for phases omitted by scope or depth",
        "must not pre-create artifacts",
        "In human mode, do not require dynamic workflows",
        "do the stage work directly and disclose the fallback",
    ] {
        assert!(claude.contains(required), "missing {required}");
    }
    assert!(!claude.contains("one workflow for each such phase"));
}

#[test]
fn claude_delegation_briefs_from_directive_and_templates_with_a_fixed_registry() {
    let claude = &skills()[0];
    for required in [
        "Brief the delegate from the CLI and the templates, not from your summary of them",
        "directive JSON in the prompt verbatim",
        "every `template` path",
        "never substituted for it",
        "never minted",
    ] {
        assert!(claude.contains(required), "missing {required}");
    }
}

#[test]
fn ai_mode_gate_approval_in_both_skills_requires_classified_open_items() {
    for skill in skills() {
        for required in [
            "## Open items",
            "`[written-approval]`",
            "`[follow-up]`",
            "`- None`",
            "A recommendation that defers a decision is not an approval",
            "When `gateMode` is `human`, present the gate to the owner",
        ] {
            assert!(skill.contains(required), "missing {required}");
        }
    }
}

#[test]
fn skills_and_methodology_explain_unit_scheduling() {
    for skill in skills() {
        for required in [
            "## Work units, park, queue and revisit",
            "--unit <id>",
            "`regression.json`",
            "dtech-aidlc orchestrate park --unit <id> --kind dep-unit|foreign-regression|owner-decision|design-defect",
            "dtech-aidlc orchestrate queue",
            "dtech-aidlc orchestrate revisit --unit <id> --detail \"<decision>\"",
            "`reason: \"queue-stalled\"`",
            "`parked.auto: true`",
            "dtech-aidlc intent rework --unit <id>",
            "dtech-aidlc intent split <id> --into <id>,<id>",
            "the final regression sweep",
            "never decide for them",
        ] {
            assert!(skill.contains(required), "missing {required}");
        }
    }
    let develop = read_lf("core/methodology/phases/develop.md");
    for required in [
        "## Work-unit scheduling",
        "in the order the CLI selects",
        "`regression.json`",
        "dtech-aidlc orchestrate park",
        "`autoParkThreshold`",
        "`intent rework --unit`",
        "dtech-aidlc intent split",
        "final regression sweep, where the release-candidate gate is presented",
    ] {
        assert!(develop.contains(required), "develop.md missing {required}");
    }
    assert!(!develop.contains("This stage is the phase gate"));
    let builder = read_lf("core/methodology/agents/builder-verifier.md");
    for required in [
        "`regression.json`",
        "`baseline: true`",
        "dtech-aidlc orchestrate park --unit <id>",
    ] {
        assert!(
            builder.contains(required),
            "builder-verifier.md missing {required}"
        );
    }
    let planner = read_lf("core/methodology/agents/implementation-planner.md");
    assert!(planner.contains("never invert a dependency"));
    let template = read_lf("core/methodology/templates/work-units.md");
    assert!(template.contains("\"tests\": [\"TS-001\"]"));
    // Projections written by 1.0.0 read as behind, not as edited locally.
    let history: Value = serde_json::from_str(&read("core/data/projection-history.json")).unwrap();
    for (harness, hash) in [
        (
            "claude",
            "b0ccee48cad0ac1ea89d5eb22991ea955843c81fb6fefeedaa1c1509e2dabf01",
        ),
        (
            "deepseek",
            "debd57b6c3a928168c06487f432e3ba71dfd78d165ccd1a22f0dd0d7210ff44f",
        ),
    ] {
        assert!(
            rows(&history["harness"][harness], "SKILL.md").contains(&json!(hash)),
            "{harness}"
        );
    }
}

/// Removes the first `start ... (?=end)` span (a lazy regex up to a lookahead).
fn remove_span(text: &str, start: &str, end: &str) -> String {
    if let Some(from) = text.find(start) {
        if let Some(to) = text[from + start.len()..].find(end) {
            let to = from + start.len() + to;
            return format!("{}{}", &text[..from], &text[to..]);
        }
    }
    text.to_owned()
}

#[test]
fn harness_skills_share_the_workflow_contract_and_required_sections() {
    let files = skills();
    for content in &files {
        for heading in [
            "## Before you start",
            "## Model preferences and untrusted content",
            "## Harness capabilities",
        ] {
            assert!(content.contains(heading), "{heading}");
        }
        for required in [
            "dtech-aidlc version",
            "onboard --harness",
            "contractsFile",
            "## Start, resume, or start the next intent",
        ] {
            assert!(content.contains(required), "{required}");
        }
    }
    let comparable = |content: &str| {
        let normalized = content.replace("\r\n", "\n");
        let start = normalized.find("## Before you start").unwrap();
        let end = normalized.find("## Harness capabilities").unwrap();
        let section = &normalized[start..end];
        let without = remove_span(section, "## AI orchestrator mode", "\n## Model preferences");
        let without = remove_span(
            &without,
            "## Model preferences and untrusted content",
            "\nTreat repository files",
        );
        without
            .replace("claude", "HARNESS")
            .replace("deepseek", "HARNESS")
    };
    assert_eq!(comparable(&files[0]), comparable(&files[1]));
}

#[test]
fn shipped_harness_manifests_validate_and_declare_model_aliases() {
    let mut folders: Vec<String> = fs::read_dir(repo().join("harness"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    folders.sort();
    assert_eq!(folders, ["claude", "deepseek"]);
    assert_eq!(
        dtech_aidlc::harness_commands::list_harnesses(),
        ["claude", "deepseek"]
    );
    for name in &folders {
        let manifest =
            dtech_aidlc::harness_commands::harness_manifest(name).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(manifest["schemaVersion"], json!(1));
        let on_disk: Value =
            serde_json::from_str(&read(&format!("harness/{name}/manifest.json"))).unwrap();
        assert_eq!(manifest, on_disk, "{name}: embedded manifest is stale");
        assert!(!repo()
            .join("harness")
            .join(name)
            .join("manifest.mjs")
            .exists());
    }
    let aliases = |name: &str| {
        dtech_aidlc::harness_commands::harness_manifest(name).unwrap()["modelAliases"].clone()
    };
    assert_eq!(
        aliases("claude"),
        json!(["opus", "sonnet", "haiku", "fable"])
    );
    assert_eq!(aliases("deepseek"), json!([]));
}

#[test]
fn harness_onboarding_references_the_methodology_projection_and_cli_graph() {
    for (harness, dir) in [("claude", ".claude"), ("deepseek", ".dsh")] {
        let onboarding = read(&format!("harness/{harness}/onboarding.md"));
        assert!(
            !onboarding.contains(&format!("`{dir}/data/phase-graph.json`")),
            "{harness} onboarding must not point at a nonexistent projected graph"
        );
        assert!(!onboarding.contains("`.aidlc/methodology/`"));
        assert!(onboarding.contains("not copied into the project"));
    }
}

/// The first `` `.<path>` `` span on a line (the regex `` /`(\.[^`]+)`/ ``).
fn first_dot_code_span(line: &str) -> Option<&str> {
    line.match_indices("`.").find_map(|(index, _)| {
        let inner = &line[index + 1..];
        inner[1..]
            .find('`')
            .filter(|close| *close >= 1)
            .map(|close| &inner[..close + 1])
    })
}

#[test]
fn onboarding_docs_match_harness_manifests_and_current_project_layout() {
    for harness in SKILL_HARNESSES {
        let manifest: Value =
            serde_json::from_str(&read(&format!("harness/{harness}/manifest.json"))).unwrap();
        let doc = read(&format!("harness/{harness}/onboarding.md"));
        assert!(!doc.contains("tools/"), "{harness}");
        for required in [
            "dtech-aidlc version",
            "dtech-aidlc onboard --harness",
            "dtech-aidlc harness refresh",
            "PROJECT.md",
            "aidlc.knowledge.json",
            ".aidlc/knowledge/",
            ".aidlc/shared/",
        ] {
            assert!(doc.contains(required), "{harness} missing {required}");
        }
        let skill_dir = text(&manifest, "skillDir");
        let name = text(&manifest, "name");
        assert!(doc.contains(&format!("- `{skill_dir}/SKILL.md`")));
        assert!(doc.contains(text(&manifest, "invoke")));
        assert!(doc.contains(&format!("--harness {name}")));
        let root = TempDir::new("onboarding-docs");
        let scope = if name == "deepseek" { "team" } else { "solo" };
        let args = ["onboard", "--harness", name, "--scope", scope];
        let onboard = run(root.path(), &args);
        assert!(onboard.status.success(), "{}", stderr(&onboard));
        assert_eq!(
            fs::read(root.path().join(skill_dir).join("SKILL.md")).unwrap(),
            fs::read(repo().join(format!("harness/{harness}/skills/aidlc/SKILL.md"))).unwrap()
        );
        let config_path = root.path().join(".aidlc/config.json");
        let before = fs::read(&config_path).unwrap();
        let repeated = run(root.path(), &args);
        assert!(repeated.status.success(), "{}", stderr(&repeated));
        assert_eq!(fs::read(&config_path).unwrap(), before);
        for line in doc.split('\n') {
            if !line.starts_with("- ") {
                continue;
            }
            if let Some(path) = first_dot_code_span(line) {
                assert!(
                    root.path().join(path).exists(),
                    "{harness} installed path missing: {path}"
                );
            }
        }
    }
}

#[test]
fn projection_history_has_the_required_hashes_and_helpers_normalize() {
    let history: Value = serde_json::from_str(&read("core/data/projection-history.json")).unwrap();
    assert!(text(&history, "source").starts_with("git 91c9eea.."));
    let skill = rows(&history["harness"]["claude"], "SKILL.md");
    for hash in [
        "1b6aebe53308e796d0ac355ae067c142e8cdd220037ffa6e0c4cfdbb8154f637",
        "89b889881b09d4c665bef9a0accdfa85e20e5ef81cb6bdfaed10780a16c95b77",
        "cefb675bfd8665a0ad7be8e8ee36fb755b52e5b25f9673682d53fc870bf44199",
    ] {
        assert!(skill.contains(&json!(hash)), "{hash}");
    }
    for path in history["methodology"].as_object().unwrap().keys() {
        assert!(
            !path.starts_with('/') && !path.split('/').any(|part| part == ".."),
            "{path}"
        );
    }
    use dtech_aidlc::harness_commands::{compare_versions, hash_text};
    assert_eq!(hash_text(b"a\r\nb"), hash_text(b"a\nb"));
    assert_eq!(
        compare_versions("0.2.0", "0.10.0"),
        std::cmp::Ordering::Less
    );
    assert_eq!(compare_versions("bad", "0.0.0"), std::cmp::Ordering::Equal);
}

// ---------------------------------------------------------------------------
// Help, completion and the command table
// ---------------------------------------------------------------------------

/// Every command key of the CLI (recorded from the former Node `COMMAND_FLAGS` table).
const COMMAND_KEYS: &[&str] = &[
    "init",
    "onboard",
    "orchestrate next",
    "orchestrate report",
    "orchestrate park",
    "orchestrate queue",
    "orchestrate revisit",
    "ids check",
    "scope",
    "status",
    "doctor",
    "intent list",
    "intent pause",
    "intent resume",
    "intent abandon",
    "intent depth",
    "intent rework",
    "intent split",
    "intent assign",
    "config agents",
    "config scope",
    "config mode",
    "config gate-mode",
    "config prerequisite",
    "harness add",
    "repos add",
    "repos pull",
    "repos list",
    "repos remove",
    "deps add",
    "deps list",
    "deps pull",
    "completion zsh",
    "knowledge init",
    "knowledge sources",
    "knowledge list",
    "knowledge show",
    "knowledge check",
    "knowledge index",
    "harness status",
    "harness refresh",
];

fn completion_text() -> String {
    let output = cli_in(&repo(), &["completion", "zsh"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    stdout(&output)
}

#[test]
fn help_completion_and_the_command_table_stay_in_step() {
    let help = help_text();
    let completion = completion_text();
    let names: BTreeSet<String> = completion
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.len() == line.len() {
                return None;
            }
            let name = trimmed.strip_prefix('\'')?;
            let end = name.find(':')?;
            let name = &name[..end];
            (!name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
                .then(|| name.to_owned())
        })
        .collect();
    assert_eq!(help_commands(&help), names);
    let mut table: BTreeSet<String> = COMMAND_KEYS
        .iter()
        .map(|key| key.split(' ').next().unwrap().to_owned())
        .collect();
    table.remove("help");
    table.remove("version");
    let mut from_help = help_commands(&help);
    from_help.remove("help");
    from_help.remove("version");
    assert_eq!(from_help, table);
    for key in COMMAND_KEYS {
        let flags = dtech_aidlc::args::command_flags(key)
            .unwrap_or_else(|| panic!("{key} is missing from the command table"));
        for flag in flags {
            let needle = format!("--{}", flag.name);
            let documented = help.match_indices(&needle).any(|(index, _)| {
                help.as_bytes()
                    .get(index + needle.len())
                    .is_none_or(|b| !is_word_byte(*b))
            });
            assert!(documented, "{key} option {needle}");
        }
    }
    let graph = graph();
    let mut words: BTreeSet<String> = rows(&graph, "agents")
        .iter()
        .map(|agent| text(agent, "id").to_owned())
        .collect();
    words.extend(graph["scopes"].as_object().unwrap().keys().cloned());
    for entry in fs::read_dir(repo().join("harness")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            words.insert(name);
        }
    }
    words.extend(
        dtech_aidlc::scope::MODEL_ALIASES
            .iter()
            .map(|alias| (*alias).to_owned()),
    );
    for word in words {
        assert!(has_delimited_word(&completion, &word), "{word}");
    }
    let scratch = TempDir::new("zsh");
    let file = scratch.path().join("completion.zsh");
    fs::write(&file, &completion).unwrap();
    match Command::new("zsh").arg("-n").arg(&file).output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("zsh: {e}"),
        Ok(output) => assert!(output.status.success(), "{}", stderr(&output)),
    }
}

// ---------------------------------------------------------------------------
// README and CHANGELOG
// ---------------------------------------------------------------------------

#[test]
fn readme_has_the_required_sections_and_mentions() {
    let readme = read_lf("README.md");
    for heading in [
        "Onboard a project",
        "Project knowledge",
        "What to commit",
        "Harnesses",
        "Upgrade and uninstall",
        "Environment variables",
    ] {
        let line = format!("## {heading}");
        assert!(readme.lines().any(|l| l == line), "{heading}");
    }
    for mention in [
        "PROJECT.md",
        "aidlc.knowledge.json",
        "DTECH_AIDLC_HOME",
        "dtech-aidlc harness refresh",
        "CHANGELOG.md",
    ] {
        assert!(readme.contains(mention), "{mention}");
    }
}

#[test]
fn every_dtech_aidlc_command_in_a_readme_sh_block_is_listed_by_help() {
    let readme = read_lf("README.md");
    let known = help_commands(&help_text());
    let mut words = Vec::new();
    let mut rest = readme.as_str();
    while let Some(start) = rest.find("```sh\n") {
        let block = &rest[start + 6..];
        let Some(end) = block.find("```") else { break };
        for line in block[..end].split('\n') {
            if let Some(after) = line.strip_prefix("dtech-aidlc") {
                if after.starts_with(char::is_whitespace) {
                    if let Some(word) = after.split_whitespace().next() {
                        words.push(word.to_owned());
                    }
                }
            }
        }
        rest = &block[end + 3..];
    }
    assert!(words.len() > 10, "{words:?}");
    for word in words {
        assert!(known.contains(&word), "README uses unknown command {word}");
    }
}

#[test]
fn changelog_has_a_section_for_each_release_and_the_crate_version() {
    let changelog = read_lf("CHANGELOG.md");
    let crate_version = env!("CARGO_PKG_VERSION");
    for version in [
        "0.1.3",
        "0.2.0",
        "0.3.0",
        "0.4.0",
        "0.5.0",
        "0.6.0",
        "0.6.1",
        "0.6.2",
        "0.6.3",
        "0.6.4",
        "0.7.0",
        "0.7.1",
        "0.7.2",
        "1.0.0",
        crate_version,
    ] {
        let heading = format!("## {version}");
        assert!(changelog.lines().any(|line| line == heading), "{version}");
    }
    assert!(
        read("README.md").contains("(.github/badges/version.svg)"),
        "README must use the local version badge"
    );
    let badge = format!("<title>version: {crate_version}</title>");
    assert!(
        read(".github/badges/version.svg").contains(&badge),
        "README version badge must show {crate_version}"
    );
}

// ---------------------------------------------------------------------------
// Methodology lint (former scripts/lint.mjs)
// ---------------------------------------------------------------------------

const NUMBER_WORDS: &[&str] = &[
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen",
];
const COUNTED: &[&str] = &["agents", "phases", "stages", "scopes", "gates"];

/// The regex `\b(\d+|one|…|fifteen)\s+(agents|phases|stages|scopes|gates)\b`, case-insensitive.
fn writes_a_count(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_word_byte(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_word_byte(bytes[i]) {
                i += 1;
            }
            tokens.push((start, i));
        } else {
            i += 1;
        }
    }
    tokens.windows(2).any(|pair| {
        let (a, b) = (pair[0], pair[1]);
        let first = line[a.0..a.1].to_ascii_lowercase();
        let second = line[b.0..b.1].to_ascii_lowercase();
        let gap = &line[a.1..b.0];
        let number =
            first.bytes().all(|c| c.is_ascii_digit()) || NUMBER_WORDS.contains(&first.as_str());
        number
            && !gap.is_empty()
            && gap.chars().all(char::is_whitespace)
            && COUNTED.contains(&second.as_str())
    })
}

fn lint_methodology_readme(root: &Path, graph: &Value, errors: &mut Vec<String>) {
    let file = "core/methodology/README.md";
    let empty = Vec::new();
    let list = |key: &str| graph[key].as_array().unwrap_or(&empty).clone();
    let phases = list("phases");
    if let Ok(content) = fs::read_to_string(root.join(file)) {
        let content = content.replace("\r\n", "\n");
        let phase_name = |id: &str| {
            phases
                .iter()
                .find(|phase| phase["id"] == id)
                .and_then(|phase| phase["name"].as_str())
                .unwrap_or(id)
                .to_owned()
        };
        let code = |id: &str| format!("`{id}`");
        let names = |value: &Value| -> Vec<String> {
            value
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_owned())
                .collect()
        };
        let scopes = graph["scopes"]
            .as_object()
            .map(|scopes| {
                scopes
                    .iter()
                    .map(|(scope, ids)| {
                        let names: Vec<String> =
                            names(ids).iter().map(|id| phase_name(id)).collect();
                        format!("- **{scope}:** {}.", names.join(", "))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let agents = format!(
            "The graph contains {} agents: {}.",
            list("agents").len(),
            phases
                .iter()
                .map(|phase| format!(
                    "{} ({})",
                    text(phase, "name"),
                    names(&phase["agents"])
                        .iter()
                        .map(|id| code(id))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
        let mut depths =
            vec!["- **full:** every stage of the selected scope (the default).".to_owned()];
        if let Some(map) = graph["depths"].as_object() {
            for (name, depth) in map {
                depths.push(format!(
                    "- **{name}:** {}{}.",
                    names(&depth["stages"]).join(", "),
                    if depth.get("produces").is_some_and(|p| !p.is_null()) {
                        ", with a reduced output list"
                    } else {
                        ""
                    }
                ));
            }
        }
        for (name, want) in [
            ("scopes", scopes),
            ("agents", agents),
            ("depths", depths.join("\n")),
        ] {
            let open = format!("<!-- generated:{name} -->");
            let close = format!("<!-- /generated:{name} -->");
            match (content.find(&open), content.find(&close)) {
                (Some(from), Some(to)) if to > from => {
                    if content[from + open.len()..to].trim() != want {
                        errors.push(format!(
                            "{file}: section {name} is out of date with phase-graph.json; expected: {want}"
                        ));
                    }
                }
                _ => errors.push(format!("{file}: missing generated section {name}")),
            }
        }
        let lines: Vec<&str> = content.split('\n').collect();
        for phase in &phases {
            let heading = format!("### {}", text(phase, "name"));
            let gate = code(text(phase, "gate"));
            let named = lines
                .iter()
                .position(|line| *line == heading)
                .is_some_and(|start| {
                    lines[start + 1..]
                        .iter()
                        .take_while(|line| !line.starts_with('#'))
                        .any(|line| line.contains(&gate))
                });
            if !named {
                errors.push(format!(
                    "{file}: phase {} has no section naming gate {}",
                    text(phase, "name"),
                    text(phase, "gate")
                ));
            }
        }
        let mut in_region = false;
        for (index, line) in lines.iter().enumerate() {
            if line.starts_with("<!-- generated:") {
                in_region = true;
            } else if line.starts_with("<!-- /generated:") {
                in_region = false;
            } else if !in_region && writes_a_count(line) {
                errors.push(format!(
                    "{file}:{}: put counts of agents, phases, stages, scopes or gates inside a generated section",
                    index + 1
                ));
            }
        }
    }
    // An agent that calls itself a gate must serve only the last stage of its phase.
    for agent in list("agents") {
        let Ok(body) = fs::read_to_string(root.join("core").join(text(&agent, "file"))) else {
            continue;
        };
        let calls_itself_gate = body.split('\n').any(|line| {
            let line = line.split('\r').next().unwrap_or_default();
            line.find("This stage is the ")
                .is_some_and(|i| line[i + "This stage is the ".len()..].contains(" gate"))
        });
        if !calls_itself_gate {
            continue;
        }
        for stage in list("stages")
            .iter()
            .filter(|stage| stage["agent"] == agent["id"])
        {
            let last = phases
                .iter()
                .find(|phase| phase["id"] == stage["phase"])
                .and_then(|phase| phase["stages"].as_array())
                .and_then(|stages| stages.last());
            if last != Some(&stage["id"]) {
                errors.push(format!(
                    "{}: calls itself a gate but serves non-final stage {}",
                    text(&agent, "file"),
                    text(stage, "id")
                ));
            }
        }
    }
}

/// The methodology rules of the former `scripts/lint.mjs` (Node runtime metadata and
/// JavaScript syntax checks are gone with the Node CLI).
fn lint(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let path = root.join("core/data/phase-graph.json");
    let Ok(content) = fs::read_to_string(&path) else {
        return vec!["phase-graph.json is missing".to_owned()];
    };
    let graph: Value = match serde_json::from_str(&content) {
        Ok(graph) => graph,
        Err(e) => return vec![format!("phase-graph.json is invalid: {e}")],
    };
    if !graph.is_object() {
        return vec!["phase-graph.json must contain a JSON object".to_owned()];
    }
    for key in ["phases", "stages", "agents"] {
        if !graph[key].is_array() {
            errors.push(format!("phase-graph.json must contain a {key} array"));
        }
    }
    let empty = Vec::new();
    let list = |key: &str| graph[key].as_array().unwrap_or(&empty);
    let ids = |key: &str| -> BTreeSet<String> {
        list(key)
            .iter()
            .filter_map(|row| row["id"].as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let phases = ids("phases");
    let agents = ids("agents");
    if graph["schemaVersion"] != json!(dtech_aidlc::scope::GRAPH_SCHEMA_VERSION) {
        errors.push(format!(
            "phase-graph schemaVersion must be {}",
            dtech_aidlc::scope::GRAPH_SCHEMA_VERSION
        ));
    }
    for phase in list("phases") {
        if text(phase, "id").is_empty() || text(phase, "name").is_empty() {
            errors.push("each phase must define id and name".to_owned());
        }
    }
    for stage in list("stages") {
        if text(stage, "id").is_empty() || !phases.contains(text(stage, "phase")) {
            errors.push(format!(
                "stage {} references unknown phase {}",
                stage["id"], stage["phase"]
            ));
        }
        if !stage["agent"].is_null() && !agents.contains(text(stage, "agent")) {
            errors.push(format!(
                "stage {} references unknown agent {}",
                stage["id"], stage["agent"]
            ));
        }
    }
    for agent in list("agents") {
        if text(agent, "id").is_empty() || !phases.contains(text(agent, "phase")) {
            errors.push(format!(
                "agent {} references unknown phase {}",
                agent["id"], agent["phase"]
            ));
        }
    }
    match &graph["scopes"] {
        Value::Null => {}
        Value::Object(scopes) => {
            for (scope, included) in scopes {
                match included.as_array() {
                    None => errors.push(format!("scope {scope} must list phases")),
                    Some(included) => {
                        for phase in included {
                            if !phase.as_str().is_some_and(|p| phases.contains(p)) {
                                errors.push(format!(
                                    "scope {scope} references unknown phase {phase}"
                                ));
                            }
                        }
                    }
                }
            }
        }
        _ => errors.push("phase-graph scopes must be an object".to_owned()),
    }
    let stage_ids: Vec<&str> = list("stages")
        .iter()
        .filter_map(|stage| stage["id"].as_str())
        .filter(|id| !id.is_empty())
        .collect();
    if stage_ids.iter().collect::<BTreeSet<_>>().len() != stage_ids.len() {
        errors.push("stage IDs must be unique".to_owned());
    }
    lint_methodology_readme(root, &graph, &mut errors);
    if graph["modelAliases"] != json!(dtech_aidlc::scope::MODEL_ALIASES) {
        errors.push(format!(
            "phase-graph modelAliases must be {}",
            dtech_aidlc::scope::MODEL_ALIASES.join(", ")
        ));
    }
    errors
}

/// A copy of `core/` that lint checks in isolation.
fn lint_copy() -> TempDir {
    let dir = TempDir::new("lint");
    copy_dir(&repo().join("core/data"), &dir.path().join("core/data"));
    copy_dir(
        &repo().join("core/methodology"),
        &dir.path().join("core/methodology"),
    );
    dir
}

fn mutate(root: &Path, file: &str, change: impl Fn(&str) -> String) {
    let path = root.join(file);
    let before = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let after = change(&before);
    assert_ne!(after, before, "mutation must change the file");
    fs::write(&path, after).unwrap();
}

const METHODOLOGY_README: &str = "core/methodology/README.md";

fn assert_lint_fails(root: &Path, expected: &str) {
    let errors = lint(root);
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected {expected:?} in {errors:?}"
    );
}

#[test]
fn lint_passes_on_the_repository() {
    assert_eq!(lint(&repo()), Vec::<String>::new());
}

#[test]
fn copied_lint_still_passes() {
    let copy = lint_copy();
    assert_eq!(lint(copy.path()), Vec::<String>::new());
}

#[test]
fn a_wrong_agent_count_in_the_methodology_readme_fails_lint() {
    let copy = lint_copy();
    mutate(copy.path(), METHODOLOGY_README, |t| {
        t.replace("12 agents", "10 agents")
    });
    assert_lint_fails(copy.path(), "section agents is out of date");
}

#[test]
fn a_scope_list_without_archived_fails_lint() {
    let copy = lint_copy();
    mutate(copy.path(), METHODOLOGY_README, |t| {
        t.replace(
            "- **solo:** Analyze, Ideate, Develop, Archived.",
            "- **solo:** Analyze, Ideate, Develop.",
        )
    });
    assert_lint_fails(copy.path(), "section scopes is out of date");
}

#[test]
fn a_phase_without_a_readme_section_naming_its_gate_fails_lint() {
    let copy = lint_copy();
    mutate(copy.path(), METHODOLOGY_README, |t| {
        t.replace("### Archived", "### Closing")
    });
    assert_lint_fails(copy.path(), "phase Archived has no section naming gate");
}

#[test]
fn a_count_written_outside_a_generated_section_fails_lint() {
    let copy = lint_copy();
    mutate(copy.path(), METHODOLOGY_README, |t| {
        format!("{t}\nThere are six phases.\n")
    });
    assert_lint_fails(copy.path(), "put counts");
}

#[test]
fn an_agent_that_calls_itself_a_gate_on_a_non_final_stage_fails_lint() {
    let copy = lint_copy();
    mutate(
        copy.path(),
        "core/methodology/agents/builder-verifier.md",
        |t| format!("{t}\nThis stage is the Develop gate.\n"),
    );
    assert_lint_fails(
        copy.path(),
        "calls itself a gate but serves non-final stage red-tests",
    );
}

#[test]
fn graph_model_aliases_must_match_the_cli_allowlist_in_lint() {
    let copy = lint_copy();
    mutate(copy.path(), "core/data/phase-graph.json", |t| {
        let mut graph: Value = serde_json::from_str(t).unwrap();
        graph["modelAliases"] = json!(["opus", "sonnet"]);
        serde_json::to_string(&graph).unwrap()
    });
    assert_lint_fails(copy.path(), "phase-graph modelAliases must be");
}
