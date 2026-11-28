use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// `cargo install --path .` tests share target/release; run them one at a time.
static CARGO_INSTALL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn cargo_install_and_update_produce_self_contained_cli() {
    let _serial = CARGO_INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aidlc-cargo-install-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("bin")).unwrap();
    let unrelated = root.join("bin").join("unrelated-tool");
    fs::write(&unrelated, b"existing unrelated file").unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let install = |update: bool| {
        let mut command = Command::new("cargo");
        command
            .args(["install", "--offline", "--locked", "--path"])
            .arg(&repo)
            .arg("--root")
            .arg(&root)
            .arg("--bin")
            .arg("dtech-aidlc");
        if update {
            command.arg("--force");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    install(false);
    install(true);
    assert_eq!(fs::read(&unrelated).unwrap(), b"existing unrelated file");
    let exe = root.join("bin").join(if cfg!(windows) {
        "dtech-aidlc.exe"
    } else {
        "dtech-aidlc"
    });
    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    let output = Command::new(&exe)
        .args(["scope", "solo"])
        .current_dir(&project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["scope"], "solo");
    let onboard = Command::new(&exe)
        .args(["onboard", "--harness", "claude"])
        .current_dir(&project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap();
    assert!(
        onboard.status.success(),
        "{}",
        String::from_utf8_lossy(&onboard.stderr)
    );
    assert!(project
        .join(".claude/skills/dtech-aidlc/SKILL.md")
        .is_file());
    let uninstall = Command::new("cargo")
        .args(["uninstall", "dtech-aidlc", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
    assert_eq!(fs::read(&unrelated).unwrap(), b"existing unrelated file");
    fs::remove_dir_all(root).unwrap();
}

/// TS-009: both executables install together from the package, run from outside the
/// checkout (the MCP server over stdio, the CLI over its arguments), and one
/// `cargo uninstall` removes both without touching unrelated files.
#[test]
fn ts009_cargo_install_and_uninstall_both_binaries() {
    let _serial = CARGO_INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aidlc-cargo-install-mcp-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("bin")).unwrap();
    let unrelated = root.join("bin").join("unrelated-tool");
    fs::write(&unrelated, b"existing unrelated file").unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let install = Command::new("cargo")
        .args(["install", "--offline", "--locked", "--path"])
        .arg(&repo)
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let exe = |name: &str| {
        root.join("bin").join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        })
    };
    assert!(exe("dtech-aidlc").is_file());
    assert!(exe("dtech-aidlc-mcp").is_file());
    assert_eq!(fs::read(&unrelated).unwrap(), b"existing unrelated file");

    // Run the installed server against a throwaway project, outside the checkout.
    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    let cli = Command::new(exe("dtech-aidlc"))
        .args([
            "onboard",
            "--harness",
            "claude",
            "--scope",
            "solo",
            "--project",
        ])
        .arg(&project)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let version = Command::new(exe("dtech-aidlc"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")));
    let help = Command::new(exe("dtech-aidlc"))
        .arg("--help")
        .output()
        .unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert_mcp_handshake(
        Command::new(exe("dtech-aidlc-mcp")),
        &project,
        &root.join("home"),
    );

    let uninstall = Command::new("cargo")
        .args(["uninstall", "dtech-aidlc", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
    assert!(!exe("dtech-aidlc").exists());
    assert!(!exe("dtech-aidlc-mcp").exists());
    assert_eq!(fs::read(&unrelated).unwrap(), b"existing unrelated file");
    fs::remove_dir_all(root).unwrap();
}

// ---------------------------------------------------------------------------
// scripts/rust-install/install.{sh,ps1} (formerly scripts/rust-install/test.mjs)
// ---------------------------------------------------------------------------

/// A throwaway source tree (`target/release/dtech-aidlc` plus `assets/`) and prefix.
struct ScriptFixture {
    dir: PathBuf,
    source: PathBuf,
    prefix: PathBuf,
}

impl ScriptFixture {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "aidlc-rust-inst-{label}-{}-{unique}",
            std::process::id()
        ));
        let source = dir.join("source");
        let bin = source.join("target/release");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(source.join("assets")).unwrap();
        fs::create_dir_all(dir.join("home")).unwrap();
        fs::write(source.join("assets/phase-graph.json"), "{\"version\":1}\n").unwrap();
        for (name, real) in [
            ("dtech-aidlc", env!("CARGO_BIN_EXE_dtech-aidlc")),
            ("dtech-aidlc-mcp", env!("CARGO_BIN_EXE_dtech-aidlc-mcp")),
        ] {
            fs::copy(real, bin.join(name)).unwrap();
        }
        let prefix = dir.join("prefix");
        Self {
            dir,
            source,
            prefix,
        }
    }

    /// Lay out a CLI-only v1 installation (marker v1, no MCP binary) plus unrelated files.
    fn write_v1_layout(&self, windows: bool) {
        let (cli, launcher, text) = if windows {
            (
                "dtech-aidlc.exe",
                "dtech-aidlc.cmd",
                "@echo off\r\n\"%~dp0..\\lib\\dtech-aidlc\\dtech-aidlc.exe\" %*\r\n",
            )
        } else {
            (
                "dtech-aidlc",
                "dtech-aidlc",
                "#!/bin/sh\nexec \"$(dirname -- \"$0\")/../lib/dtech-aidlc/dtech-aidlc\" \"$@\"\n",
            )
        };
        fs::create_dir_all(self.lib().join("assets")).unwrap();
        fs::write(
            self.lib().join(".dtech-aidlc-rust-install"),
            "dtech-aidlc rust installer v1\n",
        )
        .unwrap();
        fs::write(self.lib().join(cli), "old cli").unwrap();
        fs::write(self.lib().join("assets/phase-graph.json"), "{}").unwrap();
        fs::create_dir_all(self.prefix.join("bin")).unwrap();
        fs::write(self.prefix.join("bin").join(launcher), text).unwrap();
        fs::write(self.prefix.join("bin/unrelated-tool"), "keep").unwrap();
        fs::write(self.prefix.join("lib/other-tool"), "keep").unwrap();
    }

    /// A stand-in for the user's AI-host settings file: no installer step may touch it.
    fn write_settings(&self) -> (PathBuf, Vec<u8>) {
        let path = self.dir.join("home/.claude/settings.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = b"{\"mcpServers\": {\"keep\": true}}\n".to_vec();
        fs::write(&path, &bytes).unwrap();
        (path, bytes)
    }

    fn lib(&self) -> PathBuf {
        self.prefix.join("lib/dtech-aidlc")
    }

    /// Lay out what the removed Node installer left behind: the package with its exact
    /// identity and entry point, and optionally its launcher.
    fn write_node_layout(&self, with_launcher: bool) {
        fs::create_dir_all(self.lib().join("core/tools")).unwrap();
        fs::write(
            self.lib().join("package.json"),
            "{\"name\":\"dtech-aidlc\"}",
        )
        .unwrap();
        fs::write(self.lib().join("core/tools/dtech-aidlc.mjs"), "").unwrap();
        if with_launcher {
            fs::create_dir_all(self.prefix.join("bin")).unwrap();
            fs::write(
                self.prefix.join("bin/dtech-aidlc"),
                "#!/bin/sh\nentry=$(dirname -- \"$0\")/../lib/dtech-aidlc/core/tools/dtech-aidlc.mjs\nexec node \"$entry\" \"$@\"\n",
            )
            .unwrap();
        }
    }
}

impl Drop for ScriptFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/rust-install")
}

fn tool_available(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// A path the POSIX shell understands. On Windows (Git Bash / MSYS) `D:\a\b` becomes `/d/a/b`.
fn sh_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        let bytes = text.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' {
            return format!("/{}{}", text[..1].to_ascii_lowercase(), &text[2..]);
        }
    }
    text
}

/// Point every home-like location at `<dir>/home` so the settings-bytes checks cover the
/// files an installer, launcher or host could really touch.
fn isolate_home<'a>(command: &'a mut Command, home: &std::path::Path) -> &'a mut Command {
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
}

