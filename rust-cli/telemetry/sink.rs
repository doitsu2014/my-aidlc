//! Sink producer (never blocks, never panics) and the FileStore writer thread.

use super::{file_name, is_store_file, store_state, Event, StoreState, MAX_LINE};
use std::fs;
use std::io::{self, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PART_MAX: u64 = 5 * 1024 * 1024;
pub const TOTAL_MAX: u64 = 50 * 1024 * 1024;
pub const MAX_AGE: Duration = Duration::from_secs(7 * 86_400);
pub const QUEUE: usize = 1024;

const MSG_DROPPED: &str = "dtech-aidlc-mcp: telemetry events dropped (writer queue full)";
const MSG_UNSAFE: &str = "dtech-aidlc-mcp: telemetry disabled (unsafe telemetry location)";
const MSG_WRITE: &str = "dtech-aidlc-mcp: telemetry disabled (write failed)";
const MSG_RANDOM: &str = "dtech-aidlc-mcp: telemetry disabled (random source unavailable)";
const MSG_INTERNAL: &str = "dtech-aidlc-mcp: telemetry disabled (internal error)";

/// Disables once; only the caller that flips the flag prints the line.
fn disable(flag: &AtomicBool, msg: &str) {
    if !flag.swap(true, Ordering::SeqCst) {
        eprintln!("{msg}");
    }
}

/// Payload of the `io::Error` that marks an unsafe telemetry location (symlink or non-dir).
#[derive(Debug)]
struct UnsafeLocation;

impl std::fmt::Display for UnsafeLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unsafe telemetry location")
    }
}

impl std::error::Error for UnsafeLocation {}

fn is_unsafe(e: &io::Error) -> bool {
    e.get_ref().is_some_and(|r| r.is::<UnsafeLocation>())
}

/// The fixed disable line for a consumer error: unsafe location versus ordinary write failure.
fn disable_msg(e: &io::Error) -> &'static str {
    if is_unsafe(e) {
        MSG_UNSAFE
    } else {
        MSG_WRITE
    }
}

