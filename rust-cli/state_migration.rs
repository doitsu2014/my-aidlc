//! Explicit migration of the legacy (version 1) `.aidlc/state.json` workflow state.
//!
//! This module is standalone by design and mirrors Node's `migrateLegacy` /
//! `loadWorkflow(..., { migrate: true })` in `core/tools/workflow.mjs`.
//!
//! Safety contract (all platforms, including Windows):
//! - Every validation, symlink and conflict check runs before the first write, so malformed
//!   state, unsafe paths and conflicts leave the project untouched.
//! - The original bytes are preserved in `.aidlc/state.v1.bak.json` first. An existing backup
//!   is reused only when it holds exactly those bytes; a different backup is never replaced.
//! - The per-intent state and then the v2 pointer are written as a temp file in the same
//!   directory followed by a rename. Nothing is deleted before it is replaced.
//! - A rerun after an interrupted migration (intent written, pointer still legacy) reuses
//!   the intent folder when it holds the same intent (same `intent` and `createdAt`).
//! - The `state.migrated` audit row uses Node's JSONL shape:
//!   `{"at":ISO,"event":"state.migrated","from":1,"intentId":ID}`.
//!
//! Deliberate fail-closed differences from Node: a different backup, or an intent folder
//! holding a different (or unreadable) state for an explicit `intentId`, is refused instead
//! of being silently kept/overwritten; a derived ID that cannot be computed byte-for-byte
//! like Node (Unicode outside the supported Latin/Vietnamese set) is refused and asks for an
//! explicit `intentId`; Windows reserved device names are refused as intent IDs.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub type MigrationResult<T> = Result<T, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct Migration {
    pub intent_id: String,
    /// The per-intent state exactly as written to `.aidlc/intents/<id>/state.json`.
    pub state: Value,
}

const STATUSES: [&str; 4] = ["in_progress", "complete", "blocked", "skipped"];

/// Migrate an existing legacy state file. `graph` is the workflow graph (`scopes`, `phases`,
/// `stages`). Returns an error, with no file written, when the state is already a v2
/// pointer, malformed, unsafe, or conflicts with existing files.
pub fn migrate_legacy_state(root: impl AsRef<Path>, graph: &Value) -> MigrationResult<Migration> {
    let root = root.as_ref();
    let aidlc = root.join(".aidlc");
    let pointer_path = aidlc.join("state.json");

    // ---- Preflight: nothing below writes until every check has passed. ----
    assert_safe_state_dir(root)?;
    ensure_safe(root, &pointer_path)?;
    let original = fs::read(&pointer_path).map_err(ioerr)?;
    let parsed: Value = serde_json::from_slice(&original)
        .map_err(|_| ".aidlc/state.json cannot be parsed; no changes were made".to_owned())?;
    if is_v2_pointer(&parsed) {
        return Err("State is already version 2; no changes were made".into());
    }
    if !parsed.get("stages").is_some_and(Value::is_object) {
        return Err(
            ".aidlc/state.json is not a recognised state file; no changes were made".into(),
        );
    }

    let mut state = normalize(parsed);
    validate_legacy(&state, graph)?;

    let id = match state.get("intentId") {
        Some(value) if !js_falsy(value) => {
            let id = value.as_str().ok_or_else(id_error)?.to_owned();
            safe_id(&id)?;
            id
        }
        _ => choose_id(root, &state)?,
    };
    reject_reserved_name(&id)?;
    state["intentId"] = Value::String(id.clone());
    state["updatedAt"] = Value::String(iso_now());

    let intents = aidlc.join("intents");
    let intent_dir = intents.join(&id);
    let state_path = intent_dir.join("state.json");
    let markdown_path = intent_dir.join("intent.md");
    let backup_path = aidlc.join("state.v1.bak.json");
    let audit_path = aidlc.join("audit.jsonl");
    for path in [
        &intent_dir,
        &state_path,
        &markdown_path,
        &backup_path,
        &audit_path,
    ] {
        ensure_safe(root, path)?;
    }
    for dir in [&intents, &intent_dir] {
        if let Some(meta) = lstat(dir)? {
            if !meta.is_dir() {
                return Err(format!(
                    "Refusing unsafe {} path; no changes were made",
                    display_rel(root, dir)
                ));
            }
        }
    }
    for file in [&state_path, &backup_path, &audit_path] {
        if let Some(meta) = lstat(file)? {
            if !meta.is_file() {
                return Err(format!(
                    "Refusing unsafe {} path; no changes were made",
                    display_rel(root, file)
                ));
            }
        }
    }
    // Never replace a state that does not represent this exact (interrupted) migration.
    if lstat(&state_path)?.is_some() && !matching_by_identity(&state_path, &state) {
        return Err(format!(
            "Intent folder '{id}' already contains different state; no changes were made"
        ));
    }
    let backup_exists = match lstat(&backup_path)? {
        Some(_) => {
            if fs::read(&backup_path).map_err(ioerr)? != original {
                return Err(different_backup_error());
            }
            true
        }
        None => false,
    };

    // ---- Writes: backup, intent folder, intent state, pointer, audit. ----
    if !backup_exists {
        write_backup(root, &backup_path, &original)?;
    }
    ensure_safe(root, &intent_dir)?;
    fs::create_dir_all(&intent_dir).map_err(ioerr)?;
    ensure_safe(root, &markdown_path)?;
    if lstat(&markdown_path)?.is_none() {
        let intent = state
            .get("intent")
            .and_then(Value::as_str)
            .unwrap_or_default();
        write_new(&markdown_path, format!("# {id}\n\n{intent}\n").as_bytes())?;
    }
    atomic_json(root, &state_path, &state)?;
    atomic_json(
        root,
        &pointer_path,
        &serde_json::json!({
            "version": 2,
            "activeIntentId": id,
            "updatedAt": iso_now(),
        }),
    )?;
    append_audit(root, &audit_path, &id)?;
    Ok(Migration {
        intent_id: id,
        state,
    })
}

