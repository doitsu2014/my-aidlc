//! Pure validation for orchestration report and phase-gate policy.
//!
//! This helper checks only request-level report/gate rules. Callers remain responsible
//! for loading workflow state, resolving the effective gate mode and approver, checking
//! prerequisites, validating artifacts, and persisting/auditing an accepted report.

use crate::gate::{validate_gate_approval, GateApprovalInput, OpenItemCounts};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrchestrationReportInput<'a> {
    pub result: &'a str,
    pub detail: Option<&'a str>,
    pub needs: &'a [String],
    /// Whether this report is the actual phase gate, including last-work-unit rules.
    pub phase_gate: bool,
    pub gate_id: &'a str,
    /// Intent mode and effective mode after any human pin.
    pub intent_mode: &'a str,
    pub gate_mode: &'a str,
    pub approval: Option<&'a str>,
    /// Resolved approver identity (the CLI resolves flag/git/OS identity beforehand).
    pub approver: &'a str,
    pub approval_note: Option<&'a str>,
    /// Human approver allowlist, or empty when unrestricted/not applicable.
    pub allowed_approvers: &'a [String],
    /// Recommendation artifact contents, if available.
    pub recommendation: Option<&'a str>,
}

/// Validates request-level report rules and gate approval policy without side effects.
///
/// Successful AI approvals return counts of remaining follow-ups. For non-AI approvals
/// and reports without an AI approval, the counts are `None`.
pub fn validate_orchestration_report(
    input: &OrchestrationReportInput<'_>,
) -> Result<Option<OpenItemCounts>, String> {
    if !matches!(input.result, "complete" | "blocked" | "skipped") {
        return Err("--result must be complete, blocked, or skipped".into());
    }
    if matches!(input.result, "blocked" | "skipped") && input.detail.is_none_or(str::is_empty) {
        return Err(format!(
            "--result {} requires --detail \"<reason>\"",
            input.result
        ));
    }
    if let Some(approval) = input.approval {
        if !matches!(approval, "approved" | "rejected") {
            return Err("--approval must be approved or rejected".into());
        }
        if approval == "approved" && input.result == "blocked" {
            return Err(
                "--approval approved cannot be combined with --result blocked; a blocked report never passes a gate".into(),
            );
        }
    }
    if !input.needs.is_empty() && input.result != "blocked" {
        return Err("--needs is only accepted with --result blocked".into());
    }
    if !input.phase_gate
        && (input.approval.is_some() || !input.approver.is_empty() || input.approval_note.is_some())
    {
        return Err(
            "--approval, --approved-by and --approval-note are only accepted at a phase gate"
                .into(),
        );
    }
    if (!input.approver.is_empty() || input.approval_note.is_some()) && input.approval.is_none() {
        return Err("--approved-by and --approval-note require --approval".into());
    }
    if input.approval == Some("rejected") && input.result != "blocked" {
        return Err("--approval rejected must be reported with --result blocked --detail \"<the owner's feedback>\"".into());
    }
    if input.phase_gate
        && matches!(input.result, "complete" | "skipped")
        && input.approval != Some("approved")
    {
        return Err(format!(
            "Phase gate {} requires explicit --approval approved from the project owner; this also applies when the stage is skipped.",
            input.gate_id
        ));
    }

    if let Some(decision) = input.approval {
        return validate_gate_approval(&GateApprovalInput {
            intent_mode: input.intent_mode,
            gate_mode: input.gate_mode,
            gate_id: input.gate_id,
            decision,
            approver: input.approver,
            allowed_approvers: input.allowed_approvers,
            recommendation: input.recommendation,
        });
    }
    Ok(None)
}
