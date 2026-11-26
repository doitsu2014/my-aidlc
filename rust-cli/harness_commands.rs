//! Native `harness add|status|refresh` commands, ported from the Node router
//! (`core/tools/dtech-aidlc.mjs`) and `core/tools/projection.mjs`.
//!
//! Harness manifests and skill sources come from the assets embedded in this
//! executable (`crate::scope::embedded_asset`), never from `core/` at runtime.
//! The projection history used to recognise earlier engine releases is compiled in.
//!
//! Every write refuses symlinked components, replaces files through a temp file
//! and rename, and is preceded by the same validation the Node CLI performs.

use crate::args::FlagValue;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Engine version stamped into projections (Node reads it from package.json).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROJECTION_HISTORY: &str = include_str!("../core/data/projection-history.json");
const KNOWN_HARNESSES: &[&str] = &["claude", "deepseek"];
const SCOPES: &[&str] = &["solo", "team", "enterprise"];
const REFUSED: &[&str] = &["refused-ahead", "refused-not-a-regular-file"];

/// Handle `harness add NAME`, `harness status` and `harness refresh [NAME]`.
///
/// `key` is the command key from `args::command_key` (`harness`, `harness add`, ...),
/// `args` are the positional arguments after the command words. The returned value is
/// printed as pretty JSON; the exit code is 1 when a refresh refused a file (Node sets
/// `process.exitCode = 1` after printing the result).
pub fn run(
    key: &str,
    project: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let action = key
        .strip_prefix("harness")
        .map(str::trim_start)
        .unwrap_or("");
    match action {
        "add" => harness_add(&root, args.first().map(String::as_str)).map(|value| (value, 0)),
        "status" => harness_status(&root).map(|value| (value, 0)),
        "refresh" => harness_refresh(
            &root,
            args.first()
                .map(String::as_str)
                .filter(|name| !name.is_empty()),
            flag_set(flags, "dry-run"),
            flag_set(flags, "force"),
        ),
        _ => Err("Use harness status or harness refresh".into()),
    }
}

fn flag_set(flags: &BTreeMap<String, FlagValue>, name: &str) -> bool {
    matches!(flags.get(name), Some(FlagValue::Boolean(true)))
}

// ---------------------------------------------------------------------------
// Harness discovery (Node lib.mjs listHarnesses / configuredHarnesses / loadHarnessManifest)
// ---------------------------------------------------------------------------

/// Harnesses bundled with this executable, sorted by name.
pub fn list_harnesses() -> Vec<&'static str> {
    KNOWN_HARNESSES
        .iter()
        .copied()
        .filter(|name| {
            crate::scope::embedded_asset(&format!("harness/{name}/manifest.json")).is_some()
        })
        .collect()
}

/// Node `configuredHarnesses(config)`: a valid `harnesses` list, else the single `harness`.
pub fn configured_harnesses(config: &Value) -> Vec<String> {
    let names = list_harnesses();
    if let Some(list) = config.get("harnesses").and_then(Value::as_array) {
        if !list.is_empty()
            && list
                .iter()
                .all(|name| name.as_str().is_some_and(|name| names.contains(&name)))
        {
            return list
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
        }
    }
    match config.get("harness").and_then(Value::as_str) {
        Some(name) if names.contains(&name) => vec![name.to_owned()],
        _ => Vec::new(),
    }
}

/// Error text of Node `validateHarness` for an unknown or missing name.
pub fn harness_choice_error() -> String {
    format!("harness must be {}", list_harnesses().join(" or "))
}

fn safe_relative(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        !value.is_empty()
            && !value.starts_with('/')
            && !value.starts_with('\\')
            && !(value.len() >= 2 && value.as_bytes()[1] == b':')
            && !value.split(['/', '\\']).any(|part| part == "..")
    })
}

/// Load and validate a bundled harness manifest (Node `loadHarnessManifest`).
pub fn harness_manifest(name: &str) -> Result<Value, String> {
    let invalid = |reason: &str| format!("Invalid harness manifest for {name}: {reason}");
    let bytes = crate::scope::embedded_asset(&format!("harness/{name}/manifest.json"))
        .ok_or_else(|| invalid("cannot read JSON"))?;
    let manifest: Value = serde_json::from_slice(bytes).map_err(|_| invalid("cannot read JSON"))?;
    validate_harness_manifest(name, manifest)
}

