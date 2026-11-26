//! Named prerequisite configuration validation and check execution.
//!
//! This module intentionally uses `serde_json::Value` at the configuration boundary. Callers
//! can validate declarations before using `run_prerequisites`; execution invokes the configured
//! owner-authored command through the platform shell.

use serde_json::{Map, Value};
use std::collections::HashSet;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_TIMEOUT_SECONDS: u64 = 60;
const OUTPUT_LIMIT: usize = 600;
const CHECK_LIMIT: usize = 1_000;
const HINT_LIMIT: usize = 500;

/// Return the `prerequisites` object, or an empty object for missing/invalid config values.
pub fn prerequisites_of(config: &Value) -> Map<String, Value> {
    config
        .get("prerequisites")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Validate a prerequisite declaration. `graph` is an optional JSON object with `stages` and
/// `phases` arrays, each containing objects with string `id` fields.
pub fn validate_prerequisite<'a>(
    name: &str,
    definition: &'a Value,
    graph: Option<&Value>,
) -> Result<&'a Value, String> {
    if !valid_name(name) {
        return Err(format!(
            "Prerequisite name '{name}' must be 1-32 lowercase letters, digits or hyphens"
        ));
    }
    let object = definition
        .as_object()
        .ok_or_else(|| format!("Prerequisite {name} must be an object"))?;

    let check = object.get("check").and_then(Value::as_str).unwrap_or("");
    if check.trim().is_empty() || check.len() > CHECK_LIMIT || check.chars().any(char::is_control) {
        return Err(format!(
            "Prerequisite {name} needs a one-line --check command"
        ));
    }
    if let Some(hint) = object.get("hint") {
        if !hint.is_string() || hint.as_str().unwrap().len() > HINT_LIMIT {
            return Err(format!(
                "Prerequisite {name} hint must be text up to 500 characters"
            ));
        }
    }
    if let Some(timeout) = object.get("timeoutSeconds") {
        let seconds = timeout.as_u64();
        if seconds.is_none() || !(1..=600).contains(&seconds.unwrap()) {
            return Err(format!(
                "Prerequisite {name} timeoutSeconds must be an integer from 1 to 600"
            ));
        }
    }
    if let Some(stages) = object.get("stages") {
        let ids = stages.as_array().ok_or_else(|| {
            format!("Prerequisite {name} stages must be a list of stage or phase ids")
        })?;
        if ids.iter().any(|id| id.as_str().is_none_or(str::is_empty)) {
            return Err(format!(
                "Prerequisite {name} stages must be a list of stage or phase ids"
            ));
        }
        if let Some(graph) = graph {
            let known = graph_ids(graph);
            let unknown: Vec<&str> = ids
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| !known.contains(*id))
                .collect();
            if !unknown.is_empty() {
                return Err(format!(
                    "Prerequisite {name} names unknown stage or phase: {}",
                    unknown.join(", ")
                ));
            }
        }
    }
    Ok(definition)
}

/// Fail if any requested names have no declaration in the configuration.
pub fn require_known_prerequisites(config: &Value, names: &[String]) -> Result<(), String> {
    let declared = prerequisites_of(config);
    let unknown: Vec<&str> = names
        .iter()
        .filter(|name| !declared.contains_key(name.as_str()))
        .map(String::as_str)
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let known: Vec<&str> = declared.keys().map(String::as_str).collect();
    let listed = if known.is_empty() {
        "None are declared.".to_owned()
    } else {
        format!("Declared: {}.", known.join(", "))
    };
    Err(format!(
        "Unknown prerequisite {}. {} Declare one with: dtech-aidlc config prerequisite set NAME --check \"COMMAND\" [--hint TEXT]",
        unknown.join(", "), listed
    ))
}

/// Return prerequisite names whose configured stages include this stage's id or phase.
pub fn preflight_names(config: &Value, stage: &Value) -> Vec<String> {
    let stage_id = stage.get("id").and_then(Value::as_str);
    let phase = stage.get("phase").and_then(Value::as_str);
    prerequisites_of(config)
        .into_iter()
        .filter_map(|(name, definition)| {
            let applies = definition
                .get("stages")
                .and_then(Value::as_array)
                .is_some_and(|ids| {
                    ids.iter().any(|id| {
                        id.as_str()
                            .is_some_and(|id| Some(id) == stage_id || Some(id) == phase)
                    })
                });
            applies.then_some(name)
        })
        .collect()
}

/// Refuse a declaration that has no command to run: `check` missing, not a string, or
/// empty. Node cannot run such a declaration either (`spawnSync` throws a type error);
/// running an empty shell command instead would report the prerequisite as healthy.
pub fn require_runnable(name: &str, definition: &Value) -> Result<(), String> {
    match definition.get("check") {
        Some(Value::String(check)) if !check.is_empty() => Ok(()),
        _ => Err(format!(
            "Prerequisite {name} needs a one-line --check command"
        )),
    }
}

