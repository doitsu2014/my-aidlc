//! Golden tests for `dtech-aidlc ids check`, `repos ...` and `deps ...`.
//!
//! Each scenario builds a fixture (onboarded, plus an intent started where needed), runs
//! every step with the native handler (`dtech_aidlc::ids::run`,
//! `dtech_aidlc::dependencies::run_repos` / `run_deps`), then checks exit codes, JSON
//! output, error text, every project file (Git internals excluded) and the audit rows
//! written by the steps against `tests/golden/rust_repos_deps_ids/<test>.json`. Project
//! roots, temp paths, ISO timestamps, git commit ids and setup-time file mtimes are
//! normalised. The golden files were recorded from the reference CLI that the native
//! commands replace; see the `golden` module for re-blessing. `repos pull` needs `git`.

use dtech_aidlc::args::{command_key, parse_args};
use golden::{Golden, Masks, Tree};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const STEM: &str = "rust_repos_deps_ids";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A unique temporary directory outside the repository, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aidlc-rdi-{label}-{}-{}-{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
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

#[derive(Debug, PartialEq)]
struct Outcome {
    code: i32,
    stdout: Value,
    stderr: String,
}

/// Run the command-line program on `root` and capture its result.
fn cli(root: &Path, argv: &[&str]) -> Outcome {
    let mut program = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let out = program
        .args(argv)
        .arg("--project")
        .arg(root)
        // The test's working directory, so relative --file paths resolve alike.
        .current_dir(std::env::current_dir().unwrap())
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run dtech-aidlc");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let stdout = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };
    Outcome {
        code: out.status.code().unwrap_or(-1),
        stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Parse `argv` exactly like the binary does and call the native handler directly.
fn rust_cli(root: &Path, argv: &[&str]) -> Outcome {
    let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_owned()).collect();
    let parsed = parse_args(&argv).expect("valid arguments");
    let key = command_key(&parsed.positional).expect("a command");
    let words = key.split(' ').count().min(parsed.positional.len());
    let args = parsed.positional[words..].to_vec();
    let result = match parsed.positional[0].as_str() {
        "ids" => dtech_aidlc::ids::run(&key, root, &args, &parsed.flags),
        "repos" => dtech_aidlc::dependencies::run_repos(&key, root, &args, &parsed.flags),
        "deps" => dtech_aidlc::dependencies::run_deps(&key, root, &args, &parsed.flags),
        other => panic!("unexpected command {other}"),
    };
    match result {
        Ok((stdout, code)) => Outcome {
            code,
            stdout,
            stderr: String::new(),
        },
        Err(message) => Outcome {
            code: 1,
            stdout: Value::Null,
            stderr: format!("dtech-aidlc: {message}\n"),
        },
    }
}

/// Run one `ids`, `repos` or `deps` command on `root`.
fn run(root: &Path, argv: &[&str]) -> Outcome {
    rust_cli(root, argv)
}

fn root_variants(root: &Path) -> Vec<String> {
    let mut variants = vec![root.display().to_string()];
    if let Ok(canonical) = root.canonicalize() {
        let text = canonical.display().to_string();
        variants.push(text.trim_start_matches(r"\\?\").to_owned());
    }
    let more: Vec<String> = variants.iter().map(|v| v.replace('\\', "/")).collect();
    variants.extend(more);
    variants.sort_by_key(|v| std::cmp::Reverse(v.len()));
    variants.dedup();
    variants
}

/// Replace ISO-8601 timestamps such as `2026-01-02T03:04:05.678Z` with `<TS>`.
fn mask_timestamps(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let digit = |b: Option<&u8>| b.is_some_and(u8::is_ascii_digit);
    while i < bytes.len() {
        let shape = b"dddd-dd-ddTdd:dd:dd";
        let matches_shape = i + shape.len() <= bytes.len()
            && shape.iter().enumerate().all(|(k, s)| match s {
                b'd' => digit(bytes.get(i + k)),
                other => bytes[i + k] == *other,
            });
        if matches_shape {
            let mut j = i + shape.len();
            if bytes.get(j) == Some(&b'.') {
                j += 1;
                while digit(bytes.get(j)) {
                    j += 1;
                }
            }
            if bytes.get(j) == Some(&b'Z') {
                j += 1;
            }
            out.push_str("<TS>");
            i = j;
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn normalize_text(text: &str, root: &Path) -> String {
    let mut text = text.to_owned();
    for variant in root_variants(root) {
        text = text.replace(&variant, "<ROOT>");
    }
    mask_timestamps(&text)
}

fn normalize_value(value: &Value, root: &Path) -> Value {
    match value {
        Value::String(s) => Value::String(normalize_text(s, root)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| normalize_value(v, root)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), normalize_value(v, root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn mask_mtimes(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "mtimeMs" {
                    *item = json!("<MTIME>");
                } else {
                    mask_mtimes(item);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask_mtimes),
        _ => {}
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                if entry.file_name() == ".git" {
                    continue;
                }
                out.insert(format!("{rel}/"), String::new());
                walk(base, &path, out);
            } else if rel != ".aidlc/audit.jsonl" {
                let bytes = fs::read(&path).unwrap_or_default();
                let mut text = normalize_text(&String::from_utf8_lossy(&bytes), base);
                // Setup-time file modification times are not produced by the commands.
                if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
                    mask_mtimes(&mut value);
                    text = serde_json::to_string_pretty(&value).unwrap();
                }
                out.insert(rel, text);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn audit_rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row.as_object_mut().unwrap().remove("at");
            normalize_value(&row, root)
        })
        .collect()
}

/// The checked form of an outcome, with project paths and timestamps normalised.
fn shown(outcome: &Outcome, root: &Path, masks: &Masks) -> Value {
    masks.output(&json!({
        "code": outcome.code,
        "stdout": normalize_value(&outcome.stdout, root),
        "stderr": normalize_text(&outcome.stderr, root),
    }))
}

/// One project whose steps are checked against a golden case.
struct Pair {
    dir: TempDir,
    golden: Golden,
    masks: Masks,
    /// Files after the previous step; the fixture as built before the first step, so
    /// fixture-building commands are not part of the expectation.
    files: Option<Tree>,
    audit: Vec<Value>,
}

impl Pair {
    fn new(golden: &Golden, label: &str, setup: impl Fn(&Path)) -> Self {
        let dir = TempDir::new(label);
        setup(&dir.0);
        let masks = Masks::new()
            .path(&dir.0, "<ROOT>")
            .path(&std::env::current_dir().unwrap(), "<CWD>");
        let audit = audit_rows(&dir.0);
        golden.check(json!({"fixture": label}));
        Pair {
            dir,
            golden: golden.clone(),
            masks,
            files: None,
            audit,
        }
    }

    /// Show `text` (a path or value outside the project) as `placeholder`.
    fn mask(&mut self, path: Option<&Path>, text: &str, placeholder: &str) {
        let masks = std::mem::take(&mut self.masks);
        self.masks = match path {
            Some(path) => masks.path(path, placeholder),
            None => masks.text(text, placeholder),
        };
    }

    fn tree(&self) -> Tree {
        snapshot(&self.dir.0)
            .into_iter()
            .map(|(path, text)| (path, self.masks.file(text.as_bytes())))
            .collect()
    }

    /// Run one step and check its output, the project files and the new audit rows.
    fn step(&mut self, argv: &[&str]) -> Outcome {
        let before = self.files.take().unwrap_or_else(|| self.tree());
        let mut outcome = run(&self.dir.0, argv);
        // The native handler prints `note:` lines itself (eprintln!), which this in-process
        // harness cannot capture; they are left out here and checked through the binary.
        outcome.stderr = outcome
            .stderr
            .lines()
            .filter(|line| !line.starts_with("dtech-aidlc: note:"))
            .map(|line| format!("{line}\n"))
            .collect();
        let files = self.tree();
        let audit: Vec<Value> = audit_rows(&self.dir.0)
            .iter()
            .map(|row| self.masks.value(row))
            .collect();
        self.golden.check(json!({
            "argv": self.masks.value(&json!(argv)),
            "outcome": shown(&outcome, &self.dir.0, &self.masks),
            "files": golden::tree_delta(&before, &files),
            "audit": golden::audit_delta(&self.audit, &audit),
        }));
        self.files = Some(files);
        self.audit = audit;
        outcome
    }
}

fn onboard(root: &Path) {
    let out = cli(root, &["onboard", "--harness", "claude", "--scope", "solo"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Golden expectations stored under `tests/golden/<test file>/<case>.json`.
///
/// Each case is a JSON array of entries checked in order. Machine-specific text (temp
/// project roots, the working directory, ISO timestamps, file mtimes, git commit ids,
/// the engine version and CRLF line endings) is replaced by placeholders before an entry
/// is compared, so the files match on Windows and POSIX. Set `DTECH_AIDLC_BLESS=1` to
/// rewrite the golden files from the current output after an intentional change.
#[allow(dead_code)]
mod golden {
    use serde_json::{json, Map, Value};
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    /// Relative path (directories end in `/`) -> canonical file content.
    pub type Tree = BTreeMap<String, Value>;

    /// Canonical text longer than this is stored as a SHA-256 digest instead of inline.
    const INLINE_LIMIT: usize = 1500;

    /// JSON keys whose number is a file size.
    const SIZE_KEYS: [&str; 2] = ["bytes", "size"];

    pub fn blessing() -> bool {
        std::env::var("DTECH_AIDLC_BLESS").is_ok_and(|value| value == "1")
    }

    /// Literal replacements applied before the generic placeholders.
    #[derive(Clone, Default)]
    pub struct Masks {
        texts: Vec<(String, String)>,
        /// Working-tree size -> LF size of repository files copied into projects.
        sizes: Vec<(u64, u64)>,
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(bytes))
    }

    impl Masks {
        /// Masks that also show repository files copied into projects (the scaffold and
        /// harness files, CRLF in a Windows checkout) with their LF size and digest.
        pub fn new() -> Self {
            fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
                for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, out);
                    } else {
                        out.push(path);
                    }
                }
            }
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let mut files = Vec::new();
            walk(&root.join("core/data/scaffold"), &mut files);
            walk(&root.join("harness"), &mut files);
            let mut masks = Masks::default();
            for file in files {
                let raw = fs::read(&file).unwrap();
                let lf = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
                if raw != lf.as_bytes() {
                    masks = masks.text(&sha256_hex(&raw), &sha256_hex(lf.as_bytes()));
                    for key in SIZE_KEYS {
                        for end in [",", "\n", "}"] {
                            for colon in [": ", ":"] {
                                masks = masks.text(
                                    &format!("\"{key}\"{colon}{}{end}", raw.len()),
                                    &format!("\"{key}\"{colon}{}{end}", lf.len()),
                                );
                            }
                        }
                    }
                    masks.sizes.push((raw.len() as u64, lf.len() as u64));
                }
            }
            masks
        }

        /// Replace every spelling of `path` (as given, canonical, forward slashes, JSON
        /// escaped) with `placeholder`.
        pub fn path(mut self, path: &Path, placeholder: &str) -> Self {
            let mut forms = vec![path.display().to_string()];
            if let Ok(canonical) = fs::canonicalize(path) {
                let text = canonical.display().to_string();
                forms.push(text.trim_start_matches(r"\\?\").to_owned());
                forms.push(text);
            }
            let more: Vec<String> = forms
                .iter()
                .flat_map(|form| [form.replace('\\', "/"), form.replace('\\', "\\\\")])
                .collect();
            forms.extend(more);
            for form in forms {
                self = self.text(&form, placeholder);
            }
            self
        }

        pub fn text(mut self, from: &str, to: &str) -> Self {
            if !from.is_empty() && !self.texts.iter().any(|(known, _)| known == from) {
                self.texts.push((from.to_owned(), to.to_owned()));
                self.texts
                    .sort_by_key(|(known, _)| std::cmp::Reverse(known.len()));
            }
            self
        }

        /// The canonical form of `text`.
        pub fn apply(&self, text: &str) -> String {
            let mut text = text.replace("\r\n", "\n");
            for (from, to) in &self.texts {
                text = text.replace(from, to);
            }
            let text = mask_timestamps(&text);
            let text = mask_version(&text);
            let text = mask_mtimes(&text);
            mask_commits(&text)
        }

        /// `apply` on every string (and object key) inside `value`.
        pub fn value(&self, value: &Value) -> Value {
            match value {
                Value::String(text) => Value::String(self.apply(text)),
                Value::Array(items) => Value::Array(items.iter().map(|v| self.value(v)).collect()),
                Value::Object(map) => Value::Object(
                    map.iter()
                        .map(|(key, item)| {
                            let item = match item.as_u64() {
                                Some(size) if SIZE_KEYS.contains(&key.as_str()) => self
                                    .sizes
                                    .iter()
                                    .find(|(raw, _)| *raw == size)
                                    .map_or_else(|| item.clone(), |(_, lf)| json!(lf)),
                                _ => self.value(item),
                            };
                            (self.apply(key), item)
                        })
                        .collect(),
                ),
                other => other.clone(),
            }
        }

        /// The canonical form of a command result: `value`, with path separators in every
        /// string shown as `/` so Windows and POSIX messages match.
        pub fn output(&self, value: &Value) -> Value {
            slashes(&self.value(value))
        }

        /// Canonical file content: inline text, or a digest for long files.
        pub fn file(&self, bytes: &[u8]) -> Value {
            let text = self.apply(&String::from_utf8_lossy(bytes));
            if text.len() <= INLINE_LIMIT {
                Value::String(text)
            } else {
                Value::String(format!(
                    "sha256:{} ({} bytes)",
                    sha256_hex(text.as_bytes()),
                    text.len()
                ))
            }
        }
    }

    fn slashes(value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(text.replace('\\', "/")),
            Value::Array(items) => Value::Array(items.iter().map(slashes).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, item)| (key.clone(), slashes(item)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// `2026-01-02T03:04:05.678Z` (fraction and `Z` optional) -> `<TS>`.
    fn mask_timestamps(text: &str) -> String {
        let bytes = text.as_bytes();
        let digit = |b: Option<&u8>| b.is_some_and(u8::is_ascii_digit);
        let shape = b"dddd-dd-ddTdd:dd:dd";
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < bytes.len() {
            let matches_shape = i + shape.len() <= bytes.len()
                && shape.iter().enumerate().all(|(k, s)| match s {
                    b'd' => digit(bytes.get(i + k)),
                    other => bytes[i + k] == *other,
                });
            if matches_shape {
                let mut j = i + shape.len();
                if bytes.get(j) == Some(&b'.') && digit(bytes.get(j + 1)) {
                    j += 1;
                    while digit(bytes.get(j)) {
                        j += 1;
                    }
                }
                if bytes.get(j) == Some(&b'Z') {
                    j += 1;
                }
                out.push_str("<TS>");
                i = j;
                continue;
            }
            let c = text[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
        out
    }

    /// The engine version as a whole token (not part of a longer version) -> `<VERSION>`.
    fn mask_version(text: &str) -> String {
        let version = env!("CARGO_PKG_VERSION");
        let part = |c: char| c.is_ascii_digit() || c == '.';
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(index) = rest.find(version) {
            let before = rest[..index].chars().next_back();
            let after = rest[index + version.len()..].chars().next();
            out.push_str(&rest[..index]);
            if before.is_some_and(part) || after.is_some_and(part) {
                out.push_str(version);
            } else {
                out.push_str("<VERSION>");
            }
            rest = &rest[index + version.len()..];
        }
        out.push_str(rest);
        out
    }

    /// `"mtimeMs": 1712345678901.25` -> `"mtimeMs": "<MTIME>"` (setup-time file times).
    fn mask_mtimes(text: &str) -> String {
        let key = "\"mtimeMs\":";
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(index) = rest.find(key) {
            let end = index + key.len();
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            let spaces = rest.len() - rest.trim_start_matches(' ').len();
            out.push_str(&rest[..spaces]);
            rest = &rest[spaces..];
            let number = rest
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')))
                .unwrap_or(rest.len());
            if number > 0 {
                out.push_str("\"<MTIME>\"");
                rest = &rest[number..];
            }
        }
        out.push_str(rest);
        out
    }

    /// A run of exactly 40 hexadecimal digits (a git commit id) -> `<COMMIT>`.
    fn mask_commits(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_hexdigit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
                let run = bytes[i..]
                    .iter()
                    .take_while(|b| b.is_ascii_hexdigit())
                    .count();
                let next = bytes.get(i + run);
                if run == 40 && !next.is_some_and(u8::is_ascii_alphanumeric) {
                    out.push_str("<COMMIT>");
                    i += run;
                    continue;
                }
                out.push_str(&text[i..i + run]);
                i += run;
                continue;
            }
            let c = text[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
        out
    }

    /// Files added, changed or removed between two trees (`{}` when nothing changed).
    pub fn tree_delta(before: &Tree, after: &Tree) -> Value {
        let changed: Map<String, Value> = after
            .iter()
            .filter(|(path, content)| before.get(*path) != Some(content))
            .map(|(path, content)| (path.clone(), content.clone()))
            .collect();
        let removed: Vec<&String> = before
            .keys()
            .filter(|path| !after.contains_key(*path))
            .collect();
        let mut delta = Map::new();
        if !changed.is_empty() {
            delta.insert("changed".into(), Value::Object(changed));
        }
        if !removed.is_empty() {
            delta.insert("removed".into(), json!(removed));
        }
        Value::Object(delta)
    }

    /// Audit rows appended since `before`, or the whole log if earlier rows changed.
    pub fn audit_delta(before: &[Value], after: &[Value]) -> Value {
        if after.starts_with(before) {
            json!({"appended": after[before.len()..]})
        } else {
            json!({"rewritten": after})
        }
    }

    /// One golden case, shared by every handle; checked entry by entry.
    #[derive(Clone)]
    pub struct Golden(Rc<RefCell<Inner>>);

    struct Inner {
        file: PathBuf,
        bless: bool,
        skipped: bool,
        expected: Vec<Value>,
        seen: Vec<Value>,
    }

    impl Golden {
        pub fn new(stem: &str, case: &str) -> Self {
            let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/golden")
                .join(stem)
                .join(format!("{case}.json"));
            let bless = blessing();
            let expected = if bless {
                Vec::new()
            } else {
                let text = fs::read_to_string(&file).unwrap_or_else(|error| {
                    panic!(
                        "cannot read golden file {} ({error}); record it with DTECH_AIDLC_BLESS=1",
                        file.display()
                    )
                });
                match serde_json::from_str(&text) {
                    Ok(Value::Array(entries)) => entries,
                    _ => panic!("golden file {} is not a JSON array", file.display()),
                }
            };
            Golden(Rc::new(RefCell::new(Inner {
                file,
                bless,
                skipped: false,
                expected,
                seen: Vec::new(),
            })))
        }

        /// Neither check nor rewrite this case (the test could not run here).
        pub fn skip(&self) {
            self.0.borrow_mut().skipped = true;
        }

        /// Compare the next entry with the golden file (or record it when blessing).
        pub fn check(&self, entry: Value) {
            let mut inner = self.0.borrow_mut();
            let index = inner.seen.len();
            if !inner.bless {
                let actual = serde_json::to_string_pretty(&entry).unwrap();
                match inner.expected.get(index) {
                    Some(expected) => {
                        let expected = serde_json::to_string_pretty(expected).unwrap();
                        assert!(
                            actual == expected,
                            "{} entry {index} differs (rerun with DTECH_AIDLC_BLESS=1 only if the change is intended)\n--- expected\n{expected}\n+++ actual\n{actual}",
                            inner.file.display()
                        );
                    }
                    None => panic!(
                        "{} has no entry {index}; actual:\n{actual}",
                        inner.file.display()
                    ),
                }
            }
            inner.seen.push(entry);
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            if self.skipped || std::thread::panicking() {
                return;
            }
            if self.bless {
                fs::create_dir_all(self.file.parent().unwrap()).unwrap();
                let text =
                    serde_json::to_string_pretty(&Value::Array(std::mem::take(&mut self.seen)))
                        .unwrap();
                fs::write(&self.file, format!("{text}\n")).unwrap();
            } else {
                assert_eq!(
                    self.seen.len(),
                    self.expected.len(),
                    "{}: the test produced fewer entries than the golden file holds",
                    self.file.display()
                );
            }
        }
    }
} // mod golden

fn test_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A bare repository with one commit on `main`, shared by both projects so that the
/// URL and commit ids are identical.
fn bare_remote(base: &Path) -> PathBuf {
    let remote = base.join("remote.git");
    let seed = base.join("seed");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&seed).unwrap();
    test_git(&remote, &["init", "--bare"]);
    test_git(&seed, &["init"]);
    test_git(&seed, &["config", "user.email", "test@example.invalid"]);
    test_git(&seed, &["config", "user.name", "Test"]);
    fs::write(seed.join("tracked.txt"), "base\n").unwrap();
    test_git(&seed, &["add", "."]);
    test_git(&seed, &["commit", "-m", "base"]);
    test_git(&seed, &["branch", "-M", "main"]);
    test_git(
        &seed,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    test_git(&seed, &["push", "-u", "origin", "main"]);
    test_git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    remote
}

fn start_intent(root: &Path) {
    onboard(root);
    let out = cli(
        root,
        &["orchestrate", "next", "--intent", "Build a sample feature"],
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
}

const INTENT_DIR: &str = ".aidlc/intents/build-a-sample-feature";

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_citations(root: &Path, registry: &str) {
    write(
        root,
        &format!("{INTENT_DIR}/develop/implementation-design/ids.json"),
        registry,
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/implementation-design/work-units.json"),
        r#"{"units":[{"id":"WU-02"},{"id":"U-03"},{"name":"no id"}]}"#,
    );
    write(
        root,
        &format!("{INTENT_DIR}/analyze/problem-framing/acceptance-criteria.md"),
        "AC-1 and AC-2\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/analyze/problem-framing/user-stories.md"),
        "US-1\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/analyze/problem-framing/problem-statement.md"),
        "Cites FN-404 and AC-77.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/implementation-design/functional-design.md"),
        "FN-01 FN-03 AC-1 AC-9 US-2 WU-02 U-03 U-01 IF-1 CFG-1 TS-4 FN-03 xFN-5 FN-123456\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/red-tests/sub/notes.md"),
        "Nested FN-77.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/red-tests/a-first.md"),
        "FN-01 only.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/build-and-verify/green-verification-report.md"),
        "Covers FN-01.\n",
    );
    write(
        root,
        &format!("{INTENT_DIR}/develop/build-and-verify/other.txt"),
        "FN-88\n",
    );
}

const REGISTRY: &str = r#"{"schemaVersion":1,"ids":{"FN":[{"id":"FN-01"},{"id":"FN-02"},{"id":"FN-01"}],"IF":[{"id":"IF-1"}],"WU":[{"id":"U-01"}],"7":[{"id":"CFG-1"}],"TS":"not an array","CFG":[{"title":"missing id"}]}}"#;

#[test]
fn ids_check_matches_golden() {
    let golden = Golden::new(STEM, "ids_check_matches_golden");
    let mut idle = Pair::new(&golden, "ids-idle", onboard);
    idle.step(&["ids", "check"]);
    idle.step(&["ids", "bogus"]);

    let mut empty = Pair::new(&golden, "ids-empty", start_intent);
    let out = empty.step(&["ids", "check"]);
    assert_eq!((out.code, out.stdout["ok"].clone()), (0, json!(true)));
    empty.step(&["ids", "check", "--stage", "problem-framing"]);
    empty.step(&["ids", "check", "--stage", "release-readiness"]);
    empty.step(&["ids", "check", "--stage", "no-such-stage"]);
    empty.step(&["ids", "bogus"]);

    let mut cited = Pair::new(&golden, "ids-cited", |root| {
        start_intent(root);
        write_citations(root, REGISTRY);
    });
    let out = cited.step(&["ids", "check"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout["uncoveredFunctions"], json!(["FN-02"]));
    cited.step(&["ids", "check", "--stage", "build-and-verify"]);
    cited.step(&["ids", "check", "--stage", "red-tests"]);
    cited.step(&["ids", "check", "--stage", "problem-framing"]);

    for (label, registry) in [
        ("ids-malformed", "{not json"),
        (
            "ids-na",
            r#"{"status":"not-applicable","reason":"no code"}"#,
        ),
        (
            "ids-array",
            r#"{"schemaVersion":1,"ids":[[{"id":"FN-01"}]]}"#,
        ),
    ] {
        let mut pair = Pair::new(&golden, label, |root| {
            start_intent(root);
            write_citations(root, registry);
        });
        pair.step(&["ids", "check"]);
        pair.step(&["ids", "check", "--stage", "implementation-design"]);
    }
}

#[test]
fn deps_commands_match_golden() {
    let golden = Golden::new(STEM, "deps_commands_match_golden");
    let mut idle = Pair::new(&golden, "deps-idle", onboard);
    for argv in [
        vec!["deps", "list"],
        vec!["deps", "add", "a", "b"],
        vec!["deps", "pull", "a"],
        vec!["deps", "bogus"],
    ] {
        let out = idle.step(&argv);
        assert_eq!(out.code, 1);
    }
    let mut active = Pair::new(&golden, "deps-active", start_intent);
    for argv in [
        vec!["deps", "list"],
        vec!["deps", "add", "x", "problem-framing"],
        vec!["deps", "add", "x", "missing-stage"],
        vec!["deps", "add", "x", "problem-framing"],
        vec!["deps", "add", "y", "x"],
        vec!["deps", "add", "x", "x"],
        vec!["deps", "add", "bad id", "x"],
        vec!["deps", "add", "x", "-bad"],
        vec!["deps", "add", "x"],
        vec!["deps", "add"],
        vec!["deps", "list"],
        vec!["deps", "pull", "x"],
        vec!["deps", "pull", "y"],
        vec!["deps", "pull", "nothing"],
        vec!["deps", "pull"],
        vec!["deps", "pull", "bad id"],
        vec!["deps", "bogus"],
    ] {
        active.step(&argv);
    }
}

#[test]
fn repos_registration_matches_golden() {
    let golden = Golden::new(STEM, "repos_registration_matches_golden");
    let mut pair = Pair::new(&golden, "repos-reg", onboard);
    for argv in [
        vec!["repos", "list"],
        vec![
            "repos",
            "add",
            "lib",
            "https://example.invalid/org/lib.git",
            "--ref",
            "main",
        ],
        vec![
            "repos",
            "add",
            "lib",
            "https://example.invalid/org/lib.git",
            "--ref",
            "main",
        ],
        vec![
            "repos",
            "add",
            "lib",
            "https://example.invalid/org/other.git",
        ],
        vec!["repos", "add", "tool", "git@example.invalid:org/tool.git"],
        vec![
            "repos",
            "add",
            "ssh",
            "ssh://git@example.invalid:2222/org/ssh.git",
        ],
        vec![
            "repos",
            "add",
            "creds",
            "https://user:secret@example.invalid/x.git",
        ],
        vec!["repos", "add", "user", "https://user@example.invalid/x.git"],
        vec![
            "repos",
            "add",
            "sshpass",
            "ssh://git:pw@example.invalid/x.git",
        ],
        vec!["repos", "add", "rel", "relative/path"],
        vec!["repos", "add", "ext", "ext::sh -c x"],
        vec!["repos", "add", "dash", "-upload-pack"],
        vec!["repos", "add", "dots", "git@example.invalid:org/../x.git"],
        vec!["repos", "add", "query", "git@example.invalid:org/x.git?y"],
        vec![
            "repos",
            "add",
            "port",
            "https://example.invalid:99999/x.git",
        ],
        vec!["repos", "add", "-bad", "https://example.invalid/x.git"],
        vec![
            "repos",
            "add",
            "badref",
            "https://example.invalid/x.git",
            "--ref",
            "a..b",
        ],
        vec![
            "repos",
            "add",
            "badref2",
            "https://example.invalid/x.git",
            "--ref",
            "-x",
        ],
        vec!["repos", "add", "only-name"],
        vec!["repos", "add"],
        vec!["repos", "list"],
        vec!["repos", "remove", "unknown"],
        vec!["repos", "remove", "tool"],
        vec!["repos", "remove"],
        vec!["repos", "list"],
        vec!["repos", "bogus"],
        vec!["repos"],
    ] {
        pair.step(&argv);
    }
    // A declared aidlc.deps.json is edited alongside the runtime copy.
    let mut declared = Pair::new(&golden, "repos-declared", |root| {
        onboard(root);
        fs::write(
            root.join("aidlc.deps.json"),
            "{\n  \"schemaVersion\": 1,\n  \"repos\": {\n    \"kept\": {\n      \"url\": \"https://example.invalid/kept.git\"\n    }\n  }\n}\n",
        )
        .unwrap();
    });
    for argv in [
        vec!["repos", "add", "lib", "https://example.invalid/lib.git"],
        vec!["repos", "add", "kept", "https://example.invalid/kept.git"],
        vec![
            "repos",
            "add",
            "kept",
            "https://example.invalid/changed.git",
        ],
        vec!["repos", "list"],
        vec!["repos", "remove", "kept"],
        vec!["repos", "remove", "kept"],
        vec!["repos", "list"],
    ] {
        declared.step(&argv);
    }
    // Invalid manifests are reported, never rewritten.
    for (label, file, text) in [
        ("repos-badjson", ".aidlc/deps.json", "{oops"),
        (
            "repos-badshape",
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":[]}"#,
        ),
        (
            "repos-badentry",
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":{"a":{"url":"https://example.invalid/a.git","ref":5}}}"#,
        ),
        (
            "repos-baddeclared",
            "aidlc.deps.json",
            r#"{"schemaVersion":2,"repos":{}}"#,
        ),
    ] {
        let mut pair = Pair::new(&golden, label, |root| {
            onboard(root);
            write(root, file, text);
        });
        pair.step(&["repos", "list"]);
        pair.step(&["repos", "add", "z", "https://example.invalid/z.git"]);
        pair.step(&["repos", "remove", "a"]);
    }
}

#[test]
fn repos_locking_matches_golden() {
    let golden = Golden::new(STEM, "repos_locking_matches_golden");
    let live = std::process::id();
    let mut finished = Command::new("git").arg("--version").spawn().unwrap();
    let dead = finished.id();
    finished.wait().unwrap();
    for (label, lock, expect_ok) in [
        (
            "lock-live",
            format!("{{\"pid\":{live},\"at\":\"2026-01-01T00:00:00.000Z\"}}\n"),
            false,
        ),
        ("lock-noat", format!("{{\"pid\":{live}}}\n"), false),
        ("lock-garbage", "not json".to_owned(), false),
        (
            "lock-dead",
            format!("{{\"pid\":{dead},\"at\":\"2026-01-01T00:00:00.000Z\"}}\n"),
            true,
        ),
    ] {
        let mut pair = Pair::new(&golden, label, |root| {
            onboard(root);
            write(root, ".aidlc/deps.lock", &lock);
        });
        for (pid, placeholder) in [(live, "<LIVE_PID>"), (dead, "<DEAD_PID>")] {
            pair.mask(
                None,
                &format!("\"pid\": {pid}"),
                &format!("\"pid\": {placeholder}"),
            );
            pair.mask(
                None,
                &format!("process {pid} "),
                &format!("process {placeholder} "),
            );
        }
        let out = pair.step(&["repos", "add", "lib", "https://example.invalid/lib.git"]);
        assert_eq!(out.code == 0, expect_ok, "{label}: {out:?}");
        pair.step(&["repos", "list"]);
    }
}

#[test]
fn repos_pull_matches_golden() {
    let golden = Golden::new(STEM, "repos_pull_matches_golden");
    if !git_available() {
        eprintln!("git is not on PATH; skipping repos pull");
        golden.skip();
        return;
    }
    let shared = TempDir::new("remote");
    let remote = bare_remote(&shared.0);
    let url = remote.to_str().unwrap().to_owned();
    let declared = format!(
        "{}\n",
        serde_json::to_string_pretty(&json!({"schemaVersion":1,"repos":{"lib":{"url":url}}}))
            .unwrap()
    );
    let setup = |root: &Path| {
        onboard(root);
        fs::write(root.join("aidlc.deps.json"), &declared).unwrap();
        // A runtime-only entry is dropped (with a note) when the declared file is imported.
        write(
            root,
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":{"stale":{"url":"https://example.invalid/stale.git"}}}"#,
        );
    };
    let mut pair = Pair::new(&golden, "pull", setup);
    pair.mask(Some(&remote), "", "<REMOTE>");
    let first = pair.step(&["repos", "pull"]);
    assert_eq!(first.code, 0, "{first:?}");
    assert_eq!(first.stdout[0]["ok"], json!(true));
    // The note goes to standard error, which only the binary exposes.
    let probe = TempDir::new("pull-note");
    setup(&probe.0);
    let noted = cli(&probe.0, &["repos", "pull"]);
    assert!(
        noted.stderr.contains("dtech-aidlc: note: removed from the runtime copy because they are not declared in aidlc.deps.json: stale."),
        "{}",
        noted.stderr
    );
    pair.step(&["repos", "list"]);
    pair.step(&["repos", "pull", "lib"]);
    pair.step(&["repos", "pull", "unknown"]);
    pair.step(&["repos", "pull", "-bad"]);
    // A dirty checkout is preserved and reported as a failed row with exit code 1.
    fs::write(
        pair.dir.0.join(".aidlc/repos/lib/tracked.txt"),
        "local change\n",
    )
    .unwrap();
    let dirty = pair.step(&["repos", "pull"]);
    assert_eq!(dirty.code, 1);
    assert_eq!(dirty.stdout[0]["ok"], json!(false));
    pair.step(&["repos", "remove", "lib"]);
    pair.step(&["repos", "list"]);

    // `--file` imports another manifest; an unmanaged directory blocks the pull.
    let other = TempDir::new("declared");
    let file = other.0.join("deps.json");
    fs::write(&file, &declared).unwrap();
    let file = file.to_str().unwrap().to_owned();
    let mut with_file = Pair::new(&golden, "pull-file", |root| {
        onboard(root);
        write(root, ".aidlc/repos/lib/README.md", "not a checkout\n");
    });
    with_file.mask(Some(&remote), "", "<REMOTE>");
    with_file.mask(Some(Path::new(&file)), "", "<DEPS_FILE>");
    with_file.step(&["repos", "pull", "--file", &file]);
    with_file.step(&["repos", "pull", "--file", "missing-deps.json"]);
    with_file.step(&["repos", "list"]);
}
