//! Durable cancellation persistence for independently generated interactive-game node videos.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_text, now, row_to_json, CANCELLED, GENERATING},
};

use super::Repository;

impl Repository {
    /// Cancel the selected node's active video task and preserve any last successful video URL for playback.
    pub fn cancel_game_node_video_task(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let task = connection
                .query_row(
                    "SELECT * FROM game_tasks WHERE game_id=?1 AND type='node_video_generation' AND resource_id=?2 AND status=?3 ORDER BY created_at DESC LIMIT 1",
                    params![game_id, node_id, GENERATING],
                    row_to_json,
                )
                .optional()?
                .ok_or_else(|| AppError::BadRequest("当前节点没有正在生成的视频任务".to_owned()))?;
            let task_id = task["id"].as_str().unwrap_or_default();
            let history_json: String = connection.query_row(
                "SELECT video_history_json FROM game_nodes WHERE id=?1 AND game_id=?2",
                params![node_id, game_id],
                |row| row.get(0),
            )?;
            let mut history = serde_json::from_str::<Vec<Value>>(&history_json).unwrap_or_default();
            history.push(json!({"id":task_id,"url":null,"generated_at":now(),"task_id":task_id,"status":CANCELLED,"error_message":"节点视频生成已取消"}));
            let timestamp = now();
            connection.execute(
                "UPDATE game_tasks SET status=?1,result_json=NULL,error_message=NULL,progress=100,stage='已取消',completed_at=?2,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?3 AND status=?4",
                params![CANCELLED, timestamp, task_id, GENERATING],
            )?;
            connection.execute(
                "UPDATE game_nodes SET video_history_json=?1,status=?2,updated_at=?3 WHERE id=?4 AND game_id=?5",
                params![json_text(&Value::Array(history)), CANCELLED, timestamp, node_id, game_id],
            )?;
            connection
                .query_row("SELECT * FROM game_tasks WHERE id=?1", [task_id], row_to_json)
                .map(mapping::game_task)
                .map_err(Into::into)
        })
    }
}
