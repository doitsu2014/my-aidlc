//! Native dependency manager corresponding to `core/tools/dependencies.mjs`, plus the
//! `repos` and `deps` command handlers from `core/tools/dtech-aidlc.mjs`.
//! Operations return JSON values matching the Node implementation's records.
use crate::args::FlagValue;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SAFE_LEN: usize = 80;
const MAX_GIT_OUTPUT: usize = 8 * 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(120);
const URL_HINT: &str = "Invalid repository URL: use https://, ssh://, git@host:path, or an absolute local path without embedded passwords or tokens";

fn err<T>(s: impl Into<String>) -> Result<T, String> {
    Err(s.into())
}
fn is_js_space(c: char) -> bool {
    crate::knowledge::is_js_space(c)
}
fn js_trim(text: &str) -> &str {
    crate::knowledge::js_trim(text)
}
/// JavaScript `String(value)` for values found in a lock file.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) => "null".into(),
        Some(Value::Object(_)) => "[object Object]".into(),
        Some(other) => other.to_string(),
    }
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
/// Node `isAbsolute(url)` for the platform the CLI runs on.
fn js_is_absolute(u: &str) -> bool {
    if cfg!(windows) {
        let b = u.as_bytes();
        u.starts_with(['/', '\\'])
            || (b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && matches!(b[2], b'/' | b'\\'))
    } else {
        u.starts_with('/')
    }
}
/// The parts of a WHATWG URL that Node's `validateUrl` inspects:
/// `Some((username non-empty, password non-empty, hostname non-empty))`, or `None`
/// when the URL cannot be parsed.
fn url_authority(u: &str, special: bool) -> Option<(bool, bool, bool)> {
    let rest = &u[u.find("://")? + 3..];
    let end = rest
        .find(|c: char| c == '/' || c == '?' || c == '#' || (special && c == '\\'))
        .unwrap_or(rest.len());
    let authority = &rest[..end];
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    let (user, pass) = match userinfo {
        Some(info) => match info.split_once(':') {
            Some((user, pass)) => (user, pass),
            None => (info, ""),
        },
        None => ("", ""),
    };
    let host = if hostport.starts_with('[') {
        let close = hostport.find(']')?;
        let after = &hostport[close + 1..];
        if !after.is_empty() && !after.starts_with(':') {
            return None;
        }
        if let Some(port) = after.strip_prefix(':') {
            if !port.bytes().all(|b| b.is_ascii_digit()) || port.parse::<u64>().unwrap_or(0) > 65535
            {
                return None;
            }
        }
        &hostport[..=close]
    } else {
        let (host, port) = match hostport.rfind(':') {
            Some(colon) => (&hostport[..colon], Some(&hostport[colon + 1..])),
            None => (hostport, None),
        };
        if let Some(port) = port {
            if !port.bytes().all(|b| b.is_ascii_digit())
                || (!port.is_empty() && port.parse::<u64>().map_or(true, |n| n > 65535))
            {
                return None;
            }
        }
        host
    };
    if special && host.is_empty() {
        // `new URL('https://')` and `new URL('https://:443')` throw.
        return None;
    }
    Some((!user.is_empty(), !pass.is_empty(), !host.is_empty()))
}
fn validate_url(u: &str) -> Result<(), String> {
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| is_js_space(c) || c == '\0') {
        return err("Invalid repository URL");
    }
    let l = u.to_ascii_lowercase();
    let ok = if l.starts_with("https://") {
        url_authority(u, true).is_some_and(|(user, pass, host)| host && !user && !pass)
    } else if l.starts_with("ssh://") {
        url_authority(u, false).is_some_and(|(_, pass, host)| host && !pass)
    } else if let Some(rest) = u.strip_prefix("git@") {
        // /^git@[^/:\s]+:.+/
        match rest.find(['/', ':']) {
            Some(i) if i > 0 && rest.as_bytes()[i] == b':' && rest.len() > i + 1 => {
                !u.contains(['?', '#'])
                    && !u[u.find(':').unwrap() + 1..].split('/').any(|x| x == "..")
            }
            _ => false,
        }
    } else if js_is_absolute(u) {
        let sep = if cfg!(windows) { '\\' } else { '/' };
        !u.split(sep).any(|x| x == "..") && !u.starts_with("//")
    } else {
        false
    };
    if !ok || l.starts_with("file::") || l.starts_with("ext::") {
        err(URL_HINT)
    } else {
        Ok(())
    }
}
/// Node `checkRef(ref)`; `None` is an absent ref.
fn check_ref_value(r: Option<&Value>) -> Result<(), String> {
    match r {
        None => Ok(()),
        Some(Value::String(s)) => check_ref(Some(s)),
        Some(_) => err("Invalid dependency ref"),
    }
}
fn check_ref(r: Option<&str>) -> Result<(), String> {
    if let Some(s) = r {
        if s.is_empty()
            || s.starts_with('-')
            || s.chars()
                .any(|c| is_js_space(c) || c == '\0' || "~^:?*[\\".contains(c))
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
/// Node `path.resolve` for an already absolute path: drop `.` and fold `..`.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in p.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}
fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}
/// Node `fs.realpathSync`: the path keeps its spelling unless one of its components
/// is a symbolic link (or junction), in which case links are resolved.
fn real_path(p: &Path) -> std::io::Result<PathBuf> {
    let mut current = PathBuf::new();
    for part in p.components() {
        current.push(part.as_os_str());
        if matches!(part, Component::Normal(_))
            && fs::symlink_metadata(&current)?.file_type().is_symlink()
        {
            return p.canonicalize().map(strip_verbatim);
        }
    }
    fs::metadata(p)?;
    Ok(p.to_path_buf())
}
/// Node `existsSync` (follows symbolic links).
fn exists(p: &Path) -> bool {
    fs::metadata(p).is_ok()
}
fn root(p: &Path) -> Result<PathBuf, String> {
    if !p.is_absolute() {
        return err("Project root must be an absolute path");
    }
    let abs = normalize(p);
    if !exists(&abs) {
        return Ok(abs);
    }
    let meta = fs::symlink_metadata(&abs).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return err(format!("Refusing to use symlink path: {}", abs.display()));
    }
    if !meta.is_dir() {
        return err("Project root must be a directory");
    }
    real_path(&abs).map_err(|e| e.to_string())
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
fn unsafe_dir(p: &Path) -> bool {
    exists(p) && fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir())
}
fn paths(r: &Path, create: bool) -> Result<Paths, String> {
    let project = root(r)?;
    let aidlc = project.join(".aidlc");
    if unsafe_dir(&aidlc) {
        return err("Refusing unsafe .aidlc path");
    }
    if create && !exists(&aidlc) {
        fs::create_dir_all(&aidlc).map_err(|e| e.to_string())?;
        private(&aidlc)
    }
    let repos = aidlc.join("repos");
    if unsafe_dir(&repos) {
        return err("Refusing unsafe dependency repository directory");
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
    let shape = "Invalid dependency manifest: expected schemaVersion 1 and repos object";
    let o = v.as_object().ok_or(shape)?;
    if v["schemaVersion"].as_i64() != Some(1)
        || o.keys().any(|k| k != "schemaVersion" && k != "repos")
        || !v["repos"].is_object()
    {
        return err(shape);
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
        check_ref_value(e.get("ref"))?
    }
    Ok(())
}
fn empty() -> Value {
    json!({"schemaVersion":1,"repos":{}})
}
fn parse_manifest(p: &Path) -> Result<Value, String> {
    let bytes = fs::read(p).map_err(|_| "Invalid dependency manifest: malformed JSON")?;
    serde_json::from_str(&String::from_utf8_lossy(&bytes))
        .map_err(|_| "Invalid dependency manifest: malformed JSON".into())
}
/// Node `readManifest(paths)` for `.aidlc/deps.json`.
fn manifest(p: &Paths) -> Result<Value, String> {
    if !exists(&p.manifest) {
        return Ok(empty());
    }
    let meta = fs::symlink_metadata(&p.manifest).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return err("Refusing unsafe dependency manifest path");
    }
    let v = parse_manifest(&p.manifest)?;
    valid_manifest(&v)?;
    Ok(v)
}
fn json_text(v: &Value) -> Result<Vec<u8>, String> {
    let mut b = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    b.push(b'\n');
    Ok(b)
}
/// Node `atomicWrite` / `writeFileAtomic`: temp file beside the target, then rename.
fn write_atomic(p: &Path, v: &Value, mode: u32) -> Result<(), String> {
    crate::workflow::write_file_atomic(p, &json_text(v)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}
#[cfg(windows)]
fn pid_running(pid: u32) -> bool {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetExitCodeProcess(handle: *mut c_void, code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const STILL_ACTIVE: u32 = 259;
    // SAFETY: plain Win32 calls on a handle owned and closed within this block.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok == 0 || code == STILL_ACTIVE
    }
}
#[cfg(unix)]
fn pid_running(pid: u32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return true;
    };
    // SAFETY: signal 0 only checks whether the process exists.
    if unsafe { kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(3)
}
#[cfg(not(any(unix, windows)))]
fn pid_running(_: u32) -> bool {
    true
}
/// Node `lockIsStale(lockPath)`.
fn lock_is_stale(path: &Path) -> bool {
    let parsed = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes)).ok());
    let Some(parsed) = parsed else {
        return fs::symlink_metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > Duration::from_secs(60));
    };
    match parsed.get("pid") {
        Some(Value::Number(n)) => match n.as_u64().and_then(|pid| u32::try_from(pid).ok()) {
            Some(pid) => !pid_running(pid),
            None => false,
        },
        _ => false,
    }
}
/// Node `lockHeldMessage(lockPath)`.
fn lock_held_message(path: &Path) -> String {
    let parsed = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes)).ok());
    if let Some(data) = parsed.filter(|data| data.get("pid").is_some_and(Value::is_number)) {
        return format!(
            "Dependency manager is locked by process {} since {}. If no dtech-aidlc command is running, delete .aidlc/deps.lock.",
            data["pid"],
            js_string(data.get("at"))
        );
    }
    "Dependency manager is locked by another process".into()
}
/// Node `withLock(root, action)`: `.aidlc/deps.lock` holds `{pid, at}` while the
/// action runs and is always removed afterwards.
fn lock<T>(p: &Paths, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let open = || {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&p.lock)
    };
    let mut fd = match open() {
        Ok(fd) => fd,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !lock_is_stale(&p.lock) {
                return Err(lock_held_message(&p.lock));
            }
            let _ = fs::remove_file(&p.lock);
            match open() {
                Ok(fd) => fd,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(lock_held_message(&p.lock))
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(e) => return Err(e.to_string()),
    };
    let result = writeln!(
        fd,
        "{}",
        json!({"pid": std::process::id(), "at": crate::audit::now_iso()})
    )
    .map_err(|e| e.to_string())
    .and_then(|()| f());
    drop(fd);
    let _ = fs::remove_file(&p.lock);
    result
}
/// Node `scrub(text, url)`: hide the repository URL and any `https://user:pass@`.
fn scrub(text: &str, url: Option<&str>) -> String {
    let value = match url {
        Some(url) if !url.is_empty() => text.replace(url, "[repository URL]"),
        _ => text.to_owned(),
    };
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < value.len() {
        let rest = &value[i..];
        if rest.len() >= 8 && rest.as_bytes()[..8].eq_ignore_ascii_case(b"https://") {
            let run_end = rest[8..]
                .find(|c: char| is_js_space(c) || c == '/' || c == '@')
                .map_or(rest.len(), |n| n + 8);
            let run = &rest[8..run_end];
            let has_credentials = rest[run_end..].starts_with('@')
                && run
                    .char_indices()
                    .any(|(at, c)| c == ':' && at > 0 && at + 1 < run.len());
            if has_credentials {
                out.push_str(&rest[..8]);
                out.push_str("[credentials]@");
                i += run_end + 1;
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}
fn git(p: &Paths, args: &[&str], url: Option<&str>) -> Result<String, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = p
        .aidlc
        .join(format!(".git-template-{}-{unique:x}", std::process::id()));
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
        .stdin(Stdio::null())
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let outcome = run_with_timeout(&mut c);
    let _ = fs::remove_dir_all(&tmp);
    let (status, stdout, stderr) = match outcome {
        Ok(Some(done)) => done,
        Ok(None) => return err("git command timed out"),
        Err(e) => return Err(scrub(&format!("spawnSync git {e}"), url).trim().to_owned()),
    };
    if stdout.len() > MAX_GIT_OUTPUT || stderr.len() > MAX_GIT_OUTPUT {
        return err("spawnSync git ENOBUFS");
    }
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        let message = if stderr.is_empty() {
            match status.code() {
                Some(code) => format!("git exited {code}"),
                None => "git exited null".to_owned(),
            }
        } else {
            stderr.into_owned()
        };
        return Err(js_trim(&scrub(&message, url)).to_owned());
    }
    Ok(js_trim(&String::from_utf8_lossy(&stdout)).to_owned())
}
type GitOutput = (std::process::ExitStatus, Vec<u8>, Vec<u8>);
/// Run a command, killing it after [`GIT_TIMEOUT`] (`Ok(None)`), like Node's
/// `spawnSync(..., { timeout })`.
fn run_with_timeout(c: &mut Command) -> std::io::Result<Option<GitOutput>> {
    let mut child = c.spawn()?;
    let mut out = child.stdout.take();
    let mut errs = child.stderr.take();
    let read_out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = out.as_mut() {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let read_err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = errs.as_mut() {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + GIT_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = read_out.join().unwrap_or_default();
    let stderr = read_err.join().unwrap_or_default();
    Ok(status.map(|status| (status, stdout, stderr)))
}
fn sorted(v: &Value) -> Vec<String> {
    let mut a: Vec<_> = v["repos"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    a.sort();
    a
}
/// Node `recordFrom(paths, name, entry, commit, status)`.
fn record(p: &Paths, n: &str, e: &Value, status: Option<&str>, commit: Option<Value>) -> Value {
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
        m.insert("commit".into(), c);
    }
    Value::Object(m)
}
pub fn project_dependencies_path(r: &Path) -> Result<PathBuf, String> {
    Ok(root(r)?.join("aidlc.deps.json"))
}
/// Node `resolve(file)`.
fn resolve_file(f: &Path) -> Result<PathBuf, String> {
    let p = if f.is_absolute() {
        f.to_path_buf()
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(f)
    };
    Ok(normalize(&p))
}
/// Node `readDeclaredManifest(file)`.
fn declared(f: &Path) -> Result<Value, String> {
    let p = resolve_file(f)?;
    let missing = || format!("Dependency file is missing or unsafe: {}", p.display());
    if !exists(&p) {
        return Err(missing());
    }
    let m = fs::symlink_metadata(&p).map_err(|_| missing())?;
    if m.file_type().is_symlink() || !m.is_file() {
        return Err(missing());
    }
    let v = parse_manifest(&p)?;
    valid_manifest(&v)?;
    Ok(v)
}
pub fn import_dependencies(r: &Path, f: &Path) -> Result<Value, String> {
    let source = resolve_file(f)?;
    let d = declared(&source)?;
    let p = paths(r, true)?;
    lock(&p, || {
        let old = manifest(&p)?;
        let dropped: Vec<_> = sorted(&old)
            .into_iter()
            .filter(|n| !d["repos"].as_object().unwrap().contains_key(n))
            .collect();
        write_atomic(&p.manifest, &d, 0o600)?;
        Ok(json!({"file":source,"repos":sorted(&d),"dropped":dropped}))
    })
}
/// Node `applyToProjectFile(root, mutate)`: rewrite `aidlc.deps.json` when it exists.
fn apply_to_project_file(
    r: &Path,
    mutate: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<Option<Value>, String> {
    let dp = project_dependencies_path(r)?;
    if !exists(&dp) {
        return Ok(None);
    }
    let mut data = declared(&dp)?;
    mutate(&mut data)?;
    write_atomic(&dp, &data, 0o644)?;
    Ok(Some(data))
}
fn same_entry(old: &Value, u: &str, rf: Option<&str>) -> bool {
    old["url"].as_str() == Some(u)
        && match old.get("ref") {
            None => rf.is_none(),
            Some(Value::String(s)) => rf == Some(s.as_str()),
            Some(_) => false,
        }
}
fn new_entry(u: &str, rf: Option<&str>) -> Value {
    let mut e = json!({"url":u});
    if let Some(x) = rf {
        e["ref"] = json!(x)
    }
    e
}
pub fn add_dependency(r: &Path, n: &str, u: &str, rf: Option<&str>) -> Result<Value, String> {
    check_name(n)?;
    validate_url(u)?;
    check_ref(rf)?;
    let p = paths(r, true)?;
    lock(&p, || {
        let conflict = || format!("Dependency {n} already exists with a different configuration");
        let declared = apply_to_project_file(r, |data| {
            let repos = data["repos"].as_object_mut().unwrap();
            if let Some(old) = repos.get(n) {
                if same_entry(old, u, rf) {
                    return Ok(());
                }
                return Err(conflict());
            }
            repos.insert(n.into(), new_entry(u, rf));
            Ok(())
        })?;
        let v = match declared {
            Some(v) => v,
            None => {
                let mut v = manifest(&p)?;
                let repos = v["repos"].as_object_mut().unwrap();
                if let Some(old) = repos.get(n) {
                    if same_entry(old, u, rf) {
                        return Ok(record(&p, n, old, None, None));
                    }
                    return Err(conflict());
                }
                repos.insert(n.into(), new_entry(u, rf));
                v
            }
        };
        write_atomic(&p.manifest, &v, 0o600)?;
        Ok(record(&p, n, &v["repos"][n], None, None))
    })
}
/// Read `.aidlc/deps-lock.json` leniently (Node: unreadable or invalid is `{}`).
fn read_lock_lenient(p: &Paths) -> Value {
    if !exists(&p.lockfile) {
        return json!({});
    }
    fs::read(&p.lockfile)
        .ok()
        .and_then(|bytes| serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok())
        .unwrap_or_else(|| json!({}))
}
pub fn list_dependencies(r: &Path) -> Result<Vec<Value>, String> {
    let p = paths(r, false)?;
    let v = manifest(&p)?;
    let lock = read_lock_lenient(&p);
    let mut rows = Vec::new();
    for (n, e) in v["repos"].as_object().unwrap() {
        let target = p.repos.join(n);
        let mut status = "not-cloned";
        let mut commit = None;
        if exists(&target) {
            let unsafe_link = fs::symlink_metadata(&target)
                .map(|m| m.file_type().is_symlink())
                .and_then(|link| {
                    Ok(link
                        || fs::symlink_metadata(target.join(".git"))?
                            .file_type()
                            .is_symlink())
                });
            status = match unsafe_link {
                Ok(true) => "unsafe",
                Ok(false) => match git_path(&target).and_then(|path| {
                    git(&p, &["-C", &path, "rev-parse", "HEAD"], e["url"].as_str())
                }) {
                    Ok(head) => {
                        commit = Some(json!(head));
                        "cloned"
                    }
                    Err(_) => "invalid",
                },
                Err(_) => "invalid",
            };
        }
        let locked = lock
            .get("repos")
            .and_then(|repos| repos.get(n))
            .and_then(|row| row.get("commit"))
            .filter(|c| !c.is_null())
            .cloned();
        rows.push(record(&p, n, e, Some(status), locked.or(commit)));
    }
    Ok(rows)
}
/// Node `requireSafeCheckout(paths, name, expectedUrl)`: `Ok(None)` when absent.
fn safe_checkout(p: &Paths, n: &str, url: &str) -> Result<Option<PathBuf>, String> {
    let target = p.repos.join(n);
    if !exists(&target) {
        return Ok(None);
    }
    let meta = fs::symlink_metadata(&target).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return err(format!("Refusing unmanaged dependency path: {n}"));
    }
    let gd = target.join(".git");
    let unmanaged = || format!("Refusing unmanaged dependency checkout: {n}");
    if !exists(&gd) {
        return Err(unmanaged());
    }
    let gm = fs::symlink_metadata(&gd).map_err(|_| unmanaged())?;
    if gm.file_type().is_symlink() || !gm.is_dir() {
        return Err(unmanaged());
    }
    let actual = target.canonicalize().map_err(|e| e.to_string())?;
    let base = p.repos.canonicalize().map_err(|e| e.to_string())?;
    if !actual.starts_with(&base) || actual == base {
        return err(format!(
            "Dependency checkout escapes managed directory: {n}"
        ));
    }
    let shown = git_path(&target)?;
    let origin = git(
        p,
        &[
            "-C",
            &shown,
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
    let top = git(p, &["-C", &shown, "rev-parse", "--show-toplevel"], None)?;
    let top = Path::new(&top).canonicalize().map_err(|e| e.to_string())?;
    if top != actual {
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
fn checkout_git(
    p: &Paths,
    target: &Path,
    args: &[&str],
    url: Option<&str>,
) -> Result<String, String> {
    let path = git_path(target)?;
    let mut full = vec!["-C", path.as_str()];
    full.extend_from_slice(args);
    git(p, &full, url)
}
/// Node: immutable object IDs and `refs/tags/` refs are pins.
fn pinned_ref(r: &str) -> bool {
    ((40..=64).contains(&r.len()) && r.bytes().all(|b| b.is_ascii_hexdigit()))
        || r.starts_with("refs/tags/")
}
/// Node `desiredCommit(paths, target, ref, url)`.
fn desired_commit(p: &Paths, target: &Path, rf: Option<&str>, url: &str) -> Result<String, String> {
    let url = Some(url);
    let fetch_head = |what: &str| -> Result<String, String> {
        checkout_git(
            p,
            target,
            &[
                "fetch",
                "--no-recurse-submodules",
                "--no-tags",
                "origin",
                what,
            ],
            url,
        )?;
        checkout_git(
            p,
            target,
            &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
            url,
        )
    };
    match rf {
        Some(r) if pinned_ref(r) => checkout_git(
            p,
            target,
            &["rev-parse", "--verify", &format!("{r}^{{commit}}")],
            url,
        )
        .or_else(|_| fetch_head(r)),
        Some(r) => fetch_head(r),
        None => fetch_head("HEAD"),
    }
}
/// Node `installCheckout(paths, name, url, ref)`.
fn install_checkout(p: &Paths, n: &str, e: &Value) -> Result<String, String> {
    let url = e["url"].as_str().ok_or("Invalid repository URL")?;
    let rf = e.get("ref").and_then(Value::as_str);
    let existing = safe_checkout(p, n, url)?;
    if !exists(&p.repos) {
        fs::create_dir_all(&p.repos).map_err(|e| e.to_string())?;
        private(&p.repos);
    }
    let target = p.repos.join(n);
    if let Some(target) = existing {
        let status = checkout_git(
            p,
            &target,
            &["status", "--porcelain", "--untracked-files=all"],
            None,
        )?;
        if !status.is_empty() {
            return err("Checkout has dirty or untracked changes; refusing to update");
        }
        let current = checkout_git(p, &target, &["rev-parse", "HEAD"], None)?;
        let desired = desired_commit(p, &target, rf, url)?;
        if current == desired {
            return Ok(current);
        }
        if checkout_git(
            p,
            &target,
            &["merge-base", "--is-ancestor", &current, &desired],
            Some(url),
        )
        .is_err()
        {
            return err("Update is not a fast-forward; refusing to change checkout");
        }
        if rf.is_some_and(pinned_ref) {
            return Ok(current);
        }
        checkout_git(
            p,
            &target,
            &["checkout", "--detach", "--no-recurse-submodules", &desired],
            Some(url),
        )?;
        return Ok(desired);
    }
    if exists(&target) {
        return err(format!("Refusing to overwrite dependency path: {n}"));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = p
        .repos
        .join(format!(".tmp-{n}-{}-{stamp:x}", std::process::id()));
    fs::create_dir(&temp).map_err(|e| e.to_string())?;
    private(&temp);
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
        let commit = match rf {
            Some(r) => checkout_git(
                p,
                &temp,
                &["rev-parse", "--verify", &format!("{r}^{{commit}}")],
                Some(url),
            )
            .or_else(|_| {
                checkout_git(
                    p,
                    &temp,
                    &["fetch", "--no-recurse-submodules", "--no-tags", "origin", r],
                    Some(url),
                )?;
                checkout_git(
                    p,
                    &temp,
                    &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
                    Some(url),
                )
            })?,
            None => checkout_git(
                p,
                &temp,
                &["rev-parse", "--verify", "HEAD^{commit}"],
                Some(url),
            )?,
        };
        checkout_git(
            p,
            &temp,
            &["checkout", "--detach", "--no-recurse-submodules", &commit],
            Some(url),
        )?;
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        Ok(commit)
    })();
    if result.is_err() {
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
        // Validate every registered path before making network or filesystem changes.
        for n in &names {
            safe_checkout(&p, n, v["repos"][n]["url"].as_str().unwrap_or(""))?;
        }
        let invalid = "Invalid dependency lock file";
        let mut l = if exists(&p.lockfile) {
            let bytes = fs::read(&p.lockfile).map_err(|_| invalid)?;
            serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes)).map_err(|_| invalid)?
        } else {
            json!({"schemaVersion":1,"repos":{}})
        };
        if l["schemaVersion"].as_i64() != Some(1)
            || !(l["repos"].is_object() || l["repos"].is_array())
        {
            return err(invalid);
        }
        let mut rows = Vec::new();
        for n in names {
            let url = v["repos"][&n]["url"].as_str().map(str::to_owned);
            match install_checkout(&p, &n, &v["repos"][&n]) {
                Ok(commit) => {
                    // An array `repos` keeps only index properties in JSON, as in Node.
                    if let Some(repos) = l["repos"].as_object_mut() {
                        repos.insert(n.clone(), json!({"commit":commit}));
                    }
                    rows.push(json!({"name":n,"ok":true,"commit":commit,"path":p.repos.join(&n)}));
                }
                Err(e) => rows.push(json!({"name":n,"ok":false,"error":scrub(&e, url.as_deref())})),
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
        let has_dec = exists(&dp) && declared(&dp)?["repos"].get(n).is_some();
        let mut run = manifest(&p)?;
        let has_run = run["repos"].get(n).is_some();
        if !has_run && !has_dec {
            return err(format!("Unknown dependency: {n}"));
        }
        if has_dec {
            apply_to_project_file(r, |data| {
                data["repos"].as_object_mut().unwrap().shift_remove(n);
                Ok(())
            })?;
        }
        run["repos"].as_object_mut().unwrap().shift_remove(n);
        write_atomic(&p.manifest, &run, 0o600)?;
        if exists(&p.lockfile) {
            // Node leaves an unparseable (or unwritable) lock file alone.
            let parsed = fs::read(&p.lockfile)
                .ok()
                .and_then(|b| serde_json::from_str::<Value>(&String::from_utf8_lossy(&b)).ok());
            if let Some(mut l) = parsed {
                if l["schemaVersion"].as_i64() == Some(1) && l["repos"].is_object() {
                    l["repos"].as_object_mut().unwrap().shift_remove(n);
                    let _ = write_atomic(&p.lockfile, &l, 0o600);
                }
            }
        }
        Ok(json!({"name":n,"removed":true}))
    })
}
pub fn validate_dependencies(r: &Path) -> Result<Vec<Value>, String> {
    let p = paths(r, false)?;
    let v = manifest(&p)?;
    let mut issues = Vec::new();
    for (n, e) in v["repos"].as_object().unwrap() {
        let url = e["url"].as_str().unwrap_or("");
        if let Err(error) = safe_checkout(&p, n, url) {
            issues.push(format!("{n}: {}", scrub(&error, Some(url))));
        }
    }
    if !issues.is_empty() {
        return err(format!(
            "Dependency validation failed: {}",
            issues.join("; ")
        ));
    }
    Ok(v["repos"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(n, e)| {
            let status = if exists(&p.repos.join(n)) {
                "cloned"
            } else {
                "not-cloned"
            };
            record(&p, n, e, Some(status), None)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Command handlers (Node router in core/tools/dtech-aidlc.mjs).
// ---------------------------------------------------------------------------

fn string_flag<'a>(flags: &'a BTreeMap<String, FlagValue>, name: &str) -> Option<&'a str> {
    match flags.get(name) {
        Some(FlagValue::String(value)) => Some(value),
        _ => None,
    }
}

/// Node `dtech-aidlc repos add|list|pull|remove`.
///
/// `key` is the command key (`"repos add"`, `"repos list"`, `"repos pull"`,
/// `"repos remove"`; any other `"repos ..."` key fails like Node) and `args` are the
/// positional words after it. Returns the JSON Node prints and its exit code:
/// `repos pull` exits 1 when any repository failed (the result list is still printed).
/// `repos pull` also writes Node's `dtech-aidlc: note: removed from the runtime
/// copy ...` line to standard error itself when declared entries were dropped.
/// Audit rows `repos.added`, `repos.pulled` and `repos.removed` are appended as in
/// Node.
pub fn run_repos(
    key: &str,
    project: &Path,
    args: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let action = key.split(' ').nth(1);
    match action {
        Some("add") => {
            if args.len() != 2 {
                return err("Usage: repos add <name> <url> [--ref REF]");
            }
            let rf = string_flag(flags, "ref");
            // Each write below is followed by an audit row; refuse a log that cannot be
            // appended to before anything changes (Node writes, then fails on the append).
            crate::audit::preflight_audit(&root)?;
            let added = add_dependency(&root, &args[0], &args[1], rf)?;
            crate::audit::audit(
                &root,
                "repos.added",
                json!({"name": added["name"], "ref": rf}),
            )?;
            Ok((added, 0))
        }
        Some("list") if args.is_empty() => Ok((Value::Array(list_dependencies(&root)?), 0)),
        Some("pull") if args.len() <= 1 => {
            let file = string_flag(flags, "file");
            let source = match file {
                Some(file) => PathBuf::from(file),
                None => project_dependencies_path(&root)?,
            };
            let label = file.unwrap_or("aidlc.deps.json");
            crate::audit::preflight_audit(&root)?;
            if file.is_some() || exists(&source) {
                let imported = import_dependencies(&root, &source)?;
                let dropped: Vec<&str> = imported["dropped"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if !dropped.is_empty() {
                    eprintln!(
                        "dtech-aidlc: note: removed from the runtime copy because they are not declared in {label}: {}. Add them to {label} to keep them.",
                        dropped.join(", ")
                    );
                }
            }
            let results = pull_dependencies(&root, args.first().map(String::as_str))?;
            let rows: Vec<Value> = results
                .iter()
                .map(|row| {
                    json!({
                        "name": row["name"],
                        "ok": row["ok"],
                        "commit": row.get("commit").cloned().unwrap_or(Value::Null)
                    })
                })
                .collect();
            crate::audit::audit(&root, "repos.pulled", json!({"results": rows}))?;
            let failed = results.iter().any(|row| row["ok"] != true);
            Ok((Value::Array(results), if failed { 1 } else { 0 }))
        }
        Some("remove") if args.len() == 1 => {
            crate::audit::preflight_audit(&root)?;
            let removed = remove_dependency(&root, &args[0])?;
            crate::audit::audit(&root, "repos.removed", json!({"name": removed["name"]}))?;
            Ok((removed, 0))
        }
        _ => err("Use repos add <name> <url> [--ref REF], repos list, repos pull [name], or repos remove <name>"),
    }
}

/// Node `safeId(value, label)`.
fn safe_id<'a>(value: Option<&'a str>, label: &str) -> Result<&'a str, String> {
    match value {
        Some(v)
            if !v.is_empty()
                && v.len() <= 80
                && v.as_bytes()[0].is_ascii_alphanumeric()
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) =>
        {
            Ok(v)
        }
        _ => err(format!(
            "Invalid {label}: use letters, digits, dot, underscore, or hyphen"
        )),
    }
}

/// Node `dtech-aidlc deps add|list|pull` (dependency edges in the active intent).
///
/// `key` is `"deps add"`, `"deps list"` or `"deps pull"` (any other `"deps ..."`
/// key fails like Node, after the workflow is loaded); `args` are the positional
/// words after it. `deps add` migrates legacy state first (Node `migrate: true`),
/// persists the intent and pointer, and appends a `dependency.added` audit row.
pub fn run_deps(
    key: &str,
    project: &Path,
    args: &[String],
    _flags: &BTreeMap<String, FlagValue>,
) -> Result<(Value, i32), String> {
    let root = std::path::absolute(project).map_err(|error| error.to_string())?;
    crate::audit::assert_safe_state_dir(&root)?;
    let action = key.split(' ').nth(1);
    let graph = crate::ids::load_runtime_graph(&root)?;
    let loaded = crate::workflow::load_workflow(&root, &graph, action == Some("add"))?;
    let Some(mut state) = loaded.state else {
        return err("Initialize a workflow before managing dependencies");
    };
    if matches!(state.get("dependencies"), None | Some(Value::Null)) {
        state["dependencies"] = json!({});
    }
    let arg = |index: usize| args.get(index).map(String::as_str);
    match action {
        Some("add") => {
            if arg(0).is_none_or(str::is_empty) || arg(1).is_none_or(str::is_empty) {
                return err("Usage: deps add <id> <dependency-id>");
            }
            let id = safe_id(arg(0), "item id")?;
            let dependency = safe_id(arg(1), "dependency id")?;
            if id == dependency {
                return err("An item cannot depend on itself");
            }
            let deps = state["dependencies"]
                .as_object_mut()
                .ok_or("dependencies is not an object of string arrays")?;
            let edges = deps.entry(id.to_owned()).or_insert_with(|| json!([]));
            let list = edges
                .as_array_mut()
                .ok_or("dependencies is not an object of string arrays")?;
            if !list.iter().any(|edge| edge == dependency) {
                list.push(json!(dependency));
            }
            let edges = edges.clone();
            // Node `persistWorkflow`: save the intent, then rewrite the pointer.
            let intent_id = state
                .get("intentId")
                .and_then(Value::as_str)
                .ok_or("intentId is not a safe identifier")?
                .to_owned();
            crate::audit::preflight_audit(&root)?;
            crate::workflow::save_intent(&root, &mut state, Some(&graph))?;
            crate::workflow::save_pointer(&root, Some(&intent_id))?;
            crate::audit::audit(
                &root,
                "dependency.added",
                json!({"id": id, "dependency": dependency}),
            )?;
            Ok((json!({"id": id, "dependencies": edges}), 0))
        }
        Some("list") => {
            let rows: Vec<Value> = state["dependencies"]
                .as_object()
                .into_iter()
                .flatten()
                .flat_map(|(id, deps)| {
                    deps.as_array()
                        .into_iter()
                        .flatten()
                        .map(move |dependency| json!({"id": id, "dependency": dependency}))
                })
                .collect();
            Ok((Value::Array(rows), 0))
        }
        Some("pull") => {
            let id = safe_id(arg(0), "item id")?;
            let deps = state["dependencies"]
                .get(id)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let stages = state.get("stages").cloned().unwrap_or_else(|| json!({}));
            let rows: Vec<Value> = deps
                .iter()
                .map(|dependency| {
                    let mut row = Map::new();
                    row.insert("id".into(), dependency.clone());
                    let key = dependency.as_str().unwrap_or("");
                    match stages.get(key) {
                        Some(stage) if stages.is_object() => {
                            // `stages[id].status` may be undefined, which JSON omits.
                            if let Some(status) = stage.get("status") {
                                row.insert("status".into(), status.clone());
                            }
                        }
                        _ => {
                            row.insert("status".into(), json!("unknown"));
                        }
                    }
                    Value::Object(row)
                })
                .collect();
            Ok((json!({"id": id, "dependencies": rows}), 0))
        }
        _ => err("Use deps add, deps list, or deps pull"),
    }
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
