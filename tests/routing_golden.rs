//! Binary-level routing tests: the built `dtech-aidlc` executable against golden
//! snapshots recorded from the former JavaScript reference CLI.
//!
//! Every scenario builds a fixture project (`<base>/proj`), runs the binary on it and,
//! after each step, compares stdout, stderr, the exit code and every file and directory
//! of the project (`.git` directories and the embedded engine excepted) with the step
//! recorded in `tests/golden/routing_golden/<scenario>.json`. The files a step
//! changes are stored as a delta (new normalised content, or `null` for a removed
//! entry). Only the project root, the engine and repository directories, the shared git
//! remote, ISO-8601 timestamps and `mtimeMs` values are normalised (path separators
//! after a placeholder read as `/`); everything else, including JSON formatting and key
//! order, must match byte for byte.
//!
//! Onboarded scenarios start from the recorded reference onboarding output
//! (`fixture-onboarded-<harness>.json`), restored file by file, because Rust onboarding
//! output is not byte-identical to it (see
//! `onboarding_success_writes_the_same_configuration_and_knowledge_files_as_the_golden`).
//!
//! Each command key has at least one success and one error case, and every key is also
//! checked for the unknown-option and extra-argument errors. The `repos pull` clone case
//! needs `git`.
//!
//! `DTECH_AIDLC_BLESS=1` rewrites the scenario goldens from the binary. Onboarding
//! fixtures are kept; delete one to re-record it from the binary's own onboarding.

#[path = "common/golden.rs"]
mod golden;

use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const SUITE: &str = "routing_golden";

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dtech-aidlc-routing-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repo() -> PathBuf {
    golden::repo()
}

fn null_device() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

fn available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

#[derive(Debug)]
struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

impl Run {
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {self:?}"))
    }
}

/// How the engine (graph, methodology) is found.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Engine {
    /// `DTECH_AIDLC_HOME` names this repository, so directive paths name the
    /// methodology files under `core/`.
    Home,
    /// No `DTECH_AIDLC_HOME`: the binary uses its embedded copy (materialised under
    /// `<project>/.aidlc/engine`). Engine paths are compared from the engine directory
    /// on and `.aidlc/engine/` is left out of the file comparison.
    Embedded,
}

/// The CLI under test.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
}

/// Runs the CLI with `--project <root>`. Git reads no system or global configuration
/// and sees a fixed `user.name`, so approver identities do not depend on the machine.
fn run_cli(root: &Path, argv: &[&str], engine: Engine) -> Run {
    let cwd = if root.is_dir() {
        root.to_path_buf()
    } else {
        root.parent().unwrap().to_path_buf()
    };
    let mut command = cli();
    command
        .args(argv)
        .arg("--project")
        .arg(root)
        .current_dir(cwd)
        .env_remove("DTECH_AIDLC_HOME")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "user.name")
        .env("GIT_CONFIG_VALUE_0", "Routing Tester");
    if engine == Engine::Home {
        command.env("DTECH_AIDLC_HOME", repo());
    }
    let output = command.stdin(Stdio::null()).output().expect("run CLI");
    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        code: output.status.code().unwrap_or(-1),
    }
}

/// Every file and directory under `root` (relative, `/`-separated; directories end in
/// `/`), with file contents passed through `content`. `.git` directories (repository
/// checkouts) and the embedded engine are skipped.
fn walk_tree(root: &Path, content: &dyn Fn(&Path, Vec<u8>) -> String) -> BTreeMap<String, String> {
    fn walk(
        root: &Path,
        dir: &Path,
        content: &dyn Fn(&Path, Vec<u8>) -> String,
        into: &mut BTreeMap<String, String>,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                if entry.file_name() == ".git" || rel == ".aidlc/engine" {
                    continue;
                }
                into.insert(format!("{rel}/"), String::new());
                walk(root, &path, content, into);
            } else if kind.is_symlink() {
                into.insert(rel, "<symlink>".into());
            } else {
                let bytes = fs::read(&path).unwrap();
                into.insert(rel, content(&path, bytes));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, content, &mut files);
    files
}

