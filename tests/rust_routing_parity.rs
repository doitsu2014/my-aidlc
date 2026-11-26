//! Binary-level routing parity: the built `dtech-aidlc` executable against the Node
//! reference CLI (`node core/tools/dtech-aidlc.mjs`).
//!
//! Every scenario builds the same fixture twice (`<base>/node/proj` and
//! `<base>/rust/proj`, so both projects have the same folder name), runs Node on one
//! copy and the Rust binary on the other with identical arguments, and after each step
//! compares stdout, stderr, the exit code and every file and directory of the project
//! (`.git` directories excepted). Only the project root, ISO-8601 timestamps and
//! `mtimeMs` values are normalised; everything else, including JSON formatting and key
//! order, must match byte for byte.
//!
//! Each Node command key of `core/tools/cli-spec.mjs` has at least one success and one
//! error case, and every key is also checked for Node's unknown-option and
//! extra-argument errors. When `node` is not on PATH the tests are skipped; the
//! `repos pull` clone case also needs `git`.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

fn skip_without_node() -> bool {
    if available("node") {
        return false;
    }
    eprintln!("node is not on PATH; skipping routing parity");
    true
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
    /// `DTECH_AIDLC_HOME` names this repository for both CLIs, so directive paths name
    /// the same methodology files and must match exactly.
    Home,
    /// No `DTECH_AIDLC_HOME`: Node uses its own `core/`, the Rust binary its embedded
    /// copy (materialised under `<project>/.aidlc/engine`). Engine paths are compared
    /// from the engine directory on and `.aidlc/engine/` is left out of the file
    /// comparison.
    Embedded,
}

fn execute(mut command: Command, root: &Path, argv: &[&str], engine: Engine) -> Run {
    let cwd = if root.is_dir() {
        root.to_path_buf()
    } else {
        root.parent().unwrap().to_path_buf()
    };
    command
        .args(argv)
        .arg("--project")
        .arg(root)
        .current_dir(cwd)
        .env_remove("DTECH_AIDLC_HOME");
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

fn run_node(root: &Path, argv: &[&str], engine: Engine) -> Run {
    let mut command = Command::new("node");
    command.arg(repo().join("core/tools/dtech-aidlc.mjs"));
    execute(command, root, argv, engine)
}

fn run_rust(root: &Path, argv: &[&str], engine: Engine) -> Run {
    execute(
        Command::new(env!("CARGO_BIN_EXE_dtech-aidlc")),
        root,
        argv,
        engine,
    )
}

/// Replaces every `YYYY-MM-DDTHH:MM:SS.mmmZ` timestamp with `<ts>`.
fn replace_timestamps(text: &str) -> String {
    let bytes = text.as_bytes();
    let is_ts = |at: usize| {
        bytes.len() >= at + 24
            && (0..24).all(|index| {
                let byte = bytes[at + index];
                match index {
                    4 | 7 => byte == b'-',
                    10 => byte == b'T',
                    13 | 16 => byte == b':',
                    19 => byte == b'.',
                    23 => byte == b'Z',
                    _ => byte.is_ascii_digit(),
                }
            })
    };
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if is_ts(at) {
            out.push_str("<ts>");
            at += 24;
        } else {
            let ch = text[at..].chars().next().unwrap();
            out.push(ch);
            at += ch.len_utf8();
        }
    }
    out
}

/// Replaces the number after every `"mtimeMs":` with `0` (file times differ between
/// the two fixtures).
fn replace_mtimes(text: &str) -> String {
    let marker = "\"mtimeMs\":";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(found) = rest.find(marker) {
        let (head, tail) = rest.split_at(found + marker.len());
        out.push_str(head);
        let spaces = tail.len() - tail.trim_start_matches(' ').len();
        out.push_str(&tail[..spaces]);
        let tail = &tail[spaces..];
        let number = tail
            .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == '-'))
            .unwrap_or(tail.len());
        if number > 0 {
            out.push('0');
        }
        rest = &tail[number..];
    }
    out.push_str(rest);
    out
}

/// Replaces `path` (as shown, JSON-escaped, or with forward slashes) by `label`.
fn replace_path(text: &str, path: &Path, label: &str) -> String {
    let shown = path.display().to_string();
    let escaped = shown.replace('\\', "\\\\");
    let forward = shown.replace('\\', "/");
    text.replace(&escaped, label)
        .replace(&shown, label)
        .replace(&forward, label)
}

