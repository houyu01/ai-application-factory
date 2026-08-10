//! Durable coordinator for toolbar-triggered serial storyboard video generation.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    repository::ShotVersionInput,
    value::{CANCELLED, FAILED, GENERATING, SUCCEEDED},
};

use super::{video_snapshot, DesktopService};

pub(super) const SERIAL_VIDEO_BATCH: &str = "serial_shot_video_batch";
const ALL_SHOTS: &str = "all";

impl DesktopService {
    /// The detail toolbar's “串行生成” menu starts one durable coordinator that waits for each prior video before queueing the next shot.
    pub fn start_serial_shot_video_batch(&self, project_id: &str) -> AppResult<Value> {
        let active =
            self.repository
                .active_drama_tasks(project_id, SERIAL_VIDEO_BATCH, Some(ALL_SHOTS))?;
        if let Some(batch) = active.into_iter().next() {
            return Ok(batch);
        }
        let project = self.repository.get_drama(project_id)?;
        let (shots, _) = self.repository.list_shots(project_id)?;
        if shots.is_empty() {
            return Err(AppError::BadRequest(
                "当前项目没有可生成视频的分镜".to_owned(),
            ));
        }
        for shot in &shots {
            self.validate_video_preflight(&project, shot)?;
            if !self
                .repository
                .active_drama_tasks(project_id, "shot_video", shot["id"].as_str())?
                .is_empty()
            {
                return Err(AppError::BadRequest(
                    "存在正在生成的视频，请完成或取消后再串行生成".to_owned(),
                ));
            }
        }
        let shot_ids = shots
            .iter()
            .filter_map(|shot| shot["id"].as_str())
            .collect::<Vec<_>>();
        let batch = self.repository.create_active_drama_task(
            project_id,
            SERIAL_VIDEO_BATCH,
            Some(ALL_SHOTS),
            json!({
                "project_id": project_id,
                "mode": "serial",
                "shot_ids": shot_ids,
                "total_count": shots.len(),
                "next_index": 0,
                "completed_count": 0,
                "current_task_id": null,
                "current_shot_id": null,
            }),
        )?;
        self.advance_serial_shot_video_batch(
            project_id,
            batch["id"].as_str().unwrap_or_default(),
            None,
        )
    }

