use std::collections::BTreeMap;
use std::fmt;

/// Type accepted by a command-line option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionType {
    Boolean,
    String,
    List,
}

/// Parsed flag values. List-valued options accumulate each occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlagValue {
    Boolean(bool),
    String(String),
    List(Vec<String>),
}

/// Result of parsing arguments, including unknown option names for routing-level validation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedArgs {
    pub positional: Vec<String>,
    pub flags: BTreeMap<String, FlagValue>,
    pub unknown: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArgsError(pub String);

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ArgsError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionSpec {
    pub name: &'static str,
    pub kind: OptionType,
}

const GLOBAL_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "project",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "help",
        kind: OptionType::Boolean,
    },
    OptionSpec {
        name: "version",
        kind: OptionType::Boolean,
    },
];

/// Commands whose first two positional words form a single command key.
pub const TWO_WORD_COMMANDS: &[&str] = &[
    "orchestrate",
    "config",
    "repos",
    "deps",
    "completion",
    "intent",
    "knowledge",
    "harness",
    "ids",
];

const INIT_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "harness",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "scope",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "docs",
        kind: OptionType::List,
    },
    OptionSpec {
        name: "no-detect",
        kind: OptionType::Boolean,
    },
    OptionSpec {
        name: "rescan",
        kind: OptionType::Boolean,
    },
];
const ORCHESTRATE_NEXT_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "intent",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "description",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "scope",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "intent-file",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "depth",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "mode",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "approved-by",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "detail",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "force-resume",
        kind: OptionType::Boolean,
    },
];
const ORCHESTRATE_REPORT_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "result",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "approval",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "detail",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "stage",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "approved-by",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "approval-note",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "needs",
        kind: OptionType::List,
    },
];
const IDS_CHECK_FLAGS: &[OptionSpec] = &[OptionSpec {
    name: "stage",
    kind: OptionType::String,
}];
const SCOPE_FLAGS: &[OptionSpec] = &[OptionSpec {
    name: "depth",
    kind: OptionType::String,
}];
const STATUS_FLAGS: &[OptionSpec] = &[OptionSpec {
    name: "intent",
    kind: OptionType::String,
}];
const INTENT_DETAIL_FLAGS: &[OptionSpec] = &[OptionSpec {
    name: "detail",
    kind: OptionType::String,
}];
const INTENT_DEPTH_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "approved-by",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "detail",
        kind: OptionType::String,
    },
];
const CONFIG_PREREQUISITE_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "check",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "hint",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "applies-to",
        kind: OptionType::List,
    },
];
const REPOS_ADD_FLAGS: &[OptionSpec] = &[OptionSpec {
    name: "ref",
    kind: OptionType::String,
}];
const REPOS_PULL_FLAGS: &[OptionSpec] = &[OptionSpec {
    name: "file",
    kind: OptionType::String,
}];
const KNOWLEDGE_SOURCES_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "kind",
        kind: OptionType::String,
    },
    OptionSpec {
        name: "description",
        kind: OptionType::String,
    },
];
const HARNESS_REFRESH_FLAGS: &[OptionSpec] = &[
    OptionSpec {
        name: "dry-run",
        kind: OptionType::Boolean,
    },
    OptionSpec {
        name: "force",
        kind: OptionType::Boolean,
    },
];

/// Returns the option definitions for a command key (global flags are available separately).
pub fn command_flags(command: &str) -> Option<&'static [OptionSpec]> {
    Some(match command {
        "init" | "onboard" => INIT_FLAGS,
        "orchestrate next" => ORCHESTRATE_NEXT_FLAGS,
        "orchestrate report" => ORCHESTRATE_REPORT_FLAGS,
        "ids check" => IDS_CHECK_FLAGS,
        "scope" => SCOPE_FLAGS,
        "status" => STATUS_FLAGS,
        "doctor" | "intent list" | "intent resume" | "intent assign" | "config agents"
        | "config scope" | "config mode" | "config gate-mode" | "harness add" | "repos list"
        | "repos remove" | "deps add" | "deps list" | "deps pull" | "completion zsh"
        | "knowledge init" | "knowledge list" | "knowledge show" | "knowledge check"
        | "knowledge index" | "harness status" => &[],
        "intent pause" | "intent abandon" | "intent rework" => INTENT_DETAIL_FLAGS,
        "intent depth" => INTENT_DEPTH_FLAGS,
        "config prerequisite" => CONFIG_PREREQUISITE_FLAGS,
        "repos add" => REPOS_ADD_FLAGS,
        "repos pull" => REPOS_PULL_FLAGS,
        "knowledge sources" => KNOWLEDGE_SOURCES_FLAGS,
        "harness refresh" => HARNESS_REFRESH_FLAGS,
        _ => return None,
    })
}

/// Returns the positional-argument limit when one is specified for the command.
pub fn command_max_args(command: &str) -> Option<usize> {
    Some(match command {
        "init" | "onboard" | "status" | "doctor" | "orchestrate next" | "orchestrate report"
        | "repos list" | "deps list" | "completion zsh" | "ids check" | "intent list"
        | "intent pause" | "intent rework" | "knowledge init" | "knowledge list"
        | "knowledge check" | "harness status" => 0,
        "scope" | "repos pull" | "deps pull" | "intent resume" | "intent abandon"
        | "intent depth" | "intent assign" | "knowledge show" | "harness add"
        | "harness refresh" | "config scope" | "config mode" => 1,
        "repos add" | "deps add" | "config gate-mode" | "config prerequisite" => 2,
        "config agents" => 3,
        "knowledge sources" => 3,
        _ => return None,
    })
}

