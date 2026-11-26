//! Safe projection of build-time embedded methodology assets to an engine directory.
//!
//! The orchestration directive's Node-facing file paths point at files under an
//! engine root (normally `core/`). Installed binaries have no source-tree `core/`,
//! so callers can pass embedded `(engine-relative-name, bytes)` pairs here and use
//! the returned root when constructing those paths.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Materialize methodology assets under `engine_root` and return its canonical path.
///
/// Only `methodology/...` assets are written. Existing byte-identical regular files
/// are accepted (making repeated calls idempotent); differing files, symlinks, and
/// non-file targets are rejected rather than overwritten. The caller should pass
/// names from the embedded asset table, not paths supplied by a project or user.
pub fn materialize_methodology_assets<'a, I>(
    engine_root: &Path,
    assets: I,
) -> Result<PathBuf, String>
where
    I: IntoIterator<Item = (&'a str, &'a [u8])>,
{
    ensure_directory_chain(engine_root)?;
    let root =
        fs::canonicalize(engine_root).map_err(|e| format!("{}: {e}", engine_root.display()))?;
    if !fs::symlink_metadata(&root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .is_dir()
    {
        return Err("Engine root is not a directory".into());
    }

    for (name, bytes) in assets {
        let relative = safe_methodology_path(name)?;
        let target = root.join(&relative);
        ensure_directory_chain(target.parent().ok_or("Invalid methodology asset path")?)?;
        match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(format!(
                        "Refusing non-regular methodology asset target: {}",
                        target.display()
                    ));
                }
                let existing =
                    fs::read(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                if existing != bytes {
                    return Err(format!(
                        "Refusing to replace existing methodology asset: {}",
                        target.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                write_new_file(&target, bytes)?;
            }
            Err(error) => return Err(format!("{}: {error}", target.display())),
        }
    }
    Ok(root)
}

fn safe_methodology_path(name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if !name.starts_with("methodology/")
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || path.components().count() < 2
    {
        return Err(format!("Invalid embedded methodology asset name: {name}"));
    }
    Ok(path.to_path_buf())
}

/// Check every existing component without following a symlink, then create the
/// missing directory components one at a time and check them again.
fn ensure_directory_chain(path: &Path) -> Result<(), String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let mut current = PathBuf::new();
    for component in absolute.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "Refusing symlink or non-directory path: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|e| format!("{}: {e}", current.display()))?;
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|e| format!("{}: {e}", current.display()))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "Refusing symlink or non-directory path: {}",
                        current.display()
                    ));
                }
            }
            Err(error) => return Err(format!("{}: {error}", current.display())),
        }
    }
    Ok(())
}

fn write_new_file(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = target.parent().ok_or("Invalid methodology asset target")?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".aidlc-asset-{}-{sequence}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("{}: {e}", temp.display()))?;
        file.write_all(bytes)
            .map_err(|e| format!("{}: {e}", temp.display()))?;
        file.sync_all()
            .map_err(|e| format!("{}: {e}", temp.display()))?;
        match fs::hard_link(&temp, target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
                "Refusing to replace concurrently-created methodology asset: {}",
                target.display()
            )),
            Err(error) => Err(format!("{}: {error}", target.display())),
        }
    })();
    let _ = fs::remove_file(&temp);
    result
}
