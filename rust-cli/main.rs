use dtech_aidlc::application;
use dtech_aidlc::args::{command_flags, command_key, command_max_args, parse_args, FlagValue};
use dtech_aidlc::onboarding::{onboard_project, rescan_project, OnboardOptions};
use dtech_aidlc::orchestrate;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("dtech-aidlc: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The command router, mirroring `main()` of `core/tools/dtech-aidlc.mjs` step by step:
/// version/help, option and argument-count validation, `--project` resolution, the
/// state-directory safety check (every command except `completion`), then dispatch to
/// the native handler. Handlers return `(value, exit code)`; the value is printed like
/// Node `out()` and the exit code is used as is (Node prints some failing reports, such
/// as `doctor` or `ids check`, and then sets `process.exitCode = 1`).
fn run() -> Result<u8, String> {
    let argv: Vec<String> = env::args().skip(1).collect();
    let parsed = parse_args(&argv).map_err(|e| e.to_string())?;
    if parsed.flags.contains_key("version")
        || parsed.positional.first().is_some_and(|a| a == "version")
    {
        print_output(&Value::String(format!(
            "dtech-aidlc {}\n",
            env!("CARGO_PKG_VERSION")
        )))?;
        return Ok(0);
    }
    if parsed.flags.contains_key("help")
        || parsed.positional.is_empty()
        || parsed.positional.first().is_some_and(|a| a == "help")
    {
        print_output(&Value::String(format!(
            "dtech-aidlc {}\n\n{}",
            env!("CARGO_PKG_VERSION"),
            include_str!("help.txt")
        )))?;
        return Ok(0);
    }
    let key = command_key(&parsed.positional).expect("nonempty positionals");
    let allowed = command_flags(&key);
    let invalid = parsed.unknown.first().map(String::as_str).or_else(|| {
        parsed
            .flag_order
            .iter()
            .find(|name| {
                !matches!(name.as_str(), "project" | "help" | "version")
                    && !allowed
                        .is_some_and(|flags| flags.iter().any(|flag| flag.name == name.as_str()))
            })
            .map(String::as_str)
    });
    if let Some(name) = invalid {
        if let Some(options) = allowed {
            let options = options
                .iter()
                .map(|flag| format!("--{}", flag.name))
                .chain(std::iter::once("--project".to_owned()))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Unknown option --{name} for '{key}'. Allowed options: {options}."
            ));
        }
        return Err(format!("Unknown option --{name}. Run 'dtech-aidlc help'."));
    }
    let words = key.split(' ').count();
    if let Some(max) = command_max_args(&key) {
        if let Some(extra) = parsed.positional.get(words + max) {
            return Err(format!("Unexpected argument '{extra}' for '{key}'."));
        }
    }
    let cmd = parsed.positional[0].as_str();
    // Positional arguments after the command words (Node's `args` after its shifts).
    let rest = &parsed.positional[words.min(parsed.positional.len())..];
    let flags = &parsed.flags;
    if cmd == "completion" {
        if parsed.positional.len() != 2 || parsed.positional[1] != "zsh" {
            return Err("Usage: dtech-aidlc completion zsh".into());
        }
        print_output(&Value::String(include_str!("completion.zsh").to_owned()))?;
        return Ok(0);
    }
    // Node: `resolve(flags.project || process.cwd())`, then `assertSafeStateDir(root)`.
    let project = match flags.get("project") {
        Some(FlagValue::String(path)) => PathBuf::from(path),
        _ => env::current_dir().map_err(|e| e.to_string())?,
    };
    let root = orchestrate::resolve_like_node(&project)?;
    dtech_aidlc::audit::assert_safe_state_dir(&root)?;
    let (value, code) = match cmd {
        "onboard" | "init" => return run_onboard(&root, flags),
        "orchestrate" => {
            if key == "orchestrate next" || key == "orchestrate report" {
                return run_orchestrate(&key, &root, flags);
            }
            if key == "orchestrate queue" {
                let project = application::open(&root, application::Caller::Cli)?;
                print_json(&application::queue(&project)?)?;
                return Ok(0);
            }
            if matches!(key.as_str(), "orchestrate park" | "orchestrate revisit") {
                return run_unit_queue(&key, &root, flags);
            }
            // Node reads the graph before rejecting the action.
            load_runtime_graph(application::external_engine().as_deref())?;
            return Err("Use orchestrate next, report, park, queue or revisit".into());
        }
        "scope" | "status" => return run_read_only(&key, &root, rest, flags),
        "intent" if key == "intent list" => return run_read_only(&key, &root, rest, flags),
        "intent" => dtech_aidlc::intent_commands::run(&key, &root, rest, flags)?,
        "harness" => dtech_aidlc::harness_commands::run(&key, &root, rest, flags)?,
        "config" => dtech_aidlc::config_commands::run(&key, &root, rest, flags)?,
        "doctor" => dtech_aidlc::doctor::run(&key, &root, rest, flags)?,
        "ids" => dtech_aidlc::ids::run(&key, &root, rest, flags)?,
        "knowledge" => dtech_aidlc::knowledge_commands::run(&key, &root, rest, flags)?,
        "repos" => dtech_aidlc::dependencies::run_repos(&key, &root, rest, flags)?,
        "deps" => dtech_aidlc::dependencies::run_deps(&key, &root, rest, flags)?,
        _ => return Err(format!("Unknown command {cmd}; use dtech-aidlc help")),
    };
    print_output(&value)?;
    u8::try_from(code).map_err(|_| format!("invalid exit code {code}"))
}

