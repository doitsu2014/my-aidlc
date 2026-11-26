//! `orchestrate park`, `orchestrate queue` and `orchestrate revisit`: the work-unit
//! queue commands of an intent with a work-unit ledger (see [`crate::work_units`]).
//!
//! - `park` sets a blocked work unit aside with a structured reason (its kind and what
//!   it waits for); `orchestrate next` then continues with the next ready unit.
//! - `queue` is read-only: it lists ready, waiting, blocked, parked and done units. On
//!   a 1.0.0 state that has not migrated yet it shows the ledger the next orchestrate
//!   command would build, marked `"migration": "pending"`, and writes nothing.
//! - `revisit` returns parked units to the queue: without `--unit` only those whose
//!   blockers are done; with `--unit` the named ones (an owner decision needs
//!   `--detail`, a design defect needs `intent rework`).
//!
//! Writes follow the orchestrate rules: every check passes first, then the audit log is
//! checked, then the intent `state.json` is written only if it still holds the bytes
//! read at the start (temp file and rename), then the audit rows are appended. A 1.0.0
//! state migrates to the ledger as part of the same write.

use crate::orchestrate::{
    audit, audit_revisits, persist_active, OrchestrateResult, AI_ORCHESTRATOR,
};
use crate::work_units::{self, LedgerChange, ParkRequest};
use crate::workflow::{depth_of, load_workflow, mode_of, ordered_stages, LoadedWorkflow};
use serde_json::{json, Value};
use std::path::Path;

/// Options of the queue commands, already split (list options on `,`).
#[derive(Clone, Debug, Default)]
pub struct UnitOptions {
    pub units: Vec<String>,
    pub kind: Option<String>,
    pub blocked_by: Vec<String>,
    pub detail: Option<String>,
    pub approved_by: Option<String>,
}

const NO_INTENT: &str =
    "No active intent. Start one with: dtech-aidlc orchestrate next --intent \"...\"";
const NEXT: &str = "run dtech-aidlc orchestrate next";

/// Run `orchestrate park`, `orchestrate queue` or `orchestrate revisit`. `root` must be
/// absolute; `config` is the project configuration read without validation.
pub fn run(
    key: &str,
    root: &Path,
    graph: &Value,
    config: &Value,
    options: &UnitOptions,
) -> OrchestrateResult<Value> {
    match key {
        "orchestrate park" => park(root, graph, options),
        "orchestrate queue" => queue(root, graph, config),
        "orchestrate revisit" => revisit(root, graph, options),
        _ => Err("Use orchestrate next, report, park, queue or revisit".into()),
    }
}

/// The active intent's state and its ordered stages.
fn active(graph: &Value, loaded: &LoadedWorkflow) -> OrchestrateResult<(Value, Vec<Value>)> {
    let state = loaded.state.clone().ok_or(NO_INTENT)?;
    let scope = state
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("solo")
        .to_owned();
    let stages = ordered_stages(graph, &scope, depth_of(&state))
        .into_iter()
        .cloned()
        .collect();
    Ok((state, stages))
}

