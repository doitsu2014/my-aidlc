//! Read-only orchestration context construction, ported from Node's
//! `buildDirectiveContext` (`core/tools/context.mjs`).
//!
//! The envelope carries `request`, `project`, `sources`, `sourceChanges`
//! (Node `sourceStatus`), `codebaseMap` (Node `codebaseMapStatus`), the
//! per-stage `baseline` hint and `knowledge`. Advisory sections degrade to
//! warnings exactly where Node catches errors. Every project file is read
//! through [`crate::onboarding::safe_join`], so symbolic links inside the
//! project are refused instead of followed (Node follows some of them).
//! Methodology templates point under the engine root the caller passes (Node
//! `engineRoot()`): the `DTECH_AIDLC_HOME` engine when one is selected, otherwise the
//! embedded methodology materialized at `<project>/.aidlc/engine`.

use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_GRANTS: &[&str] = &[
    "request",
    "project",
    "sources",
    "knowledge",
    "profile",
    "repos",
];

const PROJECT_SECTIONS: &[&str] = &[
    "Environment prerequisites",
    "Commands",
    "Known baseline failures",
];

const CODEBASE_MAP: &str = ".aidlc/knowledge/_codebase-map.md";
const CODEBASE_MAP_TEMPLATE: &str = "codebase-map.md";
const KNOWLEDGE_RECORD_TEMPLATE: &str = "knowledge-record.md";
const PROFILE_JSON: &str = ".aidlc/shared/project-profile.json";
const SNAPSHOT_JSON: &str = ".aidlc/shared/sources-snapshot.json";
const HASH_LIMIT: u64 = 20 * 1024 * 1024;
const SNAPSHOT_CAP: usize = 2000;
const MAX_DEPTH: usize = 6;
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "coverage",
    ".aidlc",
    ".claude",
    ".dsh",
    "vendor",
    "target",
    "out",
    "obj",
    "venv",
    "virtualenv",
    "__pycache__",
    "site-packages",
    "bower_components",
    "Pods",
    "third_party",
];

/// Input needed to construct context for a selected stage.
/// The graph supplies metadata for knowledge writeback.
#[derive(Debug, Clone, Copy)]
pub struct ContextInput<'a> {
    pub root: &'a Path,
    pub state: &'a Value,
    pub stage: &'a Value,
    pub graph: &'a Value,
}

/// Build the read-only context envelope for a stage.
///
/// Mirrors Node `buildDirectiveContext`: keys `request`, `project`, `sources`,
/// `sourceChanges`, `codebaseMap`, `baseline` and `knowledge`, with the same
/// warnings and degradation rules. The `repos` grant is preserved in `granted`
/// but, as in Node, produces no payload. Only an unusable project root returns
/// an error (Node returns `null` for the whole context on unexpected failures).
///
/// Template paths point under the embedded engine `<project>/.aidlc/engine`; use
/// [`build_with_engine`] when another engine root is in use.
pub fn build(input: ContextInput<'_>) -> Result<Value, String> {
    let root = absolute_root(input.root)?;
    build_with_engine(input, &embedded_engine_root(&root))
}

