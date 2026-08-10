//! Cover enqueue validation for the cover dialog's typed reference groups and output count.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::GENERATING,
};

use super::DesktopService;

impl DesktopService {
    /// Create a durable cover asset only after each selected reference exists, has the intended type, and has an image.
    pub fn enqueue_cover(&self, project_id: &str, values: Map<String, Value>) -> AppResult<Value> {
        let project = self.repository.get_drama(project_id)?;
        let name = required_text(&values, "name", 200)?;
        let prompt = optional_text(&values, "prompt", 10_000)?;
        let ratio = values
            .get("ratio")
            .and_then(Value::as_str)
            .unwrap_or("9:16");
        if !["9:16", "16:9", "1:1", "3:4", "4:3"].contains(&ratio) {
            return Err(AppError::BadRequest("不支持的封面比例".to_owned()));
        }
        let count = values.get("count").map_or(Ok(1), |value| {
            value
                .as_i64()
                .ok_or_else(|| AppError::BadRequest("封面数量必须是整数".to_owned()))
        })?;
        if !(1..=8).contains(&count) {
            return Err(AppError::BadRequest(
                "封面数量必须在 1 到 8 之间".to_owned(),
            ));
        }
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let mut reference_ids = Vec::new();
        let mut metadata = Map::new();
        for (field, kind) in [
            ("character_asset_ids", "character"),
            ("scene_asset_ids", "scene"),
            ("extra_reference_asset_ids", "cover_reference"),
        ] {
            let ids = unique_ids(values.get(field));
            for id in &ids {
                let asset = assets
                    .iter()
                    .find(|asset| asset["id"].as_str() == Some(id))
                    .ok_or_else(|| {
                        AppError::BadRequest(format!("封面参考素材不存在或类型不匹配：{id}"))
                    })?;
                if asset["type"].as_str() != Some(kind) {
                    return Err(AppError::BadRequest(format!(
                        "封面参考素材不存在或类型不匹配：{id}"
                    )));
                }
                if asset["image_url"].as_str().is_none_or(str::is_empty) {
                    return Err(AppError::BadRequest(format!(
                        "请先生成或上传以下封面参考图：{}",
                        asset["name"].as_str().unwrap_or(id)
                    )));
                }
                reference_ids.push(id.clone());
            }
            metadata.insert(field.to_owned(), json!(ids));
        }
        metadata.insert("ratio".to_owned(), json!(ratio));
        metadata.insert("count".to_owned(), json!(count));
        metadata.insert("reference_asset_ids".to_owned(), json!(reference_ids));
        let asset = self.repository.create_asset(
            project_id,
            Map::from_iter([
                ("type".to_owned(), json!("cover")),
                ("name".to_owned(), json!(name)),
                ("prompt".to_owned(), json!(prompt)),
                ("metadata".to_owned(), Value::Object(metadata.clone())),
            ]),
        )?;
        let id = asset["id"].as_str().unwrap_or_default();
        self.repository
            .set_asset_status(project_id, id, GENERATING)?;
        let task = self.repository.create_active_drama_task(project_id,"cover_image",Some(id),json!({"project_id":project_id,"cover_asset_id":id, "ratio":ratio,"count":count,"reference_asset_ids":metadata["reference_asset_ids"]}))?;
        Ok(json!({"cover":self.repository.get_asset(project_id,id)?,"task":task}))
    }
}

fn required_text(values: &Map<String, Value>, key: &str, maximum: usize) -> AppResult<String> {
    let text = values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| AppError::BadRequest(format!("{key} 不能为空")))?;
    if text.chars().count() > maximum {
        return Err(AppError::BadRequest(format!(
            "{key} 不能超过 {maximum} 个字"
        )));
    }
    Ok(text.to_owned())
}

fn optional_text(values: &Map<String, Value>, key: &str, maximum: usize) -> AppResult<String> {
    let text = values.get(key).map_or(Ok(""), |value| {
        value
            .as_str()
            .ok_or_else(|| AppError::BadRequest(format!("{key} 必须是字符串")))
    })?;
    if text.chars().count() > maximum {
        return Err(AppError::BadRequest(format!(
            "{key} 不能超过 {maximum} 个字"
        )));
    }
    Ok(text.to_owned())
}

fn unique_ids(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| !id.is_empty())
        .fold(Vec::new(), |mut all, id| {
            if !all.iter().any(|saved| saved == id) {
                all.push(id.to_owned());
            }
            all
        })
}
