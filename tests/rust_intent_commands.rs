//! Golden tests for the native `intent` lifecycle commands
//! (`dtech_aidlc::intent_commands::run`).
//!
//! Every scenario builds a fixture, runs the native handler and checks the JSON output
//! (or the error text and exit code), every file under `.aidlc` and every `audit.jsonl`
//! row (ignoring `at`) against `tests/golden/rust_intent_commands/<test>.json`.
//! Timestamps, the corrupt-pointer quarantine stamp and the temp project path are
//! normalised. The golden files were recorded from the reference CLI that the native
//! commands replace; see the `golden` module for the placeholders and for re-blessing.

use dtech_aidlc::args::{command_key, parse_args};
use dtech_aidlc::intent_commands;
use golden::{Golden, Masks, Tree};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const STEM: &str = "rust_intent_commands";

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dtech-aidlc-intent-{label}-{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
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

/// The outcome of one command: `Ok(json)` or `Err((message, exit code))`.
type Outcome = Result<Value, (String, i32)>;

/// Run the command-line program and read its result.
fn run_cli(project: &Path, argv: &[&str]) -> Outcome {
    let mut program = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"));
    let output = program
        .args(argv)
        .arg("--project")
        .arg(project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run dtech-aidlc");
    let code = output.status.code().unwrap_or(-1);
    if code == 0 {
        Ok(
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "stdout is not JSON ({error}): {}",
                    String::from_utf8_lossy(&output.stdout)
                )
            }),
        )
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .trim_end()
            .strip_prefix("dtech-aidlc: ")
            .unwrap_or(&stderr)
            .to_owned();
        Err((message, code))
    }
}

fn run_rust(project: &Path, argv: &[&str]) -> Outcome {
    let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_owned()).collect();
    let parsed = parse_args(&argv).expect("parse args");
    let key = command_key(&parsed.positional).expect("command key");
    let words = key.split(' ').count();
    let args = parsed.positional[words.min(parsed.positional.len())..].to_vec();
    match intent_commands::run(&key, project, &args, &parsed.flags) {
        Ok((value, 0)) => Ok(value),
        Ok((value, code)) => panic!("unexpected exit code {code} with {value}"),
        Err(message) => Err((message, 1)),
    }
}

/// Run one `intent` command on `project`.
fn run(project: &Path, argv: &[&str]) -> Outcome {
    run_rust(project, argv)
}

fn is_iso_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 24
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            19 => *byte == b'.',
            23 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

fn normalise(value: Value) -> Value {
    match value {
        Value::String(text) if is_iso_timestamp(&text) => json!("<ts>"),
        Value::Array(items) => Value::Array(items.into_iter().map(normalise).collect()),
        // `mtimeMs` only appears in onboarding's source snapshot (fixture setup).
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let v = if k == "mtimeMs" {
                        json!("<mtime>")
                    } else {
                        normalise(v)
                    };
                    (k, v)
                })
                .collect(),
        ),
        other => other,
    }
}

fn normalise_outcome(outcome: Outcome, project: &Path) -> Outcome {
    let shown = project.display().to_string();
    match outcome {
        Ok(value) => Ok(normalise(value)),
        Err((message, code)) => Err((message.replace(&shown, "<project>"), code)),
    }
}

/// Every file under `.aidlc`, keyed by its normalised relative path. JSON files are
/// compared as normalised JSON (key order included); `audit.jsonl` as rows without `at`.
fn snapshot(project: &Path) -> BTreeMap<String, Value> {
    fn walk(base: &Path, dir: &Path, project: &Path, out: &mut BTreeMap<String, Value>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.map(Result::unwrap) {
            let path = entry.path();
            let relative = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                out.insert(format!("{relative}/"), Value::Null);
                walk(base, &path, project, out);
                continue;
            }
            let relative = match relative.find("state.json.corrupt-") {
                Some(index) => format!("{}state.json.corrupt-<stamp>", &relative[..index]),
                None => relative,
            };
            let text = String::from_utf8_lossy(&fs::read(&path).unwrap())
                .replace(&project.display().to_string(), "<project>")
                .replace(
                    &project.display().to_string().replace('\\', "/"),
                    "<project>",
                )
                .replace(
                    &project.display().to_string().replace('\\', "\\\\"),
                    "<project>",
                );
            let value = if relative.ends_with("audit.jsonl") {
                Value::Array(
                    text.lines()
                        .map(|line| {
                            let mut row: Value = serde_json::from_str(line).unwrap();
                            row.as_object_mut().unwrap().shift_remove("at");
                            normalise(row)
                        })
                        .collect(),
                )
            } else if relative.ends_with(".json") {
                serde_json::from_str::<Value>(&text)
                    .map(normalise)
                    .map(|value| json!({"json": value, "raw-trailing-newline": text.ends_with('\n')}))
                    .unwrap_or(Value::String(text))
            } else {
                Value::String(text)
            };
            out.insert(relative, value);
        }
    }
    let mut out = BTreeMap::new();
    let base = project.join(".aidlc");
    walk(&base, &base, project, &mut out);
    out
}

