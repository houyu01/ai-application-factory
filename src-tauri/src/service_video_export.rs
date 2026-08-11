//! Durable ZIP-export service flow for the short-drama workbench.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    service::DesktopService,
};

impl DesktopService {
    /// Create the export task after the download dialog has frozen one or more completed shot versions.
    ///
    /// The short-drama workbench calls this when its “下载 ZIP” button is pressed. It validates the
    /// selected versions before creating a restart-safe task, leaving partial-episode concatenation to the worker.
    pub fn enqueue_video_export(
        &self,
        project_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let format = video_export_format(&values)?;
        let selections = self.repository.video_export_snapshot(project_id, &values)?;
        self.repository.create_active_drama_task(
            project_id,
            "drama_video_export",
            Some(format),
            json!({"project_id":project_id,"format":format,"selections":selections}),
        )
    }

    /// Return the export task displayed by the ZIP dialog, including its final local media URL.
    pub fn video_export_task(&self, project_id: &str, task_id: &str) -> AppResult<Value> {
        self.repository.video_export_task(project_id, task_id)
    }

    /// Stop a queued or currently processing export without affecting generation or existing media.
    pub fn cancel_video_export(&self, project_id: &str, task_id: &str) -> AppResult<Value> {
        let task = self.repository.video_export_task(project_id, task_id)?;
        if task["status"].as_str() != Some("生成中") {
            return Ok(task);
        }
        self.repository.cancel_drama_task(task_id, "视频打包已取消")
    }
}

fn video_export_format(values: &Map<String, Value>) -> AppResult<&str> {
    let format = values
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if format == "mp4" {
        Ok(format)
    } else {
        Err(AppError::BadRequest("导出格式仅支持 mp4".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use super::video_export_format;

    #[test]
    fn video_export_accepts_only_mp4() {
        let mp4 = Map::from_iter([("format".to_owned(), json!("mp4"))]);
        let wav = Map::from_iter([("format".to_owned(), json!("wav"))]);

        assert_eq!(video_export_format(&mp4).expect("mp4 export"), "mp4");
        assert!(video_export_format(&wav).is_err());
    }
}
