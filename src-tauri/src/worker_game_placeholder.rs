//! Provider execution for restart-safe interactive-game placeholder composite tasks.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::SUCCEEDED,
};

use super::DurableWorker;

impl DurableWorker {
    /// Render one node's persisted scene-and-character layout without modifying its manually selected video references.
    pub(super) fn game_placeholder_image(
        &self,
        task_id: &str,
        game_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        let snapshot = &task["input_snapshot"];
        let asset_id = snapshot["asset_id"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("占位图任务缺少素材 ID".to_owned()))?;
        let node_id = snapshot["node_id"]
            .as_str()
            .or_else(|| task["resource_id"].as_str())
            .ok_or_else(|| AppError::BadRequest("占位图任务缺少节点 ID".to_owned()))?;
        self.repository
            .set_game_asset_image_status(game_id, asset_id, None, "生成中")?;
        self.repository
            .update_game_task_progress(task_id, 12, "正在生成节点占位图")?;
        let game = self.repository.get_game(game_id)?;
        let placeholder = self.repository.get_game_asset(game_id, asset_id)?;
        if placeholder["type"].as_str() != Some("placeholder")
            || placeholder["metadata"]["render_mode"].as_str() != Some("generated_composite")
        {
            return Err(AppError::BadRequest(
                "占位图任务引用的素材不可用".to_owned(),
            ));
        }
        let references = placeholder_references(self, &game, &placeholder)?;
        let ratio = if game["platform"].as_str() == Some("Steam游戏") {
            "16:9"
        } else {
            "9:16"
        };
        let url = self.providers.image(
            placeholder["prompt"].as_str().unwrap_or_default(),
            ratio,
            &references,
            game["multimodal_model"].as_str(),
        )?;
        self.repository
            .finish_game_asset_image(game_id, asset_id, task_id, &url)?;
        self.repository.apply_game_placeholder_to_node(
            game_id,
            node_id,
            asset_id,
            &placeholder["metadata"],
        )?;
        self.repository.finish_game_task(
            task_id,
            SUCCEEDED,
            Some(json!({
                "asset_id":asset_id,
                "node_id":node_id,
                "image_url":url,
                "scene_asset_id":placeholder["metadata"]["scene_asset_id"],
                "placements":placeholder["metadata"]["placements"],
                "reference_asset_ids":placeholder["metadata"]["reference_asset_ids"],
                "render_mode":"generated_composite",
            })),
            None,
        )?;
        Ok(())
    }
}

fn placeholder_references(
    worker: &DurableWorker,
    game: &Value,
    placeholder: &Value,
) -> AppResult<Vec<String>> {
    placeholder["metadata"]["reference_asset_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|id| {
            game["assets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|asset| asset["id"].as_str() == Some(id))
                .and_then(|asset| asset["image_url"].as_str())
                .and_then(|url| worker.media.provider_reference_url(url))
                .ok_or_else(|| {
                    AppError::BadRequest("占位图引用的场景、角色或道具图片不可用".to_owned())
                })
        })
        .collect()
}
