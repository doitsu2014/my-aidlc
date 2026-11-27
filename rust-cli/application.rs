//! Shared application facade used by both the `dtech-aidlc` CLI and the
//! `dtech-aidlc-mcp` server, so the two front ends run the same operations
//! (status, queue, next, report) with the same validation, gate policy and
//! audit. No stdin, stdout or exit codes here.

use crate::orchestrate::{self, Engine, NextOptions, NextOutcome, ReportOptions};
use crate::scope::{self, Graph};
use serde_json::{json, Value};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    Cli,
    Mcp,
}

pub struct Project {
    pub root: PathBuf,
    pub graph: Graph,
    pub engine: Engine,
    pub caller: Caller,
}

pub const APPROVER_REQUIRED: &str =
    "approved_by is required for a gate decision over MCP (no git or OS fallback)";

/// The installed engine named by `DTECH_AIDLC_HOME` (Node `engineRoot()`), if any. Like
/// Node's `path.resolve(home)`, the candidates are absolute and normalised.
pub fn external_engine() -> Option<PathBuf> {
    external_engine_from(env::var_os("DTECH_AIDLC_HOME").as_deref())
}

fn external_engine_from(home: Option<&OsStr>) -> Option<PathBuf> {
    let home = home.filter(|home| !home.is_empty())?;
    let home = orchestrate::resolve_like_node(Path::new(home)).ok()?;
    [home.join("core"), home].into_iter().find(|candidate| {
        candidate.join("data/phase-graph.json").exists() || candidate.join("methodology").exists()
    })
}

/// Resolve the root, assert a safe state dir, then pick the engine and read its graph.
pub fn open(project: &Path, caller: Caller) -> Result<Project, String> {
    let root = orchestrate::resolve_like_node(project)?;
    crate::audit::assert_safe_state_dir(&root)?;
    let engine_root = external_engine();
    let graph = scope::read_graph(engine_root.as_deref())?;
    let engine = match engine_root {
        Some(engine) => Engine::external(engine),
        None => Engine::embedded(&root),
    };
    Ok(Project {
        root,
        graph,
        engine,
        caller,
    })
}

pub fn status(p: &Project, intent: Option<&str>) -> Result<Value, String> {
    crate::status::status(&p.root, &p.graph, intent)
}

pub fn queue(p: &Project) -> Result<Value, String> {
    let config = scope::load_config_unvalidated(&p.root);
    crate::unit_commands::run(
        "orchestrate queue",
        &p.root,
        p.graph.value(),
        &config,
        &crate::unit_commands::UnitOptions::default(),
    )
}

pub fn knowledge_preflight(p: &Project) -> Result<(), String> {
    // Node reads nothing for a project folder that does not exist.
    if p.root.exists() {
        crate::knowledge::knowledge_preflight(&p.root)?;
    }
    Ok(())
}

pub fn next(p: &Project, o: &NextOptions) -> Result<NextOutcome, String> {
    let mut options = o.clone();
    if p.caller == Caller::Mcp {
        knowledge_preflight(p)?;
        options.intent = None;
        options.scope = None;
        options.depth = None;
        options.mode = None;
        options.approved_by = None;
        options.detail = None;
        options.force_resume = false;
        options.skip_prerequisite_checks = true;
    }
    let graph = p.graph.value();
    let config = scope::validate_model_config(&p.root, graph)?;
    let outcome = orchestrate::orchestrate_next(&p.root, graph, &config, &p.engine, &options)?;
    match outcome {
        NextOutcome::Directive(mut directive) => {
            orchestrate::finish_directive(&p.root, &mut directive)?;
            Ok(NextOutcome::Directive(directive))
        }
        other => Ok(other),
    }
}

