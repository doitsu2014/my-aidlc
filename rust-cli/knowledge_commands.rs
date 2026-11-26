//! Dispatch helpers for the `knowledge` CLI commands.
//!
//! The caller owns command-line parsing and output formatting. Unsupported
//! subcommands return `Ok(None)` without touching project state; recognized
//! commands return their JSON-compatible result or the Node CLI-compatible
//! usage/error text.
use crate::args::FlagValue;
use crate::knowledge;
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Node `dtech-aidlc knowledge ...` (router lines in `core/tools/dtech-aidlc.mjs`).
///
/// `key` is the command key from [`crate::args::command_key`] (`"knowledge init"`,
/// `"knowledge sources"`, `"knowledge list"`, `"knowledge show"`, `"knowledge check"`,
/// `"knowledge index"`, or any other `"knowledge ..."` key, which fails like Node).
/// `args` are the positional words after the key, so for
/// `knowledge sources add readme README.md` they are `["add", "readme", "README.md"]`.
/// Every successful result is JSON for the router to print with exit code 0;
/// errors carry Node's message text. The router has already applied the
/// option and positional-count checks (`command_flags` / `command_max_args`).
pub fn run(
    key: &str,
    project: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    // Node: `resolve(flags.project || cwd)` then `assertSafeStateDir(root)`.
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let mut words = key.split(' ');
    let _ = words.next();
    let action = words.next();
    let output = match action {
        Some("init") => knowledge::init_knowledge(&root, false)?,
        Some("sources") => run_sources(&root, args, flags)?,
        Some("list") => knowledge::read_records(&root),
        Some("show") => {
            let Some(id) = nonempty(args.first()) else {
                return Err("Usage: knowledge show <id>".into());
            };
            let records = knowledge::read_records(&root);
            records["records"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["id"] == id.as_str()))
                .cloned()
                .ok_or_else(|| format!("Unknown knowledge record '{id}'"))?
        }
        Some("check") => knowledge::check_records(&root),
        Some("index") => knowledge::write_index(&root)?,
        _ => return Err("Use knowledge init, sources, list, show, check, or index".into()),
    };
    Ok((output, 0))
}

/// JavaScript truthiness for a positional argument (`!value` rejects "").
fn nonempty(value: Option<&String>) -> Option<&String> {
    value.filter(|value| !value.is_empty())
}

