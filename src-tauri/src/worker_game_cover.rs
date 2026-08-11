//! Provider execution for restart-safe interactive-game cover tasks with multiple retained image outputs.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::SUCCEEDED,
};

use super::DurableWorker;

impl DurableWorker {
    /// Generate every requested cover variation from the persisted prompt and references, retaining partial history for retries.
    pub(super) fn game_cover_image(
        &self,
        task_id: &str,
        game_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        let cover_id = task["input_snapshot"]["cover_asset_id"]
            .as_str()
            .or_else(|| task["resource_id"].as_str())
            .unwrap_or_default();
        self.repository
            .set_game_asset_image_status(game_id, cover_id, None, "生成中")?;
        self.repository
            .update_game_task_progress(task_id, 10, "正在生成游戏封面")?;
        let game = self.repository.get_game(game_id)?;
        let cover = self.repository.get_game_asset(game_id, cover_id)?;
        if cover["type"].as_str() != Some("cover") {
            return Err(AppError::NotFound(format!(
                "Game cover asset not found: {cover_id}"
            )));
        }
        let metadata = &cover["metadata"];
        let count = metadata["count"].as_i64().unwrap_or(1).clamp(1, 8) as usize;
        let ratio = metadata["ratio"]
            .as_str()
            .unwrap_or_else(|| game_cover_ratio(&game));
        let assets = game["assets"].as_array().cloned().unwrap_or_default();
        let (references, reference_names) = cover_references(self, &assets, metadata)?;
        let prompt = game_cover_prompt(&game, &cover, ratio, &reference_names);
        let mut urls = cover["image_history"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry["url"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        while urls.len() < count {
            let number = urls.len() + 1;
            self.repository.update_game_task_progress(
                task_id,
                15 + (70 * (number - 1) / count.max(1)) as i64,
                &format!("正在生成第 {number}/{count} 张游戏封面"),
            )?;
            let url = self.providers.image(
                &prompt,
                ratio,
                &references,
                game["multimodal_model"].as_str(),
            )?;
            self.repository
                .finish_game_asset_image(game_id, cover_id, task_id, &url)?;
            urls.push(url);
        }
        self.repository
            .set_game_asset_image_status(game_id, cover_id, None, SUCCEEDED)?;
        self.repository.finish_game_task(
            task_id,
            SUCCEEDED,
            Some(json!({"cover_asset_id":cover_id,"image_urls":urls,"prompt":prompt})),
            None,
        )?;
        Ok(())
    }
}

fn cover_references(
    worker: &DurableWorker,
    assets: &[Value],
    metadata: &Value,
) -> AppResult<(Vec<String>, Vec<String>)> {
    let mut urls = Vec::new();
    let mut names = Vec::new();
    for id in metadata["reference_asset_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let asset = assets
            .iter()
            .find(|asset| asset["id"].as_str() == Some(id))
            .ok_or_else(|| AppError::BadRequest("封面引用的参考图已经缺失".to_owned()))?;
        let url = asset["image_url"]
            .as_str()
            .and_then(|url| worker.media.provider_reference_url(url))
            .ok_or_else(|| AppError::BadRequest("封面引用的参考图不可用".to_owned()))?;
        urls.push(url);
        names.push(format!(
            "{}：{}",
            asset["type"].as_str().unwrap_or("素材"),
            asset["name"].as_str().unwrap_or("未命名")
        ));
    }
    Ok((urls, names))
}

fn game_cover_prompt(game: &Value, cover: &Value, ratio: &str, references: &[String]) -> String {
    let user_prompt = cover["prompt"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("突出主角、核心场景与玩家抉择带来的冲突感，构图清晰并具有游戏封面传播力。");
    format!(
        "为互动视频游戏《{}》生成一张 {ratio} 封面海报。\n整体风格：{}。\n故事背景：{}。\n参考素材：{}。必须保持参考人物脸部、服装与场景特征一致。\n用户补充要求：{user_prompt}\n画面完整、主体突出、视觉层级清晰，体现互动选择与故事冲突；不生成水印、Logo、错误肢体、界面元素或无关文字。",
        cover["name"].as_str().unwrap_or_else(|| game["name"].as_str().unwrap_or("互动游戏")),
        game["style"].as_str().unwrap_or("真人风格"),
        game["expanded_script"].as_str().filter(|value| !value.trim().is_empty()).unwrap_or_else(|| game["script"].as_str().unwrap_or("互动叙事")),
        if references.is_empty() { "无额外参考图".to_owned() } else { references.join("、") },
    )
}

fn game_cover_ratio(game: &Value) -> &'static str {
    if game["platform"].as_str() == Some("Steam游戏") {
        "16:9"
    } else {
        "9:16"
    }
}
