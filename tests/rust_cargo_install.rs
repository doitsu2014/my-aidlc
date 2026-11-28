use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn cargo_install_and_update_produce_self_contained_cli() {
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
    use std::io::Write;
    use std::process::Stdio;
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
    let mut server = Command::new(exe("dtech-aidlc-mcp"))
        .arg("--project")
        .arg(&project)
        .current_dir(&root)
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
    let replies: Vec<serde_json::Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 2);
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
        fs::write(source.join("assets/phase-graph.json"), "{\"version\":1}\n").unwrap();
        let executable = bin.join("dtech-aidlc");
        fs::write(
            &executable,
            "#!/bin/sh\n[ -f \"$DTECH_AIDLC_ASSETS/phase-graph.json\" ] || exit 9\nprintf \"asset-ok\\n\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let prefix = dir.join("prefix");
        Self {
            dir,
            source,
            prefix,
        }
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

fn sh_install(f: &ScriptFixture, args: &[&str]) -> std::process::Output {
    Command::new("sh")
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
    Command::new("pwsh")
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
        .output()
        .unwrap();
    assert_ok(&run);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "asset-ok\n");
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
    if !tool_available(
        "pwsh",
        &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
    ) {
        eprintln!("skipped: pwsh unavailable");
        return;
    }
    let f = ScriptFixture::new("ps");
    fs::write(
        f.source.join("target/release/dtech-aidlc.exe"),
        "test executable placeholder",
    )
    .unwrap();
    assert_ok(&ps_install(&f, &[]));
    assert!(f.lib().join("assets/phase-graph.json").exists());
    assert!(f.lib().join("dtech-aidlc.exe").exists());
    assert_ok(&ps_install(&f, &["-Upgrade"]));
    assert_ok(&ps_install(&f, &["-Uninstall"]));
    assert!(!f.lib().exists());
}

#[test]
fn powershell_installer_upgrades_node_layout_and_never_removes_a_node_install() {
    if !tool_available(
        "pwsh",
        &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
    ) {
        eprintln!("skipped: pwsh unavailable");
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