/// The entries of `after` that are new or changed relative to `before` (with their
/// content) and the removed ones (`null`).
fn delta(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Value {
    let mut changes = Map::new();
    for (key, value) in after {
        if before.get(key) != Some(value) {
            changes.insert(key.clone(), json!(value));
        }
    }
    for key in before.keys() {
        if !after.contains_key(key) {
            changes.insert(key.clone(), Value::Null);
        }
    }
    Value::Object(changes)
}

/// The recorded steps of one scenario.
struct Session {
    name: String,
    expected: Vec<Value>,
    recorded: Vec<Value>,
    cursor: usize,
}

/// One project fixture driven by the binary, checked step by step against a golden.
struct Pair {
    _base: TempDir,
    root: PathBuf,
    engine: Engine,
    remote: Option<PathBuf>,
    session: RefCell<Session>,
}

impl Pair {
    /// `setup` prepares the project; `create` decides whether the project folder exists.
    /// `label` names the golden file.
    fn new(label: &str, create: bool, setup: impl Fn(&Path)) -> Self {
        let base = TempDir::new(label);
        let root = base.0.join("proj");
        if create {
            fs::create_dir_all(&root).unwrap();
        } else {
            fs::create_dir_all(root.parent().unwrap()).unwrap();
        }
        setup(&root);
        let expected = if golden::blessing() {
            Vec::new()
        } else {
            golden::read(SUITE, label)["steps"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| panic!("golden {label} has no steps array"))
        };
        Pair {
            _base: base,
            root,
            engine: Engine::Home,
            remote: None,
            session: RefCell::new(Session {
                name: label.to_owned(),
                expected,
                recorded: Vec::new(),
                cursor: 0,
            }),
        }
    }

    fn with_engine(mut self, engine: Engine) -> Self {
        self.engine = engine;
        self
    }

    /// Names a git remote outside the project whose path is normalised to `<REMOTE>`.
    fn with_remote(mut self, remote: PathBuf) -> Self {
        self.remote = Some(remote);
        self
    }

    /// The engine directory the CLI reports paths under.
    fn engine_dir(&self) -> PathBuf {
        match self.engine {
            Engine::Home => repo().join("core"),
            Engine::Embedded => self.root.join(".aidlc").join("engine"),
        }
    }

    fn empty(label: &str) -> Self {
        Self::new(label, true, |_| {})
    }

    /// A project with source and documentation files in the onboarded state recorded
    /// from the reference CLI, so every later step starts from the recorded state.
    fn onboarded(label: &str, harness: &str) -> Self {
        Self::new(label, true, |root| {
            seed_project(root);
            restore_onboarded(root, harness);
        })
    }

    /// Normalises the engine, repository, remote and project paths, timestamps and
    /// `mtimeMs` values.
    fn normalise(&self, text: &str) -> String {
        let engine = self.engine_dir();
        let repo = repo();
        let mut paths: Vec<(&Path, &str)> = vec![(&engine, "<ENGINE>"), (&repo, "<REPO>")];
        if let Some(remote) = &self.remote {
            paths.push((remote, "<REMOTE>"));
        }
        paths.push((&self.root, "<ROOT>"));
        golden::normalise(text, &paths)
    }

    /// Every file and directory of the project with normalised contents.
    fn snapshot(&self) -> BTreeMap<String, String> {
        walk_tree(&self.root, &|_, bytes| {
            self.normalise(&String::from_utf8_lossy(&bytes))
        })
    }

    /// Applies a change to the project.
    fn both(&self, change: impl Fn(&Path)) {
        change(&self.root);
    }

    /// Records the step when blessing, otherwise returns the golden step it must match.
    fn next_expected(&self, actual: &Value) -> Option<Value> {
        let mut session = self.session.borrow_mut();
        if golden::blessing() {
            session.recorded.push(actual.clone());
            return None;
        }
        let index = session.cursor;
        session.cursor += 1;
        let expected = session.expected.get(index).cloned().unwrap_or_else(|| {
            panic!(
                "golden {} has no step {index} for {}",
                session.name, actual["argv"]
            )
        });
        assert_eq!(
            expected["argv"], actual["argv"],
            "golden {} step {index} is for a different command",
            session.name
        );
        Some(expected)
    }

    /// Runs one command and asserts the observable behaviour recorded in the golden.
    fn step(&self, argv: &[&str]) -> Run {
        let before = self.snapshot();
        let run = run_cli(&self.root, argv, self.engine);
        let after = self.snapshot();
        let actual = json!({
            "argv": argv,
            "code": run.code,
            "stdout": self.normalise(&run.stdout),
            "stderr": self.normalise(&run.stderr),
            "files": delta(&before, &after),
        });
        let Some(expected) = self.next_expected(&actual) else {
            return run;
        };
        let context = || {
            format!(
                "{argv:?}\n--- golden (exit {}) ---\nstdout: {}\nstderr: {}\n--- rust (exit {}) ---\nstdout: {}\nstderr: {}",
                expected["code"],
                expected["stdout"].as_str().unwrap_or_default(),
                expected["stderr"].as_str().unwrap_or_default(),
                run.code,
                run.stdout,
                run.stderr
            )
        };
        assert_eq!(
            expected["code"],
            actual["code"],
            "exit code differs for {}",
            context()
        );
        assert_eq!(
            expected["stdout"],
            actual["stdout"],
            "stdout differs for {}",
            context()
        );
        assert_eq!(
            expected["stderr"],
            actual["stderr"],
            "stderr differs for {}",
            context()
        );
        let (want, got) = (&expected["files"], &actual["files"]);
        if want != got {
            let keys: BTreeSet<&String> = want
                .as_object()
                .into_iter()
                .chain(got.as_object())
                .flat_map(|map| map.keys())
                .collect();
            let differing: Vec<String> = keys
                .into_iter()
                .filter(|key| want.get(key.as_str()) != got.get(key.as_str()))
                .map(|key| {
                    format!(
                        "{key}\n  golden: {:?}\n  rust:   {:?}",
                        want.get(key.as_str()),
                        got.get(key.as_str())
                    )
                })
                .collect();
            panic!(
                "project files differ after {argv:?} (changes relative to the state before the step):\n{}",
                differing.join("\n")
            );
        }
        run
    }

    /// Runs a command that must exit 0 and compares only the listed files (used where a
    /// command's other output is pinned elsewhere).
    fn partial(&self, argv: &[&str], files: &[&str]) {
        let run = run_cli(&self.root, argv, self.engine);
        assert_eq!(run.code, 0, "{argv:?} (rust): {run:?}");
        let after = self.snapshot();
        let listed: Map<String, Value> = files
            .iter()
            .map(|file| ((*file).to_owned(), json!(after.get(*file))))
            .collect();
        if golden::blessing() {
            for file in files {
                assert!(after.contains_key(*file), "{argv:?}: wrote no {file}");
            }
        }
        let actual = json!({ "argv": argv, "code": run.code, "files": listed });
        let Some(expected) = self.next_expected(&actual) else {
            return;
        };
        assert_eq!(expected["code"], json!(0), "golden {argv:?} did not exit 0");
        for file in files {
            assert!(
                expected["files"][*file].is_string(),
                "{argv:?}: the golden records no {file}"
            );
            assert_eq!(
                expected["files"][*file], actual["files"][*file],
                "{file} differs after {argv:?}"
            );
        }
    }

    /// Runs a step that must succeed (exit 0) and returns its JSON output.
    fn ok(&self, argv: &[&str]) -> Value {
        let run = self.step(argv);
        assert_eq!(run.code, 0, "{argv:?} failed: {run:?}");
        run.json()
    }

    /// Runs a step that must fail with exit 1 and a `dtech-aidlc:` message.
    fn fails(&self, argv: &[&str]) -> String {
        let run = self.step(argv);
        assert_eq!(run.code, 1, "{argv:?} should fail: {run:?}");
        assert!(run.stderr.starts_with("dtech-aidlc: "), "{run:?}");
        run.stderr
    }
}

impl Drop for Pair {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let session = self.session.borrow();
        if golden::blessing() {
            golden::write(SUITE, &session.name, &json!({ "steps": session.recorded }));
        } else {
            assert_eq!(
                session.cursor,
                session.expected.len(),
                "golden {} has steps the scenario did not run",
                session.name
            );
        }
    }
}

