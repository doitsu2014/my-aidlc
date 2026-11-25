//! Native onboarding and harness-projection primitives.
//!
//! This module deliberately has no project-wide side effects on load. Callers
//! choose the project and source roots explicitly. Unsupported or unsafe paths
//! are rejected rather than followed.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

pub mod projection;

const MAX_DEPTH: usize = 6;
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "coverage",
    ".aidlc",
    ".claude",
    ".dsh",
    "vendor",
    "target",
    "out",
    "obj",
    "venv",
    "virtualenv",
    "__pycache__",
    "site-packages",
    "bower_components",
    "Pods",
    "third_party",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub kind: String,
    pub source_files: usize,
    pub languages: BTreeMap<String, Vec<String>>,
    pub architecture: BTreeMap<String, Vec<String>>,
    pub skipped: BTreeMap<String, usize>,
}

fn language_for_ext(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rs" => "rust",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "dart" => "dart",
        "vue" => "vue",
        "svelte" => "svelte",
        "scala" => "scala",
        "sql" => "sql",
        "sh" | "bash" => "shell",
        "ex" | "exs" => "elixir",
        "tf" => "terraform",
        _ => return None,
    })
}

fn rel_posix(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn walk_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<FileEntry>,
    skipped: &mut BTreeMap<String, usize>,
) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if dir == root => return Err(e),
        Err(_) => {
            *skipped.entry("unreadable".into()).or_default() += 1;
            return Ok(());
        }
    };
    for item in entries {
        let item = match item {
            Ok(v) => v,
            Err(_) => {
                *skipped.entry("unreadable".into()).or_default() += 1;
                continue;
            }
        };
        let name = item.file_name().to_string_lossy().into_owned();
        let path = item.path();
        let ft = match item.file_type() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if name.starts_with('.') {
            *skipped.entry("hidden".into()).or_default() += 1;
            continue;
        }
        if ft.is_symlink() {
            *skipped.entry("symlink".into()).or_default() += 1;
            continue;
        }
        if ft.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || path.join("pyvenv.cfg").exists() {
                *skipped.entry("vendored-or-generated".into()).or_default() += 1;
                continue;
            }
            if depth >= MAX_DEPTH {
                *skipped.entry("depth-limit".into()).or_default() += 1;
                continue;
            }
            walk_files(root, &path, depth + 1, out, skipped)?;
        } else if ft.is_file() {
            let bytes = item.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(FileEntry {
                path: rel_posix(root, &path),
                bytes,
            });
        }
    }
    Ok(())
}