pub fn report(p: &Project, o: &ReportOptions) -> Result<Value, String> {
    if p.caller == Caller::Mcp
        && o.approval.is_some()
        && o.approved_by
            .as_deref()
            .is_none_or(|by| by.trim().is_empty())
    {
        return Err(APPROVER_REQUIRED.to_owned());
    }
    let graph = p.graph.value();
    let config = scope::load_config_unvalidated(&p.root);
    let mut result =
        orchestrate::orchestrate_report_with_engine(&p.root, graph, &config, &p.engine, o)?;
    orchestrate::finish_report(&p.root, &mut result)?;
    if p.caller == Caller::Mcp {
        let pending = orchestrate::pending_prerequisites(&p.root, graph, &config)?;
        if let Some(map) = result.as_object_mut() {
            map.insert("pendingPrerequisites".into(), json!(pending));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("aidlc-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn approval(by: Option<&str>) -> ReportOptions {
        ReportOptions {
            stage: Some("any".into()),
            approval: Some("approved".into()),
            approved_by: by.map(str::to_owned),
            ..ReportOptions::default()
        }
    }

    #[test]
    fn ts010_blank_approver_refused_over_mcp_before_any_lookup() {
        let dir = temp_project("q18");
        let p = open(&dir, Caller::Mcp).unwrap();
        for by in [None, Some(""), Some("   \t\n")] {
            assert_eq!(report(&p, &approval(by)).unwrap_err(), APPROVER_REQUIRED);
        }
        assert!(!dir.join(".aidlc").exists(), "nothing may be written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ts010_cli_caller_is_not_subject_to_the_approver_guard() {
        let dir = temp_project("cli");
        let p = open(&dir, Caller::Cli).unwrap();
        let error = report(&p, &approval(Some("  "))).unwrap_err();
        assert_ne!(error, APPROVER_REQUIRED);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn initialized_project(tag: &str) -> PathBuf {
        let dir = temp_project(tag);
        std::fs::create_dir_all(dir.join(".aidlc")).unwrap();
        std::fs::write(dir.join(".aidlc/config.json"), "{}").unwrap();
        dir
    }

    #[test]
    fn ts010_mcp_next_cannot_create_an_intent() {
        let dir = initialized_project("next");
        let p = open(&dir, Caller::Mcp).unwrap();
        let options = NextOptions {
            intent: Some("build a thing".into()),
            force_resume: true,
            detail: Some("x".into()),
            ..NextOptions::default()
        };
        let error = next(&p, &options).unwrap_err();
        assert!(error.contains("No active intent"), "{error}");
        assert!(!error.contains("not initialized"), "{error}");
        assert!(!dir.join(".aidlc/state.json").exists());
        assert!(!dir.join(".aidlc/intents").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn audit_rows(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(".aidlc/audit.jsonl")).unwrap_or_default()
    }

    #[test]
    fn ts010_mcp_next_skips_prerequisite_checks_and_cli_runs_them() {
        let dir = initialized_project("skip");
        let cli = open(&dir, Caller::Cli).unwrap();
        let start = NextOptions {
            intent: Some("mcp skip".into()),
            ..NextOptions::default()
        };
        let NextOutcome::Directive(d) = next(&cli, &start).unwrap() else {
            panic!("expected a directive")
        };
        let stage = d["stage"]["id"]
            .as_str()
            .or(d["stage"].as_str())
            .unwrap()
            .to_owned();
        let marker = dir.join("marker");
        std::fs::write(
            dir.join(".aidlc/config.json"),
            json!({"prerequisites":{"toolchain":{
                "check": format!("echo ran > \"{}\" && exit 7", marker.display()),
                "hint":"Install it","stages":[stage]}}})
            .to_string(),
        )
        .unwrap();
        let audit_before = audit_rows(&dir);
        let mcp = open(&dir, Caller::Mcp).unwrap();
        // Every MCP reset (intent, scope, depth, mode, approver) is set; none may apply.
        let hostile = NextOptions {
            intent: Some("other".into()),
            scope: Some("team".into()),
            depth: Some("fast".into()),
            mode: Some("auto".into()),
            approved_by: Some("me".into()),
            force_resume: true,
            ..NextOptions::default()
        };
        let NextOutcome::Blocked(payload) = next(&mcp, &hostile).unwrap() else {
            panic!("expected blocked")
        };
        assert_eq!(payload["needs"][0]["executed"], false);
        assert!(!marker.exists(), "MCP must not spawn the check");
        let after = audit_rows(&dir);
        assert_eq!(after, audit_before);
        assert!(!after.contains("prerequisite.checked"));

        let NextOutcome::Blocked(payload) = next(&cli, &NextOptions::default()).unwrap() else {
            panic!("expected blocked")
        };
        assert!(marker.exists(), "the CLI still runs the check");
        assert!(payload["needs"][0].get("executed").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ts011_external_engine_from_honours_the_injected_home() {
        assert!(external_engine_from(None).is_none());
        assert!(external_engine_from(Some(OsStr::new(""))).is_none());
        let home = temp_project("engine");
        assert!(external_engine_from(Some(home.as_os_str())).is_none());
        std::fs::create_dir_all(home.join("core/methodology")).unwrap();
        let found = external_engine_from(Some(home.as_os_str())).unwrap();
        assert!(found.ends_with("core"), "{}", found.display());
        let _ = std::fs::remove_dir_all(&home);
    }
}
