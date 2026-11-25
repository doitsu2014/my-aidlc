//! Read-only projections for `config scope`, `config mode`, and `config gate-mode`.
//!
//! These helpers mirror the corresponding Node CLI queries and never mutate the
//! configuration. Callers provide the effective configured status (scope and
//! mode) because harness discovery belongs to command/runtime wiring.

use serde_json::{json, Value};

const SCOPES: &[&str] = &["solo", "team", "enterprise"];
const MODES: &[&str] = &["human", "ai"];
/// Return the effective default scope and whether the project is configured.
pub fn scope(config: &Value, configured: bool) -> Value {
    let scope = if configured
        && config["scope"]
            .as_str()
            .is_some_and(|s| SCOPES.contains(&s))
    {
        config["scope"].clone()
    } else {
        json!("solo")
    };
    json!({"scope": scope, "configured": configured})
}

/// Return the default orchestration mode (human when unset) and configured flag.
pub fn mode(config: &Value, configured: bool) -> Result<Value, String> {
    let value = config.get("mode");
    if value.is_some()
        && !value
            .and_then(Value::as_str)
            .is_some_and(|s| MODES.contains(&s))
    {
        return Err("mode must be human or ai".into());
    }
    Ok(json!({"mode": value.and_then(Value::as_str).unwrap_or("human"), "configured": configured}))
}

/// List gate pins, or query one gate's effective configured override.
///
/// With no `gate` argument this returns gates in graph phase order, including
/// repeated gate IDs if the graph contains them, matching the Node CLI mapping.
pub fn gate_mode(config: &Value, graph: &Value, gate: Option<&str>) -> Result<Value, String> {
    let gates: Vec<&str> = graph["phases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|phase| phase["gate"].as_str())
        .collect();
    let gate_modes = config
        .get("gateModes")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(name) = gate {
        if !gates.contains(&name) {
            return Err(format!(
                "Unknown gate '{name}'. Gates: {}",
                gates.join(", ")
            ));
        }
        let value = gate_modes
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or("default");
        Ok(json!({"gate": name, "mode": value}))
    } else {
        Ok(json!({"gateModes": gate_modes, "gates": gates}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_read_uses_default_and_configured_scope() {
        assert_eq!(
            scope(&json!({}), false),
            json!({"scope":"solo","configured":false})
        );
        assert_eq!(
            scope(&json!({"scope":"team"}), true),
            json!({"scope":"team","configured":true})
        );
        assert_eq!(
            scope(&json!({"scope":"bad"}), false),
            json!({"scope":"solo","configured":false})
        );
    }

    #[test]
    fn mode_read_defaults_and_validates() {
        assert_eq!(
            mode(&json!({}), false).unwrap(),
            json!({"mode":"human","configured":false})
        );
        assert_eq!(
            mode(&json!({"mode":"ai"}), true).unwrap(),
            json!({"mode":"ai","configured":true})
        );
        assert_eq!(
            mode(&json!({"mode":"invalid"}), true).unwrap_err(),
            "mode must be human or ai"
        );
    }

    #[test]
    fn gate_mode_lists_and_queries_without_reordering() {
        let graph = json!({"phases":[{"gate":"design"},{"gate":"release"}]});
        let config = json!({"gateModes":{"release":"human"}});
        assert_eq!(
            gate_mode(&config, &graph, None).unwrap(),
            json!({"gateModes":{"release":"human"},"gates":["design","release"]})
        );
        assert_eq!(
            gate_mode(&config, &graph, Some("design")).unwrap(),
            json!({"gate":"design","mode":"default"})
        );
        assert_eq!(
            gate_mode(&config, &graph, Some("release")).unwrap(),
            json!({"gate":"release","mode":"human"})
        );
        assert_eq!(
            gate_mode(&config, &graph, Some("unknown")).unwrap_err(),
            "Unknown gate 'unknown'. Gates: design, release"
        );
    }
}
