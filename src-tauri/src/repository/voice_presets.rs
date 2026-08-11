//! Creator-managed voice catalog and durable source-audio preview persistence.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    system_voice_samples,
    value::{new_id, now, row_to_json, string, FAILED, GENERATING, SUCCEEDED},
};

use super::Repository;

const MAX_VOICE_NAME_CHARS: usize = 80;
const MAX_VOICE_DESCRIPTION_CHARS: usize = 500;
const MAX_VOICE_GENDER_CHARS: usize = 20;

impl Repository {
    /// Return the enabled catalog for character selectors, settings playback, and video prompt assembly.
    ///
    /// This boundary owns the catalog query and enriches each row with its most recent durable audio task.
    pub fn voices(&self) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT v.id,v.name,v.gender,v.prompt,v.audio_url,v.sort_order,(SELECT status FROM voice_generation_tasks t WHERE t.voice_id=v.id ORDER BY t.created_at DESC LIMIT 1) AS audio_generation_status FROM voice_presets v WHERE v.enabled=1 ORDER BY v.sort_order,v.id",
            )?;
            let rows = statement
                .query_map([], row_to_json)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into);
            rows
        })
    }

    /// Retain the previous direct-create API for existing integrations while new settings UI uses preview confirmation.
    pub fn create_voice_preset(&self, values: Map<String, Value>) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let source = voice_task_source(connection, &values)?;
            create_voice(connection, &source, "")
        })
    }

    /// Persist a generated custom preview only after the creator explicitly confirms the playable sample.
    pub fn confirm_voice_audio_preview(&self, task_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let task = voice_task(connection, task_id)?;
            if task["voice_id"].is_string() {
                return Err(AppError::BadRequest("系统音色无需追加确认".to_owned()));
            }
            if task["status"].as_str() != Some(SUCCEEDED) || task["audio_url"].as_str().is_none() {
                return Err(AppError::BadRequest(
                    "请等待音色试听生成成功后再追加".to_owned(),
                ));
            }
            create_voice(
                connection,
                &task,
                task["audio_url"].as_str().unwrap_or_default(),
            )
        })
    }

    /// Create a restart-safe configured-audio-model task for a system voice or unconfirmed custom voice.
    pub fn create_voice_audio_task(&self, values: Map<String, Value>) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let source = voice_task_source(connection, &values)?;
            let timestamp = now();
            let id = new_id();
            connection.execute(
                "INSERT INTO voice_generation_tasks (id,voice_id,name,gender,prompt,sample_text,status,progress,stage,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,0,'等待生成音色',?8)",
                params![id, source["voice_id"].as_str(), source["name"].as_str(), source["gender"].as_str(), source["prompt"].as_str(), source["sample_text"].as_str(), GENERATING, timestamp],
            )?;
            voice_task(connection, &id)
        })
    }

    /// Recreate a preview from frozen metadata, preserving the candidate-confirmation workflow on every retry.
    pub fn regenerate_voice_audio_task(&self, task_id: &str) -> AppResult<Value> {
        let task = self.get_voice_audio_task(task_id)?;
        self.create_voice_audio_task(Map::from_iter([
            ("voice_id".to_owned(), task["voice_id"].clone()),
            ("name".to_owned(), task["name"].clone()),
            ("gender".to_owned(), task["gender"].clone()),
            ("prompt".to_owned(), task["prompt"].clone()),
        ]))
    }

    /// Read one durable audio preview for the settings polling loop and confirmation action.
    pub fn get_voice_audio_task(&self, task_id: &str) -> AppResult<Value> {
        self.db
            .with_connection(|connection| voice_task(connection, task_id))
    }

    /// Lease the oldest unclaimed audio task so configured audio calls never run twice in parallel.
    pub(crate) fn claim_voice_audio_task(&self) -> AppResult<Option<Value>> {
        self.db.with_connection(|connection| {
            let candidate = connection.query_row(
                "SELECT id FROM voice_generation_tasks WHERE status=?1 AND stage='等待生成音色' ORDER BY created_at,id LIMIT 1",
                [GENERATING],
                |row| row.get::<_, String>(0),
            ).optional()?;
            let Some(id) = candidate else { return Ok(None) };
            if connection.execute(
                "UPDATE voice_generation_tasks SET stage='正在生成音色',progress=8 WHERE id=?1 AND status=?2 AND stage='等待生成音色'",
                params![id, GENERATING],
            )? != 1 { return Ok(None) }
            voice_task(connection, &id).map(Some)
        })
    }

    /// Store a playable app-owned audio URL and atomically attach it to the regenerated system voice.
    pub(crate) fn finish_voice_audio_task(
        &self,
        task_id: &str,
        audio_url: &str,
    ) -> AppResult<Option<String>> {
        self.db.with_connection(|connection| {
            let task = voice_task(connection, task_id)?;
            if task["status"].as_str() != Some(GENERATING) { return Ok(None) }
            let previous = if let Some(voice_id) = task["voice_id"].as_str() {
                connection.query_row(
                    "SELECT audio_url FROM voice_presets WHERE id=?1", [voice_id], |row| row.get::<_, Option<String>>(0),
                ).optional()?.flatten()
            } else { None };
            connection.execute(
                "UPDATE voice_generation_tasks SET status=?1,progress=100,stage='已完成',audio_url=?2,error_message=NULL,completed_at=?3 WHERE id=?4",
                params![SUCCEEDED, audio_url, now(), task_id],
            )?;
            if let Some(voice_id) = task["voice_id"].as_str() {
                connection.execute("UPDATE voice_presets SET audio_url=?1,updated_at=?2 WHERE id=?3", params![audio_url,now(),voice_id])?;
            }
            Ok(previous.filter(|value| value != audio_url))
        })
    }

    /// Mark an audio task terminal after the provider failed, retaining the frozen preview request for a retry.
    pub(crate) fn fail_voice_audio_task(&self, task_id: &str, error: &str) -> AppResult<()> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE voice_generation_tasks SET status=?1,progress=100,stage='生成失败',error_message=?2,completed_at=?3 WHERE id=?4 AND status=?5",
                params![FAILED,error,now(),task_id,GENERATING],
            )?;
            Ok(())
        })
    }
}