/// Node `onboard` / `init` (including `--rescan`).
fn run_onboard(root: &Path, flags: &BTreeMap<String, FlagValue>) -> Result<u8, String> {
    let explicit_docs = match flags.get("docs") {
        Some(FlagValue::List(values)) => values.clone(),
        _ => Vec::new(),
    };
    let result = if flags.contains_key("rescan") {
        if ["harness", "scope", "no-detect"]
            .iter()
            .any(|name| flags.contains_key(*name))
        {
            return Err("--rescan re-scans an initialized project and does not take --harness, --scope or --no-detect".into());
        }
        validate_rescan(root, &explicit_docs)?;
        rescan_project(root, &explicit_docs).map_err(|error| error.to_string())?
    } else {
        let harness = string_flag(flags, "harness").unwrap_or("");
        let scope = string_flag(flags, "scope").unwrap_or("");
        validate_onboard(
            root,
            &explicit_docs,
            string_flag(flags, "harness"),
            string_flag(flags, "scope"),
        )?;
        onboard_project(
            root,
            &OnboardOptions {
                harness: harness.to_owned(),
                scope: scope.to_owned(),
                explicit_docs,
                detect_docs: !flags.contains_key("no-detect"),
            },
        )
        .map_err(|error| error.to_string())?
    };
    print_json(&result)?;
    Ok(0)
}

const SCOPES: [&str; 3] = ["solo", "team", "enterprise"];
const INVALID_CONFIG: &str =
    "Invalid .aidlc/config.json: harness or scope is missing or unknown; correct the file";

fn config_path(root: &Path) -> PathBuf {
    root.join(".aidlc").join("config.json")
}

/// Node `loadConfig(root)`: the parsed configuration, `{}` when missing or malformed.
fn lenient_config(root: &Path) -> Value {
    dtech_aidlc::scope::load_config_unvalidated(root)
}

/// Node `ensureWithin(root, path)` as used for `--docs`: `path` is strictly inside `root`
/// (Windows paths compare without regard to ASCII case, like `path.relative`).
fn strictly_within(root: &Path, path: &Path) -> bool {
    let root: Vec<_> = root.components().collect();
    let path: Vec<_> = path.components().collect();
    let same = |a: &std::path::Component, b: &std::path::Component| {
        if cfg!(windows) {
            a.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
        } else {
            a == b
        }
    };
    path.len() > root.len() && root.iter().zip(&path).all(|(a, b)| same(a, b))
}

/// Node's `--docs` checks (`existsSync(resolve(root, item))`, then `ensureWithin`).
fn validate_docs(root: &Path, docs: &[String]) -> Result<(), String> {
    for item in docs {
        let path = orchestrate::resolve_like_node(&root.join(item))?;
        if !path.exists() {
            return Err(format!("--docs path does not exist: {item}"));
        }
        if !strictly_within(root, &path) {
            return Err(format!("--docs path must be inside the project: {item}"));
        }
    }
    Ok(())
}