/// Execute one configured check in `root`, with a timeout and bounded failure output.
/// A declaration without a runnable command ([`require_runnable`]) fails without
/// starting a shell.
pub fn run_prerequisite(root: &str, name: &str, definition: &Value) -> Value {
    if let Err(problem) = require_runnable(name, definition) {
        let check = definition.get("check").cloned().unwrap_or(Value::Null);
        let mut result =
            result_failure(name, "", definition, timestamp_now(), None, false, &problem);
        result["check"] = check;
        return result;
    }
    let check = definition
        .get("check")
        .and_then(Value::as_str)
        .unwrap_or("");
    let timeout_seconds = definition
        .get("timeoutSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS);
    let timeout = Duration::from_secs(timeout_seconds);
    let started_at = timestamp_now();
    let (shell, args) = shell_command(check);

    let mut child = match Command::new(shell)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return result_failure(
                name,
                check,
                definition,
                started_at,
                None,
                false,
                &error.to_string(),
            );
        }
    };

    // Drain both pipes concurrently so a verbose check cannot deadlock while filling a pipe.
    let stdout = child.stdout.take().map(read_pipe);
    let stderr = child.stderr.take().map(read_pipe);
    let began = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if began.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Err(_) => {
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };
    // A timed-out shell can leave descendants holding its pipes open. Do not
    // wait indefinitely for reader threads in that case; their handles detach.
    let stdout = if timed_out {
        Vec::new()
    } else {
        stdout
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default()
    };
    let stderr = if timed_out {
        Vec::new()
    } else {
        stderr
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default()
    };
    let ok = !timed_out && status.as_ref().is_some_and(|status| status.success());
    let exit_code = status.and_then(|status| status.code());

    let hint = definition
        .get("hint")
        .cloned()
        .filter(|value| !value.is_null())
        .unwrap_or(Value::Null);
    let mut result = serde_json::json!({
        "name": name,
        "ok": ok,
        "check": check,
        "hint": hint,
        "exitCode": exit_code,
        "startedAt": started_at,
    });
    if timed_out {
        result["timedOut"] = Value::Bool(true);
    }
    if !ok {
        let mut output = String::from_utf8_lossy(&stdout).into_owned();
        output.push('\n');
        output.push_str(&String::from_utf8_lossy(&stderr));
        if timed_out {
            output.push_str("\ncheck timed out");
        }
        result["output"] = Value::String(tail(&output));
    }
    result
}

/// Run each distinct requested name once. A removed declaration no longer blocks the stage.
pub fn run_prerequisites(root: &str, config: &Value, names: &[String]) -> Vec<Value> {
    let declared = prerequisites_of(config);
    let mut seen = HashSet::new();
    names
        .iter()
        .filter(|name| seen.insert(name.as_str()))
        .map(|name| match declared.get(name) {
            Some(definition) => run_prerequisite(root, name, definition),
            None => serde_json::json!({
                "name": name,
                "ok": true,
                "check": null,
                "hint": null,
                "exitCode": null,
                "undeclared": true,
            }),
        })
        .collect()
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn graph_ids(graph: &Value) -> HashSet<String> {
    ["stages", "phases"]
        .into_iter()
        .filter_map(|key| graph.get(key).and_then(Value::as_array))
        .flat_map(|items| items.iter())
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

#[cfg(windows)]
fn shell_command(check: &str) -> (&'static str, Vec<String>) {
    ("cmd", vec!["/C".to_owned(), check.to_owned()])
}

#[cfg(not(windows))]
fn shell_command(check: &str) -> (&'static str, Vec<String>) {
    ("sh", vec!["-c".to_owned(), check.to_owned()])
}

fn read_pipe<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut retained = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    retained.extend_from_slice(&buffer[..count]);
                    // Retain only a bounded suffix while continuing to drain the pipe.
                    const CAPACITY: usize = 4096;
                    if retained.len() > CAPACITY {
                        retained.drain(..retained.len() - CAPACITY);
                    }
                }
            }
        }
        retained
    })
}

fn result_failure(
    name: &str,
    check: &str,
    definition: &Value,
    started_at: String,
    exit_code: Option<i32>,
    timed_out: bool,
    output: &str,
) -> Value {
    let mut result = serde_json::json!({
        "name": name,
        "ok": false,
        "check": check,
        "hint": definition.get("hint").cloned().unwrap_or(Value::Null),
        "exitCode": exit_code,
        "startedAt": started_at,
        "output": tail(output),
    });
    if timed_out {
        result["timedOut"] = Value::Bool(true);
    }
    result
}

