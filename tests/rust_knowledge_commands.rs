//! Golden tests for `dtech-aidlc knowledge ...` (and the `knowledge_preflight` fix that
//! lets `orchestrate next` report the reference messages for a missing project).
//!
//! Each scenario builds a fixture, runs every step with
//! `dtech_aidlc::knowledge_commands::run`, then checks exit codes, JSON output, error
//! text, every project file and the audit rows written by the steps against
//! `tests/golden/rust_knowledge_commands/<test>.json`. Only the project root and ISO
//! timestamps are normalised. The golden files were recorded from the reference CLI that
//! the native commands replace; see the `golden` module for re-blessing.

use dtech_aidlc::args::{command_key, parse_args};
use golden::{Golden, Masks, Tree};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const STEM: &str = "rust_knowledge_commands";

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
            "aidlc-kc-{label}-{}-{}-{nanos}",
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
        .current_dir(std::env::temp_dir())
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
        "knowledge" => dtech_aidlc::knowledge_commands::run(&key, root, &args, &parsed.flags),
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

/// Run one `knowledge` command on `root`.
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
            if kind.is_symlink() {
                out.insert(rel, "<symlink>".into());
            } else if kind.is_dir() {
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
    files: Tree,
    audit: Vec<Value>,
}

impl Pair {
    fn new(golden: &Golden, label: &str, setup: impl Fn(&Path)) -> Self {
        let dir = TempDir::new(label);
        setup(&dir.0);
        let masks = Masks::new().path(&dir.0, "<ROOT>");
        let audit = audit_rows(&dir.0);
        golden.check(json!({"fixture": label}));
        Pair {
            dir,
            golden: golden.clone(),
            masks,
            files: Tree::new(),
            audit,
        }
    }

    /// Run one step and check its output, the project files and the new audit rows.
    fn step(&mut self, argv: &[&str]) -> Outcome {
        let outcome = run(&self.dir.0, argv);
        let files: Tree = snapshot(&self.dir.0)
            .into_iter()
            .map(|(path, text)| (path, self.masks.file(text.as_bytes())))
            .collect();
        let audit: Vec<Value> = audit_rows(&self.dir.0)
            .iter()
            .map(|row| self.masks.value(row))
            .collect();
        self.golden.check(json!({
            "argv": argv,
            "outcome": shown(&outcome, &self.dir.0, &self.masks),
            "files": golden::tree_delta(&self.files, &files),
            "audit": golden::audit_delta(&self.audit, &audit),
        }));
        self.files = files;
        self.audit = audit;
        outcome
    }
}

