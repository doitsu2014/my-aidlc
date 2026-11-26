use crate::args::FlagValue;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const REGISTRY_PREFIXES: &[&str] = &["FN", "IF", "TS", "WU", "CFG"];
pub const ANALYZE_PREFIXES: &[&str] = &["AC", "US"];
pub const CITATION_PHASES: &[&str] = &["develop", "launch"];

fn is_digits(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit())
}

fn split_id(id: &str) -> Option<(&str, &str)> {
    let (prefix, number) = id.split_once('-')?;
    if number.contains('-') {
        return None;
    }
    Some((prefix, number))
}

fn valid_id(id: &str, prefixes: &[&str], max_digits: usize) -> bool {
    split_id(id).is_some_and(|(prefix, number)| {
        prefixes.contains(&prefix) && is_digits(number, 1, max_digits)
    })
}

/// Legacy work units named U-01 normalize to WU-01.
pub fn normalize_id(id: &str) -> String {
    if let Some(number) = id.strip_prefix("U-") {
        if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
            return format!("WU-{number}");
        }
    }
    id.to_owned()
}

pub fn prefix_of(id: &str) -> Option<&str> {
    let prefix = id.split_once('-').map(|(prefix, _)| prefix)?;
    Some(if prefix == "U" { "WU" } else { prefix })
}

/// Finds registry and Analyze identifiers in text, normalizing legacy U-n identifiers.
pub fn cited_ids(text: &str) -> HashSet<String> {
    cited_ids_ordered(text).into_iter().collect()
}

/// Node `citedIds(text)` as an insertion-ordered set: each normalized identifier once,
/// in the order of its first citation (`/\b(FN|IF|TS|WU|U|CFG|AC|US)-(\d{1,5})\b/g`).
pub fn cited_ids_ordered(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_uppercase() || !is_boundary_before(bytes, i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_uppercase() {
            i += 1;
        }
        // Only ASCII bytes were consumed, so these slices are valid UTF-8.
        let prefix = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
        if !["FN", "IF", "TS", "WU", "U", "CFG", "AC", "US"].contains(&prefix)
            || bytes.get(i) != Some(&b'-')
        {
            i = start + 1;
            continue;
        }
        i += 1;
        let number_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let digits = i - number_start;
        if digits == 0 || digits > 5 || !is_boundary_after(bytes, i) {
            continue;
        }
        let id = normalize_id(std::str::from_utf8(&bytes[start..i]).unwrap_or(""));
        if !found.contains(&id) {
            found.push(id);
        }
    }
    found
}

fn is_boundary_before(bytes: &[u8], i: usize) -> bool {
    i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')
}
fn is_boundary_after(bytes: &[u8], i: usize) -> bool {
    i == bytes.len() || !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
}

fn not_applicable(data: &Value) -> bool {
    data.is_object() && data.get("status").and_then(Value::as_str) == Some("not-applicable")
}

fn valid_reason(data: &Value) -> bool {
    data.get("reason")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}

