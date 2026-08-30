//! Durable graph-decomposition flow with per-record checkpoints for interactive games.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    planner, skills,
    value::SUCCEEDED,
};

use super::{game::GAME_GRAPH_VALIDATION_ERROR, DurableWorker};

const PREVIEW_WRITE_INTERVAL: Duration = Duration::from_millis(1_500);
const PREVIEW_WRITE_MIN_BYTES: usize = 768;

fn graph_preview_write_due(received_bytes: usize, saved_bytes: usize, elapsed: Duration) -> bool {
    received_bytes.saturating_sub(saved_bytes) >= PREVIEW_WRITE_MIN_BYTES
        || elapsed >= PREVIEW_WRITE_INTERVAL
}

/// Read only a fully normalized graph plan, never the incremental record checkpoint.
fn completed_graph_checkpoint(task: &Value) -> Option<Value> {
    let plan = task["input_snapshot"]["graph_checkpoint"].clone();
    let object = plan.as_object()?;
    ["assets", "nodes", "edges"]
        .iter()
        .all(|key| object.get(*key).is_some_and(Value::is_array))
        .then_some(plan)
}

fn progress_graph_checkpoint(task: &Value) -> Value {
    planner::game_graph_progress_checkpoint(
        "",
        Some(&task["input_snapshot"]["graph_progress_checkpoint"]),
    )
}

/// Keep a graph retry visibly in its repair phase when earlier records survived validation.
fn graph_generation_start(checkpoint: &Value, stage: planner::GameGraphStage) -> (i64, String) {
    let count = |key| checkpoint[key].as_array().map_or(0, Vec::len);
    let assets = count("assets");
    let nodes = count("nodes");
    let edges = count("edges");
    match stage {
        planner::GameGraphStage::Assets => (8, "正在生成角色、场景和道具目录".to_owned()),
        planner::GameGraphStage::Nodes => (
            42,
            format!("正在生成视频节点骨架（已保存 {assets} 个素材、{nodes} 个节点）"),
        ),
        planner::GameGraphStage::Edges => (
            68,
            format!("正在生成玩家选择边（已保存 {nodes} 个节点、{edges} 条选择边）"),
        ),
    }
}

/// Map the current model call onto a stage band so a later batch cannot rewind the meter to 90%→42%.
fn graph_stream_progress(
    stage: planner::GameGraphStage,
    checkpoint: &Value,
    received_chars: usize,
) -> i64 {
    let saved = |key| checkpoint[key].as_array().map_or(0, Vec::len) as i64;
    let stream = |divisor: i64, cap: i64| (received_chars as i64 / divisor).min(cap);
    match stage {
        planner::GameGraphStage::Assets => (8 + stream(40, 32)).min(40),
        planner::GameGraphStage::Nodes => (42 + saved("nodes").min(20) + stream(200, 4)).min(66),
        planner::GameGraphStage::Edges => (68 + stream(80, 22)).min(90),
    }
}