pub struct Sink {
    tx: SyncSender<String>,
    session: String,
    counter: AtomicU64,
    disabled: Arc<AtomicBool>,
    dropped_once: AtomicBool,
    done: Mutex<Receiver<()>>,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Sink {
    /// Starts the writer for `root`; `None` (one fixed line) when no random source exists.
    pub fn start(root: &Path) -> Option<Sink> {
        let mut buf = [0u8; 16];
        if super::os_random(&mut buf).is_err() {
            eprintln!("{MSG_RANDOM}");
            return None;
        }
        let session = super::hex(&buf);
        let mut store = FileStore::new(root, &session, now_ms(), PART_MAX);
        Some(Sink::spawn(session, move |line| store.write(&line)))
    }

    /// Same as `start` with an injected consumer. An error carrying `UnsafeLocation` means an
    /// unsafe location; any other error (including EACCES) is a write failure. Both disable.
    pub fn spawn(
        session: String,
        mut consume: impl FnMut(String) -> io::Result<()> + Send + 'static,
    ) -> Sink {
        let (tx, rx) = sync_channel::<String>(QUEUE);
        let (done_tx, done_rx) = sync_channel::<()>(1);
        let disabled = Arc::new(AtomicBool::new(false));
        let flag = disabled.clone();
        let spawned = std::thread::Builder::new()
            .name("telemetry-writer".into())
            .spawn(move || {
                for line in rx {
                    match catch_unwind(AssertUnwindSafe(|| consume(line))) {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => {
                            disable(&flag, disable_msg(&e));
                            break;
                        }
                        Err(_) => {
                            disable(&flag, MSG_INTERNAL);
                            break;
                        }
                    }
                }
                let _ = done_tx.try_send(());
            });
        if spawned.is_err() {
            disable(&disabled, MSG_INTERNAL);
        }
        Sink {
            tx,
            session,
            counter: AtomicU64::new(0),
            disabled,
            dropped_once: AtomicBool::new(false),
            done: Mutex::new(done_rx),
        }
    }

    /// `<sessionId>-<n>`.
    pub fn next_id(&self) -> String {
        format!(
            "{}-{}",
            self.session,
            self.counter.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    /// Never blocks, never panics, never touches the disk.
    pub fn send(&self, build: impl FnOnce() -> Event) {
        if self.disabled.load(Ordering::Relaxed) {
            return;
        }
        let sent = catch_unwind(AssertUnwindSafe(|| self.tx.try_send(build().to_line())));
        match sent {
            Ok(Ok(())) => {}
            Ok(Err(TrySendError::Full(_))) => {
                if !self.dropped_once.swap(true, Ordering::Relaxed) {
                    eprintln!("{MSG_DROPPED}");
                }
            }
            Ok(Err(TrySendError::Disconnected(_))) | Err(_) => {
                disable(&self.disabled, MSG_INTERNAL)
            }
        }
    }

    /// Drops the sender and waits up to `wait` for the writer to drain.
    pub fn finish(self, wait: Duration) {
        let Sink { tx, done, .. } = self;
        drop(tx);
        let done = done.into_inner().unwrap_or_else(|e| e.into_inner());
        let _ = done.recv_timeout(wait);
    }
}

/// The append-only file writer. Lazy: nothing is touched until the first line.
pub struct FileStore {
    root: PathBuf,
    session: String,
    start_ms: i64,
    part_max: u64,
    part: u32,
    open: Option<(fs::File, String, u64)>,
}

impl FileStore {
    pub fn new(root: &Path, session: &str, start_ms: i64, part_max: u64) -> FileStore {
        FileStore {
            root: root.to_path_buf(),
            session: session.to_owned(),
            start_ms,
            part_max,
            part: 0,
            open: None,
        }
    }

    fn dir(&self) -> PathBuf {
        self.root.join(".aidlc").join("telemetry")
    }

    /// Appends one line plus `\n`. `Ok` with nothing written when `.aidlc` is missing;
    /// an `UnsafeLocation` error for an unsafe location.
    pub fn write(&mut self, line: &str) -> io::Result<()> {
        if line.len() > MAX_LINE {
            return Ok(());
        }
        let need = line.len() as u64 + 1;
        if self.open.is_none() {
            match store_state(&self.root) {
                StoreState::Unsafe => {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        UnsafeLocation,
                    ))
                }
                StoreState::Absent => {
                    if fs::symlink_metadata(self.root.join(".aidlc")).is_err() {
                        return Ok(());
                    }
                    create_dir(&self.dir())?;
                }
                StoreState::Dir => {}
            }
            write_self_ignore(&self.dir())?;
            self.open_part()?;
            if let Some((_, name, _)) = &self.open {
                prune(&self.dir(), Some(name));
            }
        } else if let Some((_, _, size)) = &self.open {
            if *size > 0 && *size + need > self.part_max {
                self.part += 1;
                self.open_part()?;
                if let Some((_, name, _)) = &self.open {
                    prune(&self.dir(), Some(name));
                }
            }
        }
        let (file, _, size) = self.open.as_mut().expect("part is open");
        let mut buf = String::with_capacity(line.len() + 1);
        buf.push_str(line);
        buf.push('\n');
        file.write_all(buf.as_bytes())?;
        *size += need;
        Ok(())
    }

    /// Opens the next free part with `create_new`; an existing file is never touched.
    fn open_part(&mut self) -> io::Result<()> {
        loop {
            if self.part > 999 {
                return Err(io::Error::other("telemetry parts exhausted"));
            }
            let name = file_name(self.start_ms, std::process::id(), &self.session, self.part);
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            match options.open(self.dir().join(&name)) {
                Ok(file) => {
                    self.open = Some((file, name, 0));
                    return Ok(());
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => self.part += 1,
                Err(e) => return Err(e),
            }
        }
    }
}

fn create_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    match builder.create(dir) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => Err(e),
        _ => Ok(()),
    }
}