/// Restores the recorded onboarded state (`fixture-onboarded-<harness>.json`) into
/// `root`. When blessing and the fixture is missing, it is recorded from the CLI's own
/// `onboard --harness <harness> --scope solo`.
fn restore_onboarded(root: &Path, harness: &str) {
    let case = format!("fixture-onboarded-{harness}");
    if !golden::exists(SUITE, &case) {
        assert!(
            golden::blessing(),
            "missing fixture {}; record it with DTECH_AIDLC_BLESS=1",
            golden::path(SUITE, &case).display()
        );
        let run = run_cli(
            root,
            &["onboard", "--harness", harness, "--scope", "solo"],
            Engine::Home,
        );
        assert_eq!(run.code, 0, "{run:?}");
        let repo = repo();
        let files = walk_tree(root, &|path, bytes| {
            let text = String::from_utf8(bytes)
                .unwrap_or_else(|_| panic!("{} is not UTF-8", path.display()));
            for dir in [root, repo.as_path()] {
                assert_eq!(
                    golden::replace_path(&text, dir, "<PATH>"),
                    text,
                    "{} names a machine path",
                    path.display()
                );
            }
            text
        });
        golden::write(SUITE, &case, &json!({ "files": files }));
        return;
    }
    let fixture = golden::read(SUITE, &case);
    let files = fixture["files"]
        .as_object()
        .unwrap_or_else(|| panic!("fixture {case} has no files object"));
    for (rel, content) in files {
        let path = root.join(rel.trim_end_matches('/'));
        if rel.ends_with('/') {
            fs::create_dir_all(&path).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content.as_str().unwrap()).unwrap();
        }
    }
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn seed_project(root: &Path) {
    write(root, "src/main.rs", "fn main() {}\n");
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"proj\"\nversion = \"0.1.0\"\n",
    );
    write(root, "README.md", "# Proj\n\nA routing fixture.\n");
    write(root, "docs/guide.md", "# Guide\n\nHow it works.\n");
    write(root, "notes/extra.md", "# Extra notes\n\nMore.\n");
}

/// Writes every artifact a directive asks for into both projects.
fn write_artifacts(pair: &Pair, directive: &Value) {
    for artifact in directive["artifacts"].as_array().into_iter().flatten() {
        let rel = artifact["path"].as_str().unwrap().to_owned();
        pair.both(|root| write(root, &rel, "# Artifact\n\nContent for the routing test.\n"));
    }
}

fn first_agent() -> String {
    let graph: Value =
        serde_json::from_slice(&fs::read(repo().join("core/data/phase-graph.json")).unwrap())
            .unwrap();
    graph["agents"][0]["id"].as_str().unwrap().to_owned()
}