/// Normalises the project root and, when given, the engine directory. The engine is
/// replaced first (the embedded engine of the Rust binary lives inside the project),
/// and separators after `<ENGINE>` are compared as `/`.
fn normalise_with(text: &str, root: &Path, engine: Option<&Path>) -> String {
    let mut text = match engine {
        Some(engine) => replace_path(text, engine, "<ENGINE>"),
        None => text.to_owned(),
    };
    let mut from = 0;
    while let Some(found) = text[from..].find("<ENGINE>") {
        let start = from + found + "<ENGINE>".len();
        let end = text[start..]
            .find(|ch: char| ch == '"' || ch.is_whitespace())
            .map_or(text.len(), |offset| start + offset);
        let fixed = text[start..end].replace("\\\\", "/").replace('\\', "/");
        text.replace_range(start..end, &fixed);
        from = start;
    }
    let text = replace_path(&text, root, "<ROOT>");
    replace_mtimes(&replace_timestamps(&text))
}

fn normalise(text: &str, root: &Path) -> String {
    normalise_with(text, root, None)
}

/// Every file and directory under `root` (relative, `/`-separated), with normalised
/// file contents. `.git` directories (repository checkouts) are skipped.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, String>) {
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
                walk(root, &path, into);
            } else if kind.is_symlink() {
                into.insert(rel, "<symlink>".into());
            } else {
                let bytes = fs::read(&path).unwrap();
                into.insert(rel, normalise(&String::from_utf8_lossy(&bytes), root));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

/// Two identical project fixtures, one driven by Node and one by the Rust binary.
struct Pair {
    _base: TempDir,
    node: PathBuf,
    rust: PathBuf,
    engine: Engine,
}

impl Pair {
    /// `setup` runs on both projects; `create` decides whether the project folder exists.
    fn new(label: &str, create: bool, setup: impl Fn(&Path)) -> Self {
        let base = TempDir::new(label);
        let node = base.0.join("node").join("proj");
        let rust = base.0.join("rust").join("proj");
        for root in [&node, &rust] {
            if create {
                fs::create_dir_all(root).unwrap();
            } else {
                fs::create_dir_all(root.parent().unwrap()).unwrap();
            }
            setup(root);
        }
        Pair {
            _base: base,
            node,
            rust,
            engine: Engine::Home,
        }
    }

    fn with_engine(mut self, engine: Engine) -> Self {
        self.engine = engine;
        self
    }

    /// The engine directory each CLI reports paths under.
    fn engine_dirs(&self) -> (PathBuf, PathBuf) {
        let node = repo().join("core");
        match self.engine {
            Engine::Home => (node.clone(), node),
            Engine::Embedded => (node, self.rust.join(".aidlc").join("engine")),
        }
    }

    fn empty(label: &str) -> Self {
        Self::new(label, true, |_| {})
    }

    /// A project with source and documentation files, onboarded by Node in both copies
    /// so that every later step starts from identical state. (Rust onboarding output is
    /// not yet at byte parity; see `onboarding_matches_node`.)
    fn onboarded(label: &str, harness: &str) -> Self {
        Self::new(label, true, |root| {
            seed_project(root);
            let run = run_node(
                root,
                &["onboard", "--harness", harness, "--scope", "solo"],
                Engine::Home,
            );
            assert_eq!(run.code, 0, "{run:?}");
        })
    }

    /// Applies the same change to both projects.
    fn both(&self, change: impl Fn(&Path)) {
        change(&self.node);
        change(&self.rust);
    }

    /// Runs one command on both projects and asserts identical observable behaviour.
    fn step(&self, argv: &[&str]) -> Run {
        let node = run_node(&self.node, argv, self.engine);
        let rust = run_rust(&self.rust, argv, self.engine);
        let (node_engine, rust_engine) = self.engine_dirs();
        let context = || {
            format!(
                "{argv:?}\n--- node (exit {}) ---\nstdout: {}\nstderr: {}\n--- rust (exit {}) ---\nstdout: {}\nstderr: {}",
                node.code, node.stdout, node.stderr, rust.code, rust.stdout, rust.stderr
            )
        };
        assert_eq!(node.code, rust.code, "exit code differs for {}", context());
        assert_eq!(
            normalise_with(&node.stdout, &self.node, Some(&node_engine)),
            normalise_with(&rust.stdout, &self.rust, Some(&rust_engine)),
            "stdout differs for {}",
            context()
        );
        assert_eq!(
            normalise_with(&node.stderr, &self.node, Some(&node_engine)),
            normalise_with(&rust.stderr, &self.rust, Some(&rust_engine)),
            "stderr differs for {}",
            context()
        );
        let (node_files, rust_files) = (snapshot(&self.node), snapshot(&self.rust));
        if node_files != rust_files {
            let keys: std::collections::BTreeSet<&String> =
                node_files.keys().chain(rust_files.keys()).collect();
            let differing: Vec<String> = keys
                .into_iter()
                .filter(|key| node_files.get(*key) != rust_files.get(*key))
                .map(|key| {
                    format!(
                        "{key}\n  node: {:?}\n  rust: {:?}",
                        node_files.get(key),
                        rust_files.get(key)
                    )
                })
                .collect();
            panic!(
                "project files differ after {argv:?}:\n{}",
                differing.join("\n")
            );
        }
        rust
    }

    /// Runs a command on both projects, requiring exit code 0 from both, and compares
    /// only the listed files (used where a command is not yet at full parity).
    fn partial(&self, argv: &[&str], files: &[&str]) {
        let node = run_node(&self.node, argv, self.engine);
        let rust = run_rust(&self.rust, argv, self.engine);
        assert_eq!(node.code, 0, "{argv:?} (node): {node:?}");
        assert_eq!(rust.code, 0, "{argv:?} (rust): {rust:?}");
        let (node_files, rust_files) = (snapshot(&self.node), snapshot(&self.rust));
        for file in files {
            assert!(
                node_files.contains_key(*file),
                "{argv:?}: node wrote no {file}"
            );
            assert_eq!(
                node_files.get(*file),
                rust_files.get(*file),
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

/// Every command key of `core/tools/cli-spec.mjs` with its positional-argument limit.
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
];

#[test]
fn option_and_argument_validation_matches_node_for_every_command() {
    if skip_without_node() {
        return;
    }
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
    // Several disallowed options: Node reports the first one given.
    pair.fails(&["status", "--scope", "solo", "--depth", "fast"]);
    pair.fails(&["status", "--depth", "fast", "--scope", "solo"]);
    // Parser errors.
    pair.fails(&["status", "--intent"]);
    pair.fails(&["status", "--intent", "a", "--intent", "b"]);
    pair.fails(&["onboard", "--rescan=yes"]);
    // Unknown commands and actions get Node's exact messages.
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
    assert!(!pair.rust.join(".aidlc").exists());
}

#[test]
fn an_unsafe_state_directory_is_refused_by_every_command_but_completion() {
    if skip_without_node() {
        return;
    }
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
fn read_only_commands_match_node_on_new_and_missing_projects() {
    if skip_without_node() {
        return;
    }
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
    assert!(!pair.rust.join(".aidlc").exists());
    // `doctor` reports on an empty project without creating anything.
    pair.step(&["doctor"]);
    assert!(!pair.rust.join(".aidlc").exists());
    // A project folder that does not exist is never created.
    let missing = Pair::new("missing", false, |_| {});
    missing.step(&["status"]);
    missing.step(&["doctor"]);
    missing.step(&["intent", "list"]);
    missing.step(&["orchestrate", "next"]);
    assert!(!missing.rust.exists());
    // `status` and `intent list` read the graph only, like Node, so a malformed
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
/// router before anything is written) matches Node exactly, and nothing is written.
#[test]
fn onboarding_validation_matches_node() {
    if skip_without_node() {
        return;
    }
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
    assert!(!pair.rust.join(".aidlc").exists());

    // An initialized project (onboarded by Node in both copies).
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

/// Known gap: Rust onboarding (`dtech_aidlc::onboarding`) is not at byte parity with
/// Node. Its stdout (`\\?\` paths, shorter `projections`, missing `notes`/`skipped`
/// detail), its audit details, the project profile files and the projection stamp
/// differ. This test pins down only what does match: the exit code and the
/// configuration, knowledge manifest, PROJECT.md scaffold and projected skill files.
#[test]
fn onboarding_success_writes_the_same_configuration_and_knowledge_files_as_node() {
    if skip_without_node() {
        return;
    }
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
fn config_commands_match_node() {
    if skip_without_node() {
        return;
    }
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
fn harness_commands_match_node() {
    if skip_without_node() {
        return;
    }
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
    let skills = pair.node.join(".claude/skills");
    let skill = walk_files(&skills)
        .into_iter()
        .next()
        .expect("a projected skill file");
    let rel = skill
        .strip_prefix(&pair.node)
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
fn knowledge_commands_match_node() {
    if skip_without_node() {
        return;
    }
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
fn repos_commands_match_node() {
    if skip_without_node() {
        return;
    }
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
    let clone = Pair::onboarded("repos-pull", "claude");
    clone.both(|root| {
        fs::write(root.join("aidlc.deps.json"), &declared).unwrap();
        write(
            root,
            ".aidlc/deps.json",
            r#"{"schemaVersion":1,"repos":{"stale":{"url":"https://example.invalid/stale.git"}}}"#,
        );
    });
    // The note about the dropped runtime entry goes to stderr in both.
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
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A bare repository with one commit on `main`, shared by both projects so the URL and
/// commit ids are identical.
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
/// knowledge index and run doctor, comparing every step with Node.
#[test]
fn end_to_end_lifecycle_matches_node() {
    if skip_without_node() {
        return;
    }
    lifecycle(Pair::onboarded("lifecycle", "claude"));
}

/// The same lifecycle with the methodology embedded in the Rust binary.
#[test]
fn end_to_end_lifecycle_with_the_embedded_engine_matches_node() {
    if skip_without_node() {
        return;
    }
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
