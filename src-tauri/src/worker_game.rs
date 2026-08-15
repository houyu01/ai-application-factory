//! Interactive-game task execution split from the drama worker to keep durable flows focused.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    planner,
    value::{CANCELLED, FAILED, GENERATING, SUCCEEDED},
};

use super::{game_asset_prompt::game_asset_generation_prompt, DurableWorker};

const PREVIEW_WRITE_INTERVAL: Duration = Duration::from_millis(1_500);
const PREVIEW_WRITE_MIN_BYTES: usize = 768;
pub(super) const GAME_GRAPH_VALIDATION_ERROR: &str = "语言模型返回的游戏图谱不符合节点、分支、结局或节点文案与提示词唯一性约束；未写入任何兜底节点，请重试。";

fn join_game_screenplay(existing: &str, addition: &str) -> String {
    if existing.trim().is_empty() {
        addition.trim().to_owned()
    } else if addition.trim().is_empty() {
        existing.trim().to_owned()
    } else {
        format!("{}\n\n{}", existing.trim(), addition.trim())
    }
}

impl DurableWorker {
    pub(super) fn run_game(&self, task: Value) {
        let id = task["id"].as_str().unwrap_or_default();
        let game_id = task["game_id"].as_str().unwrap_or_default();
        let _lease = super::lease::GameTaskLease::start(self.repository.clone(), &task);
        let result = match task["type"].as_str().unwrap_or_default() {
            "game_script_expansion" => self.expand_game_screenplay(id, game_id),
            "game_graph_decomposition" => self.decompose_game_graph(id, game_id),
            "game_node_prompt" => self.game_node_prompt(id, game_id, &task),
            "node_video_generation" => self.game_video(id, game_id, &task),
            "game_asset_image" => self.game_asset_image(
                id,
                game_id,
                task["resource_id"].as_str().unwrap_or_default(),
            ),
            "game_asset_variant_image" => self.game_asset_variant_image(id, game_id, &task),
            "game_cover_image" => self.game_cover_image(id, game_id, &task),
            "game_placeholder_image" => self.game_placeholder_image(id, game_id, &task),
            other => Err(crate::error::AppError::BadRequest(format!(
                "未知的游戏任务类型：{other}"
            ))),
        };
        if let Err(error) = result {
            if self.retry_game_graph_validation_error(&task, id, &error) {
                return;
            }
            let terminal =
                self.repository
                    .finish_game_task(id, FAILED, None, Some(&error.to_string()));
            if ["game_script_expansion", "game_graph_decomposition"]
                .contains(&task["type"].as_str().unwrap_or_default())
                && terminal
                    .as_ref()
                    .ok()
                    .and_then(|value| value["status"].as_str())
                    != Some(CANCELLED)
            {
                let _ = self.repository.set_game_status(game_id, FAILED);
            }
            self.reflect_game_material_failure(&task, game_id, &error.to_string());
        }
    }

