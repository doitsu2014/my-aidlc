//! Path redaction end to end (TS-008): no tool result, error message or startup warning
//! over MCP carries an absolute path, the project root or the macOS `/private/var` twin.
//! Windows-form strings are exercised through the public `Redactor` on every OS.

#[path = "fixtures/mcp/common.rs"]
mod common;

use common::*;
use dtech_aidlc::mcp::redact::Redactor;
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

/// Every string value and every object key, at any depth.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(m) => {
            for (k, x) in m {
                out.push(k.clone());
                strings(x, out);
            }
        }
        _ => {}
    }
}

fn looks_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/')
        || s.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'/' | b'\\'))
}

/// Every spelling of the project root that must never appear.
fn root_forms(root: &Path) -> Vec<String> {
    let mut forms = vec![root.to_string_lossy().into_owned()];
    if let Ok(canonical) = root.canonicalize() {
        forms.push(plain(canonical).to_string_lossy().into_owned());
    }
    let more: Vec<String> = forms
        .iter()
        .flat_map(|f| {
            let mut v = vec![f.replace('\\', "/")];
            if let Some(rest) = f.strip_prefix("/private/var/") {
                v.push(format!("/var/{rest}"));
            } else if let Some(rest) = f.strip_prefix("/var/") {
                v.push(format!("/private/var/{rest}"));
            }
            v
        })
        .collect();
    forms.extend(more);
    forms.sort();
    forms.dedup();
    forms
}

fn assert_clean(label: &str, value: &Value, root: &Path) {
    let mut all = Vec::new();
    strings(value, &mut all);
    let forms = root_forms(root);
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok();
    for s in &all {
        for form in &forms {
            assert!(!s.contains(form.as_str()), "{label}: root leaked in {s:?}");
        }
        if let Some(home) = home.as_deref().filter(|h| h.len() > 3) {
            assert!(!s.contains(home), "{label}: home leaked in {s:?}");
        }
        assert!(!looks_absolute(s), "{label}: absolute path in {s:?}");
    }
}

fn seed_legacy_queue(project: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unit_scheduling");
    let dir = project.join(".aidlc/intents/unit-replay");
    let design = dir.join("develop/implementation-design");
    fs::create_dir_all(&design).unwrap();
    fs::copy(fixtures.join("state-1.0.0.json"), dir.join("state.json")).unwrap();
    fs::write(dir.join("intent.md"), "# unit-replay\n\nReplay\n").unwrap();
    fs::copy(
        fixtures.join("work-units.real.json"),
        design.join("work-units.json"),
    )
    .unwrap();
    fs::copy(fixtures.join("ids.json"), design.join("ids.json")).unwrap();
    fs::copy(fixtures.join("work-units.md"), design.join("work-units.md")).unwrap();
    fs::write(
        project.join(".aidlc/state.json"),
        "{\n  \"version\": 2,\n  \"activeIntentId\": \"unit-replay\",\n  \"updatedAt\": \"2026-11-26T02:17:34.449Z\"\n}\n",
    )
    .unwrap();
}

#[test]
fn ts008_status_and_queue_on_the_default_temp_base_carry_no_absolute_path() {
    // Deliberately not canonicalised: on macOS this is the `/var/folders/...` spelling
    // whose real path is `/private/var/folders/...`; the server must hide both.
    let raw_base = std::env::temp_dir();
    let project = Tmp::under(&raw_base, "redact-raw");
    cli_ok(
        &project.0,
        &["onboard", "--harness", "claude", "--scope", "solo"],
    );
    seed_legacy_queue(&project.0);
    let status = tool_ok(&project.0, "status", json!({}));
    assert_eq!(status["intentDir"], ".aidlc/intents/unit-replay");
    assert_clean("status", &status, &project.0);
    let queue = tool_ok(&project.0, "queue", json!({}));
    assert_eq!(queue["intentId"], "unit-replay");
    assert_clean("queue", &queue, &project.0);
}

#[test]
fn ts008_next_and_report_results_are_project_relative() {
    let project = Tmp::project("redact-flow");
    start_intent(&project.0, "Add login page");
    let next = tool_ok(&project.0, "next", json!({}));
    let directive = &next["result"];
    assert_eq!(directive["intentDir"], ".aidlc/intents/add-login-page");
    assert_eq!(
        directive["contractsFile"],
        ".aidlc/engine/methodology/contracts.md"
    );
    assert_eq!(
        directive["stage"]["phaseFile"],
        ".aidlc/engine/methodology/phases/analyze.md"
    );
    assert_eq!(directive["context"]["projectRoot"], ".");
    assert_clean("next", &next, &project.0);
    let report = tool_ok(
        &project.0,
        "report",
        json!({"result": "blocked", "detail": format!("see {}/notes.md", project.0.display())}),
    );
    assert_clean("report", &report, &project.0);
    // The CLI, by contrast, prints the root: the redaction is the MCP layer's doing.
    let cli_next = cli_ok(&project.0, &["orchestrate", "next"]);
    assert!(cli_next["intentDir"]
        .as_str()
        .unwrap()
        .contains(project.0.to_str().unwrap()));
}

#[test]
fn ts008_error_messages_never_carry_a_path() {
    let project = Tmp::project("redact-errors");
    start_intent(&project.0, "Add login page");
    // Corrupt the intent state so every library error names a file under the root.
    let state = project.0.join(".aidlc/intents/add-login-page/state.json");
    fs::write(&state, "{ not json").unwrap();
    for (name, args) in [
        ("status", json!({})),
        ("next", json!({})),
        ("report", json!({"result": "blocked"})),
        ("status", json!({"intent_id": "missing-intent"})),
    ] {
        let (value, is_error) = tool(&project.0, name, args);
        assert!(is_error, "{name}: {value}");
        let error = &value["error"];
        assert_eq!(error["code"], "operation_failed", "{name}: {value}");
        assert!(error["message"].as_str().unwrap().chars().count() <= 1000);
        assert_clean(name, &value, &project.0);
    }
    // A hostile value echoed into an argument error is redacted too.
    let hostile = format!("{}/secret", project.0.display());
    let (value, _) = tool(&project.0, "status", json!({ hostile.clone(): 1 }));
    assert_eq!(value["error"]["code"], "invalid_arguments");
    assert_clean("echoed argument", &value, &project.0);
}