/// Every command key of the CLI with its positional-argument limit.
const COMMAND_KEYS: &[(&str, Option<usize>)] = &[
    ("init", Some(0)),
    ("onboard", Some(0)),
    ("orchestrate next", Some(0)),
    ("orchestrate report", Some(0)),
    ("ids check", Some(0)),
    ("scope", Some(1)),
    ("status", Some(0)),
    ("doctor", Some(0)),
    ("intent list", Some(0)),
    ("intent pause", Some(0)),
    ("intent resume", Some(1)),
    ("intent abandon", Some(1)),
    ("intent depth", Some(1)),
    ("intent rework", Some(0)),
    ("intent assign", Some(1)),
    ("config agents", Some(3)),
    ("config scope", Some(1)),
    ("config mode", Some(1)),
    ("config gate-mode", Some(2)),
    ("config prerequisite", Some(2)),
    ("harness add", Some(1)),
    ("repos add", Some(2)),
    ("repos pull", Some(1)),
    ("repos list", Some(0)),
    ("repos remove", Some(1)),
    ("deps add", Some(2)),
    ("deps list", Some(0)),
    ("deps pull", Some(1)),
    ("completion zsh", Some(0)),
    ("knowledge init", Some(0)),
    ("knowledge sources", Some(3)),
    ("knowledge list", Some(0)),
    ("knowledge show", Some(1)),
    ("knowledge check", Some(0)),
    ("knowledge index", Some(0)),
    ("harness status", Some(0)),
    ("harness refresh", Some(1)),
    ("monitor", Some(0)),
];

#[test]
fn option_and_argument_validation_matches_the_golden_for_every_command() {
    let pair = Pair::empty("validation");
    for (key, max) in COMMAND_KEYS {
        let words: Vec<&str> = key.split(' ').collect();
        let mut argv = words.clone();
        argv.push("--no-such-option");
        pair.fails(&argv);
        if let Some(max) = max {
            let mut argv = words.clone();
            let extra: Vec<String> = (0..=*max).map(|index| format!("extra{index}")).collect();
            argv.extend(extra.iter().map(String::as_str));
            pair.fails(&argv);
        }
    }
    // Several disallowed options: the first one given is reported.
    pair.fails(&["status", "--scope", "solo", "--depth", "fast"]);
    pair.fails(&["status", "--depth", "fast", "--scope", "solo"]);
    // Parser errors.
    pair.fails(&["status", "--intent"]);
    pair.fails(&["status", "--intent", "a", "--intent", "b"]);
    pair.fails(&["onboard", "--rescan=yes"]);
    // Unknown commands and actions get the recorded messages.
    for argv in [
        &["frobnicate"][..],
        &["frobnicate", "--depth", "fast"],
        &["completion"],
        &["completion", "bash"],
        &["orchestrate"],
        &["orchestrate", "go"],
        &["config"],
        &["config", "colour"],
        &["harness"],
        &["harness", "remove"],
        &["ids"],
        &["ids", "list"],
        &["intent"],
        &["intent", "close"],
        &["knowledge"],
        &["knowledge", "sync"],
        &["repos"],
        &["repos", "sync"],
        &["deps"],
        &["deps", "sync"],
        &["knowledge", "sync", "--kind", "docs"],
    ] {
        pair.fails(argv);
    }
    // Help, version and completion need no project.
    for argv in [
        &["help"][..],
        &["--help"],
        &["status", "--help"],
        &["version"],
        &["--version"],
        &["completion", "zsh"],
    ] {
        let run = pair.step(argv);
        assert_eq!(run.code, 0, "{run:?}");
    }
    assert!(!pair.root.join(".aidlc").exists());
}

#[test]
fn an_unsafe_state_directory_is_refused_by_every_command_but_completion() {
    let pair = Pair::new("unsafe", true, |root| {
        fs::write(root.join(".aidlc"), "not a directory\n").unwrap()
    });
    for argv in [
        &["onboard", "--harness", "claude"][..],
        &["scope"],
        &["status"],
        &["doctor"],
        &["orchestrate", "next"],
        &["orchestrate", "go"],
        &["ids", "check"],
        &["intent", "list"],
        &["intent", "pause"],
        &["config", "scope"],
        &["config", "colour"],
        &["harness", "status"],
        &["knowledge", "list"],
        &["repos", "list"],
        &["deps", "list"],
        &["frobnicate"],
    ] {
        pair.fails(argv);
    }
    assert_eq!(pair.step(&["completion", "zsh"]).code, 0);
}

