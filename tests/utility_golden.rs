//! Engine fallback, help and completion output of the binary against goldens recorded
//! from the former JavaScript reference CLI (`tests/golden/rust_utility_parity/`). Set
//! `DTECH_AIDLC_BLESS=1` to re-record them.

#[path = "common/golden.rs"]
mod golden;

use serde_json::{json, Value};
use std::process::Command;

const SUITE: &str = "rust_utility_parity";

/// The CLI under test.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("CLI output is UTF-8")
}

#[test]
fn stale_engine_home_falls_back_to_default() {
    let missing = std::env::temp_dir().join(format!("aidlc-missing-engine-{}", std::process::id()));
    let output = cli()
        .args(["scope", "solo"])
        .env("DTECH_AIDLC_HOME", &missing)
        .output()
        .unwrap();
    let repo = golden::repo();
    let stdout = golden::normalise(
        &text(&output.stdout),
        &[(missing.as_path(), "<MISSING>"), (repo.as_path(), "<REPO>")],
    );
    let actual = json!({
        "code": output.status.code(),
        "stdout": serde_json::from_str::<Value>(&stdout).unwrap(),
    });
    let mut cases = golden::Cases::open(SUITE, "stale-engine-home");
    let expected = cases.expect("scope solo", actual.clone());
    assert_eq!(actual["code"], expected["code"]);
    assert_eq!(actual["stdout"], expected["stdout"]);
    cases.finish();
}

#[test]
fn help_and_completion_match_the_golden() {
    let mut cases = golden::Cases::open(SUITE, "help-and-completion");
    for args in [
        vec![],
        vec!["help"],
        vec!["--help"],
        vec!["completion", "zsh"],
    ] {
        let output = cli().args(&args).output().unwrap();
        // `completion zsh` prints the embedded `completion.zsh` with the checkout's line
        // endings; the golden holds the LF form.
        let actual = json!({
            "code": output.status.code(),
            "stdout": golden::checkout_text(&text(&output.stdout)),
            "stderr": golden::checkout_text(&text(&output.stderr)),
        });
        let key = if args.is_empty() {
            "(no arguments)".to_owned()
        } else {
            args.join(" ")
        };
        let expected = cases.expect(&key, actual.clone());
        assert_eq!(actual["code"], expected["code"], "{args:?}");
        assert_eq!(actual["stdout"], expected["stdout"], "{args:?}");
        assert_eq!(actual["stderr"], expected["stderr"], "{args:?}");
    }
    cases.finish();
}
