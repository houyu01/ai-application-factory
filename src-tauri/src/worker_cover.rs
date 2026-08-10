//! Restart-safe cover worker that retains every requested output in image history.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::SUCCEEDED,
};

use super::DurableWorker;

impl DurableWorker {
    pub(super) fn cover_image(
        &self,
        task_id: &str,
        project_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        let cover_id = task["input_snapshot"]["cover_asset_id"]
            .as_str()
            .or_else(|| task["resource_id"].as_str())
            .unwrap_or_default();
        let project = self.repository.get_drama(project_id)?;
        let cover = self.repository.get_asset(project_id, cover_id)?;
        if cover["type"].as_str() != Some("cover") {
            return Err(AppError::NotFound(format!(
                "Cover asset not found: {cover_id}"
            )));
        }
        let metadata = &cover["metadata"];
        let count = metadata["count"].as_i64().unwrap_or(1).clamp(1, 8) as usize;
        let ids = metadata["reference_asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let mut references = Vec::new();
        let mut reference_names = Vec::new();
        for id in &ids {
            let id = id.as_str().unwrap_or_default();
            let asset = assets
                .iter()
                .find(|asset| asset["id"].as_str() == Some(id))
                .ok_or_else(|| {
                    AppError::BadRequest("封面引用的角色、场景或上传参考图已经缺失".to_owned())
                })?;
            let url = asset["image_url"]
                .as_str()
                .and_then(|url| self.media.provider_reference_url(url))
                .ok_or_else(|| {
                    AppError::BadRequest("封面引用的角色、场景或上传参考图已经缺失".to_owned())
                })?;
            references.push(url);
            reference_names.push(format!(
                "{}：{}",
                asset["type"].as_str().unwrap_or(""),
                asset["name"].as_str().unwrap_or("")
            ));
        }
        let ratio = metadata["ratio"]
            .as_str()
            .unwrap_or_else(|| project["ratio"].as_str().unwrap_or("9:16"));
        let user_prompt = cover["prompt"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("突出核心人物与故事冲突，构图清晰，具有短剧海报传播力。");
        let prompt = format!("为短剧《{}》生成一张 {ratio} 封面海报。\n整体风格：{}；背景主题：{}。\n参考素材：{}。必须保持参考人物脸部、服装与场景特征一致。\n用户补充要求：{user_prompt}\n画面完整、主体突出、视觉层级清晰，不生成水印、Logo、错误肢体或无关文字。",cover["name"].as_str().unwrap_or_else(||project["name"].as_str().unwrap_or("")),project["style"].as_str().unwrap_or("真人风格"),project["theme"].as_str().unwrap_or("都市"),if reference_names.is_empty(){"无额外参考图".to_owned()}else{reference_names.join("、")});
        let mut urls = cover["image_history"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry["url"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        while urls.len() < count {
            let url = self.providers.image(
                &prompt,
                ratio,
                &references,
                project["multimodal_model"].as_str(),
            )?;
            self.repository
                .set_asset_image(project_id, cover_id, &url, "generated", SUCCEEDED)?;
            urls.push(url);
        }
        self.repository
            .set_asset_status(project_id, cover_id, SUCCEEDED)?;
        self.repository.finish_drama_task(
            task_id,
            SUCCEEDED,
            Some(json!({"cover_asset_id":cover_id,"image_urls":urls})),
            None,
        )?;
        Ok(())
    }
}
