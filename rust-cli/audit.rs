//! Shared audit-log writer, ported from Node `core/tools/lib.mjs` `audit()` and
//! `assertSafeStateDir()`.
//!
//! Each call appends exactly one JSON line to `.aidlc/audit.jsonl` shaped as
//! `{"at": "<ISO-8601 ms UTC>", "event": "<event>", ...details}`. Detail fields are
//! merged in their own order; a detail named `at` or `event` overrides the value but
//! keeps the leading position, matching JavaScript object-spread semantics.

use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STATE_DIR: &str = ".aidlc";
const AUDIT_FILE: &str = "audit.jsonl";

/// Current UTC time formatted like JavaScript `new Date().toISOString()`,
/// for example `2026-01-02T03:04:05.678Z`.
pub fn now_iso() -> String {
    let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_millis() as i64,
        Err(before) => -(before.duration().as_millis() as i64),
    };
    format_iso_millis(millis)
}

/// Format milliseconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn format_iso_millis(millis: i64) -> String {
    let seconds = millis.div_euclid(1000);
    let ms = millis.rem_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// `lstat` that maps "not found" to `None` and surfaces every other error,
/// mirroring Node `pathExists()` + `lstatSync()`.
fn lstat(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn absolute(root: &Path) -> PathBuf {
    if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(root))
            .unwrap_or_else(|_| root.to_path_buf())
    }
}

/// Port of Node `assertNoSymlinkComponents(root, relativePath)`: walk each existing
/// component of `relative` beneath `root` and refuse the first symlink.
fn assert_no_symlink_components(root: &Path, relative: &str) -> Result<(), String> {
    let mut current = absolute(root);
    let mut walked: Vec<&str> = Vec::new();
    for part in relative.split(['/', '\\']).filter(|part| !part.is_empty()) {
        current.push(part);
        walked.push(part);
        let Some(meta) = lstat(&current)? else { break };
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Refusing to write through symlink: {}",
                walked.join("/")
            ));
        }
    }
    Ok(())
}

/// Port of Node `assertSafeStateDir(root)`. Refuses a symlinked or non-directory
/// `.aidlc`, any symlink component of `.aidlc/intents`, and a non-directory
/// `.aidlc/intents`. Missing paths are allowed.
pub fn assert_safe_state_dir(root: &Path) -> Result<(), String> {
    let dir = root.join(STATE_DIR);
    if let Some(meta) = lstat(&dir)? {
        if meta.file_type().is_symlink() {
            return Err("Refusing to access symlink .aidlc directory".into());
        }
        if !meta.is_dir() {
            return Err("Refusing unsafe .aidlc path".into());
        }
    }
    assert_no_symlink_components(root, ".aidlc/intents")?;
    if let Some(meta) = lstat(&dir.join("intents"))? {
        if !meta.is_dir() {
            return Err("Refusing unsafe .aidlc/intents path".into());
        }
    }
    Ok(())
}

/// Build the audit row without writing it. `details` must be a JSON object or null.
fn build_row(at: String, event: &str, details: Value) -> Result<Value, String> {
    let mut row = Map::new();
    row.insert("at".into(), Value::String(at));
    row.insert("event".into(), Value::String(event.to_owned()));
    match details {
        Value::Null => {}
        Value::Object(fields) => {
            for (key, value) in fields {
                // preserve_order: overwriting an existing key keeps its position,
                // matching `{ at, event, ...details }` in JavaScript.
                row.insert(key, value);
            }
        }
        _ => return Err("Audit details must be a JSON object".into()),
    }
    Ok(Value::Object(row))
}

/// Refuse an audit log path that is a symlink or not a regular file.
fn check_audit_target(path: &Path) -> Result<Option<fs::Metadata>, String> {
    let meta = lstat(path)?;
    if let Some(meta) = &meta {
        if meta.file_type().is_symlink() {
            return Err("Refusing to write through symlink: .aidlc/audit.jsonl".into());
        }
        if !meta.is_file() {
            return Err("Refusing unsafe .aidlc/audit.jsonl path".into());
        }
    }
    Ok(meta)
}