/// Validates work-units.json text; an empty vector means it is valid.
pub fn work_unit_problems(text: &str) -> Vec<String> {
    let data: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return vec!["work-units.json is not valid JSON".into()],
    };
    if not_applicable(&data) {
        return if valid_reason(&data) {
            vec![]
        } else {
            vec!["a not-applicable work-units.json needs a non-empty 'reason'".into()]
        };
    }
    let units = data
        .get("units")
        .and_then(Value::as_array)
        .or_else(|| data.as_array());
    let Some(units) = units else {
        return vec!["work-units.json must list at least one unit under \"units\"".into()];
    };
    if units.is_empty() {
        return vec!["work-units.json must list at least one unit under \"units\"".into()];
    }

    let mut problems = Vec::new();
    let mut ids: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for (index, row) in units.iter().enumerate() {
        if !row.is_object() {
            problems.push(format!(
                "work-units.json unit {} must be an object",
                index + 1
            ));
            continue;
        }
        let Some(raw_id) = row.get("id").and_then(Value::as_str) else {
            problems.push(format!(
                "work-units.json unit {} needs an id like WU-01 (got {})",
                index + 1,
                row.get("id").unwrap_or(&Value::Null)
            ));
            continue;
        };
        if !valid_id(raw_id, &["WU", "U"], 5) {
            problems.push(format!(
                "work-units.json unit {} needs an id like WU-01 (got {})",
                index + 1,
                row.get("id").unwrap_or(&Value::Null)
            ));
            continue;
        }
        let id = normalize_id(raw_id);
        if ids.contains_key(&id) {
            problems.push(format!("work-units.json lists {raw_id} more than once"));
        }
        let mut depends = Vec::new();
        if let Some(value) = row.get("dependsOn") {
            if let Some(array) = value.as_array() {
                if array.iter().any(|v| !v.is_string()) {
                    problems.push(format!(
                        "work-units.json {raw_id} dependsOn must be an array of unit ids"
                    ));
                } else {
                    depends = array
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                }
            } else {
                problems.push(format!(
                    "work-units.json {raw_id} dependsOn must be an array of unit ids"
                ));
            }
        }
        if let Some(value) = row.get("criteria") {
            if !value.as_array().is_some_and(|a| {
                a.iter().all(|v| {
                    v.as_str().is_some_and(|s| {
                        s.strip_prefix("AC-")
                            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                    })
                })
            }) {
                problems.push(format!(
                    "work-units.json {raw_id} criteria must be an array of AC- identifiers"
                ));
            }
        }
        ids.insert(id, (raw_id.to_owned(), depends));
    }
    if !problems.is_empty() {
        return problems;
    }
    for (id, (raw, deps)) in &ids {
        for dep in deps {
            let target = normalize_id(dep);
            if target == *id {
                problems.push(format!("work-units.json {raw} depends on itself"));
            } else if !ids.contains_key(&target) {
                problems.push(format!(
                    "work-units.json {raw} depends on unknown unit {dep}"
                ));
            }
        }
    }
    if !problems.is_empty() {
        return problems;
    }
    let mut state: HashMap<String, u8> = HashMap::new();
    fn visit(
        id: &str,
        ids: &HashMap<String, (String, Vec<String>)>,
        state: &mut HashMap<String, u8>,
        trail: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        match state.get(id) {
            Some(2) => return None,
            Some(1) => {
                let pos = trail.iter().position(|v| v == id).unwrap_or(0);
                let mut cycle = trail[pos..].to_vec();
                cycle.push(id.to_owned());
                return Some(cycle);
            }
            _ => {}
        }
        state.insert(id.to_owned(), 1);
        trail.push(id.to_owned());
        for dep in &ids[id].1 {
            if let Some(cycle) = visit(&normalize_id(dep), ids, state, trail) {
                return Some(cycle);
            }
        }
        trail.pop();
        state.insert(id.to_owned(), 2);
        None
    }
    for id in ids.keys() {
        if let Some(cycle) = visit(id, &ids, &mut state, &mut Vec::new()) {
            return vec![format!(
                "work-units.json dependsOn forms a cycle: {}",
                cycle.join(" -> ")
            )];
        }
    }
    problems
}

