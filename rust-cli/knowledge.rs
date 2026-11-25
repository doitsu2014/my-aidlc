//! Native knowledge manifest and record operations corresponding to
//! `core/tools/knowledge.mjs`. These APIs preserve existing project data and
//! refuse writes through symlinks or outside the project root.
use serde_json::{json, Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const KNOWLEDGE_MANIFEST: &str = "aidlc.knowledge.json";
pub const SOURCE_KINDS: &[&str] = &[
    "project",
    "docs",
    "requirements",
    "architecture",
    "adr",
    "api-spec",
    "runbook",
    "conventions",
    "readme",
];
const IGNORE_BLOCK: &str = "# >>> dtech-aidlc managed block v1\n/state.json\n/state.v1.bak.json\n/audit.jsonl\n/config.json\n/shared/\n/repos/\n*.tmp\n# <<< dtech-aidlc managed block";

fn err<T>(message: impl Into<String>) -> Result<T, String> {
    Err(message.into())
}
fn project_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return err("Project root must be an absolute path");
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let meta = fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return err("Project root must be a directory");
    }
    Ok(root)
}
pub fn knowledge_manifest_path(root: &Path) -> Result<PathBuf, String> {
    Ok(project_root(root)?.join(KNOWLEDGE_MANIFEST))
}
pub fn knowledge_dir(root: &Path) -> Result<PathBuf, String> {
    Ok(project_root(root)?.join(".aidlc").join("knowledge"))
}
pub fn record_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    safe_id(id, "record id")?;
    Ok(knowledge_dir(root)?.join(format!("{id}.md")))
}
fn safe_id(id: &str, label: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 80
        || !id.as_bytes()[0].is_ascii_alphanumeric()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return err(format!("Invalid {label}: {id}"));
    }
    Ok(())
}
fn check_state_dir(root: &Path) -> Result<PathBuf, String> {
    let root = project_root(root)?;
    let state = root.join(".aidlc");
    match fs::symlink_metadata(&state) {
        Ok(m) if m.file_type().is_symlink() || !m.is_dir() => {
            return err("Refusing unsafe .aidlc path")
        }
        _ => {}
    }
    Ok(root)
}
fn check_components(root: &Path, relative: &str) -> Result<(), String> {
    let base = project_root(root)?;
    let mut current = base.clone();
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(p) => current.push(p),
            _ => return err("Unsafe relative path"),
        }
        if let Ok(meta) = fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                return err(format!("Refusing to write through symlink: {relative}"));
            }
        }
    }
    Ok(())
}
#[cfg(windows)]
fn replace_file_atomic(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    #[link(name = "kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }
    let to_wide: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let from_wide: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        if to.exists() {
            ReplaceFileW(
                to_wide.as_ptr(),
                from_wide.as_ptr(),
                ptr::null(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } else {
            MoveFileExW(from_wide.as_ptr(), to_wide.as_ptr(), 0x8)
        }
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = mode;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return err(format!(
                "Refusing to write through symlink: {}",
                path.display()
            ));
        }
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = path.with_extension(format!("{}.{}.tmp", std::process::id(), stamp));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))
                .map_err(|e| e.to_string())?;
        }
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        #[cfg(windows)]
        replace_file_atomic(&tmp, path).map_err(|e| e.to_string())?;
        #[cfg(not(windows))]
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
fn validate_path(id: &str, path: &Value) -> Result<(), String> {
    let p = path.as_str().ok_or_else(|| format!("source {id}: path must be a relative path inside the project root using forward slashes"))?;
    if p.is_empty()
        || p.len() > 400
        || p.contains('\0')
        || p.contains('\\')
        || p.starts_with('/')
        || p.starts_with("//")
        || p.as_bytes().get(1) == Some(&b':')
        || p.split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || matches!(p.split('/').next(), Some(".git" | ".aidlc"))
    {
        return err(format!("source {id}: path must be a relative path inside the project root using forward slashes"));
    }
    Ok(())
}
pub fn validate_knowledge_manifest(data: &Value) -> Result<(), String> {
    let fail = |s: String| format!("Invalid knowledge manifest: {s}");
    let obj = data
        .as_object()
        .ok_or_else(|| fail("expected schemaVersion 1 and a sources array".into()))?;
    if data["schemaVersion"].as_i64() != Some(1) || !data["sources"].is_array() {
        return err(fail("expected schemaVersion 1 and a sources array".into()));
    }
    if let Some(k) = obj
        .keys()
        .find(|k| *k != "schemaVersion" && *k != "sources")
    {
        return err(fail(format!("unknown key {k}")));
    }
    let sources = data["sources"].as_array().unwrap();
    if sources.len() > 50 {
        return err(fail("at most 50 sources are allowed".into()));
    }
    let mut ids = std::collections::HashSet::new();
    let mut projects = 0;
    for source in sources {
        let o = source
            .as_object()
            .ok_or_else(|| fail("each source must be an object".into()))?;
        let id = source
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        if let Some(k) = o
            .keys()
            .find(|k| !["id", "path", "kind", "description"].contains(&k.as_str()))
        {
            return err(fail(format!("source {id}: unknown key {k}")));
        }
        if safe_id(
            source.get("id").and_then(Value::as_str).unwrap_or(""),
            "source id",
        )
        .is_err()
        {
            return err(fail(
                "source id is invalid: use letters, digits, dot, underscore, or hyphen".into(),
            ));
        }
        if !ids.insert(id.to_owned()) {
            return err(fail(format!("source {id}: duplicate id")));
        }
        let kind = source.get("kind").and_then(Value::as_str).unwrap_or("");
        if !SOURCE_KINDS.contains(&kind) {
            return err(fail(format!(
                "source {id}: kind must be one of {}",
                SOURCE_KINDS.join(", ")
            )));
        }
        if kind == "project" {
            projects += 1;
        }
        if let Some(d) = source.get("description") {
            if !d.is_string()
                || d.as_str().unwrap().contains(['\r', '\n'])
                || d.as_str().unwrap().len() > 300
            {
                return err(fail(format!(
                    "source {id}: description must be one line of at most 300 characters"
                )));
            }
        }
        validate_path(id, source.get("path").unwrap_or(&Value::Null)).map_err(fail)?;
    }
    if projects > 1 {
        return err(fail("only one source may have kind project".into()));
    }
    Ok(())
}
pub fn find_project_file(root: &Path) -> Option<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let ty = e.file_type().ok()?;
            let name = e.file_name().into_string().ok()?;
            (ty.is_file() && name.eq_ignore_ascii_case("project.md")).then_some(name)
        })
        .collect();
    names.sort();
    if names.iter().any(|n| n == "PROJECT.md") {
        Some("PROJECT.md".into())
    } else {
        names.into_iter().next()
    }
}
pub fn default_knowledge_manifest(root: &Path) -> Value {
    json!({"schemaVersion":1,"sources":[{"id":"project","path":find_project_file(root).unwrap_or_else(||"PROJECT.md".into()),"kind":"project"}]})
}
pub fn load_knowledge_manifest(root: &Path) -> Result<(bool, Value), String> {
    let path = knowledge_manifest_path(root)?;
    let value = match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((false, default_knowledge_manifest(root)))
        }
        Err(e) => return err(e.to_string()),
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => {
            return err("Refusing unsafe knowledge manifest path")
        }
        Ok(_) => serde_json::from_slice(
            &fs::read(path).map_err(|_| "Invalid knowledge manifest: cannot be parsed as JSON")?,
        )
        .map_err(|_| "Invalid knowledge manifest: cannot be parsed as JSON")?,
    };
    validate_knowledge_manifest(&value)?;
    Ok((true, value))
}
pub fn write_knowledge_manifest(root: &Path, manifest: &Value) -> Result<(), String> {
    validate_knowledge_manifest(manifest)?;
    let root = check_state_dir(root)?;
    check_components(&root, KNOWLEDGE_MANIFEST)?;
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    atomic_write(&root.join(KNOWLEDGE_MANIFEST), &bytes, 0o644)
}
pub fn inspect_sources(root: &Path, manifest: &Value) -> Value {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    for source in manifest
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut row = source.clone();
        let p = source["path"].as_str().unwrap_or("");
        let abs = p.split('/').fold(root.clone(), |mut a, part| {
            a.push(part);
            a
        });
        let (status, size) = match fs::symlink_metadata(abs) {
            Ok(m) if m.file_type().is_symlink() => {
                warnings.push(format!(
                    "Source {} is a symbolic link",
                    source["id"].as_str().unwrap_or("undefined")
                ));
                ("symlink", Value::Null)
            }
            Ok(m) if m.is_dir() => ("ok", Value::Null),
            Ok(m) if m.is_file() => ("ok", json!(m.len())),
            Ok(_) => ("unsafe", Value::Null),
            Err(_) => {
                warnings.push(format!(
                    "Source {} path {} does not exist",
                    source["id"].as_str().unwrap_or("undefined"),
                    p
                ));
                ("missing", Value::Null)
            }
        };
        if let Some(o) = row.as_object_mut() {
            o.insert("status".into(), json!(status));
            o.insert("size".into(), size);
        }
        sources.push(row);
    }
    let project = sources
        .iter()
        .find(|s| s["kind"] == "project")
        .cloned()
        .unwrap_or(Value::Null);
    json!({"sources":sources,"project":project,"warnings":warnings})
}
pub fn add_source(
    root: &Path,
    id: &str,
    path: &str,
    kind: &str,
    description: Option<&str>,
) -> Result<Value, String> {
    let (exists, mut manifest) = load_knowledge_manifest(root)?;
    if manifest["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == id)
    {
        return err(format!("Source '{id}' already exists"));
    }
    let mut source = json!({"id":id,"path":path,"kind":kind});
    if let Some(d) = description.filter(|d| !d.is_empty()) {
        source["description"] = json!(d);
    }
    manifest["sources"]
        .as_array_mut()
        .unwrap()
        .push(source.clone());
    write_knowledge_manifest(root, &manifest)?;
    let _ = exists;
    Ok(source)
}
pub fn remove_source(root: &Path, id: &str) -> Result<Value, String> {
    let (exists, mut manifest) = load_knowledge_manifest(root)?;
    if !exists {
        return err("No aidlc.knowledge.json");
    }
    let rows = manifest["sources"].as_array_mut().unwrap();
    let before = rows.len();
    rows.retain(|s| s["id"] != id);
    if before == rows.len() {
        return err(format!("Unknown source '{id}'"));
    }
    write_knowledge_manifest(root, &manifest)?;
    Ok(json!({"removed":id}))
}
pub fn knowledge_preflight(root: &Path) -> Result<(bool, Value), String> {
    let root = check_state_dir(root)?;
    let path = root.join(KNOWLEDGE_MANIFEST);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return err("Refusing to write through symlink: aidlc.knowledge.json");
    }
    load_knowledge_manifest(&root)
}
fn project_template(root: &Path) -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("DTECH_AIDLC_HOME") {
        let p = PathBuf::from(home);
        for c in [p.join("core"), p] {
            let f = c.join("data/scaffold/PROJECT.md");
            if f.exists() {
                return Ok(f);
            }
        }
    }
    let p = root.join("data/scaffold/PROJECT.md");
    if p.exists() {
        Ok(p)
    } else {
        err(format!("Missing scaffold file: {}", p.display()))
    }
}
pub fn init_knowledge(root: &Path, create_manifest: bool) -> Result<Value, String> {
    let root = project_root(root)?;
    let (exists, manifest) = knowledge_preflight(&root)?;
    let warnings = Vec::<String>::new();
    let target = if exists {
        manifest["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "project")
            .and_then(|s| s["path"].as_str())
            .map(str::to_owned)
    } else {
        Some(find_project_file(&root).unwrap_or_else(|| "PROJECT.md".into()))
    };
    let project = if exists && target.is_none() {
        json!({"path":null,"created":false,"reason":"no project source declared"})
    } else {
        let rel = target.as_ref().unwrap();
        validate_path("project", &json!(rel))?;
        let abs = rel.split('/').fold(root.clone(), |mut p, s| {
            p.push(s);
            p
        });
        match fs::symlink_metadata(&abs) {
            Ok(_) => json!({"path":rel,"created":false,"reason":"already exists"}),
            Err(_) if !abs.parent().is_some_and(Path::exists) => {
                json!({"path":rel,"created":false,"reason":"parent directory does not exist"})
            }
            Err(_) => {
                let template = project_template(&root)?;
                let bytes = fs::read(template).map_err(|e| e.to_string())?;
                match OpenOptions::new().write(true).create_new(true).open(&abs) {
                    Ok(mut f) => {
                        f.write_all(&bytes).map_err(|e| e.to_string())?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            f.set_permissions(fs::Permissions::from_mode(0o644))
                                .map_err(|e| e.to_string())?;
                        }
                        json!({"path":rel,"created":true})
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        json!({"path":rel,"created":false,"reason":"already exists"})
                    }
                    Err(e) => return err(e.to_string()),
                }
            }
        }
    };
    let manifest_status = if exists {
        "unchanged"
    } else if create_manifest {
        write_knowledge_manifest(
            &root,
            &json!({"schemaVersion":1,"sources":[{"id":"project","path":target,"kind":"project"}]}),
        )?;
        "created"
    } else {
        "absent"
    };
    check_components(&root, ".aidlc/knowledge")?;
    fs::create_dir_all(root.join(".aidlc/knowledge")).map_err(|e| e.to_string())?;
    Ok(
        json!({"project":project,"manifest":KNOWLEDGE_MANIFEST,"manifestStatus":manifest_status,"knowledgeDir":".aidlc/knowledge","warnings":warnings}),
    )
}
pub fn ensure_knowledge_dir(root: &Path) -> Result<(), String> {
    let root = check_state_dir(root)?;
    check_components(&root, ".aidlc/knowledge")?;
    fs::create_dir_all(root.join(".aidlc/knowledge")).map_err(|e| e.to_string())
}
#[derive(Debug, Clone)]
pub struct ParsedRecord {
    pub frontmatter: Value,
    pub lists: Value,
    pub body: String,
    pub title: String,
    pub headings: Map<String, Value>,
}
pub fn parse_record(text: &str) -> Result<ParsedRecord, String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().copied() != Some("---") {
        return err("frontmatter is missing");
    }
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| **l == "---")
        .map(|(i, _)| i)
        .ok_or("frontmatter is missing")?;
    let mut fm = Map::new();
    for line in &lines[1..end] {
        if let Some((k, v)) = line.split_once(':') {
            let value = v.trim().trim_matches(['\'', '\"']).to_owned();
            fm.insert(k.trim().to_owned(), json!(value));
        }
    }
    let body = lines[end + 1..].join("\n");
    let title = lines[end + 1..]
        .iter()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or("")
        .trim()
        .to_owned();
    let mut headings = Map::new();
    let mut current: Option<String> = None;
    let mut buf = Vec::new();
    let flush = |current: &Option<String>, buf: &Vec<String>, headings: &mut Map<String, Value>| {
        if let Some(k) = current {
            headings.insert(k.clone(), json!(buf.join("\n").trim()));
        }
    };
    for line in lines.iter().skip(end + 1) {
        if let Some(h) = line.strip_prefix("## ") {
            flush(&current, &buf, &mut headings);
            current = Some(h.trim().into());
            buf.clear();
        } else if line.starts_with("# ") {
            flush(&current, &buf, &mut headings);
            current = None;
            buf.clear();
        } else if current.is_some() {
            buf.push((*line).to_owned());
        }
    }
    flush(&current, &buf, &mut headings);
    let list = |key: &str| -> Value {
        let s = fm
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']');
        json!(s
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>())
    };
    let lists = json!({"phases-run":list("phases-run"),"supersedes":list("supersedes"),"tags":list("tags")});
    Ok(ParsedRecord {
        frontmatter: Value::Object(fm),
        lists,
        body,
        title,
        headings,
    })
}
pub fn read_records(root: &Path) -> Value {
    let Ok(dir) = knowledge_dir(root) else {
        return json!({"records":[],"invalid":[]});
    };
    let mut records = Vec::new();
    let mut invalid = Vec::new();
    let Ok(meta) = fs::symlink_metadata(&dir) else {
        return json!({"records":records,"invalid":invalid});
    };
    if meta.file_type().is_symlink() {
        return json!({"records":records,"invalid":invalid});
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return json!({"records":records,"invalid":invalid});
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name.starts_with('_') || name.starts_with('.') {
            continue;
        }
        let rel = format!(".aidlc/knowledge/{name}");
        let Ok(m) = entry.file_type() else {
            continue;
        };
        if m.is_symlink() || !m.is_file() {
            invalid.push(json!({"file":rel,"errors":["is a symbolic link or not a regular file"]}));
            continue;
        }
        let stem = name.trim_end_matches(".md");
        if safe_id(stem, "record id").is_err() {
            invalid.push(json!({"file":rel,"errors":["file name is not a valid record id"]}));
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            invalid.push(json!({"file":rel,"errors":["cannot be read as UTF-8"]}));
            continue;
        };
        match parse_record(&text) { Err(_) => records.push(json!({"id":stem,"title":stem,"closed":null,"origin":"legacy","scope":null,"tags":[],"supersedes":[],"warnings":["no frontmatter; add it using the knowledge-record template"],"file":rel})), Ok(p)=> records.push(json!({"id":p.frontmatter["id"].as_str().filter(|s|!s.is_empty()).unwrap_or(stem),"title":p.frontmatter["title"].as_str().filter(|s|!s.is_empty()).unwrap_or(&p.title),"origin":p.frontmatter["origin"].as_str().unwrap_or("archived"),"scope":p.frontmatter.get("scope").and_then(Value::as_str),"closed":p.frontmatter.get("closed").and_then(Value::as_str),"tags":p.lists["tags"],"supersedes":p.lists["supersedes"],"file":rel,"parsed":{"frontmatter":p.frontmatter,"lists":p.lists,"body":p.body,"title":p.title,"headings":p.headings}})) }
    }
    json!({"records":records,"invalid":invalid})
}
pub fn write_index(root: &Path) -> Result<Value, String> {
    ensure_knowledge_dir(root)?;
    let data = read_records(root);
    let records = data["records"].as_array().unwrap();
    let invalid = data["invalid"].as_array().unwrap();
    let mut lines = vec![
        "# Knowledge index".to_owned(),
        "".into(),
        format!("Records: {}. Invalid: {}.", records.len(), invalid.len()),
        "".into(),
    ];
    for r in records {
        lines.push(format!(
            "- {} — {} ({}{})",
            r["id"].as_str().unwrap_or(""),
            r["title"].as_str().unwrap_or(""),
            r["origin"].as_str().unwrap_or(""),
            r["closed"]
                .as_str()
                .map(|s| format!(", closed {s}"))
                .unwrap_or_default()
        ));
    }
    for r in invalid {
        lines.push(format!(
            "- INVALID {}: {}",
            r["file"].as_str().unwrap_or(""),
            r["errors"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    atomic_write(
        &knowledge_dir(root)?.join("_index.md"),
        format!("{}\n", lines.join("\n")).as_bytes(),
        0o644,
    )?;
    Ok(json!({"records":records,"invalid":invalid,"index":".aidlc/knowledge/_index.md"}))
}
pub fn expected_record_frontmatter(graph: &Value, state: &Value) -> Value {
    let phases = graph["scopes"][state["scope"].as_str().unwrap_or("")]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let run: Vec<String> = phases
        .iter()
        .filter_map(Value::as_str)
        .filter(|phase| {
            graph["stages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| s["phase"] == *phase)
                .any(|s| {
                    let status = state["stages"][s["id"].as_str().unwrap_or("")]["status"].as_str();
                    matches!(status, Some("complete" | "skipped"))
                        || s["id"] == "knowledge-writeback"
                })
        })
        .map(str::to_owned)
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    json!({"id":state["intentId"],"origin":"intent","closed":format_utc(now),"scope":state["scope"],"phases-run":run.join(", ")})
}
fn format_utc(seconds: u64) -> String {
    let days = (seconds / 86400) as i64;
    let sec = (seconds % 86400) as u32;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        sec / 3600,
        (sec % 3600) / 60,
        sec % 60
    )
}
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    (y, m as u32, d as u32)
}
fn knowledge_template_path(root: &Path) -> PathBuf {
    if let Some(home) = std::env::var_os("DTECH_AIDLC_HOME") {
        let p = PathBuf::from(home);
        for c in [p.join("core"), p] {
            let f = c.join("methodology/templates/knowledge-record.md");
            if f.exists() {
                return f;
            }
        }
    }
    root.join("methodology/templates/knowledge-record.md")
}
pub fn knowledge_gate_check(root: &Path, graph: &Value, state: &Value) -> Value {
    let id = state["intentId"].as_str().unwrap_or("");
    let rel = format!(".aidlc/knowledge/{id}.md");
    let template = knowledge_template_path(root);
    let path = match record_path(root, id) {
        Ok(p) => p,
        Err(_) => {
            return json!({"ok":false,"path":rel,"template":template,"errors":["missing file"]})
        }
    };
    let _ = graph;
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(_) => {
            return json!({"ok":false,"path":rel,"template":template,"errors":["missing file"]})
        }
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return json!({"ok":false,"path":rel,"template":template,"errors":["is a symbolic link or not a regular file"]});
    }
    let parsed = match fs::read_to_string(path)
        .map_err(|_| ())
        .and_then(|s| parse_record(&s).map_err(|_| ()))
    {
        Ok(p) => p,
        Err(_) => {
            return json!({"ok":false,"path":rel,"template":template,"errors":["frontmatter is missing"]})
        }
    };
    let mut errors = Vec::new();
    if parsed.frontmatter["id"] != id {
        errors.push(format!("id must be {id}"));
    }
    if parsed.frontmatter["origin"] != "intent" {
        errors.push("origin must be intent".into());
    }
    if parsed.frontmatter["title"]
        .as_str()
        .unwrap_or("")
        .is_empty()
        || parsed.frontmatter["title"]
            .as_str()
            .unwrap_or("")
            .starts_with('<')
    {
        errors.push("title is empty".into());
    }
    if parsed.title.starts_with('<') {
        errors.push("title is empty".into());
    }
    if parsed.frontmatter["closed"]
        .as_str()
        .unwrap_or("")
        .is_empty()
    {
        errors.push("closed must be a date such as 2026-11-20 or 2026-11-20T10:15:00Z".into());
    }
    if parsed.frontmatter["scope"]
        .as_str()
        .is_some_and(|s| s != state["scope"].as_str().unwrap_or(""))
    {
        errors.push(format!(
            "scope must be {}",
            state["scope"].as_str().unwrap_or("undefined")
        ));
    }
    for h in ["Outcome", "Decisions", "Limitations", "Follow-ups", "Links"] {
        if !parsed.headings.contains_key(h) {
            errors.push(format!("missing heading {h}"));
        }
    }
    json!({"ok":errors.is_empty(),"path":rel,"template":template,"errors":errors})
}
pub fn ensure_ignore_file(root: &Path) -> Result<Value, String> {
    let root = check_state_dir(root)?;
    check_components(&root, ".aidlc/.gitignore")?;
    fs::create_dir_all(root.join(".aidlc")).map_err(|e| e.to_string())?;
    let path = root.join(".aidlc/.gitignore");
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            atomic_write(&path, format!("{IGNORE_BLOCK}\n").as_bytes(), 0o600)?;
            return Ok(json!({"path":".aidlc/.gitignore","result":"created"}));
        }
        Err(e) => return err(e.to_string()),
    };
    let lines: Vec<&str> = text
        .split('\n')
        .map(|x| x.strip_suffix('\r').unwrap_or(x))
        .collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with("# >>> dtech-aidlc managed block"))
        .map(|(i, _)| i)
        .collect();
    let end = lines
        .iter()
        .position(|l| l.starts_with("# <<< dtech-aidlc managed block"));
    let next = if starts.len() == 1 && end.is_some_and(|e| e > starts[0]) {
        format!(
            "{}{}{}{}",
            lines[..starts[0]].join("\n"),
            if starts[0] > 0 { "\n" } else { "" },
            IGNORE_BLOCK,
            if end.unwrap() + 1 < lines.len() {
                format!("\n{}", lines[end.unwrap() + 1..].join("\n"))
            } else {
                String::new()
            }
        )
    } else {
        let rest = lines
            .iter()
            .filter(|l| {
                !l.starts_with("# >>> dtech-aidlc managed block")
                    && !l.starts_with("# <<< dtech-aidlc managed block")
            })
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        format!("{IGNORE_BLOCK}\n\n{}", rest.trim_start_matches('\n'))
    };
    let next = if next.ends_with('\n') {
        next
    } else {
        format!("{next}\n")
    };
    if next == text || next == format!("{text}\n") {
        return Ok(json!({"path":".aidlc/.gitignore","result":"unchanged"}));
    }
    atomic_write(&path, next.as_bytes(), 0o600)?;
    Ok(json!({"path":".aidlc/.gitignore","result":"updated"}))
}
pub fn check_records(root: &Path) -> Value {
    let data = read_records(root);
    let mut problems: Vec<String> = data["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            format!(
                "{}: {}",
                x["file"].as_str().unwrap_or(""),
                x["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })
        .collect();
    let records = data["records"].as_array().unwrap();
    for r in records {
        if r["origin"] == "archived" && r.get("parsed").is_some() {
            for h in ["Outcome", "Decisions", "Limitations", "Follow-ups", "Links"] {
                if !r["parsed"]["headings"]
                    .as_object()
                    .is_some_and(|o| o.contains_key(h))
                {
                    problems.push(format!(
                        "{}: missing heading {h}",
                        r["id"].as_str().unwrap_or("")
                    ));
                }
            }
        }
    }
    json!({"ok":problems.is_empty(),"problems":problems,"records":records.len()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_manifest_safety_and_shape() {
        assert!(validate_knowledge_manifest(&json!({"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]})).is_ok());
        for path in [
            "../outside",
            ".aidlc/state.json",
            "C:/outside",
            "foo\\bar",
            "/absolute",
        ] {
            assert!(
                validate_knowledge_manifest(
                    &json!({"schemaVersion":1,"sources":[{"id":"x","path":path,"kind":"docs"}]})
                )
                .is_err(),
                "{path}"
            );
        }
        assert!(validate_knowledge_manifest(
            &json!({"schemaVersion":1,"sources":[],"unexpected":true})
        )
        .is_err());
    }
    #[test]
    fn manifest_validation_matches_node_shape_rules() {
        for data in [
            json!({"schemaVersion": 1.0, "sources": []}),
            json!({"schemaVersion": 1, "sources": [{"id": "x", "path": "//server/share", "kind": "docs"}]}),
            json!({"schemaVersion": 1, "sources": [{"id": "x", "path": "a", "kind": "docs", "description": "one\ntwo"}]}),
            json!({"schemaVersion": 1, "sources": [{"id": "x", "path": "a", "kind": "docs", "extra": true}]}),
            json!({"schemaVersion": 1, "sources": [{"id": "x", "path": "a", "kind": "docs"}, {"id": "x", "path": "b", "kind": "docs"}]}),
        ] {
            assert!(validate_knowledge_manifest(&data).is_err(), "{data}");
        }
        assert!(validate_knowledge_manifest(&json!({"schemaVersion": 1, "sources": [{"id": "a..", "path": "folder/file.md", "kind": "docs"}]})).is_ok());
    }

    #[test]
    fn atomic_write_refuses_manifest_symlink_without_touching_target() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let root =
                std::env::temp_dir().join(format!("knowledge-symlink-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let target = root.join("user-data.json");
            fs::write(&target, "preserve me").unwrap();
            symlink(&target, root.join(KNOWLEDGE_MANIFEST)).unwrap();
            assert!(
                write_knowledge_manifest(&root, &json!({"schemaVersion":1,"sources":[]})).is_err()
            );
            assert_eq!(fs::read_to_string(&target).unwrap(), "preserve me");
            assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn parses_record_frontmatter_and_sections() {
        let p=parse_record("---\nid: intent-1\ntags: one, two\n---\n# Title\n\n## Outcome\nDone\n## Follow-ups\nNone\n").unwrap();
        assert_eq!(p.frontmatter["id"], "intent-1");
        assert_eq!(p.lists["tags"], json!(["one", "two"]));
        assert!(p.headings.contains_key("Outcome"));
        assert_eq!(p.title, "Title");
        assert!(parse_record("# no frontmatter").is_err());
    }
    #[test]
    fn manifest_roundtrip_and_symlink_refusal() {
        let root = std::env::temp_dir().join(format!("knowledge-rust-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data = json!({"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]});
        write_knowledge_manifest(&root, &data).unwrap();
        assert!(load_knowledge_manifest(&root).unwrap().0);
        assert_eq!(load_knowledge_manifest(&root).unwrap().1, data);
        assert!(add_source(&root, "project", "other.md", "docs", None).is_err());
        assert_eq!(
            remove_source(&root, "project").unwrap(),
            json!({"removed":"project"})
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn read_write_records_index_and_ignore() {
        let root = std::env::temp_dir().join(format!("knowledge-records-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
        fs::write(root.join(".aidlc/knowledge/a.md"),"---\nid: a\ntitle: Alpha\norigin: archived\nclosed: 2026-01-01\ntags: x, y\n---\n# Alpha\n## Outcome\nOK\n## Decisions\nD\n## Limitations\nL\n## Follow-ups\nF\n## Links\nNone\n").unwrap();
        let data = read_records(&root);
        assert_eq!(data["records"].as_array().unwrap().len(), 1);
        assert!(check_records(&root)["ok"].as_bool().unwrap());
        assert!(write_index(&root).is_ok());
        assert_eq!(ensure_ignore_file(&root).unwrap()["result"], "created");
        let _ = fs::remove_dir_all(root);
    }
}
