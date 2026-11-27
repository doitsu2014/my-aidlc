//! Native `onboard` / `init` (including `--rescan`), ported from the `onboard` branch
//! of Node `core/tools/dtech-aidlc.mjs` and `core/tools/onboard.mjs` (`scanProject`,
//! `detectDocumentation`, `planRegistration`, `writeProjectProfile`,
//! `writeSourcesSnapshot`).
//!
//! Harness projection uses `crate::harness_commands` (bundled manifests, projection
//! stamp and status), knowledge uses `crate::knowledge`, and audit rows go through
//! `crate::audit`. Output and generated files carry Node's shapes and key order.
//!
//! Safety differences from Node, all of which leave the project unchanged on failure
//! where Node could leave a partial write: the methodology graph, every harness
//! manifest, the skill directories, the knowledge manifest, the generated-file paths
//! and the audit log are checked before the first write; symbolic links in the paths
//! read or written are refused rather than followed.

use crate::harness_commands::{
    configured_harnesses, harness_choice_error, harness_manifest, list_harnesses,
    project_harness_files, projection_status, update_projection_stamp, ENGINE_VERSION,
};
use serde_json::{json, Map, Value};
use std::cmp::Ordering;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

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
const DOC_EXT: &[&str] = &[
    ".md",
    ".markdown",
    ".mdx",
    ".rst",
    ".adoc",
    ".txt",
    ".pdf",
    ".docx",
    ".doc",
    ".odt",
    ".html",
    ".htm",
    ".xlsx",
    ".pptx",
];
const NEUTRAL_EXT: &[&str] = &[
    ".json", ".yaml", ".yml", ".toml", ".xml", ".ini", ".cfg", ".conf", ".lock", ".env", ".csv",
    ".tsv", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".ico", ".webp", ".log",
];
const TEXT_EXT: &[&str] = &[".md", ".markdown", ".rst", ".txt", ".adoc"];
const BINARY_EXT: &[&str] = &[".pdf", ".doc", ".docx", ".xlsx", ".pptx"];
const MAX_DOC_NOTE: u64 = 2 * 1024 * 1024;
const HASH_LIMIT: u64 = 20 * 1024 * 1024;
const SNAPSHOT_CAP: usize = 2000;
const SKIP_REASONS: &[&str] = &[
    "hidden",
    "vendored-or-generated",
    "symlink",
    "depth-limit",
    "unreadable",
];
const MANIFEST_LANGUAGES: &[(&str, &str)] = &[
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
];
const SCOPES: &[&str] = &["solo", "team", "enterprise"];
const KNOWLEDGE_MANIFEST: &str = "aidlc.knowledge.json";
const NOT_INITIALIZED: &str =
    "Project is not initialized; run dtech-aidlc onboard --harness <claude|deepseek> first";
const GENERATED: &[&str] = &[
    "project-profile.json",
    "project-profile.md",
    "sources-snapshot.json",
];

/// Node `LANGUAGE_BY_EXT` (the extension includes its dot and is lower case).
fn language_for_ext(ext: &str) -> Option<&'static str> {
    Some(match ext {
        ".js" | ".jsx" | ".mjs" | ".cjs" => "javascript",
        ".ts" | ".tsx" => "typescript",
        ".py" => "python",
        ".go" => "go",
        ".java" => "java",
        ".kt" | ".kts" => "kotlin",
        ".rs" => "rust",
        ".rb" => "ruby",
        ".php" => "php",
        ".cs" => "csharp",
        ".swift" => "swift",
        ".c" | ".h" => "c",
        ".cpp" | ".cc" | ".hpp" => "cpp",
        ".dart" => "dart",
        ".vue" => "vue",
        ".svelte" => "svelte",
        ".scala" => "scala",
        ".sql" => "sql",
        ".sh" | ".bash" => "shell",
        ".ex" | ".exs" => "elixir",
        ".tf" => "terraform",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Node `path.extname` of a file name: from the last dot, or `""` when the name has
/// no dot, starts with its only dot, or is `..`.
fn extname(name: &str) -> &str {
    match name.rfind('.') {
        Some(0) | None => "",
        Some(_) if name == ".." => "",
        Some(index) => &name[index..],
    }
}

/// Node `stem(name)`.
fn stem(name: &str) -> &str {
    let ext = extname(name);
    &name[..name.len() - ext.len()]
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Node `toPosix(relative(resolve(root), abs))` for a path built under `root`.
fn posix_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Node `toPosix(path)`: the platform separator becomes `/`.
fn to_posix(path: &str) -> String {
    if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    }
}

/// Node `normalizeSourcePath`: POSIX separators, no leading `./`, no trailing `/`.
fn node_normalize(path: &str) -> String {
    let posix = to_posix(path);
    let trimmed = posix.strip_prefix("./").unwrap_or(&posix);
    trimmed.strip_suffix('/').unwrap_or(trimmed).to_owned()
}

/// `join(root, ...rel.split('/'))`.
fn join_rel(root: &Path, relative: &str) -> PathBuf {
    relative.split('/').filter(|part| !part.is_empty()).fold(
        root.to_path_buf(),
        |mut path, part| {
            path.push(part);
            path
        },
    )
}

/// JavaScript `<` on strings: UTF-16 code unit order.
fn js_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Node `Stats.mtimeMs` (`sec * 1e3 + nsec / 1e6`) as a JSON number; an integral value
/// is written without a fraction, as `JSON.stringify` does.
fn mtime_json(meta: &fs::Metadata) -> Value {
    let ms = match meta.modified().map(|time| time.duration_since(UNIX_EPOCH)) {
        Ok(Ok(after)) => after.as_secs() as f64 * 1e3 + f64::from(after.subsec_nanos()) / 1e6,
        Ok(Err(before)) => {
            let before = before.duration();
            -(before.as_secs() as f64 * 1e3 + f64::from(before.subsec_nanos()) / 1e6)
        }
        Err(_) => return Value::Null,
    };
    if ms.fract() == 0.0 && ms.abs() < 9.0e15 {
        json!(ms as i64)
    } else {
        json!(ms)
    }
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

/// Atomic replacement for generated files (Node `writeGenerated` / `writeFileAtomic`):
/// every component is checked for symbolic links before and after the parent folders
/// are created, then the bytes go to a unique temp file that is renamed over the target.
pub fn write_generated(root: &Path, relative: &str, bytes: &[u8]) -> io::Result<()> {
    let path = safe_join(root, relative)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
    fs::create_dir_all(parent)?;
    // Recheck after directory creation to catch pre-existing symlink components.
    let path = safe_join(root, relative)?;
    crate::workflow::write_file_atomic(&path, bytes).map_err(io::Error::other)
}

// ---------------------------------------------------------------------------
// Directory walk (Node `walk`)
// ---------------------------------------------------------------------------

enum Visit<'a> {
    File(&'a Path, &'a str),
    Dir(&'a Path, &'a str),
    Skip(&'a Path, &'a str, &'static str),
}

/// `readdirSync(dir, { withFileTypes: true })`. Node lists entries in the order
/// libuv returns them: sorted by name on POSIX systems, file-system order on Windows.
fn read_entries(dir: &Path) -> io::Result<Vec<(String, PathBuf, fs::FileType)>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        entries.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
            kind,
        ));
    }
    if !cfg!(windows) {
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    }
    Ok(entries)
}

