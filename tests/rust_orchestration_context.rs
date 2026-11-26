use dtech_aidlc::orchestration_context::{
    build, build_with_engine, extra_required_artifacts, extra_required_artifacts_with_engine,
    project_file_warnings, ContextInput,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "aidlc-context-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn stage(grants: Option<Value>) -> Value {
    let mut row = json!({"id":"work","phase":"analyze"});
    if let Some(grants) = grants {
        row["inputs"] = json!({"context":grants});
    }
    row
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn scaffold() -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core/data/scaffold/PROJECT.md"),
    )
    .unwrap()
}

const OWNED_PROJECT: &str = "# Project\n\nBilling service for internal teams.\n\n## Commands\n\ncargo test\n\n## Environment prerequisites\n\nRust 1.80\n\n## Known baseline failures\n\nNone.\n";

/// Run a Node reference function from `core/tools` on the same fixture.
fn node(module: &str, mode: &str, input: &Value) -> Value {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = r#"
const { pathToFileURL } = await import('node:url');
const mod = await import(pathToFileURL(process.env.AIDLC_MODULE).href);
const input = JSON.parse(process.env.AIDLC_INPUT);
let out = null;
if (process.env.AIDLC_MODE === 'context') out = mod.buildDirectiveContext(input);
if (process.env.AIDLC_MODE === 'snapshot') out = mod.writeSourcesSnapshot(input.root, input.sources);
if (process.env.AIDLC_MODE === 'projectFileWarnings') out = mod.projectFileWarnings(input.root, input.path, { developing: input.developing });
if (process.env.AIDLC_MODE === 'extraRequiredArtifacts') out = mod.extraRequiredArtifacts({ root: input.root, stage: input.stage });
process.stdout.write(JSON.stringify(out));
"#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", script])
        .env_remove("DTECH_AIDLC_HOME")
        .env("AIDLC_MODULE", repo.join("core/tools").join(module))
        .env("AIDLC_MODE", mode)
        .env("AIDLC_INPUT", input.to_string())
        .current_dir(&repo)
        .output()
        .expect("Node must be available for context parity tests");
    assert!(
        output.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn node_context(root: &Path, state: &Value, stage: &Value, graph: &Value) -> Value {
    node(
        "context.mjs",
        "context",
        &json!({"root":root,"state":state,"stage":stage,"graph":graph}),
    )
}

fn rust_context(root: &Path, state: &Value, stage: &Value, graph: &Value) -> Value {
    build(ContextInput {
        root,
        state,
        stage,
        graph,
    })
    .unwrap()
}

/// Template paths intentionally differ: Node points into its install, Rust into
/// the materialized engine under `.aidlc/engine`.
fn without_templates(mut context: Value) -> Value {
    if let Some(map) = context["codebaseMap"].as_object_mut() {
        map.remove("template");
    }
    if let Some(record) = context["knowledge"]["record"].as_object_mut() {
        record.remove("template");
    }
    context
}

fn assert_node_parity(root: &Path, state: &Value, stage: &Value) -> Value {
    let graph = json!({});
    let rust = rust_context(root, state, stage, &graph);
    let node = node_context(root, state, stage, &graph);
    assert_eq!(without_templates(rust.clone()), without_templates(node));
    rust
}

fn write_profile(root: &Path, schema: u64, kind: &str) {
    write(
        root,
        ".aidlc/shared/project-profile.json",
        &json!({"schemaVersion":schema,"generated":true,"kind":kind,"scannedAt":"2026-01-02T03:04:05.000Z","languages":[{"name":"rust","evidence":["Cargo.toml"]}],"documentation":{"manifest":"aidlc.knowledge.json","detected":[]}}).to_string(),
    );
}

fn node_snapshot(root: &Path) {
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("aidlc.knowledge.json")).unwrap()).unwrap();
    node(
        "onboard.mjs",
        "snapshot",
        &json!({"root":root,"sources":manifest["sources"]}),
    );
}