    /// Expand the creator's premise before the graph planner assigns individual playable video nodes.
    fn expand_game_screenplay(&self, task_id: &str, game_id: &str) -> AppResult<()> {
        let game = self.repository.get_game(game_id)?;
        let queue_graph = game["nodes"]
            .as_array()
            .map_or(true, |nodes| nodes.is_empty());
        let existing = game["expanded_script"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let continuing = !existing.is_empty();
        self.repository.update_game_task_progress(
            task_id,
            8,
            if continuing {
                "正在继续扩写互动游戏剧本"
            } else {
                "正在扩写互动游戏剧本"
            },
        )?;
        let prompt = planner::game_expansion_prompt(&game);
        let enable_web_search = crate::value::bool_value(&game["enable_web_search"]);
        let target_chars = game[if continuing {
            "expanded_script_max_chars"
        } else {
            "expanded_script_min_chars"
        }]
        .as_i64()
        .unwrap_or(5_000)
        .max(1) as usize;
        let mut addition = String::new();
        let mut preview = existing.clone();
        let mut saved_bytes = 0;
        let mut last_saved = Instant::now() - PREVIEW_WRITE_INTERVAL;
        let response = self
            .providers
            .complete_with_web_search_content_stream(
                "language",
                game["language_model"].as_str(),
                "你是互动视频游戏编剧。先确定或补全唯一、有真实姓名的主人公；绝不能让整部游戏没有主人公。输出具体、可视化、可拆分为视频节点的叙事正文。",
                &prompt,
                enable_web_search,
                |delta| {
                    addition.push_str(delta);
                    preview = join_game_screenplay(&existing, &addition);
                    let due = preview.len().saturating_sub(saved_bytes) >= PREVIEW_WRITE_MIN_BYTES
                        || last_saved.elapsed() >= PREVIEW_WRITE_INTERVAL;
                    if due {
                        self.persist_game_screenplay_preview(
                            task_id,
                            game_id,
                            &preview,
                            target_chars,
                        )?;
                        saved_bytes = preview.len();
                        last_saved = Instant::now();
                    }
                    Ok(())
                },
            )?
            .filter(|text| !text.trim().is_empty());
        let response = response.ok_or_else(|| {
            AppError::External("语言模型未返回互动游戏剧本，请检查模型配置后重试。".to_owned())
        })?;
        let expanded = join_game_screenplay(&existing, &response);
        if preview != expanded {
            self.persist_game_screenplay_preview(task_id, game_id, &expanded, target_chars)?;
        }
        self.repository.complete_game_screenplay_expansion(
            task_id,
            game_id,
            &expanded,
            game["script"].as_str().unwrap_or_default().chars().count(),
            queue_graph,
        )?;
        Ok(())
    }

    /// Write throttled screenplay text to the game and task snapshot while generation is visible.
    fn persist_game_screenplay_preview(
        &self,
        task_id: &str,
        game_id: &str,
        screenplay: &str,
        target_chars: usize,
    ) -> AppResult<()> {
        let received_chars = screenplay.chars().count();
        let progress = 8 + (52 * received_chars.min(target_chars) / target_chars.max(1)) as i64;
        self.repository.persist_game_screenplay_preview(
            task_id,
            game_id,
            screenplay,
            progress,
            &format!("正在扩写互动游戏剧本（已接收 {received_chars} 字）"),
        )
    }

    /// Generate one reusable material image from the saved global prompt and material prompt, then retain it in history.
    fn game_asset_image(&self, task_id: &str, game_id: &str, asset_id: &str) -> AppResult<()> {
        self.repository
            .set_game_asset_image_status(game_id, asset_id, None, GENERATING)?;
        self.repository
            .update_game_task_progress(task_id, 12, "正在生成素材图片")?;
        let game = self.repository.get_game(game_id)?;
        let asset = self.repository.get_game_asset(game_id, asset_id)?;
        let prompt = game_asset_generation_prompt(&game, &asset, None);
        let url = self.providers.image(
            &prompt,
            game_image_ratio(&game),
            &[],
            game["multimodal_model"].as_str(),
        )?;
        self.repository
            .finish_game_asset_image(game_id, asset_id, task_id, &url)?;
        self.repository.finish_game_task(
            task_id,
            SUCCEEDED,
            Some(json!({"asset_id":asset_id,"url":url,"prompt":prompt})),
            None,
        )?;
        Ok(())
    }

    /// Generate one alternate form while keeping the base-material image and its image history untouched.
    fn game_asset_variant_image(
        &self,
        task_id: &str,
        game_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        let snapshot = &task["input_snapshot"];
        let asset_id = snapshot["asset_id"].as_str().unwrap_or_default();
        let variant_id = snapshot["variant_id"]
            .as_str()
            .or_else(|| task["resource_id"].as_str())
            .unwrap_or_default();
        self.repository.set_game_asset_image_status(
            game_id,
            asset_id,
            Some(variant_id),
            GENERATING,
        )?;
        self.repository
            .update_game_task_progress(task_id, 12, "正在生成素材形态图片")?;
        let game = self.repository.get_game(game_id)?;
        let asset = self.repository.get_game_asset(game_id, asset_id)?;
        let variant = asset["variants"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["id"].as_str() == Some(variant_id))
            })
            .cloned()
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!(
                    "Game asset variant not found: {variant_id}"
                ))
            })?;
        let prompt = game_asset_generation_prompt(&game, &asset, Some(&variant));
        let url = self.providers.image(
            &prompt,
            game_image_ratio(&game),
            &[],
            game["multimodal_model"].as_str(),
        )?;
        self.repository
            .finish_game_asset_variant_image(game_id, asset_id, variant_id, task_id, &url)?;
        self.repository.finish_game_task(
            task_id,
            SUCCEEDED,
            Some(json!({"asset_id":asset_id,"variant_id":variant_id,"url":url,"prompt":prompt})),
            None,
        )?;
        Ok(())
    }

    /// Keep a provider failure scoped to its material card so a failed picture cannot mark the whole game as failed.
    fn reflect_game_material_failure(&self, task: &Value, game_id: &str, error: &str) {
        let kind = task["type"].as_str().unwrap_or_default();
        let snapshot = &task["input_snapshot"];
        let asset_id = snapshot["asset_id"]
            .as_str()
            .unwrap_or_else(|| task["resource_id"].as_str().unwrap_or_default());
        let variant_id = if kind == "game_asset_variant_image" {
            snapshot["variant_id"]
                .as_str()
                .or_else(|| task["resource_id"].as_str())
        } else {
            None
        };
        if matches!(
            kind,
            "game_asset_image"
                | "game_asset_variant_image"
                | "game_cover_image"
                | "game_placeholder_image"
        ) {
            let _ = self
                .repository
                .set_game_asset_image_status(game_id, asset_id, variant_id, FAILED);
        }
        if kind == "node_video_generation" {
            let _ = self.repository.finish_game_node_video(
                game_id,
                task["resource_id"].as_str().unwrap_or_default(),
                task["id"].as_str().unwrap_or_default(),
                None,
                FAILED,
                Some(error),
            );
        }
    }
}

fn game_image_ratio(game: &Value) -> &'static str {
    if game["platform"].as_str() == Some("Steam游戏") {
        "16:9"
    } else {
        "9:16"
    }
}