pub fn list_files(root: &Path, relative: &str) -> io::Result<Vec<FileEntry>> {
    let base = safe_join(root, relative)?;
    let meta = match fs::symlink_metadata(&base) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "symlink path refused",
        ));
    }
    if meta.is_file() {
        return Ok(vec![FileEntry {
            path: rel_posix(root, &base),
            bytes: meta.len(),
        }]);
    }
    if !meta.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    walk_files(root, &base, 0, &mut out, &mut BTreeMap::new())?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub fn scan_project(root: &Path) -> io::Result<ScanResult> {
    let root = fs::canonicalize(root)?;
    let mut files = Vec::new();
    let mut skipped = BTreeMap::new();
    walk_files(&root, &root, 0, &mut files, &mut skipped)?;
    let mut languages: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut architecture: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut source_files = 0;
    for file in &files {
        let p = Path::new(&file.path);
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        if let Some(ext) = p
            .extension()
            .and_then(|e| e.to_str())
            .and_then(language_for_ext)
        {
            source_files += 1;
            let row = languages.entry(ext.into()).or_default();
            if row.len() < 8 && !row.contains(&file.path) {
                row.push(file.path.clone());
            }
        }
        if name.ends_with(".csproj") {
            languages
                .entry("csharp".into())
                .or_default()
                .push(file.path.clone());
        }
        if ["Dockerfile", "docker-compose.yml", "compose.yaml"].contains(&name.as_ref()) {
            architecture
                .entry("containers".into())
                .or_default()
                .push(file.path.clone());
        }
        if name == "pnpm-workspace.yaml" || name == "turbo.json" {
            architecture
                .entry("monorepo".into())
                .or_default()
                .push(file.path.clone());
        }
    }
    for (file, lang) in [
        ("package.json", "javascript"),
        ("tsconfig.json", "typescript"),
        ("go.mod", "go"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("Cargo.toml", "rust"),
        ("composer.json", "php"),
        ("Gemfile", "ruby"),
        ("pubspec.yaml", "dart"),
        ("mix.exs", "elixir"),
        ("build.sbt", "scala"),
    ] {
        if root.join(file).is_file() {
            let evidence = languages.entry(lang.into()).or_default();
            if evidence.len() < 8 && !evidence.contains(&file.to_string()) {
                evidence.push(file.into());
            }
        }
    }
    if root.join("apps").is_dir() && root.join("packages").is_dir() {
        architecture
            .entry("monorepo".into())
            .or_default()
            .push("apps/ and packages/".into());
    }
    for rows in languages.values_mut().chain(architecture.values_mut()) {
        rows.sort();
        rows.dedup();
    }
    let kind = if source_files > 0 || !languages.is_empty() {
        "brownfield"
    } else {
        "greenfield"
    };
    Ok(ScanResult {
        kind: kind.into(),
        source_files,
        languages,
        architecture,
        skipped,
    })
}

/// Normalize a project-relative source path, rejecting absolute paths and traversal.
pub fn normalize_source_path(value: &str) -> io::Result<String> {
    let normalized = value.replace('\\', "/");
    let trimmed = normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .trim_end_matches('/');
    let path = Path::new(trimmed);
    if trimmed.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe project-relative path",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Join a normalized project-relative path after checking every existing component for symlinks.
pub fn safe_join(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let relative = normalize_source_path(relative)?;
    let root = fs::canonicalize(root)?;
    let mut current = root.clone();
    for component in Path::new(&relative).components() {
        let Component::Normal(part) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsafe path component",
            ));
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "symlink path refused",
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    if !current.starts_with(&root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path escapes project root",
        ));
    }
    Ok(current)
}

/// Atomic replacement for generated files; parent directories are created only after path validation.
pub fn write_generated(root: &Path, relative: &str, bytes: &[u8]) -> io::Result<()> {
    let path = safe_join(root, relative)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
    fs::create_dir_all(parent)?;
    // Recheck after directory creation to catch pre-existing symlink components.
    let path = safe_join(root, relative)?;
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("generated")
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn scan_to_json(scan: &ScanResult) -> Value {
    let languages: Vec<Value> = scan
        .languages
        .iter()
        .map(|(name, evidence)| json!({"name": name, "evidence": evidence}))
        .collect();
    let architecture: Vec<Value> = scan
        .architecture
        .iter()
        .map(|(name, evidence)| json!({"name": name, "evidence": evidence}))
        .collect();
    json!({"kind":scan.kind,"sourceFiles":scan.source_files,"languages":languages,"architecture":architecture,"skipped":{"counts":scan.skipped}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!(
            "aidlc-onboard-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn rejects_traversal_and_absolute_paths() {
        assert!(normalize_source_path("../escape").is_err());
        assert!(normalize_source_path("C:/escape").is_err());
        assert_eq!(normalize_source_path("./docs/").unwrap(), "docs");
    }
    #[test]
    fn scans_languages_and_omits_generated_directories() {
        let root = temp();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("target/ignored.rs"), "").unwrap();
        let scan = scan_project(&root).unwrap();
        assert_eq!(scan.kind, "brownfield");
        assert_eq!(scan.source_files, 1);
        assert!(scan.languages.contains_key("rust"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn generated_write_rejects_symlink_components() {
        let root = temp();
        fs::create_dir_all(&root).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), root.join("link")).unwrap();
            assert!(write_generated(&root, "link/out.json", b"{}").is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
