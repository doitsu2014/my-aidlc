use dtech_aidlc::orchestration_policy::{validate_orchestration_report, OrchestrationReportInput};

fn input<'a>(result: &'a str) -> OrchestrationReportInput<'a> {
    OrchestrationReportInput {
        result,
        detail: None,
        needs: &[],
        phase_gate: false,
        gate_id: "analysis-baseline",
        intent_mode: "human",
        gate_mode: "human",
        approval: None,
        approver: "",
        approval_note: None,
        allowed_approvers: &[],
        recommendation: None,
    }
}

#[test]
fn report_inputs_enforce_result_detail_and_needs_rules() {
    let mut value = input("finished");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("--result"));

    value = input("blocked");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("requires --detail"));
    value.detail = Some("Waiting for owner");
    let needs = vec!["build-tools".to_owned()];
    value.needs = &needs;
    assert!(validate_orchestration_report(&value).is_ok());

    value.result = "complete";
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("--needs"));
}

#[test]
fn phase_gates_require_approval_even_when_skipped() {
    let mut value = input("skipped");
    value.phase_gate = true;
    value.detail = Some("Not applicable");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("explicit --approval approved"));

    value.approval = Some("approved");
    value.approver = "Owner";
    assert!(validate_orchestration_report(&value).is_ok());
}

#[test]
fn approval_flags_are_gate_only_and_consistent() {
    let mut value = input("complete");
    value.approver = "Owner";
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("only accepted at a phase gate"));

    value = input("complete");
    value.phase_gate = true;
    value.approval_note = Some("reviewed");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("require --approval"));

    value = input("blocked");
    value.detail = Some("Owner requested changes");
    value.phase_gate = true;
    value.approval = Some("rejected");
    value.approver = "Owner";
    assert!(validate_orchestration_report(&value).is_ok());
}

#[test]
fn ai_gate_requires_orchestrator_identity_and_complete_recommendation() {
    let mut value = input("complete");
    value.phase_gate = true;
    value.intent_mode = "ai";
    value.gate_mode = "ai";
    value.approval = Some("approved");
    value.approver = "Reviewer";
    value.recommendation = Some("## Open items\n- None");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("AI orchestrator mode"));

    value.approver = "AI Orchestrator";
    value.recommendation = Some("## Open items\n- [written-approval] Confirm data retention");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("need written approval open"));

    value.recommendation = Some("## Open items\n- [follow-up] Monitor rollout");
    let counts = validate_orchestration_report(&value).unwrap().unwrap();
    assert_eq!(counts.written_approval, 0);
    assert_eq!(counts.follow_up, 1);
}

#[test]
fn human_gate_allowlist_and_invalid_approval_are_checked() {
    let allowed = vec!["Ana".to_owned()];
    let mut value = input("complete");
    value.phase_gate = true;
    value.approval = Some("approved");
    value.approver = "Binh";
    value.allowed_approvers = &allowed;
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("Allowed: Ana"));

    value.approver = "Ana";
    value.approval = Some("maybe");
    assert!(validate_orchestration_report(&value)
        .unwrap_err()
        .contains("--approval must be approved or rejected"));
}
