use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn cargo_install_and_update_keep_preview_separate_and_self_contained() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aidlc-cargo-install-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("bin")).unwrap();
    let production = root.join("bin").join(if cfg!(windows) {
        "dtech-aidlc.cmd"
    } else {
        "dtech-aidlc"
    });
    fs::write(&production, b"existing production launcher").unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let install = |update: bool| {
        let mut command = Command::new("cargo");
        command
            .args(["install", "--offline", "--locked", "--path"])
            .arg(&repo)
            .arg("--root")
            .arg(&root)
            .arg("--bin")
            .arg("dtech-aidlc-rs");
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
    assert_eq!(
        fs::read(&production).unwrap(),
        b"existing production launcher"
    );
    let exe = root.join("bin").join(if cfg!(windows) {
        "dtech-aidlc-rs.exe"
    } else {
        "dtech-aidlc-rs"
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
    let unsupported = Command::new(&exe)
        .args(["onboard", "--harness", "claude"])
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    assert!(!project.join(".aidlc").exists());
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
    assert_eq!(
        fs::read(&production).unwrap(),
        b"existing production launcher"
    );
    fs::remove_dir_all(root).unwrap();
}