/// Produces the command key, combining the first two words for commands in [`TWO_WORD_COMMANDS`].
pub fn command_key(positionals: &[String]) -> Option<String> {
    let first = positionals.first()?;
    if positionals
        .get(1)
        .is_some_and(|_| TWO_WORD_COMMANDS.contains(&first.as_str()))
    {
        Some(format!("{} {}", first, positionals[1]))
    } else {
        Some(first.clone())
    }
}

/// Parses using the complete global and command option vocabulary from `cli-spec.mjs`.
pub fn parse_args(args: &[String]) -> Result<ParsedArgs, ArgsError> {
    let mut specs = GLOBAL_FLAGS.to_vec();
    for command in COMMANDS {
        if let Some(options) = command_flags(command) {
            for option in options {
                if !specs.iter().any(|existing| existing.name == option.name) {
                    specs.push(*option);
                }
            }
        }
    }
    parse_args_with_types(args, &specs)
}

/// Parses an argument vector against a supplied option table.
pub fn parse_args_with_types(
    args: &[String],
    option_types: &[OptionSpec],
) -> Result<ParsedArgs, ArgsError> {
    let mut parsed = ParsedArgs::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            parsed.positional.extend(args[i + 1..].iter().cloned());
            break;
        }
        if !arg.starts_with("--") {
            parsed.positional.push(arg.clone());
            i += 1;
            continue;
        }

        let eq = arg.find('=');
        let name = &arg[2..eq.unwrap_or(arg.len())];
        let Some(spec) = option_types.iter().find(|spec| spec.name == name) else {
            parsed.unknown.push(name.to_owned());
            i += 1;
            continue;
        };
        match spec.kind {
            OptionType::Boolean => {
                if eq.is_some() {
                    return Err(ArgsError(format!("Option --{name} does not take a value")));
                }
                parsed
                    .flags
                    .insert(name.to_owned(), FlagValue::Boolean(true));
            }
            kind => {
                let value = if let Some(eq) = eq {
                    arg[eq + 1..].to_owned()
                } else {
                    i += 1;
                    args.get(i).cloned().unwrap_or_default()
                };
                if value.is_empty() || value == "--" || is_known_option(&value, option_types) {
                    return Err(ArgsError(format!(
                        "Option --{name} requires a non-empty value"
                    )));
                }
                match kind {
                    OptionType::String => {
                        if parsed.flags.contains_key(name) {
                            return Err(ArgsError(format!(
                                "Option --{name} was given more than once"
                            )));
                        }
                        parsed
                            .flags
                            .insert(name.to_owned(), FlagValue::String(value));
                    }
                    OptionType::List => match parsed
                        .flags
                        .entry(name.to_owned())
                        .or_insert_with(|| FlagValue::List(Vec::new()))
                    {
                        FlagValue::List(values) => values.push(value),
                        _ => unreachable!("a list option always has a list value"),
                    },
                    OptionType::Boolean => unreachable!(),
                }
            }
        }
        i += 1;
    }
    Ok(parsed)
}

fn is_known_option(token: &str, option_types: &[OptionSpec]) -> bool {
    option_types.iter().any(|spec| {
        token == format!("--{}", spec.name) || token.starts_with(&format!("--{}=", spec.name))
    })
}

const COMMANDS: &[&str] = &[
    "init",
    "onboard",
    "orchestrate next",
    "orchestrate report",
    "ids check",
    "scope",
    "status",
    "doctor",
    "intent list",
    "intent pause",
    "intent resume",
    "intent abandon",
    "intent depth",
    "intent rework",
    "intent assign",
    "config agents",
    "config scope",
    "config mode",
    "config gate-mode",
    "config prerequisite",
    "harness add",
    "repos add",
    "repos pull",
    "repos list",
    "repos remove",
    "deps add",
    "deps list",
    "deps pull",
    "completion zsh",
    "knowledge init",
    "knowledge sources",
    "knowledge list",
    "knowledge show",
    "knowledge check",
    "knowledge index",
    "harness status",
    "harness refresh",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn parses_typed_flags_unknowns_and_terminator() {
        let parsed = parse_args(&strings(&[
            "--intent=hello",
            "--docs",
            "a",
            "--docs",
            "b",
            "--wat",
            "x",
            "status",
            "--",
            "--help",
        ]))
        .unwrap();
        assert_eq!(parsed.positional, ["x", "status", "--help"]);
        assert_eq!(parsed.flags["intent"], FlagValue::String("hello".into()));
        assert_eq!(
            parsed.flags["docs"],
            FlagValue::List(vec!["a".into(), "b".into()])
        );
        assert_eq!(parsed.unknown, ["wat"]);
    }

    #[test]
    fn rejects_invalid_values_and_duplicate_scalar_options() {
        for args in [
            vec!["--intent"],
            vec!["--intent="],
            vec!["--intent", "--"],
            vec!["--intent", "--project"],
            vec!["--intent", "a", "--intent", "b"],
        ] {
            assert!(
                parse_args(&strings(&args)).is_err(),
                "expected error for {args:?}"
            );
        }
        assert!(parse_args(&strings(&["--help=true"]))
            .unwrap_err()
            .to_string()
            .contains("does not take a value"));
    }

    #[test]
    fn resolves_command_keys_and_positional_limits() {
        assert_eq!(
            command_key(&strings(&["knowledge", "sources", "add"])).as_deref(),
            Some("knowledge sources")
        );
        assert_eq!(
            command_key(&strings(&["status", "extra"])).as_deref(),
            Some("status")
        );
        assert_eq!(command_key(&[]), None);
        assert_eq!(command_max_args("config agents"), Some(3));
        assert_eq!(command_max_args("orchestrate next"), Some(0));
        assert_eq!(command_max_args("not-a-command"), None);
        assert_eq!(command_flags("orchestrate next").unwrap().len(), 9);
    }
}
