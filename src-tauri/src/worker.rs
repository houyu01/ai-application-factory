//! Durable local worker that resumes SQLite-backed drama and game tasks after an app restart.

use std::sync::{atomic::AtomicBool, Arc};

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    media::MediaStore,
    providers::{ProviderClient, VideoJob},
    repository::Repository,
    value::{FAILED, GENERATING, SUCCEEDED},
};

#[path = "worker_batch.rs"]
mod batch;
#[path = "worker_cover.rs"]
mod cover;
#[path = "worker_decomposition_assets.rs"]
mod decomposition_assets;
#[path = "worker_decomposition_checkpoint.rs"]
mod decomposition_checkpoint;
#[path = "worker_expansion.rs"]
pub(super) mod expansion;
#[path = "worker_game.rs"]
mod game;
#[path = "worker_game_asset_prompt.rs"]
mod game_asset_prompt;
#[path = "worker_game_cover.rs"]
mod game_cover;
#[path = "worker_game_placeholder.rs"]
mod game_placeholder;
#[path = "worker_game_video.rs"]
mod game_video;
#[path = "worker_lease.rs"]
mod lease;
#[path = "worker_long_plan.rs"]
mod long_plan;
#[path = "worker_long_plan_pipeline.rs"]
mod long_plan_pipeline;
#[path = "worker_placeholder.rs"]
mod placeholder;
#[path = "worker_prompt_helpers.rs"]
mod prompt_helpers;
#[path = "worker_queues.rs"]
mod queues;
#[path = "worker_retry.rs"]
mod retry;
#[path = "worker_text.rs"]
mod text;
#[path = "worker_video_export.rs"]
mod video_export;
#[path = "worker_video_export_zip.rs"]
mod video_export_zip;
#[path = "worker_video_inputs.rs"]
mod video_inputs;
#[path = "worker_video_reference_lock.rs"]
mod video_reference_lock;
#[path = "worker_video_reference_plan.rs"]
mod video_reference_plan;
#[path = "worker_video_refinement_inputs.rs"]
mod video_refinement_inputs;
#[path = "worker_voice_audio.rs"]
mod voice_audio;

/// Coordinates model-only network work around locally persisted jobs, keeping UI requests non-blocking.
#[derive(Clone)]
pub struct DurableWorker {
    repository: Repository,
    media: MediaStore,
    providers: ProviderClient,
    running: Arc<AtomicBool>,
    queues: Arc<queues::QueueControl>,
}

impl DurableWorker {
    /// Construct worker dependencies once during Tauri setup, after app-local paths and SQLite exist.
    pub fn new(repository: Repository, media: MediaStore) -> AppResult<Self> {
        let queues = Arc::new(queues::QueueControl::from_repository(&repository)?);
        Ok(Self {
            providers: ProviderClient::new(repository.clone(), media.clone())?,
            repository,
            media,
            running: Arc::new(AtomicBool::new(true)),
            queues,
        })
    }

    /// Start bounded local worker threads; SQLite task leases make duplicated app launches and restart recovery safe.
    pub fn start(&self) {
        queues::start(self);
    }

    /// Resize the affected provider queue immediately after its Settings card passes a real model probe.
    pub(crate) fn set_queue_concurrency(&self, model_kind: &str, concurrency: usize) {
        queues::set_concurrency(self, model_kind, concurrency);
    }