/// The canonical `.aidlc` tree of `project` for the golden files.
fn tree(project: &Path, masks: &Masks) -> Tree {
    snapshot(project)
        .into_iter()
        .map(|(path, value)| (path, masks.value(&value)))
        .collect()
}

fn shown(outcome: &Outcome, project: &Path, masks: &Masks) -> Value {
    match normalise_outcome(outcome.clone(), project) {
        Ok(value) => json!({"ok": masks.output(&value)}),
        Err((message, code)) => json!({"error": masks.output(&json!(message)), "code": code}),
    }
}

/// Build the fixture, run the native handler and check the outcome and the resulting
/// `.aidlc` tree against the next golden entry. Returns the outcome.
fn compare(golden: &Golden, label: &str, build: impl Fn(&Path), argv: &[&str]) -> Outcome {
    let dir = TempDir::new(label);
    build(&dir.0);
    let masks = Masks::new().path(&dir.0, "<project>");
    let outcome = run(&dir.0, argv);
    golden.check(json!({
        "label": label,
        "argv": argv,
        "outcome": shown(&outcome, &dir.0, &masks),
        "aidlc": tree(&dir.0, &masks),
    }));
    outcome
}

/// Apply a sequence of commands to one fixture, checking after each step the outcome and
/// the `.aidlc` files changed since the previous step (the first step against the
/// fixture as built, so fixture-building commands are not part of the expectation).
fn compare_steps(
    golden: &Golden,
    label: &str,
    build: impl Fn(&Path),
    steps: &[&[&str]],
) -> Vec<Outcome> {
    let dir = TempDir::new(label);
    build(&dir.0);
    let masks = Masks::new().path(&dir.0, "<project>");
    let mut before = tree(&dir.0, &masks);
    let mut outcomes = Vec::new();
    for argv in steps {
        let outcome = run(&dir.0, argv);
        let after = tree(&dir.0, &masks);
        golden.check(json!({
            "label": label,
            "argv": argv,
            "outcome": shown(&outcome, &dir.0, &masks),
            "aidlc": golden::tree_delta(&before, &after),
        }));
        before = after;
        outcomes.push(outcome);
    }
    outcomes
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

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const STAMP: &str = "2026-01-02T03:04:05.000Z";

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
    )
    .unwrap();
}

fn write_pointer(project: &Path, id: Option<&str>) {
    write_json(
        &project.join(".aidlc/state.json"),
        &json!({"version": 2, "activeIntentId": id, "updatedAt": STAMP}),
    );
}

fn write_intent(project: &Path, id: &str, state: Value) {
    let dir = project.join(".aidlc/intents").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("intent.md"), format!("# {id}\n\nDo {id}\n")).unwrap();
    write_json(&dir.join("state.json"), &state);
}

fn intent_state(id: &str, stages: Value) -> Value {
    json!({
        "version": 2,
        "intentId": id,
        "intent": format!("Do {id}"),
        "scope": "solo",
        "mode": "human",
        "status": "active",
        "currentStage": "context-and-risk",
        "stages": stages,
        "dependencies": {},
        "createdAt": STAMP,
        "updatedAt": STAMP,
    })
}

fn active_stages() -> Value {
    json!({"problem-framing": {"status": "complete", "detail": null}})
}

const SOLO_STAGES: &[&str] = &[
    "problem-framing",
    "context-and-risk",
    "option-generation",
    "direction-selection",
    "implementation-design",
    "red-tests",
    "build-and-verify",
    "intent-record",
    "knowledge-writeback",
];

fn complete_stages() -> Value {
    Value::Object(
        SOLO_STAGES
            .iter()
            .map(|id| {
                (
                    (*id).to_owned(),
                    json!({"status": "complete", "detail": null}),
                )
            })
            .collect(),
    )
}

