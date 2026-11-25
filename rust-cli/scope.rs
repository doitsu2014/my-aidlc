//! Read-only native implementation of `dtech-aidlc scope` graph loading.
//!
//! Call [`load_graph`] with the engine root (the directory containing `data/`
//! and `methodology/`) and project root. The graph and project config are treated
//! as untrusted input; this module never writes files.

use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));
}

pub const GRAPH_SCHEMA_VERSION: &str = "1.5.0";
pub const MODEL_ALIASES: &[&str] = &["opus", "sonnet", "haiku", "fable"];
pub const SCOPES: &[&str] = &["solo", "team", "enterprise"];

#[derive(Debug, Clone)]
pub struct Graph {
    raw: Value,
}

impl Graph {
    pub fn value(&self) -> &Value {
        &self.raw
    }
    pub fn phases(&self) -> &[Value] {
        self.raw["phases"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn stages(&self) -> &[Value] {
        self.raw["stages"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn agents(&self) -> &[Value] {
        self.raw["agents"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn known_depths(&self) -> Vec<String> {
        let mut out = vec!["full".to_owned()];
        if let Some(depths) = self.raw["depths"].as_object() {
            out.extend(depths.keys().cloned());
        }
        out
    }
    pub fn resolve_depth<'a>(&self, depth: Option<&'a str>) -> Result<&'a str, String> {
        let name = depth.filter(|s| !s.is_empty()).unwrap_or("full");
        if name == "full" {
            return Ok("full");
        }
        if self.raw["depths"].get(name).is_none() {
            return Err(format!(
                "Unknown depth '{name}'. Known depths: {}",
                self.known_depths().join(", ")
            ));
        }
        Ok(name)
    }

    /// Stage rows in phase/stage order, matching the JavaScript `orderedStages` helper.
    pub fn ordered_stages(&self, scope: &str, depth: &str) -> Vec<Value> {
        let included: HashSet<&str> = self.raw["scopes"][scope]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let phase_order: HashMap<&str, f64> = self
            .phases()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                p["id"].as_str().map(|id| {
                    (
                        id,
                        p["order"]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .unwrap_or(i as f64),
                    )
                })
            })
            .collect();
        let allowed: Option<HashSet<&str>> = if depth != "full" {
            Some(
                self.raw["depths"][depth]["stages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect(),
            )
        } else {
            None
        };
        let mut rows: Vec<(usize, Value)> = self
            .stages()
            .iter()
            .enumerate()
            .filter(|(_, s)| s["phase"].as_str().is_some_and(|p| included.contains(p)))
            .filter(|(_, s)| {
                allowed
                    .as_ref()
                    .is_none_or(|a| s["id"].as_str().is_some_and(|id| a.contains(id)))
            })
            .map(|(i, s)| (i, s.clone()))
            .collect();
        rows.sort_by(|(ai, a), (bi, b)| {
            let phase_a = phase_order
                .get(a["phase"].as_str().unwrap_or(""))
                .copied()
                .unwrap_or(f64::INFINITY);
            let phase_b = phase_order
                .get(b["phase"].as_str().unwrap_or(""))
                .copied()
                .unwrap_or(f64::INFINITY);
            phase_a.total_cmp(&phase_b).then_with(|| {
                a["order"]
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .unwrap_or(*ai as f64)
                    .total_cmp(
                        &b["order"]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .unwrap_or(*bi as f64),
                    )
            })
        });
        rows.into_iter().map(|(_, s)| s).collect()
    }

    /// Scope response matching `dtech-aidlc scope [NAME] [--depth NAME]`.
    pub fn scope_output(
        &self,
        scope: Option<&str>,
        depth: Option<&str>,
        config: &Value,
    ) -> Result<Value, String> {
        let depth = self.resolve_depth(depth)?;
        let agent_models = config.get("agentModels").and_then(Value::as_object);
        let configured = config
            .get("harnesses")
            .and_then(Value::as_array)
            .filter(|h| {
                !h.is_empty()
                    && h.iter()
                        .all(|name| matches!(name.as_str(), Some("claude" | "deepseek")))
            })
            .cloned()
            .unwrap_or_else(|| {
                config
                    .get("harness")
                    .filter(|v| matches!(v.as_str(), Some("claude" | "deepseek")))
                    .cloned()
                    .into_iter()
                    .collect()
            });
        let aliases_supported =
            configured.is_empty() || configured.iter().any(|h| h.as_str() == Some("claude"));
        let make_scope = |name: &str| -> Value {
            let stages = self.ordered_stages(name, depth);
            let rows: Vec<Value> = stages
                .iter()
                .map(|stage| {
                    let id = stage["agent"].as_str();
                    let mut row = Map::new();
                    row.insert("id".into(), stage["id"].clone());
                    row.insert("phase".into(), stage["phase"].clone());
                    row.insert("agent".into(), id.map(|s| json!(s)).unwrap_or(Value::Null));
                    if let Some(agent_id) = id.filter(|_| aliases_supported) {
                        if let Some(agent) = self
                            .agents()
                            .iter()
                            .find(|a| a["id"].as_str() == Some(agent_id))
                        {
                            let default = agent["model"].as_str().unwrap_or("");
                            let override_model = agent_models
                                .and_then(|m| m.get(agent_id))
                                .and_then(Value::as_str);
                            let model = override_model.unwrap_or(default);
                            row.insert("defaultModel".into(), json!(default));
                            row.insert("model".into(), json!(model));
                            row.insert(
                                "source".into(),
                                json!(if override_model.is_some() {
                                    "project"
                                } else {
                                    "default"
                                }),
                            );
                        }
                    }
                    row.insert(
                        "produces".into(),
                        stage["produces"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                            .into(),
                    );
                    Value::Object(row)
                })
                .collect();
            json!({"scope":name,"depth":depth,"stages":rows})
        };
        if let Some(name) = scope {
            if !SCOPES.contains(&name) {
                return Err("scope must be solo, team, or enterprise".into());
            }
            Ok(make_scope(name))
        } else {
            Ok(Value::Array(SCOPES.iter().map(|name| {
                let output = make_scope(name);
                json!({"scope":name,"depth":depth,"stages":output["stages"].as_array().unwrap().iter().filter_map(|r| r["id"].as_str().map(str::to_owned)).collect::<Vec<_>>()})
            }).collect()))
        }
    }
}

/// Load and validate `engine_root/data/phase-graph.json`, including referenced Markdown.
pub fn load_graph(
    engine_root: impl AsRef<Path>,
    project_root: impl AsRef<Path>,
) -> Result<(Graph, Value), String> {
    let root = engine_root.as_ref();
    let graph_path = root.join("data/phase-graph.json");
    reject_symlink(&graph_path, "phase-graph.json")?;
    let bytes = fs::read(&graph_path)
        .map_err(|e| format!("Missing methodology graph: {} ({e})", graph_path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid phase-graph.json: malformed JSON ({e})"))?;
    validate_graph(Some(root), &value)?;
    let config = load_config(project_root.as_ref())?;
    validate_config(&config, &value)?;
    Ok((Graph { raw: value }, config))
}

/// Load the graph and methodology packaged in this executable.
pub fn load_embedded_graph(project_root: impl AsRef<Path>) -> Result<(Graph, Value), String> {
    let bytes = embedded::ASSETS
        .iter()
        .find(|(name, _)| *name == "data/phase-graph.json")
        .map(|(_, bytes)| *bytes)
        .ok_or("Missing embedded methodology graph")?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("Invalid embedded phase-graph.json: {e}"))?;
    validate_graph(None, &value)?;
    let config = load_config(project_root.as_ref())?;
    validate_config(&config, &value)?;
    Ok((Graph { raw: value }, config))
}

/// Loads `.aidlc/config.json`; missing config is equivalent to `{}`.
pub fn load_config(project_root: &Path) -> Result<Value, String> {
    let dir = project_root.join(".aidlc");
    reject_symlink(&dir, "Refusing to access symlink .aidlc directory")?;
    let path = dir.join("config.json");
    reject_symlink(&path, "Refusing to access symlink config.json")?;
    if !path.exists() {
        return Ok(json!({}));
    }
    let data = fs::read(&path).map_err(|e| format!("Invalid project configuration JSON: {e}"))?;
    let value: Value = serde_json::from_slice(&data)
        .map_err(|_| "Invalid project configuration JSON".to_owned())?;
    if !value.is_object() {
        return Err("Invalid project configuration: expected an object".into());
    }
    Ok(value)
}

fn reject_symlink(path: &Path, message: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(message.to_owned()),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
fn invalid(msg: impl AsRef<str>) -> String {
    format!("Invalid phase-graph.json: {}", msg.as_ref())
}
fn string<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}
fn integer_valued_number(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && number.fract() == 0.0)
}
fn arr<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(format!("{key} must be an array")))
}
fn unique_ids(rows: &[Value], label: &str) -> Result<HashSet<String>, String> {
    let mut ids = HashSet::new();
    for row in rows {
        let id = string(row, "id")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid(format!("{label} IDs must be non-empty and unique")))?;
        if !ids.insert(id.to_owned()) {
            return Err(invalid(format!("{label} IDs must be non-empty and unique")));
        }
    }
    Ok(ids)
}
fn safe_file(root: &Path, value: Option<&str>, label: &str) -> Result<PathBuf, String> {
    let raw = value.ok_or_else(|| {
        invalid(format!(
            "{label} file must be a relative path inside the engine root"
        ))
    })?;
    let rel = Path::new(raw);
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(invalid(format!(
            "{label} file must be a relative path inside the engine root"
        )));
    }
    let mut current = root.to_path_buf();
    for component in rel.components() {
        if let Component::Normal(part) = component {
            current.push(part);
            reject_symlink(
                &current,
                &format!("{label} file must not traverse symlinks"),
            )?;
        }
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|e| invalid(format!("engine root is unavailable: {e}")))?;
    let canonical = fs::canonicalize(&current)
        .map_err(|_| invalid(format!("{label} file does not exist: {raw}")))?;
    if !canonical.starts_with(canonical_root) || !canonical.is_file() {
        return Err(invalid(format!(
            "{label} file resolves outside the engine root or is not a file"
        )));
    }
    Ok(canonical)
}
fn frontmatter(path: &Path, label: &str) -> Result<(HashMap<String, String>, String), String> {
    let text = fs::read_to_string(path)
        .map_err(|e| invalid(format!("{label} Markdown cannot be read: {e}")))?;
    parse_frontmatter(&text, label, &path.display().to_string())
}

