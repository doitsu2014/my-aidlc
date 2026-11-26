//! Parser, version, configuration-query and doctor results of the binary, run in an
//! empty directory, against goldens recorded from the former JavaScript reference CLI
//! (`tests/golden/rust_parity/`). Set `DTECH_AIDLC_BLESS=1` to re-record them.

#[path = "common/golden.rs"]
mod golden;

use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const SUITE: &str = "rust_parity";

static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);

/// The CLI under test.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("CLI output is UTF-8")
}

/// Runs the CLI in a fresh empty directory, checks that it created no project state, and
/// returns its output with the directory and repository paths normalised.
fn run_isolated(args: &[&str]) -> (Output, impl Fn(&str) -> String) {
    let case_dir = std::env::temp_dir().join(format!(
        "aidlc-rust-parity-{}-{}",
        std::process::id(),
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&case_dir).expect("create isolated project directory");
    let output = cli()
        .args(args)
        .current_dir(&case_dir)
        .output()
        .expect("run CLI");
    let state = case_dir.join(".aidlc").exists();
    std::fs::remove_dir_all(&case_dir).expect("remove isolated directory");
    assert!(
        !state,
        "validation unexpectedly created project state for {args:?}"
    );
    let normalise = move |value: &str| {
        let repo = golden::repo();
        golden::normalise(
            value,
            &[(repo.as_path(), "<REPO>"), (Path::new(&case_dir), "<ROOT>")],
        )
    };
    (output, normalise)
}

/// Exit code, stdout and stderr of one run, compared byte for byte after normalisation.
fn assert_same_result(cases: &mut golden::Cases, args: &[&str]) {
    let (output, normalise) = run_isolated(args);
    let actual = json!({
        "code": output.status.code(),
        "stdout": normalise(&text(&output.stdout)),
        "stderr": normalise(&text(&output.stderr)),
    });
    let expected = cases.expect(&args.join(" "), actual.clone());
    assert_eq!(expected["code"], actual["code"], "exit status for {args:?}");
    assert_eq!(expected["stdout"], actual["stdout"], "stdout for {args:?}");
    assert_eq!(expected["stderr"], actual["stderr"], "stderr for {args:?}");
}

#[test]
fn version_matches_the_golden() {
    let mut cases = golden::Cases::open(SUITE, "version");
    assert_same_result(&mut cases, &["--version"]);
    cases.finish();
}

#[test]
fn parser_validation_matches_the_golden() {
    // These are CLI parser/routing checks only. They do not imply that the
    // underlying project commands are implemented in Rust.
    let mut cases = golden::Cases::open(SUITE, "parser-validation");
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
        assert_same_result(&mut cases, args);
    }
    cases.finish();
}

#[test]
fn configuration_queries_match_the_golden() {
    let mut cases = golden::Cases::open(SUITE, "configuration-queries");
    for args in [
        &["config", "scope"][..],
        &["config", "mode"][..],
        &["config", "gate-mode"][..],
        &["config", "gate-mode", "release-candidate"][..],
    ] {
        let (output, normalise) = run_isolated(args);
        let stdout: Value = serde_json::from_str(&normalise(&text(&output.stdout))).unwrap();
        let actual = json!({ "code": output.status.code(), "stdout": stdout });
        let expected = cases.expect(&args.join(" "), actual.clone());
        assert_eq!(expected["code"], actual["code"], "{args:?}");
        assert_eq!(expected["stdout"], actual["stdout"], "{args:?}");
    }
    cases.finish();
}

#[test]
fn doctor_matches_the_golden() {
    let mut cases = golden::Cases::open(SUITE, "doctor");
    assert_same_result(&mut cases, &["doctor"]);
    cases.finish();
}