/// Check, without writing anything, that [`audit`] could append to
/// `.aidlc/audit.jsonl`: the state directory is safe ([`assert_safe_state_dir`]) and the
/// log is absent or a regular, non-symlink file that opens for append. Commands that
/// persist state and then audit call this before their first write, so a broken audit
/// target fails the command with the project unchanged.
pub fn preflight_audit(root: &Path) -> Result<(), String> {
    assert_safe_state_dir(root)?;
    let path = root.join(STATE_DIR).join(AUDIT_FILE);
    if check_audit_target(&path)?.is_some() {
        OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

/// Port of Node `audit(root, event, details)`. Appends one JSON line to
/// `.aidlc/audit.jsonl` (creating `.aidlc` if missing) and returns the row.
pub fn audit(root: &Path, event: &str, details: Value) -> Result<Value, String> {
    assert_safe_state_dir(root)?;
    let dir = root.join(STATE_DIR);
    let path = dir.join(AUDIT_FILE);
    check_audit_target(&path)?;
    fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let row = build_row(now_iso(), event, details)?;
    let mut line = serde_json::to_string(&row).map_err(|error| error.to_string())?;
    line.push('\n');
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(line.as_bytes())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "dtech-aidlc-audit-{}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn read_lines(root: &Path) -> Vec<Value> {
        fs::read_to_string(root.join(".aidlc/audit.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn is_iso_millis(text: &str) -> bool {
        let bytes = text.as_bytes();
        bytes.len() == 24
            && text.ends_with('Z')
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[10] == b'T'
            && bytes[13] == b':'
            && bytes[16] == b':'
            && bytes[19] == b'.'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| [4, 7, 10, 13, 16, 19, 23].contains(&i) || b.is_ascii_digit())
    }

    /// Returns true when the error means the OS refused to create a symlink for lack of
    /// privilege (Windows ERROR_PRIVILEGE_NOT_HELD = 1314), so the test should be skipped.
    fn symlink_unsupported(error: &io::Error) -> bool {
        error.raw_os_error() == Some(1314)
    }

    fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link)
        }
    }

    fn symlink_file(target: &Path, link: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link)
        }
    }

    #[test]
    fn formats_iso_millis_like_javascript() {
        assert_eq!(format_iso_millis(0), "1970-01-01T00:00:00.000Z");
        // Date.UTC(2026, 0, 2, 3, 4, 5, 678)
        assert_eq!(
            format_iso_millis(1_767_323_045_678),
            "2026-01-02T03:04:05.678Z"
        );
        // Leap day: Date.UTC(2024, 1, 29, 23, 59, 59, 999)
        assert_eq!(
            format_iso_millis(1_709_251_199_999),
            "2024-02-29T23:59:59.999Z"
        );
        assert_eq!(format_iso_millis(-1), "1969-12-31T23:59:59.999Z");
        assert!(is_iso_millis(&now_iso()), "{}", now_iso());
    }

    #[test]
    fn writes_row_with_node_shape_and_field_order() {
        let temp = TempDir::new();
        let row = audit(
            &temp.0,
            "stage.selected",
            json!({"intent": "demo", "intentId": "i-1", "stage": "ideate", "rework": false}),
        )
        .unwrap();
        let keys: Vec<&str> = row
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["at", "event", "intent", "intentId", "stage", "rework"]
        );
        assert_eq!(row["event"], "stage.selected");
        assert!(is_iso_millis(row["at"].as_str().unwrap()));
        let text = fs::read_to_string(temp.0.join(".aidlc/audit.jsonl")).unwrap();
        assert!(text.ends_with('\n'));
        assert_eq!(text.matches('\n').count(), 1);
        let expected_tail = r#","event":"stage.selected","intent":"demo","intentId":"i-1","stage":"ideate","rework":false}"#;
        assert!(text.trim_end().ends_with(expected_tail), "{text}");
        assert!(text.starts_with(r#"{"at":""#));
        assert_eq!(read_lines(&temp.0), vec![row]);
    }

    #[test]
    fn detail_keys_named_at_or_event_override_in_place() {
        let temp = TempDir::new();
        let row = audit(&temp.0, "x", json!({"z": 1, "event": "y", "at": "fixed"})).unwrap();
        let keys: Vec<&str> = row
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["at", "event", "z"]);
        assert_eq!(row["at"], "fixed");
        assert_eq!(row["event"], "y");
    }

    #[test]
    fn appends_and_creates_state_dir() {
        let temp = TempDir::new();
        assert!(!temp.0.join(".aidlc").exists());
        audit(&temp.0, "one", Value::Null).unwrap();
        audit(&temp.0, "two", json!({"n": 2})).unwrap();
        let rows = read_lines(&temp.0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["event"], "one");
        assert_eq!(rows[0].as_object().unwrap().len(), 2);
        assert_eq!(rows[1]["event"], "two");
        assert_eq!(rows[1]["n"], 2);
    }

    #[test]
    fn rejects_non_object_details() {
        let temp = TempDir::new();
        assert!(audit(&temp.0, "x", json!([1, 2])).is_err());
        assert!(!temp.0.join(".aidlc/audit.jsonl").exists());
    }

    #[test]
    fn refuses_non_directory_state_paths() {
        let temp = TempDir::new();
        fs::write(temp.0.join(".aidlc"), "not a dir").unwrap();
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing unsafe .aidlc path"
        );

        let temp = TempDir::new();
        fs::create_dir_all(temp.0.join(".aidlc")).unwrap();
        fs::write(temp.0.join(".aidlc/intents"), "not a dir").unwrap();
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing unsafe .aidlc/intents path"
        );
        assert!(!temp.0.join(".aidlc/audit.jsonl").exists());
    }

    #[test]
    fn refuses_symlinked_state_dir() {
        let temp = TempDir::new();
        let target = temp.0.join("elsewhere");
        fs::create_dir_all(&target).unwrap();
        if let Err(error) = symlink_dir(&target, &temp.0.join(".aidlc")) {
            if symlink_unsupported(&error) {
                eprintln!("skipping: symlink creation not permitted ({error})");
                return;
            }
            panic!("{error}");
        }
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing to access symlink .aidlc directory"
        );
        assert!(!target.join("audit.jsonl").exists());
    }

    #[test]
    fn refuses_symlinked_intents_dir() {
        let temp = TempDir::new();
        let target = temp.0.join("elsewhere");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(temp.0.join(".aidlc")).unwrap();
        if let Err(error) = symlink_dir(&target, &temp.0.join(".aidlc/intents")) {
            if symlink_unsupported(&error) {
                eprintln!("skipping: symlink creation not permitted ({error})");
                return;
            }
            panic!("{error}");
        }
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing to write through symlink: .aidlc/intents"
        );
        assert!(!temp.0.join(".aidlc/audit.jsonl").exists());
    }

    #[test]
    fn refuses_symlinked_audit_file() {
        let temp = TempDir::new();
        let target = temp.0.join("outside.jsonl");
        fs::write(&target, "").unwrap();
        fs::create_dir_all(temp.0.join(".aidlc")).unwrap();
        if let Err(error) = symlink_file(&target, &temp.0.join(".aidlc/audit.jsonl")) {
            if symlink_unsupported(&error) {
                eprintln!("skipping: symlink creation not permitted ({error})");
                return;
            }
            panic!("{error}");
        }
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing to write through symlink: .aidlc/audit.jsonl"
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "");
        assert_eq!(
            preflight_audit(&temp.0).unwrap_err(),
            "Refusing to write through symlink: .aidlc/audit.jsonl"
        );
    }

    #[test]
    fn preflight_accepts_missing_or_regular_log_without_writing() {
        let temp = TempDir::new();
        preflight_audit(&temp.0).unwrap();
        assert!(!temp.0.join(".aidlc").exists());
        fs::create_dir_all(temp.0.join(".aidlc")).unwrap();
        preflight_audit(&temp.0).unwrap();
        assert!(!temp.0.join(".aidlc/audit.jsonl").exists());
        fs::write(temp.0.join(".aidlc/audit.jsonl"), "row\n").unwrap();
        preflight_audit(&temp.0).unwrap();
        assert_eq!(
            fs::read_to_string(temp.0.join(".aidlc/audit.jsonl")).unwrap(),
            "row\n"
        );
    }

    #[test]
    fn preflight_and_audit_refuse_a_directory_log() {
        let temp = TempDir::new();
        fs::create_dir_all(temp.0.join(".aidlc/audit.jsonl")).unwrap();
        assert_eq!(
            preflight_audit(&temp.0).unwrap_err(),
            "Refusing unsafe .aidlc/audit.jsonl path"
        );
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing unsafe .aidlc/audit.jsonl path"
        );
    }

    #[test]
    fn preflight_checks_the_state_dir() {
        let temp = TempDir::new();
        fs::write(temp.0.join(".aidlc"), "").unwrap();
        assert_eq!(
            preflight_audit(&temp.0).unwrap_err(),
            "Refusing unsafe .aidlc path"
        );
    }

    /// Junctions need no privilege on Windows, so this exercises the refusal path
    /// even when symlink tests are skipped. Node's lstat reports junctions as symlinks.
    #[cfg(windows)]
    #[test]
    fn refuses_junctioned_state_dir_on_windows() {
        let temp = TempDir::new();
        let target = temp.0.join("elsewhere");
        fs::create_dir_all(&target).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(temp.0.join(".aidlc"))
            .arg(&target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            eprintln!("skipping: could not create junction");
            return;
        }
        assert_eq!(
            audit(&temp.0, "x", Value::Null).unwrap_err(),
            "Refusing to access symlink .aidlc directory"
        );
        assert!(!target.join("audit.jsonl").exists());
    }
}
