//! Path redaction for MCP output (FN-015). Pure `rewrite` plus a `Redactor` built once at startup.

use serde_json::{Map, Value};
use std::ffi::OsStr;
use std::path::Path;

const GENERIC: &str = "<redacted-path>";

pub struct Redactor {
    prefixes: Vec<(String, String)>,
    case_insensitive: bool,
}

impl Redactor {
    pub fn new(root: &Path) -> Redactor {
        let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        Self::with_env(
            root,
            std::env::var_os("DTECH_AIDLC_HOME").as_deref(),
            std::env::var_os(home).as_deref(),
        )
    }

    /// Environment values are injected so tests need not mutate the process env.
    pub fn with_env(root: &Path, engine_home: Option<&OsStr>, home: Option<&OsStr>) -> Redactor {
        let mut prefixes = Vec::new();
        let mut add = |p: &Path, repl: &str| {
            for s in variants(p) {
                prefixes.push((s, repl.to_string()));
            }
        };
        add(root, ".");
        if let Ok(c) = root.canonicalize() {
            add(&c, ".");
        }
        if let Some(h) = engine_home.filter(|h| !h.is_empty()) {
            // A relative engine path is resolved against the project root first.
            let h = root.join(h);
            add(&h, "$DTECH_AIDLC_HOME");
            if let Ok(c) = h.canonicalize() {
                add(&c, "$DTECH_AIDLC_HOME");
            }
        }
        if let Some(h) = home.filter(|h| !h.is_empty()) {
            add(Path::new(h), "~");
        }
        Redactor {
            prefixes: prepare(prefixes),
            case_insensitive: cfg!(windows),
        }
    }

    /// Redacts every string value and every object key, at any depth.
    pub fn redact_value(&self, v: &mut Value) {
        match v {
            Value::String(s) => *s = self.rewrite(s),
            Value::Array(a) => a.iter_mut().for_each(|x| self.redact_value(x)),
            Value::Object(m) => {
                let old = std::mem::take(m);
                let mut out = Map::new();
                for (k, mut x) in old {
                    self.redact_value(&mut x);
                    let base = self.rewrite(&k);
                    let mut key = base.clone();
                    let mut n = 2;
                    while out.contains_key(&key) {
                        key = format!("{base}#{n}");
                        n += 1;
                    }
                    out.insert(key, x);
                }
                *m = out;
            }
            _ => {}
        }
    }

    /// ADR-010 step 4: registered prefixes are substituted; if any other absolute path remains
    /// (or the result is empty) the whole message becomes `fallback_code_text`.
    pub fn redact_message(&self, text: &str, fallback_code_text: &str) -> String {
        let (r, unregistered) = rewrite_core(text, &self.prefixes, self.case_insensitive);
        if unregistered || r.trim().is_empty() {
            fallback_code_text.to_string()
        } else {
            r
        }
    }

    fn rewrite(&self, text: &str) -> String {
        rewrite_core(text, &self.prefixes, self.case_insensitive).0
    }
}

/// Only absolute prefixes are usable (relative ones like '.' or 'src' would over-redact),
/// longest first. Done once at construction.
fn prepare(mut prefixes: Vec<(String, String)>) -> Vec<(String, String)> {
    prefixes.retain(|(p, _)| starts_absolute(&p.chars().collect::<Vec<_>>()));
    prefixes.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    prefixes
}

/// Native form plus the macOS `/var` <-> `/private/var` twin; `\\?\` is stripped.
fn variants(p: &Path) -> Vec<String> {
    let s = strip_verbatim(&p.to_string_lossy());
    let mut v = vec![s.clone()];
    if let Some(r) = s.strip_prefix("/private/var/") {
        v.push(format!("/var/{r}"));
    } else if let Some(r) = s.strip_prefix("/var/") {
        v.push(format!("/private/var/{r}"));
    }
    v
}

/// `\\?\C:\x` -> `C:\x`, `\\?\UNC\h\s` -> `\\h\s` (no new crate for `dunce`).
fn strip_verbatim(s: &str) -> String {
    match s.strip_prefix(r"\\?\") {
        Some(r) => match r.strip_prefix(r"UNC\") {
            Some(u) => format!(r"\\{u}"),
            None => r.to_string(),
        },
        None => s.to_string(),
    }
}

fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

fn is_path_char(c: char) -> bool {
    !c.is_whitespace() && !"\"'`(),;[]{}<>".contains(c)
}

fn starts_absolute(t: &[char]) -> bool {
    match t {
        ['/', ..] | ['\\', '\\', ..] => true,
        [d, ':', s, ..] => d.is_ascii_alphabetic() && is_sep(*s),
        _ => false,
    }
}

/// Length of `prefix` matched at the start of `t` on a path boundary, separators interchangeable.
fn match_prefix(t: &[char], prefix: &str, ci: bool) -> Option<usize> {
    let p: Vec<char> = prefix.trim_end_matches(is_sep).chars().collect();
    if p.is_empty() || t.len() < p.len() {
        return None;
    }
    let same =
        |a: char, b: char| (is_sep(a) && is_sep(b)) || a == b || (ci && a.eq_ignore_ascii_case(&b));
    if !p.iter().zip(t).all(|(a, b)| same(*a, *b)) {
        return None;
    }
    match t.get(p.len()) {
        None => Some(p.len()),
        Some(&c) if is_sep(c) || !is_path_char(c) => Some(p.len()),
        _ => None,
    }
}