/// One active intent `alpha` that the pointer names, with an audit log.
fn one_active(project: &Path) {
    write_intent(project, "alpha", intent_state("alpha", active_stages()));
    write_pointer(project, Some("alpha"));
    fs::write(
        project.join(".aidlc/audit.jsonl"),
        "{\"at\":\"2026-01-02T03:04:05.000Z\",\"event\":\"intent.created\",\"intentId\":\"alpha\"}\n",
    )
    .unwrap();
}

/// `alpha` active (pointer), `beta` paused, `done` complete, `gone` abandoned.
fn many_intents(project: &Path) {
    one_active(project);
    write_intent(project, "beta", intent_state("beta", active_stages()));
    let mut done = intent_state("done", complete_stages());
    done["currentStage"] = Value::Null;
    write_intent(project, "done", done);
    let mut gone = intent_state("gone", active_stages());
    gone["closed"] = json!({"kind": "abandoned", "at": STAMP, "detail": "old"});
    gone["currentStage"] = Value::Null;
    write_intent(project, "gone", gone);
}

fn legacy_v1(project: &Path) {
    write_json(
        &project.join(".aidlc/state.json"),
        &json!({
            "intent": "Legacy export work",
            "scope": "solo",
            "currentStage": "context-and-risk",
            "stages": {"problem-framing": {"status": "complete"}},
            "createdAt": STAMP,
            "project": "legacy",
        }),
    );
}

/// A Develop stage blocked after design: `red-tests` last reported `blocked`.
fn blocked_develop(project: &Path, status: &str, reset_at: Option<&str>) {
    let mut stages = serde_json::Map::new();
    for id in &SOLO_STAGES[..5] {
        stages.insert((*id).into(), json!({"status": "complete", "detail": null}));
    }
    let mut red = json!({
        "status": status,
        "detail": "split needs revision",
        "reports": [
            {"result": "complete", "at": "2026-01-02T03:00:00.000Z"},
            {"result": "blocked", "at": "2026-01-02T03:04:05.000Z", "detail": "split needs revision"},
        ],
        "units": {"U-01": {"status": "complete"}, "U-02": {"status": "blocked", "detail": "x"}},
    });
    if let Some(reset_at) = reset_at {
        red["reworkResetAt"] = json!(reset_at);
    }
    stages.insert("red-tests".into(), red);
    stages.insert(
        "build-and-verify".into(),
        json!({"status": "in_progress", "detail": null, "units": {"U-01": {"status": "complete"}}}),
    );
    let mut state = intent_state("alpha", Value::Object(stages));
    state["currentStage"] = json!("red-tests");
    state["units"] =
        json!([{"id": "U-01", "status": "complete"}, {"id": "U-02", "status": "in_progress"}]);
    state["currentUnit"] = json!("U-02");
    write_intent(project, "alpha", state);
    write_pointer(project, Some("alpha"));
}

fn assert_err(outcome: &Outcome, expected: &str) {
    match outcome {
        Err((message, code)) => {
            assert_eq!(message, expected);
            assert_eq!(*code, 1);
        }
        Ok(value) => panic!("expected error '{expected}', got {value}"),
    }
}

// ---------------------------------------------------------------------------
// intent depth
// ---------------------------------------------------------------------------

#[test]
fn depth_changes_active_intent_and_audits() {
    let golden = Golden::new(STEM, "depth_changes_active_intent_and_audits");
    let out = compare(
        &golden,
        "depth-ok",
        one_active,
        &[
            "intent",
            "depth",
            "standard",
            "--approved-by",
            " Owner ",
            "--detail",
            "pilot",
        ],
    );
    assert_eq!(
        out.unwrap(),
        json!({"intentId": "alpha", "from": "full", "to": "standard"})
    );
    compare_steps(
        &golden,
        "depth-twice",
        one_active,
        &[
            &[
                "intent",
                "depth",
                "fast",
                "--approved-by",
                "Owner",
                "--detail",
                "a",
            ],
            &[
                "intent",
                "depth",
                "full",
                "--approved-by",
                "Owner",
                "--detail",
                "b",
            ],
        ],
    );
}

#[test]
fn depth_migrates_legacy_state_first() {
    let golden = Golden::new(STEM, "depth_migrates_legacy_state_first");
    let out = compare(
        &golden,
        "depth-legacy",
        legacy_v1,
        &[
            "intent",
            "depth",
            "fast",
            "--approved-by",
            "Owner",
            "--detail",
            "x",
        ],
    );
    assert_eq!(out.unwrap()["intentId"], json!("legacy-export-work"));
}

