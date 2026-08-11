//! Fresh interactive-game generation runs that replace derived editor state.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::{json_text, new_id, now, CANCELLED, GENERATING},
};

use super::Repository;

impl Repository {
    /// Start a new game screenplay expansion from the editor's revised original text.
    ///
    /// The interactive-game screenplay dialog calls this after the creator confirms that a
    /// changed premise should replace the current run. This repository boundary owns the one
    /// SQLite transaction that cancels stale work, removes derived graph/material/session rows,
    /// and creates the durable bootstrap task, while retaining the project's configuration.
    pub fn regenerate_game_screenplay(
        &self,
        game_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let replacement = values
            .get("script")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if replacement
            .as_deref()
            .is_some_and(|script| script.chars().count() < 20)
        {
            return Err(AppError::BadRequest("剧本文本不少于 20 个字".to_owned()));
        }
        let task_id = self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let saved_script: String = transaction
                .query_row(
                    "SELECT script FROM interactive_games WHERE id=?1",
                    [game_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))?;
            let script = replacement.as_deref().unwrap_or(&saved_script);
            if script.chars().count() < 20 {
                return Err(AppError::BadRequest("剧本文本不少于 20 个字".to_owned()));
            }
            let timestamp = now();
            transaction.execute(
                "UPDATE game_tasks SET status=?1,stage='已因从头重新生成取消',completed_at=?2,poll_lease_until=NULL,poll_lease_token=NULL,next_poll_at=NULL WHERE game_id=?3 AND status=?4",
                params![CANCELLED, timestamp, game_id, GENERATING],
            )?;
            transaction.execute("DELETE FROM game_choice_events WHERE game_id=?1", [game_id])?;
            transaction.execute("DELETE FROM game_sessions WHERE game_id=?1", [game_id])?;
            transaction.execute("DELETE FROM game_edges WHERE game_id=?1", [game_id])?;
            transaction.execute("DELETE FROM game_nodes WHERE game_id=?1", [game_id])?;
            transaction.execute("DELETE FROM game_assets WHERE game_id=?1", [game_id])?;
            let id = new_id();
            transaction.execute(
                "INSERT INTO game_tasks (id,game_id,type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_script_expansion',?3,?4,0,'等待重新生成',?5,?5)",
                params![id, game_id, GENERATING, json_text(&json!({"game_id":game_id})), timestamp],
            )?;
            transaction.execute(
                "UPDATE interactive_games SET script=?1,expanded_script='',assets_json='[]',nodes_json='[]',edges_json='[]',historical_videos_json='[]',status=?2,updated_at=?3 WHERE id=?4",
                params![script, GENERATING, timestamp, game_id],
            )?;
            transaction.commit()?;
            Ok(id)
        })?;
        self.get_game_task(&task_id)
    }
}
