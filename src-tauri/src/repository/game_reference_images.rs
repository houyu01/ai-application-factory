//! Durable image-task enqueueing for the references selected by one game video node.

use serde_json::Value;

use crate::{
    error::{AppError, AppResult},
    value::GENERATING,
};

use super::Repository;

const GENERATABLE_REFERENCE_TYPES: [&str; 3] = ["character", "scene", "prop"];

impl Repository {
    /// The game-node inspector's one-click reference-image action calls this after saving its selected materials.
    /// It owns the boundary between node references and independently durable material-image tasks, so retries
    /// resume the existing per-material task instead of enqueueing a duplicate provider request.
    pub fn enqueue_game_node_reference_images(
        &self,
        game_id: &str,
        node_id: &str,
    ) -> AppResult<Vec<Value>> {
        let node = self.get_game_node(game_id, node_id)?;
        let mut tasks = Vec::new();
        for asset_id in node_reference_asset_ids(&node) {
            let asset = self.get_game_asset(game_id, &asset_id)?;
            let has_image = asset["image_url"]
                .as_str()
                .is_some_and(|url| !url.trim().is_empty());
            if has_image
                || !GENERATABLE_REFERENCE_TYPES
                    .contains(&asset["type"].as_str().unwrap_or_default())
            {
                continue;
            }
            let task = self.enqueue_game_asset_image(game_id, &asset_id)?;
            if task["status"].as_str() == Some(GENERATING)
                && !tasks
                    .iter()
                    .any(|current: &Value| current["id"] == task["id"])
            {
                tasks.push(task);
            }
        }
        if tasks.is_empty() {
            return Err(AppError::BadRequest(
                "当前已选参考图均已就绪，或没有可一键生成的素材".to_owned(),
            ));
        }
        Ok(tasks)
    }
}

fn node_reference_asset_ids(node: &Value) -> Vec<String> {
    let mut ids = node["prompt_rich"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "reference")
        .filter_map(|item| item["asset_id"].as_str())
        .chain(
            node["reference_asset_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.dedup();
    ids
}
