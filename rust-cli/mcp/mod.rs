//! MCP server: launch handling (FN-016). The protocol layer is `protocol`; the tool
//! adapter (`tools`) serves `status`, `queue`, `next` and `report` over the
//! application facade, wired in by `run` below.

pub mod protocol;
pub mod redact;
pub mod tools;

use std::ffi::OsString;
use std::path::PathBuf;

/// Parse `[--project <dir>] [--telemetry on|off]` in any order (CFG-005, CFG-007, IF-016).
/// Returns the project and whether telemetry is on (default). Nothing else is accepted.
fn parse_args(args: &[OsString]) -> Result<(Option<PathBuf>, bool), String> {
    let (mut project, mut telemetry) = (None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if flag == "--project" {
            let dir = it.next().ok_or("--project requires a directory")?;
            if project.replace(PathBuf::from(dir)).is_some() {
                return Err("--project given more than once".to_string());
            }
        } else if flag == "--telemetry" {
            let on = match it.next().and_then(|v| v.to_str()) {
                Some("on") => true,
                Some("off") => false,
                _ => return Err("--telemetry requires on or off".to_string()),
            };
            if telemetry.replace(on).is_some() {
                return Err("--telemetry given more than once".to_string());
            }
        } else {
            return Err(format!("unexpected argument: {}", flag.to_string_lossy()));
        }
    }
    Ok((project, telemetry.unwrap_or(true)))
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
    let launched = parse_args(&args)
        .and_then(|(project, telemetry)| launch_root(project).map(|root| (root, telemetry)));
    let (root, telemetry) = match launched {
        Ok(launched) => launched,
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
    let sink = if telemetry {
        crate::telemetry::Sink::start(&root)
    } else {
        None
    };
    let ctx = tools::ToolContext {
        root,
        redactor,
        telemetry: sink,
    };
    let host = protocol::ToolHost {
        list: tools::tool_list,
        call: tools::traced_call,
        on_error: tools::on_protocol_error,
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let code = protocol::serve(stdin.lock(), stdout.lock(), &host, &ctx) as u8;
    if let Some(sink) = ctx.telemetry {
        sink.finish(std::time::Duration::from_millis(500));
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(a: &[&str]) -> Vec<OsString> {
        a.iter().map(OsString::from).collect()
    }

    #[test]
    fn parse_args_accepts_only_project_and_telemetry() {
        assert_eq!(parse_args(&os(&[])), Ok((None, true)));
        assert_eq!(
            parse_args(&os(&["--project", "d"])),
            Ok((Some(PathBuf::from("d")), true))
        );
        assert!(parse_args(&os(&["--project"])).is_err());
        assert!(parse_args(&os(&["--version"])).is_err());
        assert_eq!(
            parse_args(&os(&["--project", "d", "x"])),
            Err("unexpected argument: x".to_string())
        );
        assert_eq!(
            parse_args(&os(&["--telemetry", "off", "--project", "d"])),
            Ok((Some(PathBuf::from("d")), false))
        );
        assert_eq!(parse_args(&os(&["--telemetry", "on"])), Ok((None, true)));
        for bad in [
            &["--telemetry"][..],
            &["--telemetry", "ON"],
            &["--telemetry", "1"],
            &["--telemetry", "on", "--telemetry", "off"],
            &["--project", "a", "--project", "b"],
        ] {
            assert!(parse_args(&os(bad)).is_err(), "{bad:?}");
        }
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