    /// The frontend resumes the durable serial coordinator after it extracts a completed video's tail frame from the WebView.
    pub fn advance_serial_shot_video_batch(
        &self,
        project_id: &str,
        batch_id: &str,
        last_frame_data_url: Option<&str>,
    ) -> AppResult<Value> {
        let batch = self.repository.get_drama_task(batch_id)?;
        if batch["drama_id"].as_str() != Some(project_id) || batch["type"] != SERIAL_VIDEO_BATCH {
            return Err(AppError::NotFound("串行视频批次不存在".to_owned()));
        }
        if batch["status"] != GENERATING {
            return Ok(batch);
        }
        let mut snapshot = batch["input_snapshot"]
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("串行视频批次缺少任务数据".to_owned()))?;
        let shot_ids = snapshot
            .get("shot_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let current_task_id = snapshot
            .get("current_task_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !current_task_id.is_empty() {
            let child = self.repository.get_drama_task(current_task_id)?;
            match child["status"].as_str().unwrap_or_default() {
                GENERATING => return Ok(batch),
                SUCCEEDED => {
                    let completed = snapshot
                        .get("completed_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        + 1;
                    snapshot.insert("completed_count".to_owned(), json!(completed));
                }
                FAILED => {
                    return self.finish_serial_batch(
                        batch_id,
                        FAILED,
                        &snapshot,
                        "上一分镜视频生成失败，串行生成已停止",
                    )
                }
                CANCELLED => {
                    return self.finish_serial_batch(
                        batch_id,
                        CANCELLED,
                        &snapshot,
                        "上一分镜视频已取消，串行生成已停止",
                    )
                }
                _ => return Ok(batch),
            }
        }
        let next = snapshot
            .get("next_index")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if next >= shot_ids.len() {
            return self.finish_serial_batch(batch_id, SUCCEEDED, &snapshot, "");
        }
        let first_frame = if next == 0 {
            None
        } else {
            let value = last_frame_data_url
                .filter(|value| value.starts_with("data:image/"))
                .ok_or_else(|| {
                    AppError::BadRequest("请先提取上一分镜视频的尾帧，再继续串行生成".to_owned())
                })?;
            Some(self.media.save_data_url(value)?)
        };
        let project = self.repository.get_drama(project_id)?;
        let shot_id = &shot_ids[next];
        let shot = self.repository.get_shot(project_id, shot_id)?;
        self.validate_video_preflight(&project, &shot)?;
        if !self
            .repository
            .active_drama_tasks(project_id, "shot_video", Some(shot_id))?
            .is_empty()
        {
            return Err(AppError::BadRequest(
                "下一分镜已有正在生成的视频，无法继续串行生成".to_owned(),
            ));
        }
        let task = self.enqueue_shot_video_run(
            project_id,
            shot_id,
            &project,
            &shot,
            first_frame.as_deref(),
            Some(batch_id),
        )?;
        snapshot.insert("next_index".to_owned(), json!(next + 1));
        snapshot.insert("current_task_id".to_owned(), task["id"].clone());
        snapshot.insert("current_shot_id".to_owned(), json!(shot_id));
        self.repository
            .update_drama_task_snapshot(batch_id, Value::Object(snapshot))?;
        self.repository
            .update_drama_task_progress(batch_id, 0, "正在等待当前分镜视频")?;
        self.repository.get_drama_task(batch_id)
    }

    /// Create one video run while freezing optional serial continuity input in the child task rather than changing editor-owned frame settings.
    pub(super) fn enqueue_shot_video_run(
        &self,
        project_id: &str,
        shot_id: &str,
        project: &Value,
        shot: &Value,
        serial_first_frame: Option<&str>,
        parent_task_id: Option<&str>,
    ) -> AppResult<Value> {
        self.repository
            .set_shot_status(project_id, shot_id, GENERATING)?;
        let task = self.repository.create_parallel_drama_task(
            project_id,
            "shot_video",
            Some(shot_id),
            json!({"project_id": project_id, "shot_id": shot_id}),
        )?;
        let task_id = task["id"].as_str().unwrap_or_default();
        let version = self.repository.create_shot_version_with_input(
            project_id,
            shot_id,
            task_id,
            ShotVersionInput {
                prompt: shot["prompt"].as_str().unwrap_or_default().to_owned(),
                prompt_rich: video_snapshot::prompt_rich(project, shot),
                structured: shot["structured"].clone(),
                refinement: None,
            },
        )?;
        let mut snapshot = Map::from_iter([
            ("project_id".to_owned(), json!(project_id)),
            ("shot_id".to_owned(), json!(shot_id)),
            ("version_id".to_owned(), version["id"].clone()),
        ]);
        if let Some(frame) = serial_first_frame {
            snapshot.insert("serial_first_frame".to_owned(), json!(frame));
        }
        if let Some(parent) = parent_task_id {
            snapshot.insert("parent_task_id".to_owned(), json!(parent));
        }
        self.repository
            .update_drama_task_snapshot(task_id, Value::Object(snapshot))?;
        self.repository.get_drama_task(task_id)
    }

    fn finish_serial_batch(
        &self,
        batch_id: &str,
        status: &str,
        snapshot: &Map<String, Value>,
        error: &str,
    ) -> AppResult<Value> {
        let total = snapshot
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let completed = snapshot
            .get("completed_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        self.repository.finish_drama_task(
            batch_id,
            status,
            Some(json!({"total_count": total, "completed_count": completed})),
            (!error.is_empty()).then_some(error),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::params;
    use serde_json::{json, Map};

    use crate::{
        db::Database,
        media::MediaStore,
        repository::Repository,
        value::{new_id, now, SUCCEEDED},
        worker::DurableWorker,
    };

    use super::DesktopService;

    fn service() -> (DesktopService, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("serial-video-batch-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        let media = MediaStore::new(repository.clone()).expect("media store");
        let worker = DurableWorker::new(repository.clone(), media.clone()).expect("worker");
        (
            DesktopService {
                repository,
                media,
                worker,
            },
            root,
        )
    }

    fn insert_shot(service: &DesktopService, project_id: &str, id: &str, index: i64) {
        service
            .repository
            .db
            .with_connection(|connection| {
                connection.execute(
                    "INSERT INTO drama_shots (id,drama_id,episode_id,episode_name,episode_sort_order,shot_index,title,original_text,duration_seconds,prompt,prompt_rich_json,status,created_at,updated_at) VALUES (?1,?2,'episode:1','第1集',1,?3,?4,'分镜内容',10,'镜头缓慢推进','[]','未生成',?5,?5)",
                    params![id, project_id, index, format!("第1集镜头{index}"), now()],
                )?;
                Ok(())
            })
            .expect("insert shot");
    }

    #[test]
    fn serial_batch_enqueues_only_one_child_and_freezes_the_previous_tail_frame() {
        let (service, root) = service();
        let project = service
            .repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("串行视频短剧")),
                (
                    "script".to_owned(),
                    json!("这是用于串行分镜视频生成的测试剧本内容。"),
                ),
            ]))
            .expect("create project");
        let project_id = project["id"].as_str().expect("project id");
        insert_shot(&service, project_id, "serial-shot-1", 1);
        insert_shot(&service, project_id, "serial-shot-2", 2);

        let batch = service
            .start_serial_shot_video_batch(project_id)
            .expect("start serial batch");
        let first_task_id = batch["input_snapshot"]["current_task_id"]
            .as_str()
            .expect("first task id");
        assert_eq!(batch["input_snapshot"]["next_index"], 1);
        assert_eq!(batch["input_snapshot"]["current_shot_id"], "serial-shot-1");
        assert_eq!(
            service
                .repository
                .active_drama_tasks(project_id, "shot_video", None)
                .expect("active children")
                .len(),
            1
        );
        service
            .repository
            .finish_drama_task(
                first_task_id,
                SUCCEEDED,
                Some(json!({"url":"/api/media/first.mp4"})),
                None,
            )
            .expect("finish first child");

        let advanced = service
            .advance_serial_shot_video_batch(
                project_id,
                batch["id"].as_str().expect("batch id"),
                Some("data:image/png;base64,aQ=="),
            )
            .expect("advance serial batch");
        let second_task_id = advanced["input_snapshot"]["current_task_id"]
            .as_str()
            .expect("second task id");
        let second = service
            .repository
            .get_drama_task(second_task_id)
            .expect("second child task");
        assert_eq!(
            advanced["input_snapshot"]["current_shot_id"],
            "serial-shot-2"
        );
        assert_eq!(advanced["input_snapshot"]["completed_count"], 1);
        assert_eq!(second["input_snapshot"]["parent_task_id"], batch["id"]);
        assert!(second["input_snapshot"]["serial_first_frame"]
            .as_str()
            .is_some_and(|url| url.starts_with("/api/media/")));
        assert_eq!(
            service
                .repository
                .active_drama_tasks(project_id, "shot_video", None)
                .expect("active children")
                .len(),
            1
        );
        drop(service);
        fs::remove_dir_all(root).expect("remove test data");
    }
}