#[test]
fn depth_error_paths_match_golden() {
    let golden = Golden::new(STEM, "depth_error_paths_match_golden");
    assert_err(
        &compare(&golden, "depth-usage", one_active, &["intent", "depth"]),
        "Usage: intent depth NAME --approved-by NAME --detail TEXT",
    );
    assert_err(
        &compare(
            &golden,
            "depth-unknown",
            one_active,
            &[
                "intent",
                "depth",
                "turbo",
                "--approved-by",
                "O",
                "--detail",
                "x",
            ],
        ),
        "Unknown depth 'turbo'. Known depths: full, standard, fast",
    );
    assert_err(
        &compare(
            &golden,
            "depth-none",
            |_| {},
            &[
                "intent",
                "depth",
                "fast",
                "--approved-by",
                "O",
                "--detail",
                "x",
            ],
        ),
        "No active intent",
    );
    assert_err(
        &compare(
            &golden,
            "depth-paused",
            |p| {
                one_active(p);
                write_pointer(p, None);
            },
            &[
                "intent",
                "depth",
                "fast",
                "--approved-by",
                "O",
                "--detail",
                "x",
            ],
        ),
        "No active intent",
    );
    assert_err(
        &compare(
            &golden,
            "depth-no-approver",
            one_active,
            &["intent", "depth", "fast", "--detail", "x"],
        ),
        "intent depth requires --approved-by NAME",
    );
    assert_err(
        &compare(
            &golden,
            "depth-no-detail",
            one_active,
            &["intent", "depth", "fast", "--approved-by", "O"],
        ),
        "intent depth requires --detail TEXT",
    );
    let corrupt = compare(
        &golden,
        "depth-corrupt",
        |p| {
            one_active(p);
            fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
        },
        &[
            "intent",
            "depth",
            "fast",
            "--approved-by",
            "O",
            "--detail",
            "x",
        ],
    );
    assert_err(
        &corrupt,
        ".aidlc/state.json cannot be parsed. No changes were made. Intents on disk: alpha. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
    );
    assert_err(
        &compare(&golden,
            "depth-missing-state",
            |p| write_pointer(p, Some("ghost")),
            &["intent", "depth", "fast", "--approved-by", "O", "--detail", "x"],
        ),
        "Active intent 'ghost' has no state file at .aidlc/intents/ghost/state.json. No changes were made. Run 'dtech-aidlc intent list', then 'dtech-aidlc intent resume <id>' or 'dtech-aidlc intent pause'.",
    );
}

/// The reference CLI persisted the new depth before rejecting an invalid `--approved-by`;
/// the native handler validates first and leaves state unchanged. Only the error text
/// (asserted below) is shared.
#[test]
fn depth_invalid_approver_fails_without_writing() {
    let dir = TempDir::new("depth-bad-approver");
    one_active(&dir.0);
    let before = snapshot(&dir.0);
    let argv = [
        "intent",
        "depth",
        "fast",
        "--approved-by",
        "bad\u{1}name",
        "--detail",
        "x",
    ];
    let rust = run_rust(&dir.0, &argv);
    assert_err(
        &rust,
        "--approved-by must be 1 to 200 characters without control characters",
    );
    assert_eq!(snapshot(&dir.0), before);
}

// ---------------------------------------------------------------------------
// intent rework
// ---------------------------------------------------------------------------

#[test]
fn rework_returns_blocked_develop_stage_to_design() {
    let golden = Golden::new(STEM, "rework_returns_blocked_develop_stage_to_design");
    let out = compare(
        &golden,
        "rework-blocked-status",
        |p| blocked_develop(p, "blocked", None),
        &[
            "intent",
            "rework",
            "--detail",
            "  Owner authorized splitting WU-06 ",
        ],
    );
    assert_eq!(
        out.unwrap(),
        json!({"reworkRequested": true, "intentId": "alpha", "fromStage": "red-tests", "nextStage": "implementation-design"})
    );
    // Status in_progress, but the last report (after no reset) was blocked.
    compare(
        &golden,
        "rework-blocked-report",
        |p| blocked_develop(p, "in_progress", None),
        &["intent", "rework", "--detail", "again"],
    )
    .unwrap();
    // A reset older than the blocked report still counts the report.
    compare(
        &golden,
        "rework-report-after-reset",
        |p| blocked_develop(p, "in_progress", Some("2026-01-02T03:01:00.000Z")),
        &["intent", "rework", "--detail", "again"],
    )
    .unwrap();
    // Twice in a row: the second request finds no blocked stage.
    let steps = compare_steps(
        &golden,
        "rework-twice",
        |p| blocked_develop(p, "blocked", None),
        &[
            &["intent", "rework", "--detail", "first"],
            &["intent", "rework", "--detail", "second"],
        ],
    );
    assert_err(
        &steps[1],
        "Design rework is only available after a blocked Develop stage",
    );
}