fn tail(text: &str) -> String {
    let value = text.trim();
    if value.len() <= OUTPUT_LIMIT {
        value.to_owned()
    } else {
        let start = (value.len() - OUTPUT_LIMIT..value.len())
            .find(|&index| value.is_char_boundary(index))
            .unwrap_or(value.len());
        format!("...{}", &value[start..])
    }
}

// Generate an RFC3339 UTC timestamp without adding a date/time dependency.
fn timestamp_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        day_seconds / 3600,
        day_seconds % 3600 / 60,
        day_seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tail_handles_multibyte_utf8_at_truncation_boundary() {
        let input = format!("{}x", "é".repeat(OUTPUT_LIMIT));
        let result = tail(&input);
        assert!(result.starts_with("..."));
        assert!(result.ends_with('x'));
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn validates_name_definition_and_limits() {
        assert!(
            validate_prerequisite("build-tools", &json!({"check":"cargo --version"}), None).is_ok()
        );
        assert!(validate_prerequisite("Bad", &json!({"check":"x"}), None)
            .unwrap_err()
            .contains("name"));
        assert!(validate_prerequisite("ok", &json!([]), None)
            .unwrap_err()
            .contains("object"));
        assert!(validate_prerequisite("ok", &json!({"check":" \n"}), None).is_err());
        assert!(
            validate_prerequisite("ok", &json!({"check":"x", "timeoutSeconds":0}), None).is_err()
        );
        assert!(
            validate_prerequisite("ok", &json!({"check":"x", "hint":"h".repeat(501)}), None)
                .is_err()
        );
    }

    #[test]
    fn validates_known_graph_ids_and_collects_preflight() {
        let graph = json!({"stages":[{"id":"develop"}],"phases":[{"id":"review"}]});
        let config = json!({"prerequisites":{
            "docker":{"check":"docker info","stages":["develop"]},
            "db":{"check":"true","stages":["review"]},
            "free":{"check":"true"}
        }});
        assert!(
            validate_prerequisite("docker", &config["prerequisites"]["docker"], Some(&graph))
                .is_ok()
        );
        assert!(validate_prerequisite(
            "docker",
            &json!({"check":"x","stages":["missing"]}),
            Some(&graph)
        )
        .unwrap_err()
        .contains("missing"));
        assert_eq!(
            preflight_names(&config, &json!({"id":"stage-1","phase":"develop"})),
            vec!["docker"]
        );
    }

    #[test]
    fn checks_names_and_runs_deduplicated_commands() {
        let config = json!({"prerequisites":{"ok":{"check":"exit 0"},"bad":{"check":"exit 7"}}});
        assert!(require_known_prerequisites(&config, &["ok".into()]).is_ok());
        assert!(require_known_prerequisites(&config, &["unknown".into()])
            .unwrap_err()
            .contains("Unknown prerequisite"));
        let results = run_prerequisites(
            ".",
            &config,
            &["ok".into(), "ok".into(), "gone".into(), "bad".into()],
        );
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["ok"], true);
        assert_eq!(results[1]["undeclared"], true);
        assert_eq!(results[2]["exitCode"], 7);
    }

    #[test]
    #[cfg(unix)]
    fn timeout_does_not_wait_for_descendant_pipe_handles() {
        let started = Instant::now();
        let result = run_prerequisite(".", "slow", &json!({"check":"sleep 4", "timeoutSeconds":1}));
        assert_eq!(result["timedOut"], true);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn declaration_without_a_command_fails_instead_of_running_an_empty_shell() {
        for definition in [
            json!({"check": 5, "hint": "start docker"}),
            json!({"hint": "no command"}),
            json!({"check": ""}),
        ] {
            assert_eq!(
                require_runnable("docker", &definition).unwrap_err(),
                "Prerequisite docker needs a one-line --check command"
            );
            let result = run_prerequisite(".", "docker", &definition);
            assert_eq!(result["ok"], false, "{definition}");
            assert_eq!(result["exitCode"], Value::Null);
        }
        assert!(require_runnable("docker", &json!({"check": "docker info"})).is_ok());
    }

    #[test]
    fn check_timeout_is_reported() {
        #[cfg(not(windows))]
        let config = json!({"prerequisites":{"slow":{"check":"sleep 1","timeoutSeconds":1}}});
        #[cfg(windows)]
        let config = json!({"prerequisites":{"slow":{"check":"ping -n 3 127.0.0.1 >NUL","timeoutSeconds":1}}});
        let result = run_prerequisites(".", &config, &["slow".into()]);
        assert_eq!(result[0]["ok"], false);
        assert_eq!(result[0]["timedOut"], true);
    }
}
