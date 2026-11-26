//! Approver identity resolution, ported from Node `core/tools/workflow.mjs`
//! `resolveApprover(root, flagValue)`.
//!
//! Precedence: explicit `--approved-by` flag (`flag`), `git config --get user.name`
//! run in the project root (`git`), the OS username (`os`), then `unknown` (`none`).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Identity recorded for a gate decision or depth choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approver {
    pub by: String,
    /// One of `flag`, `git`, `os`, `none`.
    pub by_source: &'static str,
}

/// Error text identical to Node's `resolveApprover`.
pub const APPROVED_BY_ERROR: &str =
    "--approved-by must be 1 to 200 characters without control characters";

const GIT_TIMEOUT: Duration = Duration::from_millis(5000);

/// Resolve the approver identity with Node-compatible precedence and validation.
pub fn resolve_approver(root: &Path, flag: Option<&str>) -> Result<Approver, String> {
    resolve_approver_with(flag, || git_user_name(root), os_username)
}

/// Injectable core of [`resolve_approver`]. `git` returns the raw stdout of a
/// successful (`exit 0`) `git config --get user.name`, or `None`; `os` returns the
/// OS username, or `None`. Neither lookup runs when an explicit flag is given.
pub fn resolve_approver_with<G, O>(flag: Option<&str>, git: G, os: O) -> Result<Approver, String>
where
    G: FnOnce() -> Option<String>,
    O: FnOnce() -> Option<String>,
{
    // Node: `flagValue !== undefined && flagValue !== null && String(flagValue).length`
    if let Some(raw) = flag.filter(|value| !value.is_empty()) {
        let by = js_trim(raw);
        let length = by.encode_utf16().count();
        if !(1..=200).contains(&length) || by.chars().any(|c| (c as u32) <= 0x1f) {
            return Err(APPROVED_BY_ERROR.into());
        }
        return Ok(Approver {
            by: by.to_owned(),
            by_source: "flag",
        });
    }
    if let Some(stdout) = git() {
        let name = js_trim(&stdout);
        if !name.is_empty() {
            return Ok(Approver {
                by: name.to_owned(),
                by_source: "git",
            });
        }
    }
    if let Some(name) = os().filter(|name| !name.is_empty()) {
        return Ok(Approver {
            by: name,
            by_source: "os",
        });
    }
    Ok(Approver {
        by: "unknown".into(),
        by_source: "none",
    })
}

/// JavaScript `String.prototype.trim()`: strips ECMAScript WhiteSpace and
/// LineTerminator code points. Unlike Rust's `str::trim`, this strips U+FEFF and
/// keeps U+0085.
pub fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_whitespace)
}

fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Run `git config --get user.name` in `root` with a 5 second timeout and no console
/// window. Returns raw stdout only when git exits with status 0; spawn failure,
/// timeout, or non-zero exit yield `None` (Node falls through in all three cases).
pub fn git_user_name(root: &Path) -> Option<String> {
    let mut command = Command::new("git");
    command
        .args(["config", "--get", "user.name"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + GIT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    match status {
        Some(status) if status.success() => Some(String::from_utf8_lossy(&bytes).into_owned()),
        _ => None,
    }
}

/// OS username from the environment (`USERNAME` on Windows, `USER` elsewhere, each
/// falling back to the other). Node uses `os.userInfo().username`; the environment
/// is the portable approximation available without platform FFI.
pub fn os_username() -> Option<String> {
    let order: [&str; 2] = if cfg!(windows) {
        ["USERNAME", "USER"]
    } else {
        ["USER", "USERNAME"]
    };
    order
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> Option<String> {
        panic!("lookup must not run when an explicit flag is given")
    }

    #[test]
    fn flag_wins_and_is_trimmed() {
        let who = resolve_approver_with(Some("  Ada Lovelace \n"), never, never).unwrap();
        assert_eq!(
            who,
            Approver {
                by: "Ada Lovelace".into(),
                by_source: "flag"
            }
        );
    }

    #[test]
    fn flag_validation_matches_node() {
        for bad in ["   ", "\t\n", "a\u{0007}b", "a\u{001f}b", "line\nbreak"] {
            assert_eq!(
                resolve_approver_with(Some(bad), never, never).unwrap_err(),
                APPROVED_BY_ERROR,
                "{bad:?}"
            );
        }
        let long = "x".repeat(201);
        assert_eq!(
            resolve_approver_with(Some(&long), never, never).unwrap_err(),
            APPROVED_BY_ERROR
        );
        let max = "x".repeat(200);
        assert_eq!(
            resolve_approver_with(Some(&max), never, never).unwrap().by,
            max
        );
        // DEL (U+007F) is outside Node's [\u0000-\u001f] class and is accepted.
        assert_eq!(
            resolve_approver_with(Some("a\u{007f}b"), never, never)
                .unwrap()
                .by_source,
            "flag"
        );
    }

    #[test]
    fn flag_length_counts_utf16_units_like_javascript() {
        // U+1F600 is two UTF-16 code units: 100 of them is length 200 in JS.
        let ok = "\u{1F600}".repeat(100);
        assert!(resolve_approver_with(Some(&ok), never, never).is_ok());
        let too_long = format!("{ok}x");
        assert_eq!(
            resolve_approver_with(Some(&too_long), never, never).unwrap_err(),
            APPROVED_BY_ERROR
        );
        // 200 two-byte chars are 400 UTF-8 bytes but still 200 JS units.
        let accented = "\u{00e9}".repeat(200);
        assert!(resolve_approver_with(Some(&accented), never, never).is_ok());
    }

    #[test]
    fn empty_flag_is_treated_as_absent() {
        let who = resolve_approver_with(Some(""), || Some("Git User\n".into()), never).unwrap();
        assert_eq!(who.by, "Git User");
        assert_eq!(who.by_source, "git");
    }

    #[test]
    fn falls_back_git_then_os_then_unknown() {
        let who = resolve_approver_with(None, || Some(" Grace \r\n".into()), never).unwrap();
        assert_eq!((who.by.as_str(), who.by_source), ("Grace", "git"));

        let who =
            resolve_approver_with(None, || Some("  \n".into()), || Some("osuser".into())).unwrap();
        assert_eq!((who.by.as_str(), who.by_source), ("osuser", "os"));

        let who = resolve_approver_with(None, || None, || Some("osuser".into())).unwrap();
        assert_eq!((who.by.as_str(), who.by_source), ("osuser", "os"));

        let who = resolve_approver_with(None, || None, || Some(String::new())).unwrap();
        assert_eq!((who.by.as_str(), who.by_source), ("unknown", "none"));

        let who = resolve_approver_with(None, || None, || None).unwrap();
        assert_eq!((who.by.as_str(), who.by_source), ("unknown", "none"));
    }

    #[test]
    fn js_trim_matches_ecmascript_whitespace() {
        assert_eq!(js_trim("\u{feff} a \u{3000}"), "a");
        assert_eq!(js_trim("\u{0085}a\u{0085}"), "\u{0085}a\u{0085}");
        assert_eq!(js_trim("\u{2028}\u{00a0}a\u{202f}"), "a");
    }

    #[test]
    fn git_lookup_outside_repo_does_not_panic() {
        // Real git invocation: result depends on host config, so only assert it
        // terminates and yields either nothing or raw stdout.
        let dir = std::env::temp_dir();
        let _ = git_user_name(&dir);
        let missing = dir.join("dtech-aidlc-identity-missing-dir-does-not-exist");
        assert_eq!(git_user_name(&missing), None);
    }
}
