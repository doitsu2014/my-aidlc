//! Native `doctor` command: a strictly read-only project health report, ported from
//! the `doctor` branch of Node `core/tools/dtech-aidlc.mjs`.
//!
//! [`run`] follows the shared handler contract: it returns the report
//! `{ ok, warnings, checks: [{ status, message }] }` (printed by the router as pretty
//! JSON) and exit code 1 when any check failed, 0 otherwise, exactly like Node
//! (`process.exitCode = 1` after printing). Errors (an unsafe `.aidlc` path, stray
//! arguments) are returned as `Err` and printed as `dtech-aidlc: <message>` with exit 1.
//!
//! Checks, in Node order: methodology graph; project configuration and agent models;
//! harness projection (+ `dtech-aidlc` on PATH); dependency repositories; knowledge
//! manifest; knowledge sources; workflow state; named prerequisites (their configured
//! check commands are run, as in Node); audit log; unused `.aidlc/methodology` copy;
//! default-scope approver roles.
//!
//! Nothing under the project is created, modified or removed. Public APIs of
//! `crate::scope`, `crate::knowledge`, `crate::orchestration_context`,
//! `crate::dependencies`, `crate::prerequisites`, `crate::workflow` and `crate::audit`
//! do the underlying work. Private read-only equivalents live here where no public
//! helper reproduces Node's behaviour:
//! - `validate_model_config`: Node `validateModelConfig` with its exact messages
//!   (`crate::scope` validates the same rules, but bundles graph and config loading and
//!   words several messages differently).
//! - `projection_status`: Node `projectionStatus` per file, including the projection
//!   history and legacy skill directories, against the bundled harness files
//!   (`crate::onboarding::projection::projection_status` has neither).
//! - `legacy_methodology_status`, `onboard_create_command`, `command_on_path`.

use crate::args::FlagValue;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROJECTION_HISTORY: &str = include_str!("../core/data/projection-history.json");
const SCOPES: &[&str] = &["solo", "team", "enterprise"];
/// Harness names Node `listHarnesses()` finds in the bundled `harness/` directory.
const HARNESS_CANDIDATES: &[&str] = &["claude", "deepseek"];