/// The checks of [`harness_manifest`] on the parsed manifest of the bundled harness
/// `name` (file sources are looked up among the embedded harness files).
fn validate_harness_manifest(name: &str, manifest: Value) -> Result<Value, String> {
    let invalid = |reason: &str| format!("Invalid harness manifest for {name}: {reason}");
    if manifest.get("schemaVersion").and_then(Value::as_f64) != Some(1.0) {
        return Err(invalid("schemaVersion must be 1"));
    }
    if manifest.get("name").and_then(Value::as_str) != Some(name) {
        return Err(invalid("name must match its folder"));
    }
    if !safe_relative(str_of(&manifest, "skillDir")) {
        return Err(invalid("skillDir must be a safe relative path"));
    }
    let files = manifest
        .get("files")
        .and_then(Value::as_array)
        .filter(|files| !files.is_empty())
        .ok_or_else(|| invalid("files must be a non-empty array"))?;
    let mut targets = Vec::new();
    for file in files {
        let source = str_of(file, "source");
        if !safe_relative(source) {
            return Err(invalid("source must be a safe relative path"));
        }
        let source = source.unwrap_or_default();
        if crate::scope::embedded_asset(&format!("harness/{name}/{source}")).is_none() {
            return Err(invalid(&format!("source is missing: {source}")));
        }
        let target = str_of(file, "target");
        if !safe_relative(target) {
            return Err(invalid("target must be a safe relative path"));
        }
        let target = target.unwrap_or_default();
        if targets.contains(&target) {
            return Err(invalid(&format!("target is repeated: {target}")));
        }
        targets.push(target);
    }
    if let Some(obsolete) = manifest.get("obsoleteFiles") {
        if !obsolete
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| safe_relative(row.as_str())))
        {
            return Err(invalid("obsoleteFiles must contain safe relative paths"));
        }
    }
    if let Some(legacy) = manifest.get("legacy") {
        let skill_dir = str_of(&manifest, "skillDir");
        let ok = legacy.as_array().is_some_and(|rows| {
            rows.iter().all(|entry| {
                safe_relative(str_of(entry, "dir"))
                    && str_of(entry, "dir") != skill_dir
                    && entry
                        .get("files")
                        .and_then(Value::as_array)
                        .is_some_and(|files| files.iter().all(|f| safe_relative(f.as_str())))
            })
        });
        if !ok {
            return Err(invalid("legacy paths must be safe relative paths"));
        }
    }
    if !manifest
        .get("modelAliases")
        .and_then(Value::as_array)
        .is_some_and(|aliases| {
            aliases.iter().all(|alias| {
                alias
                    .as_str()
                    .is_some_and(|alias| crate::scope::MODEL_ALIASES.contains(&alias))
            })
        })
    {
        return Err(invalid(
            "modelAliases must be an array of known model aliases",
        ));
    }
    Ok(manifest)
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn manifest_files(manifest: &Value) -> Vec<(String, String)> {
    manifest
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|file| {
            (
                str_of(file, "source").unwrap_or_default().to_owned(),
                str_of(file, "target").unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn source_bytes(name: &str, source: &str) -> Result<&'static [u8], String> {
    crate::scope::embedded_asset(&format!("harness/{name}/{source}"))
        .ok_or_else(|| format!("Invalid harness manifest for {name}: source is missing: {source}"))
}

// ---------------------------------------------------------------------------
// Path, hashing and write helpers
// ---------------------------------------------------------------------------

/// Join a `/`- or `\`-separated relative path onto `root` component by component,
/// producing native separators like Node `path.join`.
fn join_rel(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in relative
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        path.push(part);
    }
    path
}

fn rel_posix(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn parent_rel(relative: &str) -> &str {
    relative.rfind(['/', '\\']).map_or("", |i| &relative[..i])
}

fn basename(relative: &str) -> &str {
    relative.rsplit(['/', '\\']).next().unwrap_or(relative)
}

/// Node `hashText`: SHA-256 of the UTF-8 text with CRLF normalised to LF.
pub fn hash_text(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// JavaScript `String(value ?? '')` for the version strings compared below.
fn version_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Node `compareVersions`: compare the leading `MAJOR.MINOR.PATCH` (missing = 0.0.0).
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn parse(value: &str) -> [u64; 3] {
        let mut parts = [0u64; 3];
        let mut rest = value;
        for (index, part) in parts.iter_mut().enumerate() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return [0, 0, 0];
            }
            *part = rest[..digits].parse().unwrap_or(u64::MAX);
            rest = &rest[digits..];
            if index < 2 {
                match rest.strip_prefix('.') {
                    Some(next) => rest = next,
                    None => return [0, 0, 0],
                }
            }
        }
        parts
    }
    parse(a).cmp(&parse(b))
}

fn history() -> &'static Value {
    static HISTORY: OnceLock<Value> = OnceLock::new();
    HISTORY.get_or_init(|| {
        serde_json::from_str(PROJECTION_HISTORY)
            .unwrap_or_else(|_| json!({"harness":{},"methodology":{}}))
    })
}

fn history_has(map: Option<&Value>, key: &str, hash: &str) -> bool {
    map.and_then(|map| map.get(key))
        .and_then(Value::as_array)
        .is_some_and(|hashes| hashes.iter().any(|known| known.as_str() == Some(hash)))
}

fn assert_no_symlinks(root: &Path, relative: &str) -> Result<(), String> {
    crate::workflow::assert_no_symlink_components(root, relative)
}

