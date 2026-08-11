//! Service flows for refining and deleting durable interactive-game node video versions.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    providers::ProviderClient,
    service::DesktopService,
};

impl DesktopService {
    /// The node-history check action validates and persists the version the editor and runtime should play.
    pub fn select_game_node_video_for_use(
        &self,
        game_id: &str,
        node_id: &str,
        video_id: &str,
    ) -> AppResult<Value> {
        self.repository
            .select_game_node_video_for_use(game_id, node_id, video_id)
    }

    /// The node history's “微调” action validates creator feedback, then queues a dependent durable video task.
    pub fn enqueue_game_node_video_refinement(
        &self,
        game_id: &str,
        node_id: &str,
        source_video_id: &str,
        refinement_prompt: &str,
    ) -> AppResult<Value> {
        let refinement_prompt = refinement_prompt.trim();
        if refinement_prompt.is_empty() {
            return Err(AppError::BadRequest("请填写微调提示词".to_owned()));
        }
        if refinement_prompt.chars().count() > 4_000 {
            return Err(AppError::BadRequest(
                "微调提示词不能超过 4000 个字".to_owned(),
            ));
        }
        self.repository.enqueue_game_node_video_refinement(
            game_id,
            node_id,
            source_video_id,
            refinement_prompt,
        )
    }

    /// The node history's delete action removes local media and stops a still-pending provider job when applicable.
    pub fn delete_game_node_video(
        &self,
        game_id: &str,
        node_id: &str,
        video_id: &str,
    ) -> AppResult<Value> {
        let mut result = self
            .repository
            .delete_game_node_video(game_id, node_id, video_id)?;
        let deleted = self.media.delete_url(result["url"].as_str())?;
        result["media_deleted"] = json!(if deleted { 1 } else { 0 });
        if let Some(provider_task_id) = result["provider_task_id"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            if let Err(error) = ProviderClient::new(self.repository.clone(), self.media.clone())?
                .cancel_video(provider_task_id)
            {
                result["provider_cancel_errors"] = json!([error.to_string()]);
            }
        }
        Ok(result)
    }
}