fn run_sources(
    root: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<Value, String> {
    let string_flag = |name: &str| match flags.get(name) {
        Some(FlagValue::String(value)) => Some(value.as_str()),
        _ => None,
    };
    match args.first().map(String::as_str) {
        None | Some("" | "list") => {
            let (_, manifest) = knowledge::load_knowledge_manifest(root)?;
            Ok(knowledge::inspect_sources(root, &manifest))
        }
        Some("add") => {
            let (Some(id), Some(path)) = (nonempty(args.get(1)), nonempty(args.get(2))) else {
                return Err("Usage: knowledge sources add <id> <path> --kind KIND".into());
            };
            let kind = string_flag("kind").unwrap_or("docs");
            knowledge::add_source(root, id, path, kind, string_flag("description"))
        }
        Some("remove") => {
            let Some(id) = nonempty(args.get(1)) else {
                return Err("Usage: knowledge sources remove <id>".into());
            };
            knowledge::remove_source(root, id)
        }
        Some(_) => {
            Err("Use knowledge sources, knowledge sources add, or knowledge sources remove".into())
        }
    }
}

/// Execute a knowledge command. `args` contains words following `knowledge`
/// (for example, `sources add id docs/README.md`).
///
/// Older dispatcher kept for its callers; the CLI router should use [`run`], which
/// follows Node's argument handling exactly.
///
/// Returns `Ok(None)` for an unrecognized command, allowing the surrounding
/// dispatcher to fall back without causing side effects.
pub fn dispatch(
    root: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<Option<Value>, String> {
    let Some(action) = args.first().map(String::as_str) else {
        return Ok(None);
    };
    let rest = &args[1..];
    let output = match action {
        "init" if rest.is_empty() => knowledge::init_knowledge(root, false)?,
        "init" => {
            return Err(format!(
                "Unexpected argument '{}' for 'knowledge init'.",
                rest[0]
            ))
        }
        "sources" => return dispatch_sources(root, rest, flags),
        "list" if rest.is_empty() => knowledge::read_records(root),
        "list" => {
            return Err(format!(
                "Unexpected argument '{}' for 'knowledge list'.",
                rest[0]
            ))
        }
        "show" => {
            let Some(id) = rest.first() else {
                return Err("Usage: knowledge show <id>".into());
            };
            if rest.len() > 1 {
                return Err(format!(
                    "Unexpected argument '{}' for 'knowledge show'.",
                    rest[1]
                ));
            }
            let records = knowledge::read_records(root);
            records["records"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["id"] == id.as_str()))
                .cloned()
                .ok_or_else(|| format!("Unknown knowledge record '{id}'"))?
        }
        "check" if rest.is_empty() => knowledge::check_records(root),
        "check" => {
            return Err(format!(
                "Unexpected argument '{}' for 'knowledge check'.",
                rest[0]
            ))
        }
        "index" if rest.is_empty() => knowledge::write_index(root)?,
        "index" => {
            return Err(format!(
                "Unexpected argument '{}' for 'knowledge index'.",
                rest[0]
            ))
        }
        _ => return Ok(None),
    };
    Ok(Some(output))
}

fn dispatch_sources(
    root: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<Option<Value>, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        let (_, manifest) = knowledge::load_knowledge_manifest(root)?;
        return Ok(Some(knowledge::inspect_sources(root, &manifest)));
    };
    let rest = &args[1..];
    match subcommand {
        "list" if rest.is_empty() => {
            let (_, manifest) = knowledge::load_knowledge_manifest(root)?;
            Ok(Some(knowledge::inspect_sources(root, &manifest)))
        }
        "list" => Err(format!(
            "Unexpected argument '{}' for 'knowledge sources'.",
            rest[0]
        )),
        "add" => {
            if rest.len() < 2 {
                return Err("Usage: knowledge sources add <id> <path> --kind KIND".into());
            }
            if rest.len() > 2 {
                return Err(format!(
                    "Unexpected argument '{}' for 'knowledge sources'.",
                    rest[2]
                ));
            }
            let kind = match flags.get("kind") {
                Some(FlagValue::String(value)) => value.as_str(),
                _ => "docs",
            };
            let description = match flags.get("description") {
                Some(FlagValue::String(value)) => Some(value.as_str()),
                _ => None,
            };
            knowledge::add_source(root, &rest[0], &rest[1], kind, description).map(Some)
        }
        "remove" => {
            let Some(id) = rest.first() else {
                return Err("Usage: knowledge sources remove <id>".into());
            };
            if rest.len() > 1 {
                return Err(format!(
                    "Unexpected argument '{}' for 'knowledge sources'.",
                    rest[1]
                ));
            }
            knowledge::remove_source(root, id).map(Some)
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("knowledge-command-{label}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn unsupported_commands_do_not_mutate_project() {
        let root = temp_root("unsupported");
        assert_eq!(
            dispatch(&root, &words(&["not-a-command"]), &BTreeMap::new()).unwrap(),
            None
        );
        assert_eq!(
            dispatch(
                &root,
                &words(&["sources", "not-a-subcommand"]),
                &BTreeMap::new()
            )
            .unwrap(),
            None
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn routes_read_only_commands_and_preserves_unknown_record_error() {
        let root = temp_root("read");
        fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
        let listed = dispatch(&root, &words(&["list"]), &BTreeMap::new())
            .unwrap()
            .unwrap();
        assert_eq!(listed, json!({"records": [], "invalid": []}));
        assert_eq!(
            dispatch(&root, &words(&["show", "missing"]), &BTreeMap::new()).unwrap_err(),
            "Unknown knowledge record 'missing'"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_add_uses_defaults_and_flags() {
        let root = temp_root("sources");
        let mut flags = BTreeMap::new();
        flags.insert("kind".into(), FlagValue::String("readme".into()));
        flags.insert(
            "description".into(),
            FlagValue::String("Project readme".into()),
        );
        let added = dispatch(
            &root,
            &words(&["sources", "add", "readme", "README.md"]),
            &flags,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            added,
            json!({"id":"readme","path":"README.md","kind":"readme","description":"Project readme"})
        );
        let loaded = knowledge::load_knowledge_manifest(&root).unwrap();
        assert!(loaded.0);
        assert!(loaded.1["sources"].as_array().unwrap().contains(&added));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_add_does_not_write_manifest() {
        let root = temp_root("invalid-add");
        let result = dispatch(
            &root,
            &words(&["sources", "add", "docs", "../outside"]),
            &BTreeMap::new(),
        );
        assert!(result.is_err());
        assert!(!root.join(knowledge::KNOWLEDGE_MANIFEST).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