/// The validation Node's router performs for `onboard` / `init` before anything is
/// written, in Node's order: `--docs`, `--scope`, `--harness`, then the existing
/// configuration (or the required `--harness` of a new project).
fn validate_onboard(
    root: &Path,
    docs: &[String],
    harness: Option<&str>,
    scope: Option<&str>,
) -> Result<(), String> {
    use dtech_aidlc::harness_commands::{
        configured_harnesses, harness_choice_error, list_harnesses,
    };
    validate_docs(root, docs)?;
    if let Some(scope) = scope {
        if !SCOPES.contains(&scope) {
            return Err("scope must be solo, team, or enterprise".into());
        }
    }
    if let Some(harness) = harness {
        if !list_harnesses().contains(&harness) {
            return Err(harness_choice_error());
        }
    }
    if !config_path(root).exists() {
        if harness.is_none() {
            return Err(
                "--harness is required for a new project: choose claude or deepseek".into(),
            );
        }
        return Ok(());
    }
    let config = lenient_config(root);
    let harnesses = configured_harnesses(&config);
    let existing = config.get("scope").and_then(Value::as_str);
    let Some(existing) = existing.filter(|scope| SCOPES.contains(scope) && !harnesses.is_empty())
    else {
        return Err(INVALID_CONFIG.into());
    };
    let requested = scope.unwrap_or(existing);
    if requested != existing {
        return Err(format!("Project default scope is {existing}. To change the default for new intents run: dtech-aidlc config scope {requested}"));
    }
    if let Some(harness) = harness.filter(|name| !harnesses.iter().any(|item| item == name)) {
        return Err(format!(
            "Project already uses harness {}. To add another harness run: dtech-aidlc harness add {harness}",
            harnesses.join(", ")
        ));
    }
    Ok(())
}

/// The validation Node's router performs for `onboard --rescan` before anything is
/// written: an initialized project, a readable knowledge manifest (required when
/// `--docs` is given), and the `--docs` paths.
fn validate_rescan(root: &Path, docs: &[String]) -> Result<(), String> {
    if !config_path(root).exists() {
        return Err(
            "Project is not initialized; run dtech-aidlc onboard --harness <claude|deepseek> first"
                .into(),
        );
    }
    let (exists, _) = dtech_aidlc::knowledge::knowledge_preflight(root)?;
    if !exists && !docs.is_empty() {
        // Node `onboardCommands(root).create`.
        let config = lenient_config(root);
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
        let create = match (harness, scope) {
            (Some(harness), Some(scope)) => {
                format!("dtech-aidlc onboard --harness {harness} --scope {scope}")
            }
            _ => "dtech-aidlc onboard --harness <claude|deepseek>".to_owned(),
        };
        let quoted = docs
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
            .join(" ");
        return Err(format!(
            "aidlc.knowledge.json does not exist; run `{create} {quoted}` instead"
        ));
    }
    validate_docs(root, docs)
}

/// Node `scope`, `status` and `intent list`: the methodology graph (Node `readGraph()`),
/// plus the validated project configuration for `scope` only (Node
/// `validateModelConfig`); `status` and `intent list` do not read the configuration.
fn run_read_only(
    key: &str,
    root: &Path,
    rest: &[String],
    flags: &BTreeMap<String, FlagValue>,
) -> Result<u8, String> {
    let graph = load_runtime_graph(application::external_engine().as_deref())?;
    let output = match key {
        "scope" => {
            let config = dtech_aidlc::scope::validate_model_config(root, graph.value())?;
            graph.scope_output(
                rest.first().map(String::as_str),
                string_flag(flags, "depth"),
                &config,
            )?
        }
        "status" => {
            let app = application::Project {
                root: root.to_path_buf(),
                graph,
                engine: orchestrate::Engine::embedded(root),
                caller: application::Caller::Cli,
            };
            application::status(&app, string_flag(flags, "intent"))?
        }
        _ => dtech_aidlc::status::list_intents(root, &graph)?,
    };
    print_json(&output)?;
    Ok(0)
}

