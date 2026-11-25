use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);

fn run_pair(args: &[&str]) -> (Output, Output) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let case_dir = std::env::temp_dir().join(format!(
        "aidlc-rust-parity-{}-{}",
        std::process::id(),
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    let node_dir = case_dir.join("node");
    let rust_dir = case_dir.join("rust");
    std::fs::create_dir_all(&node_dir).expect("create isolated Node project directory");
    std::fs::create_dir_all(&rust_dir).expect("create isolated Rust project directory");

    let node = Command::new("node")
        .arg(root.join("core/tools/dtech-aidlc.mjs"))
        .args(args)
        .current_dir(&node_dir)
        .output()
        .expect("Node CLI must be available for parity tests");
    let rust = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(args)
        .current_dir(&rust_dir)
        .output()
        .expect("run Rust CLI");

    let node_state = node_dir.join(".aidlc").exists();
    let rust_state = rust_dir.join(".aidlc").exists();
    std::fs::remove_dir_all(&case_dir).expect("remove isolated parity directories");
    assert!(
        !node_state,
        "parity validation unexpectedly created Node project state for {args:?}"
    );
    assert!(
        !rust_state,
        "parity validation unexpectedly created Rust project state for {args:?}"
    );
    (node, rust)
}

fn assert_same_result(args: &[&str]) {
    let (node, rust) = run_pair(args);
    assert_eq!(
        node.status.code(),
        rust.status.code(),
        "exit status for {args:?}"
    );
    assert_eq!(node.stdout, rust.stdout, "stdout for {args:?}");
    assert_eq!(node.stderr, rust.stderr, "stderr for {args:?}");
}

#[test]
fn version_matches_existing_cli() {
    assert_same_result(&["--version"]);
}

#[test]
fn parser_validation_matches_existing_cli() {
    // These are CLI parser/routing checks only. They do not imply that the
    // underlying project commands are implemented in Rust.
    for args in [
        &["scope", "--bogus"][..],
        &["scope", "a", "b"][..],
        &["status", "--intent"][..],
        &["status", "--intent", "--project"][..],
        &["scope", "--depth="][..],
        &["scope", "--depth", "invalid"][..],
        &["status", "--intent", "alpha", "--intent", "beta"][..],
        &["onboard", "--no-detect=true"][..],
        &["orchestrate", "next", "--intent-file", "--force-resume"][..],
        &["config", "prerequisite", "set", "build", "--check"][..],
    ] {
        assert_same_result(args);
    }
}

#[test]
fn unsupported_commands_are_not_parity() {
    // Onboarding is not implemented in Rust yet; this remains an explicit gap.
    let (node, rust) = run_pair(&["config", "scope"]);
    assert!(node.status.success());
    assert!(!rust.status.success());
    assert_ne!(node.stdout, rust.stdout);
}