/// [`build`] with methodology templates under `engine_root` (Node `engineRoot()`, the
/// directory holding `methodology/`).
pub fn build_with_engine(input: ContextInput<'_>, engine_root: &Path) -> Result<Value, String> {
    let root = absolute_root(input.root)?;
    if !root.is_dir() {
        return Err("Project root must be a directory".into());
    }
    let intent_id = input
        .state
        .get("intentId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let granted = input
        .stage
        .pointer("/inputs/context")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| DEFAULT_GRANTS.iter().map(|name| json!(name)).collect());
    let mut context = json!({
        "schemaVersion": 1,
        "projectRoot": root,
        "granted": granted,
        "warnings": []
    });

    if includes(&context["granted"], "request") {
        if let Some(id) = intent_id {
            context["request"] = json!({
                "path": format!(".aidlc/intents/{id}/intent.md"),
                "provenance": "User-provided"
            });
        }
    }

    if includes(&context["granted"], "project") || includes(&context["granted"], "sources") {
        match crate::knowledge::load_knowledge_manifest(&root) {
            Ok((manifest_exists, manifest)) => {
                let inspected = crate::knowledge::inspect_sources(&root, &manifest);
                if includes(&context["granted"], "project") {
                    let project = if inspected["project"].is_object() {
                        let mut row = inspected["project"].clone();
                        row["provenance"] = json!("User-provided");
                        row
                    } else {
                        Value::Null
                    };
                    context["project"] = project;
                    if context["project"].is_null() {
                        push_warning(
                            &mut context,
                            "No project source is declared in aidlc.knowledge.json".into(),
                        );
                    } else if let Some(path) = context["project"]["path"]
                        .as_str()
                        .filter(|path| !path.is_empty())
                        .map(str::to_owned)
                    {
                        let developing = input.stage["phase"] == "develop";
                        for warning in project_file_warnings(&root, &path, developing) {
                            push_warning(&mut context, warning);
                        }
                    }
                }
                if includes(&context["granted"], "sources") {
                    let sources: Vec<Value> = inspected["sources"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|row| row["kind"] != "project")
                        .map(|row| {
                            let mut row = row.clone();
                            row["provenance"] = json!("Observed");
                            row
                        })
                        .collect();
                    context["sources"] = json!(sources);
                    let registered = manifest_exists
                        .then(|| manifest["sources"].as_array().map(Vec::as_slice))
                        .flatten();
                    match source_status(&root, manifest_exists, registered.unwrap_or(&[])) {
                        Ok(status) => {
                            context["sourceChanges"] = json!({
                                "scanned": status.scanned,
                                "scannedAt": status.scanned_at,
                                "missing": status.missing,
                                "changed": list_json(&status.changed),
                                "added": list_json(&status.added),
                                "removed": list_json(&status.removed),
                                "more": {
                                    "changed": status.more[0],
                                    "added": status.more[1],
                                    "removed": status.more[2]
                                }
                            });
                            let unregistered = status
                                .warnings
                                .iter()
                                .any(|row| row.starts_with("No aidlc.knowledge.json was found"));
                            for warning in status.warnings {
                                push_warning(&mut context, warning);
                            }
                            if unregistered {
                                if let Some(rows) = context["warnings"].as_array_mut() {
                                    rows.retain(|row| {
                                        let text = row.as_str().unwrap_or("");
                                        !(text.starts_with("Project file ")
                                            && text.contains("knowledge init"))
                                    });
                                }
                            }
                        }
                        Err(error) => push_warning(
                            &mut context,
                            format!("source changes could not be listed: {error}"),
                        ),
                    }
                }
                for warning in inspected["warnings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    push_warning(&mut context, warning.to_owned());
                }
            }
            Err(error) => push_warning(
                &mut context,
                format!("sources could not be listed: {error}"),
            ),
        }
    }

    if includes(&context["granted"], "profile") {
        context["codebaseMap"] = codebase_map_status(&root, engine_root);
    }

    if input.stage["repeat"] == "work-unit" {
        if let Some(id) = intent_id {
            let relative = format!(
                ".aidlc/intents/{id}/{}/baseline.md",
                input.stage["phase"].as_str().unwrap_or("")
            );
            let exists = crate::onboarding::safe_join(&root, &relative)
                .ok()
                .is_some_and(|path| path.exists());
            context["baseline"] = json!({"path":relative,"exists":exists});
        }
    }

    if includes(&context["granted"], "knowledge") {
        let records = crate::knowledge::read_records(&root);
        let listed: Vec<Value> = records["records"].as_array().into_iter().flatten().map(|record| {
            json!({"id":record["id"],"title":record["title"],"path":record["file"],"origin":record["origin"]})
        }).collect();
        context["knowledge"] =
            json!({"dir":".aidlc/knowledge","records":listed,"invalid":records["invalid"]});
        if input.stage["id"] == "knowledge-writeback" {
            if let Some(id) = intent_id {
                match knowledge_record(&root, engine_root, id, input.graph, input.state) {
                    Ok(record) => context["knowledge"]["record"] = record,
                    Err(error) => {
                        push_warning(
                            &mut context,
                            format!("knowledge could not be listed: {error}"),
                        );
                        context["knowledge"] = Value::Null;
                    }
                }
            }
        }
    }

    Ok(context)
}

