use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn dependency_files_are_not_registered_and_missing_manifest_rescan_fails_safely() {
    let root = std::env::temp_dir().join(format!(
        "aidlc-onboard-safety-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("requirements.txt"), "example==1.0\n").unwrap();
    fs::write(root.join("docs/guide.md"), "guide\n").unwrap();
    let exe = env!("CARGO_BIN_EXE_dtech-aidlc");
    let onboard = Command::new(exe)
        .current_dir(&root)
        .args(["onboard", "--harness", "claude", "--scope", "solo"])
        .output()
        .unwrap();
    assert!(
        onboard.status.success(),
        "{}",
        String::from_utf8_lossy(&onboard.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("aidlc.knowledge.json")).unwrap()).unwrap();
    assert!(!manifest["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["path"] == "requirements.txt"));
    fs::remove_file(root.join("aidlc.knowledge.json")).unwrap();
    let rescan = Command::new(exe)
        .current_dir(&root)
        .args(["onboard", "--rescan", "--docs", "docs"])
        .output()
        .unwrap();
    assert!(!rescan.status.success());
    assert!(!root.join("aidlc.knowledge.json").exists());
    fs::remove_dir_all(root).unwrap();
}