fn write_self_ignore(dir: &Path) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    match options.open(dir.join(".gitignore")) {
        Ok(mut f) => f.write_all(b"*\n"),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn prune(dir: &Path, current: Option<&str>) {
    prune_with(dir, current, SystemTime::now(), MAX_AGE, TOTAL_MAX);
}

/// Deletes pattern-matching regular files (lstat, never symlinks) older than `max_age`, then
/// oldest first until the total is at most `max_total`. Never deletes `current`; delete
/// errors are ignored.
pub fn prune_with(
    dir: &Path,
    current: Option<&str>,
    now: SystemTime,
    max_age: Duration,
    max_total: u64,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, SystemTime, u64, bool)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|n| is_store_file(n)) else {
            continue;
        };
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(now);
        files.push((entry.path(), mtime, meta.len(), Some(name) == current));
    }
    let mut total: u64 = files.iter().map(|f| f.2).sum();
    files.sort_by_key(|f| f.1);
    let mut kept = Vec::new();
    for f in files {
        let old = now.duration_since(f.1).is_ok_and(|age| age > max_age);
        if !f.3 && old && fs::remove_file(&f.0).is_ok() {
            total -= f.2;
        } else {
            kept.push(f);
        }
    }
    for f in kept {
        if total <= max_total {
            break;
        }
        if !f.3 && fs::remove_file(&f.0).is_ok() {
            total -= f.2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{parse_line, EventKind};
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    const SID: &str = "0123456789abcdef0123456789abcdef";

    fn tmp(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn ev(n: u64) -> Event {
        Event {
            kind: EventKind::RequestStarted,
            event_id: format!("{SID}-{n}"),
            timestamp: "2026-01-02T03:04:05.678Z".into(),
            process_id: 1,
            session_id: SID.into(),
            trace_id: format!("{SID}-{n}"),
            span_id: format!("{SID}-{n}"),
            parent_span_id: None,
            name: "status".into(),
            intent_id: None,
            stage_id: None,
            unit_id: None,
            duration_ms: None,
            outcome: None,
            error_code: None,
        }
    }

    #[test]
    fn ts023_stalled_writer_never_blocks_the_producer() {
        let (release_tx, release_rx) = channel::<()>();
        let sink = Sink::spawn(SID.into(), move |_| {
            let _ = release_rx.recv();
            Ok(())
        });
        let start = Instant::now();
        for n in 0..2000 {
            sink.send(|| ev(n));
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        drop(release_tx);
        sink.finish(Duration::from_millis(500));
    }

    fn wait_disabled(sink: &Sink) {
        let t = Instant::now();
        while !sink.disabled.load(Ordering::SeqCst) && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn ts023_panicking_build_is_contained_and_disables() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let sink = Sink::spawn(SID.into(), move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        sink.send(|| panic!("build boom"));
        assert!(sink.disabled.load(Ordering::SeqCst));
        sink.send(|| ev(1));
        sink.finish(Duration::from_millis(500));
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ts023_panicking_consumer_is_contained_and_disables() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let sink = Sink::spawn(SID.into(), move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            panic!("writer boom")
        });
        sink.send(|| ev(1));
        wait_disabled(&sink);
        assert!(sink.disabled.load(Ordering::SeqCst));
        sink.send(|| ev(2));
        sink.send(|| ev(3));
        sink.finish(Duration::from_millis(500));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[cfg(unix)]
    #[test]
    fn ts024_ordinary_eacces_is_a_write_failure_not_an_unsafe_location() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp("ts024-eacces");
        let aidlc = root.join(".aidlc");
        fs::create_dir_all(&aidlc).unwrap();
        fs::set_permissions(&aidlc, fs::Permissions::from_mode(0o500)).unwrap();
        // Root ignores the mode bits; skip then.
        if fs::write(aidlc.join("probe"), b"x").is_err() {
            let mut store = FileStore::new(&root, SID, 0, PART_MAX);
            let err = store.write(&ev(1).to_line()).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
            assert!(!is_unsafe(&err), "{err:?}");
            assert_eq!(disable_msg(&err), MSG_WRITE);
        }
        fs::set_permissions(&aidlc, fs::Permissions::from_mode(0o700)).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn ts024_unsafe_location_is_marked_and_chooses_the_unsafe_message() {
        let root = tmp("ts024-unsafe-msg");
        let outside = tmp("ts024-unsafe-out");
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(".aidlc/telemetry")).unwrap();
        let mut store = FileStore::new(&root, SID, 0, PART_MAX);
        let err = store.write(&ev(1).to_line()).unwrap_err();
        assert!(is_unsafe(&err));
        assert_eq!(disable_msg(&err), MSG_UNSAFE);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn ts023_next_id_is_session_prefixed_and_unique() {
        let sink = Sink::spawn(SID.into(), |_| Ok(()));
        let (a, b) = (sink.next_id(), sink.next_id());
        assert_ne!(a, b);
        assert!(a.starts_with(&format!("{SID}-")));
        assert_eq!(sink.session(), SID);
        sink.finish(Duration::from_millis(500));
    }

    #[test]
    fn ts024_writer_never_creates_aidlc_and_drops_silently() {
        let root = tmp("ts024-absent");
        let mut store = FileStore::new(&root, SID, 0, PART_MAX);
        store.write(&ev(1).to_line()).unwrap();
        assert!(!root.join(".aidlc").exists());
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        store.write(&ev(2).to_line()).unwrap();
        let dir = root.join(".aidlc/telemetry");
        let parts: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| is_store_file(&e.file_name().to_string_lossy()))
            .collect();
        assert_eq!(parts.len(), 1);
        assert_eq!(
            fs::read_to_string(parts[0].path()).unwrap().lines().count(),
            1
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ts024_writer_creates_store_gitignore_and_part_files() {
        let root = tmp("ts024-create");
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let mut store = FileStore::new(&root, SID, 1_767_323_045_678, PART_MAX);
        store.write(&ev(1).to_line()).unwrap();
        let dir = root.join(".aidlc/telemetry");
        assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "*\n");
        let file = dir.join(file_name(1_767_323_045_678, std::process::id(), SID, 0));
        let text = fs::read_to_string(&file).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(parse_line(text.trim_end()).is_some());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        // A second store for the same start and pid takes the next part, never truncating.
        let mut again = FileStore::new(&root, SID, 1_767_323_045_678, PART_MAX);
        again.write(&ev(2).to_line()).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), text);
        assert!(dir
            .join(file_name(1_767_323_045_678, std::process::id(), SID, 1))
            .exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ts024_rotates_when_the_next_line_would_exceed_the_part_limit() {
        let root = tmp("ts024-rotate");
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let line = ev(1).to_line();
        let per = line.len() as u64 + 1;
        let mut store = FileStore::new(&root, SID, 0, per * 2);
        for n in 0..5 {
            store.write(&ev(n).to_line()).unwrap();
        }
        let dir = root.join(".aidlc/telemetry");
        let sizes: Vec<u64> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| is_store_file(&e.file_name().to_string_lossy()))
            .map(|e| e.metadata().unwrap().len())
            .collect();
        assert_eq!(sizes.len(), 3);
        assert!(sizes.iter().all(|s| *s <= per * 2));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn ts024_symlinked_store_is_refused_and_nothing_is_written_through_it() {
        let root = tmp("ts024-symlink");
        let outside = tmp("ts024-outside");
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(".aidlc/telemetry")).unwrap();
        let mut store = FileStore::new(&root, SID, 0, PART_MAX);
        let err = store.write(&ev(1).to_line()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    fn touch(dir: &Path, name: &str, len: usize, age_days: u64, now: SystemTime) {
        let p = dir.join(name);
        fs::write(&p, vec![b'x'; len]).unwrap();
        let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_modified(now - Duration::from_secs(age_days * 86_400))
            .unwrap();
    }

    #[test]
    fn ts024_prune_by_age_then_size_never_touches_current_or_unrelated_files() {
        let dir = tmp("ts024-prune");
        let now = SystemTime::now();
        let name = |h: &str, p: u32| format!("mcp-20260102T030405Z-1-{h}-{p:03}.jsonl");
        touch(&dir, &name("aaaaaaaa", 0), 10, 8, now); // too old
        touch(&dir, &name("bbbbbbbb", 0), 100, 3, now); // oldest of the kept
        touch(&dir, &name("cccccccc", 0), 100, 2, now);
        touch(&dir, &name("dddddddd", 0), 100, 1, now);
        touch(&dir, &name("eeeeeeee", 0), 100, 30, now); // current, old but protected
        touch(&dir, "notes.txt", 100, 90, now);
        touch(&dir, ".gitignore", 2, 90, now);
        let current = name("eeeeeeee", 0);
        prune_with(&dir, Some(&current), now, MAX_AGE, 250);
        let left = |n: &str| dir.join(n).exists();
        assert!(!left(&name("aaaaaaaa", 0)));
        assert!(!left(&name("bbbbbbbb", 0)));
        assert!(!left(&name("cccccccc", 0)));
        assert!(left(&name("dddddddd", 0)));
        assert!(left(&current));
        assert!(left("notes.txt") && left(".gitignore"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn ts024_prune_skips_symlinks_named_like_store_files() {
        let dir = tmp("ts024-prune-link");
        let outside = tmp("ts024-prune-out");
        fs::write(outside.join("victim"), b"keep").unwrap();
        let link = dir.join("mcp-20260102T030405Z-1-aaaaaaaa-000.jsonl");
        std::os::unix::fs::symlink(outside.join("victim"), &link).unwrap();
        prune_with(
            &dir,
            None,
            SystemTime::now() + Duration::from_secs(40 * 86_400),
            MAX_AGE,
            0,
        );
        assert!(outside.join("victim").exists());
        assert!(fs::symlink_metadata(&link).is_ok());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn ts023_sink_start_writes_through_the_real_store_and_finish_drains() {
        let root = tmp("ts023-start");
        fs::create_dir_all(root.join(".aidlc")).unwrap();
        let sink = Sink::start(&root).unwrap();
        for n in 1..=3 {
            let id = sink.next_id();
            let session = sink.session().to_owned();
            sink.send(|| {
                let mut e = ev(n);
                e.session_id = session.clone();
                e.event_id = id.clone();
                e.trace_id = id.clone();
                e.span_id = id;
                e
            });
        }
        sink.finish(Duration::from_millis(500));
        let dir = root.join(".aidlc/telemetry");
        let mut lines = 0;
        for e in fs::read_dir(&dir).unwrap().flatten() {
            if is_store_file(&e.file_name().to_string_lossy()) {
                for l in fs::read_to_string(e.path()).unwrap().lines() {
                    assert!(parse_line(l).is_some(), "{l}");
                    lines += 1;
                }
            }
        }
        assert_eq!(lines, 3);
        let _ = fs::remove_dir_all(&root);
    }
}
