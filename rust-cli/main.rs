use dtech_aidlc::args::{command_flags, command_key, command_max_args, parse_args, FlagValue};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dtech-aidlc: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let parsed = parse_args(&args).map_err(|e| e.to_string())?;
    if parsed.flags.contains_key("version")
        || parsed.positional.first().is_some_and(|a| a == "version")
    {
        println!("dtech-aidlc {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if parsed.flags.contains_key("help")
        || parsed.positional.is_empty()
        || parsed.positional.first().is_some_and(|a| a == "help")
    {
        print!(
            "dtech-aidlc {}\n\n{}",
            env!("CARGO_PKG_VERSION"),
            include_str!("help.txt")
        );
        return Ok(());
    }
    let key = command_key(&parsed.positional).expect("nonempty positionals");
    let allowed = command_flags(&key);
    let invalid = parsed.unknown.first().map(String::as_str).or_else(|| {
        parsed
            .flags
            .keys()
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
    if key == "completion zsh" {
        print!("{}", include_str!("../core/tools/completion.zsh"));
        return Ok(());
    }
    let project = match parsed.flags.get("project") {
        Some(FlagValue::String(path)) => std::path::PathBuf::from(path),
        _ => env::current_dir().map_err(|e| e.to_string())?,
    };
    if key == "scope" {
        let engine = env::var_os("DTECH_AIDLC_HOME")
            .map(std::path::PathBuf::from)
            .and_then(|home| {
                [home.join("core"), home].into_iter().find(|candidate| {
                    candidate.join("data/phase-graph.json").exists()
                        || candidate.join("methodology").exists()
                })
            });
        let (graph, config) = if let Some(engine) = engine {
            dtech_aidlc::scope::load_graph(engine, &project)?
        } else {
            dtech_aidlc::scope::load_embedded_graph(&project)?
        };
        let scope = parsed.positional.get(1).map(String::as_str);
        let depth = match parsed.flags.get("depth") {
            Some(FlagValue::String(value)) => Some(value.as_str()),
            _ => None,
        };
        let output = graph.scope_output(scope, depth, &config)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    Err(
        "Rust migration in progress; command not yet implemented. Use the existing Node.js CLI."
            .into(),
    )
}
