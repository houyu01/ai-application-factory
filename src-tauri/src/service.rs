//! Desktop application service boundary combining local persistence, media, and durable workers.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::{
    db::Database,
    error::{AppError, AppResult},
    media::MediaStore,
    providers::ProviderClient,
    repository::Repository,
    value::{GENERATING, SUCCEEDED},
    worker::DurableWorker,
};

#[path = "service_cover.rs"]
mod cover;
#[path = "service_placeholder.rs"]
mod placeholder;
#[path = "service_reference_images.rs"]
mod reference_images;
#[path = "service_restart.rs"]
mod restart;
#[path = "service_screenplay.rs"]
mod screenplay;
#[path = "service_serial_video_batch.rs"]
mod serial_video_batch;
#[path = "service_video_refinement.rs"]
mod video_refinement;
#[path = "service_video_snapshot.rs"]
mod video_snapshot;

/// Owns all local application state; Tauri commands delegate here instead of starting an HTTP or Python server.
#[derive(Clone)]
pub struct DesktopService {
    pub repository: Repository,
    pub media: MediaStore,
    #[allow(dead_code)]
    worker: DurableWorker,
}

impl DesktopService {
    /// Initialize SQLite, local media, and restart-safe workers under the operating system's app-data directory.
    pub fn open(data_dir: PathBuf) -> AppResult<Self> {
        let database = Database::open(data_dir.join("ai_application_factory.db"))?;
        let repository = Repository::new(database);
        let media = MediaStore::new(repository.clone())?;
        let worker = DurableWorker::new(repository.clone(), media.clone())?;
        worker.start();
        Ok(Self {
            repository,
            media,
            worker,
        })
    }

    /// Run one local task synchronously for deterministic Rust tests without relying on a window event loop.
    #[allow(dead_code)]
    pub fn process_one_task(&self) -> AppResult<bool> {
        self.worker.process_once()
    }

