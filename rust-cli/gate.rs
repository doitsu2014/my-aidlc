//! Pure validation for phase-gate approvals.
//!
//! This module intentionally performs no file access or workflow mutation. The caller
//! resolves the effective gate mode, approver policy, and recommendation text, then
//! supplies them to [`validate_gate_approval`].

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateApprovalInput<'a> {
    /// The mode recorded on the intent (`human` or `ai`).
    pub intent_mode: &'a str,
    /// The effective mode for this particular gate, after any human pin.
    pub gate_mode: &'a str,
    pub gate_id: &'a str,
    pub decision: &'a str,
    pub approver: &'a str,
    /// The configured allowlist for this gate, or the owner's fallback allowlist.
    /// Only applies to human-mode approvals. An empty list means no restriction.
    pub allowed_approvers: &'a [String],
    /// Contents of gate-recommendation.md, if the caller could read a non-empty file.
    pub recommendation: Option<&'a str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenItemCounts {
    pub written_approval: usize,
    pub follow_up: usize,
}

/// Validates approval identity, AI recommendation completeness, and human approver
/// policy. The returned counts are present only for accepted AI approvals.
pub fn validate_gate_approval(
    input: &GateApprovalInput<'_>,
) -> Result<Option<OpenItemCounts>, String> {
    if !matches!(input.intent_mode, "human" | "ai") {
        return Err("intent mode must be human or ai".into());
    }
    if !matches!(input.gate_mode, "human" | "ai") {
        return Err("gate mode must be human or ai".into());
    }
    if !matches!(input.decision, "approved" | "rejected") {
        return Err("approval must be approved or rejected".into());
    }

    const AI_ORCHESTRATOR: &str = "AI Orchestrator";
    let gate_pinned = input.gate_mode != input.intent_mode;
    if input.gate_mode == "ai" && input.approver != AI_ORCHESTRATOR {
        return Err(format!(
            "Intent runs in AI orchestrator mode; gate approvals must use --approved-by \"{AI_ORCHESTRATOR}\". Human approvals apply to human-mode intents."
        ));
    }
    if input.gate_mode == "human" && input.approver == AI_ORCHESTRATOR {
        return Err(if gate_pinned {
            format!("Gate {} is pinned to human mode by 'config gate-mode'; \"{AI_ORCHESTRATOR}\" cannot approve it. Present it to the owner.", input.gate_id)
        } else {
            format!("Intent runs in human mode; \"{AI_ORCHESTRATOR}\" cannot approve a human-mode gate.")
        });
    }

    let open_counts = if input.gate_mode == "ai" && input.decision == "approved" {
        let text = input.recommendation.filter(|text| !text.trim().is_empty())
            .ok_or_else(|| format!("AI gate approval for {} requires a gate-recommendation artifact; the agent must research options and write its recommendation first.", input.gate_id))?;
        let items = parse_open_items(text);
        if !items.present {
            return Err(format!("gate-recommendation.md for {} needs an '## Open items' section: list each deferred item tagged [written-approval] or [follow-up], or write '- None'.", input.gate_id));
        }
        if !items.none && items.written_approval + items.follow_up + items.untagged == 0 {
            return Err(format!("gate-recommendation.md for {} has an empty '## Open items' section; write '- None' when nothing is open.", input.gate_id));
        }
        if items.untagged > 0 {
            return Err(format!("gate-recommendation.md for {} has {} open item(s) without a [written-approval] or [follow-up] tag; classify each one.", input.gate_id, items.untagged));
        }
        if items.written_approval > 0 {
            return Err(format!("Gate {} cannot be approved by the AI Orchestrator: gate-recommendation.md leaves {} item(s) that need written approval open. Report the stage blocked with those items in --detail, or ask the owner to pin the gate to human approval with 'dtech-aidlc config gate-mode {} human'.", input.gate_id, items.written_approval, input.gate_id));
        }
        Some(OpenItemCounts {
            written_approval: items.written_approval,
            follow_up: items.follow_up,
        })
    } else {
        None
    };

    if input.gate_mode == "human"
        && !input.allowed_approvers.is_empty()
        && !input
            .allowed_approvers
            .iter()
            .any(|name| name == input.approver)
    {
        return Err(format!(
            "Approver '{}' is not allowed for gate {}. Allowed: {}",
            input.approver,
            input.gate_id,
            input.allowed_approvers.join(", ")
        ));
    }

    Ok(open_counts)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParsedOpenItems {
    present: bool,
    none: bool,
    written_approval: usize,
    follow_up: usize,
    untagged: usize,
}

fn parse_open_items(text: &str) -> ParsedOpenItems {
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines
        .iter()
        .position(|line| is_open_items_heading(line.trim()))
    else {
        return ParsedOpenItems::default();
    };

    let body: Vec<&str> = lines[start + 1..]
        .iter()
        .take_while(|line| !is_section_heading(line.trim()))
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let items: Vec<String> = body
        .iter()
        .filter_map(|line| strip_list_marker(line).map(str::to_owned))
        .collect();
    let is_none = |item: &str| {
        let value = item.trim();
        value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("none.")
    };
    let open: Vec<&str> = items
        .iter()
        .map(String::as_str)
        .filter(|item| !is_none(item))
        .collect();

    let mut parsed = ParsedOpenItems {
        present: true,
        none: open.is_empty()
            && (items.iter().any(|item| is_none(item)) || body.iter().any(|line| is_none(line))),
        ..ParsedOpenItems::default()
    };
    for item in open {
        if starts_tag(item, "[written-approval]") {
            parsed.written_approval += 1;
        } else if starts_tag(item, "[follow-up]") {
            parsed.follow_up += 1;
        } else {
            parsed.untagged += 1;
        }
    }
    parsed
}

fn is_open_items_heading(line: &str) -> bool {
    let Some((hashes, title)) = heading(line) else {
        return false;
    };
    (2..=3).contains(&hashes) && title.eq_ignore_ascii_case("open items")
}

fn is_section_heading(line: &str) -> bool {
    heading(line).is_some_and(|(hashes, _)| (1..=3).contains(&hashes))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    if hashes == 0
        || !line[hashes..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        return None;
    }
    Some((hashes, line[hashes..].trim()))
}

fn strip_list_marker(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'*'))
        && line[1..].chars().next().is_some_and(char::is_whitespace)
    {
        return Some(line[1..].trim());
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits > 0
        && bytes.get(digits) == Some(&b'.')
        && line[digits + 1..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        return Some(line[digits + 1..].trim());
    }
    None
}