fn different_backup_error() -> String {
    ".aidlc/state.v1.bak.json already exists with different content; refusing to replace it. \
     Move it aside and retry. No changes were made."
        .into()
}

fn id_error() -> String {
    ".aidlc/state.json is invalid: intentId does not match folder. No changes were made.".into()
}

fn invalid(problem: &str) -> String {
    format!(".aidlc/state.json is invalid: {problem}. No changes were made.")
}

/// Node `readPointer`: a v2 pointer has `version: 2` and a null or well-formed active ID.
fn is_v2_pointer(raw: &Value) -> bool {
    raw.is_object()
        && raw.get("version").and_then(Value::as_f64) == Some(2.0)
        && match raw.get("activeIntentId") {
            Some(Value::Null) => true,
            Some(Value::String(id)) => safe_id(id).is_ok(),
            _ => false,
        }
}

/// JavaScript truthiness for JSON values that Node's `Boolean(legacy.intentId)` sees.
fn js_falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// Node `validateIntentState(legacy, graph, legacy.intentId, { checkId })` on the
/// normalised legacy state.
fn validate_legacy(state: &Value, graph: &Value) -> MigrationResult<()> {
    if !state.is_object() {
        return Err(invalid("not an object"));
    }
    if state
        .get("intent")
        .and_then(Value::as_str)
        .is_none_or(js_blank)
    {
        return Err(invalid("intent is not a non-empty string"));
    }
    let scope_ok = state
        .get("scope")
        .and_then(Value::as_str)
        .is_some_and(|scope| {
            graph
                .get("scopes")
                .and_then(Value::as_object)
                .is_some_and(|scopes| scopes.contains_key(scope))
        });
    if !scope_ok {
        return Err(invalid("scope is not a key of graph.scopes"));
    }
    if let Some(mode) = state.get("mode") {
        if !matches!(mode.as_str(), Some("ai" | "human")) {
            return Err(invalid("mode is not ai or human"));
        }
    }
    let stages = state
        .get("stages")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("stages is not an object"))?;
    for (id, row) in stages {
        let status = row.get("status").and_then(Value::as_str);
        if !row.is_object() || !status.is_some_and(|s| STATUSES.contains(&s)) {
            return Err(invalid(&format!("stage {id} has an invalid status")));
        }
    }
    if let Some(id) = state.get("intentId").filter(|v| !js_falsy(v)) {
        safe_id(id.as_str().ok_or_else(id_error)?)?;
    }
    if let Some(current) = state.get("currentStage").filter(|v| !v.is_null()) {
        if !scope_stage_ids(graph, state["scope"].as_str().unwrap_or_default())
            .is_some_and(|ids| ids.contains(&current))
        {
            return Err(invalid("currentStage is not a stage of the scope"));
        }
    }
    if let Some(deps) = state.get("dependencies") {
        if !deps.as_object().is_some_and(|m| {
            m.values()
                .all(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
        }) {
            return Err(invalid("dependencies is not an object of string arrays"));
        }
    }
    Ok(())
}

/// Stage IDs of `orderedStages(graph, scope)` (membership only; order is irrelevant here).
/// `None` mirrors Node throwing on a graph without `phases`/`stages` arrays.
fn scope_stage_ids<'a>(graph: &'a Value, scope: &str) -> Option<Vec<&'a Value>> {
    let phases = graph.get("scopes")?.get(scope)?.as_array()?;
    graph.get("phases")?.as_array()?;
    let stages = graph.get("stages")?.as_array()?;
    Some(
        stages
            .iter()
            .filter(|stage| stage.get("phase").is_some_and(|p| phases.contains(p)))
            .filter_map(|stage| stage.get("id"))
            .collect(),
    )
}

