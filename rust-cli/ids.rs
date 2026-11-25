use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
    let bytes = text.as_bytes();
    let mut found = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if !is_boundary_before(bytes, i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_uppercase() {
            i += 1;
        }
        let prefix = &text[start..i];
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
        found.insert(normalize_id(&text[start..i]));
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
        for id in cited_ids(content) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
