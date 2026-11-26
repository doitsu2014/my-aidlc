use dtech_aidlc::orchestration_report_validation::{
    resolve_approver, resolve_gate_policy, validate_report_policy, ReportPolicyInput,
};
use serde_json::json;

fn request<'a>(config: &'a serde_json::Value) -> ReportPolicyInput<'a> {
    ReportPolicyInput {
        config,
        result: "complete",
        detail: None,
        needs: &[],
        phase_gate: true,
        gate_id: "release",
        intent_mode: "ai",
        approval: Some("approved"),
        requested_approver: Some("AI Orchestrator"),
        git_user_name: Some("Git User"),
        os_username: Some("OS User"),
        approval_note: None,
        recommendation: Some("## Open items\n- [follow-up] Monitor launch"),
    }
}

#[test]
fn gate_human_pin_overrides_ai_mode_and_resolves_named_approvers_then_owner() {
    let config = json!({
        "gateModes": {"release": "human"},
        "roles": {"owner": ["Owner"], "approvers": {"release": ["Release Reviewer"]}}
    });
    let (mode, allowlist) = resolve_gate_policy(&config, "ai", "release").unwrap();
    assert_eq!(mode, "human");
    assert_eq!(allowlist, ["Release Reviewer"]);

    let fallback = json!({"roles":{"owner":["Owner"],"approvers":{"other":["Other"]}}});
    assert_eq!(
        resolve_gate_policy(&fallback, "human", "release").unwrap(),
        ("human".to_owned(), vec!["Owner".to_owned()])
    );
}

#[test]
fn approver_precedence_and_input_safety_are_preserved() {
    assert_eq!(
        resolve_approver(Some("  Explicit  "), Some("Git"), Some("OS")).unwrap(),
        "Explicit"
    );
    assert_eq!(
        resolve_approver(None, Some(" Git "), Some("OS")).unwrap(),
        "Git"
    );
    assert_eq!(resolve_approver(None, Some(" "), Some("OS")).unwrap(), "OS");
    assert_eq!(resolve_approver(None, None, None).unwrap(), "unknown");
    assert!(resolve_approver(Some(&"x".repeat(201)), None, None).is_err());
    assert!(resolve_approver(Some("bad\nname"), None, None).is_err());
}

#[test]
fn policy_integration_enforces_allowlist_and_accepts_ai_recommendation() {
    let config = json!({"roles":{"approvers":{"release":["Owner"]}}});
    let mut value = request(&config);
    value.intent_mode = "human";
    value.requested_approver = Some("Other");
    value.recommendation = None;
    assert!(validate_report_policy(&value)
        .unwrap_err()
        .contains("Allowed: Owner"));

    let config = json!({});
    let value = request(&config);
    let counts = validate_report_policy(&value).unwrap().unwrap();
    assert_eq!(counts.written_approval, 0);
    assert_eq!(counts.follow_up, 1);
}

#[test]
fn malformed_gate_policy_fails_closed_and_non_gate_reports_need_no_identity() {
    let invalid = json!({"gateModes":{"release":"ai"}});
    assert!(resolve_gate_policy(&invalid, "human", "release")
        .unwrap_err()
        .contains("only be pinned to human"));

    let invalid = json!({"roles":{"approvers":{"release":"Owner"}}});
    assert!(resolve_gate_policy(&invalid, "human", "release").is_err());

    let config = json!({});
    let mut value = request(&config);
    value.phase_gate = false;
    value.approval = None;
    value.requested_approver = None;
    value.result = "blocked";
    value.detail = Some("Waiting on dependency");
    value.intent_mode = "human";
    value.recommendation = None;
    assert!(validate_report_policy(&value).unwrap().is_none());
}