fn parse_frontmatter(
    text: &str,
    label: &str,
    location: &str,
) -> Result<(HashMap<String, String>, String), String> {
    if !text.starts_with("---\n") && !text.starts_with("---\r\n") {
        return Err(invalid(format!(
            "{label} Markdown requires frontmatter: {}",
            location
        )));
    }
    let normalized = text.replace("\r\n", "\n");
    let remainder = &normalized[4..];
    let end = remainder
        .find("\n---\n")
        .or_else(|| remainder.strip_suffix("\n---").map(|_| remainder.len() - 4))
        .ok_or_else(|| {
            invalid(format!(
                "{label} Markdown requires frontmatter: {}",
                location
            ))
        })?;
    let body = &remainder[..end];
    let mut fields = HashMap::new();
    for line in body.lines() {
        let Some((key, raw_value)) = line.split_once(':') else {
            continue;
        };
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            continue;
        }
        let value = raw_value.trim();
        if value.starts_with("- ") {
            continue;
        }
        fields.insert(key.to_owned(), value.trim_matches(['\'', '"']).to_owned());
    }
    Ok((fields, body.to_owned()))
}
fn list_field(md: &str, key: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut active = false;
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
            active = rest.trim().is_empty();
            continue;
        }
        if active {
            if let Some(item) = line.strip_prefix("  - ") {
                output.push(item.trim().to_owned());
            } else if !line.starts_with("  ") {
                active = false;
            }
        }
    }
    output
}

