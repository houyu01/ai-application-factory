//! Shot-reference image enqueueing, including independent character-form images.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    planner,
};

use super::DesktopService;

impl DesktopService {
    /// Queue only the missing base assets or exact forms selected by a shot's rich references.
    pub fn enqueue_reference_images(&self, project: &str, shot: &str) -> AppResult<Value> {
        let detail = self.repository.get_drama(project)?;
        let shot_value = self.repository.get_shot(project, shot)?;
        let assets = detail["assets"].as_array().cloned().unwrap_or_default();
        let mut requests = shot_value["prompt_rich"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|node| node["type"] == "reference")
            .map(|node| {
                (
                    node["asset_id"].as_str().unwrap_or_default().to_owned(),
                    node["variant_id"].as_str().map(str::to_owned),
                )
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            requests.extend(
                shot_value["reference_asset_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|id| (id.to_owned(), None)),
            );
        }
        let mut jobs = Vec::new();
        for (asset_id, variant_id) in requests {
            let Some(asset) =
                planner::resolve_reference_asset(&assets, &asset_id, variant_id.as_deref())
            else {
                continue;
            };
            if !["character", "scene", "prop"].contains(&asset["type"].as_str().unwrap_or(""))
                || asset["status"].as_str() == Some("生成成功")
                    && asset["image_url"]
                        .as_str()
                        .is_some_and(|url| !url.is_empty())
            {
                continue;
            }
            let job = match variant_id {
                Some(variant_id) => {
                    json!({"type":"asset_variant_image","asset_id":asset_id,"variant_id":variant_id})
                }
                None => json!({"type":"asset_image","asset_id":asset_id}),
            };
            if !jobs.iter().any(|current: &Value| current == &job) {
                jobs.push(job);
            }
        }
        if jobs.is_empty() {
            return Err(AppError::BadRequest(
                "当前已选参考图均已生成，或没有可一键生成的素材".to_owned(),
            ));
        }
        self.repository.create_active_drama_task(project, "shot_reference_image_batch", Some(shot), json!({"project_id":project,"shot_id":shot,"jobs":jobs,"batch_size":5,"next_index":0,"active_task_ids":[],"completed_count":0,"failed_count":0,"cancelled_count":0,"type":"shot_reference_image_batch"}))
    }
}
