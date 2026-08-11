//! Durable interactive-game node-video history records and refinement task snapshots.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{
        ground_game_video_prompt, json_text, new_id, now, row_to_json, CANCELLED, GENERATING,
        NOT_GENERATED, SUCCEEDED,
    },
};

use super::Repository;

impl Repository {
    /// Queue a node video after freezing its editable prompt and image references for restart-safe generation.
    pub fn enqueue_game_node_video(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        let node = self.get_game_node(game_id, node_id)?;
        let game = self.get_game(game_id)?;
        self.enqueue_game_node_video_snapshot(
            game_id,
            node_id,
            game_video_snapshot(game_id, node_id, &game, &node),
        )
    }

    /// Create a dependent node-video task from one playable history version and its creator feedback.
    pub fn enqueue_game_node_video_refinement(
        &self,
        game_id: &str,
        node_id: &str,
        source_video_id: &str,
        refinement_prompt: &str,
    ) -> AppResult<Value> {
        let source = self.game_node_video_history_record(game_id, node_id, source_video_id)?;
        let source_video_url = source["url"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::BadRequest("只能微调已生成完成的视频".to_owned()))?;
        let node = self.get_game_node(game_id, node_id)?;
        let game = self.get_game(game_id)?;
        let original_prompt = source["prompt"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| node["prompt"].as_str().unwrap_or_default());
        let original_prompt = ground_game_video_prompt(
            original_prompt,
            node["original_text"].as_str().unwrap_or_default(),
        );
        if original_prompt.trim().is_empty() {
            return Err(AppError::BadRequest(
                "所选视频缺少原始提示词，无法微调".to_owned(),
            ));
        }
        let mut snapshot = game_video_snapshot(game_id, node_id, &game, &node);
        snapshot["prompt"] = json!(original_prompt);
        if source["reference_images"].is_array() {
            snapshot["reference_images"] = source["reference_images"].clone();
        }
        snapshot["refinement"] = json!({
            "source_video_id": source_video_id,
            "source_video_url": source_video_url,
            "prompt": refinement_prompt,
        });
        self.enqueue_game_node_video_snapshot(game_id, node_id, snapshot)
    }

    /// Return one successful history version before it is refined or deleted from its node.
    pub fn game_node_video_history_record(
        &self,
        game_id: &str,
        node_id: &str,
        video_id: &str,
    ) -> AppResult<Value> {
        let node = self.get_game_node(game_id, node_id)?;
        node["video_history"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|item| item["id"].as_str() == Some(video_id))
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Game video history not found: {video_id}")))
    }

    /// Store the completed history version that the node editor and playable game should keep using.
    ///
    /// The node-history check action calls this flow after a creator compares versions. This repository
    /// boundary validates the selected record, then atomically updates the runtime-facing URL so a later
    /// generation cannot replace the creator's chosen version.
    pub fn select_game_node_video_for_use(
        &self,
        game_id: &str,
        node_id: &str,
        video_id: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let history_json: String = transaction
                .query_row(
                    "SELECT video_history_json FROM game_nodes WHERE id=?1 AND game_id=?2",
                    params![node_id, game_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Game node not found: {node_id}")))?;
            let history = serde_json::from_str::<Vec<Value>>(&history_json).unwrap_or_default();
            let selected = playable_history_video(&history, video_id).ok_or_else(|| {
                AppError::BadRequest("只能设为已生成完成的视频版本".to_owned())
            })?;
            let url = selected["url"]
                .as_str()
                .expect("playable history video has a URL");
            transaction.execute(
                "UPDATE game_nodes SET selected_video_id=?1,video_url=?2,status=?3,updated_at=?4 WHERE id=?5 AND game_id=?6",
                params![video_id, url, SUCCEEDED, now(), node_id, game_id],
            )?;
            transaction.commit()?;
            Ok(())
        })?;
        self.get_game_node(game_id, node_id)
    }

