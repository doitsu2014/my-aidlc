use std::process::Command;

#[test]
fn reports_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc-rs"))
        .arg("--version")
        .output()
        .expect("run Rust CLI");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("dtech-aidlc {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn unsupported_commands_fail_without_mutating_project() {
    let project = std::env::temp_dir().join(format!("aidlc-rust-test-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc-rs"))
        .current_dir(&project)
        .args(["orchestrate", "next", "--intent", "test"])
        .output()
        .expect("run Rust CLI");
    assert!(!output.status.success());
    assert!(!project.join(".aidlc").exists());
    std::fs::remove_dir(&project).unwrap();
}
