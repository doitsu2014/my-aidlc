//! Native dependency manager corresponding to `core/tools/dependencies.mjs`.
//! Operations return JSON values matching the Node implementation's records.
use serde_json::{json, Map, Value};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const SAFE_LEN: usize = 80;
const MAX_GIT_OUTPUT: usize = 8 * 1024 * 1024;
fn err<T>(s: impl Into<String>) -> Result<T, String> {
    Err(s.into())
}
fn check_name(n: &str) -> Result<(), String> {
    if !n.is_empty()
        && n.len() <= SAFE_LEN
        && n.as_bytes()[0].is_ascii_alphanumeric()
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && n != "."
        && n != ".."
    {
        Ok(())
    } else {
        err(format!("Invalid dependency name: {n}"))
    }
}
fn validate_url(u: &str) -> Result<(), String> {
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| c.is_whitespace() || c == '\0') {
        return err("Invalid repository URL");
    }
    let l = u.to_ascii_lowercase();
    let ok = if l.starts_with("https://") || l.starts_with("ssh://") {
        let rest = &u[u.find("://").unwrap() + 3..];
        let a = rest.split('/').next().unwrap_or("");
        !a.is_empty() && !a.contains('@') && (!l.starts_with("ssh://") || !a.ends_with(':'))
    } else if u.starts_with("git@") {
        let q: Vec<_> = u.splitn(2, ':').collect();
        q.len() == 2
            && q[0].len() > 4
            && q[0][4..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
            && !q[1].is_empty()
            && !q[1].contains('?')
            && !q[1].contains('#')
            && !q[1].split('/').any(|x| x == "..")
    } else {
        Path::new(u).is_absolute()
            && !u.starts_with("//")
            && !Path::new(u)
                .components()
                .any(|x| matches!(x, Component::ParentDir))
    };
    if !ok || l.starts_with("file::") || l.starts_with("ext::") {
        err("Invalid repository URL: use https://, ssh://, git@host:path, or an absolute local path without embedded passwords or tokens")
    } else {
        Ok(())
    }
}
fn check_ref(r: Option<&str>) -> Result<(), String> {
    if let Some(s) = r {
        if s.is_empty()
            || s.starts_with('-')
            || s.chars()
                .any(|c| c.is_whitespace() || c == '\0' || "~^:?*[\\".contains(c))
            || s.contains("..")
            || s.contains("@{")
            || s.ends_with('/')
            || s.ends_with('.')
            || s.contains("//")
        {
            return err("Invalid dependency ref");
        }
    }
    Ok(())
}
fn root(p: &Path) -> Result<PathBuf, String> {
    if !p.is_absolute() {
        return err("Project root must be an absolute path");
    }
    let x = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if let Ok(m) = fs::symlink_metadata(&x) {
        if m.file_type().is_symlink() {
            return err(format!("Refusing to use symlink path: {}", x.display()));
        }
        if !m.is_dir() {
            return err("Project root must be a directory");
        }
    }
    Ok(x)
}
#[cfg(unix)]
fn private(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o700));
}
#[cfg(not(unix))]
fn private(_: &Path) {}
#[derive(Clone)]
struct Paths {
    project: PathBuf,
    aidlc: PathBuf,
    repos: PathBuf,
    manifest: PathBuf,
    lock: PathBuf,
    lockfile: PathBuf,
}
fn paths(r: &Path, create: bool) -> Result<Paths, String> {
    let project = root(r)?;
    let aidlc = project.join(".aidlc");
    if let Ok(m) = fs::symlink_metadata(&aidlc) {
        if m.file_type().is_symlink() || !m.is_dir() {
            return err("Refusing unsafe .aidlc path");
        }
    }
    if create && !aidlc.exists() {
        fs::create_dir(&aidlc).map_err(|e| e.to_string())?;
        private(&aidlc)
    }
    let repos = aidlc.join("repos");
    if let Ok(m) = fs::symlink_metadata(&repos) {
        if m.file_type().is_symlink() || !m.is_dir() {
            return err("Refusing unsafe dependency repository directory");
        }
    }
    Ok(Paths {
        project,
        manifest: aidlc.join("deps.json"),
        lock: aidlc.join("deps.lock"),
        lockfile: aidlc.join("deps-lock.json"),
        aidlc,
        repos,
    })
}
fn valid_manifest(v: &Value) -> Result<(), String> {
    let o = v
        .as_object()
        .ok_or("Invalid dependency manifest: expected schemaVersion 1 and repos object")?;
    if o.len() != 2 || v["schemaVersion"].as_i64() != Some(1) || !v["repos"].is_object() {
        return err("Invalid dependency manifest: expected schemaVersion 1 and repos object");
    }
    for (n, e) in v["repos"].as_object().unwrap() {
        check_name(n)?;
        let m = e
            .as_object()
            .ok_or_else(|| format!("Invalid dependency manifest entry: {n}"))?;
        if m.keys().any(|k| k != "url" && k != "ref") {
            return err(format!("Invalid dependency manifest entry: {n}"));
        }
        validate_url(e["url"].as_str().ok_or("Invalid repository URL")?)?;
        check_ref(e.get("ref").and_then(Value::as_str))?
    }
    Ok(())
}
fn empty() -> Value {
    json!({"schemaVersion":1,"repos":{}})
}
fn read_json(p: &Path, missing: bool) -> Result<Value, String> {
    match fs::symlink_metadata(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && missing => Ok(empty()),
        Err(e) => err(e.to_string()),
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => {
            err("Refusing unsafe dependency manifest path")
        }
        Ok(_) => serde_json::from_slice(
            &fs::read(p).map_err(|_| "Invalid dependency manifest: malformed JSON")?,
        )
        .map_err(|_| "Invalid dependency manifest: malformed JSON".into()),
    }
}
fn manifest(p: &Paths) -> Result<Value, String> {
    let v = read_json(&p.manifest, true)?;
    valid_manifest(&v)?;
    Ok(v)
}
fn write_atomic(p: &Path, v: &Value, mode: u32) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = mode;
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let t = p.with_extension(format!("{}.{}.tmp", std::process::id(), n));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&t)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|e| e.to_string())?
    }
    let mut b = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    b.push(b'\n');
    if let Err(e) = f.write_all(&b) {
        let _ = fs::remove_file(t);
        return err(e.to_string());
    }
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    fs::rename(&t, p).map_err(|e| e.to_string())
}
fn lock<T>(p: &Paths, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let mut fd = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&p.lock)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                "Dependency manager is locked by another process".to_string()
            } else {
                e.to_string()
            }
        })?;
    let _ = writeln!(
        fd,
        "{}",
        json!({"pid":std::process::id(),"at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()})
    );
    let r = f();
    drop(fd);
    let _ = fs::remove_file(&p.lock);
    r
}
fn git(p: &Paths, args: &[&str], url: Option<&str>) -> Result<String, String> {
    let tmp = p.aidlc.join(format!(
        ".git-template-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&tmp).map_err(|e| e.to_string())?;
    private(&tmp);
    let mut c = Command::new("git");
    c.current_dir(&p.project)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=",
            "-c",
            "core.sshCommand=ssh",
            "-c",
            "core.askPass=",
            "-c",
            "credential.helper=",
            "-c",
            "diff.external=",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "submodule.recurse=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TEMPLATE_DIR", &tmp)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    for k in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        c.env_remove(k);
    }
    for (k, _) in env::vars_os() {
        let s = k.to_string_lossy();
        if s.starts_with("GIT_CONFIG_KEY_") || s.starts_with("GIT_CONFIG_VALUE_") {
            c.env_remove(k);
        }
    }
    let output = c.output().map_err(|e| e.to_string());
    let _ = fs::remove_dir_all(&tmp);
    let o = output?;
    if o.stdout.len() > MAX_GIT_OUTPUT || o.stderr.len() > MAX_GIT_OUTPUT {
        return err("git output exceeded limit");
    }
    if !o.status.success() {
        let mut e = String::from_utf8_lossy(&o.stderr).trim().to_string();
        if let Some(u) = url {
            e = e.replace(u, "[repository URL]")
        }
        return err(e);
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned())
}
fn sorted(v: &Value) -> Vec<String> {
    let mut a: Vec<_> = v["repos"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    a.sort();
    a
}
fn record(p: &Paths, n: &str, e: &Value, status: Option<&str>, commit: Option<&str>) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), json!(n));
    m.insert("url".into(), e["url"].clone());
    if let Some(r) = e.get("ref") {
        m.insert("ref".into(), r.clone());
    }
    m.insert("path".into(), json!(p.repos.join(n)));
    if let Some(s) = status {
        m.insert("status".into(), json!(s));
    }
    if let Some(c) = commit {
        m.insert("commit".into(), json!(c));
    }
    Value::Object(m)
}
pub fn project_dependencies_path(r: &Path) -> Result<PathBuf, String> {
    Ok(root(r)?.join("aidlc.deps.json"))
}
fn declared(f: &Path) -> Result<Value, String> {
    let p = if f.is_absolute() {
        f.to_path_buf()
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(f)
    };
    let m = fs::symlink_metadata(&p)
        .map_err(|_| format!("Dependency file is missing or unsafe: {}", p.display()))?;
    if m.file_type().is_symlink() || !m.is_file() {
        return err(format!(
            "Dependency file is missing or unsafe: {}",
            p.display()
        ));
    }
    let v = read_json(&p, false)?;
    valid_manifest(&v)?;
    Ok(v)
}
pub fn import_dependencies(r: &Path, f: &Path) -> Result<Value, String> {
    let d = declared(f)?;
    let p = paths(r, true)?;
    lock(&p, || {
        let old = manifest(&p)?;
        let dropped: Vec<_> = sorted(&old)
            .into_iter()
            .filter(|n| !d["repos"].as_object().unwrap().contains_key(n))
            .collect();
        write_atomic(&p.manifest, &d, 0o600)?;
        Ok(json!({"file":f,"repos":sorted(&d),"dropped":dropped}))
    })
}
pub fn add_dependency(r: &Path, n: &str, u: &str, rf: Option<&str>) -> Result<Value, String> {
    check_name(n)?;
    validate_url(u)?;
    check_ref(rf)?;
    let p = paths(r, true)?;
    lock(&p, || {
        let dp = project_dependencies_path(r)?;
        let mut v = if dp.exists() {
            declared(&dp)?
        } else {
            manifest(&p)?
        };
        let repos = v["repos"].as_object_mut().unwrap();
        if let Some(old) = repos.get(n) {
            if old["url"].as_str() == Some(u) && old.get("ref").and_then(Value::as_str) == rf {
                return Ok(record(&p, n, old, None, None));
            }
            return err(format!(
                "Dependency {n} already exists with a different configuration"
            ));
        }
        let mut e = json!({"url":u});
        if let Some(x) = rf {
            e["ref"] = json!(x)
        }
        repos.insert(n.into(), e.clone());
        if dp.exists() {
            write_atomic(&dp, &v, 0o644)?
        }
        write_atomic(&p.manifest, &v, 0o600)?;
        Ok(record(&p, n, &e, None, None))
    })
}
pub fn list_dependencies(r: &Path) -> Result<Vec<Value>, String> {
    let p = paths(r, false)?;
    let v = manifest(&p)?;
    Ok(sorted(&v)
        .iter()
        .map(|n| {
            record(
                &p,
                n,
                &v["repos"][n],
                Some(if p.repos.join(n).exists() {
                    "invalid"
                } else {
                    "not-cloned"
                }),
                None,
            )
        })
        .collect())
}
fn safe_checkout(p: &Paths, n: &str, url: &str) -> Result<Option<PathBuf>, String> {
    let target = p.repos.join(n);
    let meta = match fs::symlink_metadata(&target) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return err(e.to_string()),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return err(format!("Refusing unmanaged dependency path: {n}"));
    }
    let gd = target.join(".git");
    let gm = fs::symlink_metadata(&gd)
        .map_err(|_| format!("Refusing unmanaged dependency checkout: {n}"))?;
    if gm.file_type().is_symlink() || !gm.is_dir() {
        return err(format!("Refusing unmanaged dependency checkout: {n}"));
    }
    let actual = target.canonicalize().map_err(|e| e.to_string())?;
    let base = p.repos.canonicalize().map_err(|e| e.to_string())?;
    if !actual.starts_with(&base) || actual == base {
        return err(format!(
            "Dependency checkout escapes managed directory: {n}"
        ));
    }
    let origin = git(
        p,
        &[
            "-C",
            target.to_str().ok_or("Invalid checkout path")?,
            "config",
            "--local",
            "--get",
            "remote.origin.url",
        ],
        Some(url),
    )?;
    if origin != url {
        return err(format!(
            "Dependency checkout origin does not match manifest: {n}"
        ));
    }
    let top = git(
        p,
        &[
            "-C",
            target.to_str().ok_or("Invalid checkout path")?,
            "rev-parse",
            "--show-toplevel",
        ],
        Some(url),
    )?;
    if Path::new(&top).canonicalize().ok().as_deref() != Some(actual.as_path()) {
        return err(format!(
            "Dependency checkout has an unexpected Git worktree: {n}"
        ));
    }
    Ok(Some(target))
}
fn git_path(path: &Path) -> Result<String, String> {
    let value = path.to_str().ok_or("Invalid checkout path")?;
    #[cfg(windows)]
    if let Some(rest) = value.strip_prefix("\\\\?\\") {
        return Ok(rest.to_string());
    }
    Ok(value.to_string())
}
fn checkout_git(p: &Paths, target: &Path, args: &[&str], url: &str) -> Result<String, String> {
    let path = git_path(target)?;
    let mut full = vec!["-C", path.as_str()];
    full.extend_from_slice(args);
    git(p, &full, Some(url))
}
fn desired_commit(p: &Paths, target: &Path, rf: Option<&str>, url: &str) -> Result<String, String> {
    if let Some(r) = rf {
        let pin = r.len() >= 40 && r.len() <= 64 && r.bytes().all(|b| b.is_ascii_hexdigit());
        if pin || r.starts_with("refs/tags/") {
            if let Ok(c) = checkout_git(
                p,
                target,
                &["rev-parse", "--verify", &format!("{r}^{{commit}}")],
                url,
            ) {
                return Ok(c);
            }
        }
        checkout_git(
            p,
            target,
            &["fetch", "--no-recurse-submodules", "--no-tags", "origin", r],
            url,
        )?;
    } else {
        checkout_git(
            p,
            target,
            &[
                "fetch",
                "--no-recurse-submodules",
                "--no-tags",
                "origin",
                "HEAD",
            ],
            url,
        )?;
    }
    checkout_git(
        p,
        target,
        &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
        url,
    )
}
fn install_checkout(p: &Paths, n: &str, e: &Value) -> Result<String, String> {
    let url = e["url"].as_str().ok_or("Invalid repository URL")?;
    let rf = e.get("ref").and_then(Value::as_str);
    fs::create_dir_all(&p.repos).map_err(|e| e.to_string())?;
    let target = p.repos.join(n);
    if let Some(target) = safe_checkout(p, n, url)? {
        let status = checkout_git(
            p,
            &target,
            &["status", "--porcelain", "--untracked-files=all"],
            url,
        )?;
        if !status.is_empty() {
            return err("Checkout has dirty or untracked changes; refusing to update");
        }
        let current = checkout_git(p, &target, &["rev-parse", "HEAD"], url)?;
        let desired = desired_commit(p, &target, rf, url)?;
        if current == desired {
            return Ok(current);
        }
        if checkout_git(
            p,
            &target,
            &["merge-base", "--is-ancestor", &current, &desired],
            url,
        )
        .is_err()
        {
            return err("Update is not a fast-forward; refusing to change checkout");
        }
        let pinned = rf.is_some_and(|r| {
            (r.len() >= 40 && r.len() <= 64 && r.bytes().all(|b| b.is_ascii_hexdigit()))
                || r.starts_with("refs/tags/")
        });
        if pinned {
            return Ok(current);
        }
        checkout_git(
            p,
            &target,
            &["checkout", "--detach", "--no-recurse-submodules", &desired],
            url,
        )?;
        return Ok(desired);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = p
        .repos
        .join(format!(".tmp-{n}-{}-{stamp}", std::process::id()));
    let result = (|| {
        git(
            p,
            &[
                "clone",
                "--no-checkout",
                "--no-recurse-submodules",
                "--origin",
                "origin",
                "--",
                url,
                &git_path(&temp)?,
            ],
            Some(url),
        )?;
        let wanted = rf.unwrap_or("HEAD");
        let commit = if rf.is_some() {
            checkout_git(
                p,
                &temp,
                &["rev-parse", "--verify", &format!("{wanted}^{{commit}}")],
                url,
            )
            .or_else(|_| {
                checkout_git(
                    p,
                    &temp,
                    &[
                        "fetch",
                        "--no-recurse-submodules",
                        "--no-tags",
                        "origin",
                        wanted,
                    ],
                    url,
                )?;
                checkout_git(
                    p,
                    &temp,
                    &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
                    url,
                )
            })?
        } else {
            checkout_git(p, &temp, &["rev-parse", "--verify", "HEAD^{commit}"], url)?
        };
        checkout_git(
            p,
            &temp,
            &["checkout", "--detach", "--no-recurse-submodules", &commit],
            url,
        )?;
        if fs::symlink_metadata(&target).is_ok() {
            return err(format!("Refusing to overwrite dependency path: {n}"));
        }
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        Ok(commit)
    })();
    if temp.exists() {
        let _ = fs::remove_dir_all(&temp);
    }
    result
}
pub fn pull_dependencies(r: &Path, name: Option<&str>) -> Result<Vec<Value>, String> {
    if let Some(n) = name {
        check_name(n)?;
    }
    let p = paths(r, true)?;
    lock(&p, || {
        let v = manifest(&p)?;
        let names = match name {
            Some(n) if v["repos"].get(n).is_none() => {
                return err(format!("Unknown dependency: {n}"))
            }
            Some(n) => vec![n.to_string()],
            None => sorted(&v),
        };
        for n in &names {
            safe_checkout(&p, n, v["repos"][n]["url"].as_str().unwrap())?;
        }
        let mut l = if p.lockfile.exists() {
            read_json(&p.lockfile, false).map_err(|_| "Invalid dependency lock file")?
        } else {
            json!({"schemaVersion":1,"repos":{}})
        };
        if l["schemaVersion"].as_i64() != Some(1) || !l["repos"].is_object() {
            return err("Invalid dependency lock file");
        }
        let mut rows = Vec::new();
        for n in names {
            match install_checkout(&p, &n, &v["repos"][&n]) {
                Ok(commit) => {
                    l["repos"][&n] = json!({"commit":commit});
                    rows.push(json!({"name":n,"ok":true,"commit":commit,"path":p.repos.join(&n)}));
                }
                Err(e) => rows.push(json!({"name":n,"ok":false,"error":e})),
            }
        }
        if rows.iter().any(|x| x["ok"] == true) {
            write_atomic(&p.lockfile, &l, 0o600)?;
        }
        Ok(rows)
    })
}
pub fn remove_dependency(r: &Path, n: &str) -> Result<Value, String> {
    check_name(n)?;
    let p = paths(r, true)?;
    lock(&p, || {
        let dp = project_dependencies_path(r)?;
        let mut run = manifest(&p)?;
        let has_run = run["repos"].get(n).is_some();
        let mut dec = if dp.exists() {
            Some(declared(&dp)?)
        } else {
            None
        };
        let has_dec = dec.as_ref().is_some_and(|v| v["repos"].get(n).is_some());
        if !has_run && !has_dec {
            return err(format!("Unknown dependency: {n}"));
        }
        if let Some(d) = dec.as_mut() {
            d["repos"].as_object_mut().unwrap().remove(n);
            write_atomic(&dp, d, 0o644)?
        }
        run["repos"].as_object_mut().unwrap().remove(n);
        write_atomic(&p.manifest, &run, 0o600)?;
        if let Ok(mut l) = read_json(&p.lockfile, false) {
            if l["schemaVersion"].as_i64() == Some(1) && l["repos"].is_object() {
                l["repos"].as_object_mut().unwrap().remove(n);
                write_atomic(&p.lockfile, &l, 0o600)?
            }
        }
        Ok(json!({"name":n,"removed":true}))
    })
}
pub fn validate_dependencies(r: &Path) -> Result<Vec<Value>, String> {
    let p = paths(r, false)?;
    let v = manifest(&p)?;
    let mut rows = vec![];
    for n in sorted(&v) {
        let e = &v["repos"][&n];
        let t = p.repos.join(&n);
        if let Ok(m) = fs::symlink_metadata(&t) {
            if m.file_type().is_symlink() || !m.is_dir() {
                return err(format!(
                    "Dependency validation failed: {n}: Refusing unmanaged dependency path: {n}"
                ));
            }
            let actual = git(
                &p,
                &[
                    "-C",
                    t.to_str().unwrap_or(""),
                    "config",
                    "--local",
                    "--get",
                    "remote.origin.url",
                ],
                e["url"].as_str(),
            )
            .map_err(|x| format!("Dependency validation failed: {n}: {x}"))?;
            if Some(actual.as_str()) != e["url"].as_str() {
                return err(format!("Dependency validation failed: {n}: Dependency checkout origin does not match manifest: {n}"));
            }
        }
        rows.push(record(
            &p,
            &n,
            e,
            Some(if t.exists() { "cloned" } else { "not-cloned" }),
            None,
        ))
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_names_urls_and_refs() {
        assert!(check_name("safe-name_1").is_ok());
        assert!(check_name("../escape").is_err());
        assert!(validate_url("https://host/repo.git").is_ok());
        assert!(validate_url("https://user:secret@host/repo").is_err());
        assert!(validate_url("-x").is_err());
        assert!(check_ref(Some("main")).is_ok());
        assert!(check_ref(Some("--upload-pack=x")).is_err());
    }
    #[test]
    fn rejects_unsafe_manifest() {
        assert!(valid_manifest(
            &json!({"schemaVersion":1,"repos":{"ok":{"url":"https://host/r","extra":true}}})
        )
        .is_err());
    }
    #[test]
    fn add_list_remove_roundtrip() {
        let d = std::env::temp_dir().join(format!("deps-rust-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        add_dependency(&d, "lib", "https://host/lib.git", Some("main")).unwrap();
        assert_eq!(list_dependencies(&d).unwrap().len(), 1);
        remove_dependency(&d, "lib").unwrap();
        assert!(list_dependencies(&d).unwrap().is_empty());
        fs::remove_dir_all(d).unwrap();
    }
    fn test_git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    #[test]
    fn pull_clones_and_preserves_dirty_checkout() {
        let base = std::env::temp_dir().join(format!("deps-pull-rust-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let remote = base.join("remote");
        let project = base.join("project");
        fs::create_dir_all(&remote).unwrap();
        fs::create_dir_all(&project).unwrap();
        test_git(&remote, &["init", "--bare"]);
        let seed = base.join("seed");
        fs::create_dir_all(&seed).unwrap();
        test_git(&seed, &["init"]);
        test_git(&seed, &["config", "user.email", "test@example.invalid"]);
        test_git(&seed, &["config", "user.name", "Test"]);
        fs::write(seed.join("tracked"), "base\n").unwrap();
        test_git(&seed, &["add", "."]);
        test_git(&seed, &["commit", "-m", "base"]);
        test_git(&seed, &["branch", "-M", "main"]);
        test_git(
            &seed,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        test_git(&seed, &["push", "-u", "origin", "main"]);
        test_git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        add_dependency(&project, "lib", remote.to_str().unwrap(), None).unwrap();
        let first = pull_dependencies(&project, None).unwrap();
        assert_eq!(first[0]["ok"], true, "pull result: {first:?}");
        let checkout = project.join(".aidlc/repos/lib");
        let head = git(
            &paths(&project, false).unwrap(),
            &["-C", checkout.to_str().unwrap(), "rev-parse", "HEAD"],
            None,
        )
        .unwrap();
        fs::write(checkout.join("tracked"), "user change\n").unwrap();
        let second = pull_dependencies(&project, None).unwrap();
        assert_eq!(second[0]["ok"], false);
        assert_eq!(
            fs::read_to_string(checkout.join("tracked")).unwrap(),
            "user change\n"
        );
        assert_eq!(
            git(
                &paths(&project, false).unwrap(),
                &["-C", checkout.to_str().unwrap(), "rev-parse", "HEAD"],
                None
            )
            .unwrap(),
            head
        );
        fs::remove_dir_all(base).unwrap();
    }
}
