//! Artifact path resolution and verification, ported from `core/tools/artifacts.mjs`.
//!
//! This module is standalone and is not yet wired into the CLI. Graph and state are
//! represented as JSON values to keep the migration boundary independent of workflow types.

use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const DEFAULT: &str = ".aidlc/intents/{intentId}/{phase}/{stage}/{output}.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedArtifact {
    pub output: String,
    pub kind: String,
    /// Absolute, lexically contained path under `root`.
    pub path: PathBuf,
    /// Project-relative path using forward slashes.
    pub rel_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProblem {
    pub output: String,
    pub path: String,
    pub problem: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArtifact {
    pub output: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub not_applicable: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub ok: bool,
    pub artifacts: Vec<VerifiedArtifact>,
    pub problems: Vec<ArtifactProblem>,
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}
fn stage_produces(graph: &Value, stage: &Value, state: &Value) -> Vec<String> {
    let all = stage.get("produces").and_then(Value::as_array);
    let depth = state.get("depth").and_then(Value::as_str).unwrap_or("full");
    let subset = graph
        .get("depths")
        .and_then(|d| d.get(depth))
        .and_then(|d| d.get("produces"))
        .and_then(|p| str_field(stage, "id").and_then(|id| p.get(id)))
        .and_then(Value::as_array);
    subset
        .or(all)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn fill(template: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(template.to_owned(), |s, (key, value)| {
        s.replace(&format!("{{{key}}}"), value)
    })
}
fn safe_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let relative = Path::new(rel);
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("artifact path escapes project root".into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("project root is unavailable: {e}"))?;
    let path = root.join(relative);
    if !path.starts_with(&root) {
        return Err("artifact path escapes project root".into());
    }
    Ok(path)
}

/// Resolve the output files a stage is expected to produce, respecting depth and work units.
pub fn expected_artifacts(
    root: impl AsRef<Path>,
    graph: &Value,
    state: &Value,
    stage: &Value,
) -> Result<Vec<ExpectedArtifact>, String> {
    let root = root.as_ref();
    let intent_id = str_field(state, "intentId").unwrap_or("");
    let phase = str_field(stage, "phase").unwrap_or("");
    let stage_id = str_field(stage, "id").unwrap_or("");
    let unit_id = state
        .get("currentUnit")
        .and_then(Value::as_str)
        .unwrap_or("");
    stage_produces(graph, stage, state)
        .into_iter()
        .map(|output| {
            let meta = graph.get("outputs").and_then(|v| v.get(&output));
            let kind = str_field(meta.unwrap_or(&Value::Null), "kind")
                .unwrap_or("document")
                .to_owned();
            let configured = str_field(meta.unwrap_or(&Value::Null), "path")
                .filter(|path| !path.is_empty())
                .unwrap_or(DEFAULT);
            let template = if configured.contains("{unitId}") || unit_id.is_empty() {
                configured
            } else {
                ".aidlc/intents/{intentId}/{phase}/{stage}/{unitId}/{output}.md"
            };
            let path_template = template.replace(
                "/{unitId}",
                if unit_id.is_empty() { "" } else { "/{unitId}" },
            );
            let rel_path = fill(
                &path_template,
                &[
                    ("intentId", intent_id),
                    ("phase", phase),
                    ("stage", stage_id),
                    ("output", &output),
                    ("unitId", unit_id),
                ],
            );
            let path = safe_path(root, &rel_path)?;
            Ok(ExpectedArtifact {
                output,
                kind,
                path,
                rel_path,
            })
        })
        .collect()
}

/// Return the configured artifact path for an output, or `None` if no stage produces it.
pub fn artifact_path_for(
    root: impl AsRef<Path>,
    graph: &Value,
    intent_id: &str,
    output_id: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(stage) = graph
        .get("stages")
        .and_then(Value::as_array)
        .and_then(|stages| {
            stages.iter().find(|s| {
                s.get("produces")
                    .and_then(Value::as_array)
                    .is_some_and(|xs| xs.iter().any(|x| x.as_str() == Some(output_id)))
            })
        })
    else {
        return Ok(None);
    };
    let state = json!({"intentId": intent_id});
    Ok(expected_artifacts(root, graph, &state, stage)?
        .into_iter()
        .find(|a| a.output == output_id)
        .map(|a| a.path))
}

fn file_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
fn not_applicable_reason(text: &str) -> (bool, Option<String>) {
    let head: String = text.chars().take(2000).collect();
    let mut na = false;
    let mut reason = None;
    for line in head.lines() {
        let trimmed = line.trim();
        if let Some((label, value)) = trimmed.split_once(':') {
            if label.trim().eq_ignore_ascii_case("Status")
                && value.trim().eq_ignore_ascii_case("not-applicable")
            {
                na = true;
            }
            if label.trim().eq_ignore_ascii_case("Reason") && !value.trim().is_empty() {
                reason = Some(value.trim().to_owned());
            }
        }
    }
    (na, reason)
}

/// Verify existence, regular-file/path safety, nonempty/NA rules and built-in JSON validators.
/// The built-in data validators call `crate::ids::{work_unit_problems, registry_problems}`.
pub fn verify_artifacts(
    root: impl AsRef<Path>,
    expected: &[ExpectedArtifact],
    graph: Option<&Value>,
    state: Option<&Value>,
) -> Verification {
    let root = root.as_ref();
    let root_real = match root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Verification {
                ok: false,
                artifacts: vec![],
                problems: vec![ArtifactProblem {
                    output: "".into(),
                    path: "".into(),
                    problem: e.to_string(),
                }],
            }
        }
    };
    let mut artifacts = Vec::new();
    let mut problems = Vec::new();
    for item in expected {
        let fail = |problem: String, problems: &mut Vec<ArtifactProblem>| {
            problems.push(ArtifactProblem {
                output: item.output.clone(),
                path: item.rel_path.clone(),
                problem,
            })
        };
        let metadata = match fs::symlink_metadata(&item.path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fail("missing".into(), &mut problems);
                continue;
            }
            Err(e) => {
                fail(e.to_string(), &mut problems);
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            fail("symbolic link".into(), &mut problems);
            continue;
        }
        if !metadata.is_file() {
            fail("not a regular file".into(), &mut problems);
            continue;
        }
        let real = match item.path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                fail(e.to_string(), &mut problems);
                continue;
            }
        };
        if !real.starts_with(&root_real) || real == root_real {
            fail("outside the project".into(), &mut problems);
            continue;
        }
        let bytes = match fs::read(&item.path) {
            Ok(b) => b,
            Err(e) => {
                fail(e.to_string(), &mut problems);
                continue;
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        let (na, reason) = not_applicable_reason(&text);
        if na && reason.is_none() {
            fail(
                "not-applicable output needs a 'Reason:' line".into(),
                &mut problems,
            );
            continue;
        }
        if !na && text.trim().is_empty() {
            fail("empty".into(), &mut problems);
            continue;
        }
        let mut extra = Vec::new();
        if item.output == "work-units-json" {
            let problems = crate::ids::work_unit_problems(&text);
            if problems.is_empty() {
                if let (Some(graph), Some(state)) = (graph, state) {
                    let registry = artifact_path_for(
                        root,
                        graph,
                        str_field(state, "intentId").unwrap_or(""),
                        "id-registry",
                    )
                    .ok()
                    .flatten()
                    .filter(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file()))
                    .and_then(|path| fs::read_to_string(path).ok());
                    if let Some(registry) = registry {
                        extra.extend(crate::ids::work_unit_test_problems(&text, &registry));
                    }
                }
            }
            extra.extend(problems);
        }
        if item.output == "id-registry" {
            let units = if let (Some(graph), Some(state)) = (graph, state) {
                artifact_path_for(
                    root,
                    graph,
                    str_field(state, "intentId").unwrap_or(""),
                    "work-units-json",
                )
                .ok()
                .flatten()
                .and_then(|p| fs::read_to_string(p).ok())
                .map(|t| crate::ids::work_unit_problems(&t))
                .filter(|p| p.is_empty())
                .and_then(|_| {
                    artifact_path_for(
                        root,
                        graph,
                        str_field(state, "intentId").unwrap_or(""),
                        "work-units-json",
                    )
                    .ok()
                    .flatten()
                })
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| {
                    v.get("units")
                        .and_then(Value::as_array)
                        .cloned()
                        .or_else(|| v.as_array().cloned())
                })
                .map(|rows| {
                    rows.iter()
                        .filter_map(|r| r.get("id").and_then(Value::as_str))
                        .map(crate::ids::normalize_id)
                        .collect::<HashSet<_>>()
                })
            } else {
                None
            };
            extra.extend(crate::ids::registry_problems(&text, units.as_ref()));
        }
        for problem in extra {
            fail(problem, &mut problems);
        }
        artifacts.push(VerifiedArtifact {
            output: item.output.clone(),
            path: item.rel_path.clone(),
            bytes: metadata.len(),
            sha256: file_hash(text.as_bytes()),
            not_applicable: na,
            reason,
        });
    }
    Verification {
        ok: problems.is_empty(),
        artifacts,
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "aidlc-artifacts-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn resolves_standard_and_unit_paths_and_depth_subset() {
        let root = temp();
        let graph = json!({"outputs":{"result":{"kind":"document"}},"depths":{"quick":{"produces":{"build":["result"]}}}});
        let stage = json!({"id":"build","phase":"develop","produces":["result","other"]});
        let got = expected_artifacts(
            &root,
            &graph,
            &json!({"intentId":"i-1","depth":"quick","currentUnit":"WU-02"}),
            &stage,
        )
        .unwrap();
        assert_eq!(
            got[0].rel_path,
            ".aidlc/intents/i-1/develop/build/WU-02/result.md"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn verifies_empty_missing_and_not_applicable_reason() {
        let root = temp();
        let expected = |name: &str| ExpectedArtifact {
            output: name.into(),
            kind: "document".into(),
            path: root.join(name),
            rel_path: name.into(),
        };
        fs::write(root.join("empty"), "  \n").unwrap();
        fs::write(
            root.join("na"),
            "Status: not-applicable\nReason: outside scope\n",
        )
        .unwrap();
        let result = verify_artifacts(
            &root,
            &[expected("missing"), expected("empty"), expected("na")],
            None,
            None,
        );
        assert!(!result.ok);
        assert!(result.problems.iter().any(|p| p.problem == "missing"));
        assert!(result.problems.iter().any(|p| p.problem == "empty"));
        assert_eq!(result.artifacts[0].reason.as_deref(), Some("outside scope"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn validates_work_units_and_registry_json() {
        let root = temp();
        let units = ExpectedArtifact {
            output: "work-units-json".into(),
            kind: "data".into(),
            path: root.join("units"),
            rel_path: "units".into(),
        };
        let registry = ExpectedArtifact {
            output: "id-registry".into(),
            kind: "data".into(),
            path: root.join("registry"),
            rel_path: "registry".into(),
        };
        fs::write(&units.path, r#"{"units":[{"id":"WU-01"}]}"#).unwrap();
        fs::write(
            &registry.path,
            r#"{"schemaVersion":1,"ids":{"FN":[{"id":"FN-01","traces":["FN-99"]}]}}"#,
        )
        .unwrap();
        let result = verify_artifacts(&root, &[units, registry], None, None);
        assert!(!result.ok);
        assert!(result
            .problems
            .iter()
            .any(|p| p.problem.contains("traces to unregistered FN-99")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_symlink_and_escaping_expected_path() {
        let root = temp();
        let outside = root
            .parent()
            .unwrap()
            .join(format!("aidlc-out-{}", std::process::id()));
        fs::write(&outside, "outside").unwrap();
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(&outside, root.join("link"));
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_file(&outside, root.join("link"));
        if link.is_ok() {
            let item = ExpectedArtifact {
                output: "x".into(),
                kind: "document".into(),
                path: root.join("link"),
                rel_path: "link".into(),
            };
            assert_eq!(
                verify_artifacts(&root, &[item], None, None).problems[0].problem,
                "symbolic link"
            );
        }
        assert!(safe_path(&root, "../outside").is_err());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }
}