fn sh_install(f: &ScriptFixture, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("sh");
    isolate_home(&mut command, &f.dir.join("home"));
    command
        .arg(sh_path(&scripts_dir().join("install.sh")))
        .args([
            "--from",
            &sh_path(&f.source),
            "--prefix",
            &sh_path(&f.prefix),
        ])
        .args(args)
        .output()
        .unwrap()
}

fn ps_install(f: &ScriptFixture, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("pwsh");
    isolate_home(&mut command, &f.dir.join("home"));
    command
        .args(["-NoProfile", "-File"])
        .arg(scripts_dir().join("install.ps1"))
        .arg("-From")
        .arg(&f.source)
        .arg("-Prefix")
        .arg(&f.prefix)
        .args(args)
        .output()
        .unwrap()
}

fn assert_ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// On Windows CI pwsh must exist: a missing pwsh there is a failure, never a skip.
fn pwsh_available() -> bool {
    let ok = tool_available(
        "pwsh",
        &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
    );
    if !ok {
        assert!(!cfg!(windows), "pwsh is required on Windows");
        eprintln!("skipped: pwsh unavailable");
    }
    ok
}

fn sh_available() -> bool {
    tool_available("sh", &["-c", "true"])
}

#[test]
fn posix_installer_installs_assets_and_executable_upgrades_and_uninstalls() {
    if !sh_available() {
        eprintln!("skipped: POSIX shell unavailable");
        return;
    }
    let f = ScriptFixture::new("sh");
    assert_ok(&sh_install(&f, &[]));
    let exe = f.lib().join("dtech-aidlc");
    let run = Command::new("sh")
        .arg(sh_path(&f.prefix.join("bin/dtech-aidlc")))
        .arg("--version")
        .output()
        .unwrap();
    assert_ok(&run);
    assert!(String::from_utf8_lossy(&run.stdout).contains(env!("CARGO_PKG_VERSION")));
    assert!(f.lib().join("assets/phase-graph.json").exists());
    assert_ok(&sh_install(&f, &["--upgrade"]));
    assert!(exe.exists());
    assert_ok(&sh_install(&f, &["--uninstall"]));
    assert!(!f.lib().exists());
}