fn intent_id(state: &Value) -> String {
    state
        .get("intentId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn persist(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    state: &mut Value,
    stages: &[Value],
    now: &str,
    change: &LedgerChange,
) -> OrchestrateResult<()> {
    work_units::sync_mirrors(state, stages, now);
    crate::audit::preflight_audit(root)?;
    persist_active(root, graph, loaded, state)?;
    for (event, details) in change.audit_rows(&intent_id(state)) {
        audit(root, event, details)?;
    }
    Ok(())
}

fn park(root: &Path, graph: &Value, options: &UnitOptions) -> OrchestrateResult<Value> {
    let loaded = load_workflow(root, graph, true)?;
    park_loaded(root, graph, &loaded, options)
}

/// `orchestrate park` on a workflow the caller already loaded. Nothing is written when
/// the intent state on disk no longer holds the bytes in `loaded`.
pub fn park_loaded(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    options: &UnitOptions,
) -> OrchestrateResult<Value> {
    if options.units.len() > 1 {
        return Err("--unit takes one work unit for orchestrate park".into());
    }
    let kind = trimmed(options.kind.as_deref()).ok_or(
        "orchestrate park requires --kind dep-unit|foreign-regression|owner-decision|design-defect",
    )?;
    let (mut state, stages) = active(graph, loaded)?;
    let now = crate::audit::now_iso();
    let change = work_units::ensure_ledger(
        root,
        graph,
        &mut state,
        &stages,
        &now,
        loaded.state_bytes.as_deref(),
    )?;
    let parked = work_units::park(
        &mut state,
        &stages,
        &ParkRequest {
            unit: options.units.first().map(String::as_str),
            kind,
            blocked_by: &options.blocked_by,
            detail: options.detail.as_deref().unwrap_or(""),
        },
        &now,
    )?;
    persist(root, graph, loaded, &mut state, &stages, &now, &change)?;
    let id = intent_id(&state);
    audit(root, "unit.parked", parked.audit_row(&id))?;
    let mut output = json!({
        "parked": true,
        "intentId": id,
        "unit": parked.unit,
        "stage": parked.stage,
        "kind": parked.kind,
        "blockedBy": parked.blocked_by,
        "blockedByUnits": parked.blocked_by_units,
        "attempts": parked.attempts,
        "next": NEXT,
    });
    if !parked.warnings.is_empty() {
        output["warnings"] = json!(parked.warnings);
    }
    Ok(output)
}

fn queue(root: &Path, graph: &Value, config: &Value) -> OrchestrateResult<Value> {
    let loaded = load_workflow(root, graph, false)?;
    let (state, stages) = active(graph, &loaded)?;
    let Some((view_state, pending)) = work_units::preview(root, graph, &state, &stages)? else {
        return Err("orchestrate queue needs work units (Develop with work-units.json); this intent has not reached them or runs Develop as one implicit unit".into());
    };
    let mut view = work_units::queue_view(
        &view_state,
        &stages,
        work_units::auto_park_threshold(config),
    );
    view["drift"] = match work_units::drift(root, graph, &view_state).filter(|_| !pending) {
        Some((units, registry)) => json!({"workUnitsJson": units, "idsJson": registry}),
        None => Value::Null,
    };
    if pending {
        view["migration"] = json!("pending");
    }
    Ok(view)
}

fn revisit(root: &Path, graph: &Value, options: &UnitOptions) -> OrchestrateResult<Value> {
    let loaded = load_workflow(root, graph, true)?;
    revisit_loaded(root, graph, &loaded, options)
}

/// `orchestrate revisit` on a workflow the caller already loaded. Nothing is written
/// when the intent state on disk no longer holds the bytes in `loaded`.
pub fn revisit_loaded(
    root: &Path,
    graph: &Value,
    loaded: &LoadedWorkflow,
    options: &UnitOptions,
) -> OrchestrateResult<Value> {
    let detail = trimmed(options.detail.as_deref());
    let approved_by = trimmed(options.approved_by.as_deref());
    if options.units.is_empty() && (detail.is_some() || approved_by.is_some()) {
        return Err("--detail and --approved-by apply to orchestrate revisit --unit ID".into());
    }
    let (mut state, stages) = active(graph, loaded)?;
    let now = crate::audit::now_iso();
    let change = work_units::ensure_ledger(
        root,
        graph,
        &mut state,
        &stages,
        &now,
        loaded.state_bytes.as_deref(),
    )?;
    work_units::require_unit_queue(&state, &stages, "revisit")?;
    let id = intent_id(&state);
    let revisits = if options.units.is_empty() {
        work_units::auto_revisits(&mut state, &now)
    } else {
        let mut keys: Vec<String> = Vec::new();
        let mut owner_decision = false;
        for name in &options.units {
            let key = work_units::resolve_unit(&state, name)
                .ok_or_else(|| format!("Unknown work unit {name}"))?;
            let unit = work_units::unit(&state, &key)
                .cloned()
                .unwrap_or(Value::Null);
            let kind = unit
                .get("park")
                .filter(|park| park.is_object())
                .and_then(|park| park.get("kind"))
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{key} is not parked"))?;
            match kind {
                "design-defect" => {
                    return Err(format!("{key} is parked as a design defect; orchestrate revisit cannot clear it. Use dtech-aidlc intent rework --unit {key} --detail \"<reason>\""));
                }
                "owner-decision" => {
                    if detail.is_none() {
                        return Err(format!("{key} waits for an owner decision; record it with orchestrate revisit --unit {key} --detail \"<decision>\""));
                    }
                    owner_decision = true;
                }
                _ => {}
            }
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        // Who decided, by the gates' mode rule. In AI mode the decision is the AI
        // orchestrator's (listed as unconfirmed by the release gate) and a human name
        // is refused; in human mode it is resolved like a gate approver (flag, then
        // git user, then OS user).
        let decided_by = if !owner_decision {
            None
        } else if mode_of(&state) == "ai" {
            if approved_by.is_some_and(|name| name != AI_ORCHESTRATOR) {
                return Err(format!("Intent '{id}' runs in AI orchestrator mode; owner decisions are recorded with --approved-by \"{AI_ORCHESTRATOR}\" (or without --approved-by) and listed as unconfirmed at the release gate. Human decisions apply to human-mode intents."));
            }
            Some(AI_ORCHESTRATOR.to_owned())
        } else {
            Some(crate::identity::resolve_approver(root, approved_by)?.by)
        };
        let mut rows = Vec::new();
        for key in &keys {
            let is_decision = work_units::unit(&state, key)
                .and_then(|unit| unit.pointer("/park/kind"))
                .and_then(Value::as_str)
                == Some("owner-decision");
            let by = if is_decision {
                decided_by.as_deref()
            } else {
                None
            };
            if let Some(row) = work_units::close_park(&mut state, key, "manual", detail, by, &now) {
                rows.push(row);
            }
        }
        rows
    };
    if revisits.is_empty() {
        return Ok(json!({"revisited": [], "intentId": id, "next": NEXT}));
    }
    persist(root, graph, loaded, &mut state, &stages, &now, &change)?;
    audit_revisits(root, &id, &revisits)?;
    Ok(json!({"revisited": revisits, "intentId": id, "next": NEXT}))
}