/// Validates ids.json text, optionally matching WU entries against normalized work-unit IDs.
pub fn registry_problems(text: &str, unit_ids: Option<&HashSet<String>>) -> Vec<String> {
    let data: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return vec!["ids.json is not valid JSON".into()],
    };
    if not_applicable(&data) {
        return if valid_reason(&data) {
            vec![]
        } else {
            vec!["a not-applicable ids.json needs a non-empty 'reason'".into()]
        };
    }
    if !data.is_object() {
        return vec!["ids.json must be an object".into()];
    }
    if data.get("schemaVersion") != Some(&Value::from(1)) {
        return vec!["ids.json needs \"schemaVersion\": 1".into()];
    }
    let Some(registry) = data.get("ids").and_then(Value::as_object) else {
        return vec!["ids.json needs an \"ids\" object keyed by prefix".into()];
    };
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    let mut traces = Vec::new();
    for (prefix, entries) in registry {
        if !REGISTRY_PREFIXES.contains(&prefix.as_str()) {
            problems.push(format!(
                "ids.json has unknown prefix {prefix}; use {}",
                REGISTRY_PREFIXES.join(", ")
            ));
            continue;
        }
        let Some(entries) = entries.as_array() else {
            problems.push(format!("ids.json ids.{prefix} must be an array"));
            continue;
        };
        for entry in entries {
            let Some(id) = entry.get("id").and_then(Value::as_str) else {
                problems.push(format!(
                    "ids.json ids.{prefix} has an entry without a valid {prefix}- id"
                ));
                continue;
            };
            let valid = if prefix == "WU" {
                valid_id(id, &["WU", "U"], 5)
            } else {
                valid_id(id, &[prefix], 5)
            };
            if !entry.is_object() || !valid {
                problems.push(format!(
                    "ids.json ids.{prefix} has an entry without a valid {prefix}- id"
                ));
                continue;
            }
            let normalized = normalize_id(id);
            if !seen.insert(normalized.clone()) {
                problems.push(format!("ids.json registers {id} more than once"));
            }
            if entry.get("title").is_some_and(|v| !v.is_string()) {
                problems.push(format!("ids.json {id} title must be a string"));
            }
            if let Some(value) = entry.get("traces") {
                let Some(items) = value.as_array() else {
                    problems.push(format!(
                        "ids.json {id} traces must be an array of identifiers"
                    ));
                    continue;
                };
                if items.iter().any(|v| {
                    !v.as_str().is_some_and(|s| {
                        valid_id(s, &["FN", "IF", "TS", "WU", "U", "CFG", "AC", "US"], 5)
                    })
                }) {
                    problems.push(format!(
                        "ids.json {id} traces must be an array of identifiers"
                    ));
                } else {
                    for target in items.iter().filter_map(Value::as_str) {
                        traces.push((id.to_owned(), normalize_id(target)));
                    }
                }
            }
        }
    }
    for (from, target) in traces {
        if prefix_of(&target).is_some_and(|p| REGISTRY_PREFIXES.contains(&p))
            && !seen.contains(&target)
            && !(prefix_of(&target) == Some("WU")
                && unit_ids.is_some_and(|units| units.contains(&target)))
        {
            problems.push(format!("ids.json {from} traces to unregistered {target}"));
        }
    }
    let registered_units: Vec<_> = seen
        .iter()
        .filter(|id| prefix_of(id) == Some("WU"))
        .cloned()
        .collect();
    if let Some(units) = unit_ids {
        if !registered_units.is_empty() {
            let mut missing: Vec<_> = units
                .iter()
                .filter(|id| !seen.contains(*id))
                .cloned()
                .collect();
            let mut extra: Vec<_> = registered_units
                .into_iter()
                .filter(|id| !units.contains(id))
                .collect();
            missing.sort();
            extra.sort();
            if !missing.is_empty() || !extra.is_empty() {
                let mut message = "ids.json WU list must match work-units.json".to_owned();
                if !missing.is_empty() {
                    message.push_str(&format!("; missing {}", missing.join(", ")));
                }
                if !extra.is_empty() {
                    message.push_str(&format!("; not in work-units.json {}", extra.join(", ")));
                }
                problems.push(message);
            }
        }
    }
    problems
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnregisteredCitation {
    pub id: String,
    pub file: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationReport {
    pub applies: bool,
    pub registry: Option<String>,
    pub reason: Option<String>,
    pub checked: Vec<String>,
    pub unregistered: Vec<UnregisteredCitation>,
}

/// Scans supplied Markdown file contents. Paths are caller-provided and expected to be relative.
/// AC/US definitions are optional: absent definitions are not checked, matching the Node behavior.
pub fn citation_report(
    registry_path: Option<&str>,
    registry_text: Option<&str>,
    unit_ids: Option<&HashSet<String>>,
    analyze_definitions: &HashMap<String, Option<HashSet<String>>>,
    files: &[(String, String)],
) -> CitationReport {
    let Some(text) = registry_text else {
        return CitationReport {
            applies: false,
            registry: registry_path.map(str::to_owned),
            reason: Some("this intent has no ids.json".into()),
            checked: vec![],
            unregistered: vec![],
        };
    };
    let data: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => {
            return CitationReport {
                applies: false,
                registry: registry_path.map(str::to_owned),
                reason: Some("ids.json is not valid JSON".into()),
                checked: vec![],
                unregistered: vec![],
            }
        }
    };
    if not_applicable(&data) {
        return CitationReport {
            applies: false,
            registry: registry_path.map(str::to_owned),
            reason: Some("ids.json is marked not-applicable".into()),
            checked: vec![],
            unregistered: vec![],
        };
    }
    let mut registered = HashSet::new();
    if let Some(ids) = data.get("ids").and_then(Value::as_object) {
        for entries in ids.values().filter_map(Value::as_array) {
            for entry in entries {
                if let Some(id) = entry.get("id").and_then(Value::as_str) {
                    registered.insert(normalize_id(id));
                }
            }
        }
    }
    if let Some(units) = unit_ids {
        registered.extend(units.iter().cloned());
    }
    let mut checked = Vec::new();
    let mut unregistered = Vec::new();
    for (path, content) in files {
        if !path.ends_with(".md") {
            continue;
        }
        checked.push(path.clone());
        for id in cited_ids_ordered(content) {
            let prefix = prefix_of(&id).unwrap_or("");
            if REGISTRY_PREFIXES.contains(&prefix) {
                if !registered.contains(&id) {
                    unregistered.push(UnregisteredCitation {
                        id,
                        file: path.clone(),
                        source: "ids.json".into(),
                    });
                }
            } else if ANALYZE_PREFIXES.contains(&prefix) {
                if let Some(Some(defined)) = analyze_definitions.get(prefix) {
                    if !defined.contains(&id) {
                        let source = if prefix == "AC" {
                            "acceptance-criteria"
                        } else {
                            "user-stories"
                        };
                        unregistered.push(UnregisteredCitation {
                            id,
                            file: path.clone(),
                            source: source.into(),
                        });
                    }
                }
            }
        }
    }
    CitationReport {
        applies: true,
        registry: registry_path.map(str::to_owned),
        reason: None,
        checked,
        unregistered,
    }
}