/// Node `projectFileWarnings`: advisory warnings for a PROJECT.md that is still
/// the shipped scaffold, or (during Develop) lacks the sections agents need to
/// run code. Unreadable or symlinked files yield no warnings.
pub fn project_file_warnings(root: &Path, project_path: &str, developing: bool) -> Vec<String> {
    let Some(text) = read_text(root, project_path) else {
        return vec![];
    };
    let Some(scaffold) = crate::scope::embedded_asset("data/scaffold/PROJECT.md") else {
        return vec![];
    };
    let scaffold = String::from_utf8_lossy(scaffold);
    let scaffold_lines: HashSet<&str> = split_lines(&scaffold)
        .map(js_trim)
        .filter(|line| !line.is_empty())
        .collect();
    if owned_lines(&text, &scaffold_lines) == 0 {
        return vec![format!("{project_path} is still the unedited scaffold. Fill in the architecture, conventions, commands (build, test, lint, format, baseline), environment prerequisites and known baseline failures before Develop; every agent reads it first.")];
    }
    if !developing {
        return vec![];
    }
    let empty: Vec<String> = PROJECT_SECTIONS
        .iter()
        .filter(|heading| {
            section_body(&text, heading).is_none_or(|body| owned_lines(&body, &scaffold_lines) == 0)
        })
        .map(|heading| format!("'## {heading}'"))
        .collect();
    if empty.is_empty() {
        vec![]
    } else {
        vec![format!("{project_path} has no content under {}. Develop runs code: record the commands, the environment it needs and the failures already present, or the agents will rediscover them on every work unit.", empty.join(", "))]
    }
}

/// Node `extraRequiredArtifacts` / `requiredProjectArtifacts`: the codebase map
/// is a required artifact of `context-and-risk` for scanned existing projects.
/// The template named in the reason is under the embedded engine; see
/// [`extra_required_artifacts_with_engine`].
pub fn extra_required_artifacts(root: &Path, stage: &Value) -> Vec<Value> {
    extra_required_artifacts_with_engine(root, stage, &embedded_engine_root(root))
}

/// [`extra_required_artifacts`] naming the template under `engine_root`.
pub fn extra_required_artifacts_with_engine(
    root: &Path,
    stage: &Value,
    engine_root: &Path,
) -> Vec<Value> {
    if stage["id"] != "context-and-risk" {
        return vec![];
    }
    let status = codebase_map_status(root, engine_root);
    if status["required"] != true || status["exists"] == true {
        return vec![];
    }
    let kind = read_project_profile(root).map(|profile| profile.kind);
    vec![json!({
        "path": status["path"],
        "reason": format!(
            "the codebase map is required for an existing project (kind {}); write it from {}",
            js_string(kind.as_ref()),
            status["template"].as_str().unwrap_or("")
        )
    })]
}

/// The embedded engine root, `<project>/.aidlc/engine`.
fn embedded_engine_root(root: &Path) -> PathBuf {
    root.join(".aidlc").join("engine")
}

/// Node `join(engineRoot(), 'methodology', 'templates', name)`.
fn methodology_template(engine_root: &Path, name: &str) -> PathBuf {
    engine_root.join("methodology").join("templates").join(name)
}