fn voice_task_source(
    connection: &rusqlite::Connection,
    values: &Map<String, Value>,
) -> AppResult<Value> {
    let voice_id = values
        .get("voice_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let source = if let Some(voice_id) = voice_id {
        if voice_id == "none" {
            return Err(AppError::BadRequest("“不设置”不能生成音色试听".to_owned()));
        }
        if system_voice_samples::is_system_voice(voice_id) {
            return Err(AppError::BadRequest(
                "系统音色已内置试听音源，可直接播放".to_owned(),
            ));
        }
        connection
            .query_row(
                "SELECT id,name,gender,prompt FROM voice_presets WHERE id=?1 AND enabled=1",
                [voice_id],
                row_to_json,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("Voice preset not found: {voice_id}")))?
    } else {
        let name = string(values, "name", "");
        let gender = string(values, "gender", "");
        let prompt = string(values, "prompt", "");
        validate_voice_preset(&name, &gender, &prompt)?;
        json!({"name":name,"gender":gender,"prompt":prompt})
    };
    let name = source["name"].as_str().unwrap_or_default();
    let gender = source["gender"].as_str().unwrap_or("未标注");
    let prompt = source["prompt"].as_str().unwrap_or_default();
    Ok(json!({
        "voice_id":source["id"],
        "name":name,
        "gender":gender,
        "prompt":prompt,
        "sample_text":"你好，很高兴在这个故事里与你相遇。",
    }))
}

fn voice_task(connection: &rusqlite::Connection, task_id: &str) -> AppResult<Value> {
    connection
        .query_row(
            "SELECT * FROM voice_generation_tasks WHERE id=?1",
            [task_id],
            row_to_json,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Voice audio task not found: {task_id}")))
}

fn create_voice(
    connection: &rusqlite::Connection,
    task: &Value,
    audio_url: &str,
) -> AppResult<Value> {
    let name = task["name"].as_str().unwrap_or_default();
    let exists = connection
        .query_row("SELECT 1 FROM voice_presets WHERE name=?1", [name], |_| {
            Ok(())
        })
        .optional()?;
    if exists.is_some() {
        return Err(AppError::Conflict(
            "已存在同名音色，请修改名称后再保存".to_owned(),
        ));
    }
    let id = format!("custom-{}", new_id());
    let sort_order = connection.query_row(
        "SELECT COALESCE(MAX(sort_order),-1)+1 FROM voice_presets",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let timestamp = now();
    connection.execute(
        "INSERT INTO voice_presets (id,name,gender,prompt,audio_url,sort_order,enabled,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,1,?7,?7)",
        params![id,name,task["gender"].as_str(),task["prompt"].as_str(),audio_url,sort_order,timestamp],
    )?;
    Ok(
        json!({"id":id,"name":name,"gender":task["gender"],"prompt":task["prompt"],"audio_url":audio_url,"sort_order":sort_order}),
    )
}

fn validate_voice_preset(name: &str, gender: &str, prompt: &str) -> AppResult<()> {
    if name.is_empty() || prompt.is_empty() {
        return Err(AppError::BadRequest(
            "音色名称和音色描述不能为空".to_owned(),
        ));
    }
    if name.chars().count() > MAX_VOICE_NAME_CHARS {
        return Err(AppError::BadRequest(
            "音色名称不能超过 80 个字符".to_owned(),
        ));
    }
    if gender.chars().count() > MAX_VOICE_GENDER_CHARS {
        return Err(AppError::BadRequest(
            "适用性别不能超过 20 个字符".to_owned(),
        ));
    }
    if prompt.chars().count() > MAX_VOICE_DESCRIPTION_CHARS {
        return Err(AppError::BadRequest(
            "音色描述不能超过 500 个字符".to_owned(),
        ));
    }
    Ok(())
}
