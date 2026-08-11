//! Interactive-game cover validation and uploads for the workbench's durable cover-generation dialog.

use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};

use super::DesktopService;

impl DesktopService {
    /// Save an external image as a game-owned cover reference before it is selected by a later durable cover task.
    pub fn upload_game_cover_reference(
        &self,
        game_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let name = optional_text(&values, "name", 200)?;
        let data_url = required_text(&values, "data_url", 20_000_000)?;
        let image_url = self.media.save_data_url(&data_url)?;
        self.repository.create_game_cover_reference(
            game_id,
            if name.is_empty() {
                "封面参考图"
            } else {
                &name
            },
            &image_url,
        )
    }

    /// Validate selected game materials and create the cover asset plus durable image task before any provider work starts.
    pub fn enqueue_game_cover(
        &self,
        game_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let game = self.repository.get_game(game_id)?;
        let name = required_text(&values, "name", 200)?;
        let prompt = optional_text(&values, "prompt", 10_000)?;
        let ratio = values
            .get("ratio")
            .and_then(Value::as_str)
            .unwrap_or_else(|| game_cover_ratio(&game));
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

        let assets = game["assets"].as_array().cloned().unwrap_or_default();
        let mut references = Vec::new();
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
                    .filter(|asset| asset["type"].as_str() == Some(kind))
                    .ok_or_else(|| {
                        AppError::BadRequest(format!("封面参考素材不存在或类型不匹配：{id}"))
                    })?;
                if asset["image_url"].as_str().is_none_or(str::is_empty) {
                    return Err(AppError::BadRequest(format!(
                        "请先生成或上传以下封面参考图：{}",
                        asset["name"].as_str().unwrap_or(id)
                    )));
                }
                references.push(id.clone());
            }
            metadata.insert(field.to_owned(), json!(ids));
        }
        metadata.insert("ratio".to_owned(), json!(ratio));
        metadata.insert("count".to_owned(), json!(count));
        metadata.insert("reference_asset_ids".to_owned(), json!(references));
        self.repository
            .enqueue_game_cover(game_id, &name, &prompt, Value::Object(metadata))
    }
}

fn game_cover_ratio(game: &Value) -> &'static str {
    if game["platform"].as_str() == Some("Steam游戏") {
        "16:9"
    } else {
        "9:16"
    }
}

fn required_text(values: &Map<String, Value>, key: &str, maximum: usize) -> AppResult<String> {
    let value = optional_text(values, key, maximum)?;
    if value.is_empty() {
        Err(AppError::BadRequest(format!("{key} 不能为空")))
    } else {
        Ok(value)
    }
}

fn optional_text(values: &Map<String, Value>, key: &str, maximum: usize) -> AppResult<String> {
    let value = values.get(key).map_or(Ok(""), |value| {
        value
            .as_str()
            .ok_or_else(|| AppError::BadRequest(format!("{key} 必须是字符串")))
    })?;
    let value = value.trim();
    if value.chars().count() > maximum {
        Err(AppError::BadRequest(format!(
            "{key} 不能超过 {maximum} 个字"
        )))
    } else {
        Ok(value.to_owned())
    }
}

fn unique_ids(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| !id.is_empty())
        .fold(Vec::new(), |mut ids, id| {
            if !ids.iter().any(|saved| saved == id) {
                ids.push(id.to_owned());
            }
            ids
        })
}
