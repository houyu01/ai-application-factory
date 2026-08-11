//! Service boundary for cancelling a single interactive-game node's durable video task.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    service::{game_video_batch::SERIAL_GAME_VIDEO_BATCH, DesktopService},
    value::CANCELLED,
};

impl DesktopService {
    /// Stop a node-video task locally before asking its provider to cancel the remote job, so editor polling never restores a cancelled task.
    pub fn cancel_game_node_video(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        let mut task = self
            .repository
            .cancel_game_node_video_task(game_id, node_id)?;
        let provider_task_id = task["provider_task_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let task_id = task["id"].as_str().unwrap_or_default().to_owned();
        let object = task.as_object_mut().expect("game task is an object");
        object.insert("cancelled_count".to_owned(), json!(1));
        object.insert("game_id".to_owned(), json!(game_id));
        object.insert("node_id".to_owned(), json!(node_id));
        if !provider_task_id.is_empty() {
            if let Err(error) = self.worker_provider_cancel(&provider_task_id) {
                object.insert("provider_cancel_error".to_owned(), json!(error.to_string()));
            }
        }
        object.insert("cancelled_task_id".to_owned(), json!(task_id));
        Ok(task)
    }

    /// Stop every running game-node video and its serial coordinator locally before attempting provider cancellation for each remote job.
    pub fn cancel_all_game_node_videos(&self, game_id: &str) -> AppResult<Value> {
        self.repository.get_game(game_id)?;
        let tasks = self
            .repository
            .active_game_tasks(game_id, "node_video_generation", None)?;
        let batches =
            self.repository
                .active_game_tasks(game_id, SERIAL_GAME_VIDEO_BATCH, Some("all"))?;
        if tasks.is_empty() && batches.is_empty() {
            return Err(AppError::BadRequest(
                "当前没有正在生成的视频任务".to_owned(),
            ));
        }
        let cancelled = self.repository.cancel_all_game_node_video_tasks(game_id)?;
        for batch in batches {
            self.repository.cancel_game_task(
                batch["id"].as_str().unwrap_or_default(),
                "串行节点视频生成已取消",
            )?;
        }
        let mut provider_errors = Vec::new();
        for task in &tasks {
            let provider_task_id = task["provider_task_id"].as_str().unwrap_or_default();
            if provider_task_id.is_empty() {
                continue;
            }
            if let Err(error) = self.worker_provider_cancel(provider_task_id) {
                provider_errors.push(json!({"task_id": task["id"], "error": error.to_string()}));
            }
        }
        Ok(json!({
            "game_id": game_id,
            "cancelled_count": cancelled.len(),
            "cancelled_tasks": cancelled,
            "provider_cancel_errors": provider_errors,
            "status": CANCELLED,
        }))
    }
}