    /// Remove one node video version, restoring the newest remaining playable version for the runtime player.
    pub fn delete_game_node_video(
        &self,
        game_id: &str,
        node_id: &str,
        video_id: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let (raw, selected_video_id): (String, Option<String>) = connection
                .query_row(
                    "SELECT video_history_json,selected_video_id FROM game_nodes WHERE id=?1 AND game_id=?2",
                    params![node_id, game_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Game node not found: {node_id}")))?;
            let history = serde_json::from_str::<Vec<Value>>(&raw).unwrap_or_default();
            let removed = history
                .iter()
                .find(|item| item["id"].as_str() == Some(video_id))
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("Game video history not found: {video_id}")))?;
            let kept = history
                .into_iter()
                .filter(|item| item["id"].as_str() != Some(video_id))
                .collect::<Vec<_>>();
            let selected_video_id = retained_selected_video_id(&kept, selected_video_id);
            let latest_url = current_history_video(&kept, selected_video_id.as_deref())
                .and_then(|item| item["url"].as_str())
                .map(str::to_owned);
            let status = if latest_url.is_some() { SUCCEEDED } else { NOT_GENERATED };
            let provider_task_id = connection
                .query_row(
                    "SELECT provider_task_id FROM game_tasks WHERE id=?1 AND game_id=?2",
                    params![video_id, game_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            connection.execute(
                "UPDATE game_tasks SET status=?1,stage='视频历史已删除',completed_at=?2,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?3 AND game_id=?4 AND status=?5",
                params![CANCELLED, now(), video_id, game_id, GENERATING],
            )?;
            connection.execute(
                "UPDATE game_nodes SET selected_video_id=?1,video_url=?2,video_history_json=?3,status=?4,updated_at=?5 WHERE id=?6 AND game_id=?7",
                params![selected_video_id, latest_url, json_text(&Value::Array(kept)), status, now(), node_id, game_id],
            )?;
            Ok(json!({
                "status": "deleted",
                "id": video_id,
                "url": removed["url"],
                "provider_task_id": provider_task_id,
            }))
        })
    }

    /// Append a terminal provider result while retaining frozen prompts and source-video provenance for later refinements.
    pub fn finish_game_node_video(
        &self,
        game_id: &str,
        node_id: &str,
        task_id: &str,
        url: Option<&str>,
        status: &str,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let task = connection
                .query_row("SELECT * FROM game_tasks WHERE id=?1", [task_id], row_to_json)
                .optional()?;
            if task
                .as_ref()
                .and_then(|item| item["status"].as_str())
                .is_some_and(|current| current != GENERATING && current != status)
            {
                return Ok(());
            }
            let (raw, selected_video_id): (String, Option<String>) = connection.query_row(
                "SELECT video_history_json,selected_video_id FROM game_nodes WHERE id=?1 AND game_id=?2",
                params![node_id, game_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let mut history = serde_json::from_str::<Vec<Value>>(&raw).unwrap_or_default();
            if !history.iter().any(|item| item["id"].as_str() == Some(task_id)) {
                let snapshot = task
                    .as_ref()
                    .map(|item| serde_json::from_str::<Value>(item["input_snapshot_json"].as_str().unwrap_or("null")).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null);
                history.push(json!({
                    "id": task_id,
                    "url": url,
                    "generated_at": now(),
                    "task_id": task_id,
                    "status": status,
                    "error_message": error,
                    "prompt": snapshot["prompt"],
                    "prompt_rich": snapshot["prompt_rich"],
                    "reference_images": snapshot["reference_images"],
                    "reference_asset_ids": snapshot["reference_asset_ids"],
                    "refinement_prompt": snapshot["refinement"]["prompt"],
                    "source_video_id": snapshot["refinement"]["source_video_id"],
                }));
            }
            let selected_video_id = retained_selected_video_id(&history, selected_video_id);
            let latest_url = current_history_video(&history, selected_video_id.as_deref())
                .and_then(|item| item["url"].as_str())
                .map(str::to_owned);
            connection.execute(
                "UPDATE game_nodes SET selected_video_id=?1,video_url=?2,video_history_json=?3,status=?4,updated_at=?5 WHERE id=?6 AND game_id=?7",
                params![selected_video_id, latest_url, json_text(&Value::Array(history)), status, now(), node_id, game_id],
            )?;
            Ok(())
        })
    }