#[test]
fn rework_error_paths_match_golden() {
    let golden = Golden::new(STEM, "rework_error_paths_match_golden");
    assert_err(
        &compare(
            &golden,
            "rework-usage",
            |p| blocked_develop(p, "blocked", None),
            &["intent", "rework"],
        ),
        "Usage: intent rework [--unit ID]... --detail \"<reason>\"",
    );
    assert_err(
        &compare(
            &golden,
            "rework-blank",
            |p| blocked_develop(p, "blocked", None),
            &["intent", "rework", "--detail", "   "],
        ),
        "intent rework requires --detail \"<reason>\"",
    );
    assert_err(
        &compare(
            &golden,
            "rework-none",
            |_| {},
            &["intent", "rework", "--detail", "x"],
        ),
        "No active workflow state",
    );
    assert_err(
        &compare(
            &golden,
            "rework-not-blocked",
            one_active,
            &["intent", "rework", "--detail", "x"],
        ),
        "Design rework is only available after a blocked Develop stage",
    );
    assert_err(
        &compare(
            &golden,
            "rework-reset-after-report",
            |p| {
                blocked_develop(p, "in_progress", Some("2026-01-02T04:00:00.000Z"));
                // Develop runs as one implicit unit (no work-unit ledger). Without this
                // file, a work-units.json that cannot be loaded makes rework available
                // (tested in rust_unit_scheduling.rs).
                write_json(
                    &p.join(".aidlc/intents/alpha/develop/implementation-design/work-units.json"),
                    &json!({"status": "not-applicable", "reason": "one implicit unit"}),
                );
            },
            &["intent", "rework", "--detail", "x"],
        ),
        "Design rework is only available after a blocked Develop stage",
    );
    assert_err(
        &compare(
            &golden,
            "rework-complete",
            |p| {
                write_intent(p, "done", intent_state("done", complete_stages()));
                write_pointer(p, Some("done"));
            },
            &["intent", "rework", "--detail", "x"],
        ),
        "Design rework requires an active intent",
    );
}

// ---------------------------------------------------------------------------
// intent assign
// ---------------------------------------------------------------------------

#[test]
fn assign_matches_golden() {
    let golden = Golden::new(STEM, "assign_matches_golden");
    let out = compare(
        &golden,
        "assign-ok",
        one_active,
        &["intent", "assign", "  Dana  "],
    );
    assert_eq!(
        out.unwrap(),
        json!({"intentId": "alpha", "assignee": "Dana"})
    );
    compare(
        &golden,
        "assign-legacy",
        legacy_v1,
        &["intent", "assign", "Dana"],
    )
    .unwrap();
    assert_err(
        &compare(&golden, "assign-usage", one_active, &["intent", "assign"]),
        "Usage: intent assign NAME",
    );
    assert_err(
        &compare(
            &golden,
            "assign-blank",
            one_active,
            &["intent", "assign", "   "],
        ),
        "intent assign requires a name",
    );
    assert_err(
        &compare(
            &golden,
            "assign-none",
            |_| {},
            &["intent", "assign", "Dana"],
        ),
        "No active intent",
    );
}

// ---------------------------------------------------------------------------
// intent pause
// ---------------------------------------------------------------------------

#[test]
fn pause_matches_golden() {
    let golden = Golden::new(STEM, "pause_matches_golden");
    let out = compare(
        &golden,
        "pause-ok",
        one_active,
        &["intent", "pause", "--detail", "lunch"],
    );
    assert_eq!(
        out.unwrap(),
        json!({"paused": true, "intentId": "alpha", "resumeWith": "dtech-aidlc intent resume alpha"})
    );
    compare(&golden, "pause-no-detail", one_active, &["intent", "pause"]).unwrap();
    // A complete intent named by the pointer can still be paused (Node keeps it loaded).
    compare(
        &golden,
        "pause-complete",
        |p| {
            write_intent(p, "done", intent_state("done", complete_stages()));
            write_pointer(p, Some("done"));
        },
        &["intent", "pause"],
    )
    .unwrap();
    // The pointer names an intent whose state file is missing: pause clears it.
    let ghost = compare(
        &golden,
        "pause-ghost",
        |p| write_pointer(p, Some("ghost")),
        &["intent", "pause"],
    );
    assert_eq!(ghost.unwrap()["intentId"], json!("ghost"));
    let legacy = compare(&golden, "pause-legacy", legacy_v1, &["intent", "pause"]);
    assert_eq!(legacy.unwrap()["intentId"], json!("legacy-export-work"));
}

