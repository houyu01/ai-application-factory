//! Service boundary for cancelling a single interactive-game node's durable video task.

use serde_json::{json, Value};

use crate::{error::AppResult, service::DesktopService};

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
}