type Row = (&'static str, String);

/// Handler for `dtech-aidlc doctor` (see the module documentation).
pub fn run(
    key: &str,
    project: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    if key != "doctor" {
        let cmd = key.split(' ').next().unwrap_or(key);
        return Err(format!("Unknown command {cmd}; use dtech-aidlc help"));
    }
    if let Some(name) = flags
        .keys()
        .find(|name| !matches!(name.as_str(), "project" | "help" | "version"))
    {
        return Err(format!(
            "Unknown option --{name} for 'doctor'. Allowed options: --project."
        ));
    }
    if let Some(extra) = args.first() {
        return Err(format!("Unexpected argument '{extra}' for 'doctor'."));
    }
    let root = crate::orchestrate::resolve_like_node(project).map_err(|e| e.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let report = report(&root);
    let code = if report["ok"] == Value::Bool(true) {
        0
    } else {
        1
    };
    Ok((report, code))
}

/// Build the doctor report for an absolute project root without any safety pre-check.
/// Callers should use [`run`], which refuses an unsafe `.aidlc` first like Node.
pub fn report(root: &Path) -> Value {
    let graph = load_engine_graph();
    let graph_value = graph.as_ref().ok();
    let config = validate_model_config(root, graph_value)
        .unwrap_or_else(|_| Value::Object(Default::default()));

    let checks: Vec<Result<Vec<Row>, String>> = vec![
        Ok(check_graph(&graph)),
        Ok(check_config(root, &graph)),
        check_projection(root, &config),
        Ok(check_dependencies(root)),
        Ok(check_knowledge(root)),
        check_sources(root),
        check_workflow(root, graph_value),
        Ok(check_prerequisites(root, &config)),
        check_audit(root),
        Ok(check_legacy_methodology(root)),
        Ok(check_roles(&config, graph_value)),
    ];
    let mut rows: Vec<Row> = Vec::new();
    for check in checks {
        match check {
            Ok(found) => rows.extend(found),
            Err(message) => rows.push(("fail", message)),
        }
    }
    let ok = !rows.iter().any(|(status, _)| *status == "fail");
    let warnings = rows.iter().filter(|(status, _)| *status == "warn").count();
    let checks: Vec<Value> = rows
        .into_iter()
        .map(|(status, message)| json!({ "status": status, "message": message }))
        .collect();
    json!({ "ok": ok, "warnings": warnings, "checks": checks })
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The engine named by `DTECH_AIDLC_HOME` (Node `engineRoot()`), if any.
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

/// Node `readGraph()`. The scope loaders also read a project config, so they are given
/// a root that does not exist (read as `{}`); the project config is validated
/// separately by [`validate_model_config`].
fn load_engine_graph() -> Result<Value, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut probe = std::env::temp_dir().join(format!(
        "dtech-aidlc-doctor-graph-only-{}-{nanos}",
        std::process::id()
    ));
    let mut n = 0;
    while fs::symlink_metadata(&probe).is_ok() {
        n += 1;
        probe.set_file_name(format!(
            "dtech-aidlc-doctor-graph-only-{}-{nanos}-{n}",
            std::process::id()
        ));
    }
    let (graph, _) = match external_engine() {
        Some(engine) => crate::scope::load_graph(engine, &probe)?,
        None => crate::scope::load_embedded_graph(&probe)?,
    };
    Ok(graph.value().clone())
}

fn harness_names() -> Vec<&'static str> {
    HARNESS_CANDIDATES
        .iter()
        .copied()
        .filter(|name| {
            crate::scope::embedded_asset(&format!("harness/{name}/manifest.json")).is_some()
        })
        .collect()
}

/// Node `configuredHarnesses(config)`.
fn configured_harnesses(config: &Value) -> Vec<String> {
    let names = harness_names();
    if let Some(list) = config.get("harnesses").and_then(Value::as_array) {
        if !list.is_empty()
            && list
                .iter()
                .all(|name| name.as_str().is_some_and(|name| names.contains(&name)))
        {
            return list
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
        }
    }
    match config.get("harness").and_then(Value::as_str) {
        Some(name) if names.contains(&name) => vec![name.to_owned()],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Graph and configuration
// ---------------------------------------------------------------------------

fn check_graph(graph: &Result<Value, String>) -> Vec<Row> {
    let graph = match graph {
        Ok(graph) => graph,
        Err(error) => return vec![("fail", error.clone())],
    };
    let list = |key: &str| graph[key].as_array().cloned().unwrap_or_default();
    let (phases, stages, agents) = (list("phases"), list("stages"), list("agents"));
    let mut rows = vec![(
        "pass",
        format!(
            "Graph: {} phases, {} stages, {} agents",
            phases.len(),
            stages.len(),
            agents.len()
        ),
    )];
    for stage in &stages {
        let agent = &stage["agent"];
        if js_truthy(agent) && !agents.iter().any(|row| row["id"] == *agent) {
            rows.push((
                "fail",
                format!(
                    "Stage {} references unknown agent {}",
                    js_string(&stage["id"]),
                    js_string(agent)
                ),
            ));
        }
    }
    rows
}

fn check_config(root: &Path, graph: &Result<Value, String>) -> Vec<Row> {
    const LABEL: &str = "Project configuration and agent models";
    let graph = match graph {
        Ok(graph) => graph,
        Err(error) => return vec![("fail", format!("{LABEL}: {error}"))],
    };
    match validate_model_config(root, Some(graph)) {
        Ok(config) => {
            let names = harness_names();
            let ok = fs::metadata(config_path(root)).is_err()
                || (config["harness"]
                    .as_str()
                    .is_some_and(|h| names.contains(&h))
                    && config["scope"]
                        .as_str()
                        .is_some_and(|s| SCOPES.contains(&s)));
            vec![(if ok { "pass" } else { "fail" }, LABEL.to_owned())]
        }
        Err(error) => vec![("fail", format!("{LABEL}: {error}"))],
    }
}

fn config_path(root: &Path) -> PathBuf {
    root.join(".aidlc").join("config.json")
}

fn lstat_exists(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn plain_object(value: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    value.and_then(Value::as_object)
}

/// Node `validateModelConfig(root, graph)` (`graph` `None` stands for Node's fallback
/// `{ agents: [] }` when the methodology graph cannot be read). Read-only.
fn validate_model_config(root: &Path, graph: Option<&Value>) -> Result<Value, String> {
    let dir = root.join(".aidlc");
    let path = config_path(root);
    if lstat_exists(&dir)?.is_some_and(|meta| meta.file_type().is_symlink()) {
        return Err("Refusing to access symlink .aidlc directory".into());
    }
    let Some(meta) = lstat_exists(&path)? else {
        return Ok(Value::Object(Default::default()));
    };
    if meta.file_type().is_symlink() {
        return Err("Refusing to access symlink config.json".into());
    }
    let config: Value = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok())
        .ok_or("Invalid project configuration JSON")?;
    if !config.is_object() {
        return Err("Invalid project configuration: expected an object".into());
    }
    if let Some(mode) = config.get("mode") {
        if !matches!(mode.as_str(), Some("human" | "ai")) {
            return Err("mode must be human or ai".into());
        }
    }
    if let Some(models) = config.get("agentModels") {
        let models = plain_object(Some(models))
            .ok_or("Invalid agentModels: expected an object mapping agent IDs to model aliases")?;
        let agents = graph
            .and_then(|g| g["agents"].as_array())
            .cloned()
            .unwrap_or_default();
        for (id, model) in models {
            if !agents.iter().any(|agent| agent["id"].as_str() == Some(id)) {
                return Err(format!("Unknown agent in agentModels: {id}"));
            }
            if !model
                .as_str()
                .is_some_and(|m| crate::scope::MODEL_ALIASES.contains(&m))
            {
                let aliases = crate::scope::MODEL_ALIASES;
                let (last, rest) = aliases.split_last().expect("model aliases");
                return Err(format!(
                    "Invalid model alias for {id}: {}; choose {}, or {last}",
                    js_string(model),
                    rest.join(", ")
                ));
            }
        }
    }
    if let Some(modes) = config.get("gateModes") {
        let modes = plain_object(Some(modes))
            .ok_or("Invalid gateModes: expected an object mapping gate ids to \"human\"")?;
        let gates: Vec<&Value> = graph
            .and_then(|g| g["phases"].as_array())
            .into_iter()
            .flatten()
            .map(|phase| phase.get("gate").unwrap_or(&Value::Null))
            .collect();
        for (gate, value) in modes {
            if !gates.is_empty() && !gates.iter().any(|known| known.as_str() == Some(gate)) {
                return Err(format!("Unknown gate in gateModes: {gate}"));
            }
            if value.as_str() != Some("human") {
                return Err(format!(
                    "Invalid gateModes.{gate}: a gate can only be pinned to human"
                ));
            }
        }
    }
    if let Some(prerequisites) = config.get("prerequisites") {
        let prerequisites = plain_object(Some(prerequisites))
            .ok_or("Invalid prerequisites: expected an object mapping names to { check }")?;
        let graph = graph.filter(|g| js_truthy(&g["phases"]));
        for (name, definition) in prerequisites {
            crate::prerequisites::validate_prerequisite(name, definition, graph)?;
        }
    }
    if let Some(threshold) = config.get("unitSplitThreshold") {
        let integer = threshold
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= 1.0);
        if !integer {
            return Err("Invalid unitSplitThreshold: expected a positive integer".into());
        }
    }
    if let Some(threshold) = config.get("autoParkThreshold") {
        let integer = threshold
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= 1.0);
        if !integer {
            return Err("Invalid autoParkThreshold: expected a positive integer".into());
        }
    }
    if let Some(workers) = config.get("unitWorkers") {
        let integer = workers
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= 1.0);
        if !integer {
            return Err("Invalid unitWorkers: expected a positive integer".into());
        }
    }
    if let Some(roles) = config.get("roles") {
        let roles = plain_object(Some(roles)).ok_or("Invalid roles: expected an object")?;
        let names = |value: &Value| {
            value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string))
        };
        if let Some(owner) = roles.get("owner") {
            if !names(owner) {
                return Err("Invalid roles.owner: expected an array of names".into());
            }
        }
        if let Some(approvers) = roles.get("approvers") {
            let approvers = plain_object(Some(approvers))
                .ok_or("Invalid roles.approvers: expected an object mapping gate ids to names")?;
            for (gate, list) in approvers {
                if !names(list) {
                    return Err(format!("Invalid roles.approvers.{gate}"));
                }
            }
        }
    }
    Ok(config)
}