#[test]
fn posix_installer_upgrades_node_layout_and_never_removes_a_node_install() {
    if !sh_available() {
        eprintln!("skipped: POSIX shell unavailable");
        return;
    }
    let f = ScriptFixture::new("sh-node");
    f.write_node_layout(true);
    assert_ok(&sh_install(&f, &["--upgrade"]));
    assert!(!f.lib().join("package.json").exists());
    assert!(f.lib().join("dtech-aidlc").exists());
    assert_ok(&sh_install(&f, &["--uninstall"]));
    assert!(!f.lib().exists());
    // A Node install without the Rust marker is never removed by the Rust uninstaller.
    f.write_node_layout(false);
    assert_ok(&sh_install(&f, &["--uninstall"]));
    assert!(f.lib().join("package.json").exists());
}

#[test]
fn powershell_installer_installs_assets_and_round_trips_upgrade_and_uninstall() {
    if !pwsh_available() {
        return;
    }
    let f = ScriptFixture::new("ps");
    assert_ok(&ps_install(&f, &[]));
    assert!(f.lib().join("assets/phase-graph.json").exists());
    assert!(f.lib().join("dtech-aidlc.exe").exists());
    assert_ok(&ps_install(&f, &["-Upgrade"]));
    assert_ok(&ps_install(&f, &["-Uninstall"]));
    assert!(!f.lib().exists());
}

#[test]
fn powershell_installer_upgrades_node_layout_and_never_removes_a_node_install() {
    if !pwsh_available() {
        return;
    }
    let f = ScriptFixture::new("ps-node");
    f.write_node_layout(false);
    assert_ok(&ps_install(&f, &["-Upgrade"]));
    assert!(!f.lib().join("package.json").exists());
    assert!(f.lib().join("dtech-aidlc.exe").exists());
    assert_ok(&ps_install(&f, &["-Uninstall"]));
    f.write_node_layout(false);
    assert_ok(&ps_install(&f, &["-Uninstall"]));
    assert!(f.lib().join("package.json").exists());
}

// ---------------------------------------------------------------------------
// TS-034: installers install, upgrade and uninstall both binaries
// ---------------------------------------------------------------------------

/// Drive an installed MCP server through initialize and tools/list, from `project`
/// (outside the checkout) with `home` as every home-like directory.
fn assert_mcp_handshake(mut command: Command, project: &std::path::Path, home: &std::path::Path) {
    use std::io::Write;
    use std::process::Stdio;
    isolate_home(&mut command, home);
    let mut server = command
        .arg("--project")
        .arg(project)
        .current_dir(project)
        .env_remove("DTECH_AIDLC_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = server.stdin.take().unwrap();
        for frame in [
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-11-25", "capabilities": {}}}),
            serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        ] {
            writeln!(stdin, "{frame}").unwrap();
        }
    }
    let out = server.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let replies: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 2, "{text}");
    assert_eq!(
        replies[0]["result"]["serverInfo"]["name"],
        "dtech-aidlc-mcp"
    );
    let names: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["status", "queue", "next", "report"]);
}

