//! Interactive-game node-video submission rules shared by the workbench route and durable worker.

use serde_json::Value;

use crate::{
    error::{AppError, AppResult},
    repository::game_validation::GAME_VIDEO_DURATION_RANGE,
    service::DesktopService,
    value::{FAILED, GENERATING},
};

impl DesktopService {
    /// Validate a node's saved prompt and image references before creating the same durable video task used by short dramas.
    pub fn enqueue_game_node_video(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        let game = self.repository.get_game(game_id)?;
        let node = self.repository.get_game_node(game_id, node_id)?;
        self.validate_game_node_video_preflight(&game, &node)?;
        self.repository.enqueue_game_node_video(game_id, node_id)
    }

    fn validate_game_node_video_preflight(&self, game: &Value, node: &Value) -> AppResult<()> {
        let mut issues = Vec::new();
        if node["prompt"]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        {
            issues.push("请先填写并保存视频提示词".to_owned());
        }
        let duration = node["duration_seconds"].as_i64().unwrap_or_default();
        if !GAME_VIDEO_DURATION_RANGE.contains(&duration) {
            issues.push("节点视频时长必须在 4 到 15 秒之间".to_owned());
        }
        for id in game_node_reference_ids(node) {
            let Some(asset) = game["assets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|asset| asset["id"] == id)
            else {
                issues.push(format!("{id}（素材不存在）"));
                continue;
            };
            let name = asset["name"].as_str().unwrap_or(&id);
            if asset["status"].as_str() == Some(GENERATING) {
                issues.push(format!("{name}（图片仍在生成）"));
            } else if asset["status"].as_str() == Some(FAILED) {
                issues.push(format!("{name}（图片生成失败）"));
            } else if asset["image_url"]
                .as_str()
                .is_none_or(|url| self.media.provider_reference_url(url).is_none())
            {
                issues.push(format!("{name}（图片未生成、未上传或无法发送给视频模型）"));
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(AppError::BadRequest(issues.join("；")))
        }
    }
}

fn game_node_reference_ids(node: &Value) -> Vec<String> {
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