#[test]
fn read_only_commands_match_the_golden_on_new_and_missing_projects() {
    let pair = Pair::empty("read-only");
    pair.ok(&["scope"]);
    pair.ok(&["scope", "solo", "--depth", "fast"]);
    pair.ok(&["scope", "enterprise"]);
    pair.fails(&["scope", "galaxy"]);
    pair.fails(&["scope", "solo", "--depth", "deep"]);
    pair.ok(&["status"]);
    pair.fails(&["status", "--intent", "nope"]);
    pair.ok(&["intent", "list"]);
    pair.fails(&["ids", "check"]);
    pair.fails(&["deps", "list"]);
    pair.fails(&["orchestrate", "report", "--result", "complete"]);
    pair.fails(&["orchestrate", "next", "--mode", "robot"]);
    pair.ok(&["harness", "status"]);
    pair.fails(&["harness", "refresh"]);
    pair.fails(&["harness", "add", "claude"]);
    pair.fails(&["config", "scope", "team"]);
    pair.ok(&["config", "agents"]);
    pair.ok(&["repos", "list"]);
    pair.ok(&["knowledge", "list"]);
    pair.ok(&["knowledge", "sources"]);
    pair.fails(&["knowledge", "show"]);
    pair.fails(&["intent", "pause"]);
    pair.fails(&["onboard", "--rescan"]);
    pair.fails(&["onboard"]);
    pair.fails(&["onboard", "--harness", "vim"]);
    pair.fails(&["onboard", "--harness", "claude", "--scope", "galaxy"]);
    assert!(!pair.root.join(".aidlc").exists());
    // `doctor` reports on an empty project without creating anything.
    pair.step(&["doctor"]);
    assert!(!pair.root.join(".aidlc").exists());
    // A project folder that does not exist is never created.
    let missing = Pair::new("missing", false, |_| {});
    missing.step(&["status"]);
    missing.step(&["doctor"]);
    missing.step(&["intent", "list"]);
    missing.step(&["orchestrate", "next"]);
    assert!(!missing.root.exists());
    // `status` and `intent list` read the graph only, so a malformed
    // configuration does not stop them; `scope` validates it.
    let broken = Pair::new("broken-config", true, |root| {
        write(root, ".aidlc/config.json", "{bad json\n")
    });
    broken.ok(&["status"]);
    broken.ok(&["intent", "list"]);
    broken.fails(&["scope"]);
    broken.ok(&["config", "scope"]);
    broken.fails(&["config", "agents"]);
}

/// Every validation error of `onboard` / `init` / `onboard --rescan` (checked by the
/// router before anything is written) matches the golden exactly, and nothing is
/// written.
#[test]
fn onboarding_validation_matches_the_golden() {
    let pair = Pair::new("onboard-errors", true, seed_project);
    pair.fails(&["onboard"]);
    pair.fails(&["onboard", "--scope", "solo"]);
    pair.fails(&["onboard", "--harness", "vim"]);
    pair.fails(&["onboard", "--harness", "claude", "--scope", "galaxy"]);
    pair.fails(&["onboard", "--scope", "galaxy", "--harness", "vim"]);
    pair.fails(&["onboard", "--docs", "missing.md", "--harness", "claude"]);
    pair.fails(&["onboard", "--docs", "../outside", "--harness", "claude"]);
    pair.fails(&["onboard", "--docs", ".", "--harness", "claude"]);
    pair.fails(&["init", "--docs", "missing.md", "--harness", "vim"]);
    pair.fails(&["onboard", "--rescan"]);
    pair.fails(&["onboard", "--rescan", "--docs", "docs"]);
    pair.fails(&["onboard", "--rescan", "--no-detect"]);
    assert!(!pair.root.join(".aidlc").exists());

    // An initialized project (the recorded onboarded state).
    let pair = Pair::onboarded("onboard-errors-initialized", "claude");
    pair.fails(&["onboard", "--scope", "team"]);
    pair.fails(&["init", "--harness", "deepseek"]);
    pair.fails(&["onboard", "--harness", "deepseek", "--scope", "solo"]);
    pair.fails(&["onboard", "--rescan", "--harness", "claude"]);
    pair.fails(&["onboard", "--rescan", "--docs", "missing.md"]);
    pair.fails(&["onboard", "--rescan", "--docs", "../outside"]);
    pair.both(|root| write(root, "aidlc.knowledge.json", "{not json\n"));
    pair.fails(&["onboard", "--rescan"]);
    pair.both(|root| fs::remove_file(root.join("aidlc.knowledge.json")).unwrap());
    pair.fails(&[
        "onboard",
        "--rescan",
        "--docs",
        "docs/guide.md",
        "--docs",
        "my notes",
    ]);
    pair.both(|root| {
        write(
            root,
            ".aidlc/config.json",
            "{\"harness\":\"vim\",\"scope\":\"solo\"}\n",
        )
    });
    pair.fails(&["onboard"]);
    pair.fails(&["onboard", "--harness", "claude"]);
}

/// Pins the exit code and the configuration, knowledge manifest, PROJECT.md scaffold
/// and projected skill files that a successful onboarding writes.
#[test]
fn onboarding_success_writes_the_same_configuration_and_knowledge_files_as_the_golden() {
    let pair = Pair::new("onboard", true, seed_project);
    let shared = [
        ".aidlc/config.json",
        "aidlc.knowledge.json",
        "PROJECT.md",
        ".claude/skills/dtech-aidlc/SKILL.md",
        ".claude/skills/dtech-aidlc/onboarding.md",
    ];
    pair.partial(
        &["onboard", "--harness", "claude", "--docs", "docs/guide.md"],
        &shared,
    );
    pair.partial(&["onboard"], &shared);
    let alias = Pair::new("init", true, seed_project);
    alias.partial(
        &[
            "init",
            "--harness",
            "deepseek",
            "--scope",
            "team",
            "--no-detect",
        ],
        &[".aidlc/config.json", "PROJECT.md"],
    );
}