/// IF-020: the MCP binary takes no flags, so `--version` and `--help` exit 1.
fn assert_mcp_refuses_flags(make: impl Fn() -> Command, home: &std::path::Path) {
    for flag in ["--version", "--help"] {
        let mut command = make();
        isolate_home(&mut command, home);
        let out = command.arg(flag).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{flag}");
    }
}

#[test]
fn ts034_installed_cli_starts_monitor_from_its_own_location() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let _serial = CARGO_INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aidlc-cargo-install-monitor-{}-{unique}",
        std::process::id()
    ));
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let install = Command::new("cargo")
        .args(["install", "--offline", "--locked", "--path"])
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let exe = root.join("bin").join(if cfg!(windows) {
        "dtech-aidlc.exe"
    } else {
        "dtech-aidlc"
    });
    let mut command = Command::new(exe);
    isolate_home(&mut command, &root.join("home"));
    let mut monitor = command
        .args(["monitor", "--project"])
        .arg(&project)
        .current_dir(&project)
        .env_remove("DTECH_AIDLC_HOME")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(monitor.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    // One GET / proves the installed binary serves the dashboard it embeds.
    let page = line
        .strip_prefix("monitor: http://127.0.0.1:")
        .and_then(|rest| rest.split('/').next())
        .and_then(|port| port.parse::<u16>().ok())
        .map(|port| {
            use std::io::{Read, Write};
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                stream,
                "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            response
        });
    // The monitor never exits on its own; stop it before asserting so a failure cannot leak it.
    let _ = monitor.kill();
    let _ = monitor.wait();
    assert!(
        line.starts_with("monitor: http://127.0.0.1:") && line.contains("/#token="),
        "unexpected startup line: {line:?}"
    );
    let page = page.unwrap_or_default();
    assert!(
        page.starts_with("HTTP/1.1 200") && page.contains("dtech-aidlc Monitor"),
        "unexpected response: {}",
        page.lines().next().unwrap_or("")
    );
    let _ = Command::new("cargo")
        .args(["uninstall", "dtech-aidlc", "--root"])
        .arg(&root)
        .output();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ts034_posix_installer_round_trips_both_binaries() {
    if !sh_available() {
        eprintln!("skipped: POSIX shell unavailable");
        return;
    }
    let f = ScriptFixture::new("sh-both");
    let (settings, bytes) = f.write_settings();
    let project = f.dir.join("project");
    fs::create_dir(&project).unwrap();
    let launcher = |name: &str| sh_path(&f.prefix.join("bin").join(name));
    let check = |f: &ScriptFixture| {
        assert!(f.lib().join("dtech-aidlc-mcp").is_file());
        assert!(f.lib().join("dtech-aidlc").is_file());
        assert_eq!(
            fs::read_to_string(f.lib().join(".dtech-aidlc-rust-install")).unwrap(),
            "dtech-aidlc rust installer v2\n"
        );
        let mut version = Command::new("sh");
        isolate_home(&mut version, &f.dir.join("home"));
        let version = version
            .arg(launcher("dtech-aidlc"))
            .arg("--version")
            .output()
            .unwrap();
        assert_ok(&version);
        let mcp = || {
            let mut mcp = Command::new("sh");
            mcp.arg(launcher("dtech-aidlc-mcp"));
            mcp
        };
        let home = f.dir.join("home");
        assert_mcp_handshake(mcp(), &project, &home);
        assert_mcp_refuses_flags(mcp, &home);
        assert_eq!(fs::read(&settings).unwrap(), bytes);
    };
    let first = sh_install(&f, &[]);
    assert_ok(&first);
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        stdout.contains("Installed Rust dtech-aidlc-mcp to"),
        "{stdout}"
    );
    check(&f);
    assert_ok(&sh_install(&f, &["--upgrade"]));
    check(&f);
    assert_ok(&sh_install(&f, &["--uninstall"]));
    assert!(!f.lib().exists());
    assert!(!f.prefix.join("bin/dtech-aidlc").exists());
    assert!(!f.prefix.join("bin/dtech-aidlc-mcp").exists());
    assert_eq!(fs::read(&settings).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn ts034_posix_term_between_swaps_exits_nonzero_and_keeps_the_previous_install() {
    // T2-22a: a TERM that lands after the launcher swaps must roll back, not report success.
    use std::os::unix::fs::PermissionsExt;
    let f = ScriptFixture::new("sh-term");
    assert_ok(&sh_install(&f, &[]));
    let snapshot = |f: &ScriptFixture| {
        (
            fs::read(f.lib().join("assets/phase-graph.json")).unwrap(),
            fs::read(f.lib().join("dtech-aidlc")).unwrap(),
            fs::read(f.prefix.join("bin/dtech-aidlc")).unwrap(),
            fs::read(f.prefix.join("bin/dtech-aidlc-mcp")).unwrap(),
        )
    };
    let before = snapshot(&f);
    // A new payload, so a half-applied upgrade is visible.
    fs::write(
        f.source.join("assets/phase-graph.json"),
        "{\"version\":2}\n",
    )
    .unwrap();
    // `mv` shim: the swap that publishes the new MCP launcher sends TERM to the installer first.
    let shims = f.dir.join("shims");
    fs::create_dir_all(&shims).unwrap();
    let shim = shims.join("mv");
    fs::write(
        &shim,
        "#!/bin/sh\nfor last; do :; done\ncase $last in */bin/dtech-aidlc-mcp) [ -e \"$0.fired\" ] || { : > \"$0.fired\"; kill -TERM $PPID; sleep 1; exit 0; } ;; esac\nexec /bin/mv \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new("sh");
    isolate_home(&mut command, &f.dir.join("home"));
    let path = format!(
        "{}:{}",
        shims.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = command
        .env("PATH", path)
        .arg(sh_path(&scripts_dir().join("install.sh")))
        .args([
            "--from",
            &sh_path(&f.source),
            "--prefix",
            &sh_path(&f.prefix),
        ])
        .arg("--upgrade")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(143),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(snapshot(&f), before, "previous install must be restored");
    assert_ok(&sh_install(&f, &["--upgrade"]));
}

#[cfg(unix)]
#[test]
fn ts034_posix_failed_fresh_install_leaves_an_empty_prefix() {
    // A first install that fails after publishing lib/dtech-aidlc must remove it again,
    // so a plain retry works (same as install.ps1).
    use std::os::unix::fs::PermissionsExt;
    let f = ScriptFixture::new("sh-fresh-fail");
    let shims = f.dir.join("shims");
    fs::create_dir_all(&shims).unwrap();
    let shim = shims.join("mv");
    fs::write(
        &shim,
        "#!/bin/sh\nfor last; do :; done\ncase $last in */bin/dtech-aidlc-mcp) exit 1 ;; esac\nexec /bin/mv \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new("sh");
    isolate_home(&mut command, &f.dir.join("home"));
    let path = format!(
        "{}:{}",
        shims.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = command
        .env("PATH", path)
        .arg(sh_path(&scripts_dir().join("install.sh")))
        .args([
            "--from",
            &sh_path(&f.source),
            "--prefix",
            &sh_path(&f.prefix),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!f.lib().exists(), "lib/dtech-aidlc left behind");
    assert!(!f.prefix.join("bin/dtech-aidlc").exists());
    assert!(!f.prefix.join("bin/dtech-aidlc-mcp").exists());
    assert_ok(&sh_install(&f, &[]));
}

#[test]
fn ts034_posix_upgrade_from_cli_only_v1_adds_mcp_and_keeps_unrelated_files() {
    if !sh_available() {
        eprintln!("skipped: POSIX shell unavailable");
        return;
    }
    let f = ScriptFixture::new("sh-v1");
    f.write_v1_layout(false);
    assert_ok(&sh_install(&f, &["--upgrade"]));
    assert!(f.lib().join("dtech-aidlc-mcp").is_file());
    assert!(f.prefix.join("bin/dtech-aidlc-mcp").is_file());
    assert_eq!(
        fs::read(f.prefix.join("bin/unrelated-tool")).unwrap(),
        b"keep"
    );
    assert_eq!(fs::read(f.prefix.join("lib/other-tool")).unwrap(), b"keep");
    assert_ok(&sh_install(&f, &["--uninstall"]));
    assert!(!f.prefix.join("bin/dtech-aidlc-mcp").exists());
    assert_eq!(
        fs::read(f.prefix.join("bin/unrelated-tool")).unwrap(),
        b"keep"
    );
}

#[test]
fn ts034_posix_installer_refuses_missing_mcp_binary_and_foreign_mcp_launcher() {
    if !sh_available() {
        eprintln!("skipped: POSIX shell unavailable");
        return;
    }
    let f = ScriptFixture::new("sh-refuse");
    fs::remove_file(f.source.join("target/release/dtech-aidlc-mcp")).unwrap();
    let out = sh_install(&f, &[]);
    assert!(!out.status.success());
    assert!(!f.prefix.join("lib").exists());
    let g = ScriptFixture::new("sh-foreign");
    g.write_v1_layout(false);
    fs::write(g.prefix.join("bin/dtech-aidlc-mcp"), "someone else's").unwrap();
    let out = sh_install(&g, &["--upgrade"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a recognised"));
    assert_eq!(
        fs::read(g.prefix.join("bin/dtech-aidlc-mcp")).unwrap(),
        b"someone else's"
    );
    assert_eq!(fs::read(g.lib().join("dtech-aidlc")).unwrap(), b"old cli");
}

#[test]
fn ts034_powershell_installer_round_trips_both_binaries() {
    if !pwsh_available() {
        return;
    }
    let f = ScriptFixture::new("ps-both");
    let (settings, bytes) = f.write_settings();
    let project = f.dir.join("project");
    fs::create_dir(&project).unwrap();
    let check = |f: &ScriptFixture| {
        assert!(f.lib().join("dtech-aidlc.exe").is_file());
        assert!(f.lib().join("dtech-aidlc-mcp.exe").is_file());
        assert_eq!(
            fs::read_to_string(f.lib().join(".dtech-aidlc-rust-install"))
                .unwrap()
                .trim(),
            "dtech-aidlc rust installer v2"
        );
        let mcp = || Command::new(f.prefix.join("bin/dtech-aidlc-mcp.exe"));
        let home = f.dir.join("home");
        assert_mcp_handshake(mcp(), &project, &home);
        assert_mcp_refuses_flags(mcp, &home);
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            isolate_home(&mut cmd, &home);
            let out = cmd
                .arg("/C")
                .arg(f.prefix.join("bin").join("dtech-aidlc.cmd"))
                .arg("--version")
                .output()
                .unwrap();
            assert_ok(&out);
            assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
        }
        assert_eq!(fs::read(&settings).unwrap(), bytes);
    };
    assert_ok(&ps_install(&f, &[]));
    assert!(f.prefix.join("bin/dtech-aidlc.cmd").is_file());
    check(&f);
    assert_ok(&ps_install(&f, &["-Upgrade"]));
    check(&f);
    assert_ok(&ps_install(&f, &["-Uninstall"]));
    assert!(!f.lib().exists());
    assert!(!f.prefix.join("bin/dtech-aidlc.cmd").exists());
    assert!(!f.prefix.join("bin/dtech-aidlc-mcp.exe").exists());
    assert_eq!(fs::read(&settings).unwrap(), bytes);
}

#[test]
fn ts034_powershell_upgrade_from_cli_only_v1_adds_mcp_and_keeps_unrelated_files() {
    if !pwsh_available() {
        return;
    }
    let f = ScriptFixture::new("ps-v1");
    f.write_v1_layout(true);
    assert_ok(&ps_install(&f, &["-Upgrade"]));
    assert!(f.lib().join("dtech-aidlc-mcp.exe").is_file());
    assert!(f.prefix.join("bin/dtech-aidlc-mcp.exe").is_file());
    assert_eq!(
        fs::read(f.prefix.join("bin/unrelated-tool")).unwrap(),
        b"keep"
    );
    assert_eq!(fs::read(f.prefix.join("lib/other-tool")).unwrap(), b"keep");
    assert_ok(&ps_install(&f, &["-Uninstall"]));
    assert!(!f.prefix.join("bin/dtech-aidlc-mcp.exe").exists());
    assert_eq!(
        fs::read(f.prefix.join("bin/unrelated-tool")).unwrap(),
        b"keep"
    );
}

/// Windows only: a running installed MCP server locks its exe; the installer must stop
/// before changing anything and print the fixed message (Check-InUse throws before the
/// rollback try block, so nothing is touched and nothing needs rolling back).
#[cfg(windows)]
#[test]
fn ts034_powershell_upgrade_refuses_when_installed_files_are_in_use() {
    // T2-22c: unverified locally; runs on windows-latest CI only.
    use std::process::{Child, Stdio};
    assert!(pwsh_available());
    let f = ScriptFixture::new("ps-inuse");
    assert_ok(&ps_install(&f, &[]));
    // Every file under the prefix, with its bytes: a refused run must change none of them.
    fn tree(dir: &std::path::Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                tree(&path, out);
            } else {
                out.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    let snapshot = |f: &ScriptFixture| {
        let mut files = Vec::new();
        tree(&f.prefix, &mut files);
        files
    };
    let before = snapshot(&f);
    let start_mcp = |f: &ScriptFixture| -> Child {
        Command::new(f.prefix.join("bin/dtech-aidlc-mcp.exe"))
            .arg("--project")
            .arg(&f.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    // The monitor runs from `lib\dtech-aidlc` itself, as `cargo`-less installs launch it.
    let start_monitor = |f: &ScriptFixture| -> Child {
        let mut command = Command::new(f.lib().join("dtech-aidlc.exe"));
        isolate_home(&mut command, &f.dir.join("home"));
        command
            .args(["monitor", "--project"])
            .arg(&f.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let text_of = |out: &std::process::Output| {
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    };
    let in_use =
        "dtech-aidlc files are in use: stop the AI host and any dtech-aidlc monitor, then re-run";
    let mut first = true;
    let starts: [&dyn Fn(&ScriptFixture) -> Child; 2] = [&start_mcp, &start_monitor];
    for start in starts {
        let mut running = start(&f);
        let out = ps_install(&f, &["-Upgrade"]);
        assert!(!out.status.success(), "{}", text_of(&out));
        if first {
            assert!(text_of(&out).contains(in_use), "{}", text_of(&out));
        }
        let uninstall = ps_install(&f, &["-Uninstall"]);
        let _ = running.kill();
        let _ = running.wait();
        assert!(!uninstall.status.success(), "{}", text_of(&uninstall));
        assert!(snapshot(&f) == before, "a refused run must change nothing");
        first = false;
    }
    // ADR-035 recovery: after the process is stopped, the same command succeeds.
    assert_ok(&ps_install(&f, &["-Upgrade"]));
    assert!(f.lib().join("dtech-aidlc-mcp.exe").is_file());
    assert!(f.prefix.join("bin/dtech-aidlc.cmd").is_file());
}

/// Uninstall must refuse a foreign MCP launcher and leave it and the install untouched.
#[test]
fn ts034_posix_uninstall_refuses_foreign_mcp_launcher() {
    if !sh_available() {
        eprintln!("skipped: POSIX shell unavailable");
        return;
    }
    let f = ScriptFixture::new("sh-uninstall-foreign");
    f.write_v1_layout(false);
    fs::write(f.prefix.join("bin/dtech-aidlc-mcp"), "someone else's").unwrap();
    let out = sh_install(&f, &["--uninstall"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a recognised"));
    assert_eq!(
        fs::read(f.prefix.join("bin/dtech-aidlc-mcp")).unwrap(),
        b"someone else's"
    );
    assert_eq!(fs::read(f.lib().join("dtech-aidlc")).unwrap(), b"old cli");
}

#[test]
fn ts034_powershell_uninstall_refuses_foreign_mcp_exe() {
    if !pwsh_available() {
        return;
    }
    let f = ScriptFixture::new("ps-uninstall-foreign");
    f.write_v1_layout(true);
    fs::write(f.prefix.join("bin/dtech-aidlc-mcp.exe"), "someone else's").unwrap();
    let out = ps_install(&f, &["-Uninstall"]);
    assert!(!out.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("not a recognised"), "{text}");
    assert_eq!(
        fs::read(f.prefix.join("bin/dtech-aidlc-mcp.exe")).unwrap(),
        b"someone else's"
    );
    assert_eq!(
        fs::read(f.lib().join("dtech-aidlc.exe")).unwrap(),
        b"old cli"
    );
}