// ---------------------------------------------------------------------------
// Harness projection
// ---------------------------------------------------------------------------

struct HarnessManifest {
    name: String,
    skill_dir: String,
    /// `(source, target)` pairs.
    files: Vec<(String, String)>,
    /// `(dir, file)` obsolete candidates: `obsoleteFiles` then every `legacy` entry.
    obsolete: Vec<(String, String)>,
}

/// Node `loadHarnessManifest(name)` over the bundled harness files.
fn load_manifest(name: &str) -> Result<HarnessManifest, String> {
    let invalid = |reason: &str| format!("Invalid harness manifest for {name}: {reason}");
    let manifest: Value = crate::scope::embedded_asset(&format!("harness/{name}/manifest.json"))
        .and_then(|bytes| serde_json::from_slice(bytes).ok())
        .ok_or_else(|| invalid("cannot read JSON"))?;
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let skill_dir = text(&manifest["skillDir"]).ok_or_else(|| invalid("skillDir is invalid"))?;
    let mut files = Vec::new();
    for file in manifest["files"]
        .as_array()
        .ok_or_else(|| invalid("files must be a non-empty array"))?
    {
        let source = text(&file["source"]).ok_or_else(|| invalid("file source is invalid"))?;
        let target = text(&file["target"]).ok_or_else(|| invalid("file target is invalid"))?;
        files.push((source, target));
    }
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(text)
            .collect()
    };
    let mut obsolete: Vec<(String, String)> = strings(&manifest["obsoleteFiles"])
        .into_iter()
        .map(|file| (skill_dir.clone(), file))
        .collect();
    for entry in manifest["legacy"].as_array().into_iter().flatten() {
        let dir = text(&entry["dir"]).unwrap_or_default();
        for file in strings(&entry["files"]) {
            obsolete.push((dir.clone(), file));
        }
    }
    Ok(HarnessManifest {
        name: name.to_owned(),
        skill_dir,
        files,
        obsolete,
    })
}

/// Node `projectionHistory()`.
fn projection_history() -> Value {
    let text = match external_engine() {
        Some(engine) => fs::read_to_string(engine.join("data/projection-history.json")).ok(),
        None => Some(PROJECTION_HISTORY.to_owned()),
    };
    text.and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "harness": {}, "methodology": {} }))
}

/// Node `hashText(readFileSync(path))`: UTF-8 decoded, CRLF folded, SHA-256 hex.
fn hash_text(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn join_rel(root: &Path, rel: &str) -> PathBuf {
    rel.split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(root.to_path_buf(), |mut path, part| {
            path.push(part);
            path
        })
}

/// Node `path.join(a, b)` for clean relative paths, with the platform separator.
fn node_join(a: &str, b: &str) -> String {
    let joined = format!("{a}/{b}");
    if cfg!(windows) {
        joined.replace('/', "\\")
    } else {
        joined
    }
}

/// Node `compareVersions(a, b)` sign: `major.minor.patch` prefixes, else `0.0.0`.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn parse(value: &str) -> [f64; 3] {
        let mut out = [0.0; 3];
        let mut rest = value;
        for (index, slot) in out.iter_mut().enumerate() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return [0.0; 3];
            }
            *slot = rest[..digits].parse().unwrap_or(f64::INFINITY);
            rest = &rest[digits..];
            if index < 2 {
                match rest.strip_prefix('.') {
                    Some(next) => rest = next,
                    None => return [0.0; 3],
                }
            }
        }
        out
    }
    let (left, right) = (parse(a), parse(b));
    for i in 0..3 {
        if left[i] != right[i] {
            return left[i].total_cmp(&right[i]);
        }
    }
    std::cmp::Ordering::Equal
}