// ---------------------------------------------------------------------------
// `dtech-aidlc ids check` (Node router plus `loadRegistry`, `analyzeDefinitions`,
// `citationReport`, `markdownFilesUnder` and `uncoveredFunctions` in ids.mjs).
// ---------------------------------------------------------------------------

/// The methodology graph exactly as the CLI selects it: the engine named by
/// `DTECH_AIDLC_HOME` when it holds one (Node `engineRoot()`), otherwise the graph
/// embedded in this executable. Shared by the `ids` and `deps` commands.
#[doc(hidden)]
pub fn load_runtime_graph(project: &Path) -> Result<Value, String> {
    let engine = std::env::var_os("DTECH_AIDLC_HOME")
        .map(PathBuf::from)
        .and_then(|home| {
            [home.join("core"), home].into_iter().find(|candidate| {
                candidate.join("data/phase-graph.json").exists()
                    || candidate.join("methodology").exists()
            })
        });
    let (graph, _) = match engine {
        Some(engine) => crate::scope::load_graph(engine, project)?,
        None => crate::scope::load_embedded_graph(project)?,
    };
    Ok(graph.value().clone())
}

/// Node `readText(path)`: the file as (lossy) UTF-8, or `None` when unreadable.
fn read_text(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Node `relative(root, path).split(sep).join('/')` for a path built under `root`.
fn rel(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `Object.entries(value)` for a JSON object or array, in JavaScript property order
/// (array-index keys first, ascending, then the remaining keys in insertion order).
fn js_entries(value: &Value) -> Vec<(String, &Value)> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (index.to_string(), item))
            .collect(),
        Value::Object(map) => {
            let index_of = |key: &str| -> Option<u32> {
                let n: u32 = key.parse().ok()?;
                (n != u32::MAX && n.to_string() == key).then_some(n)
            };
            let mut indexed: Vec<(u32, String, &Value)> = map
                .iter()
                .filter_map(|(key, item)| index_of(key).map(|n| (n, key.clone(), item)))
                .collect();
            indexed.sort_by_key(|(n, _, _)| *n);
            indexed
                .into_iter()
                .map(|(_, key, item)| (key, item))
                .chain(
                    map.iter()
                        .filter(|(key, _)| index_of(key).is_none())
                        .map(|(key, item)| (key.clone(), item)),
                )
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Node `artifactPathFor(root, graph, intentId, outputId)`, joined onto `root` as
/// given (Node `join(root, ...relPath.split('/'))`), never in the canonical
/// verbatim `\\?\` form that `artifacts::artifact_path_for` returns on Windows.
fn artifact_path(
    root: &Path,
    graph: &Value,
    intent_id: &str,
    output: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(stage) = graph
        .get("stages")
        .and_then(Value::as_array)
        .and_then(|stages| {
            stages.iter().find(|stage| {
                stage
                    .get("produces")
                    .and_then(Value::as_array)
                    .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(output)))
            })
        })
    else {
        return Ok(None);
    };
    let state = serde_json::json!({ "intentId": intent_id });
    Ok(
        crate::artifacts::expected_artifacts(root, graph, &state, stage)?
            .into_iter()
            .find(|item| item.output == output)
            .map(|item| {
                item.rel_path
                    .split('/')
                    .fold(root.to_path_buf(), |path, part| path.join(part))
            }),
    )
}