#[test]
fn pause_error_paths_match_golden() {
    let golden = Golden::new(STEM, "pause_error_paths_match_golden");
    for (label, build) in [
        ("pause-none", (|_: &Path| {}) as fn(&Path)),
        ("pause-paused", |p: &Path| {
            one_active(p);
            write_pointer(p, None);
        }),
        ("pause-abandoned", |p: &Path| {
            many_intents(p);
            write_pointer(p, Some("gone"));
        }),
    ] {
        assert_err(
            &compare(&golden, label, build, &["intent", "pause"]),
            "No unfinished active intent to pause",
        );
    }
    let corrupt = compare(
        &golden,
        "pause-corrupt",
        |p| {
            one_active(p);
            fs::write(p.join(".aidlc/state.json"), "[1]").unwrap();
        },
        &["intent", "pause"],
    );
    assert_err(
        &corrupt,
        ".aidlc/state.json is not a recognised state file. No changes were made. Intents on disk: alpha. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
    );
    let invalid = compare(
        &golden,
        "pause-invalid-intent",
        |p| {
            let mut state = intent_state("alpha", active_stages());
            state["scope"] = json!("galaxy");
            write_intent(p, "alpha", state);
            write_pointer(p, Some("alpha"));
        },
        &["intent", "pause"],
    );
    assert_err(
        &invalid,
        ".aidlc/intents/alpha/state.json is invalid: scope is not a key of graph.scopes. No changes were made.",
    );
}

// ---------------------------------------------------------------------------
// intent resume
// ---------------------------------------------------------------------------

#[test]
fn resume_matches_golden() {
    let golden = Golden::new(STEM, "resume_matches_golden");
    let steps = compare_steps(
        &golden,
        "resume-cycle",
        many_intents,
        &[
            &["intent", "pause", "--detail", "switch"],
            &["intent", "resume", "beta"],
            &["intent", "resume", "beta"],
            &["intent", "resume", "alpha"],
            &["intent", "pause"],
            &["intent", "resume", "alpha"],
        ],
    );
    assert_eq!(
        steps[1].clone().unwrap(),
        json!({"resumed": true, "intentId": "beta", "currentStage": "context-and-risk"})
    );
    assert_err(
        &steps[3],
        "Intent 'beta' is active; pause or finish it first",
    );
    // currentStage missing from the state file is omitted from the output.
    let out = compare(
        &golden,
        "resume-no-current",
        |p| {
            let mut state = intent_state("alpha", active_stages());
            state.as_object_mut().unwrap().shift_remove("currentStage");
            write_intent(p, "alpha", state);
        },
        &["intent", "resume", "alpha"],
    );
    assert_eq!(out.unwrap(), json!({"resumed": true, "intentId": "alpha"}));
    // The pointer names a missing intent: resume replaces it.
    compare(
        &golden,
        "resume-over-ghost",
        |p| {
            many_intents(p);
            write_pointer(p, Some("ghost"));
        },
        &["intent", "resume", "beta"],
    )
    .unwrap();
    // The pointer names a complete intent: it is not "active", so resume proceeds.
    compare(
        &golden,
        "resume-over-complete",
        |p| {
            many_intents(p);
            write_pointer(p, Some("done"));
        },
        &["intent", "resume", "beta"],
    )
    .unwrap();
    // Legacy state migrates first, then the migrated intent blocks another resume.
    let legacy = compare_steps(
        &golden,
        "resume-legacy",
        |p| {
            legacy_v1(p);
            write_intent(p, "beta", intent_state("beta", active_stages()));
        },
        &[
            &["intent", "resume", "beta"],
            &["intent", "resume", "legacy-export-work"],
        ],
    );
    assert_err(
        &legacy[0],
        "Intent 'legacy-export-work' is active; pause or finish it first",
    );
}

