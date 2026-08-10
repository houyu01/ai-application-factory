//! Video-generation preflight rules shared by the durable local desktop enqueue flow.

use serde_json::Value;

use crate::{
    error::{AppError, AppResult},
    planner,
    service::DesktopService,
    value::{GENERATING, SUCCEEDED},
};

impl DesktopService {
    /// Reject a video task before it exists when its prompt or referenced project media cannot be sent to a model.
    pub fn validate_video_preflight(&self, project: &Value, shot: &Value) -> AppResult<()> {
        let mut issues = Vec::new();
        if shot["prompt"]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        {
            issues.push("最近更新过提示词或参考图，请确认后点击保存再生成视频".to_owned());
        }
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        for (id, variant_id) in reference_ids(shot) {
            let Some(asset) = planner::resolve_reference_asset(&assets, &id, variant_id.as_deref())
            else {
                issues.push(format!("{id}（素材不存在）"));
                continue;
            };
            let name = asset["name"].as_str().unwrap_or(&id);
            match asset["status"].as_str() {
                Some(GENERATING) => issues.push(format!("{name}（图片仍在生成）")),
                Some("生成失败") => issues.push(format!("{name}（图片生成失败）")),
                Some(SUCCEEDED)
                    if asset["image_url"]
                        .as_str()
                        .is_some_and(|url| !url.is_empty()) =>
                {
                    if asset["type"].as_str() == Some("placeholder")
                        && asset["metadata"]["render_mode"].as_str() != Some("generated_composite")
                    {
                        issues.push(format!("{name}（占位图尚未生成合成图）"));
                    } else if self
                        .media
                        .provider_reference_url(asset["image_url"].as_str().unwrap_or_default())
                        .is_none()
                    {
                        issues.push(format!("{name}（本地图片无法调用大模型）"));
                    }
                }
                _ => issues.push(format!("{name}（图片未生成或未上传）")),
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(AppError::BadRequest(issues.join("；")))
        }
    }
}

fn reference_ids(shot: &Value) -> Vec<(String, Option<String>)> {
    let mut ids = shot["prompt_rich"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|node| node["type"] == "reference")
        .filter_map(|node| {
            node["asset_id"].as_str().map(|id| {
                (
                    id.to_owned(),
                    node["variant_id"].as_str().map(str::to_owned),
                )
            })
        })
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.extend(
            shot["reference_asset_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| (id.to_owned(), None)),
        );
    }
    ids.retain(|(id, _)| !id.is_empty());
    ids.dedup();
    ids
}