#[test]
fn config_commands_match_the_golden() {
    let agent = first_agent();
    let pair = Pair::onboarded("config", "claude");
    pair.fails(&["config"]);
    pair.fails(&["config", "colour"]);
    pair.ok(&["config", "scope"]);
    pair.ok(&["config", "scope", "team"]);
    pair.ok(&["config", "scope", "team"]);
    pair.fails(&["config", "scope", "galaxy"]);
    pair.ok(&["config", "mode"]);
    pair.ok(&["config", "mode", "ai"]);
    pair.fails(&["config", "mode", "robot"]);
    pair.ok(&["config", "gate-mode"]);
    pair.ok(&["config", "gate-mode", "analysis-baseline", "human"]);
    pair.ok(&["config", "gate-mode", "analysis-baseline"]);
    pair.fails(&["config", "gate-mode", "nope", "human"]);
    pair.fails(&["config", "gate-mode", "analysis-baseline", "ai"]);
    pair.ok(&["config", "gate-mode", "analysis-baseline", "default"]);
    pair.ok(&[
        "config",
        "prerequisite",
        "set",
        "toolchain",
        "--check",
        "exit 0",
        "--hint",
        "Install the toolchain",
        "--applies-to",
        "problem-framing",
        "--applies-to",
        "problem-framing",
    ]);
    pair.ok(&[
        "config",
        "prerequisite",
        "set",
        "broken",
        "--check",
        "exit 3",
    ]);
    pair.ok(&["config", "prerequisite"]);
    pair.ok(&["config", "prerequisite", "check", "toolchain"]);
    let failing = pair.step(&["config", "prerequisite", "check"]);
    assert_eq!(failing.code, 1, "{failing:?}");
    assert_eq!(failing.json()["ok"], Value::Bool(false));
    pair.fails(&["config", "prerequisite", "check", "nope"]);
    pair.fails(&["config", "prerequisite", "set"]);
    pair.fails(&["config", "prerequisite", "set", "nocheck"]);
    pair.fails(&[
        "config",
        "prerequisite",
        "set",
        "bad",
        "--check",
        "exit 0",
        "--applies-to",
        "nowhere",
    ]);
    pair.ok(&["config", "prerequisite", "remove", "broken"]);
    pair.fails(&["config", "prerequisite", "remove", "broken"]);
    pair.fails(&["config", "prerequisite", "audit"]);
    pair.ok(&["config", "agents"]);
    pair.ok(&["config", "agents", "set", &agent, "haiku"]);
    pair.fails(&["config", "agents", "set", &agent, "gpt4"]);
    pair.fails(&["config", "agents", "set", "nobody", "haiku"]);
    pair.fails(&["config", "agents", "promote", &agent]);
    pair.ok(&["config", "agents", "reset", &agent]);
    pair.ok(&["scope", "solo"]);
    // Model aliases apply to the claude harness only.
    let deepseek = Pair::onboarded("config-deepseek", "deepseek");
    deepseek.fails(&["config", "agents", "set", &agent, "haiku"]);
    deepseek.ok(&["config", "agents"]);
}