/// Node `unitsFromData(data)`.
fn units_from_data(data: &Value) -> Vec<&Value> {
    if not_applicable(data) {
        return Vec::new();
    }
    let units = match data.get("units") {
        Some(Value::Array(units)) => Some(units),
        _ => data.as_array(),
    };
    units
        .map(|units| units.iter().collect())
        .unwrap_or_default()
}

/// Node `unitIdsFromFile(path)`.
fn unit_ids_from_file(path: Option<&Path>) -> Option<Vec<String>> {
    let text = read_text(path?)?;
    let data: Value = serde_json::from_str(&text).ok()?;
    Some(
        units_from_data(&data)
            .into_iter()
            .filter_map(|row| row.get("id").and_then(Value::as_str))
            .map(normalize_id)
            .collect(),
    )
}

/// Node `loadRegistry(root, graph, intentId)`.
struct Registry {
    path: Option<String>,
    exists: bool,
    not_applicable: bool,
    ids: HashSet<String>,
    by_prefix: Vec<(String, Vec<String>)>,
    error: Option<String>,
}

impl Registry {
    fn applies(&self) -> bool {
        self.exists && !self.not_applicable && self.error.is_none()
    }
}

fn load_registry(root: &Path, graph: &Value, intent_id: &str) -> Result<Registry, String> {
    let path = artifact_path(root, graph, intent_id, "id-registry")?;
    let units_path = artifact_path(root, graph, intent_id, "work-units-json")?;
    let mut registry = Registry {
        path: path.as_deref().map(|path| rel(root, path)),
        exists: false,
        not_applicable: false,
        ids: HashSet::new(),
        by_prefix: Vec::new(),
        error: None,
    };
    let Some(text) = path.as_deref().and_then(read_text) else {
        return Ok(registry);
    };
    registry.exists = true;
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        registry.error = Some("ids.json is not valid JSON".into());
        return Ok(registry);
    };
    if not_applicable(&data) {
        registry.not_applicable = true;
        return Ok(registry);
    }
    let ids = data.get("ids").cloned().unwrap_or(Value::Null);
    for (prefix, entries) in js_entries(&ids) {
        let Some(entries) = entries.as_array() else {
            continue;
        };
        for entry in entries {
            let Some(id) = entry.get("id").and_then(Value::as_str) else {
                continue;
            };
            let id = normalize_id(id);
            registry.ids.insert(id.clone());
            match registry
                .by_prefix
                .iter_mut()
                .find(|(name, _)| *name == prefix)
            {
                Some((_, list)) => list.push(id),
                None => registry.by_prefix.push((prefix.clone(), vec![id])),
            }
        }
    }
    for id in unit_ids_from_file(units_path.as_deref()).unwrap_or_default() {
        registry.ids.insert(id);
    }
    Ok(registry)
}