fn absolute_root(root: &Path) -> Result<PathBuf, String> {
    if root.is_absolute() {
        Ok(root.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| format!("Project root is unavailable: {e}"))?
            .join(root))
    }
}

/// Absolute intent directory helper for callers that need a consistent path.
pub fn intent_dir(root: &Path, intent_id: &str) -> PathBuf {
    root.join(".aidlc").join("intents").join(intent_id)
}

fn includes(grants: &Value, key: &str) -> bool {
    grants
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(key)))
}

fn push_warning(context: &mut Value, warning: String) {
    if let Some(warnings) = context["warnings"].as_array_mut() {
        warnings.push(json!(warning));
    }
}

fn knowledge_record(
    root: &Path,
    engine_root: &Path,
    id: &str,
    graph: &Value,
    state: &Value,
) -> Result<Value, String> {
    if !node_safe_id(id) {
        return Err("Invalid record id: use letters, digits, dot, underscore, or hyphen".into());
    }
    let relative = format!(".aidlc/knowledge/{id}.md");
    let path = crate::onboarding::safe_join(root, &relative).map_err(|error| error.to_string())?;
    Ok(json!({
        "path": relative,
        "exists": path.exists(),
        "template": methodology_template(engine_root, KNOWLEDGE_RECORD_TEMPLATE),
        "frontmatter": crate::knowledge::expected_record_frontmatter(graph, state)
    }))
}

/// Node `safeId`: `^[a-zA-Z0-9][a-zA-Z0-9._-]{0,79}$`.
fn node_safe_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 80
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b))
}

// ---------------------------------------------------------------------------
// PROJECT.md helpers (Node context.mjs sectionBody / ownedLines)
// ---------------------------------------------------------------------------

fn owned_lines(text: &str, scaffold: &HashSet<&str>) -> usize {
    split_lines(text)
        .map(js_trim)
        .filter(|line| !line.is_empty() && !scaffold.contains(line))
        .count()
}

fn section_body(text: &str, heading: &str) -> Option<String> {
    let lines: Vec<&str> = split_lines(text).collect();
    let wanted = format!("## {heading}").to_lowercase();
    let start = lines
        .iter()
        .position(|line| js_trim(line).to_lowercase() == wanted)?;
    let rest = &lines[start + 1..];
    let end = rest
        .iter()
        .position(|line| is_heading(line))
        .unwrap_or(rest.len());
    Some(rest[..end].join("\n"))
}

/// Node `/^##?\s/`.
fn is_heading(line: &str) -> bool {
    let mut chars = line.chars();
    if chars.next() != Some('#') {
        return false;
    }
    match chars.next() {
        Some(c) if is_js_space(c) => true,
        Some('#') => chars.next().is_some_and(is_js_space),
        _ => false,
    }
}

/// Node `text.split(/\r?\n/)`.
fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

/// ECMAScript WhiteSpace and LineTerminator (`String.prototype.trim`, `\s`).
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

// ---------------------------------------------------------------------------
// Symlink-safe project reads
// ---------------------------------------------------------------------------

