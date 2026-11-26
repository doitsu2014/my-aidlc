//! Native `config ...` commands, ported from the Node router (`core/tools/dtech-aidlc.mjs`)
//! and `core/tools/agent-models.mjs`.
//!
//! [`run`] handles the full key space (`config`, `config scope|mode|gate-mode|prerequisite|agents`
//! and any unknown `config X`). Read-only queries delegate to [`crate::config_read`] where its
//! output matches Node; writes validate everything first, preserve unrelated JSON fields and
//! their order, refuse symlinked state paths, replace `config.json` through a temp file and
//! rename, and append the same audit rows as Node.
//!
//! Like Node, `config scope|mode|gate-mode|prerequisite` read `.aidlc/config.json` leniently
//! (`loadConfig`: unreadable or invalid JSON reads as `{}`), while `config agents` validates the
//! file strictly (`validateModelConfig`).

use crate::args::FlagValue;
use crate::harness_commands::{configured_harnesses, harness_manifest, list_harnesses};
use crate::prerequisites::{prerequisites_of, run_prerequisites, validate_prerequisite};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCOPES: &[&str] = &["solo", "team", "enterprise"];
const MODES: &[&str] = &["human", "ai"];
const MODEL_ALIASES: &[&str] = &["opus", "sonnet", "haiku", "fable"];
const GATE_MODE_OVERRIDES: &[&str] = &["human"];
const INVALID_CONFIG: &str =
    "Invalid .aidlc/config.json: harness or scope is missing or unknown; correct the file";

