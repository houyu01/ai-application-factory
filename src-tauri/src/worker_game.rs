//! Interactive-game task execution split from the drama worker to keep durable flows focused.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    planner, skills,
    value::{CANCELLED, FAILED, GENERATING, SUCCEEDED},
};

use super::{game_asset_prompt::game_asset_generation_prompt, DurableWorker};

const PREVIEW_WRITE_INTERVAL: Duration = Duration::from_millis(250);
const PREVIEW_WRITE_MIN_BYTES: usize = 96;
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

/// Read only a fully normalized graph plan, never the incomplete JSON preview shown while streaming.
fn graph_checkpoint(task: &Value) -> Option<Value> {
    let plan = task["input_snapshot"]["graph_checkpoint"].clone();
    let object = plan.as_object()?;
    ["assets", "nodes", "edges"]
        .iter()
        .all(|key| object.get(*key).is_some_and(Value::is_array))
        .then_some(plan)
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

    /// Convert the saved expansion into a validated DAG so later node-video work can progress independently.
    fn decompose_game_graph(&self, task_id: &str, game_id: &str) -> AppResult<()> {
        let game = self.repository.get_game(game_id)?;
        let checkpoint = graph_checkpoint(&self.repository.get_game_task(task_id)?);
        if let Some(plan) = checkpoint {
            self.repository.update_game_task_progress(
                task_id,
                82,
                "已读取保存的图谱骨架，正在写入视频节点",
            )?;
            return self.save_game_graph_checkpoint(task_id, game_id, &plan);
        }
        self.repository
            .update_game_task_progress(task_id, 8, "正在拆分多分支视频节点")?;
        let screenplay = game["expanded_script"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| game["script"].as_str().unwrap_or_default());
        let skill = skills::game_branch_skill(json!({
            "branch_min": game["branch_min"],
            "branch_max": game["branch_max"],
            "success_ending_count": game["success_ending_count"],
            "failure_ending_count": game["failure_ending_count"],
        }))?;
        let prompt = planner::game_graph_prompt(&game, screenplay);
        let enable_web_search = crate::value::bool_value(&game["enable_web_search"]);
        let mut preview = String::new();
        let mut saved_bytes = 0;
        let mut last_saved = Instant::now() - PREVIEW_WRITE_INTERVAL;
        let response = self
            .providers
            .complete_with_web_search_content_stream(
                "language",
                game["language_model"].as_str(),
                skill["instruction"].as_str().unwrap_or_default(),
                &prompt,
                enable_web_search,
                |delta| {
                    preview.push_str(delta);
                    let due = preview.len().saturating_sub(saved_bytes) >= PREVIEW_WRITE_MIN_BYTES
                        || last_saved.elapsed() >= PREVIEW_WRITE_INTERVAL;
                    if due {
                        self.persist_game_graph_preview(task_id, game_id, &preview)?;
                        saved_bytes = preview.len();
                        last_saved = Instant::now();
                    }
                    Ok(())
                },
            )?
            .filter(|response| !response.trim().is_empty());
        if let Some(response) = response.as_deref() {
            if preview != response {
                self.persist_game_graph_preview(task_id, game_id, response)?;
            }
        }
        let response = response.ok_or_else(|| {
            AppError::External("语言模型未返回游戏图谱，请检查模型配置后重试。".to_owned())
        })?;
        self.repository.update_game_task_progress(
            task_id,
            78,
            "正在复核剧本与人物、场景、道具的对应关系",
        )?;
        let plan = planner::model_game_plan(&response, &game)
            .ok_or_else(|| AppError::External(GAME_GRAPH_VALIDATION_ERROR.to_owned()))?;
        self.repository
            .persist_game_graph_checkpoint(task_id, game_id, &plan)?;
        self.save_game_graph_checkpoint(task_id, game_id, &plan)
    }

    /// Write a previously checkpointed, normalized graph to SQLite and then finish its durable task.
    fn save_game_graph_checkpoint(
        &self,
        task_id: &str,
        game_id: &str,
        plan: &Value,
    ) -> AppResult<()> {
        self.repository.save_generated_game_graph(
            task_id,
            game_id,
            plan["assets"].as_array().unwrap_or(&Vec::new()),
            plan["nodes"].as_array().unwrap_or(&Vec::new()),
            plan["edges"].as_array().unwrap_or(&Vec::new()),
        )?;
        self.repository
            .finish_game_task(task_id, SUCCEEDED, Some(plan.clone()), None)?;
        Ok(())
    }

    /// Write streamed screenplay text to the game and task snapshot before the next provider delta arrives.
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

    /// Store streamed graph JSON and recognisable node/choice counts so the editor can render a live skeleton before validation completes.
    fn persist_game_graph_preview(
        &self,
        task_id: &str,
        game_id: &str,
        preview: &str,
    ) -> AppResult<()> {
        let received_chars = preview.chars().count();
        let node_count = preview.matches("\"node_type\"").count();
        let edge_count = preview.matches("\"source_node_id\"").count();
        let progress = (65 + (received_chars / 80).min(25) as i64).min(90);
        self.repository.update_game_task_snapshot(
            task_id,
            json!({
                "game_id":game_id,
                "graph_preview":preview,
                "preview_received_chars":received_chars,
                "preview_node_count":node_count,
                "preview_edge_count":edge_count,
            }),
        )?;
        self.repository.update_game_task_progress(
            task_id,
            progress,
            &format!("正在拆分视频节点骨架（已接收 {received_chars} 字，{node_count} 个节点，{edge_count} 条选择边）"),
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
