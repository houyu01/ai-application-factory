//! Durable ZIP-export service flow for the short-drama workbench.

use std::path::Path;

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

    /// Save the completed archive outside the app data directory after a creator selects a destination.
    ///
    /// The Tauri save command triggers this after the ZIP export dialog returns a native filesystem path.
    /// It verifies the scoped durable task before copying, which prevents IPC callers from reading arbitrary
    /// media through this file-writing boundary.
    pub fn save_video_export(
        &self,
        project_id: &str,
        task_id: &str,
        destination: &Path,
    ) -> AppResult<()> {
        if destination.extension().and_then(|value| value.to_str()) != Some("zip") {
            return Err(AppError::BadRequest(
                "保存文件必须使用 .zip 扩展名".to_owned(),
            ));
        }
        let task = self.repository.video_export_task(project_id, task_id)?;
        if task["status"].as_str() != Some("生成成功") {
            return Err(AppError::Conflict("视频 ZIP 尚未准备完成".to_owned()));
        }
        let source_url = task["result"]["url"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::NotFound("视频 ZIP 文件不存在".to_owned()))?;
        self.media.copy_video_export_zip(source_url, destination)
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
    use std::fs;

    use serde_json::{json, Map};

    use crate::{
        db::Database,
        media::MediaStore,
        repository::Repository,
        value::{new_id, SUCCEEDED},
        worker::DurableWorker,
    };

    use super::{video_export_format, DesktopService};

    #[test]
    fn video_export_accepts_only_mp4() {
        let mp4 = Map::from_iter([("format".to_owned(), json!("mp4"))]);
        let wav = Map::from_iter([("format".to_owned(), json!("wav"))]);

        assert_eq!(video_export_format(&mp4).expect("mp4 export"), "mp4");
        assert!(video_export_format(&wav).is_err());
    }

    #[test]
    fn completed_video_export_is_copied_to_the_creator_selected_zip_path() {
        let root = std::env::temp_dir().join(format!("ai-video-export-save-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        let media = MediaStore::new(repository.clone()).expect("media store");
        let service = DesktopService {
            repository: repository.clone(),
            media: media.clone(),
            worker: DurableWorker::new(repository.clone(), media).expect("durable worker"),
        };
        let project = repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("保存导出短剧")),
                (
                    "script".to_owned(),
                    json!("林岩与苏晚在旧宅找到一段决定命运的录像。"),
                ),
            ]))
            .expect("project");
        let project_id = project["id"].as_str().expect("project id");
        let task = repository
            .create_active_drama_task(project_id, "drama_video_export", Some("mp4"), json!({}))
            .expect("export task");
        let task_id = task["id"].as_str().expect("task id");
        let staging_zip = root.join("archive.zip");
        fs::write(&staging_zip, b"completed-video-zip").expect("archive bytes");
        let source_url = service
            .media
            .save_video_export_zip(&staging_zip)
            .expect("publish archive");
        repository
            .finish_drama_task(
                task_id,
                SUCCEEDED,
                Some(json!({"url":source_url,"file_name":"保存导出短剧-视频合集.zip"})),
                None,
            )
            .expect("finish export");

        let destination = root.join("creator-selected.zip");
        service
            .save_video_export(project_id, task_id, &destination)
            .expect("save completed archive");

        assert_eq!(
            fs::read(destination).expect("saved ZIP"),
            b"completed-video-zip"
        );
        fs::remove_dir_all(root).expect("remove test data");
    }
}