impl DurableWorker {
    /// Convert a structured expansion into a playable DAG without asking the model to invent topology.
    ///
    /// Screenplay Sxx/Exx/前往 links are compiled first. Missing optional branches are omitted so
    /// the creator can add them later, but the worker never persists a DAG that cannot be played.
    pub(super) fn decompose_game_graph(&self, task_id: &str, game_id: &str) -> AppResult<()> {
        let game = self.repository.get_game(game_id)?;
        let task = self.repository.get_game_task(task_id)?;
        if let Some(plan) = completed_graph_checkpoint(&task) {
            self.repository.advance_game_task_progress(
                task_id,
                92,
                "已读取保存的图谱骨架，正在写入视频节点",
            )?;
            return self.save_game_graph_checkpoint(task_id, game_id, &plan);
        }
        let screenplay = game["expanded_script"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| game["script"].as_str().unwrap_or_default());
        if let Some(plan) = planner::compile_game_plan(screenplay, &game) {
            let node_count = plan["nodes"].as_array().map_or(0, Vec::len);
            let edge_count = plan["edges"].as_array().map_or(0, Vec::len);
            self.persist_game_graph_preview(
                task_id,
                game_id,
                &plan.to_string(),
                &plan,
                planner::GameGraphStage::Edges,
                None,
                0,
            )?;
            self.repository.advance_game_task_progress(
                task_id,
                92,
                &format!("已根据剧本编译可玩图谱（{node_count} 个节点、{edge_count} 条选择边），正在写入视频节点"),
            )?;
            self.repository
                .persist_game_graph_checkpoint(task_id, game_id, &plan)?;
            return self.save_game_graph_checkpoint(task_id, game_id, &plan);
        }
        let mut progress_checkpoint = progress_graph_checkpoint(&task);
        let initial_checkpoint = progress_checkpoint.clone();
        let mut validation_retry_count = task["input_snapshot"]["graph_validation_retry_count"]
            .as_i64()
            .unwrap_or(0);
        let stage = planner::game_graph_stage(&progress_checkpoint, &game);
        let (start_progress, start_stage) = graph_generation_start(&progress_checkpoint, stage);
        self.repository
            .advance_game_task_progress(task_id, start_progress, &start_stage)?;
        let skill = skills::game_branch_skill(json!({
            "branch_min": game["branch_min"],
            "branch_max": game["branch_max"],
            "success_ending_count": game["success_ending_count"],
            "failure_ending_count": game["failure_ending_count"],
        }))?;
        let prompt = planner::game_graph_stage_prompt(
            stage,
            &game,
            screenplay,
            &progress_checkpoint,
            task["input_snapshot"]["graph_validation_feedback"].as_str(),
        );
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
                    if graph_preview_write_due(preview.len(), saved_bytes, last_saved.elapsed()) {
                        let checkpoint = planner::game_graph_stage_checkpoint(
                            stage,
                            &preview,
                            &progress_checkpoint,
                        );
                        if checkpoint != progress_checkpoint {
                            validation_retry_count = 0;
                        }
                        progress_checkpoint = checkpoint;
                        self.persist_game_graph_preview(
                            task_id,
                            game_id,
                            &preview,
                            &progress_checkpoint,
                            stage,
                            None,
                            validation_retry_count,
                        )?;
                        saved_bytes = preview.len();
                        last_saved = Instant::now();
                    }
                    Ok(())
                },
            )?
            .filter(|response| !response.trim().is_empty());
        if let Some(response) = response.as_deref() {
            let checkpoint =
                planner::game_graph_stage_checkpoint(stage, response, &progress_checkpoint);
            if preview != response || checkpoint != progress_checkpoint {
                progress_checkpoint = checkpoint;
                self.persist_game_graph_preview(
                    task_id,
                    game_id,
                    response,
                    &progress_checkpoint,
                    stage,
                    None,
                    validation_retry_count,
                )?;
            }
        }
        let response = response.ok_or_else(|| {
            AppError::External("语言模型未返回游戏图谱，请检查模型配置后重试。".to_owned())
        })?;
        let checkpoint = match planner::merge_game_graph_stage_response(
            stage,
            &response,
            &progress_checkpoint,
        ) {
            Some(checkpoint) => checkpoint,
            None if progress_checkpoint != initial_checkpoint => {
                let recovered = planner::game_graph_stage_response(stage, &progress_checkpoint);
                let count = progress_checkpoint[stage.key()]
                    .as_array()
                    .map_or(0, Vec::len);
                let feedback = format!(
                    "上一次{} JSON 在一条记录中间截断；已裁剪并保存 {count} 条完整记录。只补缺失记录，并返回完整闭合 JSON。",
                    stage.label()
                );
                self.persist_game_graph_preview(
                    task_id,
                    game_id,
                    &recovered,
                    &progress_checkpoint,
                    stage,
                    Some(&feedback),
                    validation_retry_count,
                )?;
                self.repository.reschedule_game_task(
                    task_id,
                    1,
                    &format!(
                        "模型响应已截断，已保存 {count} 条完整{}，正在续生成",
                        stage.label()
                    ),
                    None,
                )?;
                return Ok(());
            }
            None => return Err(AppError::External(GAME_GRAPH_VALIDATION_ERROR.to_owned())),
        };
        if stage != planner::GameGraphStage::Edges {
            let next_stage = planner::game_graph_stage(&checkpoint, &game);
            if checkpoint == progress_checkpoint && next_stage == stage {
                return Err(AppError::External(GAME_GRAPH_VALIDATION_ERROR.to_owned()));
            }
            self.persist_game_graph_preview(
                task_id,
                game_id,
                &response,
                &checkpoint,
                next_stage,
                None,
                validation_retry_count,
            )?;
            let (_, stage_label) = graph_generation_start(&checkpoint, next_stage);
            return self.repository.reschedule_game_task(
                task_id,
                1,
                &format!("已保存本批图谱记录，{stage_label}"),
                None,
            );
        }
        self.repository
            .advance_game_task_progress(task_id, 91, "正在复核选择边与完整 DAG 约束")?;
        let Some(plan) = planner::model_game_plan(&checkpoint.to_string(), &game) else {
            let feedback =
                planner::game_graph_edge_feedback(&progress_checkpoint, &response, &game);
            self.persist_game_graph_preview(
                task_id,
                game_id,
                &response,
                &progress_checkpoint,
                stage,
                Some(&feedback),
                validation_retry_count,
            )?;
            return Err(AppError::External(GAME_GRAPH_VALIDATION_ERROR.to_owned()));
        };
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
            plan["assets"].as_array().map(Vec::as_slice).unwrap_or(&[]),
            plan["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]),
            plan["edges"].as_array().map(Vec::as_slice).unwrap_or(&[]),
        )?;
        self.repository
            .finish_game_task(task_id, SUCCEEDED, Some(plan.clone()), None)?;
        Ok(())
    }

    /// Store the raw preview and accepted per-record checkpoint so retries can resume after a bad item.
    fn persist_game_graph_preview(
        &self,
        task_id: &str,
        game_id: &str,
        preview: &str,
        checkpoint: &Value,
        stage: planner::GameGraphStage,
        validation_feedback: Option<&str>,
        validation_retry_count: i64,
    ) -> AppResult<()> {
        let received_chars = preview.chars().count();
        let node_count = checkpoint["nodes"].as_array().map_or(0, Vec::len);
        let edge_count = checkpoint["edges"].as_array().map_or(0, Vec::len);
        let progress = graph_stream_progress(stage, checkpoint, received_chars);
        self.repository.persist_game_graph_preview_state(
            task_id,
            &json!({
                "game_id":game_id,
                "graph_preview":preview,
                "graph_progress_checkpoint":checkpoint,
                "graph_generation_stage":stage.key(),
                "graph_validation_feedback":validation_feedback,
                "graph_validation_retry_count":validation_retry_count,
                "preview_received_chars":received_chars,
                "preview_node_count":node_count,
                "preview_edge_count":edge_count,
            }),
            progress,
            &format!(
                "正在生成{}（已保存 {node_count} 个节点，{edge_count} 条选择边）",
                stage.label()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::{graph_generation_start, graph_preview_write_due, graph_stream_progress};
    use crate::planner::GameGraphStage;

    #[test]
    fn resume_stage_preserves_valid_graph_records_instead_of_restarting() {
        let (progress, stage) = graph_generation_start(
            &json!({
                "assets": [{"id": "hero"}],
                "nodes": [{"id": "start"}, {"id": "ending"}],
                "edges": [{"id": "start-ending"}],
            }),
            GameGraphStage::Edges,
        );

        assert_eq!(progress, 68);
        assert!(stage.contains("玩家选择边"));
        assert!(stage.contains("2 个节点"));
    }

    #[test]
    fn node_stream_stays_in_the_node_band_instead_of_hitting_ninety() {
        let checkpoint = json!({"assets":[{"id":"hero"}],"nodes":[{"id":"start"}],"edges":[]});

        assert!(graph_stream_progress(GameGraphStage::Nodes, &checkpoint, 8_000) <= 66);
        assert_eq!(
            graph_stream_progress(GameGraphStage::Edges, &json!({"nodes":[{},{}]}), 80),
            69
        );
        assert_eq!(
            graph_stream_progress(GameGraphStage::Edges, &json!({}), 10_000),
            90
        );
    }

    #[test]
    fn graph_preview_waits_for_coarse_time_or_byte_threshold() {
        assert!(!graph_preview_write_due(
            767,
            0,
            Duration::from_millis(1_499)
        ));
        assert!(graph_preview_write_due(768, 0, Duration::ZERO));
        assert!(graph_preview_write_due(1, 0, Duration::from_millis(1_500)));
    }
}