fn expected_template(root: &Path, name: &str) -> String {
    root.join(".aidlc/engine/methodology/templates")
        .join(name)
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalized(value: &Value) -> String {
    value.as_str().unwrap().replace('\\', "/")
}

#[test]
fn builds_default_envelope_request_sources_and_knowledge() {
    let root = temp();
    fs::write(root.join("PROJECT.md"), OWNED_PROJECT).unwrap();
    fs::write(root.join("GUIDE.md"), "# Guide\n").unwrap();
    fs::write(root.join("aidlc.knowledge.json"), r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"},{"id":"guide","path":"GUIDE.md","kind":"docs"}]}"#).unwrap();
    fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
    fs::write(
        root.join(".aidlc/knowledge/decision.md"),
        "# Decision\n\ntext\n",
    )
    .unwrap();
    // A scanned project whose snapshot has matching hashes but stale mtimes:
    // the hash comparison must report no change.
    write_profile(&root, 2, "greenfield");
    let files: Vec<Value> = [("project", "PROJECT.md"), ("guide", "GUIDE.md")]
        .iter()
        .map(|(source, path)| {
            let bytes = fs::read(root.join(path)).unwrap();
            json!({"source":source,"path":path,"bytes":bytes.len(),"mtimeMs":0,"sha256":format!("{:x}", Sha256::digest(&bytes))})
        })
        .collect();
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        &json!({"schemaVersion":1,"generated":true,"scannedAt":"2026-01-02T03:04:05.000Z","truncated":false,"files":files}).to_string(),
    );
    let state = json!({"intentId":"intent-1"});
    let graph = json!({});

    let context = rust_context(&root, &state, &stage(None), &graph);
    assert_eq!(context["schemaVersion"], 1);
    assert_eq!(context["projectRoot"], root.to_string_lossy().as_ref());
    assert_eq!(
        context["request"]["path"],
        ".aidlc/intents/intent-1/intent.md"
    );
    assert_eq!(context["request"]["provenance"], "User-provided");
    assert_eq!(context["project"]["provenance"], "User-provided");
    assert_eq!(context["sources"][0]["id"], "guide");
    assert_eq!(context["sources"][0]["provenance"], "Observed");
    assert_eq!(context["knowledge"]["records"][0]["id"], "decision");
    assert_eq!(
        context["knowledge"]["records"][0]["path"],
        ".aidlc/knowledge/decision.md"
    );
    assert!(context["warnings"].as_array().unwrap().is_empty());
    assert_eq!(context["sourceChanges"]["scanned"], true);
    assert_eq!(context["sourceChanges"]["changed"], json!([]));
    assert_eq!(context["codebaseMap"]["required"], false);
    assert_node_parity(&root, &state, &stage(None));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn respects_grants_and_adds_work_unit_baseline_hint() {
    let root = temp();
    fs::create_dir_all(root.join(".aidlc/intents/i/analyze")).unwrap();
    fs::write(
        root.join(".aidlc/intents/i/analyze/baseline.md"),
        "baseline\n",
    )
    .unwrap();
    let state = json!({"intentId":"i"});
    let stage = json!({"id":"unit","phase":"analyze","repeat":"work-unit","inputs":{"context":["knowledge"]}});
    let context = build(ContextInput {
        root: &root,
        state: &state,
        stage: &stage,
        graph: &json!({}),
    })
    .unwrap();
    assert_eq!(context["granted"], json!(["knowledge"]));
    assert!(context.get("request").is_none());
    assert!(context.get("project").is_none());
    assert!(context.get("sources").is_none());
    assert!(context.get("sourceChanges").is_none());
    assert!(context.get("codebaseMap").is_none());
    assert_eq!(
        context["baseline"],
        json!({"path":".aidlc/intents/i/analyze/baseline.md","exists":true})
    );
    assert_eq!(context["knowledge"]["dir"], ".aidlc/knowledge");
    assert_node_parity(&root, &state, &stage);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn writeback_context_identifies_record_and_embedded_template() {
    let root = temp();
    fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
    fs::write(root.join(".aidlc/knowledge/intent-1.md"), "record").unwrap();
    let state = json!({"intentId":"intent-1", "scope":"solo", "stages":{}});
    let graph = json!({"scopes":{"solo":[]},"stages":[]});
    let stage = json!({"id":"knowledge-writeback","inputs":{"context":["knowledge"]}});
    let context = build(ContextInput {
        root: &root,
        state: &state,
        stage: &stage,
        graph: &graph,
    })
    .unwrap();
    assert_eq!(
        context["knowledge"]["record"]["path"],
        ".aidlc/knowledge/intent-1.md"
    );
    assert_eq!(context["knowledge"]["record"]["exists"], true);
    assert_eq!(
        context["knowledge"]["record"]["frontmatter"]["id"],
        "intent-1"
    );
    assert!(context["knowledge"]["record"]["template"]
        .as_str()
        .unwrap()
        .contains("knowledge-record.md"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_manifest_warns_and_omits_unavailable_sources() {
    let root = temp();
    let state = json!({});
    let context = build(ContextInput {
        root: &root,
        state: &state,
        stage: &stage(Some(json!(["project", "sources"]))),
        graph: &json!({}),
    })
    .unwrap();
    assert_eq!(context["project"]["path"], "PROJECT.md");
    assert_eq!(context["sources"], json!([]));
    assert!(context["project"]["status"] == "missing");
    assert_eq!(
        context["warnings"],
        json!([
            "No aidlc.knowledge.json was found, so no project documentation is registered for agents. Run `dtech-aidlc onboard --harness <claude|deepseek>`.",
            "Source project path PROJECT.md does not exist"
        ])
    );
    assert_node_parity(&root, &state, &stage(Some(json!(["project", "sources"]))));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_changes_match_node_after_edits_additions_and_removals() {
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write(&root, "GUIDE.md", "# Guide\n");
    write(&root, "docs/a.md", "alpha\n");
    write(&root, "docs/b.md", "beta\n");
    write(&root, "docs/c.md", "gamma\n");
    write(&root, "docs/.hidden.md", "hidden\n");
    write(&root, "docs/node_modules/x.md", "vendored\n");
    write(
        &root,
        "aidlc.knowledge.json",
        r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"},{"id":"docs","path":"docs","kind":"docs"},{"id":"guide","path":"GUIDE.md","kind":"docs"},{"id":"gone","path":"gone/spec.md","kind":"requirements"}]}"#,
    );
    write(
        &root,
        ".aidlc/config.json",
        r#"{"harness":"claude","scope":"solo"}"#,
    );
    write_profile(&root, 2, "brownfield");
    node_snapshot(&root);
    let snapshot: Value = serde_json::from_slice(
        &fs::read(root.join(".aidlc/shared/sources-snapshot.json")).unwrap(),
    )
    .unwrap();
    let day = snapshot["scannedAt"].as_str().unwrap()[..10].to_owned();

    write(&root, "docs/a.md", "alpha, edited and longer\n");
    fs::remove_file(root.join("docs/b.md")).unwrap();
    for n in 0..22 {
        write(&root, &format!("docs/new-{n:02}.md"), "new\n");
    }

    let state = json!({"intentId":"intent-1"});
    let develop = json!({"id":"build","phase":"develop"});
    let context = assert_node_parity(&root, &state, &develop);
    let changes = &context["sourceChanges"];
    assert_eq!(changes["scanned"], true);
    assert_eq!(changes["scannedAt"], snapshot["scannedAt"]);
    assert_eq!(changes["changed"], json!(["docs/a.md"]));
    assert_eq!(changes["removed"], json!(["docs/b.md"]));
    assert_eq!(changes["added"].as_array().unwrap().len(), 20);
    assert_eq!(changes["added"][0], "docs/new-00.md");
    assert_eq!(changes["more"], json!({"changed":0,"added":2,"removed":0}));
    assert_eq!(
        changes["missing"],
        json!([{"id":"gone","path":"gone/spec.md"}])
    );
    assert_eq!(
        context["warnings"],
        json!([
            "Registered knowledge source gone (gone/spec.md) does not exist. Fix the path in aidlc.knowledge.json.",
            format!("24 registered documents changed since the last scan on {day} (docs/a.md, docs/b.md, docs/new-00.md, docs/new-01.md, docs/new-02.md). Re-read them before relying on earlier conclusions. Run `dtech-aidlc onboard --rescan` to acknowledge."),
            "Source gone path gone/spec.md does not exist"
        ])
    );
    assert_eq!(context["codebaseMap"]["required"], true);
    assert_eq!(context["codebaseMap"]["exists"], false);
    assert_eq!(
        context["codebaseMap"]["reason"],
        "project kind is brownfield"
    );
    assert_eq!(
        normalized(&context["codebaseMap"]["template"]),
        expected_template(&root, "codebase-map.md")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unchanged_snapshot_without_hashes_compares_node_mtimes_exactly() {
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write(&root, "docs/a.md", "alpha\n");
    write(&root, "docs/nested/b.md", "beta\n");
    write(
        &root,
        "aidlc.knowledge.json",
        r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"},{"id":"docs","path":"docs","kind":"docs"}]}"#,
    );
    write_profile(&root, 2, "greenfield");
    node_snapshot(&root);
    // Drop the hashes textually (re-serializing would round mtimeMs), so only
    // the size and mtimeMs comparison decides whether a file changed.
    let path = root.join(".aidlc/shared/sources-snapshot.json");
    let text = fs::read_to_string(&path).unwrap();
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("\"sha256\": \"") {
        out.push_str(&rest[..at]);
        out.push_str("\"sha256\": null");
        let tail = &rest[at + "\"sha256\": \"".len()..];
        rest = &tail[tail.find('"').unwrap() + 1..];
    }
    out.push_str(rest);
    assert!(out.contains("\"sha256\": null"));
    fs::write(&path, out).unwrap();

    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(context["sourceChanges"]["changed"], json!([]));
    assert_eq!(context["sourceChanges"]["added"], json!([]));
    assert_eq!(context["sourceChanges"]["removed"], json!([]));
    assert!(context["warnings"].as_array().unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unregistered_scaffold_project_matches_node_warnings() {
    let root = temp();
    write(&root, "PROJECT.md", &scaffold());
    write(
        &root,
        ".aidlc/config.json",
        r#"{"harness":"claude","scope":"team"}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(
        context["warnings"],
        json!([
            "PROJECT.md is still the unedited scaffold. Fill in the architecture, conventions, commands (build, test, lint, format, baseline), environment prerequisites and known baseline failures before Develop; every agent reads it first.",
            "No aidlc.knowledge.json was found, so no project documentation is registered for agents. Run `dtech-aidlc onboard --harness claude --scope team`."
        ])
    );
    assert_eq!(
        context["sourceChanges"],
        json!({"scanned":false,"scannedAt":null,"missing":[],"changed":[],"added":[],"removed":[],"more":{"changed":0,"added":0,"removed":0}})
    );
    assert_eq!(context["codebaseMap"]["required"], Value::Null);
    assert_eq!(context["codebaseMap"]["reason"], "project not scanned");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scanned_unregistered_project_without_detected_docs_has_no_manifest_warning() {
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write_profile(&root, 2, "greenfield");
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":1,"scannedAt":"2026-01-02T03:04:05.000Z","truncated":false,"files":[]}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert!(context["warnings"].as_array().unwrap().is_empty());
    assert_eq!(context["sourceChanges"]["scanned"], true);
    assert_eq!(
        context["sourceChanges"]["scannedAt"],
        "2026-01-02T03:04:05.000Z"
    );

    // Detected-but-unregistered documentation brings the warning back.
    write(
        &root,
        ".aidlc/shared/project-profile.json",
        r#"{"schemaVersion":2,"kind":"greenfield","documentation":{"detected":[{"path":"docs"}]}}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(
        context["warnings"],
        json!(["No aidlc.knowledge.json was found, so no project documentation is registered for agents. Run `dtech-aidlc onboard --harness <claude|deepseek>`."])
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outdated_profile_is_not_scanned_and_suggests_rescan() {
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write(
        &root,
        "aidlc.knowledge.json",
        r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]}"#,
    );
    write(
        &root,
        ".aidlc/config.json",
        r#"{"harness":"Bad Harness","scope":"solo"}"#,
    );
    write_profile(&root, 1, "brownfield");
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":1,"scannedAt":"2026-01-02T03:04:05.000Z","truncated":false,"files":[]}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(
        context["warnings"],
        json!(["This project has not been scanned on this machine by this version of dtech-aidlc, so the project profile and the document change check are unavailable. Run `dtech-aidlc onboard --rescan`."])
    );
    assert_eq!(context["sourceChanges"]["scanned"], false);
    assert_eq!(
        context["sourceChanges"]["scannedAt"],
        "2026-01-02T03:04:05.000Z"
    );
    assert_eq!(context["codebaseMap"]["required"], Value::Null);

    // A malformed languages list makes Node treat the profile as absent.
    write(
        &root,
        ".aidlc/shared/project-profile.json",
        r#"{"schemaVersion":2,"kind":"brownfield","languages":{"rust":true}}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(context["sourceChanges"]["scanned"], false);
    assert_eq!(context["codebaseMap"]["reason"], "project not scanned");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_and_truncated_snapshots_match_node() {
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write(&root, "docs/a.md", "alpha\n");
    write(
        &root,
        "aidlc.knowledge.json",
        r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"},{"id":"docs","path":"docs","kind":"docs"}]}"#,
    );
    write_profile(&root, 2, "greenfield");

    // Truncated: additions and removals are not reported; size changes are.
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":1,"scannedAt":"2026-03-04T00:00:00.000Z","truncated":true,"files":[{"path":"docs/a.md","bytes":1,"mtimeMs":5,"sha256":"00"},{"path":"docs/old.md","bytes":1,"mtimeMs":5,"sha256":null}]}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(context["sourceChanges"]["changed"], json!(["docs/a.md"]));
    assert_eq!(context["sourceChanges"]["added"], json!([]));
    assert_eq!(context["sourceChanges"]["removed"], json!([]));

    // Rows without a string path are carried through as Node does.
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":1,"scannedAt":7,"truncated":0,"files":[{"path":5},{"bytes":1},{"bytes":2},{"path":"docs/zz.md"}]}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(
        context["sourceChanges"]["removed"],
        json!([5, "docs/zz.md", null])
    );

    // A null row makes Node's sourceStatus throw; the section degrades.
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":1,"scannedAt":"x","truncated":false,"files":[null]}"#,
    );
    let rust = rust_context(&root, &json!({}), &stage(None), &json!({}));
    let node = node_context(&root, &json!({}), &stage(None), &json!({}));
    assert!(rust.get("sourceChanges").is_none());
    assert!(node.get("sourceChanges").is_none());
    assert_eq!(
        rust["warnings"],
        json!([
            "source changes could not be listed: Cannot read properties of null (reading 'path')"
        ])
    );
    assert_eq!(rust["warnings"], node["warnings"]);

    // Wrong schema version: the snapshot is ignored entirely.
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":2,"scannedAt":"x","files":[]}"#,
    );
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert_eq!(context["sourceChanges"]["scanned"], false);
    assert_eq!(context["sourceChanges"]["scannedAt"], Value::Null);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn codebase_map_status_matches_node_for_an_existing_map() {
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write_profile(&root, 2, "unknown");
    write(
        &root,
        ".aidlc/knowledge/_codebase-map.md",
        "# Codebase map\n\nmodules\n",
    );
    let grants = stage(Some(json!(["profile"])));
    let context = assert_node_parity(&root, &json!({}), &grants);
    let map = &context["codebaseMap"];
    assert_eq!(map["path"], ".aidlc/knowledge/_codebase-map.md");
    assert_eq!(map["exists"], true);
    assert_eq!(map["required"], true);
    assert_eq!(map["reason"], "project kind is unknown");
    assert_eq!(map["bytes"], 24);
    assert!(map["modifiedAt"].as_str().unwrap().ends_with('Z'));
    assert!(context["warnings"].as_array().unwrap().is_empty());

    // An empty map does not count as present.
    write(&root, ".aidlc/knowledge/_codebase-map.md", "");
    let context = assert_node_parity(&root, &json!({}), &grants);
    assert_eq!(context["codebaseMap"]["exists"], false);
    assert_eq!(context["codebaseMap"]["bytes"], 0);
    assert_eq!(context["codebaseMap"]["modifiedAt"], Value::Null);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn extra_required_artifacts_requires_codebase_map_for_existing_projects() {
    let root = temp();
    let stage_row = json!({"id":"context-and-risk"});
    assert!(extra_required_artifacts(&root, &stage_row).is_empty());
    write_profile(&root, 2, "brownfield");
    let rows = extra_required_artifacts(&root, &stage_row);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["path"], ".aidlc/knowledge/_codebase-map.md");
    let reason = rows[0]["reason"].as_str().unwrap().replace('\\', "/");
    assert_eq!(
        reason,
        format!(
            "the codebase map is required for an existing project (kind brownfield); write it from {}",
            expected_template(&root, "codebase-map.md")
        )
    );
    let node_rows = node(
        "context.mjs",
        "extraRequiredArtifacts",
        &json!({"root":root,"stage":stage_row}),
    );
    assert_eq!(node_rows[0]["path"], rows[0]["path"]);
    assert!(node_rows[0]["reason"].as_str().unwrap().starts_with(
        "the codebase map is required for an existing project (kind brownfield); write it from "
    ));
    assert!(extra_required_artifacts(&root, &json!({"id":"other"})).is_empty());
    write(&root, ".aidlc/knowledge/_codebase-map.md", "map\n");
    assert!(extra_required_artifacts(&root, &stage_row).is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_file_warnings_follow_node_rules() {
    let root = temp();
    let scaffold = scaffold();
    let partial = "# Project\n\nOur service.\n\n## Commands\n\ncargo test\n\n## Environment prerequisites\n\nWhat must be installed or running before code can be built or tested (for example Docker Desktop, a database, a language toolchain), and how to check it. Register a check with `dtech-aidlc config prerequisite set NAME --check \"COMMAND\"`.\n\n## Deeper docs\n";
    let crlf_bom = format!("\u{FEFF}{}", scaffold.replace('\n', "\r\n"));
    let cases = [
        ("scaffold.md", scaffold.as_str()),
        ("bom.md", crlf_bom.as_str()),
        ("owned.md", OWNED_PROJECT),
        ("partial.md", partial),
        ("headings.md", "## commands\n#\tnot a heading\nreal\n# Next\n## Environment prerequisites\n### sub\nyes\n## Known baseline failures\n##x\n"),
    ];
    for (name, text) in cases {
        write(&root, name, text);
        for developing in [false, true] {
            let rust = project_file_warnings(&root, name, developing);
            let node = node(
                "context.mjs",
                "projectFileWarnings",
                &json!({"root":root,"path":name,"developing":developing}),
            );
            assert_eq!(json!(rust), node, "{name} developing={developing}");
        }
    }
    assert_eq!(
        project_file_warnings(&root, "partial.md", true),
        vec!["partial.md has no content under '## Environment prerequisites', '## Known baseline failures'. Develop runs code: record the commands, the environment it needs and the failures already present, or the agents will rediscover them on every work unit.".to_owned()]
    );
    assert!(project_file_warnings(&root, "partial.md", false).is_empty());
    assert!(project_file_warnings(&root, "owned.md", true).is_empty());
    assert!(project_file_warnings(&root, "absent.md", true).is_empty());
    assert_eq!(project_file_warnings(&root, "bom.md", false).len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn develop_stage_reports_missing_project_sections_in_context() {
    let root = temp();
    write(&root, "PROJECT.md", "# Project\n\nOur service.\n");
    write(
        &root,
        "aidlc.knowledge.json",
        r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]}"#,
    );
    write_profile(&root, 2, "greenfield");
    write(
        &root,
        ".aidlc/shared/sources-snapshot.json",
        r#"{"schemaVersion":1,"scannedAt":"2026-01-02T03:04:05.000Z","truncated":false,"files":[]}"#,
    );
    let develop =
        json!({"id":"build","phase":"develop","inputs":{"context":["project","sources"]}});
    let context = assert_node_parity(&root, &json!({}), &develop);
    assert_eq!(
        context["warnings"],
        json!([
            "PROJECT.md has no content under '## Environment prerequisites', '## Commands', '## Known baseline failures'. Develop runs code: record the commands, the environment it needs and the failures already present, or the agents will rediscover them on every work unit.",
            "1 registered documents changed since the last scan on 2026-01-02 (PROJECT.md). Re-read them before relying on earlier conclusions. Run `dtech-aidlc onboard --rescan` to acknowledge."
        ])
    );
    assert_eq!(context["sourceChanges"]["added"], json!(["PROJECT.md"]));
    // Only the project grant: no source change status.
    let project_only = json!({"id":"build","phase":"develop","inputs":{"context":["project"]}});
    let context = assert_node_parity(&root, &json!({}), &project_only);
    assert!(context.get("sourceChanges").is_none());
    assert_eq!(context["warnings"].as_array().unwrap().len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_writeback_record_id_degrades_to_warning_like_node() {
    let root = temp();
    let state = json!({"intentId":"bad id"});
    let stage = json!({"id":"knowledge-writeback","inputs":{"context":["knowledge"]}});
    let context = assert_node_parity(&root, &state, &stage);
    assert_eq!(context["knowledge"], Value::Null);
    assert_eq!(
        context["warnings"],
        json!(["knowledge could not be listed: Invalid record id: use letters, digits, dot, underscore, or hyphen"])
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_manifest_degrades_to_sources_warning() {
    let root = temp();
    write(&root, "aidlc.knowledge.json", "{not json");
    let context = assert_node_parity(&root, &json!({}), &stage(None));
    assert!(context.get("project").is_none());
    assert!(context.get("sourceChanges").is_none());
    assert_eq!(
        context["warnings"],
        json!([
            "sources could not be listed: Invalid knowledge manifest: cannot be parsed as JSON"
        ])
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn symlink_file(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

#[test]
fn symlinked_project_files_are_not_read() {
    let root = temp();
    let outside = temp();
    fs::write(outside.join("PROJECT.md"), scaffold()).unwrap();
    if !symlink_file(&outside.join("PROJECT.md"), &root.join("PROJECT.md")) {
        eprintln!("skipping: symbolic links are not available");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
        return;
    }
    assert!(project_file_warnings(&root, "PROJECT.md", true).is_empty());
    fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
    assert!(symlink_file(
        &outside.join("PROJECT.md"),
        &root.join(".aidlc/knowledge/_codebase-map.md"),
    ));
    let context = rust_context(
        &root,
        &json!({}),
        &stage(Some(json!(["profile"]))),
        &json!({}),
    );
    assert_eq!(context["codebaseMap"]["exists"], false);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

/// Regression: with an external engine (`DTECH_AIDLC_HOME`) the codebase-map and
/// knowledge-record templates must point into that engine, as Node's `engineRoot()`
/// does, not into a `<project>/.aidlc/engine` that was never written. Node's test run
/// uses the repository engine `core/`, so with that root the whole context, templates
/// included, must equal Node's.
#[test]
fn external_engine_templates_point_into_the_engine_like_node() {
    let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core");
    let root = temp();
    write(&root, "PROJECT.md", OWNED_PROJECT);
    write(
        &root,
        "aidlc.knowledge.json",
        r#"{"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]}"#,
    );
    write_profile(&root, 2, "brownfield");
    write(&root, ".aidlc/knowledge/intent-1.md", "record");
    let state = json!({"intentId":"intent-1","scope":"solo","stages":{}});
    let graph = json!({"scopes":{"solo":[]},"stages":[]});
    // `closed` is the current time, taken separately by each implementation.
    let without_closed = |mut context: Value| {
        if let Some(frontmatter) = context["knowledge"]["record"]["frontmatter"].as_object_mut() {
            frontmatter.remove("closed");
        }
        context
    };
    for stage in [
        json!({"id":"problem-framing","phase":"analyze"}),
        json!({"id":"knowledge-writeback","phase":"operate"}),
    ] {
        let rust = build_with_engine(
            ContextInput {
                root: &root,
                state: &state,
                stage: &stage,
                graph: &graph,
            },
            &engine,
        )
        .unwrap();
        let node = node_context(&root, &state, &stage, &graph);
        assert_eq!(without_closed(rust), without_closed(node), "{stage}");
    }
    let stage = json!({"id":"knowledge-writeback","phase":"operate"});
    let rust = build_with_engine(
        ContextInput {
            root: &root,
            state: &state,
            stage: &stage,
            graph: &graph,
        },
        &engine,
    )
    .unwrap();
    for template in [
        &rust["codebaseMap"]["template"],
        &rust["knowledge"]["record"]["template"],
    ] {
        let path = PathBuf::from(template.as_str().unwrap());
        assert!(path.starts_with(&engine), "{template}");
        assert!(path.is_file(), "{template}");
    }
    assert!(!root.join(".aidlc/engine").exists());
    let rows =
        extra_required_artifacts_with_engine(&root, &json!({"id":"context-and-risk"}), &engine);
    let reason = rows[0]["reason"].as_str().unwrap();
    assert!(
        reason.ends_with(
            &engine
                .join("methodology")
                .join("templates")
                .join("codebase-map.md")
                .to_string_lossy()
                .to_string()
        ),
        "{reason}"
    );
    fs::remove_dir_all(root).unwrap();
}
