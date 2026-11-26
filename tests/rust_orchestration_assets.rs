use dtech_aidlc::orchestrate;
use dtech_aidlc::orchestration_assets::materialize_methodology_assets;
use dtech_aidlc::scope;
use dtech_aidlc::workflow::Workflow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "aidlc-orchestration-assets-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn materializes_assets_and_is_idempotent() {
    let temp = TempDir::new();
    let assets = [
        ("methodology/contracts.md", b"contracts".as_slice()),
        ("methodology/phases/analyze.md", b"phase".as_slice()),
    ];
    let canonical = materialize_methodology_assets(&temp.0, assets).unwrap();
    assert_eq!(canonical, fs::canonicalize(&temp.0).unwrap());
    assert_eq!(
        fs::read(canonical.join("methodology/contracts.md")).unwrap(),
        b"contracts"
    );
    assert_eq!(
        fs::read(canonical.join("methodology/phases/analyze.md")).unwrap(),
        b"phase"
    );
    assert_eq!(
        materialize_methodology_assets(&temp.0, assets).unwrap(),
        canonical
    );
}

#[test]
fn rejects_invalid_asset_names() {
    let temp = TempDir::new();
    for name in [
        "../outside.txt",
        "methodology/../outside.txt",
        "methodology\\..\\outside.txt",
        "data/phase-graph.json",
        "methodology/",
        "/methodology/contracts.md",
    ] {
        assert!(
            materialize_methodology_assets(&temp.0, [(name, b"x".as_slice())]).is_err(),
            "{name}"
        );
    }
}

#[test]
fn refuses_conflicting_existing_file_without_overwriting() {
    let temp = TempDir::new();
    let target = temp.0.join("methodology/contracts.md");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"local content").unwrap();
    let error = materialize_methodology_assets(
        &temp.0,
        [("methodology/contracts.md", b"embedded".as_slice())],
    )
    .unwrap_err();
    assert!(error.contains("Refusing to replace"));
    assert_eq!(fs::read(target).unwrap(), b"local content");
}

#[test]
fn refuses_symlinked_target_and_parent() {
    let temp = TempDir::new();
    let outside = temp.0.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let target_link = temp.0.join("methodology");
    fs::create_dir_all(&target_link).unwrap();
    let link = target_link.join("contracts.md");
    if make_file_symlink(&outside.join("victim.md"), &link).is_err() {
        return; // Symlink creation may be unavailable on Windows without Developer Mode/elevation.
    }
    assert!(materialize_methodology_assets(
        &temp.0,
        [("methodology/contracts.md", b"x".as_slice())]
    )
    .unwrap_err()
    .contains("Refusing non-regular"));
    assert!(!outside.join("victim.md").exists());

    fs::remove_file(link).unwrap();
    fs::remove_dir(&target_link).unwrap();
    if make_dir_symlink(&outside, &target_link).is_err() {
        return; // Symlink creation may be unavailable on Windows without Developer Mode/elevation.
    }
    assert!(materialize_methodology_assets(
        &temp.0,
        [("methodology/contracts.md", b"x".as_slice())]
    )
    .unwrap_err()
    .contains("symlink"));
    assert!(!outside.join("contracts.md").exists());
}

#[test]
fn refuses_symlink_in_engine_root_path() {
    let temp = TempDir::new();
    let outside = temp.0.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let link = temp.0.join("linked-engine");
    if make_dir_symlink(&outside, &link).is_err() {
        return; // Symlink creation may be unavailable on Windows without Developer Mode/elevation.
    }
    assert!(
        materialize_methodology_assets(&link, [("methodology/contracts.md", b"x".as_slice())])
            .unwrap_err()
            .contains("symlink")
    );
    assert!(!outside.join("methodology").exists());
}

#[test]
fn only_regular_files_are_accepted_for_preexisting_assets() {
    let temp = TempDir::new();
    let directory = temp.0.join("methodology/contracts.md");
    fs::create_dir_all(&directory).unwrap();
    assert!(materialize_methodology_assets(
        &temp.0,
        [("methodology/contracts.md", b"x".as_slice())]
    )
    .is_err());
}

#[test]
fn orchestrate_next_uses_materialized_embedded_methodology_files() {
    let temp = TempDir::new();
    let (graph, _) = scope::load_embedded_graph(&temp.0).unwrap();
    let workflow = Workflow::new(&temp.0, graph.value().clone());
    let mut state = workflow
        .create("embedded assets", "solo", "human", "full")
        .unwrap();
    let directive = orchestrate::next(&workflow, &mut state).unwrap();
    let engine_root = fs::canonicalize(temp.0.join(".aidlc/engine")).unwrap();

    {
        let path = Path::new(directive["contractsFile"].as_str().unwrap());
        assert!(path.starts_with(&engine_root));
        assert!(path.is_file());
    }
    for key in ["phaseFile", "agentFile"] {
        let value = directive["stage"][key].as_str().unwrap();
        let path = Path::new(value);
        assert!(path.starts_with(&engine_root));
        assert!(path.is_file(), "{value}");
    }
    for artifact in directive["artifacts"].as_array().unwrap() {
        if let Some(template) = artifact["template"].as_str() {
            let path = Path::new(template);
            assert!(path.starts_with(&engine_root));
            assert!(path.is_file(), "{template}");
        }
    }

    let embedded_contracts = scope::embedded_asset("methodology/contracts.md").unwrap();
    assert_eq!(
        fs::read(engine_root.join("methodology/contracts.md")).unwrap(),
        embedded_contracts
    );
}

#[cfg(unix)]
fn make_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(windows)]
fn make_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
#[cfg(unix)]
fn make_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(windows)]
fn make_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}