fn asset_frontmatter(
    root: Option<&Path>,
    file: Option<&str>,
    label: &str,
) -> Result<(HashMap<String, String>, String), String> {
    if let Some(root) = root {
        return frontmatter(&safe_file(root, file, label)?, label);
    }
    let file = file.ok_or_else(|| invalid(format!("{label} file is missing")))?;
    let bytes = embedded::ASSETS
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, bytes)| *bytes)
        .ok_or_else(|| invalid(format!("{label} file does not exist: {file}")))?;
    let text = std::str::from_utf8(bytes)
        .map_err(|e| invalid(format!("{label} Markdown cannot be read: {e}")))?;
    parse_frontmatter(text, label, file)
}

fn validate_asset(root: Option<&Path>, file: Option<&str>, label: &str) -> Result<(), String> {
    if let Some(root) = root {
        safe_file(root, file, label)?;
    } else {
        let file = file.ok_or_else(|| invalid(format!("{label} file is missing")))?;
        if !embedded::ASSETS.iter().any(|(name, _)| *name == file) {
            return Err(invalid(format!("{label} file does not exist: {file}")));
        }
    }
    Ok(())
}

fn validate_graph(root: Option<&Path>, g: &Value) -> Result<(), String> {
    if !g.is_object() || g["schemaVersion"] != GRAPH_SCHEMA_VERSION {
        return Err(invalid(format!("schemaVersion must be {GRAPH_SCHEMA_VERSION} (depths, work-unit repeat and data outputs are required)")));
    }
    let phases = arr(g, "phases")?;
    let stages = arr(g, "stages")?;
    let agents = arr(g, "agents")?;
    let pids = unique_ids(phases, "phase")?;
    let sids = unique_ids(stages, "stage")?;
    let aids = unique_ids(agents, "agent")?;
    let aliases = g["modelAliases"]
        .as_array()
        .ok_or_else(|| invalid("modelAliases must be a unique allowlisted alias array"))?;
    let mut alias_set = HashSet::new();
    if aliases.is_empty()
        || aliases.iter().any(|a| {
            !a.as_str().is_some_and(|s| MODEL_ALIASES.contains(&s))
                || !alias_set.insert(a.as_str().unwrap_or(""))
        })
    {
        return Err(invalid(
            "modelAliases must be a unique allowlisted alias array",
        ));
    }
    for p in phases {
        let id = string(p, "id").unwrap();
        if !integer_valued_number(&p["order"])
            || string(p, "name").is_none()
            || string(p, "gate").is_none()
            || !p["agents"].is_array()
            || !p["stages"].is_array()
        {
            return Err(invalid(format!("invalid phase metadata: {id}")));
        }
        let (md, text) = asset_frontmatter(root, string(p, "file"), &format!("phase {id}"))?;
        if md.get("id").map(String::as_str) != Some(id) {
            return Err(invalid(format!("phase {id} Markdown id mismatch")));
        }
        let ma = list_field(&text, "agents");
        let ms = list_field(&text, "stages");
        if p["agents"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
            != ma
            || p["stages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
                != ms
        {
            return Err(invalid(format!(
                "phase {id} membership/order disagrees with Markdown"
            )));
        }
        for agent in p["agents"].as_array().unwrap() {
            let aid = agent.as_str().unwrap_or("");
            if !aids.contains(aid)
                || agents
                    .iter()
                    .find(|a| string(a, "id") == Some(aid))
                    .and_then(|a| string(a, "phase"))
                    != Some(id)
            {
                return Err(invalid(format!(
                    "phase {id} references invalid agent {aid}"
                )));
            }
        }
        for sid in p["stages"].as_array().unwrap() {
            let sid = sid.as_str().unwrap_or("");
            if !sids.contains(sid)
                || stages
                    .iter()
                    .find(|s| string(s, "id") == Some(sid))
                    .and_then(|s| string(s, "phase"))
                    != Some(id)
            {
                return Err(invalid(format!(
                    "phase {id} references invalid stage {sid}"
                )));
            }
        }
        if text
            .lines()
            .find_map(|l| l.strip_prefix("gate:").map(|v| v.trim()))
            != string(p, "gate")
        {
            return Err(invalid(format!("phase {id} gate disagrees with Markdown")));
        }
    }
    let mut produced = HashSet::new();
    for s in stages {
        let id = string(s, "id").unwrap();
        if !string(s, "phase").is_some_and(|x| pids.contains(x))
            || !string(s, "agent").is_some_and(|x| aids.contains(x))
            || !integer_valued_number(&s["order"])
            || !s["produces"].is_array()
        {
            return Err(invalid(format!("invalid stage metadata: {id}")));
        }
        if s["repeat"].as_str().is_some_and(|r| r != "work-unit") {
            return Err(invalid(format!("stage {id} has invalid repeat")));
        }
        let inputs = &s["inputs"];
        if !inputs.is_object() || !inputs["artifacts"].is_array() || !inputs["context"].is_array() {
            return Err(invalid(format!(
                "stage {id} must declare inputs.artifacts and inputs.context"
            )));
        }
        let context_parts = [
            "request",
            "project",
            "sources",
            "knowledge",
            "profile",
            "repos",
        ];
        for part in inputs["context"].as_array().unwrap() {
            if !part.as_str().is_some_and(|p| context_parts.contains(&p)) {
                return Err(invalid(format!(
                    "stage {id} has unknown context part {}",
                    part
                )));
            }
        }
        for artifact in inputs["artifacts"].as_array().unwrap() {
            if artifact.as_str() != Some("*") && !artifact.as_str().is_some_and(valid_output) {
                return Err(invalid(format!(
                    "stage {id} inputs.artifacts has invalid id"
                )));
            }
        }
        if let Some(optional) = inputs.get("optionalArtifacts") {
            if !optional
                .as_array()
                .is_some_and(|xs| xs.iter().all(|x| x.as_str().is_some_and(valid_output)))
            {
                return Err(invalid(format!(
                    "stage {id} inputs.optionalArtifacts has invalid id"
                )));
            }
        }
        for output in s["produces"].as_array().unwrap() {
            let o = output.as_str().unwrap_or("");
            if !valid_output(o) {
                return Err(invalid(format!("stage {id} produces an invalid output id")));
            }
            if !produced.insert(o.to_owned()) {
                return Err(invalid(format!(
                    "output {o} is produced by more than one stage"
                )));
            }
        }
    }
    for a in agents {
        let id = string(a, "id").unwrap();
        let phase = string(a, "phase").unwrap_or("");
        if !pids.contains(phase) || !string(a, "model").is_some_and(|m| alias_set.contains(m)) {
            return Err(invalid(format!("invalid phase or model for agent {id}")));
        }
        let (md, _) = asset_frontmatter(root, string(a, "file"), &format!("agent {id}"))?;
        if md.get("id").map(String::as_str) != Some(id)
            || md.get("phase").map(String::as_str) != Some(phase)
            || md.contains_key("model")
        {
            return Err(invalid(format!(
                "agent {id} id or phase disagrees with Markdown or Markdown declares a model"
            )));
        }
        let p = phases
            .iter()
            .find(|p| string(p, "id") == Some(phase))
            .unwrap();
        if !p["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some(id))
        {
            return Err(invalid(format!("agent {id} missing from phase membership")));
        }
    }
    for p in phases {
        let id = string(p, "id").unwrap();
        let mut members: Vec<_> = stages
            .iter()
            .filter(|s| string(s, "phase") == Some(id))
            .collect();
        members.sort_by(|a, b| {
            a["order"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["order"].as_f64().unwrap_or(0.0))
        });
        if members
            .iter()
            .filter_map(|s| string(s, "id"))
            .collect::<Vec<_>>()
            != p["stages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
        {
            return Err(invalid(format!(
                "phase {id} stage membership/order is inconsistent"
            )));
        }
    }
    let scopes = g["scopes"]
        .as_object()
        .ok_or_else(|| invalid("scopes must be an object"))?;
    for (name, list) in scopes {
        if !list.is_array()
            || list
                .as_array()
                .unwrap()
                .iter()
                .any(|v| !v.as_str().is_some_and(|id| pids.contains(id)))
        {
            return Err(invalid(format!("invalid phase references in scope {name}")));
        }
    }
    if let Some(outputs) = g.get("outputs") {
        let entries = outputs
            .as_object()
            .ok_or_else(|| invalid("outputs must be an object"))?;
        for (id, entry) in entries {
            if !produced.contains(id) {
                return Err(invalid(format!(
                    "outputs.{id} is not produced by any stage"
                )));
            }
            let object = entry
                .as_object()
                .ok_or_else(|| invalid(format!("outputs.{id} must be an object")))?;
            if object
                .keys()
                .any(|k| !["kind", "path", "template"].contains(&k.as_str()))
            {
                return Err(invalid(format!("outputs.{id} has unknown keys")));
            }
            if let Some(kind) = entry.get("kind") {
                if !["document", "code-manifest", "data"].contains(&kind.as_str().unwrap_or("")) {
                    return Err(invalid(format!(
                        "outputs.{id} kind must be document, code-manifest, data"
                    )));
                }
            }
            if entry["kind"] == "data" && entry.get("path").is_none() {
                return Err(invalid(format!(
                    "outputs.{id} of kind data must declare a .json path"
                )));
            }
            if let Some(template) = entry.get("template") {
                validate_asset(root, template.as_str(), &format!("outputs.{id} template"))?;
            }
            if let Some(path) = entry.get("path") {
                let ext = if entry["kind"] == "data" {
                    ".json"
                } else {
                    ".md"
                };
                let p = path.as_str().unwrap_or("");
                let allowed_placeholders =
                    ["{intentId}", "{phase}", "{stage}", "{output}", "{unitId}"];
                let mut stripped = p.to_owned();
                for placeholder in allowed_placeholders {
                    stripped = stripped.replace(placeholder, "");
                }
                if !valid_relative_template_path(p) || !p.ends_with(ext) || stripped.contains('{') {
                    return Err(invalid(format!(
                        "outputs.{id} path is invalid or has invalid placeholders"
                    )));
                }
            }
        }
    }
    let depths = g["depths"]
        .as_object()
        .ok_or_else(|| invalid("depths must be an object"))?;
    if depths.contains_key("full") {
        return Err(invalid("full must not be declared"));
    }
    let rc = phases
        .iter()
        .find(|p| string(p, "gate") == Some("release-candidate"))
        .and_then(|p| p["stages"].as_array())
        .and_then(|a| a.last())
        .and_then(Value::as_str);
    let archived = phases
        .iter()
        .find(|p| string(p, "id") == Some("archived"))
        .and_then(|p| p["stages"].as_array())
        .and_then(|a| a.last())
        .and_then(Value::as_str);
    for (name, d) in depths {
        let list = d["stages"]
            .as_array()
            .ok_or_else(|| invalid(format!("depth {name} must declare a stages array")))?;
        for id in list {
            if !id.as_str().is_some_and(|s| sids.contains(s)) {
                return Err(invalid(format!("depth {name} names unknown stage {}", id)));
            }
        }
        if rc.is_some_and(|id| !list.iter().any(|v| v.as_str() == Some(id)))
            || archived.is_some_and(|id| !list.iter().any(|v| v.as_str() == Some(id)))
        {
            return Err(invalid(format!(
                "depth {name} removes a required gate stage"
            )));
        }
        if let Some(subsets) = d.get("produces") {
            if !subsets.is_object() {
                return Err(invalid(format!("depth {name} produces must be an object")));
            }
            for (sid, outs) in subsets.as_object().unwrap() {
                let stage = stages
                    .iter()
                    .find(|s| string(s, "id") == Some(sid.as_str()))
                    .ok_or_else(|| {
                        invalid(format!("depth {name} produces names unknown stage {sid}"))
                    })?;
                if !outs.as_array().is_some_and(|xs| {
                    xs.iter()
                        .all(|x| stage["produces"].as_array().is_some_and(|p| p.contains(x)))
                }) {
                    return Err(invalid(format!("depth {name} produces subset for {sid} is not a subset of that stage's produces")));
                }
            }
        }
    }
    Ok(())
}
fn valid_relative_template_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && !path.contains('\\')
        && !path.split('/').any(|part| part == "..")
}
fn valid_output(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn validate_config(c: &Value, g: &Value) -> Result<(), String> {
    if let Some(mode) = c.get("mode") {
        if !matches!(mode.as_str(), Some("human" | "ai")) {
            return Err("mode must be human or ai".into());
        }
    }
    if let Some(models) = c.get("agentModels") {
        let m = models
            .as_object()
            .ok_or("Invalid agentModels: expected an object mapping agent IDs to model aliases")?;
        let ids: HashSet<_> = g["agents"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| string(a, "id"))
            .collect();
        for (id, v) in m {
            if !ids.contains(id.as_str()) {
                return Err(format!("Unknown agent in agentModels: {id}"));
            }
            if !v.as_str().is_some_and(|x| MODEL_ALIASES.contains(&x)) {
                return Err(format!(
                    "Invalid model alias for {id}: {v}; choose opus, sonnet, haiku, or fable"
                ));
            }
        }
    }
    if let Some(gates) = c.get("gateModes") {
        let gm = gates
            .as_object()
            .ok_or("Invalid gateModes: expected an object mapping gate ids to human")?;
        let known: HashSet<_> = g["phases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| string(p, "gate"))
            .collect();
        for (gate, v) in gm {
            if !known.is_empty() && !known.contains(gate.as_str()) {
                return Err(format!("Unknown gate in gateModes: {gate}"));
            }
            if v.as_str() != Some("human") {
                return Err(format!(
                    "Invalid gateModes.{gate}: a gate can only be pinned to human"
                ));
            }
        }
    }
    if let Some(prerequisites) = c.get("prerequisites") {
        let entries = prerequisites
            .as_object()
            .ok_or("Invalid prerequisites: expected an object mapping names to { check }")?;
        for (name, definition) in entries {
            crate::prerequisites::validate_prerequisite(name, definition, Some(g))?;
        }
    }
    if let Some(n) = c.get("unitSplitThreshold") {
        if !integer_valued_number(n) || n.as_f64().is_none_or(|x| x < 1.0) {
            return Err("Invalid unitSplitThreshold: expected a positive integer".into());
        }
    }
    if let Some(roles) = c.get("roles") {
        if !roles.is_object() {
            return Err("Invalid roles: expected an object".into());
        }
        if let Some(owner) = roles.get("owner") {
            if !owner
                .as_array()
                .is_some_and(|xs| xs.iter().all(Value::is_string))
            {
                return Err("Invalid roles.owner: expected an array of names".into());
            }
        }
        if let Some(approvers) = roles.get("approvers") {
            if !approvers.is_object()
                || approvers.as_object().unwrap().values().any(|v| {
                    !v.as_array()
                        .is_some_and(|xs| xs.iter().all(Value::is_string))
                })
            {
                return Err("Invalid roles.approvers".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "aidlc-scope-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(p.join("core/data")).unwrap();
        p
    }
    #[test]
    fn resolves_depth_and_error_lists_known_names() {
        let g = Graph {
            raw: json!({"depths":{"fast":{"stages":[]},"standard":{"stages":[]}}}),
        };
        assert_eq!(g.resolve_depth(None).unwrap(), "full");
        assert!(g
            .resolve_depth(Some("bad"))
            .unwrap_err()
            .contains("full, fast, standard"));
    }
    #[test]
    fn sort_filter_and_depth_output_match_scope_contract() {
        let g = Graph {
            raw: json!({"phases":[{"id":"p2","order":2},{"id":"p1","order":1}],"stages":[{"id":"b","phase":"p2","order":1},{"id":"a2","phase":"p1","order":2},{"id":"a1","phase":"p1","order":1}],"scopes":{"solo":["p1","p2"]},"depths":{"fast":{"stages":["a1","b"]}}}),
        };
        assert_eq!(
            g.ordered_stages("solo", "full")
                .iter()
                .map(|s| string(s, "id").unwrap())
                .collect::<Vec<_>>(),
            vec!["a1", "a2", "b"]
        );
        assert_eq!(
            g.ordered_stages("solo", "fast")
                .iter()
                .map(|s| string(s, "id").unwrap())
                .collect::<Vec<_>>(),
            vec!["a1", "b"]
        );
    }
    #[test]
    fn ordered_stages_accepts_integer_valued_float_orders() {
        let g = Graph {
            raw: json!({"phases":[{"id":"p2","order":2.0},{"id":"p1","order":1.0}],"stages":[{"id":"late","phase":"p1","order":2.0},{"id":"early","phase":"p1","order":1.0},{"id":"other","phase":"p2","order":1.0}],"scopes":{"solo":["p1","p2"]},"depths":{}}),
        };
        assert_eq!(
            g.ordered_stages("solo", "full")
                .iter()
                .map(|s| string(s, "id").unwrap())
                .collect::<Vec<_>>(),
            vec!["early", "late", "other"]
        );
    }
    #[test]
    fn frontmatter_requires_a_closed_delimiter_and_parses_only_scalar_fields() {
        let root = temp();
        let path = root.join("frontmatter.md");
        fs::write(&path, "---\nid: test\nwith space: ignored\n---\nbody\n").unwrap();
        let (fields, body) = frontmatter(&path, "test").unwrap();
        assert_eq!(fields.get("id").map(String::as_str), Some("test"));
        assert!(!fields.contains_key("with space"));
        assert_eq!(body, "id: test\nwith space: ignored");
        fs::write(&path, "---\nid: test\n---not-a-delimiter\n").unwrap();
        assert!(frontmatter(&path, "test").is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_config_defaults_and_symlink_config_rejected() {
        let root = temp();
        assert_eq!(load_config(&root).unwrap(), json!({}));
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let outside = root.join("other.json");
        fs::write(&outside, "{}").unwrap();
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(&outside, root.join(".aidlc/config.json"));
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_file(&outside, root.join(".aidlc/config.json"));
        if let Err(error) = link {
            if matches!(error.kind(), std::io::ErrorKind::PermissionDenied)
                || error.raw_os_error() == Some(1314)
            {
                let _ = fs::remove_dir_all(root);
                return;
            }
            panic!("cannot create test symlink: {error}");
        }
        assert!(load_config(&root)
            .unwrap_err()
            .contains("symlink config.json"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn invalid_config_model_and_scope_are_rejected() {
        let g = json!({"agents":[{"id":"a"}],"phases":[{"gate":"g"}]});
        assert!(validate_config(&json!({"agentModels":{"missing":"opus"}}), &g).is_err());
        assert!(validate_config(&json!({"gateModes":{"g":"ai"}}), &g).is_err());
        assert!(validate_config(&json!({"unitSplitThreshold":1.0}), &g).is_ok());
        assert!(validate_config(&json!({"unitSplitThreshold":1.5}), &g).is_err());
        assert!(validate_config(&json!({"gateModes":{"unknown":"human"}}), &g).is_err());
        assert!(
            validate_config(&json!({"gateModes":{"g":"human"}}), &json!({"phases":[]})).is_ok()
        );
    }
}
