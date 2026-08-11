//! Durable interactive-game cover assets, uploaded references, and image tasks owned by SQLite.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::{json_text, new_id, now, GENERATING},
};

use super::Repository;

impl Repository {
    /// Persist a user-uploaded cover-only reference so a later game cover task can safely recover its image input.
    pub fn create_game_cover_reference(
        &self,
        game_id: &str,
        name: &str,
        image_url: &str,
    ) -> AppResult<Value> {
        let id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| {
            ensure_game(connection, game_id)?;
            connection.execute(
                "INSERT INTO game_assets (id,game_id,type,name,prompt,image_url,image_history_json,status,created_at,updated_at) VALUES (?1,?2,'cover_reference',?3,'用户上传的封面参考图',?4,?5,'已配置',?6,?6)",
                params![id, game_id, name, image_url, json_text(&json!([{"id":new_id(),"url":image_url,"generated_at":timestamp,"source_type":"uploaded"}])), timestamp],
            )?;
            Ok(())
        })?;
        self.get_game_asset(game_id, &id)
    }

    /// Create the cover asset and its queued image task together, retaining the selected references and requested output plan.
    pub fn enqueue_game_cover(
        &self,
        game_id: &str,
        name: &str,
        prompt: &str,
        metadata: Value,
    ) -> AppResult<Value> {
        let cover_id = new_id();
        let task_id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| {
            ensure_game(connection, game_id)?;
            let transaction = connection.unchecked_transaction()?;
            transaction.execute(
                "INSERT INTO game_assets (id,game_id,type,name,prompt,metadata_json,status,created_at,updated_at) VALUES (?1,?2,'cover',?3,?4,?5,?6,?7,?7)",
                params![cover_id, game_id, name, prompt, json_text(&metadata), GENERATING, timestamp],
            )?;
            transaction.execute(
                "INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_cover_image',?3,?4,?5,0,'等待封面图片生成',?6,?6)",
                params![task_id, game_id, cover_id, GENERATING, json_text(&json!({"game_id":game_id,"cover_asset_id":cover_id,"ratio":metadata["ratio"],"count":metadata["count"],"reference_asset_ids":metadata["reference_asset_ids"]})), timestamp],
            )?;
            transaction.commit()?;
            Ok(())
        })?;
        Ok(json!({
            "cover": self.get_game_asset(game_id, &cover_id)?,
            "task": self.get_game_task(&task_id)?,
        }))
    }
}

fn ensure_game(connection: &rusqlite::Connection, game_id: &str) -> AppResult<()> {
    connection
        .query_row(
            "SELECT 1 FROM interactive_games WHERE id=?1",
            [game_id],
            |_| Ok(()),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))
}