/// Node `writeFileAtomic(path, bytes, { mode })`: refuse a symlinked target, write a
/// unique sibling temp file and rename it over the target.
fn write_atomic(
    path: &Path,
    bytes: &[u8],
    #[allow(unused_variables)] mode: u32,
) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(format!(
            "Refusing to write through symlink: {}",
            path.display()
        ));
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or("atomic write target has no file name")?
        .to_string_lossy()
        .into_owned();
    let unique = format!(
        "{:x}{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let temp = path.with_file_name(format!("{name}.{}.{unique}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    let result = (|| {
        let mut file = options
            .open(&temp)
            .map_err(|error| format!("{}: {error}", temp.display()))?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        let mut attempt = 0;
        loop {
            match fs::rename(&temp, path) {
                Ok(()) => return Ok(()),
                Err(error)
                    if cfg!(windows)
                        && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                        && attempt < 20 =>
                {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(error) => return Err(format!("{}: {error}", path.display())),
            }
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn create_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Projection stamp and status (Node projection.mjs)
// ---------------------------------------------------------------------------

fn stamp_path(root: &Path, manifest: &Value) -> PathBuf {
    join_rel(root, str_of(manifest, "skillDir").unwrap_or_default()).join(".projection.json")
}

/// Node `readStamp`: the projection stamp, or `None` when missing, symlinked or invalid.
fn read_stamp(root: &Path, manifest: &Value) -> Option<Value> {
    let path = stamp_path(root, manifest);
    if fs::symlink_metadata(&path).ok()?.file_type().is_symlink() {
        return None;
    }
    let stamp: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
    (stamp.get("schemaVersion").and_then(Value::as_f64) == Some(1.0)).then_some(stamp)
}

fn stamp_files(stamp: Option<&Value>) -> Map<String, Value> {
    stamp
        .and_then(|stamp| stamp.get("files"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn stamp_hash<'a>(stamp: Option<&'a Value>, file: &str) -> Option<&'a str> {
    stamp?.get("files")?.get(file)?.get("sha256")?.as_str()
}

/// Node `writeStamp`: skip identical content, otherwise replace atomically.
fn write_stamp(root: &Path, manifest: &Value, stamp: &Value) -> Result<(), String> {
    let path = stamp_path(root, manifest);
    let mut text = serde_json::to_string_pretty(stamp).map_err(|error| error.to_string())?;
    text.push('\n');
    if fs::read(&path).is_ok_and(|existing| existing == text.as_bytes()) {
        return Ok(());
    }
    write_atomic(&path, text.as_bytes(), 0o644)
}

/// Node `projectionStatus(root, manifest, engineVersion)` as its JSON shape.
pub fn projection_status(
    root: &Path,
    manifest: &Value,
    engine_version: &str,
) -> Result<Value, String> {
    let name = str_of(manifest, "name").unwrap_or_default();
    let skill_dir = str_of(manifest, "skillDir").unwrap_or_default();
    let stamp = read_stamp(root, manifest);
    let harness_history = history().get("harness").and_then(|h| h.get(name));
    let mut files = Vec::new();
    for (source, target) in manifest_files(manifest) {
        let path = join_rel(root, &format!("{skill_dir}/{target}"));
        assert_no_symlinks(root, &format!("{skill_dir}/{}", parent_rel(&target)))?;
        let state = if !path.exists() {
            "missing"
        } else {
            match fs::symlink_metadata(&path) {
                Err(_) => "missing",
                Ok(meta) if !meta.is_file() => "modified",
                Ok(_) => match fs::read(&path) {
                    Err(_) => "missing",
                    Ok(bytes) => {
                        let hash = hash_text(&bytes);
                        if hash == hash_text(source_bytes(name, &source)?) {
                            "current"
                        } else if stamp_hash(stamp.as_ref(), &target) == Some(hash.as_str())
                            || history_has(harness_history, basename(&target), &hash)
                        {
                            "behind"
                        } else {
                            "modified"
                        }
                    }
                },
            }
        };
        files.push(json!({"path": target, "state": state}));
    }
    let mut candidates: Vec<(String, String)> = manifest
        .get("obsoleteFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|file| (skill_dir.to_owned(), file.to_owned()))
        .collect();
    for entry in manifest
        .get("legacy")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let dir = str_of(entry, "dir").unwrap_or_default();
        for file in entry
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(file) = file.as_str() {
                candidates.push((dir.to_owned(), file.to_owned()));
            }
        }
    }
    let mut obsolete = Vec::new();
    for (dir, file) in candidates {
        let path = join_rel(root, &format!("{dir}/{file}"));
        assert_no_symlinks(root, &dir)?;
        if !path.exists() {
            continue;
        }
        let mut state = "modified";
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()) {
            if let Ok(bytes) = fs::read(&path) {
                let hash = hash_text(&bytes);
                // `stamp?.files?.[file]?.sha256 ?? history[basename(file)]?.find(...)`
                let prior = match stamp_hash(stamp.as_ref(), &file) {
                    Some(hash) => Some(hash.to_owned()),
                    None => {
                        history_has(harness_history, basename(&file), &hash).then(|| hash.clone())
                    }
                };
                if prior.as_deref() == Some(hash.as_str()) {
                    state = "pristine";
                }
            }
        }
        obsolete.push(json!({"path": rel_posix(root, &path), "state": state}));
    }
    let stamped = stamp
        .as_ref()
        .and_then(|stamp| stamp.get("engineVersion"))
        .cloned()
        .unwrap_or(Value::Null);
    let states = |rows: &[Value]| -> Vec<String> {
        rows.iter()
            .map(|row| row["state"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    let file_states = states(&files);
    let obsolete_states = states(&obsolete);
    let ahead = compare_versions(&version_text(&stamped), engine_version).is_gt()
        && file_states.iter().any(|state| state != "current");
    let state = if ahead {
        "ahead"
    } else if file_states.iter().any(|s| s == "missing") {
        "missing"
    } else if file_states
        .iter()
        .chain(&obsolete_states)
        .any(|s| s == "modified")
    {
        "modified"
    } else if file_states.iter().any(|s| s == "behind")
        || obsolete_states.iter().any(|s| s == "pristine")
    {
        "behind"
    } else {
        "current"
    };
    let mut out = Map::new();
    out.insert("harness".into(), json!(name));
    if let Some(invoke) = manifest.get("invoke") {
        out.insert("invoke".into(), invoke.clone());
    }
    out.insert("skillDir".into(), json!(skill_dir));
    out.insert("state".into(), json!(state));
    out.insert("stampedEngineVersion".into(), stamped);
    out.insert("files".into(), Value::Array(files));
    out.insert("obsolete".into(), Value::Array(obsolete));
    Ok(Value::Object(out))
}

/// Node `updateProjectionStamp`: record the hashes of files that match the bundled source
/// and advance the stamped engine version only when every file is current.
pub fn update_projection_stamp(
    root: &Path,
    manifest: &Value,
    engine_version: &str,
) -> Result<Value, String> {
    let name = str_of(manifest, "name").unwrap_or_default();
    let skill_dir = str_of(manifest, "skillDir").unwrap_or_default();
    let existing = read_stamp(root, manifest);
    let mut files = stamp_files(existing.as_ref());
    let mut current = true;
    for (source, target) in manifest_files(manifest) {
        let path = join_rel(root, &format!("{skill_dir}/{target}"));
        let hash = hash_text(source_bytes(name, &source)?);
        let matches = fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
            && fs::read(&path).is_ok_and(|bytes| hash_text(&bytes) == hash);
        if matches {
            files.insert(target, json!({"sha256": hash}));
        } else {
            current = false;
        }
    }
    let existing_version = existing
        .as_ref()
        .and_then(|stamp| stamp.get("engineVersion"))
        .cloned()
        .unwrap_or(Value::Null);
    let version =
        if current && compare_versions(&version_text(&existing_version), engine_version).is_le() {
            json!(engine_version)
        } else {
            existing_version
        };
    let stamp =
        json!({"schemaVersion": 1, "harness": name, "engineVersion": version, "files": files});
    write_stamp(root, manifest, &stamp)?;
    Ok(stamp)
}

/// Node `projectHarnessFiles`: copy each bundled file whose target does not exist yet.
/// Returns the absolute paths created.
pub fn project_harness_files(root: &Path, manifest: &Value) -> Result<Vec<PathBuf>, String> {
    let name = str_of(manifest, "name").unwrap_or_default();
    let skill_dir = str_of(manifest, "skillDir").unwrap_or_default();
    assert_no_symlinks(root, skill_dir)?;
    let mut created = Vec::new();
    for (source, target) in manifest_files(manifest) {
        let rel = format!("{skill_dir}/{target}");
        assert_no_symlinks(root, parent_rel(&rel))?;
        let path = join_rel(root, &rel);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing to write through symlink: {}",
                    path.display()
                ))
            }
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
        create_parent(&path)?;
        write_atomic(&path, source_bytes(name, &source)?, 0o644)?;
        created.push(path);
    }
    Ok(created)
}

fn action(path: String, action: &str) -> Value {
    json!({"path": path, "action": action})
}

/// Node `refreshProjection(root, manifest, engineVersion, { dryRun, force })`.
pub fn refresh_projection(
    root: &Path,
    manifest: &Value,
    engine_version: &str,
    dry_run: bool,
    force: bool,
) -> Result<Value, String> {
    let name = str_of(manifest, "name").unwrap_or_default();
    let skill_dir = str_of(manifest, "skillDir").unwrap_or_default();
    let legacy_dirs: Vec<String> = manifest
        .get("legacy")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| str_of(entry, "dir").map(str::to_owned))
        .collect();
    assert_no_symlinks(root, skill_dir)?;
    for dir in &legacy_dirs {
        assert_no_symlinks(root, dir)?;
    }
    let before = projection_status(root, manifest, engine_version)?;
    let prior = read_stamp(root, manifest);
    let before_state = before["state"].as_str().unwrap_or_default().to_owned();
    if !dry_run && !(before_state == "ahead" && !force) && refresh_changes_files(&before, force) {
        // Changed files are audited as `projection.refreshed` after the writes; refuse a
        // log that cannot be appended to before anything changes.
        crate::audit::preflight_audit(root)?;
    }
    let mut actions: Vec<Value> = Vec::new();
    if before_state == "ahead" && !force {
        actions.push(action(skill_dir.to_owned(), "refused-ahead"));
    } else {
        for (source, target) in manifest_files(manifest) {
            let full = join_rel(root, &format!("{skill_dir}/{target}"));
            let rel = rel_posix(root, &full);
            let state = before["files"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["path"] == target.as_str()))
                .and_then(|row| row["state"].as_str())
                .unwrap_or("missing");
            match state {
                "missing" | "behind" => {
                    actions.push(action(
                        rel,
                        if state == "missing" {
                            "created"
                        } else {
                            "updated"
                        },
                    ));
                    if !dry_run {
                        create_parent(&full)?;
                        write_atomic(&full, source_bytes(name, &source)?, 0o644)?;
                    }
                }
                "modified" => {
                    if !fs::symlink_metadata(&full).is_ok_and(|meta| meta.is_file()) {
                        actions.push(action(rel, "refused-not-a-regular-file"));
                        continue;
                    }
                    let backup = format!("{}.local-backup", full.display());
                    let mut backup_path = PathBuf::from(&backup);
                    let mut n = 2;
                    while backup_path.exists() {
                        backup_path = PathBuf::from(format!("{backup}.{n}"));
                        n += 1;
                    }
                    if !force {
                        actions.push(action(rel, "kept-modified"));
                    } else {
                        actions.push(json!({"path": rel, "action": "replaced-with-backup", "backup": rel_posix(root, &backup_path)}));
                        if !dry_run {
                            let local = fs::read(&full)
                                .map_err(|error| format!("{}: {error}", full.display()))?;
                            write_atomic(&backup_path, &local, 0o644)?;
                            write_atomic(&full, source_bytes(name, &source)?, 0o644)?;
                        }
                    }
                }
                _ => {}
            }
        }
        let skill_abs = join_rel(root, skill_dir);
        for file in before["obsolete"].as_array().into_iter().flatten() {
            let path = file["path"].as_str().unwrap_or_default().to_owned();
            if file["state"] == "modified" {
                actions.push(action(path, "kept-modified-obsolete"));
                continue;
            }
            actions.push(action(path.clone(), "removed"));
            if !dry_run {
                let full = join_rel(root, &path);
                fs::remove_file(&full).map_err(|error| format!("{}: {error}", full.display()))?;
                let mut dir = full.parent().map(Path::to_path_buf);
                while let Some(current) = dir {
                    if current == skill_abs || !current.starts_with(root) {
                        break;
                    }
                    if fs::remove_dir(&current).is_err() {
                        break;
                    }
                    actions.push(action(rel_posix(root, &current), "removed-directory"));
                    if legacy_dirs
                        .iter()
                        .any(|legacy| join_rel(root, legacy) == current)
                    {
                        break;
                    }
                    dir = current.parent().map(Path::to_path_buf);
                }
            }
        }
        if !dry_run {
            let targets: Vec<String> = manifest_files(manifest)
                .into_iter()
                .map(|(_, t)| t)
                .collect();
            let mut next_files: Map<String, Value> = stamp_files(prior.as_ref())
                .into_iter()
                .filter(|(path, _)| targets.contains(path))
                .collect();
            for (source, target) in manifest_files(manifest) {
                let path = join_rel(root, &format!("{skill_dir}/{target}"));
                let hash = hash_text(source_bytes(name, &source)?);
                if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
                    && fs::read(&path).is_ok_and(|bytes| hash_text(&bytes) == hash)
                {
                    next_files.insert(target, json!({"sha256": hash}));
                }
            }
            let next = json!({"schemaVersion": 1, "harness": name, "engineVersion": engine_version, "files": next_files});
            write_stamp(root, manifest, &next)?;
            let pick = |kinds: &[&str]| -> Vec<Value> {
                actions
                    .iter()
                    .filter(|row| row["action"].as_str().is_some_and(|a| kinds.contains(&a)))
                    .map(|row| row["path"].clone())
                    .collect()
            };
            if !pick(&["created", "updated", "replaced-with-backup", "removed"]).is_empty() {
                crate::audit::audit(
                    root,
                    "projection.refreshed",
                    json!({
                        "harness": name,
                        "from": prior.as_ref().and_then(|p| p.get("engineVersion")).cloned().unwrap_or(Value::Null),
                        "to": engine_version,
                        "created": pick(&["created"]),
                        "updated": pick(&["updated", "replaced-with-backup"]),
                        "removed": pick(&["removed"]),
                        "keptModified": pick(&["kept-modified", "kept-modified-obsolete"]),
                    }),
                )?;
            }
        }
    }
    let has = |kind: &str| actions.iter().any(|row| row["action"] == kind);
    let state = if has("refused-ahead") {
        "ahead".to_owned()
    } else if has("refused-not-a-regular-file") {
        "modified".to_owned()
    } else if dry_run {
        before_state.clone()
    } else {
        projection_status(root, manifest, engine_version)?["state"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    Ok(json!({
        "harness": name,
        "from": prior.as_ref().and_then(|p| p.get("engineVersion")).cloned().unwrap_or(Value::Null),
        "to": engine_version,
        "stateBefore": before_state,
        "state": state,
        "actions": actions,
    }))
}

/// Whether a (non-dry-run) refresh from `status` creates, replaces or removes a file,
/// and so appends a `projection.refreshed` audit row.
fn refresh_changes_files(status: &Value, force: bool) -> bool {
    let files = status["files"].as_array().into_iter().flatten();
    let obsolete = status["obsolete"].as_array().into_iter().flatten();
    files
        .map(|row| row["state"].as_str().unwrap_or("missing"))
        .any(|state| matches!(state, "missing" | "behind") || (force && state == "modified"))
        || obsolete.into_iter().any(|row| row["state"] != "modified")
}

// ---------------------------------------------------------------------------
// Legacy `.aidlc/methodology` copies (Node legacyMethodologyStatus / refreshLegacyMethodology)
// ---------------------------------------------------------------------------

fn sorted_entries(dir: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    #[allow(unused_mut)]
    let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    // libuv sorts scandir results bytewise on POSIX; Windows keeps the NTFS order.
    #[cfg(not(windows))]
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

/// Node `legacyMethodologyStatus(root)`: `null` when `.aidlc/methodology` is absent.
pub fn legacy_methodology_status(root: &Path) -> Value {
    let base = root.join(".aidlc").join("methodology");
    if !base.exists() {
        return Value::Null;
    }
    let methodology = history().get("methodology");
    let mut pristine = Vec::new();
    let mut modified = Vec::new();
    fn walk(
        dir: &Path,
        prefix: &str,
        methodology: Option<&Value>,
        pristine: &mut Vec<String>,
        modified: &mut Vec<String>,
    ) -> std::io::Result<()> {
        for entry in sorted_entries(dir)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let Ok(meta) = fs::symlink_metadata(&path) else {
                modified.push(rel);
                continue;
            };
            if meta.file_type().is_symlink() {
                modified.push(rel);
            } else if meta.is_dir() {
                walk(&path, &rel, methodology, pristine, modified)?;
            } else if meta.is_file()
                && history_has(methodology, &rel, &hash_text(&fs::read(&path)?))
            {
                pristine.push(rel);
            } else {
                modified.push(rel);
            }
        }
        Ok(())
    }
    if walk(&base, "", methodology, &mut pristine, &mut modified).is_err() {
        modified.push(".".into());
    }
    json!({"pristine": pristine, "modified": modified})
}

/// Node `refreshLegacyMethodology(root, { dryRun })`: delete pristine legacy copies.
pub fn refresh_legacy_methodology(root: &Path, dry_run: bool) -> Result<Value, String> {
    let base = root.join(".aidlc").join("methodology");
    assert_no_symlinks(root, ".aidlc/methodology")?;
    let status = legacy_methodology_status(root);
    if status.is_null() {
        return Ok(json!({"removed": [], "keptModified": []}));
    }
    let methodology = history().get("methodology");
    if !dry_run
        && status["pristine"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    {
        crate::audit::preflight_audit(root)?;
    }
    let mut removed = Vec::new();
    for rel in status["pristine"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let path = join_rel(&base, rel);
        let pristine = fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
            && fs::read(&path).is_ok_and(|bytes| history_has(methodology, rel, &hash_text(&bytes)));
        if !pristine {
            continue;
        }
        if !dry_run && fs::remove_file(&path).is_err() {
            continue;
        }
        removed.push(rel.to_owned());
    }
    if !dry_run && !removed.is_empty() {
        fn prune(dir: &Path) {
            if let Ok(entries) = sorted_entries(dir) {
                for entry in entries {
                    let path = entry.path();
                    if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                        prune(&path);
                    }
                }
            }
            let _ = fs::remove_dir(dir);
        }
        prune(&base);
        crate::audit::audit(
            root,
            "methodology.legacy.cleaned",
            json!({"removed": removed}),
        )?;
    }
    Ok(json!({"removed": removed, "keptModified": status["modified"]}))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn config_path(root: &Path) -> PathBuf {
    root.join(".aidlc").join("config.json")
}

/// Node `loadConfig`: `readJson(configPath, {})`, lenient about unreadable or invalid JSON.
fn load_config_lenient(root: &Path) -> Value {
    fs::read(config_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn not_initialized() -> String {
    format!(
        "Project is not initialized; run: dtech-aidlc onboard --harness {} --scope <solo|team|enterprise>",
        list_harnesses().join("|")
    )
}

fn harness_add(root: &Path, name: Option<&str>) -> Result<Value, String> {
    let name = name
        .filter(|name| list_harnesses().contains(name))
        .ok_or_else(harness_choice_error)?;
    if !config_path(root).exists() {
        return Err(not_initialized());
    }
    let mut config = load_config_lenient(root);
    let configured = configured_harnesses(&config);
    if configured.is_empty()
        || !config
            .get("scope")
            .and_then(Value::as_str)
            .is_some_and(|scope| SCOPES.contains(&scope))
    {
        return Err(
            "Invalid .aidlc/config.json: harness or scope is missing or unknown; correct the file"
                .into(),
        );
    }
    let manifest = harness_manifest(name)?;
    let skill_dir = str_of(&manifest, "skillDir").unwrap_or_default().to_owned();
    assert_no_symlinks(root, &skill_dir)?;
    // Refuse a symlinked config before projecting anything when a config write will follow.
    let path = config_path(root);
    let will_write = !configured.iter().any(|item| item == name);
    if will_write && fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(format!(
            "Refusing to write through symlink: {}",
            path.display()
        ));
    }
    if will_write {
        // `harness.added` is audited after the writes; refuse a log that cannot be
        // appended to before anything changes (Node writes, then fails on the append).
        crate::audit::preflight_audit(root)?;
    }
    let created = project_harness_files(root, &manifest)?;
    update_projection_stamp(root, &manifest, ENGINE_VERSION)?;
    let projection = join_rel(root, &skill_dir).display().to_string();
    if will_write {
        let mut harnesses: Vec<Value> = configured.iter().map(|item| json!(item)).collect();
        harnesses.push(json!(name));
        config["harnesses"] = Value::Array(harnesses.clone());
        crate::workflow::write_json_atomic(&path, &config)?;
        crate::audit::audit(root, "harness.added", json!({"harness": name}))?;
        return Ok(
            json!({"harness": name, "harnesses": harnesses, "projection": projection, "filesCreated": created.len()}),
        );
    }
    Ok(
        json!({"harness": name, "harnesses": configured, "projection": projection, "filesCreated": created.len()}),
    )
}

fn harness_status(root: &Path) -> Result<Value, String> {
    let harnesses = if config_path(root).exists() {
        configured_harnesses(&load_config_lenient(root))
    } else {
        Vec::new()
    };
    let mut rows = Vec::new();
    for name in harnesses {
        rows.push(projection_status(
            root,
            &harness_manifest(&name)?,
            ENGINE_VERSION,
        )?);
    }
    Ok(json!({
        "engineVersion": ENGINE_VERSION,
        "harnesses": rows,
        "legacyMethodology": legacy_methodology_status(root),
    }))
}

fn harness_refresh(
    root: &Path,
    name: Option<&str>,
    dry_run: bool,
    force: bool,
) -> Result<(Value, i32), String> {
    let configured = if config_path(root).exists() {
        configured_harnesses(&load_config_lenient(root))
    } else {
        Vec::new()
    };
    if configured.is_empty() {
        return Err(not_initialized());
    }
    if let Some(name) = name {
        if !configured.iter().any(|item| item == name) {
            return Err(format!(
                "Harness {name} is not configured for this project (configured: {})",
                configured.join(", ")
            ));
        }
    }
    let selected: Vec<String> = match name {
        Some(name) => vec![name.to_owned()],
        None => configured,
    };
    // Load (validate) every manifest before writing anything.
    let manifests = selected
        .iter()
        .map(|item| harness_manifest(item))
        .collect::<Result<Vec<_>, _>>()?;
    let mut harnesses = Vec::new();
    for manifest in &manifests {
        harnesses.push(refresh_projection(
            root,
            manifest,
            ENGINE_VERSION,
            dry_run,
            force,
        )?);
    }
    let actions: Vec<&str> = harnesses
        .iter()
        .flat_map(|row| row["actions"].as_array().into_iter().flatten())
        .filter_map(|row| row["action"].as_str())
        .collect();
    let legacy = refresh_legacy_methodology(root, dry_run)?;
    let needs_attention = actions.iter().any(|action| {
        matches!(
            *action,
            "kept-modified"
                | "kept-modified-obsolete"
                | "refused-ahead"
                | "refused-not-a-regular-file"
        )
    }) || legacy["keptModified"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty());
    let code = i32::from(actions.iter().any(|action| REFUSED.contains(action)));
    Ok((
        json!({
            "dryRun": dry_run,
            "engineVersion": ENGINE_VERSION,
            "harnesses": harnesses,
            "legacyMethodology": legacy,
            "needsAttention": needs_attention,
        }),
        code,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "aidlc-harness-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn no_flags() -> BTreeMap<String, FlagValue> {
        BTreeMap::new()
    }

    #[test]
    fn versions_compare_like_node() {
        assert!(compare_versions("0.10.0", "0.9.9").is_gt());
        assert!(compare_versions("", "0.0.0").is_eq());
        assert!(compare_versions("1.2", "0.0.1").is_lt());
        assert!(compare_versions("1.2.3-beta", "1.2.3").is_eq());
    }

    #[test]
    fn bundled_manifests_validate() {
        assert_eq!(list_harnesses(), vec!["claude", "deepseek"]);
        for name in list_harnesses() {
            harness_manifest(name).unwrap();
        }
        assert!(harness_manifest("nope").is_err());
    }

    #[test]
    fn add_requires_known_harness_and_initialized_project() {
        let t = Temp::new();
        let error = run("harness add", &t.0, &["nope".into()], &no_flags()).unwrap_err();
        assert_eq!(error, "harness must be claude or deepseek");
        let error = run("harness add", &t.0, &["claude".into()], &no_flags()).unwrap_err();
        assert!(error.starts_with("Project is not initialized"));
        assert!(!t.0.join(".claude").exists());
    }

    #[test]
    fn add_projects_files_stamps_and_records_harness() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join(".aidlc")).unwrap();
        fs::write(
            t.0.join(".aidlc/config.json"),
            r#"{"harness":"claude","scope":"solo","custom":1}"#,
        )
        .unwrap();
        let (out, code) = run("harness add", &t.0, &["deepseek".into()], &no_flags()).unwrap();
        assert_eq!(code, 0);
        assert_eq!(out["harnesses"], json!(["claude", "deepseek"]));
        assert_eq!(out["filesCreated"], 2);
        let config: Value =
            serde_json::from_slice(&fs::read(t.0.join(".aidlc/config.json")).unwrap()).unwrap();
        assert_eq!(config["custom"], 1);
        assert_eq!(config["harnesses"], json!(["claude", "deepseek"]));
        let (status, _) = run("harness status", &t.0, &[], &no_flags()).unwrap();
        assert_eq!(status["harnesses"][1]["state"], "current");
        assert_eq!(status["harnesses"][0]["state"], "missing");
        let (refresh, code) = run("harness refresh", &t.0, &[], &no_flags()).unwrap();
        assert_eq!(code, 0);
        assert_eq!(refresh["harnesses"][0]["state"], "current");
    }

    #[test]
    fn refresh_keeps_local_edits_unless_forced() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join(".aidlc")).unwrap();
        fs::write(
            t.0.join(".aidlc/config.json"),
            r#"{"harness":"claude","scope":"solo"}"#,
        )
        .unwrap();
        run("harness refresh", &t.0, &[], &no_flags()).unwrap();
        let skill = t.0.join(".claude/skills/dtech-aidlc/SKILL.md");
        fs::write(&skill, "local edit\n").unwrap();
        let (kept, _) = run("harness refresh", &t.0, &[], &no_flags()).unwrap();
        assert_eq!(
            kept["harnesses"][0]["actions"][0]["action"],
            "kept-modified"
        );
        assert_eq!(kept["needsAttention"], true);
        let mut force = no_flags();
        force.insert("force".into(), FlagValue::Boolean(true));
        let (replaced, _) = run("harness refresh", &t.0, &[], &force).unwrap();
        assert_eq!(
            replaced["harnesses"][0]["actions"][0]["action"],
            "replaced-with-backup"
        );
        assert_eq!(
            fs::read_to_string(t.0.join(".claude/skills/dtech-aidlc/SKILL.md.local-backup"))
                .unwrap(),
            "local edit\n"
        );
    }

    /// Former Node test (harness-manifest.test.mjs): a manifest with an unknown or
    /// missing model alias is rejected.
    #[test]
    fn manifest_with_unknown_or_missing_model_alias_is_rejected() {
        let shipped = harness_manifest("claude").unwrap();
        assert_eq!(
            shipped["modelAliases"],
            serde_json::json!(["opus", "sonnet", "haiku", "fable"])
        );
        assert_eq!(
            harness_manifest("deepseek").unwrap()["modelAliases"],
            serde_json::json!([])
        );
        assert_eq!(
            validate_harness_manifest("claude", shipped.clone()).unwrap(),
            shipped
        );
        let error = "Invalid harness manifest for claude: \
                     modelAliases must be an array of known model aliases";
        let with = |aliases: Value| {
            let mut manifest = shipped.clone();
            manifest["modelAliases"] = aliases;
            validate_harness_manifest("claude", manifest)
        };
        assert_eq!(with(serde_json::json!(["opus", "gpt"])), Err(error.into()));
        assert_eq!(with(serde_json::json!(["opus", 1])), Err(error.into()));
        assert_eq!(with(serde_json::json!("opus")), Err(error.into()));
        assert_eq!(with(Value::Null), Err(error.into()));
        assert!(with(serde_json::json!([])).is_ok());
        let mut without = shipped.clone();
        without.as_object_mut().unwrap().remove("modelAliases");
        assert_eq!(
            validate_harness_manifest("claude", without),
            Err(error.into())
        );
    }
}