/// JavaScript `!String.prototype.trim()` (ECMAScript WhiteSpace + LineTerminator).
fn js_blank(text: &str) -> bool {
    text.chars()
        .all(|c| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
}

/// Node `normalise`: keeps key order, sets `version: 2`, defaults `dependencies`, gives each
/// stage row a `detail` (string or null) and drops `project`.
fn normalize(mut state: Value) -> Value {
    let Some(obj) = state.as_object_mut() else {
        return state;
    };
    obj.insert("version".into(), Value::from(2));
    if obj.get("dependencies").is_none_or(Value::is_null) {
        obj.insert("dependencies".into(), Value::Object(Map::new()));
    }
    if let Some(rows) = obj.get_mut("stages").and_then(Value::as_object_mut) {
        for row in rows.values_mut() {
            if row.is_null() {
                *row = Value::Object(Map::new());
            }
            if let Some(fields) = row.as_object_mut() {
                if !fields.get("detail").is_some_and(Value::is_string) {
                    fields.insert("detail".into(), Value::Null);
                }
            }
        }
    }
    obj.shift_remove("project");
    state
}

/// Derive the intent ID like Node: slug of the intent, reusing a folder that already holds
/// this same intent (interrupted migration), otherwise the first free `base`, `base-2`, ...
fn choose_id(root: &Path, state: &Value) -> MigrationResult<String> {
    let base = intent_slug(state["intent"].as_str().unwrap_or_default())?;
    let mut id = base.clone();
    let mut n = 2usize;
    loop {
        safe_id(&id)?;
        let dir = root.join(".aidlc/intents").join(&id);
        let knowledge = root.join(".aidlc/knowledge").join(format!("{id}.md"));
        if matching_by_identity(&dir.join("state.json"), state) {
            return Ok(id);
        }
        if lstat(&dir)?.is_none() && lstat(&knowledge)?.is_none() {
            return Ok(id);
        }
        id = format!("{base}-{n}");
        n += 1;
    }
}

enum Fold {
    /// Folds (NFKD, marks stripped, lowercased) to this ASCII letter; counts as a letter.
    Ascii(char),
    /// A letter that folds to no `[a-z0-9]`: counted as a letter, acts as a separator.
    OtherLetter,
    /// Not a letter or number after folding: acts as a separator.
    Separator,
    /// A combining mark in U+0300..U+036F: removed entirely.
    Mark,
}

/// Exact per-character model of Node's `normalize('NFKD')`, mark stripping, `đ/Đ -> d` and
/// `toLowerCase()` for the characters we can reproduce without Unicode tables.
fn fold_char(c: char) -> Option<Fold> {
    if c.is_ascii() {
        return Some(if c.is_ascii_alphanumeric() {
            Fold::Ascii(c.to_ascii_lowercase())
        } else {
            Fold::Separator
        });
    }
    let base = match c as u32 {
        0x0300..=0x036F => return Some(Fold::Mark),
        // Latin-1 letters with canonical decompositions to an ASCII base + marks.
        0xC0..=0xC5 | 0xE0..=0xE5 => 'a',
        0xC7 | 0xE7 => 'c',
        0xC8..=0xCB | 0xE8..=0xEB => 'e',
        0xCC..=0xCF | 0xEC..=0xEF => 'i',
        0xD1 | 0xF1 => 'n',
        0xD2..=0xD6 | 0xF2..=0xF6 => 'o',
        0xD9..=0xDC | 0xF9..=0xFC => 'u',
        0xDD | 0xFD | 0xFF => 'y',
        // Latin-1 letters without decomposition.
        0xC6 | 0xD0 | 0xD8 | 0xDE | 0xDF | 0xE6 | 0xF0 | 0xF8 | 0xFE => {
            return Some(Fold::OtherLetter)
        }
        // Vietnamese letters outside Latin-1 (đ/Đ are replaced explicitly by Node).
        0x0110 | 0x0111 => 'd',
        0x0102 | 0x0103 => 'a',
        0x0128 | 0x0129 => 'i',
        0x0168 | 0x0169 => 'u',
        0x01A0 | 0x01A1 => 'o',
        0x01AF | 0x01B0 => 'u',
        0x1EA0..=0x1EB7 => 'a',
        0x1EB8..=0x1EC7 => 'e',
        0x1EC8..=0x1ECB => 'i',
        0x1ECC..=0x1EE3 => 'o',
        0x1EE4..=0x1EF1 => 'u',
        0x1EF2..=0x1EF9 => 'y',
        // Symbols, spaces and punctuation whose NFKD contains no letter or number.
        0xA0
        | 0xAB
        | 0xB7
        | 0xBB
        | 0xD7
        | 0xF7
        | 0x2010..=0x2015
        | 0x2018..=0x201F
        | 0x2022
        | 0x2026
        | 0x3000
        | 0xFEFF => return Some(Fold::Separator),
        _ => return None,
    };
    Some(Fold::Ascii(base))
}

/// Node `intentSlug` for the supported character set; refuses (no writes) otherwise.
fn intent_slug(text: &str) -> MigrationResult<String> {
    let mut folded = String::new();
    let mut letters = 0usize;
    let mut kept = 0usize;
    let mut has_mark = false;
    for c in text.chars() {
        match fold_char(c) {
            Some(Fold::Ascii(a)) => {
                folded.push(a);
                letters += 1;
                kept += 1;
            }
            Some(Fold::OtherLetter) => {
                folded.push(' ');
                letters += 1;
            }
            Some(Fold::Separator) => folded.push(' '),
            Some(Fold::Mark) => has_mark = true,
            None => return Err(unsupported_slug_error()),
        }
    }
    // folded.replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48).replace(/-+$/, '')
    let mut base = String::new();
    for word in folded.split(' ').filter(|w| !w.is_empty()) {
        if !base.is_empty() {
            base.push('-');
        }
        base.push_str(word);
    }
    base.truncate(48);
    while base.ends_with('-') {
        base.pop();
    }
    if kept >= 3 && kept * 2 >= letters && !base.is_empty() {
        return Ok(base);
    }
    // Node hashes the NFC form; text without combining marks in the supported set is
    // already NFC, otherwise we cannot reproduce the hash safely.
    if has_mark {
        return Err(unsupported_slug_error());
    }
    let day = iso_now()
        .get(0..10)
        .unwrap_or("1970-01-01")
        .replace('-', "");
    let digest = Sha256::digest(text.as_bytes());
    Ok(format!(
        "intent-{day}-{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2]
    ))
}

fn unsupported_slug_error() -> String {
    "Cannot derive the legacy intentId exactly like the Node CLI for this intent text; \
     add an explicit \"intentId\" to .aidlc/state.json and retry. No changes were made."
        .into()
}

fn matching_by_identity(path: &Path, wanted: &Value) -> bool {
    read_value(path).is_ok_and(|existing| {
        existing.get("intent") == wanted.get("intent")
            && existing.get("createdAt") == wanted.get("createdAt")
    })
}
fn read_value(path: &Path) -> MigrationResult<Value> {
    let bytes = fs::read(path).map_err(ioerr)?;
    serde_json::from_slice(&bytes).map_err(|_| "existing intent state cannot be parsed".into())
}

fn safe_id(id: &str) -> MigrationResult<()> {
    let bytes = id.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 64
        || !(bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        || !bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(id_error());
    }
    Ok(())
}

/// Windows resolves these names to devices in any directory; refuse them as folder names.
fn reject_reserved_name(id: &str) -> MigrationResult<()> {
    let reserved = matches!(id, "con" | "prn" | "aux" | "nul")
        || (id.len() == 4
            && (id.starts_with("com") || id.starts_with("lpt"))
            && id.as_bytes()[3].is_ascii_digit());
    if cfg!(windows) && reserved {
        return Err(format!(
            "Intent ID '{id}' is a reserved device name on Windows; add a different explicit \
             \"intentId\" to .aidlc/state.json and retry. No changes were made."
        ));
    }
    Ok(())
}

fn lstat(path: &Path) -> MigrationResult<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ioerr(e)),
    }
}

