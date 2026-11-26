//! Golden snapshot helpers shared by the binary-level parity tests.
//!
//! A golden file holds the expected, normalised result of a scenario as JSON under
//! `tests/golden/<suite>/<case>.json`. The initial goldens were recorded from the former
//! JavaScript reference CLI; the tests now compare the Rust CLI against them.
//!
//! Set `DTECH_AIDLC_BLESS=1` to write the current Rust output to the golden files instead
//! of comparing (only after an intentional behaviour change; review the diff).
//!
//! Golden files are JSON so that `core.autocrlf` checkouts cannot change the files
//! themselves: every text value is a JSON string with escaped line endings. The binary,
//! however, embeds the repository files (scaffold, harness, `completion.zsh`) with the
//! line endings of the checkout, so its output does change with the checkout. Goldens
//! hold the output of an LF checkout (the line endings of the git index); on a CRLF
//! checkout (`core.autocrlf=true` on Windows) [`checkout_text`] and [`checkout_value`]
//! turn the output back into that form. See [`crlf_checkout`].
#![allow(dead_code)]

use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What a CRLF checkout changes in the output of the binary: the size and SHA-256
/// digest of each repository file it copies into projects.
struct Checkout {
    /// (working-tree size, LF size) of each CRLF scaffold or harness file.
    sizes: Vec<(u64, u64)>,
    /// (working-tree digest, LF digest) of each CRLF scaffold or harness file.
    digests: Vec<(String, String)>,
    /// Whether any embedded repository file has CRLF line endings.
    crlf: bool,
}

/// JSON keys whose number is a file size.
const SIZE_KEYS: [&str; 2] = ["size", "bytes"];

fn checkout() -> &'static Checkout {
    static CHECKOUT: OnceLock<Checkout> = OnceLock::new();
    CHECKOUT.get_or_init(|| {
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
        fn sha256_hex(bytes: &[u8]) -> String {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(bytes))
        }
        let root = repo();
        let mut copied = Vec::new();
        walk(&root.join("core/data/scaffold"), &mut copied);
        walk(&root.join("harness"), &mut copied);
        // The harness manifests are committed with CRLF line endings, so they read the
        // same in every checkout (and are only parsed, never copied into projects).
        copied.retain(|file| file.file_name().is_none_or(|name| name != "manifest.json"));
        let mut embedded = Vec::new();
        walk(&root.join("core/methodology"), &mut embedded);
        embedded.push(root.join("core/data/phase-graph.json"));
        embedded.push(root.join("rust-cli/completion.zsh"));
        let mut result = Checkout {
            sizes: Vec::new(),
            digests: Vec::new(),
            crlf: false,
        };
        for file in copied {
            let raw = fs::read(&file).unwrap();
            let lf = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
            if raw != lf.as_bytes() {
                result.crlf = true;
                result.sizes.push((raw.len() as u64, lf.len() as u64));
                result
                    .digests
                    .push((sha256_hex(&raw), sha256_hex(lf.as_bytes())));
            }
        }
        result.crlf = result.crlf
            || embedded.iter().any(|file| {
                fs::read(file).is_ok_and(|raw| raw.windows(2).any(|pair| pair == b"\r\n"))
            });
        result
    })
}

/// Whether this checkout has CRLF line endings in the files the binary embeds.
pub fn crlf_checkout() -> bool {
    checkout().crlf
}

/// On a CRLF checkout: `text` with CRLF read as LF, and the working-tree size (after a
/// `"size"` or `"bytes"` key) and digest of each CRLF repository file replaced by its LF
/// size and digest. On an LF checkout `text` is returned unchanged, so the comparison
/// stays byte for byte.
pub fn checkout_text(text: &str) -> String {
    let checkout = checkout();
    if !checkout.crlf {
        return text.to_owned();
    }
    let mut text = text.replace("\r\n", "\n");
    for (raw, lf) in &checkout.digests {
        text = text.replace(raw, lf);
    }
    for (raw, lf) in &checkout.sizes {
        for key in SIZE_KEYS {
            for colon in ["\": ", "\":", "\\\": ", "\\\":"] {
                let from = format!("\"{key}{colon}{raw}");
                let mut out = String::with_capacity(text.len());
                let mut rest = text.as_str();
                while let Some(found) = rest.find(&from) {
                    let end = found + from.len();
                    let whole = !rest[end..].starts_with(|ch: char| ch.is_ascii_digit());
                    out.push_str(&rest[..found]);
                    if whole {
                        out.push_str(&format!("\"{key}{colon}{lf}"));
                    } else {
                        out.push_str(&from);
                    }
                    rest = &rest[end..];
                }
                out.push_str(rest);
                text = out;
            }
        }
    }
    text
}