/// Read a project file as text (lossy UTF-8, like Node `readFileSync(…, 'utf8')`),
/// refusing symlinked components.
fn read_text(root: &Path, relative: &str) -> Option<String> {
    let path = crate::onboarding::safe_join(root, relative).ok()?;
    let meta = fs::symlink_metadata(&path).ok()?;
    if meta.file_type().is_symlink() {
        return None;
    }
    fs::read(&path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn read_json(root: &Path, relative: &str) -> Option<Value> {
    serde_json::from_str(&read_text(root, relative)?).ok()
}

// ---------------------------------------------------------------------------
// Project profile and codebase map (Node onboard.mjs readProjectProfile /
// codebaseMapStatus)
// ---------------------------------------------------------------------------

struct Profile {
    outdated: bool,
    kind: Value,
}

fn read_project_profile(root: &Path) -> Option<Profile> {
    let data = read_json(root, PROFILE_JSON)?;
    if !(data.is_object() || data.is_array()) {
        return None;
    }
    let schema = match data.get("schemaVersion") {
        None | Some(Value::Null) => json!(1),
        Some(value) => value.clone(),
    };
    // `(data.languages ?? []).map((row) => row.name ?? row)` throws, and the
    // profile is then treated as absent, for non-arrays and null rows.
    match data.get("languages") {
        None | Some(Value::Null) => {}
        Some(Value::Array(rows)) if rows.iter().all(|row| !row.is_null()) => {}
        Some(_) => return None,
    }
    Some(Profile {
        outdated: js_number(&schema) < 2.0,
        kind: data.get("kind").cloned().unwrap_or(Value::Null),
    })
}

fn codebase_map_status(root: &Path, engine_root: &Path) -> Value {
    let template = methodology_template(engine_root, CODEBASE_MAP_TEMPLATE);
    let profile = read_project_profile(root);
    let stat = crate::onboarding::safe_join(root, CODEBASE_MAP)
        .ok()
        .and_then(|path| fs::symlink_metadata(path).ok());
    let exists = stat
        .as_ref()
        .is_some_and(|meta| meta.is_file() && !meta.file_type().is_symlink() && meta.len() > 0);
    let required = match &profile {
        Some(profile) if !profile.outdated => {
            Some(profile.kind == "brownfield" || profile.kind == "unknown")
        }
        _ => None,
    };
    let reason = match (required, &profile) {
        (None, _) => "project not scanned".to_owned(),
        (Some(true), Some(profile)) => {
            format!("project kind is {}", js_string(Some(&profile.kind)))
        }
        _ => "project kind is greenfield".to_owned(),
    };
    let (bytes, modified_at) = match (&stat, exists) {
        (Some(meta), true) => (
            meta.len(),
            meta.modified()
                .ok()
                .map(|time| iso_from_ms(mtime_ms(time).round())),
        ),
        _ => (0, None),
    };
    json!({
        "path": CODEBASE_MAP,
        "template": template,
        "exists": exists,
        "required": required,
        "reason": reason,
        "bytes": bytes,
        "modifiedAt": modified_at
    })
}

// ---------------------------------------------------------------------------
// Source change status (Node onboard.mjs sourceStatus)
// ---------------------------------------------------------------------------

/// A listed path. `None` stands for a JavaScript `undefined` key read from a
/// malformed snapshot row; it serializes as `null`, like `JSON.stringify`.
type Listed = Option<Value>;

struct SourceStatus {
    scanned: bool,
    scanned_at: Value,
    missing: Vec<Value>,
    changed: Vec<Listed>,
    added: Vec<Listed>,
    removed: Vec<Listed>,
    more: [usize; 3],
    warnings: Vec<String>,
}

struct SourceFile {
    path: String,
    bytes: u64,
    mtime_ms: f64,
}

fn list_json(rows: &[Listed]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| row.clone().unwrap_or(Value::Null))
            .collect(),
    )
}

