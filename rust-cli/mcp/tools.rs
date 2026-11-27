//! MCP tool adapter (FN-013, FN-014): four tools over the application facade.
//! Arguments are validated before the project is opened; errors are classified by
//! exact equality; every value, key and message is redacted.

use super::redact::Redactor;
use crate::application::{self, Caller};
use crate::orchestrate::{self, NextOptions, NextOutcome, ReportOptions};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub const MAX_INTENT_ID: usize = 64;
pub const MAX_STAGE: usize = 64;
pub const MAX_UNIT: usize = 64;
pub const MAX_APPROVED_BY: usize = 200;
pub const MAX_APPROVAL_NOTE: usize = 4096;
pub const MAX_DETAIL: usize = 16384;
pub const MAX_NEEDS: usize = 16;
pub const MAX_NEED: usize = 32;
pub const MAX_ERROR_MESSAGE: usize = 1000;

pub struct ToolContext {
    pub root: PathBuf,
    pub redactor: Redactor,
}

type Chars = (fn(char) -> bool, &'static str);

#[derive(Clone, Copy)]
enum Kind {
    Str {
        max: usize,
        /// Allowed characters (and a length of at least 1) when set.
        chars: Option<Chars>,
        enum_: &'static [&'static str],
        min1: bool,
    },
    Bool,
    Needs,
}

fn lower(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}
fn mixed(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

const fn text(max: usize, min1: bool) -> Kind {
    Kind::Str {
        max,
        chars: None,
        enum_: &[],
        min1,
    }
}
const fn id(max: usize, p: fn(char) -> bool, re: &'static str) -> Kind {
    Kind::Str {
        max,
        chars: Some((p, re)),
        enum_: &[],
        min1: true,
    }
}
const fn choice(e: &'static [&'static str]) -> Kind {
    Kind::Str {
        max: 64,
        chars: None,
        enum_: e,
        min1: false,
    }
}

const LOWER_RE: &str = "^[a-z0-9-]{1,64}$";
const MIXED_RE: &str = "^[A-Za-z0-9-]{1,64}$";

// Enums read once from the CLI parser: `--result must be complete, blocked, or skipped`
// (orchestrate.rs) and the approval values `approved` / `rejected`. Stage ids all match
// ^[a-z0-9-]{1,64}$ in core/data/phase-graph.json; unit ids are the work-unit ids.
type Field = (&'static str, Kind);
const STATUS: &[Field] = &[("intent_id", id(MAX_INTENT_ID, lower, LOWER_RE))];
const NEXT: &[Field] = &[
    ("claim", Kind::Bool),
    ("unit", id(MAX_UNIT, mixed, MIXED_RE)),
];
const REPORT: &[Field] = &[
    ("stage", id(MAX_STAGE, lower, LOWER_RE)),
    ("unit", id(MAX_UNIT, mixed, MIXED_RE)),
    ("result", choice(&["complete", "blocked", "skipped"])),
    ("approval", choice(&["approved", "rejected"])),
    ("approved_by", text(MAX_APPROVED_BY, true)),
    ("approval_note", text(MAX_APPROVAL_NOTE, false)),
    ("detail", text(MAX_DETAIL, false)),
    ("needs", Kind::Needs),
];

fn fields(tool: &str) -> Option<&'static [Field]> {
    match tool {
        "status" => Some(STATUS),
        "queue" => Some(&[]),
        "next" => Some(NEXT),
        "report" => Some(REPORT),
        _ => None,
    }
}

fn schema(kind: Kind) -> Value {
    match kind {
        Kind::Bool => json!({"type": "boolean"}),
        Kind::Needs => json!({"type": "array", "maxItems": MAX_NEEDS, "items": {
            "type": "string", "maxLength": MAX_NEED, "pattern": "^[a-z0-9-]{1,32}$"}}),
        Kind::Str {
            max,
            chars,
            enum_,
            min1,
        } => {
            let mut s = json!({"type": "string", "maxLength": max});
            if !enum_.is_empty() {
                s["enum"] = json!(enum_);
            }
            if let Some((_, re)) = chars {
                s["pattern"] = json!(re);
            }
            if min1 && chars.is_none() {
                s["minLength"] = json!(1);
            }
            s
        }
    }
}

