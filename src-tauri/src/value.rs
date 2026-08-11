//! JSON and time helpers used to preserve the API dictionaries emitted by the Python service.

use chrono::{SecondsFormat, Utc};
use rusqlite::{types::ValueRef, Row};
use serde_json::{Map, Value};
use uuid::Uuid;

/// Public durable task states retained verbatim for existing project data and UI labels.
pub const NOT_GENERATED: &str = "未生成";
pub const GENERATING: &str = "生成中";
pub const SUCCEEDED: &str = "生成成功";
pub const FAILED: &str = "生成失败";
pub const CANCELLED: &str = "已取消";

const GAME_PROMPT_GROUNDING: &str = "原始剧情依据（必须画面化）：";
const GAME_PROMPT_EXECUTION_RULE: &str = "执行约束：镜头中的场景、角色、关键动作与结果必须直接来自上述原始剧情依据，不得以泛化情节替代。";

/// Keep the editable node screenplay as a non-optional source of truth for video generation.
pub(crate) fn ground_game_video_prompt(prompt: &str, original_text: &str) -> String {
    let prompt = remove_game_prompt_grounding(prompt.trim());
    let original_text = original_text.trim();
    if prompt.is_empty() || original_text.is_empty() {
        return prompt.to_owned();
    }
    format!("{prompt}\n\n{GAME_PROMPT_GROUNDING}{original_text}\n{GAME_PROMPT_EXECUTION_RULE}")
}

fn remove_game_prompt_grounding(prompt: &str) -> String {
    let Some(start) = prompt.find(GAME_PROMPT_GROUNDING) else {
        return prompt.to_owned();
    };
    let Some(rule) = prompt[start..].find(GAME_PROMPT_EXECUTION_RULE) else {
        return prompt.to_owned();
    };
    let end = start + rule + GAME_PROMPT_EXECUTION_RULE.len();
    format!(
        "{}{}",
        prompt[..start].trim_end(),
        prompt[end..].trim_start()
    )
}

/// Generate a stable opaque identifier compatible with existing UUID persistence fields.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate an RFC3339 UTC timestamp compatible with the prior Python SQLite rows.
pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// Convert SQLite's dynamically typed values into JSON without narrowing existing persisted data.
pub fn row_to_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    let statement = row.as_ref();
    let mut object = Map::with_capacity(statement.column_count());
    for index in 0..statement.column_count() {
        let name = statement.column_name(index)?.to_owned();
        let value = match row.get_ref(index)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(value) => Value::from(value),
            ValueRef::Real(value) => Value::from(value),
            ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
            ValueRef::Blob(value) => Value::String(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                value,
            )),
        };
        object.insert(name, value);
    }
    Ok(Value::Object(object))
}

/// Read a serialized JSON field safely, matching the old repository's defensive fallback behavior.
pub fn json_field(object: &mut Map<String, Value>, name: &str, default: Value) -> Value {
    object
        .remove(name)
        .and_then(|value| value.as_str().map(str::to_owned))
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or(default)
}

/// Store JSON in one SQLite text column using UTF-8, including Chinese prompt content.
pub fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

/// Read a boolean persisted through SQLite, which may return an integer for legacy flag columns.
pub fn bool_value(value: &Value) -> bool {
    value
        .as_bool()
        .or_else(|| value.as_i64().map(|value| value != 0))
        .unwrap_or(false)
}

/// Read a JSON request object while producing a concise error for the frontend toast.
pub fn object(value: Value) -> Result<Map<String, Value>, crate::error::AppError> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| crate::error::AppError::BadRequest("请求体必须是 JSON 对象".to_owned()))
}

/// Read a JSON string setting while accepting omitted optional form fields.
pub fn string(value: &Map<String, Value>, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::ground_game_video_prompt;

    #[test]
    fn game_prompt_grounding_tracks_the_latest_original_text() {
        let original = ground_game_video_prompt("镜头：主角停下观察。", "主角在旧车站发现信件。");
        let updated = ground_game_video_prompt(&original, "主角在旧车站烧毁信件后离开。");

        assert!(updated.contains("烧毁信件后离开"));
        assert!(!updated.contains("发现信件。"));
        assert_eq!(updated.matches("原始剧情依据（必须画面化）：").count(), 1);
    }
}