/// Handle one `config ...` command.
///
/// `key` is the command key from `args::command_key` (`config`, `config scope`, ...), `args`
/// the positional arguments after the command words, `flags` the parsed options
/// (`--check`, `--hint`, `--applies-to` for `config prerequisite set`). The value is printed as
/// pretty JSON. The exit code is 1 when `config prerequisite check` reports a failing check
/// (Node prints the result and sets `process.exitCode = 1`); every other success is 0.
pub fn run(
    key: &str,
    project: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let section = key
        .strip_prefix("config")
        .map(str::trim_start)
        .unwrap_or("");
    match section {
        "scope" => scope(&root, args).map(|value| (value, 0)),
        "mode" => mode(&root, args).map(|value| (value, 0)),
        "gate-mode" => gate_mode(&root, args, &engine_graph()?).map(|value| (value, 0)),
        "prerequisite" => prerequisite(&root, args, flags, &engine_graph()?),
        "agents" => agents(&root, args, &engine_graph()?).map(|value| (value, 0)),
        _ => Err(
            "Use config agents, config scope, config mode, config gate-mode, or config prerequisite"
                .into(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Engine graph and configuration loading
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

/// Node `readGraph()`: the validated phase graph of the installed (or embedded) engine.
///
/// The `scope` loaders also read a project configuration; they are pointed at a directory
/// that has no `.aidlc` so only the graph is loaded and validated here. The project
/// configuration is loaded separately with Node's own leniency rules.
fn engine_graph() -> Result<Value, String> {
    let probe = std::env::temp_dir().join(format!(
        "dtech-aidlc-graph-probe-{}-absent",
        std::process::id()
    ));
    let probe = probe.join("no-project");
    let (graph, _) = match external_engine() {
        Some(engine) => crate::scope::load_graph(engine, &probe)?,
        None => crate::scope::load_embedded_graph(&probe)?,
    };
    Ok(graph.value().clone())
}

fn config_path(root: &Path) -> PathBuf {
    root.join(".aidlc").join("config.json")
}

/// Node `existsSync(configPath(root))` and `loadConfig(root)` (`readJson(path, {})`).
fn load_lenient(root: &Path) -> (bool, Value) {
    let path = config_path(root);
    let config = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    (path.exists(), config)
}

fn is_configured(config: &Value) -> bool {
    config
        .get("scope")
        .and_then(Value::as_str)
        .is_some_and(|scope| SCOPES.contains(&scope))
        && !configured_harnesses(config).is_empty()
}

/// Node `requireConfigured(what)` / the inline checks of `config scope|mode`.
fn require_configured(has_config: bool, configured: bool, what: &str) -> Result<(), String> {
    if !has_config {
        return Err(format!("Initialize the project before changing {what}"));
    }
    if !configured {
        return Err(INVALID_CONFIG.into());
    }
    Ok(())
}

/// Node `writeJsonAtomic(configPath(root), config)`. Every caller appends an audit row
/// next, so the audit log is checked first: a log that cannot be appended to fails the
/// command with the configuration unchanged (Node writes, then fails on the append).
fn write_config(root: &Path, config: &Value) -> Result<(), String> {
    crate::audit::preflight_audit(root)?;
    crate::workflow::write_json_atomic(&config_path(root), config)
}

/// JavaScript `String(value)` for values echoed in Node error messages.
fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

fn alias_choices() -> String {
    let (last, rest) = MODEL_ALIASES.split_last().expect("aliases");
    format!("choose {}, or {last}", rest.join(", "))
}

// ---------------------------------------------------------------------------
// config scope / config mode
// ---------------------------------------------------------------------------

fn scope(root: &Path, args: &[String]) -> Result<Value, String> {
    let (has_config, mut config) = load_lenient(root);
    let configured = is_configured(&config);
    let Some(requested) = args.first() else {
        return Ok(crate::config_read::scope(&config, configured));
    };
    if !SCOPES.contains(&requested.as_str()) {
        return Err("scope must be solo, team, or enterprise".into());
    }
    require_configured(has_config, configured, "the default scope")?;
    if config["scope"].as_str() != Some(requested.as_str()) {
        let from = config["scope"].clone();
        config["scope"] = json!(requested);
        write_config(root, &config)?;
        crate::audit::audit(
            root,
            "config.scope.changed",
            json!({"from": from, "to": requested}),
        )?;
    }
    Ok(json!({"scope": requested, "appliesTo": "new intents"}))
}

fn mode(root: &Path, args: &[String]) -> Result<Value, String> {
    let (has_config, mut config) = load_lenient(root);
    let configured = is_configured(&config);
    // `config.mode ?? 'human'`: Node echoes a stored value without validating it here.
    let current = match config.get("mode") {
        None | Some(Value::Null) => json!("human"),
        Some(value) => value.clone(),
    };
    let Some(requested) = args.first() else {
        return Ok(json!({"mode": current, "configured": configured}));
    };
    if !MODES.contains(&requested.as_str()) {
        return Err(format!("mode must be {}", MODES.join(" or ")));
    }
    require_configured(has_config, configured, "the default mode")?;
    if current.as_str() != Some(requested.as_str()) {
        config["mode"] = json!(requested);
        write_config(root, &config)?;
        crate::audit::audit(
            root,
            "config.mode.changed",
            json!({"from": current, "to": requested}),
        )?;
    }
    Ok(json!({"mode": requested, "appliesTo": "new intents"}))
}

// ---------------------------------------------------------------------------
// config gate-mode
// ---------------------------------------------------------------------------

fn gate_mode(root: &Path, args: &[String], graph: &Value) -> Result<Value, String> {
    let (has_config, mut config) = load_lenient(root);
    let configured = is_configured(&config);
    if args.len() < 2 {
        return crate::config_read::gate_mode(&config, graph, args.first().map(String::as_str));
    }
    let gates: Vec<&str> = graph["phases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|phase| phase["gate"].as_str())
        .collect();
    let gate = args[0].as_str();
    if !gates.contains(&gate) {
        return Err(format!(
            "Unknown gate '{gate}'. Gates: {}",
            gates.join(", ")
        ));
    }
    let value = args[1].as_str();
    let allowed: Vec<&str> = GATE_MODE_OVERRIDES
        .iter()
        .copied()
        .chain(["default"])
        .collect();
    if !allowed.contains(&value) {
        return Err(format!(
            "Gate mode must be {}: a gate can be pinned to human approval, never to AI approval",
            allowed.join(" or ")
        ));
    }
    require_configured(has_config, configured, "gate modes")?;
    // `config.gateModes?.[gate] ?? 'default'`
    let from = match config.get("gateModes").and_then(|modes| modes.get(gate)) {
        None | Some(Value::Null) => json!("default"),
        Some(current) => current.clone(),
    };
    if from.as_str() != Some(value) {
        let object = config
            .as_object_mut()
            .ok_or("Invalid project configuration: expected an object")?;
        if value == "default" {
            if let Some(pins) = object.get_mut("gateModes").and_then(Value::as_object_mut) {
                pins.shift_remove(gate);
                if pins.is_empty() {
                    object.shift_remove("gateModes");
                }
            }
        } else {
            let mut pins = object
                .get("gateModes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            pins.insert(gate.to_owned(), json!(value));
            object.insert("gateModes".into(), Value::Object(pins));
        }
        write_config(root, &config)?;
        crate::audit::audit(
            root,
            "config.gate-mode.changed",
            json!({"gate": gate, "from": from, "to": value}),
        )?;
    }
    Ok(json!({
        "gate": gate,
        "mode": value,
        "appliesTo": "every intent, from its next report of this gate",
    }))
}

// ---------------------------------------------------------------------------
// config prerequisite
// ---------------------------------------------------------------------------

/// `{ name, ...definition }`: `name` leads; a `name` field in the definition overrides it.
fn named(name: &str, definition: &Value) -> Value {
    let mut row = Map::new();
    row.insert("name".into(), json!(name));
    if let Some(fields) = definition.as_object() {
        for (key, value) in fields {
            row.insert(key.clone(), value.clone());
        }
    }
    Value::Object(row)
}

fn string_flag<'a>(flags: &'a BTreeMap<String, FlagValue>, name: &str) -> Option<&'a str> {
    match flags.get(name) {
        Some(FlagValue::String(value)) => Some(value),
        _ => None,
    }
}

fn list_flag(flags: &BTreeMap<String, FlagValue>, name: &str) -> Option<Vec<String>> {
    match flags.get(name) {
        Some(FlagValue::List(values)) => Some(values.clone()),
        Some(FlagValue::String(value)) => Some(vec![value.clone()]),
        _ => None,
    }
}

fn prerequisite(
    root: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
    graph: &Value,
) -> Result<(Value, i32), String> {
    let (has_config, mut config) = load_lenient(root);
    let configured = is_configured(&config);
    let defined = prerequisites_of(&config);
    let name = args.get(1).filter(|name| !name.is_empty());
    match args.first().map(String::as_str) {
        None | Some("") => Ok((
            Value::Array(
                defined
                    .iter()
                    .map(|(name, definition)| named(name, definition))
                    .collect(),
            ),
            0,
        )),
        Some("check") => {
            let names: Vec<String> = match name {
                Some(name) => {
                    if !defined.contains_key(name) {
                        let declared = defined.keys().cloned().collect::<Vec<_>>().join(", ");
                        return Err(format!(
                            "Unknown prerequisite '{name}'. Declared: {}",
                            if declared.is_empty() {
                                "none"
                            } else {
                                &declared
                            }
                        ));
                    }
                    vec![name.clone()]
                }
                None => defined.keys().cloned().collect(),
            };
            // Node cannot run a declaration without a command string either (spawnSync
            // throws); fail the command instead of reporting it as passing.
            for name in &names {
                if let Some(definition) = defined.get(name) {
                    crate::prerequisites::require_runnable(name, definition)?;
                }
            }
            let results = run_prerequisites(&root.to_string_lossy(), &config, &names);
            let ok = results.iter().all(|row| row["ok"] == true);
            Ok((json!({"ok": ok, "results": results}), i32::from(!ok)))
        }
        Some("set") => {
            let name = name.ok_or(
                "Usage: config prerequisite set NAME --check \"COMMAND\" [--hint TEXT] [--applies-to STAGE]...",
            )?;
            let mut definition = Map::new();
            if let Some(check) = string_flag(flags, "check") {
                definition.insert("check".into(), json!(check));
            }
            if let Some(hint) = string_flag(flags, "hint") {
                definition.insert("hint".into(), json!(hint));
            }
            if let Some(stages) = list_flag(flags, "applies-to") {
                // `[...new Set(list)]`: drop repeats, keep first-occurrence order.
                let mut unique: Vec<String> = Vec::new();
                for stage in stages {
                    if !unique.contains(&stage) {
                        unique.push(stage);
                    }
                }
                definition.insert("stages".into(), json!(unique));
            }
            let definition = Value::Object(definition);
            validate_prerequisite(name, &definition, Some(graph))?;
            require_configured(has_config, configured, "prerequisites")?;
            let mut next = defined;
            next.insert(name.clone(), definition.clone());
            config["prerequisites"] = Value::Object(next);
            write_config(root, &config)?;
            crate::audit::audit(
                root,
                "config.prerequisite.set",
                json!({
                    "name": name,
                    "check": definition["check"],
                    "stages": definition.get("stages").cloned().unwrap_or_else(|| json!([])),
                }),
            )?;
            Ok((named(name, &definition), 0))
        }
        Some("remove") => {
            let Some(name) = name.filter(|name| defined.contains_key(*name)) else {
                let shown = args.get(1).map_or("(missing name)", String::as_str);
                return Err(format!("Unknown prerequisite '{shown}'"));
            };
            require_configured(has_config, configured, "prerequisites")?;
            let mut rest = defined;
            rest.shift_remove(name);
            let object = config
                .as_object_mut()
                .ok_or("Invalid project configuration: expected an object")?;
            if rest.is_empty() {
                object.shift_remove("prerequisites");
            } else {
                object.insert("prerequisites".into(), Value::Object(rest));
            }
            write_config(root, &config)?;
            crate::audit::audit(root, "config.prerequisite.removed", json!({"name": name}))?;
            Ok((json!({"name": name, "removed": true}), 0))
        }
        Some(_) => Err("Use config prerequisite set, remove or check".into()),
    }
}

// ---------------------------------------------------------------------------
// config agents (Node agent-models.mjs)
// ---------------------------------------------------------------------------

fn lstat_exists(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Node `validateModelConfig(root, graph)`: strict load and validation of `config.json`.
pub fn validate_model_config(root: &Path, graph: &Value) -> Result<Value, String> {
    let dir = root.join(".aidlc");
    if lstat_exists(&dir)?.is_some_and(|meta| meta.file_type().is_symlink()) {
        return Err("Refusing to access symlink .aidlc directory".into());
    }
    let path = config_path(root);
    let Some(meta) = lstat_exists(&path)? else {
        return Ok(json!({}));
    };
    if meta.file_type().is_symlink() {
        return Err("Refusing to access symlink config.json".into());
    }
    let config: Value = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or("Invalid project configuration JSON")?;
    if !config.is_object() {
        return Err("Invalid project configuration: expected an object".into());
    }
    if let Some(mode) = config.get("mode") {
        if !mode.as_str().is_some_and(|mode| MODES.contains(&mode)) {
            return Err(format!("mode must be {}", MODES.join(" or ")));
        }
    }
    if let Some(models) = config.get("agentModels") {
        let models = models
            .as_object()
            .ok_or("Invalid agentModels: expected an object mapping agent IDs to model aliases")?;
        let ids: Vec<&str> = graph["agents"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|agent| agent["id"].as_str())
            .collect();
        for (id, model) in models {
            if !ids.contains(&id.as_str()) {
                return Err(format!("Unknown agent in agentModels: {id}"));
            }
            if !model.as_str().is_some_and(|m| MODEL_ALIASES.contains(&m)) {
                return Err(format!(
                    "Invalid model alias for {id}: {}; {}",
                    js_string(model),
                    alias_choices()
                ));
            }
        }
    }
    if let Some(pins) = config.get("gateModes") {
        let pins = pins
            .as_object()
            .ok_or("Invalid gateModes: expected an object mapping gate ids to \"human\"")?;
        let gates: Vec<&str> = graph["phases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|phase| phase["gate"].as_str())
            .collect();
        for (gate, value) in pins {
            if !gates.is_empty() && !gates.contains(&gate.as_str()) {
                return Err(format!("Unknown gate in gateModes: {gate}"));
            }
            if !value
                .as_str()
                .is_some_and(|v| GATE_MODE_OVERRIDES.contains(&v))
            {
                return Err(format!(
                    "Invalid gateModes.{gate}: a gate can only be pinned to {}",
                    GATE_MODE_OVERRIDES.join(", ")
                ));
            }
        }
    }
    if let Some(prerequisites) = config.get("prerequisites") {
        let entries = prerequisites
            .as_object()
            .ok_or("Invalid prerequisites: expected an object mapping names to { check }")?;
        let graph_ref = graph.get("phases").filter(|p| !p.is_null()).map(|_| graph);
        for (name, definition) in entries {
            validate_prerequisite(name, definition, graph_ref)?;
        }
    }
    if let Some(threshold) = config.get("unitSplitThreshold") {
        let ok = threshold
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= 1.0);
        if !ok {
            return Err("Invalid unitSplitThreshold: expected a positive integer".into());
        }
    }
    if let Some(threshold) = config.get("autoParkThreshold") {
        let ok = threshold
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= 1.0);
        if !ok {
            return Err("Invalid autoParkThreshold: expected a positive integer".into());
        }
    }
    if let Some(roles) = config.get("roles") {
        let roles = roles
            .as_object()
            .ok_or("Invalid roles: expected an object")?;
        if let Some(owner) = roles.get("owner") {
            if !owner
                .as_array()
                .is_some_and(|names| names.iter().all(Value::is_string))
            {
                return Err("Invalid roles.owner: expected an array of names".into());
            }
        }
        if let Some(approvers) = roles.get("approvers") {
            let approvers = approvers
                .as_object()
                .ok_or("Invalid roles.approvers: expected an object mapping gate ids to names")?;
            for (gate, names) in approvers {
                if !names
                    .as_array()
                    .is_some_and(|names| names.iter().all(Value::is_string))
                {
                    return Err(format!("Invalid roles.approvers.{gate}"));
                }
            }
        }
    }
    Ok(config)
}

/// Node `aliasesSupported(config)`: true without configured harnesses, else when any
/// configured harness declares model aliases.
pub fn aliases_supported(config: &Value) -> Result<bool, String> {
    let harnesses = configured_harnesses(config);
    if harnesses.is_empty() {
        return Ok(true);
    }
    for name in harnesses {
        let manifest = harness_manifest(&name)?;
        if manifest["modelAliases"]
            .as_array()
            .is_some_and(|aliases| !aliases.is_empty())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Node `resolveAgentModel(agent, config)` fields (`defaultModel`, `model`, `source`).
fn resolve_agent_model(agent: &Value, config: &Value) -> Result<Map<String, Value>, String> {
    let default = agent
        .get("model")
        .filter(|v| !v.is_null())
        .or_else(|| {
            agent
                .get("modelRecommendation")
                .and_then(|r| r.get("model"))
        })
        .filter(|v| !v.is_null())
        .cloned();
    let mut row = Map::new();
    if let Some(default) = &default {
        row.insert("defaultModel".into(), default.clone());
    }
    if !aliases_supported(config)? {
        row.insert("model".into(), Value::Null);
        row.insert("source".into(), json!("unsupported"));
        return Ok(row);
    }
    let id = agent["id"].as_str().unwrap_or_default();
    let chosen = config
        .get("agentModels")
        .and_then(|models| models.get(id))
        .filter(|v| !v.is_null())
        .cloned();
    let source = if chosen.is_some() {
        "project"
    } else {
        "default"
    };
    if let Some(model) = chosen.or(default) {
        row.insert("model".into(), model);
    }
    row.insert("source".into(), json!(source));
    Ok(row)
}

/// Node `writeModelConfig(root, config)`.
fn write_model_config(root: &Path, config: &Value) -> Result<(), String> {
    let dir = root.join(".aidlc");
    let refuse_dir = || "Refusing to write through symlink .aidlc directory".to_owned();
    if lstat_exists(&dir)?.is_some_and(|meta| meta.file_type().is_symlink()) {
        return Err(refuse_dir());
    }
    fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    if lstat_exists(&dir)?.is_some_and(|meta| meta.file_type().is_symlink()) {
        return Err(refuse_dir());
    }
    let path = config_path(root);
    if lstat_exists(&path)?.is_some_and(|meta| meta.file_type().is_symlink()) {
        return Err("Refusing to write through symlink config.json".into());
    }
    // `config agents set|reset` audits next; see `write_config`.
    crate::audit::preflight_audit(root)?;
    crate::workflow::write_json_atomic(&path, config)
}

fn agents(root: &Path, args: &[String], graph: &Value) -> Result<Value, String> {
    let mut config = validate_model_config(root, graph)?;
    let rows = graph["agents"]
        .as_array()
        .ok_or("Invalid phase-graph.json: agents must be an array")?;
    let Some(action) = args.first().map(String::as_str) else {
        let mut out = Vec::new();
        for agent in rows {
            let mut row = Map::new();
            row.insert("id".into(), agent["id"].clone());
            if let Some(phase) = agent.get("phase").filter(|v| !v.is_null()) {
                row.insert("phase".into(), phase.clone());
            }
            row.extend(resolve_agent_model(agent, &config)?);
            out.push(Value::Object(row));
        }
        return Ok(Value::Array(out));
    };
    if action != "set" && action != "reset" {
        return Err("Use config agents set or config agents reset".into());
    }
    let id = args.get(1);
    let Some(agent) = id.and_then(|id| rows.iter().find(|row| row["id"].as_str() == Some(id)))
    else {
        return Err(format!(
            "Unknown agent: {}",
            id.map_or("(missing agent ID)", String::as_str)
        ));
    };
    let id = id.expect("agent found").clone();
    if !config_path(root).exists() {
        return Err("Initialize the project before changing agent models".into());
    }
    let initialized = config
        .get("harness")
        .and_then(Value::as_str)
        .is_some_and(|harness| list_harnesses().contains(&harness))
        && config
            .get("scope")
            .and_then(Value::as_str)
            .is_some_and(|scope| SCOPES.contains(&scope))
        && config
            .get("initializedAt")
            .is_some_and(|value| match value {
                Value::Null => false,
                Value::Bool(flag) => *flag,
                Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
                Value::String(text) => !text.is_empty(),
                _ => true,
            });
    if !initialized {
        return Err("Initialize the project before changing agent models".into());
    }
    if action == "set" {
        let model = args.get(2);
        if !aliases_supported(&config)? {
            return Err(format!(
                "Model aliases ({}) apply to the claude harness only; this project uses {}, which has no model alias support. Nothing was changed.",
                MODEL_ALIASES.join(", "),
                configured_harnesses(&config).join(", ")
            ));
        }
        let Some(model) = model.filter(|model| MODEL_ALIASES.contains(&model.as_str())) else {
            return Err(format!(
                "Invalid model alias: {}; {}",
                model.map_or("(missing)", String::as_str),
                alias_choices()
            ));
        };
        config
            .as_object_mut()
            .ok_or("Invalid project configuration: expected an object")?
            .entry("agentModels")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("Invalid agentModels: expected an object mapping agent IDs to model aliases")?
            .insert(id.clone(), json!(model));
    } else if let Some(models) = config.get_mut("agentModels").and_then(Value::as_object_mut) {
        // Node deletes only the agent's key and keeps an emptied `agentModels: {}`.
        models.shift_remove(&id);
    }
    write_model_config(root, &config)?;
    if action == "set" {
        crate::audit::audit(
            root,
            "config.agent-model.set",
            json!({"agent": id, "model": config["agentModels"][&id]}),
        )?;
    } else {
        crate::audit::audit(root, "config.agent-model.reset", json!({"agent": id}))?;
    }
    let mut out = Map::new();
    out.insert("id".into(), json!(id));
    out.extend(resolve_agent_model(agent, &config)?);
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "aidlc-config-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn words(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).into()).collect()
    }
    fn none() -> BTreeMap<String, FlagValue> {
        BTreeMap::new()
    }
    fn call(t: &Temp, key: &str, args: &[&str]) -> Result<Value, String> {
        run(key, &t.0, &words(args), &none()).map(|(value, _)| value)
    }
    fn saved(t: &Temp) -> Value {
        serde_json::from_slice(&fs::read(t.0.join(".aidlc/config.json")).unwrap()).unwrap()
    }
    fn init(t: &Temp, extra: &str) {
        fs::create_dir_all(t.0.join(".aidlc")).unwrap();
        fs::write(
            t.0.join(".aidlc/config.json"),
            format!(
                r#"{{"harness":"claude","scope":"solo","initializedAt":"2024-01-01T00:00:00.000Z"{extra}}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn reads_and_updates_scope_mode_preserving_unknown_fields_and_auditing() {
        let t = Temp::new();
        init(&t, r#","mode":"human","custom":{"keep":true}"#);
        assert_eq!(
            call(&t, "config scope", &[]).unwrap(),
            json!({"scope":"solo","configured":true})
        );
        call(&t, "config scope", &["team"]).unwrap();
        call(&t, "config mode", &["ai"]).unwrap();
        let saved = saved(&t);
        assert_eq!(saved["custom"]["keep"], true);
        assert_eq!(saved["scope"], "team");
        assert_eq!(saved["mode"], "ai");
        assert_eq!(
            fs::read_to_string(t.0.join(".aidlc/audit.jsonl"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }

    #[test]
    fn gate_mode_queries_pins_and_removes_without_touching_other_fields() {
        let t = Temp::new();
        init(&t, r#","custom":1"#);
        let graph = engine_graph().unwrap();
        let gate = graph["phases"][0]["gate"].as_str().unwrap().to_owned();
        assert_eq!(
            call(&t, "config gate-mode", &[&gate]).unwrap(),
            json!({"gate":gate,"mode":"default"})
        );
        call(&t, "config gate-mode", &[&gate, "human"]).unwrap();
        assert_eq!(saved(&t)["gateModes"][&gate], "human");
        call(&t, "config gate-mode", &[&gate, "default"]).unwrap();
        let saved = saved(&t);
        assert!(saved.get("gateModes").is_none());
        assert_eq!(saved["custom"], 1);
        assert!(call(&t, "config gate-mode", &["missing"])
            .unwrap_err()
            .contains("Unknown gate"));
        assert!(call(&t, "config gate-mode", &[&gate, "ai"])
            .unwrap_err()
            .starts_with("Gate mode must be human or default"));
    }

    #[test]
    fn prerequisite_writes_validate_deduplicate_and_preserve_configuration() {
        let t = Temp::new();
        init(&t, r#","custom":"x""#);
        let graph = engine_graph().unwrap();
        let stage = graph["stages"][0]["id"].as_str().unwrap().to_owned();
        let phase = graph["phases"][0]["id"].as_str().unwrap().to_owned();
        let mut flags = none();
        flags.insert("check".into(), FlagValue::String("exit 0".into()));
        flags.insert("hint".into(), FlagValue::String("ready".into()));
        flags.insert(
            "applies-to".into(),
            FlagValue::List(vec![stage.clone(), phase.clone(), stage.clone()]),
        );
        let (out, _) = run(
            "config prerequisite",
            &t.0,
            &words(&["set", "tools"]),
            &flags,
        )
        .unwrap();
        assert_eq!(
            out,
            json!({"name":"tools","check":"exit 0","hint":"ready","stages":[stage, phase]})
        );
        let config = saved(&t);
        assert_eq!(
            config["prerequisites"]["tools"]["stages"],
            json!([stage, phase])
        );
        assert_eq!(config["custom"], "x");
        assert!(run("config prerequisite", &t.0, &words(&["set", "Bad"]), &flags).is_err());
        let (check, code) = run("config prerequisite", &t.0, &words(&["check"]), &none()).unwrap();
        assert_eq!((check["ok"].clone(), code), (json!(true), 0));
        call(&t, "config prerequisite", &["remove", "tools"]).unwrap();
        assert!(saved(&t).get("prerequisites").is_none());
    }

    #[test]
    fn agents_set_and_reset_only_change_valid_agent_override() {
        let t = Temp::new();
        init(&t, r#","custom":true"#);
        let graph = engine_graph().unwrap();
        let agent = graph["agents"][0]["id"].as_str().unwrap().to_owned();
        let set = call(&t, "config agents", &["set", &agent, "fable"]).unwrap();
        assert_eq!(set["model"], "fable");
        assert_eq!(set["source"], "project");
        assert_eq!(
            call(&t, "config agents", &["set", "bad", "fable"]).unwrap_err(),
            "Unknown agent: bad"
        );
        call(&t, "config agents", &["reset", &agent]).unwrap();
        let saved = saved(&t);
        // Node keeps the emptied object after a reset.
        assert_eq!(saved["agentModels"], json!({}));
        assert_eq!(saved["custom"], true);
    }

    #[test]
    fn changes_require_an_initialized_project_and_leave_no_state() {
        let t = Temp::new();
        assert_eq!(
            call(&t, "config scope", &["team"]).unwrap_err(),
            "Initialize the project before changing the default scope"
        );
        assert_eq!(
            call(&t, "config mode", &["robot"]).unwrap_err(),
            "mode must be human or ai"
        );
        assert!(!t.0.join(".aidlc").exists());
        assert_eq!(
            call(&t, "config", &[]).unwrap_err(),
            "Use config agents, config scope, config mode, config gate-mode, or config prerequisite"
        );
    }

    #[test]
    fn refuses_symlinked_config() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join(".aidlc")).unwrap();
        let target = t.0.join("outside.json");
        fs::write(
            &target,
            r#"{"harness":"claude","scope":"solo","initializedAt":"x"}"#,
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, t.0.join(".aidlc/config.json")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&target, t.0.join(".aidlc/config.json")).is_err() {
            return;
        }
        assert!(call(&t, "config agents", &[])
            .unwrap_err()
            .contains("symlink"));
        assert!(call(&t, "config scope", &["team"])
            .unwrap_err()
            .contains("Refusing to write through symlink"));
        assert!(fs::read_to_string(&target).unwrap().contains("solo"));
    }
}