/// Node `walk(dir, handlers, depth)`: hidden entries, symbolic links, vendored or
/// generated folders and folders deeper than six levels are skipped and reported.
fn walk(dir: &Path, depth: usize, visit: &mut dyn FnMut(Visit)) {
    if !dir.exists() {
        return;
    }
    let entries = match read_entries(dir) {
        Ok(entries) => entries,
        Err(_) => {
            visit(Visit::Skip(dir, &file_name(dir), "unreadable"));
            return;
        }
    };
    for (name, path, kind) in entries {
        if name.starts_with('.') {
            visit(Visit::Skip(&path, &name, "hidden"));
            continue;
        }
        if kind.is_symlink() {
            visit(Visit::Skip(&path, &name, "symlink"));
            continue;
        }
        let vendored = SKIP_DIRS.contains(&name.as_str())
            || (kind.is_dir() && path.join("pyvenv.cfg").exists());
        if vendored {
            visit(Visit::Skip(&path, &name, "vendored-or-generated"));
            continue;
        }
        if kind.is_dir() {
            if depth + 1 > MAX_DEPTH {
                visit(Visit::Skip(&path, &name, "depth-limit"));
                continue;
            }
            visit(Visit::Dir(&path, &name));
            walk(&path, depth + 1, visit);
        } else if kind.is_file() {
            visit(Visit::File(&path, &name));
        }
    }
}

/// One file found by [`list_files`].
#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub bytes: u64,
    pub mtime_ms: Value,
}

/// Node `listFiles(root, relPath)`: the regular files at or under `relPath`, sorted.
/// A path with a symbolic link component (or one escaping the project) lists nothing.
pub fn list_files(root: &Path, relative: &str) -> Vec<FileEntry> {
    if safe_join(root, relative).is_err() {
        return Vec::new();
    }
    let abs = join_rel(root, relative);
    if !abs.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut visit_file = |path: &Path| {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.is_file() {
                out.push(FileEntry {
                    path: posix_rel(root, path),
                    bytes: meta.len(),
                    mtime_ms: mtime_json(&meta),
                });
            }
        }
    };
    match fs::symlink_metadata(&abs) {
        Ok(meta) if meta.is_file() => visit_file(&abs),
        Ok(meta) if meta.is_dir() => walk(&abs, 0, &mut |visit| {
            if let Visit::File(path, _) = visit {
                visit_file(path);
            }
        }),
        _ => {}
    }
    out.sort_by(|a, b| js_cmp(&a.path, &b.path));
    out
}

// ---------------------------------------------------------------------------
// Project scan (Node `scanProject`)
// ---------------------------------------------------------------------------

type Evidence = Vec<(String, Vec<String>)>;

/// Node `addLanguage(map, key, evidence)`: insertion-ordered, at most eight items.
fn add_evidence(map: &mut Evidence, key: &str, evidence: &str) {
    let index = match map.iter().position(|(name, _)| name == key) {
        Some(index) => index,
        None => {
            map.push((key.to_owned(), Vec::new()));
            map.len() - 1
        }
    };
    let rows = &mut map[index].1;
    let posix = to_posix(evidence);
    if rows.len() < 8 && !rows.contains(&posix) {
        rows.push(posix);
    }
}

fn evidence_json(map: &Evidence) -> Value {
    Value::Array(
        map.iter()
            .map(|(name, evidence)| json!({"name": name, "evidence": evidence}))
            .collect(),
    )
}

/// Node `scanProject(root)` as its JSON shape:
/// `{ kind, sourceFiles, unclassifiedFiles, unclassified, languages, architecture,
/// skipped: { counts, examples } }`.
pub fn scan_project(root: &Path) -> Value {
    let mut languages: Evidence = Vec::new();
    let mut architecture: Evidence = Vec::new();
    let mut counts: Vec<(String, usize)> = SKIP_REASONS
        .iter()
        .map(|reason| ((*reason).to_owned(), 0))
        .collect();
    let mut examples: Vec<Value> = Vec::new();
    let mut source_files = 0usize;
    let mut unclassified: Vec<String> = Vec::new();
    let mut unclassified_files = 0usize;
    for (file, language) in MANIFEST_LANGUAGES {
        if root.join(file).exists() {
            add_evidence(&mut languages, language, file);
        }
    }
    walk(root, 0, &mut |visit| match visit {
        Visit::File(path, name) => {
            let ext = extname(name).to_lowercase();
            let rel = posix_rel(root, path);
            let language = language_for_ext(&ext);
            if let Some(language) = language {
                source_files += 1;
                add_evidence(&mut languages, language, &rel);
            }
            let csproj = name.ends_with(".csproj");
            if csproj {
                add_evidence(&mut languages, "csharp", &rel);
            }
            if matches!(name, "Dockerfile" | "docker-compose.yml" | "compose.yaml") {
                add_evidence(&mut architecture, "containers", &rel);
            }
            if matches!(name, "pnpm-workspace.yaml" | "turbo.json") {
                add_evidence(&mut architecture, "monorepo", &rel);
            }
            if !ext.is_empty()
                && language.is_none()
                && !csproj
                && !DOC_EXT.contains(&ext.as_str())
                && !NEUTRAL_EXT.contains(&ext.as_str())
            {
                unclassified_files += 1;
                if unclassified.len() < 8 {
                    unclassified.push(rel);
                }
            }
        }
        Visit::Dir(..) => {}
        Visit::Skip(path, name, reason) => {
            match counts.iter_mut().find(|(key, _)| key == reason) {
                Some((_, count)) => *count += 1,
                None => counts.push((reason.to_owned(), 1)),
            }
            if reason != "hidden" && examples.len() < 20 {
                let rel = posix_rel(root, path);
                let shown = if rel.is_empty() { name.to_owned() } else { rel };
                examples.push(json!({"path": shown, "reason": reason}));
            }
        }
    });
    if root.join("apps").exists() && root.join("packages").exists() {
        add_evidence(&mut architecture, "monorepo", "apps/ and packages/");
    }
    let kind = if source_files > 0 || !languages.is_empty() {
        "brownfield"
    } else if unclassified_files > 0 {
        "unknown"
    } else {
        "greenfield"
    };
    let counts: Map<String, Value> = counts
        .into_iter()
        .map(|(reason, count)| (reason, json!(count)))
        .collect();
    json!({
        "kind": kind,
        "sourceFiles": source_files,
        "unclassifiedFiles": unclassified_files,
        "unclassified": unclassified,
        "languages": evidence_json(&languages),
        "architecture": evidence_json(&architecture),
        "skipped": {"counts": counts, "examples": examples},
    })
}

