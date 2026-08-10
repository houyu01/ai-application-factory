//! Creator-managed voice-preset catalog persistence.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::{new_id, now, row_to_json, string},
};

use super::Repository;

const MAX_VOICE_NAME_CHARS: usize = 80;
const MAX_VOICE_DESCRIPTION_CHARS: usize = 500;
const MAX_VOICE_GENDER_CHARS: usize = 20;

impl Repository {
    /// Return the enabled catalog used by the settings view, character editor, and video-prompt builder.
    ///
    /// This repository boundary owns the SQLite query so every caller receives the same ordered preset list.
    pub fn voices(&self) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,name,gender,prompt,sort_order FROM voice_presets WHERE enabled=1 ORDER BY sort_order,id",
            )?;
            let rows = statement
                .query_map([], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Add a creator-defined voice to the settings catalog before it can be assigned to a character.
    ///
    /// The settings form supplies the display name, optional gender, and prompt description; this repository
    /// owns validation, stable ID creation, ordering, and the SQLite transaction used by downstream selectors.
    pub fn create_voice_preset(&self, values: Map<String, Value>) -> AppResult<Value> {
        let name = string(&values, "name", "");
        let gender = string(&values, "gender", "");
        let prompt = string(&values, "prompt", "");
        validate_voice_preset(&name, &gender, &prompt)?;

        let id = format!("custom-{}", new_id());
        let timestamp = now();
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let exists = transaction
                .query_row(
                    "SELECT 1 FROM voice_presets WHERE name=?1",
                    params![name],
                    |_| Ok(()),
                )
                .optional()?;
            if exists.is_some() {
                return Err(AppError::Conflict("已存在同名音色，请修改名称后再保存".to_owned()));
            }
            let sort_order = transaction.query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM voice_presets",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            transaction.execute(
                "INSERT INTO voice_presets (id,name,gender,prompt,sort_order,enabled,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,1,?6,?6)",
                params![id, name, gender, prompt, sort_order, timestamp],
            )?;
            transaction.commit()?;
            Ok(json!({"id":id,"name":name,"gender":gender,"prompt":prompt,"sort_order":sort_order}))
        })
    }
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