fn tool(name: &str, description: &str, read_only: bool, destructive: Option<bool>) -> Value {
    let mut annotations = json!({"readOnlyHint": read_only, "openWorldHint": false});
    if let Some(d) = destructive {
        annotations["destructiveHint"] = json!(d);
    }
    let props: Map<String, Value> = fields(name)
        .unwrap_or(&[])
        .iter()
        .map(|(k, kind)| ((*k).to_string(), schema(*kind)))
        .collect();
    json!({"name": name, "description": description,
        "inputSchema": {"type": "object", "additionalProperties": false, "properties": props},
        "annotations": annotations})
}

pub fn tool_list() -> Value {
    json!([
        tool("status", "Read-only: report the workflow status of the project, optionally for one intent.", true, None),
        tool("queue", "Read-only: show the work-unit queue (a preview; nothing is changed).", true, None),
        tool("next", "Select and return the next stage directive. Prerequisite checks are not executed over MCP; a failing or unrun check is returned as outcome blocked. Cannot create an intent. With claim, claim one more ready work unit; with unit, print one already in flight (not both).", false, Some(false)),
        tool("report", "Report the result of a stage. Use approval only for a decision the human owner made in this conversation. approved_by is required with approval; it is a self-asserted label, not authentication. pendingPrerequisites in the result is a preview, not a promise.", false, Some(true)),
    ])
}

fn validate(tool: &str, args: &Value) -> Result<(), String> {
    let spec = fields(tool).unwrap_or(&[]);
    let map = args
        .as_object()
        .ok_or_else(|| "arguments must be an object".to_string())?;
    for (key, v) in map {
        let Some((_, kind)) = spec.iter().find(|(k, _)| k == key) else {
            return Err(format!("unknown argument: {key}"));
        };
        let ok = match *kind {
            Kind::Bool => v.is_boolean(),
            Kind::Str {
                max,
                chars,
                enum_,
                min1,
            } => v.as_str().is_some_and(|s| {
                let n = s.chars().count();
                n <= max
                    && (!min1 || n >= 1)
                    && (enum_.is_empty() || enum_.contains(&s))
                    && chars.is_none_or(|(p, _)| s.chars().all(p))
            }),
            Kind::Needs => v.as_array().is_some_and(|a| {
                a.len() <= MAX_NEEDS
                    && a.iter().all(|x| {
                        x.as_str().is_some_and(|s| {
                            (1..=MAX_NEED).contains(&s.len()) && s.chars().all(lower)
                        })
                    })
            }),
        };
        if !ok {
            return Err(format!("invalid value for argument: {key}"));
        }
    }
    if tool == "next" && map.get("claim") == Some(&json!(true)) && map.contains_key("unit") {
        return Err("--claim and --unit cannot be combined: --claim takes a new unit, --unit prints one already in flight".into());
    }
    Ok(())
}

fn string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Exact-equality classification (never substring): (code, generic text, retryable).
fn classify(message: &str) -> (&'static str, &'static str, bool) {
    if message == orchestrate::STATE_CHANGED {
        (
            "state_conflict",
            "Workflow state changed; run the tool again.",
            true,
        )
    } else if message == application::APPROVER_REQUIRED {
        ("approver_required", application::APPROVER_REQUIRED, false)
    } else {
        ("operation_failed", "The operation failed.", false)
    }
}

fn error_value(
    ctx: &ToolContext,
    code: &str,
    message: &str,
    generic: &str,
    retryable: bool,
) -> Value {
    let mut m = ctx.redactor.redact_message(message, generic);
    if m.chars().count() > MAX_ERROR_MESSAGE {
        m = m.chars().take(MAX_ERROR_MESSAGE).collect();
    }
    json!({"error": {"code": code, "message": m, "retryable": retryable}})
}

fn outcome_value(o: NextOutcome) -> Value {
    let (name, v) = match o {
        NextOutcome::Directive(v) => ("directive", v),
        NextOutcome::Complete(v) => ("complete", v),
        NextOutcome::Blocked(v) => ("blocked", v),
        NextOutcome::Busy(v) => ("busy", v),
    };
    json!({"outcome": name, "result": v})
}