struct FileState {
    path: String,
    state: &'static str,
}

struct ProjectionStatus {
    state: &'static str,
    /// `stamp?.engineVersion ?? null`.
    stamped: Option<Value>,
    files: Vec<FileState>,
    obsolete: Vec<FileState>,
}

fn read_stamp(root: &Path, manifest: &HarnessManifest) -> Option<Value> {
    let path = join_rel(root, &manifest.skill_dir).join(".projection.json");
    let meta = fs::symlink_metadata(&path).ok()?;
    if meta.file_type().is_symlink() {
        return None;
    }
    let stamp: Value =
        serde_json::from_str(&String::from_utf8_lossy(&fs::read(&path).ok()?)).ok()?;
    (stamp.get("schemaVersion").and_then(Value::as_f64) == Some(1.0)).then_some(stamp)
}

/// Node `projectionStatus(root, manifest, engineVersion)`. Read-only.
fn projection_status(
    root: &Path,
    manifest: &HarnessManifest,
    history: &Value,
) -> Result<ProjectionStatus, String> {
    let stamp = read_stamp(root, manifest);
    let history = &history["harness"][&manifest.name];
    let stamp_hash = |file: &str| -> Option<&Value> {
        stamp
            .as_ref()?
            .get("files")?
            .get(file)?
            .get("sha256")
            .filter(|value| !value.is_null())
    };
    let in_history = |file: &str, hash: &str| {
        history[basename(file)]
            .as_array()
            .is_some_and(|known| known.iter().any(|row| row.as_str() == Some(hash)))
    };
    let mut files = Vec::new();
    for (source, target) in &manifest.files {
        let parent = target.rsplit_once(['/', '\\']).map_or("", |(dir, _)| dir);
        let dir = if parent.is_empty() {
            manifest.skill_dir.clone()
        } else {
            format!("{}/{parent}", manifest.skill_dir)
        };
        crate::workflow::assert_no_symlink_components(root, &dir)?;
        let path = join_rel(&join_rel(root, &manifest.skill_dir), target);
        let state = if fs::metadata(&path).is_err() {
            "missing"
        } else {
            match fs::symlink_metadata(&path) {
                Err(_) => "missing",
                Ok(meta) if !meta.is_file() => "modified",
                Ok(_) => match fs::read(&path) {
                    Err(_) => "missing",
                    Ok(bytes) => {
                        let hash = hash_text(&bytes);
                        let source_bytes = crate::scope::embedded_asset(&format!(
                            "harness/{}/{source}",
                            manifest.name
                        ))
                        .ok_or_else(|| format!("missing harness source file {source}"))?;
                        if hash == hash_text(source_bytes) {
                            "current"
                        } else if stamp_hash(target).and_then(Value::as_str) == Some(&hash)
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
        files.push(FileState {
            path: target.clone(),
            state,
        });
    }
    let mut obsolete = Vec::new();
    for (dir, file) in &manifest.obsolete {
        let path = join_rel(&join_rel(root, dir), file);
        crate::workflow::assert_no_symlink_components(root, dir)?;
        if fs::metadata(&path).is_err() {
            continue;
        }
        let mut state = "modified";
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()) {
            if let Ok(bytes) = fs::read(&path) {
                let hash = hash_text(&bytes);
                // `stamp?.files?.[file]?.sha256 ?? history[basename(file)]?.find(...)`
                let pristine = match stamp_hash(file) {
                    Some(prior) => prior.as_str() == Some(hash.as_str()),
                    None => in_history(file, &hash),
                };
                if pristine {
                    state = "pristine";
                }
            }
        }
        let relative = path
            .strip_prefix(root)
            .map(|rel| {
                rel.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_else(|_| format!("{dir}/{file}"));
        obsolete.push(FileState {
            path: relative,
            state,
        });
    }
    let stamped = stamp
        .as_ref()
        .and_then(|stamp| stamp.get("engineVersion"))
        .filter(|value| !value.is_null())
        .cloned();
    let stamped_text = stamped.as_ref().map(js_string).unwrap_or_default();
    let ahead = compare_versions(&stamped_text, ENGINE_VERSION).is_gt()
        && files.iter().any(|file| file.state != "current");
    let any = |rows: &[FileState], state: &str| rows.iter().any(|row| row.state == state);
    let state = if ahead {
        "ahead"
    } else if any(&files, "missing") {
        "missing"
    } else if any(&files, "modified") || any(&obsolete, "modified") {
        "modified"
    } else if any(&files, "behind") || any(&obsolete, "pristine") {
        "behind"
    } else {
        "current"
    };
    Ok(ProjectionStatus {
        state,
        stamped,
        files,
        obsolete,
    })
}

/// Node `projectionDoctorRows(root, config, VERSION)`.
fn check_projection(root: &Path, config: &Value) -> Result<Vec<Row>, String> {
    let harnesses = configured_harnesses(config);
    if harnesses.is_empty() {
        return Ok(Vec::new());
    }
    let history = projection_history();
    let mut rows = Vec::new();
    for name in &harnesses {
        let manifest = load_manifest(name)?;
        let status = match projection_status(root, &manifest, &history) {
            Ok(status) => status,
            Err(error) => {
                rows.push(("fail", format!("Projection {name}: {error}")));
                continue;
            }
        };
        for file in status.files.iter().filter(|file| file.state == "missing") {
            // Node splits on a doubled backslash here, so Windows keeps `\` separators.
            rows.push((
                "fail",
                format!(
                    "Projection {name}: {} is missing; run: dtech-aidlc harness refresh",
                    node_join(&manifest.skill_dir, &file.path)
                ),
            ));
        }
        let paths: Vec<String> = status
            .files
            .iter()
            .filter(|file| file.state == "modified")
            .map(|file| node_join(&manifest.skill_dir, &file.path))
            .chain(
                status
                    .obsolete
                    .iter()
                    .filter(|file| file.state == "modified")
                    .map(|file| file.path.clone()),
            )
            .collect();
        match status.state {
            "behind" => rows.push((
                "warn",
                format!(
                    "Projection {name}: behind engine {ENGINE_VERSION} (projected by {}); run: dtech-aidlc harness refresh",
                    status
                        .stamped
                        .as_ref()
                        .map(js_string)
                        .unwrap_or_else(|| "an earlier version without a stamp".into())
                ),
            )),
            "modified" => rows.push((
                "warn",
                format!(
                    "Projection {name}: {} edited locally or of unknown origin; keep them, or for skill files run: dtech-aidlc harness refresh --force; obsolete files must be deleted by hand",
                    paths.join(", ")
                ),
            )),
            "ahead" => rows.push((
                "warn",
                format!(
                    "Projection {name}: projected by engine {}, newer than the installed {ENGINE_VERSION}; upgrade dtech-aidlc",
                    status.stamped.as_ref().map(js_string).unwrap_or_else(|| "null".into())
                ),
            )),
            "current" => rows.push((
                "pass",
                format!(
                    "Projection {name}: current ({}, engine {ENGINE_VERSION})",
                    manifest.skill_dir
                ),
            )),
            _ => {}
        }
    }
    if !command_on_path("dtech-aidlc") {
        rows.push((
            "warn",
            "dtech-aidlc is not on PATH; the projected skill runs the bare command dtech-aidlc"
                .into(),
        ));
    }
    Ok(rows)
}

/// Node `commandOnPath(command)`.
fn command_on_path(command: &str) -> bool {
    let extensions: Vec<String> = if cfg!(windows) {
        let pathext = std::env::var_os("PATHEXT")
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        pathext.split(';').map(str::to_owned).collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let delimiter = if cfg!(windows) { ';' } else { ':' };
    path.split(delimiter).any(|directory| {
        extensions.iter().any(|extension| {
            fs::metadata(Path::new(directory).join(format!("{command}{extension}")))
                .is_ok_and(|meta| meta.is_file())
        })
    })
}

// ---------------------------------------------------------------------------
// Dependencies and knowledge
// ---------------------------------------------------------------------------

fn check_dependencies(root: &Path) -> Vec<Row> {
    match crate::dependencies::validate_dependencies(root) {
        Ok(rows) => vec![(
            "pass",
            format!("Dependency repositories: {} registered", rows.len()),
        )],
        Err(error) => vec![("fail", format!("Dependency repositories: {error}"))],
    }
}

enum Knowledge {
    Absent,
    Present(usize),
    Invalid(String),
}

/// Node `knowledgeState(root)`. A project root that is not a directory has no
/// manifest, as in Node (`existsSync` of the manifest is false).
fn knowledge_state(root: &Path) -> Knowledge {
    if !root.is_dir() {
        return Knowledge::Absent;
    }
    match crate::knowledge::load_knowledge_manifest(root) {
        Ok((false, _)) => Knowledge::Absent,
        Ok((true, manifest)) => {
            Knowledge::Present(manifest["sources"].as_array().map_or(0, Vec::len))
        }
        Err(error) => Knowledge::Invalid(error),
    }
}

fn check_knowledge(root: &Path) -> Vec<Row> {
    match knowledge_state(root) {
        Knowledge::Invalid(error) => vec![("fail", format!("Knowledge: {error}"))],
        _ => Vec::new(),
    }
}

/// Node `onboardCommands(root).create`.
fn onboard_create_command(root: &Path) -> String {
    let path = config_path(root);
    let config = fs::symlink_metadata(&path)
        .ok()
        .filter(|meta| !meta.file_type().is_symlink())
        .and_then(|_| fs::read(&path).ok())
        .and_then(|bytes| serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes)).ok())
        .unwrap_or(Value::Null);
    let harness = config.get("harness").and_then(Value::as_str).filter(|h| {
        !h.is_empty()
            && h.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    let scope = config
        .get("scope")
        .and_then(Value::as_str)
        .filter(|s| SCOPES.contains(s));
    match (harness, scope) {
        (Some(harness), Some(scope)) => {
            format!("dtech-aidlc onboard --harness {harness} --scope {scope}")
        }
        _ => "dtech-aidlc onboard --harness <claude|deepseek>".into(),
    }
}

const UNREGISTERED: &str = "No aidlc.knowledge.json was found";
const SOURCE_STATUS_ERROR: &str = "source changes could not be listed: ";

/// The knowledge sources row, from Node `sourceStatus` as exposed by
/// [`crate::orchestration_context::build`] with only the `sources` grant.
fn check_sources(root: &Path) -> Result<Vec<Row>, String> {
    let sources = match knowledge_state(root) {
        Knowledge::Invalid(_) => return Ok(Vec::new()),
        Knowledge::Absent => None,
        Knowledge::Present(count) => Some(count),
    };
    let context = if root.is_dir() {
        let stage = json!({ "inputs": { "context": ["sources"] } });
        let state = json!({});
        let graph = json!({});
        Some(crate::orchestration_context::build(
            crate::orchestration_context::ContextInput {
                root,
                state: &state,
                stage: &stage,
                graph: &graph,
            },
        )?)
    } else {
        None
    };
    let warnings: Vec<&str> = context
        .as_ref()
        .and_then(|context| context["warnings"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let changes = context.as_ref().map(|context| &context["sourceChanges"]);
    if changes.is_none_or(Value::is_null) {
        if let Some(error) = warnings
            .iter()
            .find_map(|row| row.strip_prefix(SOURCE_STATUS_ERROR))
        {
            return Err(error.to_owned());
        }
    }
    let Some(count) = sources else {
        // A missing project directory has never been scanned.
        let missing = context.is_none() || warnings.iter().any(|row| row.starts_with(UNREGISTERED));
        let message = if missing {
            format!(
                "Knowledge sources: no aidlc.knowledge.json; run {}",
                onboard_create_command(root)
            )
        } else {
            "Knowledge sources: no aidlc.knowledge.json; no documentation detected at the last scan"
                .into()
        };
        return Ok(vec![("pass", message)]);
    };
    let changes = changes.cloned().unwrap_or(Value::Null);
    let len = |key: &str| changes[key].as_array().map_or(0, Vec::len);
    let more = |key: &str| changes["more"][key].as_u64().unwrap_or(0) as usize;
    let missing = len("missing");
    let changed = len("changed")
        + len("added")
        + len("removed")
        + more("changed")
        + more("added")
        + more("removed");
    let status = if missing > 0 { "warn" } else { "pass" };
    if changes["scanned"] != Value::Bool(true) {
        return Ok(vec![(
            status,
            format!(
                "Knowledge sources: {count} registered, {missing} missing; not scanned on this machine"
            ),
        )]);
    }
    Ok(vec![(
        status,
        format!(
            "Knowledge sources: {count} registered, {missing} missing, {changed} changed since last scan"
        ),
    )])
}

// ---------------------------------------------------------------------------
// Workflow state
// ---------------------------------------------------------------------------

/// Node `checkWorkflowState(root, graph)`. Read-only.
fn check_workflow(root: &Path, graph: Option<&Value>) -> Result<Vec<Row>, String> {
    use crate::workflow::{
        depth_of, intent_ids_on_disk, intent_state_path, load_intent, normalise_state,
        ordered_stages, read_pointer, valid_intent_id, validate_intent_state, Pointer,
    };
    let Some(graph) = graph else {
        return Ok(Vec::new());
    };
    let mut rows: Vec<Row> = Vec::new();
    let (pointer, _) = read_pointer(root);
    match &pointer {
        Pointer::Corrupt(reason) => rows.push((
            "fail",
            if *reason == "cannot be parsed" {
                "State pointer: .aidlc/state.json cannot be parsed".into()
            } else {
                "State pointer: .aidlc/state.json is not a recognised state file".into()
            },
        )),
        Pointer::Active(Some(id)) if fs::metadata(intent_state_path(root, id)).is_err() => {
            rows.push((
                "fail",
                format!("State pointer: active intent '{id}' has no state file"),
            ))
        }
        Pointer::Legacy(_) => rows.push((
            "warn",
            "State: version 1 format; it is migrated automatically by the next orchestrate or intent command".into(),
        )),
        _ => {}
    }
    let ids = intent_ids_on_disk(root);
    let has_state = !matches!(pointer, Pointer::Absent) || !ids.is_empty();
    if has_state && fs::metadata(config_path(root)).is_err() {
        rows.push((
            "warn",
            "Project has workflow state but no .aidlc/config.json; run dtech-aidlc onboard --harness <claude|deepseek> --scope <scope>".into(),
        ));
    }
    // The split threshold for the wave suggestion (an invalid value is reported by the
    // configuration check and falls back to the default here, as in orchestrate next).
    let split_threshold = fs::read_to_string(config_path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|config| crate::orchestrate::unit_split_threshold(&config))
        .unwrap_or(crate::orchestrate::DEFAULT_UNIT_SPLIT_THRESHOLD as f64)
        as usize;
    let graph_ids: Vec<&Value> = graph["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|stage| &stage["id"])
        .collect();
    for id in ids.iter().filter(|id| valid_intent_id(id)) {
        let path = intent_state_path(root, id);
        if fs::metadata(&path).is_err() {
            continue;
        }
        let raw: Value = match fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok())
        {
            Some(raw) => raw,
            None => {
                rows.push(("fail", format!("Intent {id}: cannot be parsed")));
                continue;
            }
        };
        if raw.is_null() {
            // Node `normalise(null)` throws, ending the whole workflow check.
            return Err("Cannot read properties of null (reading 'dependencies')".into());
        }
        let problems = validate_intent_state(&normalise_state(raw), graph, Some(id), true);
        for problem in &problems {
            rows.push(("fail", format!("Intent {id}: {problem}")));
        }
        if !problems.is_empty() {
            continue;
        }
        let (state, _) = load_intent(root, graph, id)?;
        let scope = state["scope"].as_str().unwrap_or("");
        let depth = depth_of(&state);
        let stages = ordered_stages(graph, scope, depth);
        let recorded = state["stages"].as_object().cloned().unwrap_or_default();
        for stage_id in recorded.keys() {
            let key = Value::String(stage_id.clone());
            if !graph_ids.contains(&&key) {
                rows.push((
                    "warn",
                    format!("Intent {id}: stage {stage_id} is not in the current graph"),
                ));
            }
        }
        let status_of = |stage: &Value| -> Option<&str> {
            stage["id"]
                .as_str()
                .and_then(|sid| recorded.get(sid))
                .and_then(|row| row["status"].as_str())
        };
        let finished = |stage: &Value| matches!(status_of(stage), Some("complete" | "skipped"));
        let mut unfinished_seen = false;
        for stage in &stages {
            if finished(stage) && unfinished_seen {
                rows.push((
                    "fail",
                    format!(
                        "Intent {id}: stage {} is recorded {} although the earlier stage is not finished",
                        js_string(&stage["id"]),
                        status_of(stage).unwrap_or("")
                    ),
                ));
            }
            if !finished(stage) {
                unfinished_seen = true;
            }
        }
        // Work units: the stored ledger, or the one the next orchestrate command would
        // build (computed on a copy; doctor never writes).
        let owned: Vec<Value> = stages.iter().map(|stage| (*stage).clone()).collect();
        let mut unit_scheduled = false;
        match crate::work_units::preview_with_change(root, graph, &state, &owned) {
            Err(error) => rows.push(("fail", format!("Intent {id}: {error}"))),
            Ok(None) => {}
            Ok(Some((preview, pending, change))) => {
                unit_scheduled = true;
                if !pending {
                    if let Some(warning) = crate::work_units::stale_warning(&change) {
                        rows.push(("warn", format!("Intent {id}: {warning}")));
                    }
                    for warning in crate::work_units::drift_warnings(root, graph, &preview) {
                        rows.push(("warn", format!("Intent {id}: {warning}")));
                    }
                }
                let ignored: Vec<String> = preview
                    .pointer("/workUnits/migration/ignoredKeys")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                for stage in crate::work_units::span(&owned).repeating {
                    let sid = stage["id"].as_str().unwrap_or("");
                    for key in &ignored {
                        if recorded
                            .get(sid)
                            .and_then(|row| row.get("units"))
                            .and_then(|units| units.get(key))
                            .is_some()
                        {
                            rows.push((
                                "warn",
                                format!("Intent {id}: stage {sid} holds a placeholder entry \"{key}\" from an earlier one-unit fallback; it is ignored"),
                            ));
                        }
                    }
                }
                for warning in crate::regression::doctor_warnings(&preview) {
                    rows.push(("warn", format!("Intent {id}: {warning}")));
                }
                for warning in
                    crate::work_units::doctor_warnings(root, graph, &preview, split_threshold)
                {
                    rows.push(("warn", format!("Intent {id}: {warning}")));
                }
            }
        }
        // With unit-level scheduling, red-tests of one unit and build-and-verify of
        // another interleave: any repeating stage may be current once the stages before
        // the span are finished.
        let span = crate::work_units::span(&owned);
        let unit_step_current = unit_scheduled
            && span.before.iter().all(|stage| finished(stage))
            && span
                .repeating
                .iter()
                .any(|stage| state["currentStage"] == stage["id"]);
        let first_unfinished = stages.iter().find(|stage| !finished(stage));
        if let (true, Some(first)) = (js_truthy(&state["currentStage"]), first_unfinished) {
            if state["currentStage"] != first["id"] && !unit_step_current {
                rows.push((
                    "fail",
                    format!(
                        "Intent {id}: currentStage {} is not the first unfinished stage",
                        js_string(&state["currentStage"])
                    ),
                ));
            }
        }
        for stage in &stages {
            let phase_id = &stage["phase"];
            let final_stage = stages
                .iter()
                .rfind(|item| item["phase"] == *phase_id)
                .map(|item| &item["id"]);
            if final_stage != Some(&stage["id"]) {
                continue;
            }
            if !finished(stage) {
                continue;
            }
            let gate = graph["phases"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|phase| phase["id"] == *phase_id)
                .map(|phase| phase.get("gate").cloned().unwrap_or(Value::Null))
                .unwrap_or(Value::Null);
            let gate_key = js_string(&gate);
            let decision = &state["gates"][&gate_key]["decision"];
            let reports = stage["id"]
                .as_str()
                .and_then(|sid| recorded.get(sid))
                .is_some_and(|row| row["reports"].is_array());
            if reports && decision.as_str() != Some("approved") {
                rows.push((
                    "fail",
                    format!("Intent {id}: gate {gate_key} was passed without a recorded approval"),
                ));
            }
        }
    }
    if rows.is_empty()
        && (matches!(pointer, Pointer::Active(_) | Pointer::Legacy(_)) || !ids.is_empty())
    {
        rows.push(("pass", "Workflow state".into()));
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Prerequisites, audit log, legacy methodology, roles
// ---------------------------------------------------------------------------

fn check_prerequisites(root: &Path, config: &Value) -> Vec<Row> {
    let names: Vec<String> = crate::prerequisites::prerequisites_of(config)
        .keys()
        .cloned()
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    // Node's check throws on the first declaration it cannot run (no command string)
    // and the doctor records that as one failing row.
    let declared = crate::prerequisites::prerequisites_of(config);
    for name in &names {
        if let Err(problem) = crate::prerequisites::require_runnable(name, &declared[name]) {
            return vec![("fail", problem)];
        }
    }
    crate::prerequisites::run_prerequisites(&root.to_string_lossy(), config, &names)
        .into_iter()
        .map(|item| {
            let name = js_string(&item["name"]);
            if item["ok"] == Value::Bool(true) {
                ("pass", format!("Prerequisite {name}: check passed"))
            } else {
                let hint = if js_truthy(&item["hint"]) {
                    format!("; {}", js_string(&item["hint"]))
                } else {
                    String::new()
                };
                (
                    "warn",
                    format!(
                        "Prerequisite {name}: check failed ({}){hint}",
                        js_string(&item["check"])
                    ),
                )
            }
        })
        .collect()
}

fn check_audit(root: &Path) -> Result<Vec<Row>, String> {
    let path = root.join(".aidlc").join("audit.jsonl");
    let valid = if fs::metadata(&path).is_err() {
        true
    } else {
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        text.split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter(|line| !line.is_empty())
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    };
    Ok(vec![(
        if valid { "pass" } else { "fail" },
        "Audit log contains valid JSON lines".into(),
    )])
}

/// Node `legacyMethodologyStatus(root)`: `(pristine, modified)` counts, `None` when
/// there is no `.aidlc/methodology`.
fn legacy_methodology_status(root: &Path) -> Option<(usize, usize)> {
    let base = root.join(".aidlc").join("methodology");
    fs::metadata(&base).ok()?;
    let history = projection_history();
    let history = &history["methodology"];
    let (mut pristine, mut modified) = (0, 0);
    fn walk(
        dir: &Path,
        prefix: &str,
        history: &Value,
        pristine: &mut usize,
        modified: &mut usize,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let Ok(meta) = fs::symlink_metadata(&path) else {
                *modified += 1;
                continue;
            };
            if meta.file_type().is_symlink() {
                *modified += 1;
            } else if meta.is_dir() {
                walk(&path, &rel, history, pristine, modified)?;
            } else if meta.is_file() {
                let hash = hash_text(&fs::read(&path)?);
                if history[rel.as_str()]
                    .as_array()
                    .is_some_and(|known| known.iter().any(|row| row.as_str() == Some(&hash)))
                {
                    *pristine += 1;
                } else {
                    *modified += 1;
                }
            } else {
                *modified += 1;
            }
        }
        Ok(())
    }
    if walk(&base, "", history, &mut pristine, &mut modified).is_err() {
        modified += 1;
    }
    Some((pristine, modified))
}

fn check_legacy_methodology(root: &Path) -> Vec<Row> {
    match legacy_methodology_status(root) {
        Some((pristine, modified)) => vec![(
            "warn",
            format!(
                "Unused methodology copy at .aidlc/methodology from an earlier version; {pristine} pristine file(s) can be removed by harness refresh, {modified} modified or unknown file(s) will be kept"
            ),
        )],
        None => Vec::new(),
    }
}

fn check_roles(config: &Value, graph: Option<&Value>) -> Vec<Row> {
    let Some(graph) = graph else {
        return Vec::new();
    };
    let Some(scope) = config["scope"].as_str().filter(|s| SCOPES.contains(s)) else {
        return Vec::new();
    };
    if js_truthy(&config["roles"]) {
        return Vec::new();
    }
    let phases = graph["phases"].as_array().cloned().unwrap_or_default();
    let gate = graph["scopes"][scope]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| {
            phases
                .iter()
                .find(|phase| phase["id"] == *id)
                .map(|phase| &phase["gate"])
        })
        .find(|gate| js_truthy(gate))
        .map(js_string)
        .unwrap_or_else(|| "analysis-baseline".into());
    vec![(
        "warn",
        format!("No allowed approver is configured for gate {gate} in the default scope"),
    )]
}

// ---------------------------------------------------------------------------
// JavaScript coercions used by the Node reference
// ---------------------------------------------------------------------------

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `String(value)` for JSON values.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) => f.to_string(),
            _ => n.to_string(),
        },
        Value::String(text) => text.clone(),
        Value::Array(rows) => rows
            .iter()
            .map(|row| {
                if row.is_null() {
                    String::new()
                } else {
                    js_string(row)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_matches_node_prefix_rules() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.7.3", "0.7.2"), Greater);
        assert_eq!(compare_versions("", "0.7.2"), Less);
        assert_eq!(compare_versions("1.2", "0.0.0"), Equal);
        assert_eq!(compare_versions("10.0.0-beta", "9.9.9"), Greater);
        assert_eq!(compare_versions("v1.0.0", "0.0.0"), Equal);
    }

    #[test]
    fn js_string_matches_string_coercion() {
        assert_eq!(js_string(&json!(5)), "5");
        assert_eq!(js_string(&json!(1.5)), "1.5");
        assert_eq!(js_string(&json!(null)), "null");
        assert_eq!(js_string(&json!(["a", null, 2])), "a,,2");
        assert_eq!(js_string(&json!({"a": 1})), "[object Object]");
    }
}
