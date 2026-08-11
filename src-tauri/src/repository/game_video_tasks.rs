//! Durable cancellation persistence for independently generated interactive-game node videos.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_text, new_id, now, row_to_json, CANCELLED, GENERATING},
};

use super::{game_video_history::game_video_snapshot, Repository};

impl Repository {
    /// Find active game tasks of one type so services can coordinate batches and provider cancellation without issuing SQL themselves.
    pub(crate) fn active_game_tasks(
        &self,
        game_id: &str,
        kind: &str,
        resource_id: Option<&str>,
    ) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let rows = if let Some(resource_id) = resource_id {
                let mut statement = connection.prepare(
                    "SELECT * FROM game_tasks WHERE game_id=?1 AND type=?2 AND resource_id=?3 AND status=?4 ORDER BY created_at",
                )?;
                let rows = statement
                    .query_map(params![game_id, kind, resource_id, GENERATING], row_to_json)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            } else {
                let mut statement = connection.prepare(
                    "SELECT * FROM game_tasks WHERE game_id=?1 AND type=?2 AND status=?3 ORDER BY created_at",
                )?;
                let rows = statement
                    .query_map(params![game_id, kind, GENERATING], row_to_json)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };
            Ok(rows.into_iter().map(mapping::game_task).collect())
        })
    }

    /// Create an idempotent game-wide coordinator task before serial video work begins, preserving restart recovery in SQLite.
    pub(crate) fn create_active_game_task(
        &self,
        game_id: &str,
        kind: &str,
        resource_id: &str,
        snapshot: Value,
        stage: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row("SELECT 1 FROM interactive_games WHERE id=?1", [game_id], |_| Ok(()))
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))?;
            let existing = connection
                .query_row(
                    "SELECT * FROM game_tasks WHERE game_id=?1 AND type=?2 AND resource_id=?3 AND status=?4 ORDER BY created_at DESC LIMIT 1",
                    params![game_id, kind, resource_id, GENERATING],
                    row_to_json,
                )
                .optional()?;
            if let Some(task) = existing {
                return Ok(mapping::game_task(task));
            }
            let id = new_id();
            let timestamp = now();
            connection.execute(
                "INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,?3,?4,?5,?6,0,?7,?8,?8)",
                params![id, game_id, kind, resource_id, GENERATING, json_text(&snapshot), stage, timestamp],
            )?;
            self.get_game_task(&id)
        })
    }

    /// Queue a node video with an immutable previous-node tail frame rather than changing the creator's saved frame settings.
    pub(crate) fn enqueue_game_node_video_with_serial_frame(
        &self,
        game_id: &str,
        node_id: &str,
        serial_first_frame: Option<&str>,
        parent_task_id: Option<&str>,
    ) -> AppResult<Value> {
        let node = self.get_game_node(game_id, node_id)?;
        let game = self.get_game(game_id)?;
        let mut snapshot = game_video_snapshot(game_id, node_id, &game, &node);
        if let Some(frame) = serial_first_frame {
            snapshot["serial_first_frame"] = json!(frame);
        }
        if let Some(parent) = parent_task_id {
            snapshot["parent_task_id"] = json!(parent);
        }
        self.enqueue_game_node_video_snapshot(game_id, node_id, snapshot)
    }

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
            cancel_game_node_task(connection, game_id, node_id, &task)
        })
    }

    /// Cancel every active node-video task in one game and retain cancellation entries in each node's immutable video history.
    pub(crate) fn cancel_all_game_node_video_tasks(&self, game_id: &str) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT * FROM game_tasks WHERE game_id=?1 AND type='node_video_generation' AND status=?2 ORDER BY created_at",
            )?;
            let tasks = statement
                .query_map(params![game_id, GENERATING], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?;
            tasks
                .into_iter()
                .map(|task| {
                    let node_id = task["resource_id"].as_str().unwrap_or_default();
                    cancel_game_node_task(connection, game_id, node_id, &task)
                })
                .collect()
        })
    }

    /// Mark a coordinator cancelled after its child node tasks have been durably stopped.
    pub(crate) fn cancel_game_task(&self, task_id: &str, stage: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE game_tasks SET status=?1,stage=?2,completed_at=?3,poll_lease_until=NULL,poll_lease_token=NULL,next_poll_at=NULL WHERE id=?4 AND status=?5",
                params![CANCELLED, stage, now(), task_id, GENERATING],
            )?;
            self.get_game_task(task_id)
        })
    }
}

fn cancel_game_node_task(
    connection: &Connection,
    game_id: &str,
    node_id: &str,
    task: &Value,
) -> AppResult<Value> {
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
        "UPDATE game_tasks SET status=?1,result_json=NULL,error_message=NULL,progress=100,stage='已取消',completed_at=?2,poll_lease_until=NULL,poll_lease_token=NULL,next_poll_at=NULL WHERE id=?3 AND status=?4",
        params![CANCELLED, timestamp, task_id, GENERATING],
    )?;
    connection.execute(
        "UPDATE game_nodes SET video_history_json=?1,status=?2,updated_at=?3 WHERE id=?4 AND game_id=?5",
        params![json_text(&Value::Array(history)), CANCELLED, timestamp, node_id, game_id],
    )?;
    connection
        .query_row(
            "SELECT * FROM game_tasks WHERE id=?1",
            [task_id],
            row_to_json,
        )
        .map(mapping::game_task)
        .map_err(Into::into)
}