/// Node `analyzeDefinitions(root, graph, intentId)`.
fn analyze_definitions(
    root: &Path,
    graph: &Value,
    intent_id: &str,
) -> Result<HashMap<&'static str, Option<HashSet<String>>>, String> {
    let mut defined = HashMap::new();
    for (output, prefix) in [("acceptance-criteria", "AC"), ("user-stories", "US")] {
        let path = artifact_path(root, graph, intent_id, output)?;
        let ids = path.as_deref().and_then(read_text).map(|text| {
            cited_ids_ordered(&text)
                .into_iter()
                .filter(|id| prefix_of(id) == Some(prefix))
                .collect::<HashSet<_>>()
        });
        defined.insert(prefix, ids);
    }
    Ok(defined)
}

/// Node `markdownFilesUnder(dir)`: every regular `.md` file below `dir` (symbolic
/// links are neither followed nor listed), sorted by full path.
pub fn markdown_files_under(dir: &Path) -> Vec<PathBuf> {
    fn walk(current: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = current.join(entry.file_name());
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                walk(&path, files);
            } else if kind.is_file() && entry.file_name().to_string_lossy().ends_with(".md") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    files.sort_by(|a, b| {
        let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
        a.encode_utf16().cmp(b.encode_utf16())
    });
    files
}

/// Node `citationReport(root, graph, intentId, files)` restricted to the fields the
/// `ids check` command prints: `(checked, unregistered)`.
fn stage_citations(
    root: &Path,
    graph: &Value,
    intent_id: &str,
    files: &[PathBuf],
) -> Result<(Vec<String>, Vec<Value>), String> {
    let registry = load_registry(root, graph, intent_id)?;
    if !registry.applies() {
        return Ok((Vec::new(), Vec::new()));
    }
    let defined = analyze_definitions(root, graph, intent_id)?;
    let mut checked = Vec::new();
    let mut unregistered = Vec::new();
    for path in files {
        if !path.to_string_lossy().ends_with(".md") {
            continue;
        }
        let Some(text) = read_text(path) else {
            continue;
        };
        let file = rel(root, path);
        checked.push(file.clone());
        for id in cited_ids_ordered(&text) {
            let prefix = prefix_of(&id).unwrap_or("");
            if REGISTRY_PREFIXES.contains(&prefix) {
                if !registry.ids.contains(&id) {
                    unregistered
                        .push(serde_json::json!({"id": id, "file": file, "source": "ids.json"}));
                }
            } else if let Some(Some(known)) = defined.get(prefix) {
                if !known.contains(&id) {
                    let source = if prefix == "AC" {
                        "acceptance-criteria"
                    } else {
                        "user-stories"
                    };
                    unregistered
                        .push(serde_json::json!({"id": id, "file": file, "source": source}));
                }
            }
        }
    }
    Ok((checked, unregistered))
}