// ---------------------------------------------------------------------------
// Documentation detection (Node `detectDocumentation`)
// ---------------------------------------------------------------------------

fn is_root_text_name(name: &str, base: &str) -> bool {
    let lower = name.to_lowercase();
    let wanted = base.to_lowercase();
    lower == wanted || (TEXT_EXT.contains(&extname(&lower)) && stem(&lower) == wanted)
}

fn open_api_kind(name: &str) -> bool {
    let lower = name.to_lowercase();
    if !matches!(extname(&lower), ".yaml" | ".yml" | ".json") {
        return false;
    }
    let base = stem(&lower);
    base == "openapi"
        || base == "swagger"
        || ["openapi.", "openapi-", "swagger.", "swagger-"]
            .iter()
            .any(|prefix| base.starts_with(prefix))
        || base.ends_with(".openapi")
        || base.ends_with(".swagger")
}

/// `/^(requirements([-_].*)?|constraints)\.(txt|in)$/i`
fn pip_requirements(name: &str) -> bool {
    let lower = name.to_lowercase();
    let Some(base) = lower
        .strip_suffix(".txt")
        .or_else(|| lower.strip_suffix(".in"))
    else {
        return false;
    };
    base == "constraints"
        || base == "requirements"
        || base.starts_with("requirements-")
        || base.starts_with("requirements_")
}

/// `/(^|[^a-z0-9])(brd|prd|business[-_]?requirements?)([^a-z0-9]|$)/i`
fn requirements_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let alnum = |index: usize| bytes.get(index).is_some_and(u8::is_ascii_alphanumeric);
    for start in 0..bytes.len() {
        if start > 0 && alnum(start - 1) {
            continue;
        }
        let rest = &lower[start..];
        let mut ends = Vec::new();
        for word in ["brd", "prd"] {
            if rest.starts_with(word) {
                ends.push(start + word.len());
            }
        }
        if let Some(after) = rest.strip_prefix("business") {
            let after = after
                .strip_prefix('-')
                .or_else(|| after.strip_prefix('_'))
                .unwrap_or(after);
            if let Some(tail) = after.strip_prefix("requirement") {
                let end = lower.len() - tail.len();
                ends.push(end);
                if tail.starts_with('s') {
                    ends.push(end + 1);
                }
            }
        }
        if ends.into_iter().any(|end| !alnum(end)) {
            return true;
        }
    }
    false
}

fn requirements_kind(name: &str) -> bool {
    !pip_requirements(name) && requirements_name(name)
}

fn runbook_kind(name: &str) -> bool {
    name.to_lowercase().contains("runbook")
}