/// Test helper: `rewrite_core` over raw (unprepared) prefixes.
#[cfg(test)]
fn rewrite(text: &str, prefixes: &[(String, String)], ci: bool) -> String {
    rewrite_core(text, &prepare(prefixes.to_vec()), ci).0
}

/// Pure rewrite over `prepare`d prefixes: longest matching prefix first, then any other
/// absolute-looking token becomes `<redacted-path>`; the flag reports whether that happened.
/// Runs with Windows-form strings on any OS.
// ponytail: an unregistered absolute path containing a space is redacted only up to the space
// in values; messages fall back to generic text wholesale, so nothing leaks there.
fn rewrite_core(text: &str, sorted: &[(String, String)], ci: bool) -> (String, bool) {
    let mut unregistered = false;
    let t: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut forced = false;
    while i < t.len() {
        // `file:///x` keeps its scheme; any other `scheme://host` is a URL, not a path.
        if !forced && i > 0 && t[i - 1] == ':' && t[i..].starts_with(&['/', '/']) {
            if i >= 5
                && t[i - 5..i - 1]
                    .iter()
                    .collect::<String>()
                    .eq_ignore_ascii_case("file")
                && t.get(i + 2) == Some(&'/')
            {
                out.push_str("//");
                i += 2;
                forced = true;
            } else {
                out.push(t[i]);
                i += 1;
            }
            continue;
        }
        let at_start =
            forced || i == 0 || t[i - 1].is_whitespace() || "\"'`:(=[{<,;".contains(t[i - 1]);
        forced = false;
        if !at_start {
            out.push(t[i]);
            i += 1;
            continue;
        }
        // `\\?\C:\x` is matched as `C:\x`; `\\?\UNC\h\s` falls through to the generic rule.
        let verbatim = t[i..].starts_with(&['\\', '\\', '?', '\\'])
            && !t[i + 4..].starts_with(&['U', 'N', 'C', '\\']);
        let skip = if verbatim { 4 } else { 0 };
        let hit = sorted
            .iter()
            .find_map(|(p, r)| match_prefix(&t[i + skip..], p, ci).map(|n| (n, r)));
        if hit.is_none() && !starts_absolute(&t[i..]) {
            out.push(t[i]);
            i += 1;
            continue;
        }
        let mut end = i + skip + hit.map_or(0, |h| h.0);
        while end < t.len() && is_path_char(t[end]) {
            // `a:/b` or `a=/b` joins a second absolute path; stop so it is redacted on its
            // own. A drive letter's colon (`C:/`) sits right after the token start.
            let joins = t[end] == '=' || (t[end] == ':' && end > i + skip + 1);
            if joins && starts_absolute(&t[end + 1..]) {
                break;
            }
            end += 1;
        }
        // Trailing punctuation belongs to the sentence, not the path.
        while end > i && ".:".contains(t[end - 1]) {
            end -= 1;
        }
        if end <= i {
            // Always make progress, even for degenerate input.
            out.push(t[i]);
            i += 1;
            continue;
        }
        match hit {
            Some((n, repl)) => {
                let rest: String = t[(i + skip + n).min(end)..end]
                    .iter()
                    .map(|&c| if c == '\\' { '/' } else { c })
                    .collect();
                let rest = rest.trim_start_matches('/');
                match (repl.as_str(), rest) {
                    (".", "") => out.push('.'),
                    (".", r) => out.push_str(r),
                    (r, "") => out.push_str(r),
                    (r, rest) => out.push_str(&format!("{r}/{rest}")),
                }
            }
            None => {
                unregistered = true;
                out.push_str(GENERIC);
            }
        }
        i = end;
    }
    (out, unregistered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    #[test]
    fn ts008_longest_prefix_wins_and_is_relative() {
        let pre = [p("/a", "$H"), p("/a/b", ".")];
        assert_eq!(rewrite("/a/b/c/d.txt", &pre, false), "c/d.txt");
        assert_eq!(rewrite("/a/b", &pre, false), ".");
        assert_eq!(rewrite("/a/z", &pre, false), "$H/z");
    }

    #[test]
    fn ts008_boundary_is_respected() {
        let pre = [p("/a/b", ".")];
        assert_eq!(rewrite("/a/bc/x", &pre, false), "<redacted-path>");
        assert_eq!(rewrite("see \"/a/b\" now", &pre, false), "see \".\" now");
    }

    #[test]
    fn ts008_second_path_after_colon_or_equals_is_not_swallowed() {
        let pre = [p("/srv/proj", ".")];
        assert_eq!(
            rewrite("PATH=/srv/proj/bin:/usr/secret/bin", &pre, false),
            "PATH=bin:<redacted-path>"
        );
        assert_eq!(
            rewrite("copy /srv/proj/a=/etc/shadow", &pre, false),
            "copy a=<redacted-path>"
        );
        assert_eq!(
            rewrite("/usr/a:/usr/b", &pre, false),
            "<redacted-path>:<redacted-path>"
        );
        // A drive letter's colon is part of the path, not a separator.
        assert_eq!(rewrite("C:/x/y", &pre, false), "<redacted-path>");
        assert_eq!(rewrite(r"C:\x\y", &pre, false), "<redacted-path>");
    }

    #[test]
    fn ts008_windows_forms_on_any_os() {
        let pre = [p(r"C:\Users\me\proj", ".")];
        assert_eq!(
            rewrite(r"C:\Users\me\proj\src\x.rs", &pre, false),
            "src/x.rs"
        );
        assert_eq!(rewrite(r"\\?\C:\Users\me\proj\a", &pre, false), "a");
        assert_eq!(rewrite("c:/users/ME/proj/x", &pre, true), "x");
        assert_eq!(
            rewrite("c:/users/ME/proj/x", &pre, false),
            "<redacted-path>"
        );
        assert_eq!(
            rewrite(r"\\host\share\secret", &pre, false),
            "<redacted-path>"
        );
        assert_eq!(rewrite(r"D:\other\f", &pre, false), "<redacted-path>");
    }

    #[test]
    fn ts008_remaining_absolute_and_non_paths() {
        let pre = [p("/home/me", "~")];
        assert_eq!(
            rewrite("/home/me/.x and /etc/passwd.", &pre, false),
            "~/.x and <redacted-path>."
        );
        assert_eq!(
            rewrite("https://x.io/a src/b a/b", &pre, false),
            "https://x.io/a src/b a/b"
        );
        assert_eq!(
            rewrite("open /etc/x: denied", &pre, false),
            "open <redacted-path>: denied"
        );
    }

    #[test]
    fn rewrite_terminates_on_relative_prefixes() {
        let pre = [p(".", "$H"), p("", "$E"), p("src", "$S")];
        assert_eq!(rewrite(".", &pre, false), ".");
        assert_eq!(
            rewrite("the engine failed.", &pre, false),
            "the engine failed."
        );
        assert_eq!(rewrite("src/x ./y", &pre, false), "src/x ./y");
    }

    #[test]
    fn relative_engine_home_is_resolved_not_registered_raw() {
        let r = Redactor::with_env(Path::new("/srv/proj"), Some(OsStr::new("engine")), None);
        assert_eq!(r.rewrite("the engine failed."), "the engine failed.");
        assert_eq!(r.rewrite("/srv/proj/engine/x"), "$DTECH_AIDLC_HOME/x");
    }

    #[test]
    fn paths_after_backtick_colon_and_file_url() {
        let pre = [p("/Users/me", "~")];
        assert_eq!(
            rewrite("cannot read `/Users/me/x`", &pre, false),
            "cannot read `~/x`"
        );
        assert_eq!(rewrite("path:/etc/x", &pre, false), "path:<redacted-path>");
        assert_eq!(rewrite("file:///Users/me/x", &pre, false), "file://~/x");
        assert_eq!(rewrite("FILE:///Users/me/x", &pre, false), "FILE://~/x");
        assert_eq!(
            rewrite("file:///etc/x", &pre, false),
            "file://<redacted-path>"
        );
        assert_eq!(
            rewrite("see https://x.io/a", &pre, false),
            "see https://x.io/a"
        );
    }

    #[test]
    fn messages_use_generic_text_and_keys_do_not_collide() {
        let r = Redactor::new(Path::new("/srv/proj"));
        assert_eq!(r.redact_message("cannot read /etc/x: no", "f"), "f");
        assert_eq!(r.redact_message("bad /Volumes/My Disk/secret", "f"), "f");
        let h = Redactor::with_env(Path::new("/srv/proj"), None, Some(OsStr::new("/home/me")));
        assert_eq!(
            h.redact_message("read /home/me/x failed", "f"),
            "read ~/x failed"
        );
        assert_eq!(h.redact_message("read /home/me/x and /etc/y", "f"), "f");
        let mut v = json!({"/etc/a": 1, "/etc/b": 2});
        r.redact_value(&mut v);
        assert_eq!(v, json!({"<redacted-path>": 1, "<redacted-path>#2": 2}));
    }

    #[test]
    fn ts008_macos_var_and_private_var_both_covered() {
        let r = Redactor::new(Path::new("/var/folders/ab/T/proj"));
        let mut v = json!({"/var/folders/ab/T/proj/k": ["/private/var/folders/ab/T/proj/state.json", "/var/folders/ab/T/proj"]});
        r.redact_value(&mut v);
        assert_eq!(v, json!({"k": ["state.json", "."]}));
    }

    #[test]
    fn ts008_message_is_redacted_with_fallback() {
        let r = Redactor::new(Path::new("/srv/proj"));
        assert_eq!(
            r.redact_message("cannot read /srv/proj/state.json: denied", "x"),
            "cannot read state.json: denied"
        );
        assert_eq!(
            r.redact_message("  ", "operation_failed"),
            "operation_failed"
        );
    }
}
