//! Regression tests for the methodology engine the native `orchestrate next` points at:
//! the embedded copy under `<project>/.aidlc/engine`, or the `DTECH_AIDLC_HOME` engine.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "aidlc-engine-{label}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(project: &Path, args: &[&str], home: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"));
    command.args(args).arg("--project").arg(project);
    match home {
        Some(home) => command.env("DTECH_AIDLC_HOME", home),
        None => command.env_remove("DTECH_AIDLC_HOME"),
    };
    command.output().unwrap()
}

fn ok(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
}

fn onboard(project: &Path, home: Option<&Path>) {
    ok(&cli(
        project,
        &["onboard", "--harness", "claude", "--scope", "solo"],
        home,
    ));
}

fn next_new(project: &Path, home: Option<&Path>) -> Value {
    ok(&cli(
        project,
        &["orchestrate", "next", "--intent", "Add login page"],
        home,
    ))
}

fn embedded(name: &str) -> &'static [u8] {
    dtech_aidlc::scope::embedded_asset(name).unwrap()
}

/// Regression: a methodology file that differs from the embedded bytes (written by
/// another CLI release, or rewritten by a checkout with autocrlf) used to block every
/// later `orchestrate next`. It is now refreshed, a version marker is written, and the
/// folder ignores itself so it is not committed.
#[test]
fn stale_engine_files_are_refreshed_and_the_engine_is_git_ignored() {
    let temp = TempDir::new("refresh");
    let project = &temp.0;
    onboard(project, None);
    next_new(project, None);
    let engine = project.join(".aidlc").join("engine");
    let contracts = engine.join("methodology/contracts.md");
    assert_eq!(
        fs::read(&contracts).unwrap(),
        embedded("methodology/contracts.md")
    );

    let mut stale = fs::read(&contracts).unwrap();
    stale[0] ^= 0x20;
    fs::write(&contracts, &stale).unwrap();
    let crlf = String::from_utf8_lossy(embedded("methodology/contracts.md")).replace('\n', "\r\n");
    let phase = engine.join("methodology/phases/analyze.md");
    fs::write(&phase, crlf).unwrap();

    let resumed = ok(&cli(project, &["orchestrate", "next"], None));
    assert_eq!(resumed["intentId"], "add-login-page");
    assert_eq!(
        fs::read(&contracts).unwrap(),
        embedded("methodology/contracts.md")
    );
    assert_eq!(
        fs::read(&phase).unwrap(),
        embedded("methodology/phases/analyze.md")
    );

    let ignore = fs::read_to_string(engine.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line.trim() == "*"), "{ignore}");
    let version = fs::read_to_string(engine.join(".engine-version")).unwrap();
    assert!(
        version.starts_with(&format!("dtech-aidlc {}\n", env!("CARGO_PKG_VERSION"))),
        "{version}"
    );
    assert!(version.contains("methodology-sha256 "), "{version}");
    // No temporary files are left behind.
    let leftovers = fs::read_dir(engine.join("methodology"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0);
}

/// When git is available, the engine files are neither tracked candidates nor shown as
/// untracked in the project repository.
#[test]
fn engine_files_do_not_show_up_in_git_status() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new("git");
    let project = &temp.0;
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(project)
        .output()
        .unwrap();
    assert!(init.status.success());
    onboard(project, None);
    next_new(project, None);
    let status = Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(project)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(
        !text.lines().any(|line| line.contains(".aidlc/engine")),
        "{text}"
    );
    assert!(text.contains(".aidlc/intents/"), "{text}");
}

#[cfg(windows)]
fn link_directory(target: &Path, link: &Path) -> Result<(), String> {
    // A junction needs no elevation or Developer Mode.
    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(unix)]
fn link_directory(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|error| error.to_string())
}

/// Regression: a project reached through a linked folder (junction on Windows, symlink
/// elsewhere, e.g. macOS `/tmp`) used to be refused because every folder from the
/// filesystem root was checked.
#[test]
fn project_reached_through_a_linked_folder_works() {
    let temp = TempDir::new("linked");
    let real = temp.0.join("real");
    fs::create_dir_all(&real).unwrap();
    let link = temp.0.join("link");
    if let Err(error) = link_directory(&real, &link) {
        eprintln!("directory links are unavailable ({error}); skipping");
        return;
    }
    onboard(&link, None);
    let directive = next_new(&link, None);
    assert_eq!(directive["intentId"], "add-login-page");
    assert!(real
        .join(".aidlc/intents/add-login-page/state.json")
        .is_file());
    assert!(real
        .join(".aidlc/engine/methodology/contracts.md")
        .is_file());
    // Remove the link before the temp folder so cleanup cannot follow it.
    let _ = fs::remove_dir(&link);
}

/// The narrower check still refuses a linked `.aidlc/engine`, writes nothing through it
/// and creates no intent.
#[test]
fn linked_engine_folder_is_refused_without_writes() {
    let temp = TempDir::new("linked-engine");
    let project = temp.0.join("project");
    let outside = temp.0.join("outside");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&outside).unwrap();
    onboard(&project, None);
    let engine = project.join(".aidlc").join("engine");
    if let Err(error) = link_directory(&outside, &engine) {
        eprintln!("directory links are unavailable ({error}); skipping");
        return;
    }
    let output = cli(
        &project,
        &["orchestrate", "next", "--intent", "Add login page"],
        None,
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Refusing symlink or non-directory path"),
        "{stderr}"
    );
    assert!(
        stderr.contains("remove it and run the command again"),
        "{stderr}"
    );
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    assert!(!project.join(".aidlc/intents/add-login-page").exists());
    let _ = fs::remove_dir(&engine);
}

/// A methodology target that is not a regular file is refused, not replaced.
#[test]
fn non_regular_methodology_target_is_refused() {
    let temp = TempDir::new("non-regular");
    let project = &temp.0;
    onboard(project, None);
    fs::create_dir_all(project.join(".aidlc/engine/methodology/contracts.md")).unwrap();
    let output = cli(
        project,
        &["orchestrate", "next", "--intent", "Add login page"],
        None,
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("non-regular methodology asset target"),
        "{stderr}"
    );
    assert!(project
        .join(".aidlc/engine/methodology/contracts.md")
        .is_dir());
    assert!(!project.join(".aidlc/intents/add-login-page").exists());
}

/// Regression: with `DTECH_AIDLC_HOME`, context templates point into that engine, and no
/// embedded copy is written into the project.
#[test]
fn external_engine_context_templates_point_into_the_engine() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp = TempDir::new("external");
    let project = &temp.0;
    onboard(project, Some(&repo));
    let directive = next_new(project, Some(&repo));
    let template = PathBuf::from(
        directive["context"]["codebaseMap"]["template"]
            .as_str()
            .unwrap_or_else(|| panic!("no codebaseMap template: {directive}")),
    );
    assert_eq!(
        template,
        repo.join("core")
            .join("methodology")
            .join("templates")
            .join("codebase-map.md")
    );
    assert!(template.is_file());
    assert!(!project.join(".aidlc/engine").exists());
}
