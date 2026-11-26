use std::process::Command;

#[test]
fn stale_engine_home_falls_back_to_default() {
    let missing = std::env::temp_dir().join(format!("aidlc-missing-engine-{}", std::process::id()));
    let reference = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/core/tools/dtech-aidlc.mjs"
        ))
        .args(["scope", "solo"])
        .env("DTECH_AIDLC_HOME", &missing)
        .output()
        .unwrap();
    let actual = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(["scope", "solo"])
        .env("DTECH_AIDLC_HOME", &missing)
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), reference.status.code());
    let actual: serde_json::Value = serde_json::from_slice(&actual.stdout).unwrap();
    let expected: serde_json::Value = serde_json::from_slice(&reference.stdout).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn help_and_completion_match_existing_cli() {
    for args in [
        vec![],
        vec!["help"],
        vec!["--help"],
        vec!["completion", "zsh"],
    ] {
        let reference = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/core/tools/dtech-aidlc.mjs"
            ))
            .args(&args)
            .output()
            .unwrap();
        let actual = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), reference.status.code(), "{args:?}");
        assert_eq!(actual.stdout, reference.stdout, "{args:?}");
        assert_eq!(actual.stderr, reference.stderr, "{args:?}");
    }
}