    /// Create a project with only currently selectable configured models, matching the stale-form protection in Python.
    pub fn create_drama(&self, mut values: Map<String, Value>) -> AppResult<Value> {
        for (kind, field) in [
            ("language", "language_model"),
            ("multimodal", "multimodal_model"),
            ("video", "video_model"),
        ] {
            let config = self.repository.setting(kind)?;
            let models = config["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .fold(Vec::new(), |mut all, model| {
                    if !all.contains(&model.to_owned()) {
                        all.push(model.to_owned());
                    }
                    all
                });
            let default = config["model"].as_str().unwrap_or_default().trim();
            let mut available = models;
            if !default.is_empty() && !available.contains(&default.to_owned()) {
                available.insert(0, default.to_owned());
            }
            if available.is_empty() {
                continue;
            }
            let requested = values
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if !available.iter().any(|model| model == requested) {
                values.insert(
                    field.to_owned(),
                    json!(if available.iter().any(|model| model == default) {
                        default
                    } else {
                        available.first().map(String::as_str).unwrap_or_default()
                    }),
                );
            }
        }
        let mut project = self.repository.create_drama(values)?;
        let queue = self
            .repository
            .drama_queue_metadata(project["id"].as_str().unwrap_or_default())?;
        project
            .as_object_mut()
            .expect("project is an object")
            .extend(queue);
        Ok(project)
    }

    /// Create an idempotent asset-image task and reflect its generating state before returning to the UI.
    pub fn enqueue_asset_image(&self, project: &str, asset: &str) -> AppResult<Value> {
        self.repository
            .set_asset_status(project, asset, GENERATING)?;
        self.repository.create_active_drama_task(
            project,
            "asset_image",
            Some(asset),
            json!({"project_id":project,"asset_id":asset}),
        )
    }

    /// Create the durable alternative-form image task used by one variant card.
    pub fn enqueue_variant_image(
        &self,
        project: &str,
        asset: &str,
        variant: &str,
    ) -> AppResult<Value> {
        self.repository
            .set_asset_variant_status(project, asset, variant, GENERATING)?;
        self.repository.create_active_drama_task(
            project,
            "asset_variant_image",
            Some(variant),
            json!({"project_id":project,"asset_id":asset,"variant_id":variant}),
        )
    }

    /// Queue prompt generation without changing the shot's video-generation state.
    pub fn enqueue_shot_prompt(&self, project: &str, shot: &str) -> AppResult<Value> {
        self.repository.create_active_drama_task(
            project,
            "shot_prompt",
            Some(shot),
            json!({"project_id":project,"shot_id":shot}),
        )
    }

    /// Queue quality validation for a selected editable shot.
    pub fn enqueue_shot_quality(&self, project: &str, shot: &str) -> AppResult<Value> {
        self.repository.create_active_drama_task(
            project,
            "shot_quality",
            Some(shot),
            json!({"project_id":project,"shot_id":shot}),
        )
    }

    /// Queue one to three independently persistent video runs, each with a version card for history/cancellation.
    pub fn enqueue_shot_videos(
        &self,
        project: &str,
        shot: &str,
        count: i64,
    ) -> AppResult<Vec<Value>> {
        if !(1..=3).contains(&count) {
            return Err(AppError::BadRequest(
                "一次生成视频的数量必须为 1 到 3".to_owned(),
            ));
        }
        let project_value = self.repository.get_drama(project)?;
        let shot_value = self.repository.get_shot(project, shot)?;
        self.validate_video_preflight(&project_value, &shot_value)?;
        let active = self
            .repository
            .active_drama_tasks(project, "shot_video", Some(shot))?;
        if !active.is_empty() {
            return Ok(active);
        }
        let mut tasks = Vec::new();
        for _ in 0..count {
            tasks.push(self.enqueue_shot_video_run(
                project,
                shot,
                &project_value,
                &shot_value,
                None,
                None,
            )?);
        }
        Ok(tasks)
    }

    /// Queue an ordered bulk asset run for a single drawer type.
    pub fn enqueue_asset_batch(
        &self,
        project: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let ids = values
            .get("asset_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .fold(Vec::new(), |mut all, id| {
                if !all.contains(&id) {
                    all.push(id);
                }
                all
            });
        if ids.is_empty() {
            return Err(AppError::BadRequest("请至少选择一个素材".to_owned()));
        }
        let detail = self.repository.get_drama(project)?;
        let selected = ids
            .iter()
            .filter_map(|id| {
                detail["assets"]
                    .as_array()?
                    .iter()
                    .find(|asset| asset["id"] == *id)
            })
            .collect::<Vec<_>>();
        if selected.len() != ids.len() {
            return Err(AppError::NotFound("素材不存在或不属于当前项目".to_owned()));
        }
        let kind = selected
            .first()
            .and_then(|asset| asset["type"].as_str())
            .unwrap_or_default();
        if !["character", "scene", "prop"].contains(&kind)
            || selected
                .iter()
                .any(|asset| asset["type"].as_str() != Some(kind))
        {
            return Err(AppError::BadRequest(
                "生成全部图片时只能选择同一类素材".to_owned(),
            ));
        }
        let jobs = selected.iter().flat_map(|asset| { let id = asset["id"].clone(); let mut jobs = vec![json!({"type":"asset_image","asset_id":id})]; jobs.extend(asset["variants"].as_array().into_iter().flatten().filter_map(|variant| variant["id"].as_str().map(|variant_id| json!({"type":"asset_variant_image","asset_id":asset["id"],"variant_id":variant_id})))); jobs }).collect::<Vec<_>>();
        self.repository.create_active_drama_task(
            project,
            "asset_image_batch",
            Some(kind),
            json!({"project_id":project,"asset_type":kind,"asset_ids":ids,"jobs":jobs,"batch_size":5,"next_index":0,"active_task_ids":[],"completed_count":0,"failed_count":0,"cancelled_count":0,"type":"asset_image_batch"}),
        )
    }

    /// Stop only image-related assets for one drawer tab without touching videos, prompts, or screenplay expansion.
    pub fn cancel_asset_images(&self, project: &str, kind: &str) -> AppResult<Value> {
        if !["character", "scene", "prop"].contains(&kind) {
            return Err(AppError::BadRequest(
                "只支持取消角色、场景或道具的图片任务".to_owned(),
            ));
        }
        let detail = self.repository.get_drama(project)?;
        let assets = detail["assets"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter(|asset| asset["type"].as_str() == Some(kind))
            .cloned()
            .collect::<Vec<_>>();
        let asset_ids = assets
            .iter()
            .filter_map(|asset| asset["id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let variants = assets
            .iter()
            .flat_map(|asset| {
                asset["variants"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(move |variant| {
                        variant["id"].as_str().map(|id| {
                            (
                                id.to_owned(),
                                asset["id"].as_str().unwrap_or_default().to_owned(),
                            )
                        })
                    })
            })
            .collect::<Vec<_>>();
        let label = match kind {
            "character" => "角色",
            "scene" => "场景",
            _ => "道具",
        };
        let mut cancelled = Vec::new();
        for task in self
            .repository
            .active_drama_tasks(project, "asset_image_batch", None)?
        {
            if task["resource_id"].as_str() == Some(kind) {
                cancelled.push(self.repository.cancel_drama_task(
                    task["id"].as_str().unwrap_or_default(),
                    &format!("{label}图片生成已取消"),
                )?);
            }
        }
        for task in self
            .repository
            .active_drama_tasks(project, "asset_image", None)?
        {
            if let Some(asset_id) = task["resource_id"].as_str() {
                if asset_ids.iter().any(|id| id == asset_id) {
                    self.repository
                        .set_asset_status(project, asset_id, "已取消")?;
                    cancelled.push(self.repository.cancel_drama_task(
                        task["id"].as_str().unwrap_or_default(),
                        &format!("{label}图片生成已取消"),
                    )?);
                }
            }
        }
        for task in self
            .repository
            .active_drama_tasks(project, "asset_variant_image", None)?
        {
            if let Some(variant_id) = task["resource_id"].as_str() {
                if let Some((_, asset_id)) = variants.iter().find(|(id, _)| id == variant_id) {
                    self.repository.set_asset_variant_status(
                        project,
                        asset_id,
                        variant_id,
                        "已取消",
                    )?;
                    cancelled.push(self.repository.cancel_drama_task(
                        task["id"].as_str().unwrap_or_default(),
                        &format!("{label}图片生成已取消"),
                    )?);
                }
            }
        }
        Ok(
            json!({"project_id":project,"asset_type":kind,"cancelled_count":cancelled.len(),"cancelled_tasks":cancelled}),
        )
    }

    /// Upload a user-selected reference and record it in both current-image and image-history fields.
    pub fn upload_asset(
        &self,
        project: &str,
        asset: &str,
        body: Map<String, Value>,
    ) -> AppResult<Value> {
        let data = body
            .get("data_url")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::BadRequest("缺少 data_url".to_owned()))?;
        let url = self.media.save_data_url(data)?;
        self.repository
            .set_asset_image(project, asset, &url, "uploaded", SUCCEEDED)
    }

    /// Run the object-store probe before committing a new storage target, preserving the old configuration on failure.
    pub fn save_storage(&self, values: Map<String, Value>) -> AppResult<Value> {
        let candidate = self.repository.storage_config_candidate(&values)?;
        self.media.probe(candidate)?;
        self.repository.save_storage_config(values)
    }

    /// Probe a candidate provider through the same Rust client used by workers before replacing saved settings.
    pub fn save_model_config(&self, values: Map<String, Value>) -> AppResult<Value> {
        let candidate = self.repository.model_config_candidate(&values)?;
        ProviderClient::for_model_probe(self.repository.clone(), self.media.clone())?
            .probe_model_config(&candidate)?;
        let saved = self.repository.save_model_config(values)?;
        let concurrency = saved["generation_concurrency"].as_u64().unwrap_or(2) as usize;
        let kind = saved["kind"].as_str().unwrap_or_default();
        self.worker.set_queue_concurrency(kind, concurrency);
        Ok(saved)
    }
}
