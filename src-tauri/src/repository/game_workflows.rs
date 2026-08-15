//! Durable interactive-game creation tasks and screenplay checkpoints owned by SQLite.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_text, new_id, now, row_to_json, CANCELLED, FAILED, GENERATING},
};

use super::Repository;

const STALE_GAME_TASK_STAGE: &str = "游戏生成工作线程已中断";
const STALE_GAME_TASK_ERROR: &str = "生成工作线程已停止续租，任务已停止。请重试。";

impl Repository {
    /// Queue rich prompt decomposition for one game node before its video task is submitted.
    pub fn enqueue_game_node_prompt(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        self.get_game_node(game_id, node_id)?;
        let task_id = self.db.with_connection(|connection| {
            let active = connection.query_row(
                "SELECT id FROM game_tasks WHERE game_id=?1 AND type='game_node_prompt' AND resource_id=?2 AND status=?3 ORDER BY created_at DESC LIMIT 1",
                params![game_id, node_id, GENERATING],
                |row| row.get::<_, String>(0),
            ).optional()?;
            if let Some(id) = active { return Ok(id); }
            let version: String = connection.query_row(
                "SELECT prompt_template_version FROM game_nodes WHERE id=?1 AND game_id=?2",
                params![node_id, game_id],
                |row| row.get(0),
            )?;
            let id = new_id();
            let timestamp = now();
            connection.execute(
                "INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_node_prompt',?3,?4,?5,0,'等待生成节点提示词',?6,?6)",
                params![id, game_id, node_id, GENERATING, json_text(&json!({"game_id":game_id,"node_id":node_id,"prompt_template_version":version})), timestamp],
            )?;
            Ok(id)
        })?;
        self.get_game_task(&task_id)
    }

    /// Preserve the legacy focused title update while routing it through the workbench's full graph snapshot save.
    pub fn update_game_name(
        &self,
        game_id: &str,
        values: serde_json::Map<String, Value>,
    ) -> AppResult<Value> {
        self.save_game_editor(game_id, values)
    }

    /// Persist the game editor's title plus its current normalized asset, node, and choice snapshots for packaging and recovery.
    pub fn save_game_editor(
        &self,
        game_id: &str,
        values: serde_json::Map<String, Value>,
    ) -> AppResult<Value> {
        let name = values
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !(1..=120).contains(&name.chars().count()) {
            return Err(AppError::BadRequest(
                "游戏名称长度需在 1 到 120 个字之间".to_owned(),
            ));
        }
        self.db.with_connection(|connection| {
            let assets = self.game_assets_for(connection, game_id)?;
            let nodes = self.game_nodes_for(connection, game_id)?;
            let edges = self.game_edges_for(connection, game_id)?;
            if connection.execute(
                "UPDATE interactive_games SET name=?1,assets_json=?2,nodes_json=?3,edges_json=?4,updated_at=?5 WHERE id=?6",
                params![name, json_text(&Value::Array(assets)), json_text(&Value::Array(nodes)), json_text(&Value::Array(edges)), now(), game_id],
            )? == 0
            {
                return Err(AppError::NotFound(format!(
                    "Interactive game not found: {game_id}"
                )));
            }
            Ok(())
        })?;
        self.get_game(game_id)
    }

