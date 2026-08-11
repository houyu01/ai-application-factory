//! Typed validation shared by interactive-game creation before persistence begins.

use serde_json::{Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::string,
};

/// Seedance 2.0 accepts interactive-game video tasks from 4 through 15 seconds.
///
/// Game graph planning, editor validation, durable snapshots, and the provider worker all use
/// this range so a node cannot be saved with a duration that Ark will reject at submission.
pub(crate) const GAME_VIDEO_DURATION_RANGE: std::ops::RangeInclusive<i64> = 4..=15;

/// Read a Pydantic-compatible bounded integer without silently clamping a submitted setting.
pub(crate) fn game_integer(
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

/// Validate enum and cross-field constraints imposed by the original game creation request model.
pub(crate) fn validate_game_form(values: &Map<String, Value>) -> AppResult<()> {
    let name = string(values, "name", "");
    if !(1..=120).contains(&name.chars().count()) {
        return Err(AppError::BadRequest(
            "游戏名称长度需在 1 到 120 个字之间".to_owned(),
        ));
    }
    let platform = string(values, "platform", "Steam游戏");
    if !["微信小游戏", "手机原生游戏", "Steam游戏"].contains(&platform.as_str()) {
        return Err(AppError::BadRequest("不支持的游戏发布平台".to_owned()));
    }
    let style = string(values, "style", "真人风格");
    if !["真人风格", "2D动漫", "3D动漫"].contains(&style.as_str()) {
        return Err(AppError::BadRequest("不支持的游戏视觉风格".to_owned()));
    }
    let branch_min = game_integer(values, "branch_min", 2, 2, 4)?;
    let branch_max = game_integer(values, "branch_max", 4, 2, 4)?;
    let duration_min = game_integer(
        values,
        "node_duration_min",
        5,
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    )?;
    let duration_max = game_integer(
        values,
        "node_duration_max",
        15,
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    )?;
    let expansion_min = game_integer(values, "expanded_script_min_chars", 5_000, 1, 1_000_000)?;
    let expansion_max = game_integer(values, "expanded_script_max_chars", 10_000, 1, 1_000_000)?;
    game_integer(values, "node_script_max_chars", 400, 1, 1_000_000)?;
    if branch_min > branch_max {
        return Err(AppError::BadRequest(
            "branch_min must be less than or equal to branch_max".to_owned(),
        ));
    }
    if duration_min > duration_max {
        return Err(AppError::BadRequest(
            "node_duration_min must be less than or equal to node_duration_max".to_owned(),
        ));
    }
    if expansion_min > expansion_max {
        return Err(AppError::BadRequest(
            "expanded_script_min_chars must be less than or equal to expanded_script_max_chars"
                .to_owned(),
        ));
    }
    let resolution = string(values, "resolution", "720p");
    if !["480p", "720p"].contains(&resolution.as_str()) {
        return Err(AppError::BadRequest("不支持的游戏节点分辨率".to_owned()));
    }
    if values
        .get("enable_web_search")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(AppError::BadRequest(
            "enable_web_search 必须是布尔值".to_owned(),
        ));
    }
    Ok(())
}