#[test]
fn resume_quarantines_a_corrupt_pointer() {
    let golden = Golden::new(STEM, "resume_quarantines_a_corrupt_pointer");
    let out = compare(
        &golden,
        "resume-corrupt",
        |p| {
            many_intents(p);
            fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
        },
        &["intent", "resume", "beta"],
    );
    assert_eq!(out.unwrap()["resumed"], json!(true));
    // Quarantine happens even when the target is then rejected.
    let rejected = compare(
        &golden,
        "resume-corrupt-unknown",
        |p| {
            many_intents(p);
            fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
        },
        &["intent", "resume", "nope"],
    );
    assert_err(
        &rejected,
        "Unknown intent 'nope'. Run 'dtech-aidlc intent list'.",
    );
}

#[test]
fn resume_error_paths_match_golden() {
    let golden = Golden::new(STEM, "resume_error_paths_match_golden");
    assert_err(
        &compare(&golden, "resume-usage", many_intents, &["intent", "resume"]),
        "Usage: intent resume <id>",
    );
    for id in ["nope", "../alpha", "Alpha"] {
        assert_err(
            &compare(
                &golden,
                "resume-unknown",
                many_intents,
                &["intent", "resume", id],
            ),
            &format!("Unknown intent '{id}'. Run 'dtech-aidlc intent list'."),
        );
    }
    assert_err(
        &compare(
            &golden,
            "resume-complete",
            many_intents,
            &["intent", "resume", "done"],
        ),
        "Intent 'done' is complete and cannot be resumed",
    );
    assert_err(
        &compare(
            &golden,
            "resume-abandoned",
            many_intents,
            &["intent", "resume", "gone"],
        ),
        "Intent 'gone' was abandoned and cannot be resumed",
    );
    assert_err(
        &compare(
            &golden,
            "resume-other-active",
            many_intents,
            &["intent", "resume", "beta"],
        ),
        "Intent 'alpha' is active; pause or finish it first",
    );
    assert_err(
        &compare(&golden,
            "resume-invalid-active",
            |p| {
                many_intents(p);
                let mut state = intent_state("alpha", active_stages());
                state["stages"] = json!({"problem-framing": {"status": "done"}});
                write_intent(p, "alpha", state);
            },
            &["intent", "resume", "beta"],
        ),
        ".aidlc/intents/alpha/state.json is invalid: stage problem-framing has an invalid status. No changes were made.",
    );
}

// ---------------------------------------------------------------------------
// intent abandon
// ---------------------------------------------------------------------------

#[test]
fn abandon_matches_golden() {
    let golden = Golden::new(STEM, "abandon_matches_golden");
    let out = compare(
        &golden,
        "abandon-active",
        many_intents,
        &["intent", "abandon", "--detail", "dropped"],
    );
    assert_eq!(
        out.unwrap(),
        json!({"abandoned": true, "intentId": "alpha"})
    );
    // Naming the active intent clears the pointer; naming another keeps it.
    compare(
        &golden,
        "abandon-named-active",
        many_intents,
        &["intent", "abandon", "alpha", "--detail", "dropped"],
    )
    .unwrap();
    compare(
        &golden,
        "abandon-named-other",
        many_intents,
        &["intent", "abandon", "beta", "--detail", "dropped"],
    )
    .unwrap();
    let steps = compare_steps(
        &golden,
        "abandon-then",
        many_intents,
        &[
            &["intent", "abandon", "--detail", "dropped"],
            &["intent", "abandon", "--detail", "again"],
            &["intent", "resume", "alpha"],
            &["intent", "resume", "beta"],
        ],
    );
    assert_err(&steps[1], "No active intent; name the intent to abandon");
    assert_err(
        &steps[2],
        "Intent 'alpha' was abandoned and cannot be resumed",
    );
    compare(
        &golden,
        "abandon-legacy",
        legacy_v1,
        &["intent", "abandon", "--detail", "old"],
    )
    .unwrap();
}