    /// Save the visual direction and model choices edited through the interactive game's global-parameters dialog.
    pub fn update_game_parameters(
        &self,
        game_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let style = values
            .get("style")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !["真人风格", "2D动漫", "3D动漫"].contains(&style) {
            return Err(AppError::BadRequest("不支持的游戏视觉风格".to_owned()));
        }
        let model = |key: &str| {
            values
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::BadRequest(format!("{key} 不能为空")))
        };
        let language_model = model("language_model")?;
        let multimodal_model = model("multimodal_model")?;
        let video_model = model("video_model")?;
        let enable_web_search = values
            .get("enable_web_search")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    AppError::BadRequest("enable_web_search 必须是布尔值".to_owned())
                })
            })
            .transpose()?;
        self.db.with_connection(|connection| {
            let changed = if let Some(enable_web_search) = enable_web_search {
                connection.execute(
                    "UPDATE interactive_games SET style=?1,language_model=?2,multimodal_model=?3,video_model=?4,enable_web_search=?5,updated_at=?6 WHERE id=?7",
                    params![style, language_model, multimodal_model, video_model, enable_web_search as i64, now(), game_id],
                )?
            } else {
                connection.execute(
                    "UPDATE interactive_games SET style=?1,language_model=?2,multimodal_model=?3,video_model=?4,updated_at=?5 WHERE id=?6",
                    params![style, language_model, multimodal_model, video_model, now(), game_id],
                )?
            };
            if changed == 0 {
                return Err(AppError::NotFound(format!("Interactive game not found: {game_id}")));
            }
            Ok(())
        })?;
        self.get_game(game_id)
    }

    /// Save the original and expanded game screenplays from the editor dialog while leaving the existing graph untouched.
    pub fn update_game_screenplay(
        &self,
        game_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let script = values
            .get("script")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if script.chars().count() < 20 {
            return Err(AppError::BadRequest("剧本文本不少于 20 个字".to_owned()));
        }
        let expanded_script = values
            .get("expanded_script")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE interactive_games SET script=?1,expanded_script=?2,updated_at=?3 WHERE id=?4",
                params![script, expanded_script, now(), game_id],
            )? == 0 {
                return Err(AppError::NotFound(format!("Interactive game not found: {game_id}")));
            }
            Ok(())
        })?;
        self.get_game(game_id)
    }

    /// Queue an explicit game-screenplay expansion from the editor without disturbing the already saved graph.
    pub fn continue_game_screenplay(&self, game_id: &str) -> AppResult<Value> {
        let task_id = self.db.with_connection(|connection| {
            connection
                .query_row("SELECT id FROM interactive_games WHERE id=?1", [game_id], |row| row.get::<_, String>(0))
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))?;
            let active = connection
                .query_row(
                    "SELECT id FROM game_tasks WHERE game_id=?1 AND type='game_script_expansion' AND status=?2 ORDER BY created_at DESC LIMIT 1",
                    params![game_id, GENERATING],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(id) = active {
                return Ok(id);
            }
            let planning = connection
                .query_row(
                    "SELECT 1 FROM game_tasks WHERE game_id=?1 AND type='game_graph_decomposition' AND status=?2 LIMIT 1",
                    params![game_id, GENERATING],
                    |_| Ok(()),
                )
                .optional()?;
            if planning.is_some() {
                return Err(AppError::BadRequest(
                    "视频节点图谱正在拆分，请完成后再扩写剧本".to_owned(),
                ));
            }
            let id = new_id();
            let timestamp = now();
            let expanded_script = connection.query_row(
                "SELECT expanded_script FROM interactive_games WHERE id=?1",
                [game_id],
                |row| row.get::<_, String>(0),
            )?;
            connection.execute(
                "INSERT INTO game_tasks (id,game_id,type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_script_expansion',?3,?4,0,'等待扩写剧本',?5,?5)",
                params![id, game_id, GENERATING, json_text(&json!({"game_id":game_id,"expanded_script_preview":expanded_script})), timestamp],
            )?;
            connection.execute(
                "UPDATE interactive_games SET status=?1,updated_at=?2 WHERE id=?3",
                params![GENERATING, now(), game_id],
            )?;
            Ok(id)
        })?;
        self.get_game_task(&task_id)
    }

    /// Requeue the newest failed game screenplay or graph task with its durable preview intact, matching the short-drama retry behavior.
    pub fn retry_game_generation(&self, game_id: &str) -> AppResult<Value> {
        let task_id = self.db.with_connection(|connection| {
            connection
                .query_row("SELECT id FROM interactive_games WHERE id=?1", [game_id], |row| row.get::<_, String>(0))
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))?;
            let active = connection.query_row(
                "SELECT id FROM game_tasks WHERE game_id=?1 AND type IN ('game_script_expansion','game_graph_decomposition') AND status=?2 ORDER BY created_at DESC LIMIT 1",
                params![game_id, GENERATING],
                |row| row.get::<_, String>(0),
            ).optional()?;
            if let Some(id) = active { return Ok(id); }
            let (id, kind) = connection.query_row(
                "SELECT id,type FROM game_tasks WHERE game_id=?1 AND type IN ('game_script_expansion','game_graph_decomposition') AND status IN (?2,?3) ORDER BY created_at DESC LIMIT 1",
                params![game_id, FAILED, CANCELLED],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ).optional()?.ok_or_else(|| AppError::BadRequest("没有可重试的游戏生成任务".to_owned()))?;
            let stage = if kind == "game_script_expansion" { "等待重试剧本扩写" } else { "等待重试图谱生成" };
            connection.execute(
                "UPDATE game_tasks SET status=?1,error_message=NULL,completed_at=NULL,progress=0,stage=?2,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?3",
                params![GENERATING, stage, id],
            )?;
            connection.execute(
                "UPDATE interactive_games SET status=?1,updated_at=?2 WHERE id=?3",
                params![GENERATING, now(), game_id],
            )?;
            Ok(id)
        })?;
        self.get_game_task(&task_id)
    }

    /// Stop the active screenplay or graph task while preserving its streamed checkpoints.
    pub fn cancel_game_screenplay(&self, game_id: &str) -> AppResult<Value> {
        let task = self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM game_tasks WHERE game_id=?1 AND type IN ('game_script_expansion','game_graph_decomposition') ORDER BY created_at DESC LIMIT 1",
                    [game_id],
                    row_to_json,
                )
                .optional()?
                .map(mapping::game_task)
                .ok_or_else(|| AppError::BadRequest("未找到可停止的生成任务".to_owned()))
        })?;
        match task["status"].as_str() {
            Some(CANCELLED) => Ok(task),
            Some(GENERATING) => {
                let id = task["id"].as_str().unwrap_or_default();
                let stage = if task["type"].as_str() == Some("game_graph_decomposition") {
                    "图谱生成已停止，当前进度已保存"
                } else {
                    "剧本扩写已停止，当前进度已保存"
                };
                self.db.with_connection(|connection| {
                    connection.execute(
                        "UPDATE game_tasks SET status=?1,stage=?2,completed_at=?3,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?4 AND status=?5",
                        params![CANCELLED, stage, now(), id, GENERATING],
                    )?;
                    connection.execute(
                        "UPDATE interactive_games SET status=?1,updated_at=?2 WHERE id=?3",
                        params![CANCELLED, now(), game_id],
                    )?;
                    Ok(())
                })?;
                self.get_game_task(id)
            }
            _ => Err(AppError::BadRequest("生成任务已完成，无法停止".to_owned())),
        }
    }

    /// Persist a video provider's asynchronous job id and release this local worker until the next poll is due.
    pub(crate) fn schedule_game_provider_poll(
        &self,
        task_id: &str,
        provider_task_id: &str,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let next = (chrono::Utc::now() + chrono::Duration::seconds(3)).to_rfc3339();
            connection.execute("UPDATE game_tasks SET provider_task_id=?1,progress=?2,stage=?3,next_poll_at=?4,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?5 AND status=?6", params![provider_task_id,progress.clamp(0,99),stage,next,task_id,GENERATING])?;
            Ok(())
        })
    }

    /// Release a game generation task for a delayed fresh model call without discarding its visible preview.
    ///
    /// Graph decomposition uses this after its strict model-output validation rejects a response, so
    /// the desktop workbench can show retry progress while SQLite preserves the same durable task.
    pub(crate) fn reschedule_game_task(
        &self,
        task_id: &str,
        delay_seconds: i64,
        stage: &str,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let next = (chrono::Utc::now() + chrono::Duration::seconds(delay_seconds.max(0)))
                .to_rfc3339();
            connection.execute(
                "UPDATE game_tasks SET stage=?1,error_message=?2,next_poll_at=?3,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?4 AND status=?5",
                params![stage, error, next, task_id, GENERATING],
            )?;
            Ok(())
        })
    }

    /// Renew a claimed game task so slow language calls remain owned by the current worker.
    pub(crate) fn renew_game_task_lease(&self, task_id: &str, token: &str) -> AppResult<bool> {
        self.db.with_connection(|connection| {
            Ok(connection.execute(
                "UPDATE game_tasks SET poll_lease_until=?1 WHERE id=?2 AND status=?3 AND poll_lease_token=?4",
                params![(chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339(), task_id, GENERATING, token],
            )? > 0)
        })
    }

    /// Mark only a bootstrap game-flow failure on the aggregate game, leaving independent node-video failures local.
    pub(crate) fn set_game_status(&self, game_id: &str, status: &str) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE interactive_games SET status=?1,updated_at=?2 WHERE id=?3",
                params![status, now(), game_id],
            )? == 0
            {
                return Err(AppError::NotFound(format!(
                    "Interactive game not found: {game_id}"
                )));
            }
            Ok(())
        })
    }

    /// Convert an expired screenplay or graph lease into a retryable failure before the editor can report a dead worker as active.
    pub(crate) fn fail_expired_game_generation_tasks(&self) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let timestamp = now();
            connection.execute(
                "UPDATE game_tasks SET status=?1,stage=?2,error_message=?3,progress=100,completed_at=?4,poll_lease_token=NULL,poll_lease_until=NULL WHERE status=?5 AND type IN ('game_script_expansion','game_graph_decomposition') AND poll_lease_until IS NOT NULL AND poll_lease_until<?4",
                params![FAILED, STALE_GAME_TASK_STAGE, STALE_GAME_TASK_ERROR, timestamp, GENERATING],
            )?;
            connection.execute(
                "UPDATE interactive_games SET status=?1,updated_at=?2 WHERE id IN (SELECT game_id FROM game_tasks WHERE status=?1 AND stage=?3 AND completed_at=?2)",
                params![FAILED, timestamp, STALE_GAME_TASK_STAGE],
            )?;
            Ok(())
        })
    }

    /// Claim a queued game task belonging to a specific provider family without stealing work from another queue.
    pub(crate) fn claim_game_task_types(&self, types: &[&str]) -> AppResult<Option<Value>> {
        if types.is_empty() {
            return Ok(None);
        }
        self.db.with_connection(|connection| {
            let placeholders = std::iter::repeat("?").take(types.len()).collect::<Vec<_>>().join(",");
            let query = format!("SELECT id FROM game_tasks WHERE status=? AND type IN ({placeholders}) AND (next_poll_at IS NULL OR next_poll_at<?) AND (poll_lease_until IS NULL OR poll_lease_until<?) ORDER BY created_at LIMIT 1");
            let mut values = vec![rusqlite::types::Value::Text(GENERATING.to_owned())];
            values.extend(types.iter().map(|value| rusqlite::types::Value::Text((*value).to_owned())));
            values.push(rusqlite::types::Value::Text(now()));
            values.push(rusqlite::types::Value::Text(now()));
            let id = connection.query_row(&query, rusqlite::params_from_iter(values), |row| row.get::<_, String>(0)).optional()?;
            let Some(id) = id else { return Ok(None); };
            connection.execute(
                "UPDATE game_tasks SET poll_lease_token=?1,poll_lease_until=?2,poll_attempts=poll_attempts+1 WHERE id=?3",
                params![new_id(), (chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339(), id],
            )?;
            self.get_game_task(&id).map(Some)
        })
    }
}