    fn enqueue_game_node_video_snapshot(
        &self,
        game_id: &str,
        node_id: &str,
        snapshot: Value,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let timestamp = now();
            let existing = connection.query_row(
                "SELECT * FROM game_tasks WHERE game_id=?1 AND type='node_video_generation' AND resource_id=?2 AND status=?3 ORDER BY created_at DESC LIMIT 1",
                params![game_id, node_id, GENERATING],
                row_to_json,
            ).optional()?;
            if let Some(task) = existing {
                connection.execute("UPDATE game_nodes SET status=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![GENERATING, timestamp, node_id, game_id])?;
                let mut task = mapping::game_task(task);
                task.as_object_mut().expect("game task is an object").insert("_reused".to_owned(), json!(true));
                return Ok(task);
            }
            let id = new_id();
            connection.execute(
                "INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'node_video_generation',?3,?4,?5,0,'等待节点视频生成',?6,?6)",
                params![id, game_id, node_id, GENERATING, json_text(&snapshot), timestamp],
            )?;
            connection.execute("UPDATE game_nodes SET status=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![GENERATING, timestamp, node_id, game_id])?;
            let mut task = self.get_game_task(&id)?;
            task.as_object_mut().expect("game task is an object").insert("_reused".to_owned(), json!(false));
            Ok(task)
        })
    }
}

fn playable_history_video<'a>(history: &'a [Value], video_id: &str) -> Option<&'a Value> {
    history.iter().find(|item| {
        item["id"].as_str() == Some(video_id)
            && item["status"].as_str() == Some(SUCCEEDED)
            && item["url"].as_str().is_some_and(|url| !url.is_empty())
    })
}

fn retained_selected_video_id(
    history: &[Value],
    selected_video_id: Option<String>,
) -> Option<String> {
    selected_video_id.filter(|video_id| playable_history_video(history, video_id).is_some())
}

fn current_history_video<'a>(
    history: &'a [Value],
    selected_video_id: Option<&str>,
) -> Option<&'a Value> {
    selected_video_id
        .and_then(|video_id| playable_history_video(history, video_id))
        .or_else(|| {
            history.iter().rev().find(|item| {
                item["status"].as_str() == Some(SUCCEEDED)
                    && item["url"].as_str().is_some_and(|url| !url.is_empty())
            })
        })
}

fn game_video_snapshot(game_id: &str, node_id: &str, game: &Value, node: &Value) -> Value {
    let reference_ids = game_video_reference_ids(node);
    let references = game["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|asset| reference_ids.iter().any(|id| id == &asset["id"]))
        .map(|asset| json!({"id":asset["id"],"name":asset["name"],"type":asset["type"],"image_url":asset["image_url"],"prompt":asset["prompt"],"voice_id":asset["voice_id"]}))
        .collect::<Vec<_>>();
    json!({
        "game_id": game_id,
        "node_id": node_id,
        "prompt": node["prompt"],
        "prompt_rich": node["prompt_rich"],
        "reference_asset_ids": reference_ids,
        "reference_images": references,
        "first_last_frames": node["first_last_frames"],
        "placeholder_asset_id": node["placeholder_asset_id"],
        "placeholder_scene_asset_id": node["placeholder_scene_asset_id"],
        "placeholder_placements": node["placeholder_placements"],
    })
}

/// Match node-video task snapshots to the reference chips validated by the service boundary.
///
/// Rich prompt references take precedence because they are the creator-visible input passed to
/// the model. Older nodes without rich prompts retain their saved reference-material list.
fn game_video_reference_ids(node: &Value) -> Vec<String> {
    let mut ids = node["prompt_rich"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "reference")
        .filter_map(|item| item["asset_id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.extend(
            node["reference_asset_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    ids.retain(|id| !id.is_empty());
    ids.dedup();
    ids
}