fn onboard(root: &Path) {
    let out = cli(root, &["onboard", "--harness", "claude", "--scope", "solo"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
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
                .arg(link.components().collect::<PathBuf>())
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

const COMPLETE: &str = "---\nid: alpha\ntitle: Alpha record\norigin: archived\nclosed: 2026-01-01\nscope: solo\ntags: [one, two ,, three]\nsupersedes: old-1\n---\n# Alpha\n\n## Outcome\nShipped.\n## Decisions\nD\n## Limitations\nL\n## Follow-ups\nF\n## Links\nNone\n";

fn write_records(root: &Path) {
    let dir = root.join(".aidlc/knowledge");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("alpha.md"), COMPLETE).unwrap();
    // Missing headings, quoted values, CRLF line endings, H1 title fallback.
    fs::write(
        dir.join("beta.md"),
        "---\r\nid: \"beta\"\r\ntitle: ''\r\n  indented: ignored\r\nweird key: ignored\r\nphases-run: analyze, develop\r\n---\r\n# Beta heading  \r\n## Outcome\r\nPartial\r\n",
    )
    .unwrap();
    // No frontmatter: a legacy record.
    fs::write(dir.join("gamma.md"), "# Gamma\n\nPlain notes.\n").unwrap();
    // Frontmatter closed at end of file, empty values fall back.
    fs::write(dir.join("delta.md"), "---\nid:\norigin:\n---").unwrap();
    // Empty frontmatter block requires a newline before the closing fence.
    fs::write(dir.join("epsilon.md"), "---\n---\n# E\n").unwrap();
    // Intent-origin record (not checked for headings) with invalid UTF-8 bytes.
    fs::write(
        dir.join("zeta.md"),
        b"---\nid: zeta\norigin: intent\ntitle: Z \xff\n---\n# Zeta\n".as_slice(),
    )
    .unwrap();
    // Skipped and invalid entries.
    fs::write(dir.join("_index.md"), "old index").unwrap();
    fs::write(dir.join(".hidden.md"), "hidden").unwrap();
    fs::write(dir.join("notes.txt"), "not markdown").unwrap();
    fs::write(dir.join("-bad.md"), "---\nid: bad\n---\n").unwrap();
    fs::create_dir_all(dir.join("folder.md")).unwrap();
}

#[test]
fn onboarded_project_commands_match_golden() {
    let golden = Golden::new(STEM, "onboarded_project_commands_match_golden");
    let mut pair = Pair::new(&golden, "onboarded", onboard);
    for argv in [
        vec!["knowledge", "sources"],
        vec!["knowledge", "sources", "list"],
        vec!["knowledge", "list"],
        vec!["knowledge", "check"],
        vec!["knowledge", "show", "missing"],
        vec!["knowledge", "show"],
        vec!["knowledge", "init"],
        vec!["knowledge", "index"],
        vec!["knowledge", "list"],
        vec!["knowledge", "bogus"],
        vec!["knowledge", "sources", "bogus"],
    ] {
        pair.step(&argv);
    }
}

#[test]
fn records_list_show_check_and_index_match_golden() {
    let golden = Golden::new(STEM, "records_list_show_check_and_index_match_golden");
    let mut pair = Pair::new(&golden, "records", |root| {
        onboard(root);
        write_records(root);
    });
    let listed = pair.step(&["knowledge", "list"]);
    assert_eq!(listed.stdout["invalid"].as_array().unwrap().len(), 2);
    for argv in [
        vec!["knowledge", "show", "alpha"],
        vec!["knowledge", "show", "beta"],
        vec!["knowledge", "show", "gamma"],
        vec!["knowledge", "show", "delta"],
        vec!["knowledge", "show", "epsilon"],
        vec!["knowledge", "show", "zeta"],
        vec!["knowledge", "show", "bad"],
        vec!["knowledge", "check"],
        vec!["knowledge", "index"],
    ] {
        pair.step(&argv);
    }
    let checked = pair.step(&["knowledge", "check"]);
    assert_eq!(checked.code, 0);
    assert_eq!(checked.stdout["ok"], json!(false));
}

#[test]
fn source_management_and_errors_match_golden() {
    let golden = Golden::new(STEM, "source_management_and_errors_match_golden");
    let mut pair = Pair::new(&golden, "sources", |root| {
        onboard(root);
        fs::create_dir_all(root.join("docs/adr")).unwrap();
        fs::write(root.join("docs/README.md"), "# Docs\n").unwrap();
    });
    for argv in [
        vec!["knowledge", "sources", "remove", "project"],
        vec![
            "knowledge",
            "sources",
            "add",
            "readme",
            "docs/README.md",
            "--kind",
            "readme",
            "--description",
            "Project readme",
        ],
        vec![
            "knowledge",
            "sources",
            "add",
            "adr",
            "docs/adr",
            "--kind",
            "adr",
        ],
        vec!["knowledge", "sources", "add", "later", "docs/missing.md"],
        vec!["knowledge", "sources"],
        vec!["knowledge", "sources", "add", "readme", "docs/README.md"],
        vec!["knowledge", "sources", "add", "up", "../outside"],
        vec!["knowledge", "sources", "add", "state", ".aidlc/state.json"],
        vec!["knowledge", "sources", "add", "git", ".git/config"],
        vec!["knowledge", "sources", "add", "drive", "C:/x"],
        vec!["knowledge", "sources", "add", "back", "docs\\README.md"],
        vec![
            "knowledge",
            "sources",
            "add",
            "kind",
            "docs",
            "--kind",
            "nope",
        ],
        vec!["knowledge", "sources", "add", "-bad", "docs"],
        vec![
            "knowledge",
            "sources",
            "add",
            "p2",
            "docs",
            "--kind",
            "project",
        ],
        vec!["knowledge", "sources", "add", "only-id"],
        vec!["knowledge", "sources", "remove"],
        vec!["knowledge", "sources", "remove", "unknown"],
        vec!["knowledge", "sources", "remove", "readme"],
        vec!["knowledge", "sources", "list"],
        vec!["knowledge", "init"],
    ] {
        pair.step(&argv);
    }
}

#[test]
fn uninitialized_and_missing_projects_match_golden() {
    let golden = Golden::new(STEM, "uninitialized_and_missing_projects_match_golden");
    // A bare directory: init creates PROJECT.md from the scaffold and .aidlc/knowledge.
    let mut bare = Pair::new(&golden, "bare", |_| {});
    for argv in [
        vec!["knowledge", "sources", "remove", "project"],
        vec!["knowledge", "sources"],
        vec!["knowledge", "list"],
        vec!["knowledge", "init"],
        vec!["knowledge", "init"],
        vec!["knowledge", "sources"],
    ] {
        bare.step(&argv);
    }
    // A lowercase project.md is found and kept.
    let mut lower = Pair::new(&golden, "lower", |root| {
        fs::write(root.join("project.md"), "# lower\n").unwrap();
    });
    lower.step(&["knowledge", "sources"]);
    lower.step(&["knowledge", "init"]);
    // A manifest naming a project file in a missing folder.
    let mut nested = Pair::new(&golden, "nested", |root| {
        fs::write(
            root.join("aidlc.knowledge.json"),
            r#"{"schemaVersion":1,"sources":[{"id":"project","path":"docs/PROJECT.md","kind":"project"}]}"#,
        )
        .unwrap();
    });
    nested.step(&["knowledge", "init"]);
    // No project source declared.
    let mut none = Pair::new(&golden, "noproject", |root| {
        fs::write(
            root.join("aidlc.knowledge.json"),
            r#"{"schemaVersion":1,"sources":[]}"#,
        )
        .unwrap();
    });
    none.step(&["knowledge", "init"]);
    none.step(&["knowledge", "sources"]);
    // A project root that does not exist: read-only commands behave like the reference.
    let parent = TempDir::new("missing");
    let root = parent.0.join("absent");
    let masks = Masks::new().path(&root, "<ROOT>");
    for argv in [
        vec!["knowledge", "sources"],
        vec!["knowledge", "list"],
        vec!["knowledge", "check"],
        vec!["knowledge", "show", "x"],
    ] {
        let outcome = run(&root, &argv);
        golden.check(json!({"argv": argv, "missingProject": shown(&outcome, &root, &masks)}));
        assert!(!root.exists(), "{argv:?} created the project");
    }
}

#[test]
fn invalid_manifests_and_unsafe_state_match_golden() {
    let golden = Golden::new(STEM, "invalid_manifests_and_unsafe_state_match_golden");
    for (label, manifest) in [
        ("malformed", "{not json"),
        ("version", r#"{"schemaVersion":2,"sources":[]}"#),
        ("extra", r#"{"schemaVersion":1,"sources":[],"x":1}"#),
        (
            "dup",
            r#"{"schemaVersion":1,"sources":[{"id":"a","path":"a","kind":"docs"},{"id":"a","path":"b","kind":"docs"}]}"#,
        ),
        (
            "numeric-id",
            r#"{"schemaVersion":1,"sources":[{"id":5,"path":"a","kind":"docs"}]}"#,
        ),
        (
            "desc",
            r#"{"schemaVersion":1,"sources":[{"id":"a","path":"a","kind":"docs","description":"x\ny"}]}"#,
        ),
    ] {
        let mut pair = Pair::new(&golden, label, |root| {
            fs::write(root.join("aidlc.knowledge.json"), manifest).unwrap();
        });
        pair.step(&["knowledge", "sources"]);
        pair.step(&["knowledge", "init"]);
        pair.step(&["knowledge", "sources", "add", "x", "docs"]);
        pair.step(&["knowledge", "sources", "remove", "a"]);
    }
    // `.aidlc` that is a file is refused before anything is read or written.
    let mut file_state = Pair::new(&golden, "aidlc-file", |root| {
        fs::write(root.join(".aidlc"), "not a directory").unwrap();
    });
    for argv in [
        vec!["knowledge", "init"],
        vec!["knowledge", "list"],
        vec!["knowledge", "index"],
        vec!["knowledge", "sources", "add", "x", "docs"],
    ] {
        let out = file_state.step(&argv);
        assert_eq!(out.code, 1);
    }
}

#[test]
fn symlinked_state_is_refused() {
    let golden = Golden::new(STEM, "symlinked_state_is_refused");
    let outside = TempDir::new("outside");
    let probe = TempDir::new("probe");
    if !link_dir(&outside.0, &probe.0.join(".aidlc")) {
        eprintln!("symlinks unavailable; skipping");
        golden.skip();
        return;
    }
    let target = outside.0.clone();
    let mut pair = Pair::new(&golden, "symlink", move |root| {
        assert!(link_dir(&target, &root.join(".aidlc")));
    });
    pair.step(&["knowledge", "init"]);
    pair.step(&["knowledge", "index"]);
    assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
}

/// The reviewed defect: `knowledge_preflight` on a project that does not exist used to
/// fail with the raw OS error. It now behaves like Node (no error, default manifest).
#[test]
fn knowledge_preflight_accepts_a_missing_project() {
    let parent = TempDir::new("preflight");
    let missing = parent.0.join("absent");
    let (exists, manifest) = dtech_aidlc::knowledge::knowledge_preflight(&missing).unwrap();
    assert!(!exists);
    assert_eq!(
        manifest,
        json!({"schemaVersion":1,"sources":[{"id":"project","path":"PROJECT.md","kind":"project"}]})
    );
    assert_eq!(
        dtech_aidlc::knowledge::resolve_project_root(&missing).unwrap(),
        dtech_aidlc::knowledge::ProjectRoot::Missing(missing.clone())
    );
    assert!(!missing.exists());
    let file = parent.0.join("file");
    fs::write(&file, "x").unwrap();
    assert_eq!(
        dtech_aidlc::knowledge::knowledge_preflight(&file).unwrap_err(),
        "Project root must be a directory"
    );
}

/// With the preflight fixed, the binary's `orchestrate next` reports the reference
/// messages for a project directory that does not exist.
#[test]
fn orchestrate_next_on_missing_project_matches_golden() {
    let golden = Golden::new(STEM, "orchestrate_next_on_missing_project_matches_golden");
    let parent = TempDir::new("orch-missing");
    for extra in [vec!["--intent", "x"], vec![]] {
        let root = parent.0.join("absent");
        let masks = Masks::new().path(&root, "<ROOT>");
        let mut argv = vec!["orchestrate", "next"];
        argv.extend(extra.iter().copied());
        let out = cli(&root, &argv);
        golden.check(masks.output(&json!({
            "argv": argv,
            "code": out.code,
            "stderr": normalize_text(&out.stderr, &root),
        })));
        assert!(!out.stderr.is_empty());
    }
}
