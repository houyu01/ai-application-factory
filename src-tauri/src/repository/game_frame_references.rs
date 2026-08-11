//! Validation for game-node boundary frames selected from related branch videos.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::SUCCEEDED,
};

use super::game_materials::require_asset;

/// Keep a node's boundary references compatible with legacy assets and uploads while ensuring video frames belong to an upstream or downstream branch.
pub(super) fn frame_references(
    value: &Value,
    connection: &rusqlite::Connection,
    game_id: &str,
    node_id: &str,
) -> AppResult<Value> {
    let frames = value
        .as_object()
        .ok_or_else(|| AppError::BadRequest("first_last_frames 必须是对象".to_owned()))?;
    let mut result = Map::new();
    for side in ["first", "last"] {
        let Some(frame) = frames.get(side).and_then(Value::as_object) else {
            continue;
        };
        if let Some(asset_id) = frame
            .get("asset_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            require_asset(connection, game_id, asset_id, None)?;
            result.insert(side.to_owned(), json!({"asset_id": asset_id}));
            continue;
        }
        let url = frame
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| url.starts_with("data:image/"))
            .ok_or_else(|| {
                AppError::BadRequest(format!("输入{side}帧必须选择素材图、上传图片或关联视频帧"))
            })?;
        if frame["source"].as_str() == Some("upload") {
            result.insert(side.to_owned(), json!({"url": url, "source": "upload"}));
            continue;
        }
        let source_node_id = required_frame_field(frame, "node_id", side)?;
        let video_id = required_frame_field(frame, "video_id", side)?;
        let position = frame
            .get("position")
            .and_then(Value::as_str)
            .filter(|position| matches!(*position, "first" | "last"))
            .ok_or_else(|| AppError::BadRequest(format!("输入{side}帧缺少有效视频位置")))?;
        verify_related_video(connection, game_id, node_id, source_node_id, video_id)?;
        result.insert(
            side.to_owned(),
            json!({
                "url": url,
                "source": "related_video",
                "node_id": source_node_id,
                "video_id": video_id,
                "position": position,
            }),
        );
    }
    Ok(Value::Object(result))
}

fn required_frame_field<'a>(
    frame: &'a Map<String, Value>,
    key: &str,
    side: &str,
) -> AppResult<&'a str> {
    frame
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest(format!("输入{side}帧缺少{key}")))
}

fn verify_related_video(
    connection: &rusqlite::Connection,
    game_id: &str,
    node_id: &str,
    source_node_id: &str,
    video_id: &str,
) -> AppResult<()> {
    let linked = connection
        .query_row(
            "WITH RECURSIVE upstream(id) AS (
                SELECT source_node_id FROM game_edges WHERE game_id=?1 AND target_node_id=?2
                UNION
                SELECT edge.source_node_id FROM game_edges edge JOIN upstream ON edge.target_node_id=upstream.id WHERE edge.game_id=?1
            ), downstream(id) AS (
                SELECT target_node_id FROM game_edges WHERE game_id=?1 AND source_node_id=?2
                UNION
                SELECT edge.target_node_id FROM game_edges edge JOIN downstream ON edge.source_node_id=downstream.id WHERE edge.game_id=?1
            )
            SELECT 1 FROM upstream WHERE id=?3 UNION SELECT 1 FROM downstream WHERE id=?3 LIMIT 1",
            params![game_id, node_id, source_node_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !linked {
        return Err(AppError::BadRequest(
            "只能选择当前节点的上游或下游视频帧".to_owned(),
        ));
    }
    let (history_json, current_video_url): (String, Option<String>) = connection
        .query_row(
            "SELECT video_history_json,video_url FROM game_nodes WHERE id=?1 AND game_id=?2",
            params![source_node_id, game_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::BadRequest("关联视频节点不存在".to_owned()))?;
    let available = serde_json::from_str::<Vec<Value>>(&history_json)
        .unwrap_or_default()
        .iter()
        .any(|video| {
            video["id"].as_str() == Some(video_id)
                && video["status"].as_str() == Some(SUCCEEDED)
                && video["url"].as_str().is_some_and(|url| !url.is_empty())
        });
    if available || (video_id == "current" && current_video_url.is_some_and(|url| !url.is_empty()))
    {
        Ok(())
    } else {
        Err(AppError::BadRequest("所选关联视频版本不可用".to_owned()))
    }
}
