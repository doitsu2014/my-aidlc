//! Golden tests for the native `config ...` commands (`dtech_aidlc::config_commands::run`).
//!
//! Each scenario builds one fixture (an onboarded project, an empty directory or a
//! hand-written configuration), runs a sequence of commands and checks after every step
//! the JSON output (paths and timestamps normalised), error texts, exit codes, every
//! project file (except `audit.jsonl`) and the audit rows without their `at` field
//! against `tests/golden/rust_config_commands/<test>.json`. The golden files were
//! recorded from the reference CLI that the native commands replace; see the `golden`
//! module for the placeholders and for re-blessing.

use dtech_aidlc::args::{command_key, parse_args};
use dtech_aidlc::config_commands;
use golden::{Golden, Masks, Tree};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const STEM: &str = "rust_config_commands";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aidlc-config-golden-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn graph() -> Value {
    serde_json::from_slice(&fs::read(repo().join("core/data/phase-graph.json")).unwrap()).unwrap()
}

/// Run the command-line program (used to build onboarded fixtures).
fn cli(project: &Path, words: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(words)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap()
}

/// An onboarded project (solo scope) used as the starting fixture.
fn onboarded(harness: &str) -> Dir {
    let dir = Dir::new("template");
    let out = cli(
        &dir.0,
        &["onboard", "--harness", harness, "--scope", "solo"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Ok(Value, i32),
    Err(String),
}

fn normalize_text(text: &str, roots: &[&Path]) -> String {
    let mut text = text.to_owned();
    for root in roots {
        text = text.replace(&root.display().to_string(), "<ROOT>");
    }
    text
}

fn normalize(value: &Value, roots: &[&Path]) -> Value {
    match value {
        Value::String(text) => Value::String(normalize_text(text, roots)),
        Value::Array(items) => Value::Array(items.iter().map(|v| normalize(v, roots)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| {
                    let v = if key == "startedAt" {
                        json!("<TIME>")
                    } else {
                        normalize(v, roots)
                    };
                    (key.clone(), v)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn run_rust(project: &Path, words: &[&str]) -> Outcome {
    let argv: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let parsed = parse_args(&argv).unwrap();
    let key = command_key(&parsed.positional).unwrap();
    let count = key.split(' ').count();
    let args = parsed.positional[count.min(parsed.positional.len())..].to_vec();
    match config_commands::run(&key, project, &args, &parsed.flags) {
        Ok((value, code)) => Outcome::Ok(normalize(&value, &[project]), code),
        Err(message) => Outcome::Err(normalize_text(&message, &[project])),
    }
}

/// Run one command on `project`.
fn run(project: &Path, words: &[&str]) -> Outcome {
    run_rust(project, words)
}

/// Every file under the project except the audit log, as relative path -> canonical content.
fn snapshot(root: &Path, masks: &Masks) -> Tree {
    fn walk(root: &Path, dir: &Path, masks: &Masks, out: &mut Tree) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                out.insert(rel, json!("<symlink>"));
            } else if kind.is_dir() {
                out.insert(format!("{rel}/"), json!(""));
                walk(root, &path, masks, out);
            } else if rel != ".aidlc/audit.jsonl" {
                out.insert(rel, masks.file(&fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Tree::new();
    walk(root, root, masks, &mut out);
    out
}

fn audit_rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join(".aidlc/audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row.as_object_mut().unwrap().remove("at");
            row
        })
        .collect()
}

/// One project whose command results are checked against a golden case step by step.
struct Pair {
    dir: Dir,
    golden: Golden,
    masks: Masks,
    files: RefCell<Tree>,
    audit: RefCell<Vec<Value>>,
}

impl Pair {
    fn from(golden: &Golden, template: &Path) -> Self {
        let pair = Pair::empty(golden);
        copy_tree(template, &pair.dir.0);
        pair
    }
    fn empty(golden: &Golden) -> Self {
        let dir = Dir::new("project");
        let masks = Masks::new().path(&dir.0, "<ROOT>");
        Pair {
            dir,
            golden: golden.clone(),
            masks,
            files: RefCell::new(Tree::new()),
            audit: RefCell::new(Vec::new()),
        }
    }
    fn write(&self, rel: &str, text: &str) {
        let path = self.dir.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    /// Run one command and check output, changed files and new audit rows.
    fn step(&self, words: &[&str]) -> Outcome {
        let outcome = run(&self.dir.0, words);
        let files = snapshot(&self.dir.0, &self.masks);
        let audit: Vec<Value> = audit_rows(&self.dir.0)
            .iter()
            .map(|row| self.masks.value(row))
            .collect();
        let shown = match &outcome {
            Outcome::Ok(value, code) => json!({"ok": self.masks.output(value), "code": code}),
            Outcome::Err(message) => json!({"error": self.masks.output(&json!(message))}),
        };
        self.golden.check(json!({
            "argv": words,
            "outcome": shown,
            "files": golden::tree_delta(&self.files.borrow(), &files),
            "audit": golden::audit_delta(&self.audit.borrow(), &audit),
        }));
        *self.files.borrow_mut() = files;
        *self.audit.borrow_mut() = audit;
        outcome
    }
}

fn expect_err(outcome: &Outcome) {
    assert!(
        matches!(outcome, Outcome::Err(_)),
        "expected an error, got {outcome:?}"
    );
}

fn expect_ok(outcome: &Outcome) {
    assert!(
        matches!(outcome, Outcome::Ok(..)),
        "expected success, got {outcome:?}"
    );
}

/// Link `link` to directory `target`: a symlink, or on Windows without symlink rights a
/// junction (treated as a symbolic link).
fn link_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
            || Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()
                .is_ok_and(|out| out.status.success())
    }
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

#[test]
fn config_scope_matches_golden() {
    let golden = Golden::new(STEM, "config_scope_matches_golden");
    let template = onboarded("claude");
    let pair = Pair::from(&golden, &template.0);
    expect_ok(&pair.step(&["config", "scope"]));
    expect_err(&pair.step(&["config", "scope", "galaxy"]));
    expect_ok(&pair.step(&["config", "scope", "solo"]));
    expect_ok(&pair.step(&["config", "scope", "team"]));
    expect_ok(&pair.step(&["config", "scope"]));
    expect_ok(&pair.step(&["config", "scope", "enterprise"]));

    let fresh = Pair::empty(&golden);
    expect_ok(&fresh.step(&["config", "scope"]));
    expect_err(&fresh.step(&["config", "scope", "team"]));
    assert!(!fresh.dir.0.join(".aidlc").exists());

    let broken = Pair::empty(&golden);
    broken.write(".aidlc/config.json", r#"{"scope":"solo"}"#);
    expect_ok(&broken.step(&["config", "scope"]));
    expect_err(&broken.step(&["config", "scope", "team"]));
    broken.write(".aidlc/config.json", "{not json");
    expect_ok(&broken.step(&["config", "scope"]));
    expect_err(&broken.step(&["config", "scope", "team"]));
}

#[test]
fn config_mode_matches_golden() {
    let golden = Golden::new(STEM, "config_mode_matches_golden");
    let template = onboarded("claude");
    let pair = Pair::from(&golden, &template.0);
    expect_ok(&pair.step(&["config", "mode"]));
    expect_err(&pair.step(&["config", "mode", "robot"]));
    expect_ok(&pair.step(&["config", "mode", "human"]));
    expect_ok(&pair.step(&["config", "mode", "ai"]));
    expect_ok(&pair.step(&["config", "mode", "ai"]));
    expect_ok(&pair.step(&["config", "mode"]));
    expect_ok(&pair.step(&["config", "mode", "human"]));

    let fresh = Pair::empty(&golden);
    expect_ok(&fresh.step(&["config", "mode"]));
    expect_err(&fresh.step(&["config", "mode", "ai"]));

    let odd = Pair::empty(&golden);
    odd.write(
        ".aidlc/config.json",
        r#"{"harness":"claude","scope":"team","mode":"robot","extra":[1,2]}"#,
    );
    expect_ok(&odd.step(&["config", "mode"]));
    expect_ok(&odd.step(&["config", "mode", "ai"]));
}

#[test]
fn config_gate_mode_matches_golden() {
    let golden = Golden::new(STEM, "config_gate_mode_matches_golden");
    let gates: Vec<String> = graph()["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["gate"].as_str().unwrap().to_owned())
        .collect();
    let (first, second) = (gates[0].as_str(), gates[2].as_str());
    let template = onboarded("claude");
    let pair = Pair::from(&golden, &template.0);
    expect_ok(&pair.step(&["config", "gate-mode"]));
    expect_ok(&pair.step(&["config", "gate-mode", first]));
    expect_err(&pair.step(&["config", "gate-mode", "no-such-gate"]));
    expect_err(&pair.step(&["config", "gate-mode", first, "ai"]));
    expect_ok(&pair.step(&["config", "gate-mode", second, "human"]));
    expect_ok(&pair.step(&["config", "gate-mode", first, "human"]));
    expect_ok(&pair.step(&["config", "gate-mode", first, "human"]));
    expect_ok(&pair.step(&["config", "gate-mode"]));
    expect_ok(&pair.step(&["config", "gate-mode", second, "default"]));
    expect_ok(&pair.step(&["config", "gate-mode", second, "default"]));
    expect_ok(&pair.step(&["config", "gate-mode", first, "default"]));
    expect_ok(&pair.step(&["config", "gate-mode"]));

    let fresh = Pair::empty(&golden);
    expect_ok(&fresh.step(&["config", "gate-mode"]));
    expect_err(&fresh.step(&["config", "gate-mode", first, "human"]));
    let broken = Pair::empty(&golden);
    broken.write(".aidlc/config.json", r#"{"harness":"nope","scope":"solo"}"#);
    expect_err(&broken.step(&["config", "gate-mode", first, "human"]));
}

#[test]
fn config_prerequisite_matches_golden() {
    let golden = Golden::new(STEM, "config_prerequisite_matches_golden");
    let graph = graph();
    let stage = graph["stages"][0]["id"].as_str().unwrap().to_owned();
    let phase = graph["phases"][1]["id"].as_str().unwrap().to_owned();
    let template = onboarded("claude");
    let pair = Pair::from(&golden, &template.0);
    expect_ok(&pair.step(&["config", "prerequisite"]));
    expect_err(&pair.step(&["config", "prerequisite", "set"]));
    expect_err(&pair.step(&["config", "prerequisite", "set", "docker"]));
    expect_err(&pair.step(&[
        "config",
        "prerequisite",
        "set",
        "Bad_Name",
        "--check",
        "exit 0",
    ]));
    expect_err(&pair.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
        "--applies-to",
        "nowhere",
    ]));
    let set = pair.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
        "--hint",
        "Start Docker",
        "--applies-to",
        &stage,
        "--applies-to",
        &phase,
        "--applies-to",
        &stage,
    ]);
    assert_eq!(
        set,
        Outcome::Ok(
            json!({"name":"docker","check":"exit 0","hint":"Start Docker","stages":[stage, phase]}),
            0
        )
    );
    expect_ok(&pair.step(&["config", "prerequisite", "set", "db", "--check", "exit 3"]));
    expect_ok(&pair.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
    ]));
    expect_ok(&pair.step(&["config", "prerequisite"]));
    let ok = pair.step(&["config", "prerequisite", "check", "docker"]);
    assert!(matches!(ok, Outcome::Ok(_, 0)), "{ok:?}");
    let failing = pair.step(&["config", "prerequisite", "check"]);
    assert!(matches!(failing, Outcome::Ok(_, 1)), "{failing:?}");
    expect_err(&pair.step(&["config", "prerequisite", "check", "ghost"]));
    expect_err(&pair.step(&["config", "prerequisite", "remove"]));
    expect_err(&pair.step(&["config", "prerequisite", "remove", "ghost"]));
    expect_err(&pair.step(&["config", "prerequisite", "frobnicate"]));
    expect_ok(&pair.step(&["config", "prerequisite", "remove", "db"]));
    expect_ok(&pair.step(&["config", "prerequisite", "remove", "docker"]));
    expect_ok(&pair.step(&["config", "prerequisite"]));

    let fresh = Pair::empty(&golden);
    expect_ok(&fresh.step(&["config", "prerequisite"]));
    expect_err(&fresh.step(&[
        "config",
        "prerequisite",
        "set",
        "docker",
        "--check",
        "exit 0",
    ]));
    expect_err(&fresh.step(&["config", "prerequisite", "check", "docker"]));
    expect_ok(&fresh.step(&["config", "prerequisite", "check"]));
}

#[test]
fn config_agents_matches_golden() {
    let golden = Golden::new(STEM, "config_agents_matches_golden");
    let agent = graph()["agents"][0]["id"].as_str().unwrap().to_owned();
    let other = graph()["agents"][2]["id"].as_str().unwrap().to_owned();
    let template = onboarded("claude");
    let pair = Pair::from(&golden, &template.0);
    expect_ok(&pair.step(&["config", "agents"]));
    expect_ok(&pair.step(&["config", "agents", "set", &agent, "opus"]));
    expect_ok(&pair.step(&["config", "agents", "set", &other, "haiku"]));
    expect_err(&pair.step(&["config", "agents", "set", &agent, "gpt"]));
    expect_err(&pair.step(&["config", "agents", "set", &agent]));
    expect_err(&pair.step(&["config", "agents", "set", "nobody", "opus"]));
    expect_err(&pair.step(&["config", "agents", "set"]));
    expect_err(&pair.step(&["config", "agents", "frobnicate"]));
    expect_ok(&pair.step(&["config", "agents"]));
    expect_ok(&pair.step(&["config", "agents", "reset", &agent]));
    expect_ok(&pair.step(&["config", "agents", "reset", &other]));
    expect_ok(&pair.step(&["config", "agents", "reset", &other]));
    expect_ok(&pair.step(&["config", "agents"]));

    let deepseek = Pair::from(&golden, &onboarded("deepseek").0);
    expect_ok(&deepseek.step(&["config", "agents"]));
    expect_err(&deepseek.step(&["config", "agents", "set", &agent, "opus"]));
    expect_ok(&deepseek.step(&["config", "agents", "reset", &agent]));

    let fresh = Pair::empty(&golden);
    expect_ok(&fresh.step(&["config", "agents"]));
    expect_err(&fresh.step(&["config", "agents", "set", &agent, "opus"]));

    let broken = Pair::empty(&golden);
    for text in [
        "{not json",
        "[]",
        r#"{"harness":"claude","scope":"solo","mode":"robot"}"#,
        r#"{"harness":"claude","scope":"solo","agentModels":{"nobody":"opus"}}"#,
        &format!(r#"{{"harness":"claude","scope":"solo","agentModels":{{"{agent}":7}}}}"#),
        r#"{"harness":"claude","scope":"solo","gateModes":{"nope":"human"}}"#,
        r#"{"harness":"claude","scope":"solo","unitSplitThreshold":0}"#,
        r#"{"harness":"claude","scope":"solo","roles":{"owner":"me"}}"#,
    ] {
        broken.write(".aidlc/config.json", text);
        expect_err(&broken.step(&["config", "agents"]));
    }
    // Valid but not initialized (no initializedAt): listing works, changes are refused.
    broken.write(
        ".aidlc/config.json",
        r#"{"harness":"claude","scope":"solo"}"#,
    );
    expect_ok(&broken.step(&["config", "agents"]));
    expect_err(&broken.step(&["config", "agents", "set", &agent, "opus"]));
}

#[test]
fn config_unknown_section_matches_golden() {
    let golden = Golden::new(STEM, "config_unknown_section_matches_golden");
    let pair = Pair::empty(&golden);
    expect_err(&pair.step(&["config"]));
    expect_err(&pair.step(&["config", "colour"]));
}

#[test]
fn config_refuses_symlinked_state_dir() {
    let real = Dir::new("real");
    let project = Dir::new("link");
    fs::create_dir_all(real.0.join(".aidlc")).unwrap();
    fs::write(
        real.0.join(".aidlc/config.json"),
        r#"{"harness":"claude","scope":"solo","initializedAt":"x"}"#,
    )
    .unwrap();
    if !link_dir(&real.0.join(".aidlc"), &project.0.join(".aidlc")) {
        eprintln!("symlinks unavailable; skipping");
        return;
    }
    let before = fs::read(real.0.join(".aidlc/config.json")).unwrap();
    // The expected text is the reference CLI's message (checked when it was recorded).
    let outcome = run(&project.0, &["config", "scope", "team"]);
    assert_eq!(
        outcome,
        Outcome::Err("Refusing to access symlink .aidlc directory".into())
    );
    assert_eq!(fs::read(real.0.join(".aidlc/config.json")).unwrap(), before);
    assert!(!real.0.join(".aidlc/audit.jsonl").exists());
}
