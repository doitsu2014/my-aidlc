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