fn display_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// Node `assertSafeStateDir`: `.aidlc` and `.aidlc/intents` must be real directories.
fn assert_safe_state_dir(root: &Path) -> MigrationResult<()> {
    let dir = root.join(".aidlc");
    if let Some(meta) = lstat(&dir)? {
        if meta.file_type().is_symlink() {
            return Err("Refusing to access symlink .aidlc directory".into());
        }
        if !meta.is_dir() {
            return Err("Refusing unsafe .aidlc path".into());
        }
    }
    ensure_safe(root, &dir.join("intents"))
}

/// Refuse any symlink (or Windows junction / name-surrogate reparse point) on the path
/// from `root` to `path`, and any path that escapes `root`.
fn ensure_safe(root: &Path, path: &Path) -> MigrationResult<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "workflow path escapes project root".to_owned())?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err("workflow path escapes project root".into());
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing to write through symlink: {}",
                    display_rel(root, &current)
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => break,
            Err(e) => return Err(ioerr(e)),
        }
    }
    Ok(())
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// A fresh sibling temp path (`<file>.<pid>.<unique>.tmp`, like Node's `writeFileAtomic`).
fn temp_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let unique = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("state.json");
    path.with_file_name(format!(
        "{name}.{}.{nanos:x}{unique:x}.tmp",
        std::process::id()
    ))
}

fn create_new(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn write_temp(path: &Path, bytes: &[u8]) -> MigrationResult<PathBuf> {
    let tmp = temp_path(path);
    let mut file = create_new(&tmp).map_err(ioerr)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(ioerr);
    drop(file);
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(tmp)
}

/// Write `value` as Node's `JSON.stringify(value, null, 2) + "\n"` via temp + rename.
fn atomic_json(root: &Path, path: &Path, value: &Value) -> MigrationResult<()> {
    ensure_safe(root, path)?;
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    let tmp = write_temp(path, &bytes)?;
    let result = replace_file(&tmp, path);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Rename `from` over `to`. On Windows `std::fs::rename` replaces an existing destination
/// atomically (MoveFileEx with MOVEFILE_REPLACE_EXISTING / POSIX rename semantics); the
/// destination is never deleted first. Transient sharing violations (antivirus, indexers)
/// are retried briefly; any final failure leaves the destination untouched.
fn replace_file(from: &Path, to: &Path) -> MigrationResult<()> {
    let mut attempt = 0u64;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if cfg!(windows) && attempt < 8 && transient_windows(&error) => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(15 * attempt));
            }
            Err(error) => return Err(ioerr(error)),
        }
    }
}