fn onboard_commands(root: &Path) -> (String, String) {
    let config = read_json(root, ".aidlc/config.json").unwrap_or(Value::Null);
    let harness = config.get("harness").and_then(Value::as_str).filter(|h| {
        !h.is_empty()
            && h.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    let scope = config
        .get("scope")
        .and_then(Value::as_str)
        .filter(|s| ["solo", "team", "enterprise"].contains(s));
    let fallback = "dtech-aidlc onboard --harness <claude|deepseek>".to_owned();
    let create = match (harness, scope) {
        (Some(harness), Some(scope)) => {
            format!("dtech-aidlc onboard --harness {harness} --scope {scope}")
        }
        _ => fallback.clone(),
    };
    let scan = if root.join(".aidlc").join("config.json").exists() {
        "dtech-aidlc onboard --rescan".to_owned()
    } else {
        fallback
    };
    (create, scan)
}

fn read_snapshot(root: &Path) -> Option<Value> {
    let data = read_json(root, SNAPSHOT_JSON)?;
    let valid = data.get("schemaVersion").and_then(Value::as_f64) == Some(1.0)
        && data.get("files").is_some_and(Value::is_array);
    valid.then_some(data)
}

/// Node `sourceStatus(root, knowledge)` as JSON, for `onboard --rescan`:
/// `{ scannedAt, missing, changed, added, removed, more, warnings }` (`sources` is the
/// registered manifest's sources; ignored when `manifest_exists` is false).
pub(crate) fn source_status_json(
    root: &Path,
    manifest_exists: bool,
    sources: &[Value],
) -> Result<Value, String> {
    let status = source_status(root, manifest_exists, sources)?;
    Ok(json!({
        "scannedAt": status.scanned_at,
        "missing": status.missing,
        "changed": list_json(&status.changed),
        "added": list_json(&status.added),
        "removed": list_json(&status.removed),
        "more": {
            "changed": status.more[0],
            "added": status.more[1],
            "removed": status.more[2]
        },
        "warnings": status.warnings,
    }))
}

fn source_status(
    root: &Path,
    manifest_exists: bool,
    sources: &[Value],
) -> Result<SourceStatus, String> {
    let (create, scan) = onboard_commands(root);
    let profile = read_project_profile(root);
    let snapshot = read_snapshot(root);
    let scanned = profile.as_ref().is_some_and(|p| !p.outdated) && snapshot.is_some();
    let mut status = SourceStatus {
        scanned,
        scanned_at: snapshot
            .as_ref()
            .and_then(|s| s.get("scannedAt").cloned())
            .unwrap_or(Value::Null),
        missing: vec![],
        changed: vec![],
        added: vec![],
        removed: vec![],
        more: [0; 3],
        warnings: vec![],
    };
    if !manifest_exists {
        let detected = read_json(root, PROFILE_JSON)
            .and_then(|data| data.get("documentation")?.get("detected").cloned());
        let has_detected = match detected {
            Some(Value::Array(rows)) => !rows.is_empty(),
            Some(Value::String(text)) => !text.is_empty(),
            _ => false,
        };
        if !scanned || has_detected {
            status.warnings.push(format!("No aidlc.knowledge.json was found, so no project documentation is registered for agents. Run `{create}`."));
        }
        return Ok(status);
    }
    if !scanned {
        status.warnings.push(format!("This project has not been scanned on this machine by this version of dtech-aidlc, so the project profile and the document change check are unavailable. Run `{scan}`."));
    }
    for source in sources {
        let path = source["path"].as_str().unwrap_or("");
        let abs = path.split('/').fold(root.to_path_buf(), |mut abs, part| {
            abs.push(part);
            abs
        });
        if !abs.exists() {
            status
                .missing
                .push(json!({"id":source["id"],"path":source["path"]}));
            status.warnings.push(format!(
                "Registered knowledge source {} ({path}) does not exist. Fix the path in aidlc.knowledge.json.",
                js_string(source.get("id"))
            ));
        }
    }
    let Some(snapshot) = snapshot.filter(|_| scanned) else {
        return Ok(status);
    };
    let canonical = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let (current, _) = enumerate_source_files(&canonical, sources);
    let truncated = js_truthy(snapshot.get("truncated"));
    let mut prev: HashMap<String, &Value> = HashMap::new();
    let mut prev_order: Vec<Listed> = Vec::new();
    for file in snapshot["files"].as_array().into_iter().flatten() {
        if file.is_null() {
            return Err("Cannot read properties of null (reading 'path')".into());
        }
        match file.get("path") {
            Some(Value::String(path)) => {
                if prev.insert(path.clone(), file).is_none() {
                    prev_order.push(Some(json!(path)));
                }
            }
            None => {
                if !prev_order.iter().any(Option::is_none) {
                    prev_order.push(None);
                }
            }
            Some(other) => {
                let primitive = !(other.is_object() || other.is_array());
                if !primitive || !prev_order.iter().any(|row| row.as_ref() == Some(other)) {
                    prev_order.push(Some(other.clone()));
                }
            }
        }
    }
    let now: HashSet<&str> = current.iter().map(|file| file.path.as_str()).collect();
    let mut changed: Vec<Listed> = Vec::new();
    let mut added: Vec<Listed> = Vec::new();
    for file in &current {
        let Some(before) = prev.get(&file.path) else {
            if !truncated {
                added.push(Some(json!(file.path)));
            }
            continue;
        };
        let same_bytes = before.get("bytes").and_then(Value::as_f64) == Some(file.bytes as f64);
        if same_bytes && same_number(before.get("mtimeMs"), file.mtime_ms) {
            continue;
        }
        match before.get("sha256") {
            None | Some(Value::Null) => changed.push(Some(json!(file.path))),
            Some(previous) => {
                if json!(hash_file(&canonical, &file.path, file.bytes)?) != *previous {
                    changed.push(Some(json!(file.path)));
                }
            }
        }
    }
    let mut removed: Vec<Listed> = Vec::new();
    if !truncated {
        for key in &prev_order {
            let present = matches!(key, Some(Value::String(path)) if now.contains(path.as_str()));
            if !present {
                removed.push(key.clone());
            }
        }
    }
    let total = changed.len() + added.len() + removed.len();
    let mut sample: Vec<Listed> = changed
        .iter()
        .chain(&added)
        .chain(&removed)
        .cloned()
        .collect();
    js_sort(&mut sample);
    let (changed_list, changed_more) = cap_list(changed);
    let (added_list, added_more) = cap_list(added);
    let (removed_list, removed_more) = cap_list(removed);
    status.changed = changed_list;
    status.added = added_list;
    status.removed = removed_list;
    status.more = [changed_more, added_more, removed_more];
    if total > 0 {
        let sample = sample
            .iter()
            .take(5)
            .map(|row| match row {
                None | Some(Value::Null) => String::new(),
                Some(value) => js_string(Some(value)),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let day: String = js_string(snapshot.get("scannedAt"))
            .chars()
            .take(10)
            .collect();
        status.warnings.push(format!("{total} registered documents changed since the last scan on {day} ({sample}). Re-read them before relying on earlier conclusions. Run `dtech-aidlc onboard --rescan` to acknowledge."));
    }
    Ok(status)
}

fn cap_list(mut rows: Vec<Listed>) -> (Vec<Listed>, usize) {
    js_sort(&mut rows);
    let more = rows.len().saturating_sub(20);
    rows.truncate(20);
    (rows, more)
}

/// `Array.prototype.sort()` without a comparator: `undefined` last, everything
/// else by its string form in UTF-16 code unit order.
fn js_sort(rows: &mut [Listed]) {
    rows.sort_by(|a, b| match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Greater,
        (_, None) => Ordering::Less,
        (Some(a), Some(b)) => utf16_cmp(&js_string(Some(a)), &js_string(Some(b))),
    });
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Node `enumerateSourceFiles`: files claimed by the most specific (longest)
/// source path first, sorted, capped at 2000.
fn enumerate_source_files(root: &Path, sources: &[Value]) -> (Vec<SourceFile>, bool) {
    let mut ordered: Vec<&Value> = sources.iter().collect();
    ordered.sort_by(|a, b| {
        let length = |source: &Value| js_string(source.get("path")).encode_utf16().count();
        length(b).cmp(&length(a))
    });
    let mut by_path: HashMap<String, SourceFile> = HashMap::new();
    for source in ordered {
        let Some(path) = source["path"].as_str() else {
            continue;
        };
        for file in list_files(root, path) {
            by_path.entry(file.path.clone()).or_insert(file);
        }
    }
    let mut files: Vec<SourceFile> = by_path.into_values().collect();
    files.sort_by(|a, b| utf16_cmp(&a.path, &b.path));
    let truncated = files.len() > SNAPSHOT_CAP;
    files.truncate(SNAPSHOT_CAP);
    (files, truncated)
}

/// Node `listFiles`, refusing symlinked components of the source path.
fn list_files(root: &Path, relative: &str) -> Vec<SourceFile> {
    let Ok(abs) = crate::onboarding::safe_join(root, relative) else {
        return vec![];
    };
    let Ok(meta) = fs::symlink_metadata(&abs) else {
        return vec![];
    };
    let mut out = Vec::new();
    if meta.is_file() {
        visit_file(root, &abs, &mut out);
    } else if meta.is_dir() {
        walk(root, &abs, 0, &mut out);
    }
    out
}

/// Node `walk`: skips hidden entries, symlinks, vendored or generated
/// directories, and anything deeper than six levels.
fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<SourceFile>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if SKIP_DIRS.contains(&name.as_str()) || (kind.is_dir() && path.join("pyvenv.cfg").exists())
        {
            continue;
        }
        if kind.is_dir() {
            if depth + 1 > MAX_DEPTH {
                continue;
            }
            walk(root, &path, depth + 1, out);
        } else if kind.is_file() {
            visit_file(root, &path, out);
        }
    }
}

fn visit_file(root: &Path, path: &Path, out: &mut Vec<SourceFile>) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if !meta.is_file() {
        return;
    }
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    out.push(SourceFile {
        path: relative,
        bytes: meta.len(),
        mtime_ms: meta.modified().map(mtime_ms).unwrap_or(f64::NAN),
    });
}

fn hash_file(root: &Path, relative: &str, bytes: u64) -> Result<Option<String>, String> {
    use sha2::{Digest, Sha256};
    if bytes > HASH_LIMIT {
        return Ok(None);
    }
    let path = crate::onboarding::safe_join(root, relative).map_err(|e| e.to_string())?;
    let contents = fs::read(path).map_err(|e| e.to_string())?;
    Ok(Some(format!("{:x}", Sha256::digest(&contents))))
}

/// Node `Stats.mtimeMs`: `sec * 1e3 + nsec / 1e6`.
fn mtime_ms(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as f64 * 1e3 + f64::from(after.subsec_nanos()) / 1e6,
        Err(before) => {
            let before = before.duration();
            -(before.as_secs() as f64 * 1e3 + f64::from(before.subsec_nanos()) / 1e6)
        }
    }
}