#[test]
fn ts008_unusable_engine_home_warns_on_stderr_without_the_path_and_stdout_stays_pure() {
    let project = Tmp::project("redact-home");
    start_intent(&project.0, "Add login page");
    let engine = project.0.join("no-such-engine");
    let out = run_mcp_env(
        Some(&project.0),
        lines(&[modern(
            1,
            "tools/call",
            json!({"name": "status", "arguments": {}}),
        )]),
        &[("DTECH_AIDLC_HOME", &engine)],
    );
    let replies = frames_of(&out);
    assert_eq!(replies.len(), 1);
    assert_eq!(
        replies[0]["result"]["structuredContent"]["intentId"],
        "add-login-page"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(stderr.contains("DTECH_AIDLC_HOME"), "{stderr}");
    assert!(stderr.contains("embedded engine"), "{stderr}");
    for form in root_forms(&project.0) {
        assert!(!stderr.contains(&form), "root leaked on stderr: {stderr}");
    }
    assert_clean(
        "stdout",
        &replies[0]["result"]["structuredContent"],
        &project.0,
    );
}

// ---------------------------------------------------------------------------
// Pure strings (any OS)
// ---------------------------------------------------------------------------

fn redactor(root: &str) -> Redactor {
    Redactor::with_env(
        Path::new(root),
        Some(OsStr::new("/opt/engine")),
        Some(OsStr::new("/home/dev")),
    )
}

#[test]
fn ts008_windows_form_strings_are_redacted_on_any_os() {
    let r = redactor(r"C:\Users\dev\proj");
    let generic = "<generic>";
    // The project root in each form becomes project-relative.
    for (text, expected) in [
        (r"C:\Users\dev\proj\.aidlc\state.json", ".aidlc/state.json"),
        (r"C:/Users/dev/proj/.aidlc/state.json", ".aidlc/state.json"),
        (
            r"\\?\C:\Users\dev\proj\.aidlc\state.json",
            ".aidlc/state.json",
        ),
        (r"C:\Users\dev\proj", "."),
    ] {
        let mut v = json!({ text: [text] });
        r.redact_value(&mut v);
        let (k, x) = v.as_object().unwrap().iter().next().unwrap();
        assert_eq!(k, expected, "key of {text}");
        assert_eq!(x[0], expected, "value of {text}");
        assert_eq!(
            r.redact_message(&format!("cannot read {text}"), generic),
            format!("cannot read {expected}")
        );
    }
    // Anything else absolute, in any Windows form, never survives.
    for text in [
        r"C:\Windows\System32\config",
        r"D:/data/secret.txt",
        r"\\host\share\file",
        r"\\?\UNC\host\share\file",
        r"\\?\D:\data\x",
        "/etc/passwd",
        "/home/other/.ssh/id_rsa",
    ] {
        let mut v = json!(text);
        r.redact_value(&mut v);
        assert_eq!(v, "<redacted-path>", "{text}");
        assert_eq!(
            r.redact_message(&format!("failed: {text}"), generic),
            generic
        );
    }
}

#[test]
fn ts008_prefix_matches_respect_path_boundaries_and_registered_roots() {
    let r = redactor("/work/proj");
    let mut v = json!({
        "inside": "/work/proj/src/lib.rs",
        "sibling": "/work/project/src/lib.rs",
        "engine": "/opt/engine/core/data/phase-graph.json",
        "home": "/home/dev/.config/x",
        "relative": "src/lib.rs",
        "sentence": "wrote /work/proj/a.md.",
    });
    r.redact_value(&mut v);
    assert_eq!(v["inside"], "src/lib.rs");
    assert_eq!(
        v["sibling"], "<redacted-path>",
        "/work/project is not /work/proj"
    );
    assert_eq!(v["engine"], "$DTECH_AIDLC_HOME/core/data/phase-graph.json");
    assert_eq!(v["home"], "~/.config/x");
    assert_eq!(v["relative"], "src/lib.rs");
    assert_eq!(v["sentence"], "wrote a.md.");
}

#[test]
fn ts008_macos_private_var_twin_is_registered_for_a_var_root() {
    let r = redactor("/var/folders/ab/T/proj");
    let mut v = json!([
        "/private/var/folders/ab/T/proj/.aidlc/x",
        "/var/folders/ab/T/proj/y"
    ]);
    r.redact_value(&mut v);
    assert_eq!(v, json!([".aidlc/x", "y"]));
    let r = redactor("/private/var/folders/ab/T/proj");
    let mut v = json!("/var/folders/ab/T/proj/z");
    r.redact_value(&mut v);
    assert_eq!(v, "z");
}

#[test]
fn ts008_windows_matching_is_case_insensitive_only_on_windows() {
    let r = redactor(r"C:\Users\Dev\Proj");
    let mut v = json!(r"c:\users\dev\proj\a.md");
    r.redact_value(&mut v);
    if cfg!(windows) {
        assert_eq!(v, "a.md");
    } else {
        // Elsewhere case matters; the string is still not allowed to leak.
        assert!(v == "a.md" || v == "<redacted-path>", "{v}");
        assert!(!v.as_str().unwrap().to_lowercase().contains("users"));
    }
}
