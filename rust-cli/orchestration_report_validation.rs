//! Bounded orchestration report policy integration.
//!
//! This module joins request-level validation from `orchestration_policy` with the
//! existing Node policy semantics for gate pins, approver allowlists, and identity
//! fallback. It is a pure boundary: callers still load workflow state, determine
//! phase-gate status, validate artifacts/prerequisites, read the recommendation
//! artifact safely, and persist/audit only after success. It is not routed from
//! the CLI yet.

use crate::gate::OpenItemCounts;
use crate::orchestration_policy::{validate_orchestration_report, OrchestrationReportInput};
use serde_json::Value;

/// Inputs available at the report-policy boundary after workflow/gate discovery.
#[derive(Clone, Debug)]
pub struct ReportPolicyInput<'a> {
    pub config: &'a Value,
    pub result: &'a str,
    pub detail: Option<&'a str>,
    pub needs: &'a [String],
    pub phase_gate: bool,
    pub gate_id: &'a str,
    pub intent_mode: &'a str,
    pub approval: Option<&'a str>,
    pub requested_approver: Option<&'a str>,
    /// Already obtained by the caller using Node-compatible git config lookup.
    pub git_user_name: Option<&'a str>,
    /// Already obtained by the caller using Node-compatible OS username lookup.
    pub os_username: Option<&'a str>,
    pub approval_note: Option<&'a str>,
    pub recommendation: Option<&'a str>,
}

/// Identity chosen with Node's precedence: explicit flag, git user.name, OS username,
/// then `unknown`. This validates the explicit identity limit/control-character rule.
pub fn resolve_approver(
    requested: Option<&str>,
    git_user_name: Option<&str>,
    os_username: Option<&str>,
) -> Result<String, String> {
    if let Some(value) = requested.filter(|value| !value.is_empty()) {
        let name = value.trim();
        if name.is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
            return Err(
                "--approved-by must be 1 to 200 characters without control characters".into(),
            );
        }
        return Ok(name.to_owned());
    }
    for candidate in [git_user_name, os_username].into_iter().flatten() {
        let name = candidate.trim();
        if !name.is_empty() {
            return Ok(name.to_owned());
        }
    }
    Ok("unknown".into())
}

/// Resolve the effective gate mode and the Node-compatible approver allowlist.
/// A gate can only be pinned to `human`; absent a pin, the intent mode controls.
pub fn resolve_gate_policy(
    config: &Value,
    intent_mode: &str,
    gate_id: &str,
) -> Result<(String, Vec<String>), String> {
    let gate_modes = config.get("gateModes");
    if let Some(value) = gate_modes.and_then(|modes| modes.get(gate_id)) {
        if value.as_str() != Some("human") {
            return Err(format!(
                "Invalid gateModes.{gate_id}: a gate can only be pinned to human"
            ));
        }
    }
    let gate_mode = if gate_modes
        .and_then(|modes| modes.get(gate_id))
        .and_then(Value::as_str)
        == Some("human")
    {
        "human"
    } else {
        intent_mode
    };

    let roles = config.get("roles");
    let Some(roles) = roles.filter(|roles| !roles.is_null()) else {
        return Ok((gate_mode.to_owned(), Vec::new()));
    };
    if !roles.is_object() {
        return Err("Invalid roles: expected an object".into());
    }

    let approvers = roles.get("approvers");
    if let Some(approvers) = approvers.filter(|value| !value.is_null()) {
        if !approvers.is_object() {
            return Err(
                "Invalid roles.approvers: expected an object mapping gate ids to names".into(),
            );
        }
        if let Some(named) = approvers.get(gate_id) {
            let names = string_array(named, &format!("roles.approvers.{gate_id}"))?;
            if !names.is_empty() {
                return Ok((gate_mode.to_owned(), names));
            }
        }
    }

    let owner = roles.get("owner");
    if let Some(owner) = owner.filter(|value| !value.is_null()) {
        let names = string_array(owner, "roles.owner")?;
        if !names.is_empty() {
            return Ok((gate_mode.to_owned(), names));
        }
    }
    Ok((gate_mode.to_owned(), Vec::new()))
}

fn string_array(value: &Value, label: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .filter(|items| items.iter().all(Value::is_string))
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .ok_or_else(|| format!("Invalid {label}"))
}

/// Resolve the available config/identity policy and run the pure report validator.
/// Call only after deriving `phase_gate` and loading recommendation contents.
pub fn validate_report_policy(
    input: &ReportPolicyInput<'_>,
) -> Result<Option<OpenItemCounts>, String> {
    let (gate_mode, allowed_approvers) =
        resolve_gate_policy(input.config, input.intent_mode, input.gate_id)?;
    let approver = if input.approval.is_some() {
        resolve_approver(
            input.requested_approver,
            input.git_user_name,
            input.os_username,
        )?
    } else {
        String::new()
    };
    validate_orchestration_report(&OrchestrationReportInput {
        result: input.result,
        detail: input.detail,
        needs: input.needs,
        phase_gate: input.phase_gate,
        gate_id: input.gate_id,
        intent_mode: input.intent_mode,
        gate_mode: &gate_mode,
        approval: input.approval,
        approver: &approver,
        approval_note: input.approval_note,
        allowed_approvers: &allowed_approvers,
        recommendation: input.recommendation,
    })
}