/// [`checkout_text`] on every string and object key inside `value`, with the number
/// under a `"size"` or `"bytes"` key mapped from working-tree to LF size as well.
pub fn checkout_value(value: &Value) -> Value {
    let checkout = checkout();
    if !checkout.crlf {
        return value.clone();
    }
    match value {
        Value::String(text) => Value::String(checkout_text(text)),
        Value::Array(items) => Value::Array(items.iter().map(checkout_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| {
                    let item = match item.as_u64() {
                        Some(size) if SIZE_KEYS.contains(&key.as_str()) => checkout
                            .sizes
                            .iter()
                            .find(|(raw, _)| *raw == size)
                            .map_or_else(|| item.clone(), |(_, lf)| json!(lf)),
                        _ => checkout_value(item),
                    };
                    (checkout_text(key), item)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Whether `DTECH_AIDLC_BLESS=1` asks the tests to rewrite their golden files.
pub fn blessing() -> bool {
    std::env::var("DTECH_AIDLC_BLESS").is_ok_and(|value| value == "1")
}

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `tests/golden/<suite>/<case>.json`.
pub fn path(suite: &str, case: &str) -> PathBuf {
    repo()
        .join("tests")
        .join("golden")
        .join(suite)
        .join(format!("{case}.json"))
}

pub fn exists(suite: &str, case: &str) -> bool {
    path(suite, case).is_file()
}

/// Reads a golden file, panicking with a hint when it is missing or malformed.
pub fn read(suite: &str, case: &str) -> Value {
    let file = path(suite, case);
    let text = fs::read_to_string(&file).unwrap_or_else(|error| {
        panic!(
            "cannot read golden file {} ({error}); record it with DTECH_AIDLC_BLESS=1",
            file.display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("golden file {} is not JSON: {error}", file.display()))
}

/// Writes a golden file as pretty-printed JSON with a trailing newline.
pub fn write(suite: &str, case: &str, value: &Value) {
    let file = path(suite, case);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    fs::write(&file, text).unwrap();
}

/// Compares `actual` with the golden file, or rewrites the golden file when blessing.
pub fn check(suite: &str, case: &str, actual: &Value) {
    if blessing() {
        write(suite, case, actual);
        return;
    }
    let expected = read(suite, case);
    assert!(
        expected == *actual,
        "result differs from golden file {}\n--- expected ---\n{}\n--- actual ---\n{}\n\
         (rerun with DTECH_AIDLC_BLESS=1 only if the change is intentional)",
        path(suite, case).display(),
        serde_json::to_string_pretty(&expected).unwrap(),
        serde_json::to_string_pretty(actual).unwrap()
    );
}

/// A golden file holding named cases (`{ "<key>": <value>, ... }`).
///
/// [`Cases::expect`] returns the recorded value of a case; when blessing it records the
/// given actual value instead (and returns it). [`Cases::finish`] writes the file when
/// blessing, and otherwise checks that every recorded case was used.
pub struct Cases {
    suite: &'static str,
    case: &'static str,
    expected: Map<String, Value>,
    recorded: Map<String, Value>,
}

impl Cases {
    pub fn open(suite: &'static str, case: &'static str) -> Self {
        let expected = if blessing() {
            Map::new()
        } else {
            match read(suite, case) {
                Value::Object(map) => map,
                other => panic!("golden {suite}/{case} is not an object: {other}"),
            }
        };
        Cases {
            suite,
            case,
            expected,
            recorded: Map::new(),
        }
    }

    pub fn expect(&mut self, key: &str, actual: Value) -> Value {
        if blessing() {
            if let Some(previous) = self.recorded.get(key) {
                assert_eq!(
                    *previous, actual,
                    "case {key} recorded twice with different values"
                );
            }
            self.recorded.insert(key.to_owned(), actual.clone());
            return actual;
        }
        let expected = self.expected.get(key).cloned().unwrap_or_else(|| {
            panic!(
                "golden {} has no case {key:?}; record it with DTECH_AIDLC_BLESS=1",
                path(self.suite, self.case).display()
            )
        });
        self.recorded.insert(key.to_owned(), expected.clone());
        expected
    }

    pub fn finish(self) {
        if blessing() {
            write(self.suite, self.case, &Value::Object(self.recorded));
            return;
        }
        let unused: Vec<&String> = self
            .expected
            .keys()
            .filter(|key| !self.recorded.contains_key(*key))
            .collect();
        assert!(
            unused.is_empty(),
            "golden {} has cases the test did not check: {unused:?}",
            path(self.suite, self.case).display()
        );
    }
}

/// Replaces every `YYYY-MM-DDTHH:MM:SS.mmmZ` timestamp with `<ts>`.
pub fn replace_timestamps(text: &str) -> String {
    let bytes = text.as_bytes();
    let is_ts = |at: usize| {
        bytes.len() >= at + 24
            && (0..24).all(|index| {
                let byte = bytes[at + index];
                match index {
                    4 | 7 => byte == b'-',
                    10 => byte == b'T',
                    13 | 16 => byte == b':',
                    19 => byte == b'.',
                    23 => byte == b'Z',
                    _ => byte.is_ascii_digit(),
                }
            })
    };
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if is_ts(at) {
            out.push_str("<ts>");
            at += 24;
        } else {
            let ch = text[at..].chars().next().unwrap();
            out.push(ch);
            at += ch.len_utf8();
        }
    }
    out
}

/// Replaces the number after every `"mtimeMs":` with `0` (file times differ per run).
pub fn replace_mtimes(text: &str) -> String {
    let marker = "\"mtimeMs\":";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(found) = rest.find(marker) {
        let (head, tail) = rest.split_at(found + marker.len());
        out.push_str(head);
        let spaces = tail.len() - tail.trim_start_matches(' ').len();
        out.push_str(&tail[..spaces]);
        let tail = &tail[spaces..];
        let number = tail
            .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == '-'))
            .unwrap_or(tail.len());
        if number > 0 {
            out.push('0');
        }
        rest = &tail[number..];
    }
    out.push_str(rest);
    out
}

/// Replaces `path` (as shown, JSON-escaped, or with forward slashes) by `label`.
pub fn replace_path(text: &str, path: &Path, label: &str) -> String {
    let shown = path.display().to_string();
    let escaped = shown.replace('\\', "\\\\");
    let forward = shown.replace('\\', "/");
    text.replace(&escaped, label)
        .replace(&shown, label)
        .replace(&forward, label)
}

/// Turns the separators of the path tail after every `label` (up to a quote or white
/// space) into `/`, so Windows and POSIX runs produce the same text.
pub fn forward_after(text: &str, label: &str) -> String {
    let mut text = text.to_owned();
    let mut from = 0;
    while let Some(found) = text[from..].find(label) {
        let start = from + found + label.len();
        let end = text[start..]
            .find(|ch: char| ch == '"' || ch.is_whitespace())
            .map_or(text.len(), |offset| start + offset);
        let fixed = text[start..end].replace("\\\\", "/").replace('\\', "/");
        text.replace_range(start..end, &fixed);
        from = start;
    }
    text
}

/// Reads the output of a CRLF checkout as that of an LF one ([`checkout_text`]),
/// replaces each `(path, label)` in order (put nested paths first), makes the separators
/// after each label `/`, and normalises timestamps and `mtimeMs` values.
pub fn normalise(text: &str, paths: &[(&Path, &str)]) -> String {
    let mut text = checkout_text(text);
    for (path, label) in paths {
        text = replace_path(&text, path, label);
    }
    for (_, label) in paths {
        text = forward_after(&text, label);
    }
    replace_mtimes(&replace_timestamps(&text))
}

/// Normalises a JSON value through its compact serialisation (see [`normalise`]).
pub fn normalise_json(value: &Value, paths: &[(&Path, &str)]) -> Value {
    let text = serde_json::to_string(&checkout_value(value)).unwrap();
    serde_json::from_str(&normalise(&text, paths)).unwrap()
}