fn starts_tag(item: &str, tag: &str) -> bool {
    item.get(..tag.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(
        intent_mode: &str,
        gate_mode: &str,
        decision: &str,
        approver: &str,
        recommendation: Option<&str>,
        allowed: &[&str],
    ) -> Result<Option<OpenItemCounts>, String> {
        let allowed: Vec<String> = allowed.iter().map(|name| (*name).to_owned()).collect();
        validate_gate_approval(&GateApprovalInput {
            intent_mode,
            gate_mode,
            gate_id: "analysis-baseline",
            decision,
            approver,
            allowed_approvers: &allowed,
            recommendation,
        })
    }

    #[test]
    fn enforces_gate_mode_approver_identity_and_pinning() {
        assert!(
            validate("human", "human", "approved", "AI Orchestrator", None, &[])
                .unwrap_err()
                .contains("human mode")
        );
        assert!(validate(
            "ai",
            "ai",
            "approved",
            "Reviewer",
            Some("## Open items\n- None"),
            &[]
        )
        .unwrap_err()
        .contains("AI orchestrator mode"));
        assert!(
            validate("ai", "human", "approved", "AI Orchestrator", None, &[])
                .unwrap_err()
                .contains("pinned to human mode")
        );
        assert!(validate("ai", "human", "approved", "Reviewer", None, &["Reviewer"]).is_ok());
    }

    #[test]
    fn nonbreaking_space_cannot_hide_written_approval() {
        let recommendation =
            "## Open items\n- None\n-\u{a0}[written-approval] Owner approval required";
        assert!(validate(
            "ai",
            "ai",
            "approved",
            "AI Orchestrator",
            Some(recommendation),
            &[]
        )
        .unwrap_err()
        .contains("written approval"));
    }

    #[test]
    fn requires_valid_recommendation_and_classified_open_items_for_ai_approval() {
        assert!(
            validate("ai", "ai", "approved", "AI Orchestrator", None, &[])
                .unwrap_err()
                .contains("gate-recommendation")
        );
        assert!(validate(
            "ai",
            "ai",
            "approved",
            "AI Orchestrator",
            Some("## Decision\nChosen"),
            &[]
        )
        .unwrap_err()
        .contains("needs an '## Open items' section"));
        assert!(validate(
            "ai",
            "ai",
            "approved",
            "AI Orchestrator",
            Some("## Open items\n"),
            &[]
        )
        .unwrap_err()
        .contains("empty '## Open items' section"));
        assert!(validate(
            "ai",
            "ai",
            "approved",
            "AI Orchestrator",
            Some("## Open items\n- Deferred"),
            &[]
        )
        .unwrap_err()
        .contains("without a [written-approval] or [follow-up] tag"));
        assert!(validate(
            "ai",
            "ai",
            "approved",
            "AI Orchestrator",
            Some("## Open items\n- [written-approval] Owner decision"),
            &[]
        )
        .unwrap_err()
        .contains("need written approval open"));

        let accepted = validate(
            "ai",
            "ai",
            "approved",
            "AI Orchestrator",
            Some("## Open items\n- [follow-up] Tune alerts\n- None\n\n## Notes\nLater"),
            &[],
        )
        .unwrap();
        assert_eq!(
            accepted,
            Some(OpenItemCounts {
                written_approval: 0,
                follow_up: 1
            })
        );
    }

    #[test]
    fn enforces_human_allowlist_but_allows_unrestricted_policy() {
        assert!(
            validate("human", "human", "approved", "Cara", None, &["Ana", "Binh"])
                .unwrap_err()
                .contains("Allowed: Ana, Binh")
        );
        assert_eq!(
            validate("human", "human", "rejected", "Cara", None, &[]),
            Ok(None)
        );
    }

    #[test]
    fn validation_is_pure_and_rejects_unknown_modes_or_decisions() {
        assert!(validate("robot", "human", "approved", "Ana", None, &[]).is_err());
        assert!(validate("human", "robot", "approved", "Ana", None, &[]).is_err());
        assert!(validate("human", "human", "maybe", "Ana", None, &[]).is_err());
    }
}