#[test]
fn harness_commands_match_the_golden() {
    let pair = Pair::onboarded("harness", "claude");
    pair.ok(&["harness", "status"]);
    pair.fails(&["harness", "add", "vim"]);
    pair.fails(&["harness", "add"]);
    pair.ok(&["harness", "add", "deepseek"]);
    pair.ok(&["harness", "add", "deepseek"]);
    pair.ok(&["harness", "status"]);
    pair.ok(&["harness", "refresh", "--dry-run"]);
    pair.fails(&["harness", "refresh", "vim"]);
    // A user-modified projected file is kept, then replaced (with a backup) on --force.
    let skills = pair.root.join(".claude/skills");
    let skill = walk_files(&skills)
        .into_iter()
        .next()
        .expect("a projected skill file");
    let rel = skill
        .strip_prefix(&pair.root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    pair.both(|root| write(root, &rel, "locally edited\n"));
    pair.ok(&["harness", "status"]);
    pair.ok(&["harness", "refresh", "claude"]);
    pair.ok(&["harness", "refresh", "--force"]);
    pair.ok(&["harness", "status"]);
    pair.ok(&["doctor"]);
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn knowledge_commands_match_the_golden() {
    let fresh = Pair::new("knowledge-init", true, seed_project);
    fresh.ok(&["knowledge", "init"]);
    fresh.ok(&["knowledge", "init"]);
    let pair = Pair::onboarded("knowledge", "claude");
    pair.ok(&["knowledge", "sources"]);
    pair.ok(&["knowledge", "sources", "list"]);
    pair.ok(&[
        "knowledge",
        "sources",
        "add",
        "extra",
        "notes/extra.md",
        "--kind",
        "docs",
        "--description",
        "Extra notes",
    ]);
    pair.fails(&["knowledge", "sources", "add", "extra", "notes/extra.md"]);
    pair.fails(&["knowledge", "sources", "add", "only-id"]);
    pair.fails(&["knowledge", "sources", "add", "outside", "../elsewhere.md"]);
    pair.ok(&["knowledge", "sources", "remove", "extra"]);
    pair.fails(&["knowledge", "sources", "remove", "extra"]);
    pair.fails(&["knowledge", "sources", "remove"]);
    pair.fails(&["knowledge", "sources", "rename"]);
    pair.both(|root| {
        write(
            root,
            ".aidlc/knowledge/routing-note.md",
            "---\nid: routing-note\ntitle: Routing note\norigin: archived\nscope: solo\nclosed: 2026-11-20\ntags: [routing, cli]\n---\n# Routing note\n\nNative routing.\n",
        );
        write(root, ".aidlc/knowledge/legacy-note.md", "# Legacy\n\nNo frontmatter.\n");
    });
    pair.ok(&["knowledge", "list"]);
    pair.ok(&["knowledge", "show", "routing-note"]);
    pair.fails(&["knowledge", "show", "nope"]);
    pair.ok(&["knowledge", "check"]);
    pair.ok(&["knowledge", "index"]);
    pair.both(|root| write(root, "aidlc.knowledge.json", "{not json\n"));
    pair.fails(&["knowledge", "sources"]);
}

#[test]
fn repos_commands_match_the_golden() {
    let pair = Pair::onboarded("repos", "claude");
    pair.ok(&["repos", "list"]);
    pair.ok(&[
        "repos",
        "add",
        "lib",
        "https://example.invalid/org/lib.git",
        "--ref",
        "main",
    ]);
    pair.fails(&["repos", "add", "lib", "https://example.invalid/org/lib.git"]);
    pair.fails(&["repos", "add", "bad name", "https://example.invalid/x.git"]);
    pair.fails(&["repos", "add", "lib"]);
    pair.fails(&["repos", "add", "rel", "relative/path"]);
    pair.ok(&["repos", "list"]);
    pair.ok(&["repos", "remove", "lib"]);
    pair.fails(&["repos", "remove", "lib"]);
    pair.fails(&["repos", "remove"]);
    pair.ok(&["repos", "pull"]);
    pair.fails(&["repos", "pull", "nope"]);

    if !available("git") {
        eprintln!("git is not on PATH; skipping the repos pull clone case");
        return;
    }
    let shared = TempDir::new("remote");
    let remote = bare_remote(&shared.0);
    let declared = format!(
        "{{\n  \"schemaVersion\": 1,\n  \"repos\": {{\n    \"lib\": {{\n      \"url\": {}\n    }}\n  }}\n}}\n",
        serde_json::to_string(remote.to_str().unwrap()).unwrap()
    );
    let clone = Pair::onboarded("repos-pull", "claude").with_remote(remote.clone());
    clone.both(|root| {
        fs::write(root.join("aidlc.deps.json"), &declared).unwrap();
        write(
            root,
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":{"stale":{"url":"https://example.invalid/stale.git"}}}"#,
        );
    });
    // The note about the dropped runtime entry goes to stderr.
    let pulled = clone.step(&["repos", "pull"]);
    assert_eq!(pulled.code, 0, "{pulled:?}");
    assert!(
        pulled.stderr.contains("removed from the runtime copy"),
        "{pulled:?}"
    );
    clone.ok(&["repos", "list"]);
    clone.ok(&["repos", "pull", "lib"]);
    clone.both(|root| {
        fs::write(root.join(".aidlc/repos/lib/tracked.txt"), "local change\n").unwrap()
    });
    let dirty = clone.step(&["repos", "pull"]);
    assert_eq!(dirty.code, 1, "{dirty:?}");
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A bare repository with one commit on `main`. The author, committer and dates are
/// fixed, so the commit id is the same on every run and matches the golden.
fn bare_remote(base: &Path) -> PathBuf {
    let remote = base.join("remote.git");
    let seed = base.join("seed");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&seed).unwrap();
    git(&remote, &["init", "--bare"]);
    git(&seed, &["init"]);
    git(&seed, &["config", "user.email", "test@example.invalid"]);
    git(&seed, &["config", "user.name", "Test"]);
    fs::write(seed.join("tracked.txt"), "base\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "base"]);
    git(&seed, &["branch", "-M", "main"]);
    git(
        &seed,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&seed, &["push", "-u", "origin", "main"]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    remote
}

/// End to end: onboard, start an intent, pass a gate, block Develop and rework the
/// design, use the intent, ids and deps commands, pause/resume/abandon, write the
/// knowledge index and run doctor, comparing every step with the golden.
#[test]
fn end_to_end_lifecycle_matches_the_golden() {
    lifecycle(Pair::onboarded("lifecycle", "claude"));
}

/// The same lifecycle with the methodology embedded in the Rust binary.
#[test]
fn end_to_end_lifecycle_with_the_embedded_engine_matches_the_golden() {
    lifecycle(Pair::onboarded("lifecycle-embedded", "claude").with_engine(Engine::Embedded));
}

fn lifecycle(pair: Pair) {
    pair.ok(&["doctor"]);
    pair.fails(&["intent", "assign", "Alex"]);
    pair.fails(&["intent", "rework", "--detail", "too early"]);
    pair.fails(&["deps", "add", "a", "b"]);

    let first = pair.ok(&["orchestrate", "next", "--intent", "Routing lifecycle work"]);
    assert_eq!(first["stage"]["id"], "problem-framing");
    pair.ok(&["status"]);
    pair.ok(&["intent", "list"]);
    pair.fails(&[
        "intent",
        "depth",
        "deep",
        "--approved-by",
        "Pat Owner",
        "--detail",
        "x",
    ]);
    pair.fails(&["intent", "depth", "fast", "--detail", "smaller"]);
    pair.fails(&["intent", "depth"]);
    pair.ok(&[
        "intent",
        "depth",
        "fast",
        "--approved-by",
        "Pat Owner",
        "--detail",
        "Smaller change",
    ]);
    pair.fails(&["intent", "assign"]);
    pair.ok(&["intent", "assign", "Alex Doe"]);
    pair.fails(&["intent", "rework"]);
    pair.fails(&["intent", "rework", "--detail", "nothing blocked yet"]);

    // Analyze: the fast depth makes problem-framing the final stage before the gate.
    let directive = pair.ok(&["orchestrate", "next"]);
    assert_eq!(directive["stage"]["id"], "problem-framing");
    pair.fails(&["orchestrate", "report", "--result", "done"]);
    pair.fails(&["orchestrate", "report", "--result", "complete"]);
    write_artifacts(&pair, &directive);
    pair.fails(&["orchestrate", "report", "--result", "complete"]);
    let gate = pair.ok(&[
        "orchestrate",
        "report",
        "--result",
        "complete",
        "--approval",
        "approved",
        "--approved-by",
        "Pat Owner",
        "--approval-note",
        "Looks right",
    ]);
    assert_eq!(gate["gate"], "analysis-baseline");

    // Develop: block implementation-design, then request a design rework.
    let design = pair.ok(&["orchestrate", "next"]);
    assert_eq!(design["stage"]["id"], "implementation-design");
    pair.ok(&["ids", "check"]);
    pair.fails(&["ids", "check", "--stage", "nowhere"]);
    pair.step(&["ids", "check", "--stage", "implementation-design"]);
    pair.ok(&[
        "orchestrate",
        "report",
        "--result",
        "blocked",
        "--detail",
        "The API contract is unclear",
    ]);
    pair.ok(&["intent", "rework", "--detail", "Redo the design"]);
    let rework = pair.ok(&["orchestrate", "next"]);
    assert_eq!(rework["stage"]["id"], "implementation-design");

    // Dependency edges live in the active intent.
    pair.ok(&["deps", "add", "item-a", "item-b"]);
    pair.ok(&["deps", "add", "item-a", "item-b"]);
    pair.fails(&["deps", "add", "item-a", "item-a"]);
    pair.fails(&["deps", "add", "item-a"]);
    pair.fails(&["deps", "add", "Bad Id", "item-b"]);
    pair.ok(&["deps", "list"]);
    pair.ok(&["deps", "pull", "item-a"]);
    pair.ok(&["deps", "pull", "item-z"]);
    pair.fails(&["deps", "pull"]);

    // Pause, resume and abandon.
    pair.ok(&["intent", "pause", "--detail", "Coffee break"]);
    pair.fails(&["intent", "pause"]);
    pair.ok(&["status"]);
    pair.ok(&["intent", "list"]);
    pair.fails(&["intent", "resume"]);
    pair.fails(&["intent", "resume", "nope"]);
    pair.ok(&["intent", "resume", "routing-lifecycle-work"]);
    pair.ok(&["status", "--intent", "routing-lifecycle-work"]);
    pair.step(&["orchestrate", "next", "--intent", "Second piece of work"]);
    pair.ok(&["intent", "pause"]);
    pair.ok(&["orchestrate", "next", "--intent", "Second piece of work"]);
    pair.fails(&["intent", "resume", "routing-lifecycle-work"]);
    pair.fails(&["intent", "abandon"]);
    pair.ok(&[
        "intent",
        "abandon",
        "routing-lifecycle-work",
        "--detail",
        "Superseded",
    ]);
    pair.fails(&[
        "intent",
        "abandon",
        "routing-lifecycle-work",
        "--detail",
        "again",
    ]);
    pair.fails(&["intent", "resume", "routing-lifecycle-work"]);
    pair.ok(&["intent", "abandon", "--detail", "Not needed"]);
    pair.fails(&["intent", "abandon", "--detail", "nothing active"]);
    pair.ok(&["intent", "list"]);
    pair.ok(&["status"]);

    // Knowledge index and doctor at the end of the run.
    pair.ok(&["knowledge", "index"]);
    pair.ok(&["knowledge", "check"]);
    pair.ok(&["doctor"]);
    // A corrupt audit line makes doctor fail: the report is printed with exit code 1.
    pair.both(|root| {
        let path = root.join(".aidlc/audit.jsonl");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("not json\n");
        fs::write(path, text).unwrap();
    });
    let doctor = pair.step(&["doctor"]);
    assert_eq!(doctor.code, 1, "{doctor:?}");
    assert_eq!(doctor.json()["ok"], Value::Bool(false));
}
