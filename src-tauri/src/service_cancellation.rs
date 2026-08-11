//! Cancellation flows that stop local persistence first and then request provider cancellation when available.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    service::DesktopService,
    value::CANCELLED,
};

const SERIAL_VIDEO_BATCH: &str = "serial_shot_video_batch";

impl DesktopService {
    /// Cancel one shot or all project videos, keeping local cancellation durable if a provider request later fails.
    pub fn cancel_videos(&self, project_id: &str, shot_id: Option<&str>) -> AppResult<Value> {
        if let Some(shot) = shot_id {
            self.repository.get_shot(project_id, shot)?;
        }
        let tasks = self
            .repository
            .active_drama_tasks(project_id, "shot_video", shot_id)?;
        let serial_batches = self
            .repository
            .active_drama_tasks(project_id, SERIAL_VIDEO_BATCH, Some("all"))?
            .into_iter()
            .filter(|task| {
                shot_id.is_none() || task["input_snapshot"]["current_shot_id"].as_str() == shot_id
            })
            .collect::<Vec<_>>();
        if tasks.is_empty() && serial_batches.is_empty() {
            return Err(AppError::BadRequest(
                "当前分镜没有正在生成的视频任务".to_owned(),
            ));
        }
        self.repository
            .cancel_drama_tasks(project_id, Some("shot_video"), shot_id)?;
        for batch in serial_batches {
            self.repository.cancel_drama_task(
                batch["id"].as_str().unwrap_or_default(),
                "串行视频生成已取消",
            )?;
        }
        let mut cancelled = Vec::new();
        let mut provider_errors = Vec::new();
        for task in tasks {
            let id = task["id"].as_str().unwrap_or_default();
            let shot = task["resource_id"].as_str().unwrap_or_default();
            if let Some(version) = task["input_snapshot"]["version_id"]
                .as_str()
                .filter(|value| !value.is_empty())
            {
                self.repository
                    .cancel_shot_version(project_id, shot, version)?;
            }
            let provider_id = task["provider_task_id"].as_str().unwrap_or_default();
            let mut item = json!({"id":id,"project_id":project_id,"resource_id":shot,"status":CANCELLED,"provider_cancelled":false});
            if !provider_id.is_empty() {
                match self.worker_provider_cancel(provider_id) {
                    Ok(()) => item["provider_cancelled"] = json!(true),
                    Err(error) => {
                        item["provider_cancel_error"] = json!(error.to_string());
                        provider_errors.push(json!({"task_id":id,"error":error.to_string()}));
                    }
                }
            }
            cancelled.push(item);
        }
        for shot in cancelled
            .iter()
            .filter_map(|item| item["resource_id"].as_str())
        {
            if self
                .repository
                .active_drama_tasks(project_id, "shot_video", Some(shot))?
                .is_empty()
            {
                self.repository
                    .set_shot_status(project_id, shot, CANCELLED)?;
            }
        }
        Ok(
            json!({"project_id":project_id,"cancelled_count":cancelled.len(),"cancelled_tasks":cancelled,"provider_cancel_errors":provider_errors}),
        )
    }

    pub(crate) fn worker_provider_cancel(&self, provider_task_id: &str) -> AppResult<()> {
        // Constructing a short-lived provider client prevents stale settings from being retained after a settings edit.
        crate::providers::ProviderClient::new(self.repository.clone(), self.media.clone())?
            .cancel_video(provider_task_id)
    }
}