/// Strict equality between a snapshot number and a freshly computed one.
/// serde_json's default float parser may differ from JavaScript's by one ULP,
/// so the computed value is round-tripped through the same shortest-form
/// serializer and parser the snapshot number went through.
fn same_number(snapshot: Option<&Value>, current: f64) -> bool {
    let Some(before) = snapshot.and_then(Value::as_f64) else {
        return false;
    };
    serde_json::to_string(&current)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_f64())
        == Some(before)
}

/// `Date.prototype.toISOString` for a millisecond time value.
fn iso_from_ms(ms: f64) -> String {
    let ms = ms as i64;
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

// ---------------------------------------------------------------------------
// JavaScript value coercions used by the Node reference
// ---------------------------------------------------------------------------

fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(rows)) => rows
            .iter()
            .map(|row| {
                if row.is_null() {
                    String::new()
                } else {
                    js_string(Some(row))
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// `Number(value)` for the relational comparison `schemaVersion < 2`.
fn js_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(flag) => f64::from(u8::from(*flag)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(text) => {
            let text = js_trim(text);
            if text.is_empty() {
                0.0
            } else {
                text.parse().unwrap_or(f64::NAN)
            }
        }
        Value::Array(_) => {
            let text = js_string(Some(value));
            js_number(&Value::String(text))
        }
        Value::Object(_) => f64::NAN,
    }
}