#[test]
fn abandon_error_paths_match_golden() {
    let golden = Golden::new(STEM, "abandon_error_paths_match_golden");
    assert_err(
        &compare(
            &golden,
            "abandon-usage",
            many_intents,
            &["intent", "abandon"],
        ),
        "intent abandon requires --detail \"<reason>\"",
    );
    assert_err(
        &compare(
            &golden,
            "abandon-none",
            |_| {},
            &["intent", "abandon", "--detail", "x"],
        ),
        "No active intent; name the intent to abandon",
    );
    assert_err(
        &compare(
            &golden,
            "abandon-ghost",
            |p| write_pointer(p, Some("ghost")),
            &["intent", "abandon", "--detail", "x"],
        ),
        "No active intent; name the intent to abandon",
    );
    assert_err(
        &compare(
            &golden,
            "abandon-unknown",
            many_intents,
            &["intent", "abandon", "nope", "--detail", "x"],
        ),
        "Unknown intent 'nope'. Run 'dtech-aidlc intent list'.",
    );
    assert_err(
        &compare(
            &golden,
            "abandon-complete",
            many_intents,
            &["intent", "abandon", "done", "--detail", "x"],
        ),
        "Intent 'done' is already complete",
    );
    assert_err(
        &compare(
            &golden,
            "abandon-again",
            many_intents,
            &["intent", "abandon", "gone", "--detail", "x"],
        ),
        "Intent 'gone' is already abandoned",
    );
    assert_err(
        &compare(&golden,
            "abandon-corrupt",
            |p| {
                many_intents(p);
                fs::write(p.join(".aidlc/state.json"), "{broken").unwrap();
            },
            &["intent", "abandon", "beta", "--detail", "x"],
        ),
        ".aidlc/state.json cannot be parsed. No changes were made. Intents on disk: alpha, beta, done, gone. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.",
    );
}

// ---------------------------------------------------------------------------
// Unknown actions, onboarded project, safety
// ---------------------------------------------------------------------------

#[test]
fn unknown_action_matches_golden() {
    let golden = Golden::new(STEM, "unknown_action_matches_golden");
    let expected =
        "Use intent list, intent pause, intent resume, intent abandon, intent depth, or intent assign";
    assert_err(
        &compare(&golden, "action-missing", one_active, &["intent"]),
        expected,
    );
    assert_err(
        &compare(
            &golden,
            "action-unknown",
            one_active,
            &["intent", "frobnicate"],
        ),
        expected,
    );
}

#[test]
fn lifecycle_on_onboarded_project_matches_golden() {
    let golden = Golden::new(STEM, "lifecycle_on_onboarded_project_matches_golden");
    let build = |project: &Path| {
        for argv in [
            &["onboard", "--harness", "claude", "--scope", "solo"][..],
            &["orchestrate", "next", "--intent", "Parity intent one"][..],
        ] {
            run_cli(project, argv).unwrap_or_else(|error| panic!("{argv:?}: {error:?}"));
        }
    };
    compare_steps(
        &golden,
        "onboarded",
        build,
        &[
            &["intent", "assign", "Dana"],
            &[
                "intent",
                "depth",
                "standard",
                "--approved-by",
                "Owner",
                "--detail",
                "pilot",
            ],
            &["intent", "pause", "--detail", "later"],
            &["intent", "resume", "parity-intent-one"],
            &["intent", "rework", "--detail", "x"],
            &["intent", "abandon", "--detail", "superseded"],
            &["intent", "pause"],
        ],
    );
}

#[test]
fn symlinked_state_directory_is_refused() {
    let dir = TempDir::new("symlink-aidlc");
    let target = TempDir::new("symlink-target");
    one_active(&target.0);
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(target.0.join(".aidlc"), dir.0.join(".aidlc")).is_ok();
    #[cfg(windows)]
    let linked =
        std::os::windows::fs::symlink_dir(target.0.join(".aidlc"), dir.0.join(".aidlc")).is_ok();
    if !linked {
        return; // Symlink creation needs privileges on Windows.
    }
    let before = snapshot(&target.0);
    assert_err(
        &run_rust(&dir.0, &["intent", "pause"]),
        "Refusing to access symlink .aidlc directory",
    );
    assert_eq!(snapshot(&target.0), before);
}

#[test]
fn symlinked_pointer_is_not_written_through() {
    let dir = TempDir::new("symlink-pointer");
    one_active(&dir.0);
    let outside = TempDir::new("symlink-pointer-target");
    let real = outside.0.join("pointer.json");
    fs::rename(dir.0.join(".aidlc/state.json"), &real).unwrap();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&real, dir.0.join(".aidlc/state.json")).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&real, dir.0.join(".aidlc/state.json")).is_ok();
    if !linked {
        return;
    }
    let pointer = fs::read(&real).unwrap();
    let state = fs::read(dir.0.join(".aidlc/intents/alpha/state.json")).unwrap();
    let outcome = run_rust(&dir.0, &["intent", "assign", "Dana"]);
    assert!(
        matches!(&outcome, Err((message, 1)) if message.starts_with("Refusing to write through symlink")),
        "{outcome:?}"
    );
    assert_eq!(fs::read(&real).unwrap(), pointer);
    assert_eq!(
        fs::read(dir.0.join(".aidlc/intents/alpha/state.json")).unwrap(),
        state,
        "intent state must not be written when the pointer write would be refused"
    );
}