fn string_flag<'a>(flags: &'a BTreeMap<String, FlagValue>, name: &str) -> Option<&'a str> {
    match flags.get(name) {
        Some(FlagValue::String(value)) => Some(value),
        _ => None,
    }
}

/// A list option's values, each also split on `,` and trimmed; empty items are refused.
fn list_flag(flags: &BTreeMap<String, FlagValue>, name: &str) -> Result<Vec<String>, String> {
    let raw = match flags.get(name) {
        Some(FlagValue::List(values)) => values.clone(),
        Some(FlagValue::String(value)) => vec![value.clone()],
        _ => return Ok(Vec::new()),
    };
    let mut values = Vec::new();
    for item in raw.iter().flat_map(|value| value.split(',')) {
        let item = item.trim();
        if item.is_empty() {
            return Err(format!("--{name} has an empty item"));
        }
        values.push(item.to_owned());
    }
    Ok(values)
}

fn print_json(value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    write_stdout(&format!("{text}\n"))
}

/// Node `out(value)`: a string is written as is, anything else as
/// `JSON.stringify(value, null, 2)` followed by a newline.
fn print_output(value: &Value) -> Result<(), String> {
    match value {
        Value::String(text) => write_stdout(text),
        other => print_json(other),
    }
}

fn write_stdout(text: &str) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|error| error.to_string())
}

/// Node `readGraph()` for orchestration: the methodology graph of the installed engine
/// (`engine`) or of this executable. The project configuration is not read here.
fn load_runtime_graph(engine: Option<&Path>) -> Result<dtech_aidlc::scope::Graph, String> {
    dtech_aidlc::scope::read_graph(engine)
}

/// Node `orchestrate next` / `orchestrate report` command handling.
fn run_orchestrate(
    key: &str,
    project: &Path,
    flags: &BTreeMap<String, FlagValue>,
) -> Result<u8, String> {
    // Node resolves --project (path.resolve) before anything else, then checks the
    // state directory and reads the methodology graph; the project configuration is read
    // later, by each action, as Node's orchestrateNext / orchestrateReport do.
    let app = application::open(project, application::Caller::Cli)?;
    if key == "orchestrate report" {
        // Node orchestrateReport reads the configuration with loadConfig: unvalidated,
        // and `{}` when it is missing or malformed.
        let units = list_flag(flags, "unit")?;
        if units.len() > 1 {
            return Err("--unit takes one work unit for orchestrate report".into());
        }
        let result = application::report(
            &app,
            &orchestrate::ReportOptions {
                stage: string_flag(flags, "stage").map(str::to_owned),
                result: string_flag(flags, "result").map(str::to_owned),
                approval: string_flag(flags, "approval").map(str::to_owned),
                detail: string_flag(flags, "detail").map(str::to_owned),
                approved_by: string_flag(flags, "approved-by").map(str::to_owned),
                approval_note: string_flag(flags, "approval-note").map(str::to_owned),
                needs: match flags.get("needs") {
                    Some(FlagValue::List(values)) => values.clone(),
                    Some(FlagValue::String(value)) => vec![value.clone()],
                    _ => Vec::new(),
                },
                unit: units.into_iter().next(),
            },
        )?;
        print_json(&result)?;
        return Ok(0);
    }
    // Node knowledgePreflight reads nothing for a project folder that does not exist
    // (no state directory, no manifest); orchestrateNext then reports it as having no
    // active intent, or as not initialized when an intent is given.
    application::knowledge_preflight(&app)?;
    let intent = intent_text(flags)?
        .map(|text| orchestrate::normalize_intent_text(&text))
        .transpose()?;
    if let Some(mode) = string_flag(flags, "mode") {
        if !matches!(mode, "human" | "ai") {
            return Err("mode must be human or ai".into());
        }
    }
    let force_resume = flags.contains_key("force-resume");
    if force_resume && string_flag(flags, "detail").is_none() {
        return Err("--force-resume requires --detail \"<reason>\"".into());
    }
    let claim = flags.contains_key("claim");
    let units = list_flag(flags, "unit")?;
    if units.len() > 1 {
        return Err("--unit takes one work unit for orchestrate next".into());
    }
    let unit = units.into_iter().next();
    if claim && unit.is_some() {
        return Err("--claim and --unit cannot be combined: --claim takes a new unit, --unit prints one already in flight".into());
    }
    if (claim || unit.is_some()) && intent.is_some() {
        return Err("--claim and --unit continue the active intent; they cannot start one".into());
    }
    // Node orchestrateNext validates the configuration first (validateModelConfig).
    let outcome = application::next(
        &app,
        &orchestrate::NextOptions {
            intent,
            scope: string_flag(flags, "scope").map(str::to_owned),
            depth: string_flag(flags, "depth").map(str::to_owned),
            mode: string_flag(flags, "mode").map(str::to_owned),
            approved_by: string_flag(flags, "approved-by").map(str::to_owned),
            detail: string_flag(flags, "detail").map(str::to_owned),
            force_resume,
            claim,
            unit,
            skip_prerequisite_checks: false,
        },
    )?;
    match outcome {
        orchestrate::NextOutcome::Blocked(payload) => {
            print_json(&payload)?;
            Ok(orchestrate::BLOCKED_EXIT_CODE)
        }
        orchestrate::NextOutcome::Directive(directive) => {
            print_json(&directive)?;
            Ok(0)
        }
        orchestrate::NextOutcome::Complete(payload) | orchestrate::NextOutcome::Busy(payload) => {
            print_json(&payload)?;
            Ok(0)
        }
    }
}

