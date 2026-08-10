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
