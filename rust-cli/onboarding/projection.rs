//! Safe, data-driven harness projection support.
//!
//! Callers supply a manifest and source root; this module does not infer engine
//! locations or modify shared project files. Locally edited targets are retained
//! unless `force` is explicitly requested, in which case a numbered backup is made.

use super::{safe_join, write_generated};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionFile {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifest {
    pub harness: String,
    pub skill_dir: String,
    pub files: Vec<ProjectionFile>,
    pub obsolete_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatus {
    pub path: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionStatus {
    pub harness: String,
    pub state: String,
    pub files: Vec<FileStatus>,
    pub obsolete: Vec<FileStatus>,
    pub stamped_engine_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionAction {
    pub path: String,
    pub action: String,
    pub backup: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshResult {
    pub harness: String,
    pub state_before: String,
    pub state: String,
    pub actions: Vec<ProjectionAction>,
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    // Node's hashText decodes UTF-8 and normalizes CRLF before hashing.
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn parse(value: &str) -> [u64; 3] {
        let mut out = [0; 3];
        let mut it = value.split('.');
        for item in &mut out {
            *item = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        }
        out
    }
    parse(a).cmp(&parse(b))
}

fn read_stamp(root: &Path, manifest: &ProjectionManifest) -> Option<Value> {
    let path = safe_join(root, &format!("{}/.projection.json", manifest.skill_dir)).ok()?;
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (value.get("schemaVersion").and_then(Value::as_u64) == Some(1)).then_some(value)
}

fn current_hash(path: &Path) -> io::Result<String> {
    Ok(sha256(&fs::read(path)?))
}
fn stamp_hash<'a>(stamp: &'a Value, target: &str) -> Option<&'a str> {
    stamp.get("files")?.get(target)?.get("sha256")?.as_str()
}

pub fn projection_status(
    project_root: &Path,
    source_root: &Path,
    manifest: &ProjectionManifest,
    engine_version: &str,
) -> io::Result<ProjectionStatus> {
    let stamp = read_stamp(project_root, manifest);
    let stamp_version = stamp
        .as_ref()
        .and_then(|v| v.get("engineVersion"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut files = Vec::new();
    for item in &manifest.files {
        let target = safe_join(
            project_root,
            &format!("{}/{}", manifest.skill_dir, item.target),
        )?;
        let source = safe_join(
            source_root,
            &format!("{}/{}", manifest.harness, item.source),
        )?;
        let state = match fs::symlink_metadata(&target) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => "missing",
            Err(e) => return Err(e),
            Ok(m) if !m.is_file() => "modified",
            Ok(_) => {
                let actual = current_hash(&target)?;
                if actual == current_hash(&source)? {
                    "current"
                } else if stamp.as_ref().and_then(|s| stamp_hash(s, &item.target))
                    == Some(actual.as_str())
                {
                    "behind"
                } else {
                    "modified"
                }
            }
        };
        files.push(FileStatus {
            path: item.target.clone(),
            state: state.into(),
        });
    }
    let mut obsolete = Vec::new();
    for rel in &manifest.obsolete_files {
        let path = safe_join(project_root, &format!("{}/{}", manifest.skill_dir, rel))?;
        let state = match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
            Ok(m) if !m.is_file() => "modified",
            Ok(_) => {
                let actual = current_hash(&path)?;
                if stamp.as_ref().and_then(|s| stamp_hash(s, rel)) == Some(actual.as_str()) {
                    "pristine"
                } else {
                    "modified"
                }
            }
        };
        obsolete.push(FileStatus {
            path: rel.clone(),
            state: state.into(),
        });
    }
    let ahead = stamp_version
        .as_ref()
        .is_some_and(|v| compare_versions(v, engine_version).is_gt())
        && files.iter().any(|f| f.state != "current");
    let state = if ahead {
        "ahead"
    } else if files.iter().any(|f| f.state == "missing") {
        "missing"
    } else if files
        .iter()
        .chain(obsolete.iter())
        .any(|f| f.state == "modified")
    {
        "modified"
    } else if files.iter().any(|f| f.state == "behind")
        || obsolete.iter().any(|f| f.state == "pristine")
    {
        "behind"
    } else {
        "current"
    };
    Ok(ProjectionStatus {
        harness: manifest.harness.clone(),
        state: state.into(),
        files,
        obsolete,
        stamped_engine_version: stamp_version,
    })
}

fn numbered_backup(path: &Path) -> io::Result<PathBuf> {
    let first = PathBuf::from(format!("{}.local-backup", path.display()));
    if !first.exists() {
        return Ok(first);
    }
    for n in 2..=10000 {
        let candidate = PathBuf::from(format!("{}.local-backup.{n}", path.display()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free backup path",
    ))
}

pub fn refresh_projection(
    project_root: &Path,
    source_root: &Path,
    manifest: &ProjectionManifest,
    engine_version: &str,
    dry_run: bool,
    force: bool,
) -> io::Result<RefreshResult> {
    let before = projection_status(project_root, source_root, manifest, engine_version)?;
    let mut actions = Vec::new();
    if before.state == "ahead" && !force {
        actions.push(ProjectionAction {
            path: manifest.skill_dir.clone(),
            action: "refused-ahead".into(),
            backup: None,
        });
        return Ok(RefreshResult {
            harness: manifest.harness.clone(),
            state_before: before.state.clone(),
            state: "ahead".into(),
            actions,
        });
    }
    for item in &manifest.files {
        let state = before
            .files
            .iter()
            .find(|f| f.path == item.target)
            .map(|f| f.state.as_str())
            .unwrap_or("missing");
        if state == "current" {
            continue;
        }
        let target_rel = format!("{}/{}", manifest.skill_dir, item.target);
        let target = safe_join(project_root, &target_rel)?;
        let source = safe_join(
            source_root,
            &format!("{}/{}", manifest.harness, item.source),
        )?;
        match state {
            "missing" | "behind" => {
                actions.push(ProjectionAction {
                    path: target_rel.clone(),
                    action: if state == "missing" {
                        "created"
                    } else {
                        "updated"
                    }
                    .into(),
                    backup: None,
                });
                if !dry_run {
                    write_generated(project_root, &target_rel, &fs::read(source)?)?;
                }
            }
            "modified" if !force => actions.push(ProjectionAction {
                path: target_rel,
                action: "kept-modified".into(),
                backup: None,
            }),
            "modified" => {
                let metadata = fs::symlink_metadata(&target)?;
                if !metadata.is_file() {
                    actions.push(ProjectionAction {
                        path: target_rel,
                        action: "refused-not-a-regular-file".into(),
                        backup: None,
                    });
                    continue;
                }
                let backup = numbered_backup(&target)?;
                let backup_rel = backup
                    .strip_prefix(fs::canonicalize(project_root)?)
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "backup outside root")
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");
                actions.push(ProjectionAction {
                    path: target_rel.clone(),
                    action: "replaced-with-backup".into(),
                    backup: Some(backup_rel),
                });
                if !dry_run {
                    let backup_rel = actions.last().and_then(|a| a.backup.as_deref()).unwrap();
                    write_generated(project_root, backup_rel, &fs::read(&target)?)?;
                    write_generated(project_root, &target_rel, &fs::read(source)?)?;
                }
            }
            _ => {}
        }
    }
    for item in &before.obsolete {
        if item.state == "modified" {
            actions.push(ProjectionAction {
                path: item.path.clone(),
                action: "kept-modified-obsolete".into(),
                backup: None,
            });
            continue;
        }
        actions.push(ProjectionAction {
            path: format!("{}/{}", manifest.skill_dir, item.path),
            action: "removed".into(),
            backup: None,
        });
        if !dry_run {
            fs::remove_file(safe_join(
                project_root,
                &format!("{}/{}", manifest.skill_dir, item.path),
            )?)?;
        }
    }
    if !dry_run {
        let status = projection_status(project_root, source_root, manifest, engine_version)?;
        let mut hashes = BTreeMap::new();
        for file in &manifest.files {
            if file_state(&status, &file.target) == Some("current") {
                let path = safe_join(
                    project_root,
                    &format!("{}/{}", manifest.skill_dir, file.target),
                )?;
                hashes.insert(file.target.clone(), json!({"sha256": current_hash(&path)?}));
            }
        }
        let stamp = json!({"schemaVersion":1,"harness":manifest.harness,"engineVersion":engine_version,"files":hashes});
        write_generated(
            project_root,
            &format!("{}/.projection.json", manifest.skill_dir),
            format!("{}\n", serde_json::to_string_pretty(&stamp)?).as_bytes(),
        )?;
    }
    let state = if actions
        .iter()
        .any(|a| a.action == "refused-not-a-regular-file")
    {
        "modified".into()
    } else if dry_run {
        before.state.clone()
    } else {
        projection_status(project_root, source_root, manifest, engine_version)?.state
    };
    Ok(RefreshResult {
        harness: manifest.harness.clone(),
        state_before: before.state,
        state,
        actions,
    })
}

fn file_state<'a>(status: &'a ProjectionStatus, path: &str) -> Option<&'a str> {
    status
        .files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.state.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn hashes_normalized_text_like_node() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256(b"line\r\n"), sha256(b"line\n"));
    }

    fn temp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "aidlc-projection-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn setup() -> (PathBuf, PathBuf, ProjectionManifest) {
        let project = temp("project");
        let source = temp("source");
        fs::create_dir_all(project.join(".claude/skills/demo")).unwrap();
        fs::create_dir_all(source.join("claude")).unwrap();
        fs::write(source.join("claude/SKILL.md"), "managed v2\n").unwrap();
        (
            project,
            source,
            ProjectionManifest {
                harness: "claude".into(),
                skill_dir: ".claude/skills/demo".into(),
                files: vec![ProjectionFile {
                    source: "SKILL.md".into(),
                    target: "SKILL.md".into(),
                }],
                obsolete_files: vec![],
            },
        )
    }
    #[test]
    fn versions_compare_numeric_components() {
        assert!(compare_versions("0.10.0", "0.9.9").is_gt());
    }
    #[test]
    fn creates_missing_projection_and_stamp() {
        let (p, s, m) = setup();
        let result = refresh_projection(&p, &s, &m, "1.0.0", false, false).unwrap();
        assert_eq!(result.actions[0].action, "created");
        assert_eq!(
            fs::read_to_string(p.join(".claude/skills/demo/SKILL.md")).unwrap(),
            "managed v2\n"
        );
        assert_eq!(
            projection_status(&p, &s, &m, "1.0.0").unwrap().state,
            "current"
        );
        fs::remove_dir_all(p).unwrap();
        fs::remove_dir_all(s).unwrap();
    }
    #[test]
    fn repeated_refresh_keeps_modified_obsolete_file() {
        let (project, source, mut manifest) = setup();
        manifest.obsolete_files.push("old.md".into());
        let old = project.join(".claude/skills/demo/old.md");
        fs::write(&old, "user-authored notes").unwrap();
        for _ in 0..2 {
            let result =
                refresh_projection(&project, &source, &manifest, "1.0.0", false, false).unwrap();
            assert!(result
                .actions
                .iter()
                .any(|action| action.action == "kept-modified-obsolete"));
            assert_eq!(fs::read_to_string(&old).unwrap(), "user-authored notes");
        }
        fs::remove_dir_all(project).unwrap();
        fs::remove_dir_all(source).unwrap();
    }

    #[test]
    fn keeps_user_edits_unless_force_then_backups() {
        let (p, s, m) = setup();
        fs::write(p.join(".claude/skills/demo/SKILL.md"), "local\n").unwrap();
        let kept = refresh_projection(&p, &s, &m, "1.0.0", false, false).unwrap();
        assert_eq!(kept.actions[0].action, "kept-modified");
        let replaced = refresh_projection(&p, &s, &m, "1.0.0", false, true).unwrap();
        assert_eq!(replaced.actions[0].action, "replaced-with-backup");
        assert_eq!(
            fs::read_to_string(p.join(".claude/skills/demo/SKILL.md.local-backup")).unwrap(),
            "local\n"
        );
        fs::remove_dir_all(p).unwrap();
        fs::remove_dir_all(s).unwrap();
    }
    #[test]
    fn refuses_symlink_targets() {
        #[cfg(unix)]
        {
            let (p, s, m) = setup();
            fs::remove_file(p.join(".claude/skills/demo/SKILL.md")).unwrap();
            std::os::unix::fs::symlink(
                s.join("claude/SKILL.md"),
                p.join(".claude/skills/demo/SKILL.md"),
            )
            .unwrap();
            assert!(projection_status(&p, &s, &m, "1.0.0").is_err());
            fs::remove_dir_all(p).unwrap();
            fs::remove_dir_all(s).unwrap();
        }
    }
}