/// `isRootTextName(name, 'architecture') || /^architecture\b/i.test(stem(name))`
fn architecture_file(name: &str) -> bool {
    if is_root_text_name(name, "architecture") {
        return true;
    }
    let base = stem(name).to_ascii_lowercase();
    base.strip_prefix("architecture").is_some_and(|rest| {
        !rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Node `notesFor(path, bytes)`.
fn notes_for(path: &str, bytes: u64) -> Vec<String> {
    let mut notes = Vec::new();
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = extname(name).to_lowercase();
    if BINARY_EXT.contains(&ext.as_str()) {
        notes.push(format!("binary format ({ext})"));
    }
    if bytes > MAX_DOC_NOTE {
        notes.push(format!(
            "large file ({:.1} MB)",
            bytes as f64 / (1024.0 * 1024.0)
        ));
    }
    notes
}

/// Node `candidate(root, rel, kind, origin)`.
fn candidate(root: &Path, rel: &str, kind: &str, origin: &str) -> Value {
    let files = list_files(root, rel);
    let bytes: u64 = files.iter().map(|file| file.bytes).sum();
    let abs = join_rel(root, rel);
    let kind_of_path = if abs.exists() {
        match fs::symlink_metadata(&abs) {
            Ok(meta) if meta.is_dir() => json!("directory"),
            _ => json!("file"),
        }
    } else {
        json!(false)
    };
    let notes = if files.len() == 1 {
        notes_for(rel, bytes)
    } else {
        Vec::new()
    };
    json!({
        "path": rel,
        "type": kind_of_path,
        "kind": kind,
        "origin": origin,
        "files": files.len(),
        "bytes": bytes,
        "notes": notes,
    })
}

/// Node `detectDocumentation(root, { explicit, detect })`: the candidates (sorted by
/// path) and the skipped entries.
pub fn detect_documentation(
    root: &Path,
    explicit: &[String],
    detect: bool,
) -> (Vec<Value>, Vec<Value>) {
    let mut skipped: Vec<Value> = Vec::new();
    let mut by_path: Vec<Value> = Vec::new();
    let add = |by_path: &mut Vec<Value>, rel: &str, kind: &str, origin: &str| {
        if rel.is_empty() {
            return;
        }
        if let Some(existing) = by_path.iter_mut().find(|row| row["path"] == rel) {
            if origin == "explicit" {
                existing["origin"] = json!("explicit");
            }
            return;
        }
        by_path.push(candidate(root, rel, kind, origin));
    };
    if detect {
        for (name, _, kind) in read_entries(root).unwrap_or_default() {
            if name.starts_with('.') {
                skipped.push(json!({"path": name, "reason": "hidden"}));
                continue;
            }
            if kind.is_symlink() {
                skipped.push(json!({"path": name, "reason": "symlink"}));
                continue;
            }
            if kind.is_dir() {
                let lower = name.to_lowercase();
                if matches!(lower.as_str(), "docs" | "doc" | "documentation") {
                    add(&mut by_path, &name, "docs", "detected");
                }
                if matches!(lower.as_str(), "adr" | "adrs") {
                    add(&mut by_path, &name, "adr", "detected");
                }
            } else if kind.is_file() {
                if is_root_text_name(&name, "README") {
                    add(&mut by_path, &name, "readme", "detected");
                }
                if is_root_text_name(&name, "CONTRIBUTING") {
                    add(&mut by_path, &name, "conventions", "detected");
                }
                if is_root_text_name(&name, "ARCHITECTURE") {
                    add(&mut by_path, &name, "architecture", "detected");
                }
            }
        }
        walk(root, 0, &mut |visit| match visit {
            Visit::File(path, name) => {
                let rel = posix_rel(root, path);
                if open_api_kind(name) {
                    add(&mut by_path, &rel, "api-spec", "detected");
                }
                if requirements_kind(name) {
                    add(&mut by_path, &rel, "requirements", "detected");
                }
                if runbook_kind(name) {
                    add(&mut by_path, &rel, "runbook", "detected");
                }
                if architecture_file(name) && rel.contains('/') {
                    add(&mut by_path, &rel, "architecture", "detected");
                }
            }
            Visit::Dir(path, name) => {
                let rel = posix_rel(root, path);
                if matches!(name.to_lowercase().as_str(), "adr" | "adrs") && rel.contains('/') {
                    add(&mut by_path, &rel, "adr", "detected");
                }
            }
            Visit::Skip(path, name, reason) => {
                let rel = posix_rel(root, path);
                let shown = if rel.is_empty() { name.to_owned() } else { rel };
                skipped.push(json!({"path": shown, "reason": reason}));
            }
        });
    }
    for item in explicit {
        let rel = node_normalize(item);
        add(&mut by_path, &rel, "docs", "explicit");
    }
    by_path.sort_by(|a, b| {
        js_cmp(
            a["path"].as_str().unwrap_or(""),
            b["path"].as_str().unwrap_or(""),
        )
    });
    (by_path, skipped)
}

// ---------------------------------------------------------------------------
// Knowledge registration (Node `planRegistration`)
// ---------------------------------------------------------------------------

/// Node `registerCommand(paths)`.
fn register_command(paths: &[&str]) -> String {
    let mut command = "dtech-aidlc onboard --rescan".to_owned();
    for path in paths {
        let plain = path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
        if plain {
            command.push_str(&format!(" --docs {path}"));
        } else {
            command.push_str(&format!(" --docs \"{path}\""));
        }
    }
    command
}

/// `path.replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '') || 'docs'`
fn source_id(path: &str) -> String {
    let mut id = String::new();
    let mut in_run = false;
    for c in path.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            id.push(c);
            in_run = false;
        } else if !in_run {
            id.push('-');
            in_run = true;
        }
    }
    let id = id.trim_matches('-');
    if id.is_empty() {
        "docs".to_owned()
    } else {
        id.to_owned()
    }
}

struct Plan {
    manifest: Value,
    status: &'static str,
    added: Vec<String>,
    would_add: Vec<Value>,
    already_registered: Vec<String>,
}

/// Node `planRegistration(existingManifest, defaultManifest, candidates)`.
fn plan_registration(existing: Option<&Value>, default: &Value, candidates: &[Value]) -> Plan {
    let sources_of = |manifest: &Value| -> Vec<Value> {
        manifest["sources"].as_array().cloned().unwrap_or_default()
    };
    let path_of = |source: &Value| node_normalize(source["path"].as_str().unwrap_or(""));
    let mut registered: Vec<String> = sources_of(existing.unwrap_or(default))
        .iter()
        .map(path_of)
        .collect();
    let mut sources = sources_of(existing.unwrap_or(default));
    let mut added = Vec::new();
    let mut would_add = Vec::new();
    let mut already_registered = Vec::new();
    let mut eligible: Vec<usize> = Vec::new();
    for (index, item) in candidates.iter().enumerate() {
        let path = path_of(item);
        if registered.contains(&path) || sources.iter().any(|source| path_of(source) == path) {
            already_registered.push(path);
            continue;
        }
        if existing.is_some() && item["origin"] != "explicit" {
            would_add.push(json!({
                "path": path,
                "kind": item["kind"],
                "command": register_command(&[&path]),
            }));
            continue;
        }
        eligible.push(index);
    }
    let capacity = 50usize.saturating_sub(sources.len());
    let (explicit, detected): (Vec<usize>, Vec<usize>) = eligible
        .into_iter()
        .partition(|index| candidates[*index]["origin"] == "explicit");
    let mut keep = Vec::new();
    for index in explicit.into_iter().chain(detected) {
        if keep.len() >= capacity {
            let item = &candidates[index];
            would_add.push(json!({
                "path": path_of(item),
                "kind": item["kind"],
                "command": register_command(&[item["path"].as_str().unwrap_or("")]),
                "truncated": true,
            }));
        } else {
            keep.push(index);
        }
    }
    keep.sort_unstable();
    for index in keep {
        let item = &candidates[index];
        let path = path_of(item);
        let id = source_id(&path);
        let mut unique = id.clone();
        let mut n = 2;
        while sources.iter().any(|source| source["id"] == unique.as_str()) {
            unique = format!("{id}-{n}");
            n += 1;
        }
        let description = if item["origin"] == "explicit" {
            "Added with --docs.".to_owned()
        } else {
            format!("Detected {} source.", item["kind"].as_str().unwrap_or(""))
        };
        sources.push(
            json!({"id": unique, "path": path, "kind": item["kind"], "description": description}),
        );
        added.push(unique);
        registered.push(path);
    }
    let status = match (existing.is_some(), added.is_empty()) {
        (false, false) => "created",
        (false, true) => "absent",
        (true, false) => "updated",
        (true, true) => "unchanged",
    };
    Plan {
        manifest: json!({"schemaVersion": 1, "sources": sources}),
        status,
        added,
        would_add,
        already_registered,
    }
}

// ---------------------------------------------------------------------------
// Generated files (Node `writeProjectProfile`, `writeSourcesSnapshot`)
// ---------------------------------------------------------------------------

fn generated_path(name: &str) -> String {
    format!(".aidlc/shared/{name}")
}

/// Node `writeProjectProfile(root, scan, documentation)`: writes
/// `.aidlc/shared/project-profile.json` and `.md` and returns the profile.
fn write_project_profile(
    root: &Path,
    scan: &Value,
    candidates: &[Value],
    doc_skipped: &[Value],
) -> Result<Value, String> {
    let brd = root.join(".aidlc").join("shared").join("brd");
    let legacy = match fs::symlink_metadata(&brd) {
        Ok(meta) if brd.exists() && meta.is_dir() => fs::read_dir(&brd)
            .map(|entries| entries.count())
            .ok()
            .filter(|files| *files > 0)
            .map(|files| json!({"brdCopies": {"path": ".aidlc/shared/brd", "files": files}})),
        _ => None,
    };
    let mut profile = json!({
        "schemaVersion": 2,
        "generated": true,
        "generatedBy": "dtech-aidlc onboard",
        "scannedAt": crate::audit::now_iso(),
        "kind": scan["kind"],
        "sourceFiles": scan["sourceFiles"],
        "unclassifiedFiles": scan["unclassifiedFiles"],
        "unclassified": scan["unclassified"],
        "languages": scan["languages"],
        "architecture": scan["architecture"],
        "documentation": {
            "manifest": KNOWLEDGE_MANIFEST,
            "detected": candidates,
            "skipped": doc_skipped,
            "truncated": false,
        },
        "skipped": scan["skipped"],
    });
    if let Some(legacy) = &legacy {
        profile["legacy"] = legacy.clone();
    }
    let mut text = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    text.push('\n');
    write_generated(
        root,
        &generated_path("project-profile.json"),
        text.as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let string = |value: &Value| match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let evidence_lines = |rows: &Value| -> Vec<String> {
        let rows = rows.as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return vec!["- none detected".to_owned()];
        }
        rows.iter()
            .map(|row| {
                let evidence = row["evidence"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("- {}: {evidence}", string(&row["name"]))
            })
            .collect()
    };
    let doc_lines: Vec<String> = if candidates.is_empty() {
        vec!["- none detected".to_owned()]
    } else {
        candidates
            .iter()
            .map(|row| {
                let notes: Vec<String> = row["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(string)
                    .collect();
                let notes = if notes.is_empty() {
                    String::new()
                } else {
                    format!("; {}", notes.join("; "))
                };
                format!(
                    "- {} ({}, {} files, {} bytes){notes}",
                    string(&row["path"]),
                    string(&row["kind"]),
                    string(&row["files"]),
                    string(&row["bytes"])
                )
            })
            .collect()
    };
    let counts = &scan["skipped"]["counts"];
    let count = |key: &str| counts.get(key).map(string).unwrap_or_else(|| "0".into());
    let kind = string(&scan["kind"]);
    let kind_note = if kind == "unknown" {
        let examples = scan["unclassified"]
            .as_array()
            .into_iter()
            .flatten()
            .take(3)
            .map(string)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            " ({} files of unrecognized types, for example {examples}). Treated as an existing project.",
            string(&scan["unclassifiedFiles"])
        )
    } else {
        String::new()
    };
    let mut lines = vec![
        "# Project profile".to_owned(),
        String::new(),
        "> Generated by `dtech-aidlc onboard`. Do not edit: this file is rewritten on every scan. Put project knowledge in PROJECT.md and documentation sources in aidlc.knowledge.json.".to_owned(),
        String::new(),
        format!("Kind: {kind}{kind_note}"),
        format!("Source files: {}", string(&scan["sourceFiles"])),
        String::new(),
        "## Languages".to_owned(),
    ];
    lines.extend(evidence_lines(&scan["languages"]));
    lines.push(String::new());
    lines.push("## Architecture".to_owned());
    lines.extend(evidence_lines(&scan["architecture"]));
    lines.push(String::new());
    lines.push("## Documentation".to_owned());
    lines.extend(doc_lines);
    lines.push(String::new());
    lines.push("## Skipped".to_owned());
    lines.extend(
        doc_skipped
            .iter()
            .map(|row| format!("- {}: {}", string(&row["path"]), string(&row["reason"]))),
    );
    lines.push(format!(
        "Counts: hidden {}, vendored-or-generated {}, symlink {}, depth-limit {}, unreadable {}",
        count("hidden"),
        count("vendored-or-generated"),
        count("symlink"),
        count("depth-limit"),
        count("unreadable")
    ));
    lines.push(String::new());
    if let Some(legacy) = &legacy {
        lines.push("## Legacy".to_owned());
        lines.push(format!(
            "BRD copies from an earlier dtech-aidlc version are in .aidlc/shared/brd ({} files). They are no longer updated or read; documents are now read in place. You may delete the folder.",
            string(&legacy["brdCopies"]["files"])
        ));
        lines.push(String::new());
    }
    write_generated(
        root,
        &generated_path("project-profile.md"),
        lines.join("\n").as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(profile)
}

/// Node `hashFile(root, rel, bytes)`: `null` above 20 MB.
fn hash_file(root: &Path, relative: &str, bytes: u64) -> Result<Value, String> {
    use sha2::{Digest, Sha256};
    if bytes > HASH_LIMIT {
        return Ok(Value::Null);
    }
    let path = join_rel(root, relative);
    let contents = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!(format!("{:x}", Sha256::digest(&contents))))
}

/// Node `writeSourcesSnapshot(root, sources)`.
fn write_sources_snapshot(root: &Path, sources: &[Value]) -> Result<(), String> {
    let mut ordered: Vec<&Value> = sources.iter().collect();
    // Node sorts by `String(path).length` (UTF-16 units), longest first; the sort is stable.
    ordered.sort_by(|a, b| {
        let length = |source: &Value| source["path"].as_str().unwrap_or("").encode_utf16().count();
        length(b).cmp(&length(a))
    });
    let mut files: Vec<(FileEntry, Value)> = Vec::new();
    for source in ordered {
        let Some(path) = source["path"].as_str() else {
            continue;
        };
        for file in list_files(root, path) {
            if !files.iter().any(|(known, _)| known.path == file.path) {
                files.push((file, source["id"].clone()));
            }
        }
    }
    files.sort_by(|a, b| js_cmp(&a.0.path, &b.0.path));
    let truncated = files.len() > SNAPSHOT_CAP;
    files.truncate(SNAPSHOT_CAP);
    let mut rows = Vec::new();
    for (file, source) in files {
        rows.push(json!({
            "source": source,
            "path": file.path,
            "bytes": file.bytes,
            "mtimeMs": file.mtime_ms,
            "sha256": hash_file(root, &file.path, file.bytes)?,
        }));
    }
    let snapshot = json!({
        "schemaVersion": 1,
        "generated": true,
        "scannedAt": crate::audit::now_iso(),
        "truncated": truncated,
        "files": rows,
    });
    let mut text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    text.push('\n');
    write_generated(
        root,
        &generated_path("sources-snapshot.json"),
        text.as_bytes(),
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Options for `onboard` / `init` without `--rescan`. Empty `harness` / `scope` mean
/// the flag was not given.
#[derive(Debug, Clone)]
pub struct OnboardOptions {
    pub harness: String,
    pub scope: String,
    pub explicit_docs: Vec<String>,
    pub detect_docs: bool,
}

/// The engine named by `DTECH_AIDLC_HOME` (Node `engineRoot()`), if any.
fn external_engine() -> Option<PathBuf> {
    let home = std::env::var_os("DTECH_AIDLC_HOME").filter(|home| !home.is_empty())?;
    let home = crate::orchestrate::resolve_like_node(Path::new(&home)).ok()?;
    [home.join("core"), home].into_iter().find(|candidate| {
        candidate.join("data/phase-graph.json").exists() || candidate.join("methodology").exists()
    })
}

fn audit(root: &Path, event: &str, details: Value) -> Result<(), String> {
    crate::audit::audit(root, event, details).map(|_| ())
}

/// Node's `--docs` checks: each path exists (`existsSync(resolve(root, item))`) and lies
/// strictly inside the project (`ensureWithin`).
fn validate_docs(root: &Path, docs: &[String]) -> Result<(), String> {
    for item in docs {
        let path = crate::orchestrate::resolve_like_node(&root.join(item))?;
        if !path.exists() {
            return Err(format!("--docs path does not exist: {item}"));
        }
        if path == root || !path.starts_with(root) {
            return Err(format!("--docs path must be inside the project: {item}"));
        }
    }
    Ok(())
}

/// Checks shared by `onboard` and `onboard --rescan` before their first write: the
/// generated files and the knowledge manifest have no symbolic link component, the
/// manifest (if any) is valid, and the audit log can be appended to.
fn preflight_writes(root: &Path) -> Result<(), String> {
    for name in GENERATED {
        crate::workflow::assert_no_symlink_components(root, &generated_path(name))?;
    }
    crate::knowledge::knowledge_preflight(root)?;
    crate::audit::preflight_audit(root)
}

/// Full onboarding (Node `onboard` / `init`): writes the configuration of a new project,
/// projects every selected harness and stamps it, scaffolds `PROJECT.md`, registers
/// documentation in a new knowledge manifest, writes the project profile and sources
/// snapshot, and records audit rows. Returns Node's output object.
pub fn onboard_project(root: &Path, options: &OnboardOptions) -> io::Result<Value> {
    onboard(root, options).map_err(io::Error::other)
}

fn onboard(root: &Path, options: &OnboardOptions) -> Result<Value, String> {
    let root = crate::orchestrate::resolve_like_node(root)?;
    crate::audit::assert_safe_state_dir(&root)?;
    validate_docs(&root, &options.explicit_docs)?;
    let requested_scope = Some(options.scope.as_str()).filter(|scope| !scope.is_empty());
    let requested_harness = Some(options.harness.as_str()).filter(|name| !name.is_empty());
    if requested_scope.is_some_and(|scope| !SCOPES.contains(&scope)) {
        return Err("scope must be solo, team, or enterprise".into());
    }
    if requested_harness.is_some_and(|name| !list_harnesses().contains(&name)) {
        return Err(harness_choice_error());
    }
    let config_path = root.join(".aidlc").join("config.json");
    let has_config = config_path.exists();
    let (harnesses, scope) = if has_config {
        let existing = crate::scope::load_config_unvalidated(&root);
        let configured = configured_harnesses(&existing);
        let existing_scope = existing
            .get("scope")
            .and_then(Value::as_str)
            .filter(|scope| SCOPES.contains(scope));
        let Some(existing_scope) = existing_scope.filter(|_| !configured.is_empty()) else {
            return Err(
                "Invalid .aidlc/config.json: harness or scope is missing or unknown; correct the file"
                    .into(),
            );
        };
        let scope = requested_scope.unwrap_or(existing_scope);
        if scope != existing_scope {
            return Err(format!("Project default scope is {existing_scope}. To change the default for new intents run: dtech-aidlc config scope {scope}"));
        }
        if let Some(name) = requested_harness {
            if !configured.iter().any(|item| item == name) {
                return Err(format!(
                    "Project already uses harness {}. To add another harness run: dtech-aidlc harness add {name}",
                    configured.join(", ")
                ));
            }
        }
        let selected = match requested_harness {
            Some(name) => vec![name.to_owned()],
            None => configured,
        };
        (selected, scope.to_owned())
    } else {
        let Some(name) = requested_harness else {
            return Err(
                "--harness is required for a new project: choose claude or deepseek".into(),
            );
        };
        (
            vec![name.to_owned()],
            requested_scope.unwrap_or("solo").to_owned(),
        )
    };
    let engine = external_engine();
    let graph = crate::scope::read_graph(engine.as_deref())?;
    let graph_phases = graph.value()["phases"].as_array().map_or(0, Vec::len);
    // Validate everything before the first write.
    let manifests = harnesses
        .iter()
        .map(|name| harness_manifest(name))
        .collect::<Result<Vec<_>, _>>()?;
    let skill_dir = |manifest: &Value| manifest["skillDir"].as_str().unwrap_or("").to_owned();
    for manifest in &manifests {
        crate::workflow::assert_no_symlink_components(&root, &skill_dir(manifest))?;
    }
    preflight_writes(&root)?;

    let mut projections = Vec::new();
    let mut files_created = 0;
    for (index, (name, manifest)) in harnesses.iter().zip(&manifests).enumerate() {
        if !has_config && index == 0 {
            let config = json!({
                "harness": name,
                "harnesses": harnesses,
                "scope": scope,
                "initializedAt": crate::audit::now_iso(),
            });
            create_config(&root, &config_path, &config)?;
        }
        files_created += project_harness_files(&root, manifest)?.len();
        update_projection_stamp(&root, manifest, ENGINE_VERSION)?;
        projections.push(projection_status(&root, manifest, ENGINE_VERSION)?);
    }
    let harness = harnesses[0].clone();
    let projection_dir = skill_dir(&manifests[0]);
    let projection_root = join_rel(&root, &projection_dir);
    // Node reports `join(engineRoot(), 'methodology')`. Without an installed engine the
    // native CLI's engine is its embedded methodology, which `orchestrate next` writes
    // to `.aidlc/engine` and points directives at; onboarding does not write it.
    let engine_root = engine.clone().unwrap_or_else(|| {
        crate::orchestrate::Engine::embedded(&root)
            .root()
            .to_path_buf()
    });
    let methodology = engine_root.join("methodology");
    if !has_config {
        audit(
            &root,
            "project.initialized",
            json!({
                "harness": harness,
                "scope": scope,
                "graphPhases": graph_phases,
                "projection": projection_dir,
            }),
        )?;
    }
    let mut knowledge = crate::knowledge::init_knowledge(&root, false)?;
    let scan = scan_project(&root);
    let (candidates, doc_skipped) =
        detect_documentation(&root, &options.explicit_docs, options.detect_docs);
    let (exists, existing) = crate::knowledge::load_knowledge_manifest(&root)?;
    let plan = plan_registration(
        exists.then_some(&existing),
        &crate::knowledge::default_knowledge_manifest(&root),
        &candidates,
    );
    if matches!(plan.status, "created" | "updated") {
        crate::knowledge::write_knowledge_manifest(&root, &plan.manifest)?;
        audit(
            &root,
            "knowledge.sources.registered",
            json!({"added": plan.added, "status": plan.status}),
        )?;
    }
    knowledge["manifestStatus"] = json!(plan.status);
    knowledge["added"] = json!(plan.added);
    knowledge["wouldAdd"] = json!(plan.would_add);
    knowledge["alreadyRegistered"] = json!(plan.already_registered);
    knowledge["registered"] = json!(plan.added);
    if plan.status == "absent" {
        knowledge["review"] = json!("No documentation was detected");
    }
    let profile = write_project_profile(&root, &scan, &candidates, &doc_skipped)?;
    write_sources_snapshot(
        &root,
        plan.manifest["sources"]
            .as_array()
            .map_or(&[], Vec::as_slice),
    )?;
    let language_names: Vec<Value> = profile["languages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["name"].clone())
        .collect();
    audit(
        &root,
        "project.onboarded",
        json!({"kind": profile["kind"], "languages": language_names}),
    )?;
    let mut out = json!({
        "initialized": true,
        "harness": harness,
        "harnesses": configured_harnesses(&crate::scope::load_config_unvalidated(&root)),
        "scope": scope,
        "stateDir": root.join(".aidlc").to_string_lossy(),
        "projection": projection_root.to_string_lossy(),
        "projections": projections,
        "methodology": methodology.to_string_lossy(),
        "filesCreated": files_created,
        "knowledge": knowledge,
        "kind": profile["kind"],
        "shared": root.join(".aidlc").join("shared").to_string_lossy(),
        "languages": profile["languages"],
        "architecture": profile["architecture"],
        "documentation": profile["documentation"],
    });
    if let Some(files) = profile.pointer("/legacy/brdCopies/files") {
        out["legacyBrdCopies"] = files.clone();
    }
    Ok(out)
}

/// Node `writeFileSync(configPath(root), json, { flag: 'wx', mode: 0o600 })`.
fn create_config(root: &Path, path: &Path, config: &Value) -> Result<(), String> {
    let dir = root.join(".aidlc");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    text.push('\n');
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(error) = written {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("{}: {error}", path.display()));
    }
    Ok(())
}

/// `onboard --rescan` (Node): re-scan an initialized project, register `--docs` paths
/// in an existing knowledge manifest, rewrite the profile and snapshot, and report what
/// changed since the previous scan. The configuration is never touched.
pub fn rescan_project(root: &Path, explicit_docs: &[String]) -> io::Result<Value> {
    rescan(root, explicit_docs).map_err(io::Error::other)
}

fn rescan(root: &Path, explicit_docs: &[String]) -> Result<Value, String> {
    let root = crate::orchestrate::resolve_like_node(root)?;
    crate::audit::assert_safe_state_dir(&root)?;
    if !root.join(".aidlc").join("config.json").exists() {
        return Err(NOT_INITIALIZED.into());
    }
    let (exists, manifest) = crate::knowledge::knowledge_preflight(&root)?;
    let config = crate::scope::load_config_unvalidated(&root);
    let create = onboard_create_command(&config);
    if !exists && !explicit_docs.is_empty() {
        return Err(format!(
            "aidlc.knowledge.json does not exist; run `{create} {}` instead",
            explicit_docs
                .iter()
                .map(|path| {
                    let plain = path
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
                    if plain {
                        format!("--docs {path}")
                    } else {
                        format!("--docs \"{path}\"")
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    validate_docs(&root, explicit_docs)?;
    preflight_writes(&root)?;
    let sources = |manifest: &Value| manifest["sources"].as_array().cloned().unwrap_or_default();
    let before =
        crate::orchestration_context::source_status_json(&root, exists, &sources(&manifest))?;
    let (candidates, doc_skipped) = detect_documentation(&root, explicit_docs, true);
    let default = crate::knowledge::default_knowledge_manifest(&root);
    let plan = if exists {
        let plan = plan_registration(Some(&manifest), &default, &candidates);
        if plan.status == "updated" {
            crate::knowledge::write_knowledge_manifest(&root, &plan.manifest)?;
            let added_paths: Vec<Value> = sources(&plan.manifest)
                .into_iter()
                .filter(|source| {
                    plan.added
                        .iter()
                        .any(|id| source["id"].as_str() == Some(id.as_str()))
                })
                .map(|source| source["path"].clone())
                .collect();
            audit(
                &root,
                "knowledge.sources.registered",
                json!({"manifestStatus": "updated", "added": added_paths}),
            )?;
        }
        plan
    } else {
        Plan {
            manifest: default,
            status: "absent",
            added: Vec::new(),
            would_add: candidates
                .iter()
                .map(|row| json!({"path": row["path"], "kind": row["kind"], "command": create}))
                .collect(),
            already_registered: Vec::new(),
        }
    };
    let scan = scan_project(&root);
    let profile = write_project_profile(&root, &scan, &candidates, &doc_skipped)?;
    let plan_sources = sources(&plan.manifest);
    write_sources_snapshot(&root, &plan_sources)?;
    let after_present = exists && plan.status == "updated";
    let after = crate::orchestration_context::source_status_json(
        &root,
        exists,
        &if after_present {
            plan_sources
        } else {
            sources(&manifest)
        },
    )?;
    let count = |key: &str| {
        before[key].as_array().map_or(0, Vec::len)
            + before["more"][key].as_u64().unwrap_or(0) as usize
    };
    audit(
        &root,
        "project.rescanned",
        json!({
            "kind": profile["kind"],
            "changed": count("changed"),
            "added": count("added"),
            "removed": count("removed"),
            "missing": before["missing"].as_array().map_or(0, Vec::len),
        }),
    )?;
    let knowledge = if exists {
        json!({
            "manifest": KNOWLEDGE_MANIFEST,
            "manifestStatus": plan.status,
            "registered": plan.added,
            "added": plan.added,
            "wouldAdd": plan.would_add,
            "warnings": after["warnings"],
        })
    } else {
        let mut knowledge = Map::new();
        knowledge.insert("manifest".into(), json!(KNOWLEDGE_MANIFEST));
        knowledge.insert("manifestStatus".into(), json!("absent"));
        knowledge.insert("registered".into(), json!([]));
        knowledge.insert("added".into(), json!([]));
        knowledge.insert("wouldAdd".into(), json!(plan.would_add));
        if !plan.would_add.is_empty() {
            knowledge.insert("registerAllCommand".into(), json!(create));
        }
        knowledge.insert("review".into(), json!("No documentation was detected"));
        knowledge.insert("warnings".into(), after["warnings"].clone());
        Value::Object(knowledge)
    };
    let mut out = json!({
        "rescanned": true,
        "kind": profile["kind"],
        "languages": profile["languages"],
        "architecture": profile["architecture"],
        "documentation": profile["documentation"],
        "knowledge": knowledge,
        "changesSinceLastScan": {
            "scannedAt": before["scannedAt"],
            "changed": before["changed"],
            "added": before["added"],
            "removed": before["removed"],
            "more": before["more"],
        },
        "missing": before["missing"],
    });
    if has_legacy_brd(&root) {
        out["legacyBrdCopies"] = json!(1);
    }
    Ok(out)
}

/// Node `onboardCommands(root).create`.
fn onboard_create_command(config: &Value) -> String {
    let harness = config
        .get("harness")
        .and_then(Value::as_str)
        .filter(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    let scope = config
        .get("scope")
        .and_then(Value::as_str)
        .filter(|scope| SCOPES.contains(scope));
    match (harness, scope) {
        (Some(harness), Some(scope)) => {
            format!("dtech-aidlc onboard --harness {harness} --scope {scope}")
        }
        _ => "dtech-aidlc onboard --harness <claude|deepseek>".to_owned(),
    }
}

/// Node `hasLegacyBrd(root)`.
fn has_legacy_brd(root: &Path) -> bool {
    let dir = root.join(".aidlc").join("shared").join("brd");
    dir.exists()
        && fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir())
        && fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!(
            "aidlc-onboard-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn options(harness: &str, scope: &str) -> OnboardOptions {
        OnboardOptions {
            harness: harness.into(),
            scope: scope.into(),
            explicit_docs: vec![],
            detect_docs: true,
        }
    }
    #[test]
    fn rejects_traversal_and_absolute_paths() {
        assert!(normalize_source_path("../escape").is_err());
        assert!(normalize_source_path("/escape").is_err());
        #[cfg(windows)]
        assert!(normalize_source_path("C:/escape").is_err());
        #[cfg(not(windows))]
        assert_eq!(normalize_source_path("C:/escape").unwrap(), "C:/escape");
        assert_eq!(normalize_source_path("./docs/").unwrap(), "docs");
    }
    #[test]
    fn extname_matches_node() {
        for (name, ext) in [
            ("a.", "."),
            ("...", "."),
            ("..a", ".a"),
            ("a..b", ".b"),
            (".a.b", ".b"),
            (".bashrc", ""),
            ("..", ""),
            ("x", ""),
            ("main.RS", ".RS"),
        ] {
            assert_eq!(extname(name), ext, "{name}");
        }
    }
    #[test]
    fn documentation_name_rules_match_node() {
        assert!(requirements_kind("product-BRD.md"));
        assert!(requirements_kind("business_requirements.docx"));
        assert!(requirements_kind("prd"));
        assert!(!requirements_kind("prdx.md"));
        assert!(!requirements_kind("requirements.txt"));
        assert!(!requirements_kind("requirements-dev.in"));
        assert!(open_api_kind("openapi.yaml"));
        assert!(open_api_kind("api.swagger.json"));
        assert!(!open_api_kind("openapi.md"));
        assert!(architecture_file("architecture-overview.md"));
        assert!(!architecture_file("architectures.md"));
        assert!(is_root_text_name("Readme.md", "README"));
        assert_eq!(source_id("docs/My Guide!.md"), "docs-My-Guide-.md");
        assert_eq!(source_id("///"), "docs");
    }
    #[test]
    fn scans_languages_and_omits_generated_directories() {
        let root = temp();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("target/ignored.rs"), "").unwrap();
        fs::write(root.join(".hidden.rs"), "").unwrap();
        let scan = scan_project(&root);
        assert_eq!(scan["kind"], "brownfield");
        assert_eq!(scan["sourceFiles"], 1);
        assert_eq!(
            scan["languages"],
            json!([{"name": "rust", "evidence": ["src/main.rs"]}])
        );
        assert_eq!(scan["skipped"]["counts"]["hidden"], 1);
        assert_eq!(scan["skipped"]["counts"]["vendored-or-generated"], 1);
        assert_eq!(
            scan["skipped"]["examples"],
            json!([{"path": "target", "reason": "vendored-or-generated"}])
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unrecognized_files_make_an_unknown_project() {
        let root = temp();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("model.abc"), "").unwrap();
        fs::write(root.join("notes.md"), "").unwrap();
        let scan = scan_project(&root);
        assert_eq!(scan["kind"], "unknown");
        assert_eq!(scan["unclassified"], json!(["model.abc"]));
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

    #[test]
    fn onboarding_defaults_scope_and_reuses_existing_configuration() {
        let root = temp();
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        fs::write(
            root.join(".aidlc/config.json"),
            r#"{"harness":"deepseek","harnesses":["deepseek"],"scope":"team","initializedAt":"2024-01-01T00:00:00.000Z"}"#,
        )
        .unwrap();
        let result = onboard_project(&root, &options("", "")).unwrap();
        assert_eq!(result["harness"], "deepseek");
        assert_eq!(result["scope"], "team");
        assert_eq!(result["knowledge"]["manifestStatus"], "absent");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn onboarding_defaults_scope_for_new_project() {
        let root = temp();
        fs::create_dir_all(&root).unwrap();
        let mut opts = options("claude", "");
        opts.detect_docs = false;
        let result = onboard_project(&root, &opts).unwrap();
        assert_eq!(result["scope"], "solo");
        let config: Value =
            serde_json::from_slice(&fs::read(root.join(".aidlc/config.json")).unwrap()).unwrap();
        assert_eq!(config["scope"], "solo");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn onboarding_writes_profile_and_preserves_existing_files() {
        let root = temp();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let opts = options("claude", "solo");
        let result = onboard_project(&root, &opts).unwrap();
        assert_eq!(result["kind"], "brownfield");
        let config: Value =
            serde_json::from_slice(&fs::read(root.join(".aidlc/config.json")).unwrap()).unwrap();
        assert_eq!(config["harness"], "claude");
        let profile: Value = serde_json::from_slice(
            &fs::read(root.join(".aidlc/shared/project-profile.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(profile["kind"], "brownfield");
        let rerun = onboard_project(&root, &opts).unwrap();
        assert_eq!(rerun["knowledge"]["manifestStatus"], "absent");
        assert_eq!(rerun["knowledge"]["added"], json!([]));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rescan_rejects_unsafe_docs_and_retains_config() {
        let root = temp();
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        fs::write(root.join(".aidlc/config.json"), "user config\n").unwrap();
        assert!(rescan_project(&root, &["../outside".into()]).is_err());
        assert_eq!(
            fs::read_to_string(root.join(".aidlc/config.json")).unwrap(),
            "user config\n"
        );
        rescan_project(&root, &[]).unwrap();
        assert!(rescan_project(&root, &[]).is_ok());
        assert_eq!(
            fs::read_to_string(root.join(".aidlc/config.json")).unwrap(),
            "user config\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
