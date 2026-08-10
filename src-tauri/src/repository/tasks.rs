//! Restart-safe short-drama task persistence and polling projections.

use rusqlite::{params, OptionalExtension, ToSql};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_text, new_id, now, row_to_json, CANCELLED, GENERATING},
};

use super::Repository;

impl Repository {
    /// Create a generating task or return its active predecessor so repeated UI clicks are idempotent.
    pub fn create_active_drama_task(
        &self,
        drama_id: &str,
        kind: &str,
        resource_id: Option<&str>,
        snapshot: Value,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            self.assert_drama(drama_id)?;
            let existing = connection.query_row(
                "SELECT * FROM generation_tasks WHERE drama_id=?1 AND type=?2 AND resource_id IS ?3 AND status='生成中' ORDER BY created_at DESC LIMIT 1",
                params![drama_id, kind, resource_id], row_to_json,
            ).optional()?;
            if let Some(row) = existing { return Ok(mapping::drama_task(row)); }
            let id = new_id();
            let timestamp = now();
            let job_id = format!("{drama_id}:{}", resource_id.unwrap_or(kind));
            let number: i64 = connection.query_row("SELECT COUNT(*)+1 FROM generation_tasks WHERE job_id=?1", [job_id.as_str()], |row| row.get(0))?;
            connection.execute(
                "INSERT INTO generation_tasks (id,drama_id,type,job_id,task_no,trigger_type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,'等待队列',?10,?10)",
                params![id,drama_id,kind,job_id,number,kind.to_uppercase(),resource_id,GENERATING,json_text(&snapshot),timestamp],
            )?;
            self.get_drama_task(&id)
        })
    }

    /// Create a parallel task even if the same resource has another active video generation.
    pub fn create_parallel_drama_task(
        &self,
        drama_id: &str,
        kind: &str,
        resource_id: Option<&str>,
        snapshot: Value,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            self.assert_drama(drama_id)?;
            let id = new_id(); let timestamp = now(); let job_id = format!("{drama_id}:{}", resource_id.unwrap_or(kind));
            let number: i64 = connection.query_row("SELECT COUNT(*)+1 FROM generation_tasks WHERE job_id=?1", [job_id.as_str()], |row| row.get(0))?;
            connection.execute("INSERT INTO generation_tasks (id,drama_id,type,job_id,task_no,trigger_type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,'等待队列',?10,?10)", params![id,drama_id,kind,job_id,number,kind.to_uppercase(),resource_id,GENERATING,json_text(&snapshot),timestamp])?;
            self.get_drama_task(&id)
        })
    }

    /// Read one drama task with public `project_id`, JSON snapshots, and provider metadata restored.
    pub fn get_drama_task(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM generation_tasks WHERE id=?1",
                    [id],
                    row_to_json,
                )
                .optional()?
                .map(mapping::drama_task)
                .ok_or_else(|| AppError::NotFound(format!("Task not found: {id}")))
        })
    }

    /// Load the newest task that owns screenplay text, including its private checkpoint snapshot for service recovery.
    pub(crate) fn latest_expansion_task(&self, drama_id: &str) -> AppResult<Option<Value>> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM generation_tasks WHERE drama_id=?1 AND type IN ('script_decomposition','script_expansion') ORDER BY created_at DESC LIMIT 1",
                    [drama_id],
                    row_to_json,
                )
                .optional()
                .map(|value| value.map(mapping::drama_task))
                .map_err(Into::into)
        })
    }

    /// Reuse a saved story bible when an append-only expansion starts after bootstrap completion.
    pub(crate) fn latest_expansion_story_bible(&self, drama_id: &str) -> AppResult<String> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT * FROM generation_tasks WHERE drama_id=?1 AND type IN ('script_decomposition','script_expansion') ORDER BY created_at DESC",
            )?;
            let rows = statement
                .query_map([drama_id], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows
                .into_iter()
                .map(mapping::drama_task)
                .find_map(|task| task["input_snapshot"]["story_bible"].as_str().map(str::to_owned))
                .unwrap_or_default())
        })
    }

    /// Cancel exactly the newest screenplay owner without stopping unrelated durable tasks.
    pub(crate) fn cancel_drama_task(&self, id: &str, stage: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE generation_tasks SET status=?1,stage=?2,completed_at=?3,finished_at=?3,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?4 AND status=?5",
                params![CANCELLED, stage, now(), id, GENERATING],
            )?;
            self.get_drama_task(id)
        })
    }

    /// Mark a task stage/progress without releasing the worker that currently owns its lease.
    pub fn update_drama_task_progress(
        &self,
        id: &str,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE generation_tasks SET progress=?1,stage=?2 WHERE id=?3",
                params![progress.clamp(0, 100), stage, id],
            )?;
            Ok(())
        })
    }

    /// Persist terminal state and error/output fields so a restart never loses a provider result.
    pub fn finish_drama_task(
        &self,
        id: &str,
        status: &str,
        result: Option<Value>,
        error: Option<&str>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let timestamp = now();
            if connection.execute("UPDATE generation_tasks SET status=?1,output_result_json=?2,result_json=?2,error_message=?3,progress=?4,stage=?5,completed_at=?6,finished_at=?6,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?7 AND status=?8", params![status,result.as_ref().map(json_text),error, if status == GENERATING { 0 } else {100},if status == GENERATING {"处理中"} else {"已完成"},timestamp,id,GENERATING])? == 0 {
                return self.get_drama_task(id);
            }
            self.get_drama_task(id)
        })
    }

    /// Update an existing task snapshot after an expansion checkpoint or a batch coordinator advances.
    pub fn update_drama_task_snapshot(&self, id: &str, snapshot: Value) -> AppResult<()> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE generation_tasks SET input_snapshot_json=?1 WHERE id=?2",
                params![json_text(&snapshot), id],
            )?;
            Ok(())
        })
    }

    /// Claim one currently generating task with a short SQLite lease for a local worker slot.
    pub fn claim_drama_task(&self) -> AppResult<Option<Value>> {
        self.claim_drama_task_types(&[])
    }

    /// Lease one runnable task from an independent model queue without allowing another queue to consume it.
    pub(crate) fn claim_drama_task_types(&self, task_types: &[&str]) -> AppResult<Option<Value>> {
        self.db.with_connection(|connection| {
            let timestamp = now();
            let mut query = "SELECT id FROM generation_tasks WHERE status='生成中' AND (next_poll_at IS NULL OR next_poll_at <= ?1) AND (poll_lease_until IS NULL OR poll_lease_until < ?1)".to_owned();
            if !task_types.is_empty() {
                let placeholders = (2..task_types.len() + 2)
                    .map(|index| format!("?{index}"))
                    .collect::<Vec<_>>()
                    .join(",");
                query.push_str(&format!(" AND type IN ({placeholders})"));
            }
            query.push_str(" ORDER BY CASE type WHEN 'script_decomposition' THEN 0 ELSE 1 END,next_poll_at,created_at,id LIMIT 1");
            let mut values: Vec<&dyn ToSql> = vec![&timestamp];
            values.extend(task_types.iter().map(|kind| kind as &dyn ToSql));
            let candidate = connection
                .query_row(&query, values.as_slice(), |row| row.get::<_, String>(0))
                .optional()?;
            let Some(id) = candidate else { return Ok(None); };
            let lease = drama_task_lease_until();
            let claimed = connection.execute("UPDATE generation_tasks SET poll_lease_token=?1,poll_lease_until=?2,poll_attempts=poll_attempts+1,stage='正在执行' WHERE id=?3 AND status=?4 AND (poll_lease_until IS NULL OR poll_lease_until < ?5)", params![new_id(),lease,id,GENERATING,timestamp])?;
            if claimed != 1 { return Ok(None); }
            self.get_drama_task(&id).map(Some)
        })
    }

    /// Extend one worker's exclusive claim while a synchronous provider call is still in flight.
    pub(crate) fn renew_drama_task_lease(&self, id: &str, token: &str) -> AppResult<bool> {
        self.db.with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE generation_tasks SET poll_lease_until=?1 WHERE id=?2 AND status=?3 AND poll_lease_token=?4",
                params![drama_task_lease_until(), id, GENERATING, token],
            )?;
            Ok(changed == 1)
        })
    }

    /// Persist an asynchronous provider id and defer the next poll without releasing a cancelled task.
    pub fn schedule_drama_provider_poll(
        &self,
        id: &str,
        provider_task_id: &str,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let next = (chrono::Utc::now() + chrono::Duration::seconds(3)).to_rfc3339();
            connection.execute("UPDATE generation_tasks SET provider_task_id=?1,progress=?2,stage=?3,next_poll_at=?4,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?5 AND status=?6", params![provider_task_id,progress.clamp(0,99),stage,next,id,GENERATING])?;
            Ok(())
        })
    }

    /// Release a local coordinator for a delayed retry while preserving its durable snapshot and last diagnostic.
    pub(crate) fn reschedule_drama_task(
        &self,
        id: &str,
        delay_seconds: i64,
        stage: &str,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let next = (chrono::Utc::now() + chrono::Duration::seconds(delay_seconds.max(0))).to_rfc3339();
            connection.execute("UPDATE generation_tasks SET stage=?1,error_message=?2,next_poll_at=?3,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?4 AND status=?5", params![stage,error,next,id,GENERATING])?;
            Ok(())
        })
    }

    /// Return current generating plus newly terminal tasks since the last polling cursor.
    pub fn poll_drama_tasks(
        &self,
        drama_id: &str,
        status: Option<&str>,
        since: Option<&str>,
    ) -> AppResult<Value> {
        self.assert_drama(drama_id)?;
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare("SELECT * FROM generation_tasks WHERE drama_id=?1 AND (?2 IS NULL OR status=?2 OR completed_at>?3) ORDER BY created_at,id")?;
            let rows = statement.query_map(params![drama_id,status,since], row_to_json)?.collect::<Result<Vec<_>, _>>()?;
            let tasks = rows.into_iter().map(|row| {
                let task = mapping::drama_task(row); let fields = ["id","type","status","project_id","resource_id","progress","stage","provider_task_id","next_poll_at","created_at","started_at","completed_at","finished_at","error_message"];
                let mut item = serde_json::Map::new(); for field in fields { item.insert(field.to_owned(), task.get(field).cloned().unwrap_or(Value::Null)); }
                if matches!(
                    task["type"].as_str(),
                    Some("script_decomposition") | Some("script_expansion")
                ) {
                    let mut preview = Map::new();
                    for key in ["expanded_script_preview", "story_bible_preview"] {
                        if let Some(value) = task["input_snapshot"][key].as_str() {
                            preview.insert(key.to_owned(), json!(value));
                        }
                    }
                    if !preview.is_empty() { item.insert("input_snapshot".to_owned(), Value::Object(preview)); }
                } else if task["type"].as_str() == Some("serial_shot_video_batch") {
                    item.insert(
                        "input_snapshot".to_owned(),
                        mapping::serial_video_batch_snapshot(&task["input_snapshot"]),
                    );
                }
                Value::Object(item)
            }).collect::<Vec<_>>();
            Ok(json!({"project_id":drama_id,"server_time":now(),"tasks":tasks}))
        })
    }

    /// Cancel active tasks for one project/resource boundary before optional remote provider cancellation runs.
    pub fn cancel_drama_tasks(
        &self,
        drama_id: &str,
        kind: Option<&str>,
        resource: Option<&str>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let changed = connection.execute("UPDATE generation_tasks SET status=?1,stage='任务已取消',completed_at=?2,finished_at=?2,poll_lease_until=NULL,poll_lease_token=NULL WHERE drama_id=?3 AND status=?4 AND (?5 IS NULL OR type=?5) AND (?6 IS NULL OR resource_id=?6)", params![CANCELLED,now(),drama_id,GENERATING,kind,resource])?;
            Ok(json!({"status":CANCELLED,"cancelled_count":changed}))
        })
    }

    /// Requeue a failed bootstrap task using its persisted input and expansion checkpoints.
    pub fn retry_drama_task(&self, drama_id: &str, kind: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let id = connection.query_row("SELECT id FROM generation_tasks WHERE drama_id=?1 AND type=?2 AND status IN ('生成失败','已取消') ORDER BY created_at DESC LIMIT 1", params![drama_id,kind], |row| row.get::<_,String>(0)).optional()?
                .ok_or_else(|| AppError::Conflict("没有可重试的任务".to_owned()))?;
            connection.execute("UPDATE generation_tasks SET status=?1,error_message=NULL,completed_at=NULL,finished_at=NULL,progress=0,stage='等待重试',poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?2", params![GENERATING,id])?;
            connection.execute(
                "UPDATE short_dramas SET status=?1,updated_at=?2 WHERE id=?3",
                params![GENERATING, now(), drama_id],
            )?;
            self.get_drama_task(&id)
        })
    }

    /// Create a new bootstrap task after cancellation so an old in-flight worker can never complete the restarted run.
    pub fn restart_drama_task(&self, drama_id: &str, kind: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            self.assert_drama(drama_id)?;
            let job_id = connection.query_row("SELECT job_id FROM generation_tasks WHERE drama_id=?1 AND type=?2 AND status IN ('生成失败','已取消') ORDER BY created_at DESC LIMIT 1", params![drama_id, kind], |row| row.get::<_, String>(0)).optional()?.ok_or_else(|| AppError::Conflict("没有可重新启动的任务".to_owned()))?;
            let active: i64 = connection.query_row("SELECT COUNT(*) FROM generation_tasks WHERE drama_id=?1 AND type=?2 AND status=?3", params![drama_id, kind, GENERATING], |row| row.get(0))?;
            if active > 0 { return Err(AppError::Conflict("任务正在生成中".to_owned())); }
            let number: i64 = connection.query_row("SELECT COUNT(*)+1 FROM generation_tasks WHERE job_id=?1", [job_id.as_str()], |row| row.get(0))?;
            let id = new_id(); let timestamp = now();
            connection.execute("INSERT INTO generation_tasks (id,drama_id,type,job_id,task_no,trigger_type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,?3,?4,?5,'DRAMA_BOOTSTRAP',?6,?7,0,'等待重新生成',?8,?8)", params![id,drama_id,kind,job_id,number,GENERATING,json_text(&json!({"drama_id":drama_id})),timestamp])?;
            connection.execute("UPDATE short_dramas SET status=?1,expanded_script='',updated_at=?2 WHERE id=?3", params![GENERATING,timestamp,drama_id])?;
            self.get_drama_task(&id)
        })
    }

    /// Replace a completed or failed short-drama run with a fresh bootstrap task, clearing its derived editor graph.
    pub fn regenerate_drama(&self, drama_id: &str, script: Option<&str>) -> AppResult<Value> {
        let replacement = script.map(|value| value.trim().to_owned());
        if replacement
            .as_deref()
            .is_some_and(|value| value.chars().count() < 10)
        {
            return Err(AppError::BadRequest("剧本文本不少于 10 个字".to_owned()));
        }
        let task_id = self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let saved_script: String = transaction
                .query_row("SELECT script FROM short_dramas WHERE id=?1", [drama_id], |row| {
                    row.get(0)
                })
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Project not found: {drama_id}")))?;
            let source = replacement.as_deref().unwrap_or(&saved_script);
            if source.chars().count() < 10 {
                return Err(AppError::BadRequest("剧本文本不少于 10 个字".to_owned()));
            }
            let timestamp = now();
            transaction.execute(
                "UPDATE generation_tasks SET status=?1,stage='已因从头重新生成取消',completed_at=?2,finished_at=?2,poll_lease_until=NULL,poll_lease_token=NULL WHERE drama_id=?3 AND status=?4",
                params![CANCELLED, timestamp, drama_id, GENERATING],
            )?;
            transaction.execute("DELETE FROM drama_shot_versions WHERE drama_id=?1", [drama_id])?;
            transaction.execute("DELETE FROM drama_assets WHERE drama_id=?1", [drama_id])?;
            transaction.execute("DELETE FROM drama_shots WHERE drama_id=?1", [drama_id])?;
            let number: i64 = transaction.query_row(
                "SELECT COUNT(*)+1 FROM generation_tasks WHERE job_id=?1",
                [drama_id],
                |row| row.get(0),
            )?;
            let id = new_id();
            transaction.execute(
                "INSERT INTO generation_tasks (id,drama_id,type,job_id,task_no,trigger_type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'script_decomposition',?2,?3,'DRAMA_BOOTSTRAP',?4,?5,0,'等待重新生成',?6,?6)",
                params![id, drama_id, number, GENERATING, json_text(&json!({"drama_id":drama_id})), timestamp],
            )?;
            transaction.execute(
                "UPDATE short_dramas SET script=?1,expanded_script='',assets_json='[]',shots_json='[]',historical_videos_json='[]',status=?2,updated_at=?3 WHERE id=?4",
                params![source, GENERATING, timestamp, drama_id],
            )?;
            transaction.commit()?;
            Ok(id)
        })?;
        self.get_drama_task(&task_id)
    }

    /// Find active task rows used to synchronize shot-level states after parallel video cancellation.
    pub fn active_drama_tasks(
        &self,
        drama_id: &str,
        kind: &str,
        resource: Option<&str>,
    ) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare("SELECT * FROM generation_tasks WHERE drama_id=?1 AND type=?2 AND status=?3 AND (?4 IS NULL OR resource_id=?4) ORDER BY created_at")?;
            let rows = statement
                .query_map(params![drama_id,kind,GENERATING,resource], row_to_json)?
                .collect::<Result<Vec<_>,_>>()?
                .into_iter()
                .map(mapping::drama_task)
                .collect();
            Ok(rows)
        })
    }

    /// Find the newest active task whose persisted input owns a logical resource outside `resource_id`.
    pub(crate) fn active_drama_task_by_snapshot(
        &self,
        drama_id: &str,
        kind: &str,
        key: &str,
        value: &str,
    ) -> AppResult<Option<Value>> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare("SELECT * FROM generation_tasks WHERE drama_id=?1 AND type=?2 AND status=?3 ORDER BY created_at DESC")?;
            let rows = statement
                .query_map(params![drama_id, kind, GENERATING], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().map(mapping::drama_task).find(|task| {
                task["input_snapshot"][key].as_str() == Some(value)
            }))
        })
    }
}

fn drama_task_lease_until() -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339()
}