    /// Process at most one drama and one game task so tests can deterministically drain work without a UI.
    pub fn process_once(&self) -> AppResult<bool> {
        let drama = self.repository.claim_drama_task()?;
        if let Some(task) = drama {
            self.run_drama(task);
            return Ok(true);
        }
        if let Some(task) = self.repository.claim_voice_audio_task()? {
            self.run_voice_audio(task);
            return Ok(true);
        }
        let game = self.repository.claim_game_task()?;
        if let Some(task) = game {
            self.run_game(task);
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn run_drama(&self, task: Value) {
        let id = task["id"].as_str().unwrap_or_default();
        let project = task["drama_id"].as_str().unwrap_or_default();
        let _lease = lease::DramaTaskLease::start(self.repository.clone(), &task);
        let result = match task["type"].as_str().unwrap_or_default() {
            "script_decomposition" => self.decompose(id, project),
            "script_expansion" => self.expand(id, project),
            "shot_prompt" => self.shot_prompt(
                id,
                project,
                task["resource_id"].as_str().unwrap_or_default(),
            ),
            "shot_quality" => self.shot_quality(
                id,
                project,
                task["resource_id"].as_str().unwrap_or_default(),
            ),
            "asset_image" => self.asset_image(
                id,
                project,
                task["resource_id"].as_str().unwrap_or_default(),
            ),
            "asset_variant_image" => self.variant_image(id, project, &task),
            "asset_image_batch" | "shot_reference_image_batch" => {
                self.asset_batch(id, project, &task)
            }
            "placeholder_image" => self.placeholder_image(id, project, &task),
            "cover_image" => self.cover_image(id, project, &task),
            "shot_video" => self.shot_video(id, project, &task),
            "drama_video_export" => self.export_drama_videos(id, project, &task),
            other => Err(crate::error::AppError::BadRequest(format!(
                "未知的短剧任务类型：{other}"
            ))),
        };
        if let Err(error) = result {
            if self.retry_durable_provider_error(&task, id, &error) {
                return;
            }
            let persisted = self
                .repository
                .finish_drama_task(id, FAILED, None, Some(&error.to_string()))
                .ok()
                .is_some_and(|saved| saved["status"] == FAILED);
            if persisted {
                self.reflect_drama_task_failure(&task, project, &error.to_string());
            }
        }
    }

    /// Mirror a task failure only to the affected asset, shot, or bootstrap project state.
    fn reflect_drama_task_failure(&self, task: &Value, project_id: &str, error: &str) {
        let kind = task["type"].as_str().unwrap_or_default();
        let resource = task["resource_id"].as_str().unwrap_or_default();
        match kind {
            "script_decomposition" => {
                let _ = self.repository.set_drama_status(project_id, FAILED);
            }
            "asset_image" | "cover_image" | "placeholder_image" => {
                let _ = self
                    .repository
                    .set_asset_status(project_id, resource, FAILED);
            }
            "asset_variant_image" => {
                let snapshot = &task["input_snapshot"];
                let asset = snapshot["asset_id"].as_str().unwrap_or_default();
                let variant = snapshot["variant_id"].as_str().unwrap_or(resource);
                let _ = self
                    .repository
                    .set_asset_variant_status(project_id, asset, variant, FAILED);
            }
            "shot_video" => {
                let version = task["input_snapshot"]["version_id"]
                    .as_str()
                    .unwrap_or_default();
                let _ = self.repository.finish_shot_version(
                    project_id,
                    resource,
                    version,
                    FAILED,
                    None,
                    Some(error),
                );
            }
            _ => {}
        }
    }

    fn asset_image(&self, id: &str, project_id: &str, asset_id: &str) -> AppResult<()> {
        self.repository
            .set_asset_status(project_id, asset_id, GENERATING)?;
        let project = self.repository.get_drama(project_id)?;
        let asset = self.repository.get_asset(project_id, asset_id)?;
        let prompt = self.asset_generation_prompt(&project, &asset);
        let url = self.providers.image(
            &prompt,
            project["ratio"].as_str().unwrap_or("9:16"),
            &[],
            project["multimodal_model"].as_str(),
        )?;
        self.repository
            .mark_asset_succeeded(project_id, asset_id, &url)?;
        self.repository.finish_drama_task(
            id,
            SUCCEEDED,
            Some(json!({"asset_id":asset_id,"url":url})),
            None,
        )?;
        Ok(())
    }

    fn variant_image(&self, id: &str, project_id: &str, task: &Value) -> AppResult<()> {
        let snapshot = &task["input_snapshot"];
        let asset_id = snapshot["asset_id"]
            .as_str()
            .or_else(|| task["resource_id"].as_str())
            .unwrap_or_default();
        let variant_id = snapshot["variant_id"].as_str().unwrap_or_default();
        let asset = self.repository.get_asset(project_id, asset_id)?;
        let variant = asset["variants"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["id"].as_str() == Some(variant_id))
            })
            .cloned()
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!("Asset variant not found: {variant_id}"))
            })?;
        self.repository
            .set_asset_variant_status(project_id, asset_id, variant_id, GENERATING)?;
        let project = self.repository.get_drama(project_id)?;
        let mut variant_asset = asset.clone();
        let base = asset["prompt"].as_str().unwrap_or_default().trim();
        let variant_prompt = variant["prompt"].as_str().unwrap_or_default().trim();
        variant_asset["prompt"] = json!([base, variant_prompt]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"));
        let prompt = self.asset_generation_prompt(&project, &variant_asset);
        let url = self.providers.image(
            &prompt,
            project["ratio"].as_str().unwrap_or("9:16"),
            &[],
            project["multimodal_model"].as_str(),
        )?;
        self.repository
            .set_asset_variant_image(project_id, asset_id, variant_id, &url, SUCCEEDED)?;
        self.repository.finish_drama_task(
            id,
            SUCCEEDED,
            Some(json!({"asset_id":asset_id,"variant_id":variant_id,"url":url,"prompt":prompt})),
            None,
        )?;
        Ok(())
    }

    fn shot_video(&self, id: &str, project_id: &str, task: &Value) -> AppResult<()> {
        let shot_id = task["resource_id"].as_str().unwrap_or_default();
        let project = self.repository.get_drama(project_id)?;
        let mut shot = self.repository.get_shot(project_id, shot_id)?;
        if let Some(frame) = task["input_snapshot"]["serial_first_frame"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            shot["first_last_frames"]["first"] = json!({
                "url": frame,
                "source": "serial_previous_video",
                "position": "last",
            });
        }
        let version = task["input_snapshot"]["version_id"]
            .as_str()
            .unwrap_or_default();
        let refinement = task["input_snapshot"]["refinement"].as_object();
        let (prompt, refs, audio_refs) = if let Some(refinement) = refinement {
            self.video_refinement_inputs(&project, &shot, &Value::Object(refinement.clone()))?
        } else {
            self.video_generation_inputs(&project, &shot)?
        };
        let source_video = refinement
            .and_then(|value| value.get("source_video_url"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(|value| {
                self.media.provider_reference_url(value).ok_or_else(|| {
                    AppError::BadRequest(
                        "原始视频无法发送给当前视频模型，请检查媒体存储配置".to_owned(),
                    )
                })
            })
            .transpose()?;
        let result = if let Some(provider_task_id) = task["provider_task_id"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            self.providers.poll_video(provider_task_id)?
        } else {
            self.providers.start_video(
                &prompt,
                project["ratio"].as_str().unwrap_or("9:16"),
                project["resolution"].as_str().unwrap_or("720p"),
                shot["duration_seconds"].as_i64().unwrap_or(10),
                &refs,
                &audio_refs,
                source_video.as_deref(),
                project["video_model"].as_str(),
            )?
        };
        let url = match result {
            VideoJob::Ready(url) => url,
            VideoJob::Pending {
                provider_task_id,
                progress,
            } => {
                self.repository.schedule_shot_version_poll(
                    project_id,
                    shot_id,
                    version,
                    &provider_task_id,
                    progress,
                )?;
                self.repository.schedule_drama_provider_poll(
                    id,
                    &provider_task_id,
                    progress,
                    "正在等待视频模型结果",
                )?;
                return Ok(());
            }
        };
        self.repository.finish_shot_version(
            project_id,
            shot_id,
            version,
            SUCCEEDED,
            Some(&url),
            None,
        )?;
        self.repository.finish_drama_task(
            id,
            SUCCEEDED,
            Some(json!({"shot_id":shot_id,"url":url})),
            None,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Map};

    use super::DurableWorker;
    use crate::{
        db::Database,
        media::MediaStore,
        planner,
        repository::Repository,
        value::{new_id, NOT_GENERATED},
    };

    #[test]
    fn prompt_failure_does_not_mark_an_unstarted_video_as_failed() {
        let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        let project = repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("状态隔离短剧")),
                (
                    "script".to_owned(),
                    json!("林观在旧宅发现一封密信，决定追查它的来历。"),
                ),
            ]))
            .expect("create project");
        let project_id = project["id"].as_str().expect("project id");
        let plan = planner::fallback_drama_plan(
            "林观在旧宅发现一封密信，决定追查它的来历。",
            "真人风格",
            "悬疑",
            80,
        );
        repository
            .save_drama_decomposition(project_id, &plan)
            .expect("save plan");
        let shot_id = repository.get_drama(project_id).expect("detail")["shots"][0]["id"]
            .as_str()
            .expect("shot id")
            .to_owned();
        let worker = DurableWorker::new(
            repository.clone(),
            MediaStore::new(repository.clone()).expect("media store"),
        )
        .expect("worker");

        worker.reflect_drama_task_failure(
            &json!({"type":"shot_prompt","resource_id":shot_id}),
            project_id,
            "提示词模型不可用",
        );

        assert_eq!(
            repository.get_shot(project_id, &shot_id).expect("shot")["status"],
            NOT_GENERATED
        );
        fs::remove_dir_all(root).expect("remove test data");
    }
}