fn transient_windows(error: &std::io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// Create the backup without ever replacing an existing file: write a temp file, then
/// hard-link it into place (fails if the backup exists). Falls back to an exclusive create
/// on file systems without hard links.
fn write_backup(root: &Path, path: &Path, bytes: &[u8]) -> MigrationResult<()> {
    ensure_safe(root, path)?;
    let tmp = write_temp(path, bytes)?;
    let linked = fs::hard_link(&tmp, path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if fs::read(path).map_err(ioerr)? == bytes {
                Ok(())
            } else {
                Err(different_backup_error())
            }
        }
        Err(_) => write_new(path, bytes),
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> MigrationResult<()> {
    let mut file = create_new(path).map_err(ioerr)?;
    file.write_all(bytes).map_err(ioerr)?;
    file.sync_all().map_err(ioerr)
}

/// Node `audit(root, 'state.migrated', { from: 1, intentId })`.
fn append_audit(root: &Path, path: &Path, id: &str) -> MigrationResult<()> {
    assert_safe_state_dir(root)?;
    ensure_safe(root, path)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(ioerr)?;
    let row = serde_json::json!({"at":iso_now(),"event":"state.migrated","from":1,"intentId":id});
    let line = format!(
        "{}\n",
        serde_json::to_string(&row).map_err(|e| e.to_string())?
    );
    file.write_all(line.as_bytes()).map_err(ioerr)?;
    file.sync_all().map_err(ioerr)
}

fn ioerr(error: std::io::Error) -> String {
    error.to_string()
}

/// `new Date().toISOString()` (UTC, millisecond precision).
fn iso_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() as i64;
    let days = seconds.div_euclid(86400);
    let day_seconds = seconds.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60,
        now.subsec_millis()
    )
}
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn graph() -> Value {
        serde_json::json!({"scopes":{"solo":["phase"]},"phases":[{"id":"phase"}],"stages":[{"id":"problem-framing","phase":"phase","order":1},{"id":"other","phase":"elsewhere","order":2}]})
    }
    /// Test roots live under the crate's `target/` directory (not the system temp dir).
    fn temp() -> PathBuf {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("state-migration-tests")
            .join(format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(p.join(".aidlc")).unwrap();
        p
    }
    fn legacy() -> Value {
        serde_json::json!({"version":1,"scope":"solo","intent":"test","currentStage":"problem-framing","stages":{"problem-framing":{"status":"in_progress","startedAt":"then"}},"dependencies":{},"createdAt":"original"})
    }
    fn write_legacy(root: &Path, state: &Value) -> Vec<u8> {
        let bytes = serde_json::to_vec(state).unwrap();
        fs::write(root.join(".aidlc/state.json"), &bytes).unwrap();
        bytes
    }
    /// Every file under `.aidlc`, relative, with contents, for "no writes" assertions.
    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let meta = fs::symlink_metadata(&path).unwrap();
                let rel = display_rel(root, &path);
                if meta.file_type().is_symlink() {
                    out.push((rel, b"<link>".to_vec()));
                } else if meta.is_dir() {
                    out.push((rel.clone(), b"<dir>".to_vec()));
                    walk(&path, root, out);
                } else {
                    out.push((rel, fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&root.join(".aidlc"), root, &mut out);
        out.sort();
        out
    }
    fn no_temp_files(dir: &Path) -> bool {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
    }
    fn audit_lines(root: &Path) -> Vec<Value> {
        fs::read_to_string(root.join(".aidlc/audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn today() -> String {
        iso_now()[0..10].replace('-', "")
    }

    fn assert_fresh_migration(root: &Path, bytes: &[u8]) {
        let result = migrate_legacy_state(root, &graph()).unwrap();
        assert_eq!(result.intent_id, "test");
        assert_eq!(
            fs::read(root.join(".aidlc/state.v1.bak.json")).unwrap(),
            bytes
        );
        let written = fs::read_to_string(root.join(".aidlc/intents/test/state.json")).unwrap();
        assert!(written.ends_with("}\n"));
        let on_disk: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(on_disk, result.state);
        // Node key order: original keys in place, then intentId and updatedAt appended.
        let keys: Vec<&str> = on_disk
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "version",
                "scope",
                "intent",
                "currentStage",
                "stages",
                "dependencies",
                "createdAt",
                "intentId",
                "updatedAt"
            ]
        );
        assert_eq!(on_disk["version"], 2);
        assert_eq!(on_disk["stages"]["problem-framing"]["detail"], Value::Null);
        assert!(on_disk.get("status").is_none());
        assert_eq!(
            fs::read_to_string(root.join(".aidlc/intents/test/intent.md")).unwrap(),
            "# test\n\ntest\n"
        );
        let pointer: Value =
            serde_json::from_slice(&fs::read(root.join(".aidlc/state.json")).unwrap()).unwrap();
        assert_eq!(pointer["version"], 2);
        assert_eq!(pointer["activeIntentId"], "test");
        assert!(pointer["updatedAt"].as_str().unwrap().ends_with('Z'));
        let audit = audit_lines(root);
        assert_eq!(audit.len(), 1);
        let keys: Vec<&String> = audit[0].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["at", "event", "from", "intentId"]);
        assert_eq!(audit[0]["event"], "state.migrated");
        assert_eq!(audit[0]["from"], 1);
        assert_eq!(audit[0]["intentId"], "test");
        assert!(no_temp_files(&root.join(".aidlc")));
        assert!(no_temp_files(&root.join(".aidlc/intents/test")));
    }

    #[test]
    fn migrates_preserving_bytes_and_is_idempotent() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        let bytes = b"{\"version\":1,\"scope\":\"solo\",\"intent\":\"test\",\"currentStage\":\"problem-framing\",\"stages\":{\"problem-framing\":{\"status\":\"in_progress\",\"startedAt\":\"then\"}},\"dependencies\":{},\"createdAt\":\"original\"}\n";
        fs::write(&path, bytes).unwrap();
        assert_fresh_migration(&root, bytes);
        let before = snapshot(&root);
        assert_eq!(
            migrate_legacy_state(&root, &graph()).unwrap_err(),
            "State is already version 2; no changes were made"
        );
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    /// Expected bytes were produced by Node `loadWorkflow(root, graph, { migrate: true })`
    /// on the same fixture; only the timestamps differ.
    #[test]
    fn written_files_match_node_bytes() {
        let root = temp();
        let bytes = br#"{"version":1,"project":"p","scope":"solo","intent":"Build the payment API","currentStage":"problem-framing","stages":{"problem-framing":{"status":"in_progress","detail":5,"startedAt":"then"},"x":{"detail":"keep","status":"complete"}},"dependencies":null,"createdAt":"original","extra":{"n":[1,2]}}"#;
        fs::write(root.join(".aidlc/state.json"), bytes).unwrap();
        let result = migrate_legacy_state(&root, &graph()).unwrap();
        let id = "build-the-payment-api";
        assert_eq!(result.intent_id, id);
        let state_at = result.state["updatedAt"].as_str().unwrap().to_owned();
        let expected_state = format!(
            "{{\n  \"version\": 2,\n  \"scope\": \"solo\",\n  \"intent\": \"Build the payment API\",\n  \"currentStage\": \"problem-framing\",\n  \"stages\": {{\n    \"problem-framing\": {{\n      \"status\": \"in_progress\",\n      \"detail\": null,\n      \"startedAt\": \"then\"\n    }},\n    \"x\": {{\n      \"detail\": \"keep\",\n      \"status\": \"complete\"\n    }}\n  }},\n  \"dependencies\": {{}},\n  \"createdAt\": \"original\",\n  \"extra\": {{\n    \"n\": [\n      1,\n      2\n    ]\n  }},\n  \"intentId\": \"{id}\",\n  \"updatedAt\": \"{state_at}\"\n}}\n"
        );
        assert_eq!(
            fs::read_to_string(root.join(".aidlc/intents").join(id).join("state.json")).unwrap(),
            expected_state
        );
        let pointer = fs::read_to_string(root.join(".aidlc/state.json")).unwrap();
        let pointer_at: Value = serde_json::from_str(&pointer).unwrap();
        let pointer_at = pointer_at["updatedAt"].as_str().unwrap();
        assert_eq!(
            pointer,
            format!("{{\n  \"version\": 2,\n  \"activeIntentId\": \"{id}\",\n  \"updatedAt\": \"{pointer_at}\"\n}}\n")
        );
        let audit = fs::read_to_string(root.join(".aidlc/audit.jsonl")).unwrap();
        let audit_at = audit_lines(&root)[0]["at"].as_str().unwrap().to_owned();
        assert_eq!(
            audit,
            format!("{{\"at\":\"{audit_at}\",\"event\":\"state.migrated\",\"from\":1,\"intentId\":\"{id}\"}}\n")
        );
        assert_eq!(
            fs::read_to_string(root.join(".aidlc/intents").join(id).join("intent.md")).unwrap(),
            "# build-the-payment-api\n\nBuild the payment API\n"
        );
        assert_eq!(
            fs::read(root.join(".aidlc/state.v1.bak.json")).unwrap(),
            bytes
        );
        assert_eq!(state_at.len(), "2026-11-25T15:58:43.663Z".len());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn windows_fresh_migration() {
        let root = temp();
        let bytes = write_legacy(&root, &legacy());
        assert_fresh_migration(&root, &bytes);
        fs::remove_dir_all(root).unwrap();
    }

    /// Simulates a run that wrote the backup, intent.md and intent state but crashed before
    /// replacing the pointer; the rerun must reuse the folder and finish.
    fn rerun_after_interrupted(explicit_id: bool) {
        let root = temp();
        let mut state = legacy();
        state["intent"] = Value::String("Build the payment API".into());
        if explicit_id {
            state["intentId"] = Value::String("build-the-payment-api".into());
        }
        let bytes = write_legacy(&root, &state);
        let dir = root.join(".aidlc/intents/build-the-payment-api");
        fs::create_dir_all(&dir).unwrap();
        fs::write(root.join(".aidlc/state.v1.bak.json"), &bytes).unwrap();
        fs::write(dir.join("intent.md"), "# keep\n").unwrap();
        let mut partial = normalize(state.clone());
        partial["intentId"] = Value::String("build-the-payment-api".into());
        partial["updatedAt"] = Value::String("earlier".into());
        fs::write(
            dir.join("state.json"),
            serde_json::to_vec(&partial).unwrap(),
        )
        .unwrap();
        // A stale temp file from the crashed run must not block the rerun.
        fs::write(root.join(".aidlc/state.json.1.0.tmp"), b"junk").unwrap();

        let result = migrate_legacy_state(&root, &graph()).unwrap();
        assert_eq!(result.intent_id, "build-the-payment-api");
        assert!(!root.join(".aidlc/intents/build-the-payment-api-2").exists());
        assert_eq!(
            fs::read(root.join(".aidlc/state.v1.bak.json")).unwrap(),
            bytes
        );
        assert_eq!(
            fs::read_to_string(dir.join("intent.md")).unwrap(),
            "# keep\n"
        );
        let pointer: Value =
            serde_json::from_slice(&fs::read(root.join(".aidlc/state.json")).unwrap()).unwrap();
        assert_eq!(pointer["activeIntentId"], "build-the-payment-api");
        let on_disk: Value =
            serde_json::from_slice(&fs::read(dir.join("state.json")).unwrap()).unwrap();
        assert_ne!(on_disk["updatedAt"], "earlier");
        assert_eq!(audit_lines(&root).len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn windows_rerun_after_interrupted_migration() {
        rerun_after_interrupted(true);
        rerun_after_interrupted(false);
    }

    #[test]
    #[cfg(not(windows))]
    fn rerun_after_interrupted_migration() {
        rerun_after_interrupted(true);
        rerun_after_interrupted(false);
    }

    fn refuses_different_backup() {
        let root = temp();
        write_legacy(&root, &legacy());
        fs::write(root.join(".aidlc/state.v1.bak.json"), b"{\"older\":true}").unwrap();
        let before = snapshot(&root);
        let error = migrate_legacy_state(&root, &graph()).unwrap_err();
        assert!(
            error.contains("state.v1.bak.json already exists"),
            "{error}"
        );
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn windows_existing_different_backup_refused() {
        refuses_different_backup();
    }

    #[test]
    #[cfg(not(windows))]
    fn existing_different_backup_refused() {
        refuses_different_backup();
    }

    #[test]
    fn identical_existing_backup_is_reused() {
        let root = temp();
        let bytes = write_legacy(&root, &legacy());
        fs::write(root.join(".aidlc/state.v1.bak.json"), &bytes).unwrap();
        migrate_legacy_state(&root, &graph()).unwrap();
        assert_eq!(
            fs::read(root.join(".aidlc/state.v1.bak.json")).unwrap(),
            bytes
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_json_replaces_existing_file_without_leftovers() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        fs::write(&path, b"old contents").unwrap();
        atomic_json(&root, &path, &serde_json::json!({"a":1,"b":[]})).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"a\": 1,\n  \"b\": []\n}\n"
        );
        atomic_json(&root, &path, &serde_json::json!({"a":2})).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\n  \"a\": 2\n}\n");
        assert!(no_temp_files(&root.join(".aidlc")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_state_fails_without_writes() {
        let cases: [fn(&mut Value); 8] = [
            |s| s["scope"] = Value::String("galaxy".into()),
            |s| s["intent"] = Value::String(" \u{feff}\n".into()),
            |s| s["mode"] = Value::Null,
            |s| s["stages"]["problem-framing"]["status"] = "done".into(),
            |s| s["currentStage"] = "other".into(),
            |s| s["dependencies"] = serde_json::json!({"a":[1]}),
            |s| s["intentId"] = "Bad_ID".into(),
            |s| s["intentId"] = Value::Bool(true),
        ];
        for mutate in cases {
            let root = temp();
            let mut state = legacy();
            mutate(&mut state);
            write_legacy(&root, &state);
            let before = snapshot(&root);
            let error = migrate_legacy_state(&root, &graph()).unwrap_err();
            assert!(error.contains("is invalid"), "{error}");
            assert_eq!(snapshot(&root), before);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn malformed_or_unrecognised_state_fails_without_writes() {
        for bytes in [
            &b"{not json"[..],
            &b"\xef\xbb\xbf{}"[..],
            &b"[]"[..],
            &b"{\"version\":1,\"stages\":[]}"[..],
            &b"{\"version\":2,\"activeIntentId\":null}"[..],
        ] {
            let root = temp();
            fs::write(root.join(".aidlc/state.json"), bytes).unwrap();
            let before = snapshot(&root);
            assert!(migrate_legacy_state(&root, &graph()).is_err());
            assert_eq!(snapshot(&root), before);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn different_state_in_intent_folder_is_refused_without_writes() {
        let root = temp();
        let mut state = legacy();
        state["intentId"] = Value::String("test".into());
        write_legacy(&root, &state);
        fs::create_dir_all(root.join(".aidlc/intents/test")).unwrap();
        fs::write(
            root.join(".aidlc/intents/test/state.json"),
            b"{\"intent\":\"another\",\"createdAt\":\"x\"}",
        )
        .unwrap();
        let before = snapshot(&root);
        assert!(migrate_legacy_state(&root, &graph())
            .unwrap_err()
            .contains("already contains different state"));
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn derived_id_skips_taken_folders_and_knowledge() {
        let root = temp();
        let mut state = legacy();
        state["intentId"] = Value::Null;
        state["project"] = Value::String("dropped".into());
        write_legacy(&root, &state);
        fs::create_dir_all(root.join(".aidlc/intents/test")).unwrap();
        fs::write(
            root.join(".aidlc/intents/test/state.json"),
            b"{\"intent\":\"another\"}",
        )
        .unwrap();
        fs::create_dir_all(root.join(".aidlc/knowledge")).unwrap();
        fs::write(root.join(".aidlc/knowledge/test-2.md"), b"k").unwrap();
        let result = migrate_legacy_state(&root, &graph()).unwrap();
        assert_eq!(result.intent_id, "test-3");
        assert!(result.state.get("project").is_none());
        // intentId was present (null) so it keeps its position, like Node's assignment.
        let keys: Vec<&String> = result.state.as_object().unwrap().keys().collect();
        assert_eq!(keys.last().unwrap().as_str(), "updatedAt");
        assert_eq!(
            fs::read(root.join(".aidlc/intents/test/state.json")).unwrap(),
            b"{\"intent\":\"another\"}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn slug_matches_node_intent_slug() {
        // Expected values produced by core/tools/workflow.mjs intentSlug (Node 24).
        for (text, expected) in [
            ("Build the payment API", "build-the-payment-api"),
            ("Café au lait", "cafe-au-lait"),
            (
                "Xây dựng hệ thống thanh toán",
                "xay-dung-he-thong-thanh-toan",
            ),
            ("Đăng nhập", "dang-nhap"),
            ("test", "test"),
            ("Ærø island ß", "r-island"),
            ("x — y “z”", "x-y-z"),
            ("  --Hello, World!--  ", "hello-world"),
            ("ß ð æ abc", "abc"),
        ] {
            assert_eq!(intent_slug(text).unwrap(), expected, "{text}");
        }
        assert_eq!(
            intent_slug("a b").unwrap(),
            format!("intent-{}-c8687a", today())
        );
        assert_eq!(
            intent_slug("ab").unwrap(),
            format!("intent-{}-fb8e20", today())
        );
        let long = "word ".repeat(20);
        let slug = intent_slug(&long).unwrap();
        assert!(slug.len() <= 48 && !slug.ends_with('-'));
        assert!(intent_slug("日本語のテスト").is_err());
        assert!(intent_slug("e\u{0301}").is_err());
    }

    #[test]
    fn unsupported_unicode_without_intent_id_fails_without_writes() {
        let root = temp();
        let mut state = legacy();
        state["intent"] = Value::String("日本語のテスト".into());
        write_legacy(&root, &state);
        let before = snapshot(&root);
        assert!(migrate_legacy_state(&root, &graph())
            .unwrap_err()
            .contains("explicit \"intentId\""));
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn windows_reserved_device_name_refused_without_writes() {
        let root = temp();
        let mut state = legacy();
        state["intent"] = Value::String("Con".into());
        write_legacy(&root, &state);
        let before = snapshot(&root);
        assert!(migrate_legacy_state(&root, &graph())
            .unwrap_err()
            .contains("reserved device name"));
        assert_eq!(snapshot(&root), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_symlink_components() {
        let root = temp();
        let target = root.join("elsewhere");
        fs::create_dir_all(&target).unwrap();
        write_legacy(&root, &legacy());
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(&target, root.join(".aidlc/intents"));
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_dir(&target, root.join(".aidlc/intents"));
        if let Err(error) = link {
            if error.kind() == ErrorKind::PermissionDenied || error.raw_os_error() == Some(1314) {
                fs::remove_dir_all(root).unwrap();
                return;
            }
            panic!("cannot create symlink fixture: {error}");
        }
        let before = snapshot(&root);
        assert!(migrate_legacy_state(&root, &graph()).is_err());
        assert_eq!(snapshot(&root), before);
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    /// Junctions need no privilege on Windows, so this always exercises the refusal.
    #[test]
    #[cfg(windows)]
    fn windows_refuses_junction_components_without_writes() {
        for link_rel in [".aidlc\\intents", ".aidlc\\intents\\test"] {
            let root = temp();
            let target = root.join("elsewhere");
            fs::create_dir_all(&target).unwrap();
            fs::create_dir_all(root.join(".aidlc\\intents")).unwrap();
            let link = root.join(link_rel);
            if link_rel.ends_with("intents") {
                fs::remove_dir(&link).unwrap();
            }
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&target)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "mklink /J failed");
            let mut state = legacy();
            state["intentId"] = Value::String("test".into());
            write_legacy(&root, &state);
            let before = snapshot(&root);
            let error = migrate_legacy_state(&root, &graph()).unwrap_err();
            assert!(error.contains("symlink"), "{error}");
            assert_eq!(snapshot(&root), before);
            assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
            fs::remove_dir(&link).unwrap();
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn interrupted_matching_folder_reused_and_conflicts_refused() {
        let root = temp();
        let path = root.join(".aidlc/state.json");
        let mut state = legacy();
        state["intentId"] = Value::String("test".into());
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        fs::create_dir_all(root.join(".aidlc/intents/test")).unwrap();
        let mirror = normalize(state.clone());
        fs::write(
            root.join(".aidlc/intents/test/state.json"),
            serde_json::to_vec(&mirror).unwrap(),
        )
        .unwrap();
        assert_eq!(
            migrate_legacy_state(&root, &graph()).unwrap().intent_id,
            "test"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
