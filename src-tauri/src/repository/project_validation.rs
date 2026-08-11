//! Typed validation for short-drama project settings before they are persisted.

use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

pub(super) fn create_integer(
    values: &Map<String, Value>,
    key: &str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> AppResult<i64> {
    let value = values.get(key).map_or(Ok(default), |value| {
        value
            .as_i64()
            .ok_or_else(|| AppError::BadRequest(format!("{key} 必须是整数")))
    })?;
    if !(minimum..=maximum).contains(&value) {
        return Err(AppError::BadRequest(format!(
            "{key} 必须在 {minimum} 到 {maximum} 之间"
        )));
    }
    Ok(value)
}

pub(super) fn optional_boolean(values: &Map<String, Value>, key: &str) -> AppResult<Option<bool>> {
    values
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| AppError::BadRequest(format!("{key} 必须是布尔值")))
        })
        .transpose()
}