/// Node `uncoveredFunctions(root, graph, intentId, intentDirectory)`.
fn uncovered_functions(
    root: &Path,
    graph: &Value,
    intent_id: &str,
    intent_dir: &Path,
) -> Result<Vec<String>, String> {
    let registry = load_registry(root, graph, intent_id)?;
    if !registry.applies() {
        return Ok(Vec::new());
    }
    let functions = registry
        .by_prefix
        .iter()
        .find(|(prefix, _)| prefix == "FN")
        .map(|(_, ids)| ids.clone())
        .unwrap_or_default();
    if functions.is_empty() {
        return Ok(Vec::new());
    }
    let mut cited = HashSet::new();
    for path in markdown_files_under(&intent_dir.join("develop").join("build-and-verify")) {
        let shown = path.to_string_lossy();
        if shown.ends_with(&format!(
            "{}green-verification-report.md",
            std::path::MAIN_SEPARATOR
        )) || shown.ends_with("/green-verification-report.md")
        {
            cited.extend(cited_ids_ordered(&read_text(&path).unwrap_or_default()));
        }
    }
    Ok(functions
        .into_iter()
        .filter(|id| !cited.contains(id))
        .collect())
}

/// Node `dtech-aidlc ids check [--stage ID]`.
///
/// `key` is `"ids check"` (any other `"ids ..."` key fails with Node's
/// `Use ids check`). Returns the report JSON with exit code 0 when every cited
/// identifier is registered, or the same JSON with exit code 1 when `ok` is false
/// (Node prints the report to stdout and sets `process.exitCode = 1`).
pub fn run(
    key: &str,
    project: &Path,
    _args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let graph = load_runtime_graph(&root)?;
    if key != "ids check" {
        return Err("Use ids check".into());
    }
    let loaded = crate::workflow::load_workflow(&root, &graph, false)?;
    let Some(state) = loaded.state.filter(|state| {
        !matches!(
            state.get("intentId"),
            None | Some(Value::Null) | Some(Value::Bool(false))
        ) && state.get("intentId") != Some(&Value::String(String::new()))
    }) else {
        return Err("No active intent".into());
    };
    let intent_id = state
        .get("intentId")
        .and_then(Value::as_str)
        .ok_or("No active intent")?
        .to_owned();
    let scope = state.get("scope").and_then(Value::as_str).unwrap_or("");
    let in_scope =
        crate::workflow::ordered_stages(&graph, scope, crate::workflow::depth_of(&state));
    let wanted = match flags.get("stage") {
        Some(FlagValue::String(stage)) => Some(stage.as_str()),
        _ => None,
    };
    let stage_id = |stage: &Value| stage.get("id").and_then(Value::as_str).map(str::to_owned);
    if let Some(wanted) = wanted {
        if !in_scope
            .iter()
            .any(|stage| stage_id(stage).as_deref() == Some(wanted))
        {
            return Err(format!(
                "Stage {wanted} is not in the active intent's scope"
            ));
        }
    }
    let selected: Vec<&Value> = in_scope
        .into_iter()
        .filter(|stage| match wanted {
            Some(wanted) => stage_id(stage).as_deref() == Some(wanted),
            None => {
                CITATION_PHASES.contains(&stage.get("phase").and_then(Value::as_str).unwrap_or(""))
            }
        })
        .collect();
    let dir = crate::workflow::intent_dir_path(&root, &intent_id);
    let registry = load_registry(&root, &graph, &intent_id)?;
    let mut stages = Vec::new();
    let mut unregistered_total = 0;
    for stage in selected {
        let phase = stage.get("phase").and_then(Value::as_str).unwrap_or("");
        let id = stage.get("id").and_then(Value::as_str).unwrap_or("");
        let files = markdown_files_under(&dir.join(phase).join(id));
        let (checked, unregistered) = stage_citations(&root, &graph, &intent_id, &files)?;
        unregistered_total += unregistered.len();
        stages.push(
            serde_json::json!({"stage": id, "checked": checked, "unregistered": unregistered}),
        );
    }
    let mut registry_row = serde_json::Map::new();
    registry_row.insert("path".into(), serde_json::json!(registry.path));
    registry_row.insert("exists".into(), Value::Bool(registry.exists));
    if let Some(error) = &registry.error {
        registry_row.insert("error".into(), Value::String(error.clone()));
    }
    let counts: serde_json::Map<String, Value> = registry
        .by_prefix
        .iter()
        .map(|(prefix, ids)| (prefix.clone(), Value::from(ids.len())))
        .collect();
    registry_row.insert("counts".into(), Value::Object(counts));
    let ok = unregistered_total == 0 && registry.error.is_none();
    let result = serde_json::json!({
        "intentId": intent_id,
        "registry": registry_row,
        "applies": registry.applies(),
        "ok": ok,
        "stages": stages,
        "uncoveredFunctions": uncovered_functions(&root, &graph, &intent_id, &dir)?,
    });
    Ok((result, if ok { 0 } else { 1 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cited_ids_keep_first_citation_order_and_survive_multibyte_text() {
        assert_eq!(
            cited_ids_ordered("é FN-02 then U-1, AC-3 and FN-02 again; XFN-9 FN-123456 ÄUS-4"),
            vec!["FN-02", "WU-1", "AC-3", "US-4"]
        );
    }

    #[test]
    fn validates_registry_duplicates_traces_and_work_unit_alignment() {
        let json = r#"{"schemaVersion":1,"ids":{"FN":[{"id":"FN-01","traces":["WU-02"]},{"id":"FN-01"}],"WU":[{"id":"WU-01"}]}}"#;
        let units = ["WU-01".to_string(), "WU-02".to_string()]
            .into_iter()
            .collect();
        let problems = registry_problems(json, Some(&units));
        assert!(problems
            .iter()
            .any(|p| p.contains("registers FN-01 more than once")));
        assert!(!problems
            .iter()
            .any(|p| p.contains("traces to unregistered WU-02")));
        assert!(registry_problems(json, None)
            .iter()
            .any(|p| p.contains("traces to unregistered WU-02")));
        assert!(problems.iter().any(|p| p.contains("missing WU-02")));
    }

    #[test]
    fn validates_work_unit_cycles_and_not_applicable_reason() {
        let cycle = r#"{"units":[{"id":"WU-01","dependsOn":["WU-02"]},{"id":"WU-02","dependsOn":["WU-01"]}]}"#;
        assert!(work_unit_problems(cycle)[0].contains("forms a cycle"));
        assert!(
            !work_unit_problems(r#"{"status":"not-applicable","reason":"none"}"#)
                .len()
                .gt(&0)
        );
        assert!(!work_unit_problems(r#"{"status":"not-applicable","reason":"  "}"#).is_empty());
    }

    #[test]
    fn scans_citations_normalizing_legacy_units_and_checks_markdown_only() {
        let json = r#"{"schemaVersion":1,"ids":{"WU":[{"id":"WU-01"}],"FN":[{"id":"FN-01"}]}}"#;
        let analyze = HashMap::from([
            ("AC".into(), Some(["AC-01".into()].into_iter().collect())),
            ("US".into(), Some(HashSet::new())),
        ]);
        let files = vec![
            (
                "notes.md".into(),
                "FN-01 cites U-01 and AC-02, US-03".into(),
            ),
            ("ignored.txt".into(), "FN-99".into()),
        ];
        let report = citation_report(Some("intent/ids.json"), Some(json), None, &analyze, &files);
        assert!(report.applies);
        assert_eq!(report.checked, vec!["notes.md"]);
        assert_eq!(report.unregistered.len(), 2);
        assert!(report
            .unregistered
            .iter()
            .any(|row| row.id == "AC-02" && row.source == "acceptance-criteria"));
        assert!(report
            .unregistered
            .iter()
            .any(|row| row.id == "US-03" && row.source == "user-stories"));
        assert!(!report
            .unregistered
            .iter()
            .any(|row| row.id == "U-01" || row.id == "FN-99"));
    }
}
