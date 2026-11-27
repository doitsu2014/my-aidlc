//! MCP server: launch handling (FN-016). The protocol layer is `protocol`; the tool
//! adapter (`tools`) serves `status`, `queue`, `next` and `report` over the
//! application facade, wired in by `run` below.

pub mod protocol;
pub mod redact;
pub mod tools;

use std::ffi::OsString;
use std::path::PathBuf;

/// Parse `[--project <dir>]` (CFG-005). No other argument is accepted.
fn parse_args(args: &[OsString]) -> Result<Option<PathBuf>, String> {
    match args {
        [] => Ok(None),
        [flag, _, rest @ ..] if flag == "--project" && !rest.is_empty() => Err(format!(
            "unexpected argument: {}",
            rest[0].to_string_lossy()
        )),
        [flag, dir] if flag == "--project" => Ok(Some(PathBuf::from(dir))),
        [flag] if flag == "--project" => Err("--project requires a directory".to_string()),
        [other, ..] => Err(format!("unexpected argument: {}", other.to_string_lossy())),
    }
}

/// Resolve the launch root once and check it is a directory with a safe state dir.
fn launch_root(project: Option<PathBuf>) -> Result<PathBuf, String> {
    let given = match project {
        Some(p) => p,
        None => {
            std::env::current_dir().map_err(|e| format!("cannot read current directory: {e}"))?
        }
    };
    let root = crate::orchestrate::resolve_like_node(&given)?;
    if !root.is_dir() {
        return Err(format!(
            "project root is not a directory: {}",
            root.display()
        ));
    }
    crate::audit::assert_safe_state_dir(&root)?;
    Ok(root)
}

/// Entry point of `dtech-aidlc-mcp`; returns the process exit code.
///
/// Arguments are taken as `OsString` so a non-UTF-8 `--project` path never
/// panics before the server can report a proper error. A startup failure prints
/// `dtech-aidlc-mcp: <msg>` to stderr and exits 1 before reading stdin.
pub fn run(args: Vec<OsString>) -> u8 {
    let root = match parse_args(&args).and_then(launch_root) {
        Ok(root) => root,
        Err(msg) => {
            eprintln!("dtech-aidlc-mcp: {msg}");
            return 1;
        }
    };
    let redactor = redact::Redactor::new(&root);
    if let Some(home) = std::env::var_os("DTECH_AIDLC_HOME").filter(|h| !h.is_empty()) {
        if crate::application::external_engine().is_none() {
            eprintln!(
                "dtech-aidlc-mcp: warning: DTECH_AIDLC_HOME is set but not usable ({}); using the embedded engine",
                redactor.redact_message(&home.to_string_lossy(), "<redacted-path>")
            );
        }
    }
    let ctx = tools::ToolContext { root, redactor };
    let host = protocol::ToolHost {
        list: tools::tool_list,
        call: tools::call_tool,
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    protocol::serve(stdin.lock(), stdout.lock(), &host, &ctx) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(a: &[&str]) -> Vec<OsString> {
        a.iter().map(OsString::from).collect()
    }

    #[test]
    fn parse_args_accepts_only_project() {
        assert_eq!(parse_args(&os(&[])), Ok(None));
        assert_eq!(
            parse_args(&os(&["--project", "d"])),
            Ok(Some(PathBuf::from("d")))
        );
        assert!(parse_args(&os(&["--project"])).is_err());
        assert!(parse_args(&os(&["--version"])).is_err());
        assert_eq!(
            parse_args(&os(&["--project", "d", "x"])),
            Err("unexpected argument: x".to_string())
        );
    }

    #[test]
    fn launch_root_rejects_missing_and_file_roots() {
        let dir = std::env::temp_dir().join(format!("mcp-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, "x").unwrap();
        assert!(launch_root(Some(dir.join("missing"))).is_err());
        assert!(launch_root(Some(file)).is_err());
        assert!(launch_root(Some(dir.clone())).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
