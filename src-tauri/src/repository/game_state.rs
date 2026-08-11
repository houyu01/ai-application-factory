//! Runtime choice-state rules that preserve consequential decisions after video paths merge.

use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

/// Validate the edge-owned state contract before a creator manually saves it.
pub(super) fn normalize_edge_conditions(value: Option<&Value>) -> AppResult<Value> {
    let Some(value) = value else {
        return Ok(json!({}));
    };
    let object = value
        .as_object()
        .ok_or_else(|| AppError::BadRequest("conditions 必须是对象".to_owned()))?;
    if object.keys().any(|key| key != "requires" && key != "set") {
        return Err(AppError::BadRequest(
            "conditions 仅支持 requires 和 set".to_owned(),
        ));
    }
    for kind in ["requires", "set"] {
        let Some(entries) = object.get(kind) else {
            continue;
        };
        let entries = entries
            .as_object()
            .ok_or_else(|| AppError::BadRequest(format!("conditions.{kind} 必须是对象")))?;
        if entries
            .iter()
            .any(|(key, value)| !valid_state_key(key) || !valid_state_value(value))
        {
            return Err(AppError::BadRequest(
                "状态键须为 snake_case，状态值须为字符串、数字或布尔值".to_owned(),
            ));
        }
    }
    Ok(value.clone())
}

/// Recover the durable state recorded after previous choices, treating legacy sessions as empty state.
pub(super) fn session_state(session: &Value) -> Value {
    serde_json::from_str(session["state_json"].as_str().unwrap_or("{}"))
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// Return whether every state requirement attached to an edge holds for the active session.
pub(super) fn conditions_match(state: &Value, conditions: &Value) -> bool {
    let Some(requires) = conditions.get("requires") else {
        return true;
    };
    requires.as_object().is_some_and(|rules| {
        rules
            .iter()
            .all(|(key, value)| state.get(key) == Some(value))
    })
}

/// Apply state fields emitted by the chosen edge before the next playable node is returned.
pub(super) fn apply_state_changes(state: &mut Value, conditions: &Value) {
    let Some(changes) = conditions.get("set").and_then(Value::as_object) else {
        return;
    };
    let object = state.as_object_mut().expect("session state is an object");
    for (key, value) in changes {
        object.insert(key.to_owned(), value.clone());
    }
}

fn valid_state_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn valid_state_value(value: &Value) -> bool {
    value.is_string() || value.is_number() || value.is_boolean()
}