fn finish(ctx: &ToolContext, r: Result<Value, String>) -> Result<Value, Value> {
    match r {
        Ok(mut v) => {
            ctx.redactor.redact_value(&mut v);
            Ok(v)
        }
        Err(e) => {
            let (code, generic, retryable) = classify(&e);
            Err(error_value(ctx, code, &e, generic, retryable))
        }
    }
}

pub fn call_tool(ctx: &ToolContext, name: &str, args: &Value) -> Option<Result<Value, Value>> {
    fields(name)?;
    let empty = json!({});
    let args = if args.is_null() { &empty } else { args };
    if let Err(m) = validate(name, args) {
        return Some(Err(error_value(
            ctx,
            "invalid_arguments",
            &m,
            "Invalid arguments.",
            false,
        )));
    }
    let run = || -> Result<Value, String> {
        let p = application::open(&ctx.root, Caller::Mcp)?;
        match name {
            "status" => application::status(&p, string(args, "intent_id").as_deref()),
            "queue" => application::queue(&p),
            "next" => application::next(
                &p,
                &NextOptions {
                    claim: args.get("claim") == Some(&json!(true)),
                    unit: string(args, "unit"),
                    ..NextOptions::default()
                },
            )
            .map(outcome_value),
            _ => application::report(
                &p,
                &ReportOptions {
                    stage: string(args, "stage"),
                    unit: string(args, "unit"),
                    result: string(args, "result"),
                    approval: string(args, "approval"),
                    approved_by: string(args, "approved_by"),
                    approval_note: string(args, "approval_note"),
                    detail: string(args, "detail"),
                    needs: args
                        .get("needs")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            ),
        }
    };
    Some(finish(ctx, run()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mcp-tools-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
    fn ctx(root: &std::path::Path) -> ToolContext {
        ToolContext {
            root: root.to_path_buf(),
            redactor: Redactor::new(root),
        }
    }
    fn err_of(r: Option<Result<Value, Value>>) -> Value {
        r.unwrap().unwrap_err()["error"].clone()
    }

    #[test]
    fn ts018_exactly_four_tools_with_contract_annotations() {
        let list = tool_list();
        let names: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["status", "queue", "next", "report"]);
        for t in list.as_array().unwrap() {
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
            assert_eq!(t["annotations"]["openWorldHint"], false);
            assert!(t.get("outputSchema").is_none());
        }
        assert_eq!(list[0]["annotations"]["readOnlyHint"], true);
        assert!(list[0]["annotations"].get("destructiveHint").is_none());
        assert_eq!(list[2]["annotations"]["destructiveHint"], false);
        assert_eq!(list[3]["annotations"]["destructiveHint"], true);
        let report = list[3]["description"].as_str().unwrap();
        assert!(report.contains("self-asserted") && report.contains("not authentication"));
        assert!(list[2]["description"]
            .as_str()
            .unwrap()
            .contains("not executed"));
        let enum_ = &list[3]["inputSchema"]["properties"]["result"]["enum"];
        assert_eq!(enum_, &json!(["complete", "blocked", "skipped"]));
        assert!(call_tool(&ctx(&base("x")), "doctor", &json!({})).is_none());
    }

    #[test]
    fn ts004_validation_runs_before_the_project_opens() {
        let root = base("noopen");
        let c = ctx(&root);
        let long = "x".repeat(MAX_DETAIL + 1);
        let cases = [
            ("status", json!({"root": "/etc"})),
            ("status", json!({"intent_id": "Bad_Id"})),
            ("queue", json!({"x": 1})),
            ("next", json!({"intent": "new"})),
            ("next", json!({"approved_by": "me"})),
            ("next", json!({"claim": true, "unit": "WU-1"})),
            ("report", json!({"result": "done"})),
            ("report", json!({"detail": long})),
            ("report", json!({"needs": ["Bad"]})),
            ("report", json!({"approved_by": ""})),
            ("report", json!({"force_resume": true})),
            ("report", json!([])),
        ];
        for (t, a) in cases {
            let e = err_of(call_tool(&c, t, &a));
            assert_eq!(e["code"], "invalid_arguments", "{t} {a}");
            assert_eq!(e["retryable"], false);
        }
        assert!(!root.exists(), "validation must not touch the filesystem");
    }

    #[test]
    fn ts004_invalid_arguments_win_even_when_open_would_fail() {
        let root = base("openfails");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".aidlc"), "x").unwrap();
        let c = ctx(&root);
        // Valid arguments reach open and fail there ...
        let e = err_of(call_tool(&c, "status", &json!({})));
        assert_eq!(e["code"], "operation_failed");
        // ... but invalid ones must be rejected before open is attempted.
        let e = err_of(call_tool(&c, "status", &json!({"intent_id": "Bad_Id"})));
        assert_eq!(e["code"], "invalid_arguments");
        let e = err_of(call_tool(&c, "report", &json!({"result": "done"})));
        assert_eq!(e["code"], "invalid_arguments");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ts004_errors_are_classified_by_exact_equality_and_redacted() {
        assert_eq!(
            classify(orchestrate::STATE_CHANGED),
            (
                "state_conflict",
                "Workflow state changed; run the tool again.",
                true
            )
        );
        assert_eq!(
            classify(application::APPROVER_REQUIRED).0,
            "approver_required"
        );
        let near = format!("{} extra", orchestrate::STATE_CHANGED);
        assert_eq!(classify(&near).0, "operation_failed");
        assert_eq!(
            classify(&format!("x {}", application::APPROVER_REQUIRED)).0,
            "operation_failed"
        );
        let root = base("redact");
        let c = ctx(&root);
        let msg = format!("cannot read {}/secret and /etc/passwd", root.display());
        let e = error_value(&c, "operation_failed", &msg, "The operation failed.", false);
        assert_eq!(e["error"]["message"], "The operation failed.");
        let ok = error_value(
            &c,
            "operation_failed",
            &format!("bad file {}/x", root.display()),
            "g",
            false,
        );
        assert!(!ok["error"]["message"]
            .as_str()
            .unwrap()
            .contains(root.to_str().unwrap()));
        let long = error_value(&c, "operation_failed", &"a".repeat(5000), "g", false);
        assert_eq!(
            long["error"]["message"].as_str().unwrap().len(),
            MAX_ERROR_MESSAGE
        );
    }

    #[test]
    fn ts004_results_are_redacted_in_values_and_keys() {
        let root = base("rkeys");
        let c = ctx(&root);
        let key = root.join("k").display().to_string();
        let val = root.join("v").display().to_string();
        let r = finish(&c, Ok(json!({ key: [val] }))).unwrap();
        let root_str = root.to_str().unwrap();
        let (k, v) = r.as_object().unwrap().iter().next().unwrap();
        assert!(!k.contains(root_str), "{r}");
        assert!(!v[0].as_str().unwrap().contains(root_str), "{r}");
    }

    #[test]
    fn ts004_blocked_next_is_a_result_not_an_error() {
        let v = outcome_value(NextOutcome::Blocked(json!({"needs": []})));
        assert_eq!(v, json!({"outcome": "blocked", "result": {"needs": []}}));
        assert_eq!(
            outcome_value(NextOutcome::Busy(json!(1)))["outcome"],
            "busy"
        );
        assert_eq!(
            outcome_value(NextOutcome::Complete(json!(1)))["outcome"],
            "complete"
        );
        assert_eq!(
            outcome_value(NextOutcome::Directive(json!(1)))["outcome"],
            "directive"
        );
    }

    #[test]
    fn ts004_approval_without_label_is_approver_required() {
        let root = base("approver");
        std::fs::create_dir_all(&root).unwrap();
        let c = ctx(&root);
        for a in [
            json!({"approval": "approved"}),
            json!({"approval": "approved", "approved_by": "  "}),
        ] {
            let e = err_of(call_tool(&c, "report", &a));
            assert_eq!(e["code"], "approver_required");
            assert_eq!(e["retryable"], false);
        }
        assert!(!root.join(".aidlc").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