/// `orchestrate park` and `orchestrate revisit` (`queue` goes through `application`), with the same root
/// resolution, state-directory check and methodology graph as `run_orchestrate`.
fn run_unit_queue(
    key: &str,
    project: &Path,
    flags: &BTreeMap<String, FlagValue>,
) -> Result<u8, String> {
    let root = orchestrate::resolve_like_node(project)?;
    dtech_aidlc::audit::assert_safe_state_dir(&root)?;
    let graph = load_runtime_graph(application::external_engine().as_deref())?;
    let config = dtech_aidlc::scope::load_config_unvalidated(&root);
    let options = dtech_aidlc::unit_commands::UnitOptions {
        units: list_flag(flags, "unit")?,
        confirm: flags.contains_key("confirm"),
        kind: string_flag(flags, "kind").map(str::to_owned),
        blocked_by: list_flag(flags, "blocked-by")?,
        detail: string_flag(flags, "detail").map(str::to_owned),
        approved_by: string_flag(flags, "approved-by").map(str::to_owned),
    };
    let output = dtech_aidlc::unit_commands::run(key, &root, graph.value(), &config, &options)?;
    print_json(&output)?;
    Ok(0)
}

/// Node's intent sources for `orchestrate next`: --intent, --description or
/// --intent-file (a path, or `-` for standard input), at most one of them.
fn intent_text(flags: &BTreeMap<String, FlagValue>) -> Result<Option<String>, String> {
    let given = ["intent", "description", "intent-file"]
        .into_iter()
        .filter(|name| flags.contains_key(*name))
        .count();
    if given > 1 {
        return Err(
            "Give the intent with only one of --intent, --description or --intent-file".into(),
        );
    }
    let Some(path) = string_flag(flags, "intent-file") else {
        return Ok(string_flag(flags, "intent")
            .or_else(|| string_flag(flags, "description"))
            .map(str::to_owned));
    };
    if path == "-" {
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() {
            return Err(
                "--intent-file - reads the intent from standard input; pipe the text in".into(),
            );
        }
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read the intent from standard input: {error}"))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if text.len() > 100_000 {
            return Err("Intent file is larger than 100000 bytes".into());
        }
        return Ok(Some(text));
    }
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let shown = absolute.display();
    if !absolute.exists() {
        return Err(format!("Intent file not found: {shown}"));
    }
    let metadata = fs::symlink_metadata(&absolute).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!("Refusing to read symlink intent file: {shown}"));
    }
    if !metadata.is_file() {
        return Err(format!("Intent file is not a regular file: {shown}"));
    }
    let bytes = fs::read(&absolute).map_err(|error| error.to_string())?;
    if metadata.len() > 100_000 {
        return Err("Intent file is larger than 100000 bytes".into());
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}
